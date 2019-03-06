//! Implements the functionality to interact with data located in different storages.
use crate::{fmt, Debug, Display, Failure, Formatter, Outcome};
use jsonrpc_core;
use lsp_types::notification::Notification;
use lsp_types::request::Request;
use lsp_types::{lsp_notification, lsp_request};
use serde;
use serde_json;
use std::cell::RefCell;
use std::error;
use std::fs;
use std::io::{self, BufRead, Read, Write};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};
use std::rc::Rc;
use std::sync::mpsc::{channel, Receiver, RecvError};
use std::thread::{self, JoinHandle};

/// Signifies a file.
#[derive(Clone, Debug)]
pub struct File {
    /// The path of the file.
    path: String,
    /// The [`Explorer`] used for interacting with the file.
    explorer: Rc<RefCell<dyn Explorer>>,
}

impl File {
    /// Creates a new `File`.
    pub fn new(explorer: Rc<RefCell<dyn Explorer>>, path: String) -> Self {
        explorer
            .borrow_mut()
            .start()
            .expect("Attempting to start the Explorer");
        Self { path, explorer }
    }

    /// Returns the data read from the `File`.
    pub(crate) fn read(&self) -> Outcome<String> {
        self.explorer.borrow_mut().read(&self.path)
    }

    /// Writes the given data to the `File`.
    pub(crate) fn write(&self, data: &str) -> Outcome<()> {
        self.explorer.borrow_mut().write(&self.path, data)
    }
}

impl Default for File {
    fn default() -> Self {
        Self {
            path: String::new(),
            explorer: Rc::new(RefCell::new(NullExplorer::default())),
        }
    }
}

/// Interacts and processes file data.
pub trait Explorer: Debug {
    /// Initializes all functionality needed by the Explorer.
    fn start(&mut self) -> Outcome<()>;
    /// Returns the data from the file at a given path.
    fn read(&self, path: &str) -> Outcome<String>;
    /// Writes data to a file at the given path.
    fn write(&self, path: &str, data: &str) -> Outcome<()>;
}

/// The interface with the language server.
#[derive(Debug)]
struct LanguageClient {
    /// The thread running the language server.
    server: Child,
    /// Writes the input to the language server.
    writer: ChildStdin,
    /// The id for the next request to be sent by the `LanguageClient`.
    request_id: u64,
    /// How the language server expects to text documents to be synchronized.
    text_document_sync: lsp_types::TextDocumentSyncKind,
    /// If the language server is a document symbol provider.
    is_document_symbol_provider: bool,
    /// Receives the [`InitializeResult`] message.
    result_rx: Receiver<lsp_types::InitializeResult>,
    /// The handle of the thread that processes the reception of messages from the language server.
    receiver_thread: JoinHandle<()>,
}

/// Returns the length of the content that is next to be read.
fn get_content_length(reader: &mut std::io::BufReader<ChildStdout>) -> Option<usize> {
    let mut line = String::new();
    let mut blank_line = String::new();

    if reader.read_line(&mut line).is_ok() {
        let mut split = line.trim().split(": ");

        if split.next() == Some("Content-Length") && reader.read_line(&mut blank_line).is_ok() {
            return split
                .next()
                .and_then(|value_string| value_string.parse().ok());
        }
    }

    None
}

impl LanguageClient {
    /// Creates a new `LanguageClient`.
    fn new(command: &str) -> Self {
        let mut server = Command::new(command)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("Attempting to spawn language server process.");
        let mut reader = std::io::BufReader::new(
            server
                .stdout
                .take()
                .expect("Attempting to take stdout of language server process"),
        );
        let writer = server
            .stdin
            .take()
            .expect("Attempting to take stdin of language server process");
        let (result_tx, result_rx) = channel::<lsp_types::InitializeResult>();
        let receiver_thread = thread::spawn(move || loop {
            if let Some(content_length) = get_content_length(&mut reader) {
                let mut content = vec![0; content_length];

                if reader.read_exact(&mut content).is_ok() {
                    if let Ok(json_string) = String::from_utf8(content) {
                        if let Ok(jsonrpc_core::Output::Success(message)) =
                            serde_json::from_str(&json_string)
                        {
                            if let Ok(initialize_result) =
                                serde_json::from_value::<lsp_types::InitializeResult>(
                                    message.result,
                                )
                            {
                                if result_tx.send(initialize_result).is_err() {
                                    dbg!("Error sending initialize result");
                                    return;
                                }
                            }
                        }
                    }
                }
            }
        });

        Self {
            server,
            writer,
            result_rx,
            receiver_thread,
            request_id: 0,
            text_document_sync: lsp_types::TextDocumentSyncKind::None,
            is_document_symbol_provider: false,
        }
    }

    /// Initializes the language server.
    fn initialize(&mut self) -> Result<(), LspError> {
        self.send_request::<lsp_request!("initialize")>(lsp_types::InitializeParams {
            process_id: Some(u64::from(std::process::id())),
            root_path: None,
            root_uri: None,
            initialization_options: None,
            capabilities: lsp_types::ClientCapabilities::default(),
            trace: None,
            workspace_folders: None,
        })?;
        let initialize_result = self.result_rx.recv()?;

        if let Some(lsp_types::TextDocumentSyncCapability::Kind(text_document_sync_kind)) =
            initialize_result.capabilities.text_document_sync
        {
            self.text_document_sync = text_document_sync_kind;
        }

        if let Some(is_document_symbol_provider) =
            initialize_result.capabilities.document_symbol_provider
        {
            self.is_document_symbol_provider = is_document_symbol_provider;
        }

        self.send_notification::<lsp_notification!("initialized")>(lsp_types::InitializedParams {})
    }

    /// Sends a request to the language server.
    fn send_request<T: Request>(&mut self, params: T::Params) -> Result<(), LspError>
    where
        T::Params: serde::Serialize,
    {
        if let serde_json::value::Value::Object(params) = serde_json::to_value(params)? {
            let request = jsonrpc_core::Call::MethodCall(jsonrpc_core::MethodCall {
                jsonrpc: Some(jsonrpc_core::Version::V2),
                method: T::METHOD.to_string(),
                params: jsonrpc_core::Params::Map(params),
                id: jsonrpc_core::Id::Num(self.request_id),
            });
            self.request_id += 1;
            self.send_call(&request)
        } else {
            Ok(())
        }
    }

    /// Sends a notification to the language server.
    fn send_notification<T: Notification>(&mut self, params: T::Params) -> Result<(), LspError>
    where
        T::Params: serde::Serialize,
    {
        if let serde_json::value::Value::Object(params) = serde_json::to_value(params)? {
            let notification = jsonrpc_core::Call::Notification(jsonrpc_core::Notification {
                jsonrpc: Some(jsonrpc_core::Version::V2),
                method: T::METHOD.to_string(),
                params: jsonrpc_core::Params::Map(params),
            });
            self.send_call(&notification)
        } else {
            Ok(())
        }
    }

    /// Sends the call to the language server.
    fn send_call(&mut self, call: &jsonrpc_core::Call) -> Result<(), LspError> {
        let json_string = serde_json::to_string(call)?;
        write!(
            self.writer,
            "Content-Length: {}\r\n\r\n{}",
            json_string.len(),
            json_string
        )?;
        Ok(())
    }
}

/// An error within the Language Server Protocol functionality.
#[derive(Clone, Copy, Debug)]
pub enum LspError {
    /// An error caused by serde_json.
    SerdeJson {
        /// The index of the line where the error occurred.
        line: usize,
        /// The index of the column where the error occurred.
        column: usize,
    },
    /// An error in IO.
    Io,
    /// An error caused while managing threads.
    Thread(RecvError),
}

impl From<serde_json::Error> for LspError {
    fn from(error: serde_json::Error) -> Self {
        LspError::SerdeJson {
            line: error.line(),
            column: error.column(),
        }
    }
}

impl From<io::Error> for LspError {
    fn from(_error: io::Error) -> Self {
        LspError::Io
    }
}

impl From<RecvError> for LspError {
    fn from(_error: RecvError) -> Self {
        LspError::Thread(RecvError)
    }
}

impl Display for LspError {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            LspError::SerdeJson { line, column } => {
                write!(f, "Serde Json Error at ({}, {})", line, column)
            }
            LspError::Io => write!(f, "IO Error"),
            LspError::Thread(error) => write!(f, "Thread Error {}", error),
        }
    }
}

impl error::Error for LspError {}

/// A null instance of [`Explorer`].
#[derive(Debug, Default)]
struct NullExplorer;

impl Explorer for NullExplorer {
    fn start(&mut self) -> Outcome<()> {
        Err(Failure::Quit)
    }

    fn read(&self, _path: &str) -> Outcome<String> {
        Err(Failure::Quit)
    }

    fn write(&self, _path: &str, _data: &str) -> Outcome<()> {
        Err(Failure::Quit)
    }
}

/// Signifies an [`Explorer`] of the local storage.
#[derive(Debug)]
pub(crate) struct Local {
    /// The [`LanguageClient`] fo the local storage [`Explorer`].
    language_client: LanguageClient,
}

impl Local {
    /// Creates a new Local.
    pub(crate) fn new() -> Self {
        Self {
            language_client: LanguageClient::new("rls"),
        }
    }
}

impl Explorer for Local {
    fn start(&mut self) -> Outcome<()> {
        self.language_client.initialize()?;
        Ok(())
    }

    fn read(&self, path: &str) -> Outcome<String> {
        Ok(fs::read_to_string(path).map(|data| data.replace('\r', ""))?)
    }

    fn write(&self, path: &str, data: &str) -> Outcome<()> {
        fs::write(path, data)?;
        Ok(())
    }
}

/// Signifies an [`Error`] from an [`Explorer`].
// Needed due to io::Error not implementing Clone for double.
#[derive(Clone, Copy, Debug)]
pub struct Error {
    /// The kind of the [`io::Error`].
    kind: io::ErrorKind,
}

impl error::Error for Error {}

impl Display for Error {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "IO Error")
    }
}

impl From<io::Error> for Error {
    fn from(value: io::Error) -> Self {
        Self { kind: value.kind() }
    }
}

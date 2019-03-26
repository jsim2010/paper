//! Implements the functionality to interact with data located in different storages.
use crate::mode::{Flag, Output};
use crate::{fmt, Debug, Display, Formatter};
use jsonrpc_core;
use lsp_types::notification::Notification;
use lsp_types::request::Request;
use lsp_types::{lsp_notification, lsp_request};
use serde::{Deserialize, Serialize};
use serde_json::{self, Value};
use std::cell::RefCell;
use std::error;
use std::fs;
use std::io::{self, BufRead, BufReader, Read, Write};
use std::path::Path;
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};
use std::rc::Rc;
use std::sync::mpsc::{channel, Receiver, RecvError};
use std::sync::{Arc, Mutex, MutexGuard};
use std::thread::{self, JoinHandle};

/// Interacts and processes file data.
pub trait Explorer: Debug {
    /// Initializes all functionality needed by the Explorer.
    fn start(&mut self) -> Output<()>;
    /// Returns the data from the file at a given path.
    fn read(&mut self, path: &Path) -> Output<String>;
    /// Writes data to a file at the given path.
    fn write(&self, path: &Path, data: &str) -> Output<()>;
    /// Returns the oldest notification from `Explorer`.
    fn receive_notification(&mut self) -> Option<ProgressParams>;
}

#[derive(Deserialize, Debug)]
/// ProgressParams defined by VSCode.
pub struct ProgressParams {
    id: String,
    title: String,
    pub message: Option<String>,
    done: Option<bool>,
}

/// The interface with the language server.
#[derive(Debug)]
struct LanguageClient {
    /// The thread running the language server.
    server: Child,
    /// The id for the next request to be sent by the `LanguageClient`.
    request_id: u64,
    /// How the language server expects to text documents to be synchronized.
    text_document_sync: lsp_types::TextDocumentSyncKind,
    /// If the language server is a document symbol provider.
    is_document_symbol_provider: bool,
    /// Receives the [`InitializeResult`] message.
    result_rx: Receiver<lsp_types::InitializeResult>,
    /// Registrations received from language server.
    registrations: lsp_types::RegistrationParams,
    /// Handle of the receiver thread.
    receiver_handle: Option<JoinHandle<()>>,
    notification_rx: Receiver<ProgressParams>,
}

/// Returns the length of the content that is next to be read.
fn get_content_length(reader: &mut BufReader<ChildStdout>) -> Result<usize, LspError> {
    let mut line = String::new();
    let mut blank_line = String::new();

    let _bytes_read = reader.read_line(&mut line)?;
    let mut split = line.trim().split(": ");

    if split.next() == Some("Content-Length") {
        let _bytes_read = reader.read_line(&mut blank_line)?;
        Ok(split
            .next()
            .ok_or(LspError::Protocol)
            .and_then(|value_string| value_string.parse().map_err(|_| LspError::Parse))?)
    } else {
        Err(LspError::Protocol)
    }
}

fn read_message(reader: &mut BufReader<ChildStdout>) -> Result<Value, LspError> {
    let content_length = get_content_length(reader)?;
    let mut content = vec![0; content_length];

    reader.read_exact(&mut content)?;
    let json_string = String::from_utf8(content).map_err(|_| LspError::Parse)?;
    let message = serde_json::from_str(&json_string)?;
    Ok(message)
}

impl LanguageClient {
    /// Creates a new `LanguageClient`.
    fn new(command: &str) -> Arc<Mutex<Self>> {
        let mut server = Command::new(command)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("Spawning the language server process.");
        let mut reader = std::io::BufReader::new(
            server
                .stdout
                .take()
                .expect("Accessing stdout of language server process"),
        );
        let (result_tx, result_rx) = channel::<lsp_types::InitializeResult>();
        let (notification_tx, notification_rx) = channel::<ProgressParams>();
        let new_client = Arc::new(Mutex::new(Self {
            server,
            result_rx,
            notification_rx,
            request_id: 0,
            text_document_sync: lsp_types::TextDocumentSyncKind::None,
            is_document_symbol_provider: false,
            registrations: lsp_types::RegistrationParams {
                registrations: Vec::new(),
            },
            receiver_handle: None,
        }));
        let client = Arc::clone(&new_client);

        let receiver_handle = thread::spawn(move || loop {
            let message = read_message(&mut reader).unwrap();

            if let Some(id) = message.get("id") {
                if let Ok(id) = serde_json::from_value::<u64>(id.to_owned()) {
                    if let Some(_method) = message.get("method") {
                        if let Ok(params) = serde_json::from_value::<lsp_types::RegistrationParams>(
                            message.get("params").unwrap().to_owned(),
                        ) {
                            let mut client = client
                                .lock()
                                .expect("Accessing language client from receiver");
                            client.registrations = params;
                            client
                                .send_response::<lsp_types::request::RegisterCapability>((), id)
                                .expect("Sending RegisterCapability to language server");
                        } else {
                            dbg!(message);
                        }
                    } else if let Some(result) = message.get("result") {
                        if let Ok(initialize_result) =
                            serde_json::from_value::<lsp_types::InitializeResult>(result.to_owned())
                        {
                            result_tx
                                .send(initialize_result)
                                .expect("Transferring InitializeResult to be processed");
                        } else {
                            dbg!(result);
                        }
                    } else {
                        dbg!(message);
                    }
                } else {
                    dbg!(message);
                }
            } else if let Some(_method) = message.get("method") {
                if let Ok(params) = serde_json::from_value::<ProgressParams>(
                    message.get("params").unwrap().to_owned(),
                ) {
                    notification_tx
                        .send(params)
                        .expect("Transferring notification")
                } else {
                    dbg!(message);
                }
            } else {
                dbg!(message);
            }
        });

        new_client
            .lock()
            .expect("Accessing language client")
            .receiver_handle = Some(receiver_handle);
        new_client
    }

    /// Returns a mutable reference to the stdin of the language server.
    fn stdin_mut(&mut self) -> &mut ChildStdin {
        self.server
            .stdin
            .as_mut()
            .expect("Accessing stdin of language server process.")
    }

    /// Initializes the language server.
    fn initialize(&mut self) -> Result<(), LspError> {
        self.send_request::<lsp_request!("initialize")>(lsp_types::InitializeParams {
            process_id: Some(u64::from(std::process::id())),
            root_path: None,
            root_uri: Some(
                lsp_types::Url::from_file_path(std::env::current_dir()?.as_path())
                    .map_err(|_| LspError::Io)?,
            ),
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
        T::Params: Serialize,
    {
        if let serde_json::value::Value::Object(params) = serde_json::to_value(params)? {
            let request = jsonrpc_core::Call::MethodCall(jsonrpc_core::MethodCall {
                jsonrpc: Some(jsonrpc_core::Version::V2),
                method: T::METHOD.to_string(),
                params: jsonrpc_core::Params::Map(params),
                id: jsonrpc_core::Id::Num(self.request_id),
            });
            self.request_id += 1;
            self.send_message(&request)
        } else {
            Ok(())
        }
    }

    /// Sends a response to the language server.
    fn send_response<T: Request>(&mut self, result: T::Result, id: u64) -> Result<(), LspError>
    where
        T::Result: Serialize,
    {
        let response = jsonrpc_core::Output::Success(jsonrpc_core::Success {
            jsonrpc: Some(jsonrpc_core::Version::V2),
            result: serde_json::to_value(result)?,
            id: jsonrpc_core::Id::Num(id),
        });
        self.send_message(&response)
    }

    /// Sends a notification to the language server.
    fn send_notification<T: Notification>(&mut self, params: T::Params) -> Result<(), LspError>
    where
        T::Params: Serialize,
    {
        if let serde_json::value::Value::Object(params) = serde_json::to_value(params)? {
            let notification = jsonrpc_core::Call::Notification(jsonrpc_core::Notification {
                jsonrpc: Some(jsonrpc_core::Version::V2),
                method: T::METHOD.to_string(),
                params: jsonrpc_core::Params::Map(params),
            });
            self.send_message(&notification)
        } else {
            Ok(())
        }
    }

    /// Sends a message to the language server.
    fn send_message<T: Serialize>(&mut self, message: &T) -> Result<(), LspError> {
        let json_string = serde_json::to_string(message)?;
        write!(
            self.stdin_mut(),
            "Content-Length: {}\r\n\r\n{}",
            json_string.len(),
            json_string
        )?;
        Ok(())
    }

    fn receive_notification(&self) -> Option<ProgressParams> {
        self.notification_rx.try_recv().ok()
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
    /// An error in parsing LSP data.
    Parse,
    /// An error in the LSP protocol.
    Protocol,
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
            LspError::Parse => write!(f, "Parse Error"),
            LspError::Protocol => write!(f, "Protocol Error"),
        }
    }
}

impl error::Error for LspError {}

/// Signifies an [`Explorer`] of the local storage.
#[derive(Debug)]
pub struct Local {
    /// The [`LanguageClient`] fo the local storage [`Explorer`].
    language_client: Arc<Mutex<LanguageClient>>,
}

impl Local {
    /// Creates a new Local.
    pub fn new() -> Rc<RefCell<Self>> {
        Rc::new(RefCell::new(Self {
            language_client: LanguageClient::new("rls"),
        }))
    }

    /// Returns a mutable reference to the language_client.
    fn language_client_mut(&mut self) -> MutexGuard<'_, LanguageClient> {
        self.language_client
            .lock()
            .expect("Accessing language_client")
    }
}

impl Explorer for Local {
    fn start(&mut self) -> Output<()> {
        self.language_client_mut().initialize()?;
        Ok(())
    }

    fn read(&mut self, path: &Path) -> Output<String> {
        let content = fs::read_to_string(path).map(|data| data.replace('\r', ""))?;
        self.language_client_mut()
            .send_notification::<lsp_notification!("textDocument/didOpen")>(
                lsp_types::DidOpenTextDocumentParams {
                    text_document: lsp_types::TextDocumentItem::new(
                        lsp_types::Url::from_file_path(path).map_err(|_| Flag::Quit)?,
                        "rust".into(),
                        0,
                        content.clone(),
                    ),
                },
            )?;
        Ok(content)
    }

    fn write(&self, path: &Path, data: &str) -> Output<()> {
        fs::write(path, data)?;
        Ok(())
    }

    fn receive_notification(&mut self) -> Option<ProgressParams> {
        self.language_client_mut().receive_notification()
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

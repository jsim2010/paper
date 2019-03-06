//! Implements the functionality to interact with data located in different storages.
use crate::{Failure, fmt, Debug, Display, Formatter, Outcome};
use std::error;
use std::fs;
use std::io::{self, Write, Read, BufRead};
use std::rc::Rc;
use std::process::{ChildStdin, Child, Command, Stdio};
use lsp_types::request::Request;
use lsp_types::notification::Notification;
use lsp_types::{lsp_request, lsp_notification};
use serde;
use serde_json;
use jsonrpc_core;
use std::cell::RefCell;
use std::thread;
use std::sync::mpsc::{channel, Receiver};

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
        explorer.borrow_mut().start();
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
        let s = Self {
            path: String::new(),
            explorer: Rc::new(RefCell::new(NullExplorer::default())),
        };
        s
    }
}

/// Interacts and processes file data.
pub trait Explorer: Debug {
    fn start(&mut self);
    /// Returns the data from the file at a given path.
    fn read(&self, path: &str) -> Outcome<String>;
    /// Writes data to a file at the given path.
    fn write(&self, path: &str, data: &str) -> Outcome<()>;
}

#[derive(Debug)]
struct LanguageClient {
    server: Child,
    writer: ChildStdin,
    request_id: u64,
    text_document_sync: lsp_types::TextDocumentSyncKind,
    is_document_symbol_provider: bool,
    result_rx: Receiver<lsp_types::InitializeResult>,
}

impl LanguageClient {
    fn new(command: &str) -> Self {
        let mut server = Command::new(command).stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped()).spawn().ok().unwrap();
        let mut reader = std::io::BufReader::new(server.stdout.take().unwrap());
        let writer = server.stdin.take().unwrap();
        let (result_tx, result_rx) = channel::<lsp_types::InitializeResult>();
        let client = Self {server, writer, result_rx, request_id: 0, text_document_sync: lsp_types::TextDocumentSyncKind::None, is_document_symbol_provider: false};

        thread::spawn(move || {
            loop {
                let mut line = String::new();
                reader.read_line(&mut line);
                let mut split = line.trim().split(": ");
                
                if split.next() == Some("Content-Length") {
                    let content_len = split.next().unwrap().parse().unwrap();
                    let mut content = vec![0; content_len];
                    reader.read_line(&mut line);
                    reader.read_exact(&mut content).unwrap();
                    let json_msg = String::from_utf8(content).unwrap();
                    if let Ok(jsonrpc_core::Output::Success(message)) = serde_json::from_str(&json_msg) {
                        result_tx.send(serde_json::from_value::<lsp_types::InitializeResult>(message.result).unwrap());
                    }
                }
            }
        });

        client
    }

    fn initialize(&mut self) {
        self.send_request::<lsp_request!("initialize")>(lsp_types::InitializeParams {
            process_id: Some(u64::from(std::process::id())),
            root_path: None,
            root_uri: None,
            initialization_options: None,
            capabilities: lsp_types::ClientCapabilities::default(),
            trace: None,
            workspace_folders: None,
        });
        let initialize_result = self.result_rx.recv().unwrap();

        if let Some(lsp_types::TextDocumentSyncCapability::Kind(text_document_sync_kind)) = initialize_result.capabilities.text_document_sync {
            self.text_document_sync = text_document_sync_kind;
        }

        if let Some(is_document_symbol_provider) = initialize_result.capabilities.document_symbol_provider {
            self.is_document_symbol_provider = is_document_symbol_provider;
        }

        self.send_notification::<lsp_notification!("initialized")>(lsp_types::InitializedParams{});
    }

    fn send_request<T: Request>(&mut self, params: T::Params) -> io::Result<()>
    where
        T::Params: serde::Serialize,
    {
        if let serde_json::value::Value::Object(params) = serde_json::to_value(params).unwrap() {
            let request = jsonrpc_core::Call::MethodCall(jsonrpc_core::MethodCall {
                jsonrpc: Some(jsonrpc_core::Version::V2),
                method: T::METHOD.to_string(),
                params: jsonrpc_core::Params::Map(params),
                id: jsonrpc_core::Id::Num(self.request_id),
            });
            self.request_id += 1;
            let request_serde = serde_json::to_string(&request).unwrap();
            write!(self.writer, "Content-Length: {}\r\n\r\n{}", request_serde.len(), request_serde)
        } else {
            Ok(())
        }
    }

    fn send_notification<T: Notification>(&mut self, params: T::Params) -> io::Result<()>
    where
        T::Params: serde::Serialize,
    {
        if let serde_json::value::Value::Object(params) = serde_json::to_value(params).unwrap() {
            let notification = jsonrpc_core::Call::Notification(jsonrpc_core::Notification {
                jsonrpc: Some(jsonrpc_core::Version::V2),
                method: T::METHOD.to_string(),
                params: jsonrpc_core::Params::Map(params),
            });
            let notification_serde = serde_json::to_string(&notification).unwrap();
            write!(self.writer, "Content-Length: {}\r\n\r\n{}", notification_serde.len(), notification_serde)
        } else {
            Ok(())
        }
    }
}

#[derive(Debug, Default)]
struct NullExplorer;

impl Explorer for NullExplorer {
    fn start(&mut self) {
    }

    fn read(&self, path: &str) -> Outcome<String> {
        Err(Failure::Quit)
    }

    fn write(&self, path: &str, data: &str) -> Outcome<()> {
        Err(Failure::Quit)
    }
}

/// Signifies an [`Explorer`] of the local storage.
#[derive(Debug)]
pub(crate) struct Local {
    language_client: LanguageClient
}

impl Local {
    pub fn new() -> Self {
        Local { language_client: LanguageClient::new("rls") }
    }
}

impl Explorer for Local {
    fn start(&mut self) {
        self.language_client.initialize();
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

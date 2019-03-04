//! Implements the functionality to interact with data located in different storages.
use crate::{Failure, fmt, Debug, Display, Formatter, Outcome};
use std::error;
use std::fs;
use std::io::{self, Write, Read, BufRead};
use std::rc::Rc;
use std::process::{Child, Command, Stdio};
use lsp_types::request::Request;
use serde;
use serde_json;
use jsonrpc_core;
use std::cell::RefCell;

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
    process: Child,
    reader: io::BufReader<std::process::ChildStdout>,
    id: u64,
}

impl LanguageClient {
    fn new(command: &str) -> Self {
        let mut process = Command::new(command).stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped()).spawn().ok().unwrap();
        let reader = std::io::BufReader::new(process.stdout.take().unwrap());

        Self {process, reader, id: 0}
    }

    fn send_request<T: Request>(&mut self, params: T::Params) -> io::Result<()>
    where
        T:: Params: serde::Serialize,
    {
        if let serde_json::value::Value::Object(params) = serde_json::to_value(params).unwrap() {
            let request = jsonrpc_core::Call::MethodCall(jsonrpc_core::MethodCall {
                jsonrpc: Some(jsonrpc_core::Version::V2),
                method: T::METHOD.to_string(),
                params: jsonrpc_core::Params::Map(params),
                id: jsonrpc_core::Id::Num(self.id),
            });
            self.id += 1;
            let request_serde = serde_json::to_string(&request).unwrap();
            write!(self.process.stdin.take().unwrap(), "Content-Length: {}\r\n\r\n{}", request_serde.len(), request_serde)
        } else {
            Ok(())
        }
    }

    fn recv_message(&mut self) {
        let mut line = String::new();
        self.reader.read_line(&mut line);
        let mut split = line.trim().split(": ");
        
        if split.next() == Some("Content-Length") {
            let content_len = split.next().unwrap().parse().unwrap();
            let mut content = vec![0; content_len];
            self.reader.read_line(&mut line);
            self.reader.read_exact(&mut content).unwrap();
            let json_msg = String::from_utf8(content).unwrap();
            if let Ok(jsonrpc_core::Output::Success(message)) = serde_json::from_str(&json_msg) {
                dbg!(message.result);
            }
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
        self.language_client.send_request::<lsp_types::request::Initialize>(lsp_types::InitializeParams {
            process_id: Some(u64::from(std::process::id())),
            root_path: None,
            root_uri: None,
            initialization_options: None,
            capabilities: lsp_types::ClientCapabilities::default(),
            trace: None,
            workspace_folders: None,
        });
        self.language_client.recv_message();
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

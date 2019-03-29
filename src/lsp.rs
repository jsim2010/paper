//! Implements the client side of the language server protocol.
use crate::storage::LspError;
use jsonrpc_core::{self, Id, Version};
use lsp_types::{
    ClientCapabilities, DidOpenTextDocumentParams, InitializeParams, InitializeResult,
    InitializedParams, PublishDiagnosticsParams, Registration, RegistrationParams,
    ServerCapabilities, Url,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::env;
use std::io::{BufRead, BufReader, Read, Write};
use std::process::{self, Child, ChildStdin, ChildStdout, Command, Stdio};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::thread::{Builder, JoinHandle};

/// The interface with the language server.
#[derive(Debug)]
pub(crate) struct LanguageClient {
    /// The thread running the language server.
    server: Child,
    /// The id for the next request to be sent by `LanguageClient`.
    request_id: u64,
    /// The capabilities of the language server.
    server_capabilities: ServerCapabilities,
    /// Registrations received from language server.
    registrations: Vec<Registration>,
    /// Handle of the receiver thread.
    receiver_handle: Option<JoinHandle<()>>,
    /// Receives notifications.
    notification_rx: Receiver<ProgressParams>,
}

impl LanguageClient {
    /// Creates a new `LanguageClient`.
    pub(crate) fn new(command: &str) -> Arc<Mutex<Self>> {
        let server = Self::spawn_server(command);
        let (notification_tx, notification_rx) = mpsc::channel::<ProgressParams>();
        let their_client = Arc::new(Mutex::new(Self {
            server,
            notification_rx,
            request_id: u64::default(),
            server_capabilities: ServerCapabilities::default(),
            registrations: Vec::new(),
            receiver_handle: None,
        }));
        let my_client = Arc::clone(&their_client);
        their_client
            .lock()
            .expect("Locking language client")
            .receiver_handle = Builder::new()
            .name("LangClientRx".to_string())
            .spawn(move || Self::process(my_client, &notification_tx))
            .ok();
        their_client
    }

    /// Returns a spawned progress running the language server.
    fn spawn_server(command: &str) -> Child {
        Command::new(command)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("Spawning language server process")
    }

    /// Process a message received from the language server.
    fn process(mut client: Arc<Mutex<Self>>, notification_tx: &Sender<ProgressParams>) {
        let mut messages = MessageReader::new(&mut client);

        loop {
            match messages.next_message() {
                Ok(message) => match message.message {
                    Message::Request(request) => {
                        Self::process_request(&mut client, request);
                    }
                    Message::Response(response) => {
                        Self::process_response(&mut client, response);
                    }
                    Message::Notification(notification) => {
                        Self::process_notification(notification_tx, notification);
                    }
                },
                Err(e) => {
                    dbg!(e);
                }
            }
        }
    }

    /// Processes a request received from the language server.
    fn process_request(client: &mut Arc<Mutex<Self>>, request: RequestMessage) {
        let mut client = client.lock().expect("Locking language client");

        if let RequestMethod::RegisterCapability(params) = request.method {
            client.registrations = params.registrations;
            client
                .send_message(Message::register_capability_response(request.id))
                .expect("Sending registerCapability response");
        }
    }

    /// Processes a response received from the language server.
    fn process_response(client: &mut Arc<Mutex<Self>>, response: ResponseMessage) {
        let mut client = client.lock().expect("Locking language client");

        if let Status::Result(ResultValue::Initialize(result)) = response.status {
            client.server_capabilities = result.capabilities;
            client
                .send_message(Message::initialized_notification())
                .expect("Sending initialized notification");
        }
    }

    /// Processes a notification received from the language server.
    fn process_notification(
        notification_tx: &Sender<ProgressParams>,
        notification: NotificationMessage,
    ) {
        // TODO: Might be easier to store the data in client then check it periodically.
        if let NotificationMessage::WindowProgress(params) = notification {
            notification_tx.send(params).expect("Queuing progress data");
        }
    }

    /// Initializes the language server.
    pub(crate) fn initialize(&mut self) -> Result<(), LspError> {
        self.request_id += 1;
        self.send_message(Message::initialize_request(
            self.request_id,
            InitializeParams {
                process_id: Some(u64::from(process::id())),
                root_path: None,
                root_uri: Some(
                    Url::from_file_path(env::current_dir()?.as_path()).map_err(|_| LspError::Io)?,
                ),
                initialization_options: None,
                capabilities: ClientCapabilities::default(),
                trace: None,
                workspace_folders: None,
            },
        ))
    }

    /// Sends a message to the language server.
    pub(crate) fn send_message(&mut self, message: Message) -> Result<(), LspError> {
        let json_string = serde_json::to_string(&AbstractMessage::new(message))?;

        write!(
            self.stdin_mut(),
            "Content-Length: {}\r\n\r\n{}",
            json_string.len(),
            json_string
        )?;

        Ok(())
    }

    /// Returns a mutable reference to the stdin of the language server.
    fn stdin_mut(&mut self) -> &mut ChildStdin {
        self.server
            .stdin
            .as_mut()
            .expect("Accessing stdin of language server process.")
    }

    /// Return the notification from the `Explorer`.
    pub(crate) fn receive_notification(&self) -> Option<ProgressParams> {
        self.notification_rx.try_recv().ok()
    }

    /// Returns the stdout of the language server.
    fn stdout(&mut self) -> ChildStdout {
        self.server
            .stdout
            .take()
            .expect("Taking stdout of language server")
    }
}

/// Specifies the content of a message.
#[derive(Debug, Deserialize, Serialize)]
pub(crate) struct AbstractMessage {
    /// The JSON-RPC version of the message.
    jsonrpc: Version,

    /// The data of a message.
    #[serde(flatten)]
    message: Message,
}

/// Specifies all types of messages.
#[derive(Debug, Deserialize, Serialize)]
#[serde(untagged)]
pub(crate) enum Message {
    /// A request message.
    Request(RequestMessage),
    /// A response message.
    Response(ResponseMessage),
    /// A notification message.
    Notification(NotificationMessage),
}

/// Specifies the format of a request message.
#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct RequestMessage {
    /// The request id.
    id: Id,
    /// The method to be invoked.
    #[serde(flatten)]
    method: RequestMethod,
}

/// Specifies all request messages.
#[derive(Debug, Deserialize, Serialize)]
#[serde(tag = "method", content = "params")]
pub(crate) enum RequestMethod {
    /// The initialize request.
    #[serde(rename = "initialize")]
    Initialize(Box<InitializeParams>),
    /// The registerCapabiility request.
    #[serde(rename = "client/registerCapability")]
    RegisterCapability(RegistrationParams),
}

/// Specifies the format of a response message.
#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ResponseMessage {
    /// The request id.
    id: Id,
    /// The result or error.
    #[serde(flatten)]
    status: Status,
}

/// Specifies the format of the result or error in a response message.
#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) enum Status {
    /// The result of a request.
    ///
    /// REQUIRED on success.
    /// MUST NOT exist if there was an error invoking the method.
    Result(ResultValue),
    /// The error object in case a request fails.
    Error(ResponseError),
}

/// Specifies all response messages.
#[derive(Debug, Deserialize, Serialize)]
#[serde(untagged)]
pub(crate) enum ResultValue {
    /// The initialize response.
    Initialize(Box<InitializeResult>),
    /// The registerCapability response.
    RegisterCapability,
}

/// Specifies the error of a response message.
#[derive(Debug, Deserialize, Serialize)]
pub(crate) struct ResponseError {
    /// Indicates the error type that occurred.
    code: u64,
    /// Provides a short description of the error.
    message: String,
    /// Contains additional information about the error.
    ///
    /// Can be omitted.
    data: Option<Value>,
}

/// Specifies all notification messages.
#[derive(Debug, Deserialize, Serialize)]
#[serde(tag = "method", content = "params")]
pub(crate) enum NotificationMessage {
    /// The progress notification.
    #[serde(rename = "window/progress")]
    WindowProgress(ProgressParams),
    /// The initialized notification.
    #[serde(rename = "initialized")]
    Initialized(InitializedParams),
    /// The didOpen notification.
    #[serde(rename = "textDocument/didOpen")]
    DidOpenTextDocument(DidOpenTextDocumentParams),
    /// The publishDiagnostics notification.
    #[serde(rename = "textDocument/publishDiagnostics")]
    PublishDiagnostics(PublishDiagnosticsParams),
}

#[derive(Deserialize, Debug, Serialize)]
/// `ProgressParams` defined by `VSCode`.
pub struct ProgressParams {
    /// The id of the notification.
    id: String,
    /// The title of the notification.
    title: String,
    /// The message of the notification.
    pub message: Option<String>,
    /// Indicates if no more notifications will be sent.
    done: Option<bool>,
}

impl AbstractMessage {
    /// Creates a new `AbstractMessage`.
    fn new(message: Message) -> Self {
        Self {
            jsonrpc: Version::V2,
            message,
        }
    }
}

impl Message {
    /// Creates a new Initialize request `Message`.
    fn initialize_request(id: u64, params: InitializeParams) -> Self {
        Message::Request(RequestMessage {
            id: Id::Num(id),
            method: RequestMethod::Initialize(Box::new(params)),
        })
    }

    /// Creates a new RegisterCapability response `Message`.
    fn register_capability_response(id: Id) -> Self {
        Message::Response(ResponseMessage {
            id,
            status: Status::Result(ResultValue::RegisterCapability),
        })
    }

    /// Creates a new Initialized notification `Message`.
    fn initialized_notification() -> Self {
        Message::Notification(NotificationMessage::Initialized(InitializedParams {}))
    }

    /// Creates a new DidOpenTextDocument notification `Message`.
    pub(crate) fn did_open_text_document_notification(params: DidOpenTextDocumentParams) -> Self {
        Message::Notification(NotificationMessage::DidOpenTextDocument(params))
    }
}

/// Reads messages from the language server.
struct MessageReader {
    /// Reads data from the language server.
    reader: BufReader<ChildStdout>,
}

impl MessageReader {
    /// Creates a new `MessageReader`.
    fn new(client: &mut Arc<Mutex<LanguageClient>>) -> Self {
        Self {
            reader: BufReader::new(client.lock().expect("Locking language client").stdout()),
        }
    }

    /// Returns the next message from the language server.
    fn next_message(&mut self) -> Result<AbstractMessage, LspError> {
        let mut content = vec![0; self.get_content_length()?];
        self.reader.read_exact(&mut content)?;
        // TODO: Improve error handling to give better info.
        serde_json::from_slice(&content).map_err(|_| LspError::Parse)
    }

    /// Returns the length of the content.
    ///
    /// When this returns, the reader will point to the content of the message.
    fn get_content_length(&mut self) -> Result<usize, LspError> {
        let mut line = String::new();
        let mut blank_line = String::new();

        let mut _bytes_read = self.reader.read_line(&mut line)?;
        let mut split = line.trim().split(": ");

        if split.next() == Some("Content-Length") {
            _bytes_read = self.reader.read_line(&mut blank_line)?;
            Ok(split
                .next()
                .ok_or(LspError::Protocol)
                .and_then(|value_string| value_string.parse().map_err(|_| LspError::Parse))?)
        } else {
            Err(LspError::Protocol)
        }
    }
}

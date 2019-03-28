use crate::file::ProgressParams;
use crate::storage::LspError;
use jsonrpc_core;
use lsp_types::notification::{DidOpenTextDocument, Initialized, Notification};
use lsp_types::request::{Initialize, RegisterCapability, Request};
use lsp_types::{
    ClientCapabilities, DidOpenTextDocumentParams, InitializeParams, InitializeResult, Registration,
    RegistrationParams, ServerCapabilities, Url,
};
use serde::Serialize;
use serde_json::Value;
use std::io::{BufRead, BufReader, Read, Write};
use std::process::{self, Child, ChildStdin, ChildStdout, Command, Stdio};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::env;
use try_from::TryFrom;

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
            .receiver_handle = Some(thread::spawn(move || {
            Self::process(my_client, notification_tx)
        }));
        their_client
    }

    fn spawn_server(command: &str) -> Child {
        Command::new(command)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("Spawning language server process")
    }

    fn process(mut client: Arc<Mutex<Self>>, notification_tx: Sender<ProgressParams>) {
        let mut messages = MessageReader::new(&mut client);

        loop {
            if let Ok(message) = messages.next_message() {
                match message {
                    Message::Request(id, request) => {
                        Self::process_request(&mut client, id, request);
                    }
                    Message::Response(_id, response) => {
                        Self::process_response(&mut client, response);
                    }
                    Message::Notification(notification) => {
                        Self::process_notification(&notification_tx, notification);
                    }
                    Message::Unknown => (),
                }
            }
        }
    }

    fn process_request(client: &mut Arc<Mutex<LanguageClient>>, id: u64, request: RequestParams) {
        let mut client = client.lock().expect("Locking language client");

        match request {
            RequestParams::RegisterCapability(params) => {
                client.registrations = params.registrations;
                client
                    .send_message(&Message::register_capability_result(id))
                    .unwrap();
            }
            _ => {
                // TODO: Send error.
            }
        }
    }

    fn process_response(client: &mut Arc<Mutex<LanguageClient>>, response: Response) {
        let mut client = client.lock().expect("Locking language client");

        match response {
            Response::Result(ResultValue::Initialize(result)) => {
                client.server_capabilities = result.capabilities;
                client
                    .send_message(&Message::initialized_notification())
                    .unwrap()
            }
            _ => {
                // TODO: Process error.
            }
        }
    }

    fn process_notification(
        notification_tx: &Sender<ProgressParams>,
        notification: NotificationParams,
    ) {
        match notification {
            NotificationParams::WindowProgress(params) => notification_tx.send(params).unwrap(),
            _ => (),
        }
    }

    pub(crate) fn initialize(&mut self) -> Result<(), LspError> {
        let result = self.send_message(&Message::initialize_request(
            self.request_id,
            InitializeParams {
                process_id: Some(u64::from(process::id())),
                root_path: None,
                root_uri: Some(
                    Url::from_file_path(env::current_dir()?.as_path())
                        .map_err(|_| LspError::Io)?,
                ),
                initialization_options: None,
                capabilities: ClientCapabilities::default(),
                trace: None,
                workspace_folders: None,
            },
        ));

        if result.is_ok() {
            self.request_id += 1;
        }

        result
    }

    /// Sends a message to the language server.
    pub(crate) fn send_message(&mut self, message: &Message) -> Result<(), LspError> {
        let json_string = match message {
            Message::Request(id, params) => {
                if let Value::Object(params_value) = serde_json::to_value(params)? {
                    let request = jsonrpc_core::Call::MethodCall(jsonrpc_core::MethodCall {
                        jsonrpc: Some(jsonrpc_core::Version::V2),
                        method: message.method(),
                        params: jsonrpc_core::Params::Map(params_value),
                        id: jsonrpc_core::Id::Num(*id),
                    });
                    Some(serde_json::to_string(&request)?)
                } else {
                    None
                }
            }
            Message::Response(id, response) => match response {
                Response::Result(result) => {
                    let content = jsonrpc_core::Output::Success(jsonrpc_core::Success {
                        jsonrpc: Some(jsonrpc_core::Version::V2),
                        result: serde_json::to_value(result)?,
                        id: jsonrpc_core::Id::Num(*id),
                    });
                    Some(serde_json::to_string(&content)?)
                }
                Response::Error(_) => None,
            },
            Message::Notification(params) => {
                if let serde_json::value::Value::Object(params) = serde_json::to_value(params)? {
                    let notification =
                        jsonrpc_core::Call::Notification(jsonrpc_core::Notification {
                            jsonrpc: Some(jsonrpc_core::Version::V2),
                            method: message.method(),
                            params: jsonrpc_core::Params::Map(params),
                        });
                    Some(serde_json::to_string(&notification)?)
                } else {
                    None
                }
            }
            _ => None,
        };

        // TODO: Move self.old_message to here (currently blocked by fact that different messages
        // return different types for their content.
        // We should convert to value first.

        if let Some(data) = json_string {
            write!(
                self.stdin_mut(),
                "Content-Length: {}\r\n\r\n{}",
                data.len(),
                data
            )?;
        }

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

    fn stdout(&mut self) -> ChildStdout {
        self.server
            .stdout
            .take()
            .expect("Taking stdout of language server")
    }
}

pub(crate) enum Message {
    Unknown,
    Request(u64, RequestParams),
    Response(u64, Response),
    Notification(NotificationParams),
}

impl Message {
    fn method(&self) -> String {
        match self {
            Message::Request(_id, params) => params.method(),
            Message::Notification(params) => params.method(),
            _ => "",
        }
        .to_string()
    }

    fn initialize_request(id: u64, params: InitializeParams) -> Self {
        Message::Request(id, RequestParams::Initialize(params))
    }

    fn register_capability_result(id: u64) -> Self {
        Message::Response(id, Response::Result(ResultValue::RegisterCapability))
    }

    fn initialized_notification() -> Self {
        Message::Notification(NotificationParams::Initialized)
    }

    pub(crate) fn did_open_text_document(params: DidOpenTextDocumentParams) -> Self {
        Message::Notification(NotificationParams::DidOpenTextDocument(params))
    }
}

#[derive(Serialize)]
pub(crate) enum NotificationParams {
    WindowProgress(ProgressParams),
    Initialized,
    DidOpenTextDocument(DidOpenTextDocumentParams),
}

impl NotificationParams {
    fn method(&self) -> &str {
        match self {
            NotificationParams::WindowProgress(_) => "window/progress",
            NotificationParams::Initialized => Initialized::METHOD,
            NotificationParams::DidOpenTextDocument(_) => DidOpenTextDocument::METHOD,
        }
    }
}

pub(crate) enum Response {
    Result(ResultValue),
    Error(jsonrpc_core::ErrorCode),
}

#[derive(Serialize)]
pub(crate) enum ResultValue {
    Initialize(InitializeResult),
    RegisterCapability,
}

#[derive(Serialize)]
pub(crate) enum RequestParams {
    RegisterCapability(RegistrationParams),
    Initialize(InitializeParams),
}

impl RequestParams {
    fn method(&self) -> &str {
        match self {
            RequestParams::RegisterCapability(_) => RegisterCapability::METHOD,
            RequestParams::Initialize(_) => Initialize::METHOD,
        }
    }
}

impl TryFrom<Value> for Message {
    type Err = LspError;

    fn try_from(value: Value) -> Result<Self, Self::Err> {
        if let Some(id_value) = value.get("id") {
            if let Ok(id) = serde_json::from_value::<u64>(id_value.to_owned()) {
                if value.get("method").is_some() {
                    if let Ok(params) = serde_json::from_value::<RegistrationParams>(
                        value.get("params").unwrap().to_owned(),
                    ) {
                        return Ok(Message::Request(
                            id,
                            RequestParams::RegisterCapability(params),
                        ));
                    }
                } else if let Some(result) = value.get("result") {
                    if let Ok(initialize_result) =
                        serde_json::from_value::<InitializeResult>(result.to_owned())
                    {
                        return Ok(Message::Response(
                            id,
                            Response::Result(ResultValue::Initialize(initialize_result)),
                        ));
                    }
                }
            }
        } else if value.get("method").is_some() {
            if let Ok(params) =
                serde_json::from_value::<ProgressParams>(value.get("params").unwrap().to_owned())
            {
                return Ok(Message::Notification(NotificationParams::WindowProgress(
                    params,
                )));
            }
        }

        dbg!(value);
        Ok(Message::Unknown)
    }
}

struct MessageReader {
    reader: BufReader<ChildStdout>,
}

impl MessageReader {
    fn new(client: &mut Arc<Mutex<LanguageClient>>) -> Self {
        Self {
            reader: BufReader::new(client.lock().expect("Locking language client").stdout()),
        }
    }

    fn next_message(&mut self) -> Result<Message, LspError> {
        let content_length = self.get_content_length()?;
        let mut content = vec![0; content_length];

        self.reader.read_exact(&mut content)?;
        let json_string = String::from_utf8(content).map_err(|_| LspError::Parse)?;
        serde_json::from_str(&json_string)
            .map_err(|_| LspError::Parse)
            .and_then(|content| Message::try_from(content))
    }

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

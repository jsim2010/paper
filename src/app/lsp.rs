//! Implements management and use of language servers.
use {
    jsonrpc_core::{Call, Id, MethodCall, Output, Params, Success, Value, Version},
    log::{error, trace, warn},
    lsp_types::{DidOpenTextDocumentParams, TextDocumentItem, TextDocumentSyncCapability, TextDocumentSyncKind, ClientCapabilities, InitializeParams, InitializedParams, Url},
    serde::Serialize,
    serde_json::error::Error as SerdeJsonError,
    std::{
        env, fmt,
        io::{self, BufRead, BufReader, Read, Write},
        process::{self, Child, ChildStdin, ChildStdout, Command, Stdio},
        sync::{
            mpsc::{self, Receiver, Sender},
            Arc, Mutex, MutexGuard,
        },
        thread,
    },
    thiserror::Error,
};

/// An LSP Error.
#[derive(Debug, Error)]
pub enum Fault {
    /// Receiver of channel is disconnected.
    #[error("{0} receiver disconnected")]
    DisconnectedReceiver(String),
    /// Sender of channel is disconnected.
    #[error("{0} sender disconnected")]
    DisconnectedSender(String),
    /// Io of language server is unaccessable.
    #[error("unable to access {0} of language server")]
    UnaccessableIo(String),
    /// Command unable to run.
    #[error("command failed to run")]
    InvalidCommand,
    /// Invalid current working directory.
    #[error("current working directory is invalid: {0}")]
    InvalidCwd(#[source] io::Error),
    /// Failure writing to language server process.
    #[error("unable to write to language server process: {0}")]
    LanguageServerWrite(#[source] io::Error),
    /// Invalid [`Params`].
    #[error("invalid params")]
    InvalidParams,
    /// Error while acquiring mutex.
    #[error("mutex")]
    Mutex,
    /// Invalid path.
    #[error("invalid path")]
    InvalidPath,
    /// Serde json error.
    #[error("serde json: {0}")]
    SerdeJson(#[from] SerdeJsonError),
}

/// Processes data from the language server.
struct LspProcessor {
    /// Transmits data to the language server process.
    transmitter: LspTransmitter,
    /// Reads data from the language server process.
    reader: BufReader<ChildStdout>,
    /// Sends data to the [`LspServer`].
    response_tx: Sender<Message>,
    /// Signifies if the thread is quitting.
    is_quitting: bool,
}

impl LspProcessor {
    /// Creates a new `LspProcessor`.
    fn new(
        process: &mut Child,
        response_tx: Sender<Message>,
        transmitter: LspTransmitter,
    ) -> Result<Self, Fault> {
        process
            .stdout
            .take()
            .ok_or_else(|| Fault::UnaccessableIo("stdout".to_string()))
            .map(|stdout| Self {
                reader: BufReader::new(stdout),
                response_tx,
                is_quitting: false,
                transmitter,
            })
    }

    /// Processes data from the language server.
    fn process(&mut self) -> Result<(), Fault> {
        let mut line = String::new();
        let mut blank_line = String::new();

        while !self.is_quitting {
            if self.reader.read_line(&mut line).is_ok() {
                let mut split = line.trim().split(": ");

                if split.next() == Some("Content-Length")
                    && self.reader.read_line(&mut blank_line).is_ok()
                {
                    if let Some(length_str) = split.next() {
                        if let Ok(length) = length_str.parse() {
                            let mut content = vec![0; length];

                            if self.reader.read_exact(&mut content).is_ok() {
                                if let Ok(json_string) = String::from_utf8(content) {
                                    trace!("Received: {}", json_string);
                                    if let Ok(message) = serde_json::from_str::<Value>(&json_string)
                                    {
                                        if let Some(id) = message.get("id") {
                                            if let Some(result) = message.get("result") {
                                                // Success response
                                                let response_id = serde_json::from_value(id.clone())?;
                                                let response_result = serde_json::from_value(result.clone())?;

                                                self.response_tx.send(Message::Response{
                                                    id: response_id,
                                                    outcome: Outcome::Success(response_result),
                                                }).map_err(
                                                    |_| {
                                                        Fault::DisconnectedReceiver(
                                                            "response".to_string(),
                                                        )
                                                    },
                                                )?;
                                            } else if message.get("error").is_some() {
                                                // Error response
                                            } else {
                                                // Request
                                                if let Ok(message_id) =
                                                    serde_json::from_value::<u64>(id.clone())
                                                {
                                                    self.transmitter
                                                        .respond(&Response {
                                                            id: Id::Num(message_id),
                                                        })?;
                                                }
                                            }
                                        } else {
                                            // Notification
                                        }
                                    }
                                }
                            }
                        }
                    }
                }

                line.clear();
            }
        }

        Ok(())
    }
}

impl Drop for LspProcessor {
    fn drop(&mut self) {
        self.is_quitting = true;
    }
}

enum Message {
    Response {
        id: u64,
        outcome: Outcome,
    },
}

enum Outcome {
    Success(Value),
}

#[derive(Debug)]
struct LspReceiver(Receiver<Message>);

impl LspReceiver {
    fn new(server_process: &mut Child, transmitter: &LspTransmitter) -> Result<Self, Fault> {
        let (tx, rx) = mpsc::channel();
        let mut processor = LspProcessor::new(server_process, tx, transmitter.clone())?;

        let _ = thread::spawn(move || {
            if let Err(error) = processor.process() {
                error!("processing language server output: {}", error);
            }
        });

        Ok(Self(rx))
    }

    fn recv(&self) -> Result<Message, Fault> {
        self.0.recv().map_err(|_| Fault::DisconnectedSender("language server stdout".to_string()))
    }
}

#[derive(Debug)]
struct LspErrorProcessor(Sender<()>);

impl LspErrorProcessor {
    fn new(server_process: &mut Child) -> Result<Self, Fault> {
        let stderr = server_process.stderr.take().ok_or_else(|| Fault::UnaccessableIo("stderr".to_string()))?;
        let (tx, rx) = mpsc::channel();
        let _ = thread::spawn(move || {
            let mut reader = BufReader::new(stderr);
            let mut line = String::new();

            while rx.try_recv().is_err() {
                // Rust's language server (rls) seems to send empty lines over stderr after shutdown request so skip those.
                if reader.read_line(&mut line).is_ok() && !line.is_empty() {
                    error!("{}", line);
                    line.clear();
                }
            }
        });

        Ok(Self(tx))
    }

    fn terminate(&self) -> Result<(), Fault> {
        self.0
            .send(())
            .map_err(|_| Fault::DisconnectedReceiver("language server stderr".to_string()))
    }
}

/// Represents a language server process.
#[derive(Debug)]
pub(crate) struct LspServer {
    /// The language server process.
    process: Child,
    /// Transmits messages to the language server process.
    transmitter: LspTransmitter,
    error_processor: LspErrorProcessor,
    notify_open_close: bool,
    receiver: LspReceiver,
}

impl LspServer {
    /// Creates a new `LspServer` represented by `process_cmd`.
    pub(crate) fn new(process_cmd: &str) -> Result<Self, Fault> {
        let mut process = Command::new(process_cmd)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|_| Fault::InvalidCommand)?;
        let mut transmitter = LspTransmitter::new(&mut process)?;
        let receiver = LspReceiver::new(&mut process, &transmitter)?;
        let error_processor = LspErrorProcessor::new(&mut process)?;

        #[allow(deprecated)] // root_path is a required field.
        let server = transmitter.request(
            &InitializeParams {
                process_id: Some(u64::from(process::id())),
                root_path: None,
                root_uri: Some(
                    Url::from_directory_path(
                        env::current_dir().map_err(Fault::InvalidCwd)?.as_path(),
                    )
                    .map_err(|_| Fault::InvalidPath)?,
                ),
                initialization_options: None,
                capabilities: ClientCapabilities::default(),
                trace: None,
                workspace_folders: None,
                client_info: None,
            }
            .into(),
            &receiver,
        )?;

        let mut notify_open_close = false;

        if let Some(text_document_sync) = server.capabilities.text_document_sync {
            match text_document_sync {
                TextDocumentSyncCapability::Kind(kind) => {
                    if kind != TextDocumentSyncKind::None {
                        notify_open_close = true;
                    }
                }
                TextDocumentSyncCapability::Options(options) => {
                    if let Some(open_close) = options.open_close {
                        notify_open_close = open_close;
                    }
                }
            }
        }

        transmitter.notify(&InitializedParams {}.into())?;

        Ok(Self {
            process,
            transmitter,
            error_processor,
            notify_open_close,
            receiver,
        })
    }

    pub(crate) fn did_open(&mut self, text_document: &TextDocumentItem) -> Result<(), Fault> {
        if self.notify_open_close {
            self.transmitter.notify(&DidOpenTextDocumentParams {
                text_document: text_document.clone(),
            }.into())?;
        }

        Ok(())
    }

    /// Attempts to cleanly kill the language server process.
    fn shutdown_and_exit(&mut self) -> Result<(), Fault> {
        self.transmitter.request(&ShutdownParams(()).into(), &self.receiver)?;
        self.error_processor.terminate()?;
        self.transmitter.notify(&ExitParams {}.into())?;

        if let Err(e) = self.process.wait() {
            warn!("Unable to wait on language server process exit: {}", e);
        }

        Ok(())
    }
}

impl Drop for LspServer {
    fn drop(&mut self) {
        if let Err(e) = self.shutdown_and_exit() {
            warn!("Unable to cleanly shutdown and exit language server: {}", e);

            if let Err(kill_error) = self.process.kill() {
                warn!("Unable to kill language server process: {}", kill_error);
            }
        }
    }
}

#[derive(Clone, Debug)]
struct LspTransmitter(Arc<Mutex<AtomicTransmitter>>);

impl LspTransmitter {
    /// Creates a new `LspTransmitter`.
    fn new(process: &mut Child) -> Result<Self, Fault> {
        Ok(Self(Arc::new(Mutex::new(AtomicTransmitter {
            id: 0,
            stdin: process.stdin.take().ok_or_else(|| Fault::UnaccessableIo("stdin".to_string()))?,
        }))))

    }

    /// Sends `notification` to the lsp server.
    fn notify(&mut self, notification: &Notification) -> Result<(), Fault> {
        self.lock()?.send_message(Content::Request(Call::Notification(
            jsonrpc_core::Notification {
                jsonrpc: Some(Version::V2),
                method: notification.method().clone(),
                params: notification.params()?,
            },
        )))
    }

    /// Sends `response` to the lsp server.
    fn respond(&mut self, response: &Response) -> Result<(), Fault> {
        self.lock()?.send_message(Content::Response(Output::Success(Success {
            jsonrpc: Some(Version::V2),
            result: Value::Null,
            id: response.id().clone(),
        })))
    }

    /// Sends `request` to the lsp server and waits for the response.
    #[allow(single_use_lifetimes)] // 'de is needed to compile.
    fn request<T>(&mut self, request: &Request<T>, receiver: &LspReceiver) -> Result<T::Result, Fault>
    where
        T: lsp_types::request::Request,
        <T as lsp_types::request::Request>::Params: Serialize,
        for <'de> <T as lsp_types::request::Request>::Result: serde::Deserialize<'de> + core::fmt::Debug,
    {
        let mut transmitter = self.lock()?;
        let id = transmitter.id;

        transmitter.send_message(Content::Request(Call::MethodCall(MethodCall {
            jsonrpc: Some(Version::V2),
            method: request.method().clone(),
            params: request.params()?,
            id: Id::Num(id),
        })))?;

        loop {
            let Message::Response{id, outcome} = receiver.recv()?;

            if transmitter.id == id {
                transmitter.id = transmitter.id.wrapping_add(1);

                let Outcome::Success(value) = outcome;
                return serde_json::from_value::<T::Result>(value).map_err(Fault::from);
            }
        }
    }

    fn lock(&self) -> Result<MutexGuard<'_, AtomicTransmitter>, Fault> {
        self.0.lock().map_err(|_| Fault::Mutex)
    }
}

/// Transmits messages to the language server process.
#[derive(Debug)]
struct AtomicTransmitter {
    /// The input of the language server process.
    stdin: ChildStdin,
    /// Current request id.
    id: u64,
}

impl AtomicTransmitter {
    /// Sends `content` as a LSP message.
    fn send_message(&mut self, content: Content) -> Result<(), Fault> {
        trace!("Sending: {:?}", content);
        write!(self.stdin, "{}", Msg(content)).map_err(Fault::LanguageServerWrite)?;
        Ok(())
    }
}

/// Represents a LSP message.
struct Msg(Content);

impl fmt::Display for Msg {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let content = serde_json::to_string(&self.0).map_err(|_| fmt::Error)?;
        write!(f, "Content-Length: {}\r\n\r\n{}", content.len(), content)
    }
}

/// Represents the content of a lsp message.
#[derive(Debug, Serialize)]
#[serde(untagged)]
enum Content {
    /// A request or notification.
    Request(Call),
    /// A response.
    Response(Output),
}

/// Represents a response.
struct Response {
    /// Id of response.
    id: Id,
}

impl Response {
    /// Returns id of response.
    const fn id(&self) -> &Id {
        &self.id
    }
}

/// Represents a request.
struct Request<T>
where
    T: lsp_types::request::Request,
{
    /// Method of request.
    method: String,
    /// Params of request.
    params: T::Params,
}

impl<T> Request<T> 
where
    T: lsp_types::request::Request,
    <T as lsp_types::request::Request>::Params: Serialize,
{
    /// Returns method of `self`.
    fn method(&self) -> &String {
        &self.method
    }

    /// Returns [`Params`] of `self`.
    fn params(&self) -> Result<Params, Fault> {
        to_params(serde_json::to_value(&self.params)?)
    }
}

impl From<InitializeParams> for Request<lsp_types::request::Initialize> {
    fn from(value: InitializeParams) -> Self {
        Self {
            method: "initialize".to_string(),
            params: value,
        }
    }
}

impl From<ShutdownParams> for Request<lsp_types::request::Shutdown> {
    fn from(_: ShutdownParams) -> Self {
        Self {
            method: "shutdown".to_string(),
            params: (),
        }
    }
}

/// Represents a notification.
struct Notification {
    /// Method of the notification.
    method: String,
    /// Params of the notification.
    params: NotificationParams,
}

impl Notification {
    /// Returns the method of `self`.
    const fn method(&self) -> &String {
        &self.method
    }

    /// Returns the [`Params`] of `self`.
    fn params(&self) -> Result<Params, Fault> {
        to_params(serde_json::to_value(self.params.clone())?)
    }
}

/// Converts `value` to a [`Params`].
fn to_params(value: Value) -> Result<Params, Fault> {
    match value {
        Value::Object(object) => Ok(Params::Map(object)),
        Value::Null => Ok(Params::None),
        Value::Bool(..) | Value::Number(..) | Value::String(..) | Value::Array(..) => {
            Err(Fault::InvalidParams)
        }
    }
}

impl From<InitializedParams> for Notification {
    fn from(value: InitializedParams) -> Self {
        Self {
            method: "initialized".to_string(),
            params: NotificationParams::Initialized(value),
        }
    }
}

impl From<ExitParams> for Notification {
    fn from(value: ExitParams) -> Self {
        Self {
            method: "exit".to_string(),
            params: NotificationParams::Exit(value),
        }
    }
}

impl From<DidOpenTextDocumentParams> for Notification {
    fn from(value: DidOpenTextDocumentParams) -> Self {
        Self {
            method: "textDocument/didOpen".to_string(),
            params: NotificationParams::DidOpen(value),
        }
    }
}

/// Parameters for notifications.
#[derive(Clone, Serialize)]
#[serde(untagged)]
enum NotificationParams {
    /// The parameters for the Initialized notification.
    Initialized(InitializedParams),
    /// The parameters for the Exit notification.
    Exit(ExitParams),
    DidOpen(DidOpenTextDocumentParams),
}

/// Params field for the exit notification.
// lsp_types does not define a Params for "exit".
#[derive(Clone, Copy, Serialize)]
struct ExitParams {}

/// Params field for the shutdown request.
// lsp_types does not define a Params for "shutdown".
#[derive(Serialize)]
struct ShutdownParams(());

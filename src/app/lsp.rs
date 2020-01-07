//! Implements management and use of language servers.
use {
    crate::Failure,
    jsonrpc_core::{Call, Id, MethodCall, Output, Params, Success, Value, Version},
    log::{error, trace, warn},
    lsp_types::{ClientCapabilities, InitializeParams, InitializedParams, Url},
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
    transmitter: Arc<Mutex<LspTransmitter>>,
    /// Reads data from the language server process.
    reader: BufReader<ChildStdout>,
    /// Sends data to the [`LspServer`].
    response_tx: Sender<u64>,
    /// Signifies if the thread is quitting.
    is_quitting: bool,
}

impl LspProcessor {
    /// Creates a new `LspProcessor`.
    fn new(
        process: &mut Child,
        response_tx: Sender<u64>,
        transmitter: Arc<Mutex<LspTransmitter>>,
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
                                        if let Some(_result) = message.get("result") {
                                            if let Some(id) = message.get("id") {
                                                if let Ok(response_id) =
                                                    serde_json::from_value(id.clone())
                                                {
                                                    self.response_tx.send(response_id).map_err(
                                                        |_| {
                                                            Fault::DisconnectedReceiver(
                                                                "response".to_string(),
                                                            )
                                                        },
                                                    )?;
                                                }
                                            }
                                        } else if let Some(id) = message.get("id") {
                                            if let Ok(message_id) =
                                                serde_json::from_value::<u64>(id.clone())
                                            {
                                                self.transmitter
                                                    .lock()
                                                    .map_err(|_| Fault::Mutex)?
                                                    .respond(&Response {
                                                        id: Id::Num(message_id),
                                                    })?;
                                            }
                                        } else {
                                            // Do nothing for now.
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

/// Represents a language server process.
#[derive(Debug)]
pub(crate) struct LspServer {
    /// The language server process.
    process: Child,
    /// Transmits messages to the language server process.
    transmitter: Arc<Mutex<LspTransmitter>>,
    /// The [`Sender`] to stderr processing.
    stderr_tx: Sender<()>,
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
        let (response_tx, response_rx) = mpsc::channel();
        let transmitter = Arc::new(Mutex::new(LspTransmitter::new(&mut process, response_rx)?));
        let mut processor = LspProcessor::new(&mut process, response_tx, Arc::clone(&transmitter))?;

        let _ = thread::spawn(move || {
            if let Err(error) = processor.process() {
                error!("Error in LspProcessor: {}", error);
            }
        });

        let stderr = process
            .stderr
            .take()
            .ok_or_else(|| Fault::UnaccessableIo("stderr".to_string()))?;
        let (stderr_tx, stderr_rx) = mpsc::channel();
        let _ = thread::spawn(move || {
            let mut reader = BufReader::new(stderr);
            let mut line = String::new();

            while stderr_rx.try_recv().is_err() {
                // Rust's language server (rls) seems to send empty lines over stderr after shutdown request so skip those.
                if reader.read_line(&mut line).is_ok() && !line.is_empty() {
                    error!("{}", line);
                    line.clear();
                }
            }
        });

        Ok(Self {
            process,
            transmitter,
            stderr_tx,
        })
    }

    /// Initializes the `LspServer`.
    pub(crate) fn initialize(&mut self) -> Result<(), Fault> {
        #[allow(deprecated)] // root_path is a required field.
        self.transmitter()?.request(
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
        )?;

        self.transmitter()?.notify(&InitializedParams {}.into())
    }

    /// Attempts to cleanly kill the language server process.
    fn shutdown_and_exit(&mut self) -> Result<(), Failure> {
        self.transmitter()?.request(&ShutdownParams(()).into())?;
        self.terminate_stderr_thread()?;
        self.transmitter()?.notify(&ExitParams {}.into())?;

        if let Err(e) = self.process.wait() {
            warn!("Unable to wait on language server process exit: {}", e);
        }

        Ok(())
    }

    /// Terminates the thread processing stderr output.
    fn terminate_stderr_thread(&self) -> Result<(), Fault> {
        self.stderr_tx
            .send(())
            .map_err(|_| Fault::DisconnectedReceiver("stderr".to_string()))
    }

    /// Returns the transmitter, blocking until it is available.
    fn transmitter(&mut self) -> Result<MutexGuard<'_, LspTransmitter>, Fault> {
        self.transmitter.lock().map_err(|_| Fault::Mutex)
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

/// Transmits messages to the language server process.
#[derive(Debug)]
struct LspTransmitter {
    /// The input of the language server process.
    stdin: ChildStdin,
    /// Current request id.
    id: u64,
    /// Receives responses from the language server process.
    response_rx: Receiver<u64>,
}

impl LspTransmitter {
    /// Creates a new `LspTransmitter`.
    fn new(process: &mut Child, response_rx: Receiver<u64>) -> Result<Self, Fault> {
        Ok(Self {
            id: 0,
            stdin: process
                .stdin
                .take()
                .ok_or_else(|| Fault::UnaccessableIo("stdin".to_string()))?,
            response_rx,
        })
    }

    /// Waits to receive a response id.
    fn recv_response(&self) -> Result<u64, Fault> {
        self.response_rx
            .recv()
            .map_err(|_| Fault::DisconnectedSender("response".to_string()))
    }

    /// Sends `response` to the lsp server.
    fn respond(&mut self, response: &Response) -> Result<(), Fault> {
        self.send_message(Content::Response(Output::Success(Success {
            jsonrpc: Some(Version::V2),
            result: Value::Null,
            id: response.id().clone(),
        })))
    }

    /// Sends `request` to the lsp server and waits for the response.
    fn request(&mut self, request: &Request) -> Result<(), Fault> {
        self.send_message(Content::Request(Call::MethodCall(MethodCall {
            jsonrpc: Some(Version::V2),
            method: request.method().clone(),
            params: request.params()?,
            id: Id::Num(self.id),
        })))?;

        while self.recv_response()? != self.id {}

        self.id = self.id.wrapping_add(1);
        Ok(())
    }

    /// Sends `notification` to the lsp server.
    fn notify(&mut self, notification: &Notification) -> Result<(), Fault> {
        self.send_message(Content::Request(Call::Notification(
            jsonrpc_core::Notification {
                jsonrpc: Some(Version::V2),
                method: notification.method().clone(),
                params: notification.params()?,
            },
        )))
    }

    /// Sends `content` as a LSP message.
    fn send_message(&mut self, content: Content) -> Result<(), Fault> {
        trace!("Sending: {:?}", content);
        write!(self.stdin, "{}", Message(content)).map_err(Fault::LanguageServerWrite)?;
        Ok(())
    }
}

/// Represents a LSP message.
struct Message(Content);

impl fmt::Display for Message {
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
struct Request {
    /// Method of request.
    method: String,
    /// Params of request.
    params: RequestParams,
}

impl Request {
    /// Returns method of `self`.
    const fn method(&self) -> &String {
        &self.method
    }

    /// Returns [`Params`] of `self`.
    fn params(&self) -> Result<Params, Fault> {
        to_params(serde_json::to_value(&self.params)?)
    }
}

impl From<InitializeParams> for Request {
    fn from(value: InitializeParams) -> Self {
        Self {
            method: "initialize".to_string(),
            params: RequestParams::Initialize(value),
        }
    }
}

impl From<ShutdownParams> for Request {
    fn from(value: ShutdownParams) -> Self {
        Self {
            method: "shutdown".to_string(),
            params: RequestParams::Shutdown(value),
        }
    }
}

/// Parameters for a request.
#[derive(Serialize)]
#[serde(untagged)]
enum RequestParams {
    /// Parameters for the initialize request.
    Initialize(InitializeParams),
    /// Parameters for the shutdown request.
    Shutdown(ShutdownParams),
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
        to_params(serde_json::to_value(self.params)?)
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

/// Parameters for notifications.
#[derive(Clone, Copy, Serialize)]
#[serde(untagged)]
enum NotificationParams {
    /// The parameters for the Initialized notification.
    Initialized(InitializedParams),
    /// The parameters for the Exit notification.
    Exit(ExitParams),
}

/// Params field for the exit notification.
// lsp_types does not define a Params for "exit".
#[derive(Clone, Copy, Serialize)]
struct ExitParams {}

/// Params field for the shutdown request.
// lsp_types does not define a Params for "shutdown".
#[derive(Serialize)]
struct ShutdownParams(());

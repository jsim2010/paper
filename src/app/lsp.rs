//! Implements management and use of language servers.
use {
    crate::Failure,
    displaydoc::Display as DisplayDoc,
    jsonrpc_core::{Call, Id, MethodCall, Output, Params, Response, Success, Value, Version},
    log::{error, trace, warn},
    lsp_types::{ClientCapabilities, InitializeParams, InitializedParams, Url},
    serde::Serialize,
    serde_json::error::Error as SerdeJsonError,
    std::{
        env,
        io::{self, BufRead, BufReader, ErrorKind, Read, Write},
        process::{self, Child, ChildStdin, ChildStdout, Command, Stdio},
        sync::{
            mpsc::{self, Receiver, RecvError, SendError, Sender},
            Arc, Mutex,
        },
        thread,
    },
};

/// An LSP Error.
#[derive(Debug, DisplayDoc)]
pub enum Error {
    /// send error `{0}`
    Send(SendError<u64>),
    /// send error `{0}`
    Send2(SendError<()>),
    /// receive error `{0}`
    Receive(RecvError),
    /// unable to access stdout or stderr of language server
    InvalidIo,
    /// unable to process request params
    InvalidRequestParams,
    /// io error `{0}`
    Io(io::Error),
    /// mutex error
    Mutex,
    /// serde json `{0}`
    SerdeJson(SerdeJsonError),
}

impl From<SendError<u64>> for Error {
    fn from(value: SendError<u64>) -> Self {
        Self::Send(value)
    }
}

impl From<SendError<()>> for Error {
    fn from(value: SendError<()>) -> Self {
        Self::Send2(value)
    }
}

impl From<RecvError> for Error {
    fn from(value: RecvError) -> Self {
        Self::Receive(value)
    }
}

impl From<io::Error> for Error {
    fn from(value: io::Error) -> Self {
        Self::Io(value)
    }
}

impl From<SerdeJsonError> for Error {
    fn from(value: SerdeJsonError) -> Self {
        Self::SerdeJson(value)
    }
}

/// Processes data from the language server.
struct LspProcessor {
    /// Transmits data to the language server process.
    transmitter: LspTransmitter,
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
        transmitter: LspTransmitter,
    ) -> Result<Self, Failure> {
        process
            .stdout
            .take()
            .ok_or_else(|| Error::InvalidIo.into())
            .map(|stdout| Self {
                reader: BufReader::new(stdout),
                response_tx,
                is_quitting: false,
                transmitter,
            })
    }

    /// Processes data from the language server.
    fn process(&mut self) -> Result<(), Error> {
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
                                                    self.response_tx.send(response_id)?;
                                                }
                                            }
                                        } else if let Some(id) = message.get("id") {
                                            if let Ok(message_id) =
                                                serde_json::from_value::<u64>(id.clone())
                                            {
                                                self.transmitter.send_response(
                                                    &Response::Single(Output::Success(Success {
                                                        jsonrpc: Some(Version::V2),
                                                        result: Value::Null,
                                                        id: Id::Num(message_id),
                                                    })),
                                                )?;
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
    /// Receives responses from the language server process.
    response_rx: Receiver<u64>,
    /// Transmits messages to the language server process.
    transmitter: LspTransmitter,
    /// The [`Sender`] to stderr processing.
    stderr_tx: Sender<()>,
}

impl LspServer {
    /// Creates a new `LspServer` represented by `process_cmd`.
    pub(crate) fn new(process_cmd: &str) -> Result<Self, Failure> {
        let mut process = Command::new(process_cmd)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?;
        let (response_tx, response_rx) = mpsc::channel();
        let transmitter =
            LspTransmitter::new(process.stdin.take().ok_or_else(|| Error::InvalidIo)?);
        let mut processor = LspProcessor::new(&mut process, response_tx, transmitter.clone())?;

        let _ = thread::spawn(move || {
            if let Err(error) = processor.process() {
                error!("Error in LspProcessor: {}", error);
            }
        });

        let stderr = process.stderr.take().ok_or_else(|| Error::InvalidIo)?;
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
            response_rx,
            transmitter,
            stderr_tx,
        })
    }

    /// Initializes the `LspServer`.
    pub(crate) fn initialize(&mut self) -> Result<(), Failure> {
        #[allow(deprecated)] // root_path is a required field.
        self.request(&InitializeParams {
            process_id: Some(u64::from(process::id())),
            root_path: None,
            root_uri: Some(
                Url::from_directory_path(env::current_dir()?.as_path()).map_err(|_| {
                    Failure::File(io::Error::new(
                        ErrorKind::Other,
                        "cannot convert current_dir to url",
                    ))
                })?,
            ),
            initialization_options: None,
            capabilities: ClientCapabilities::default(),
            trace: None,
            workspace_folders: None,
            client_info: None,
        })?;

        self.notify(&InitializedParams {})?;

        Ok(())
    }

    /// Attempts to cleanly kill the language server process.
    fn shutdown_and_exit(&mut self) -> Result<(), Failure> {
        self.request(&ShutdownParams {})?;
        self.terminate_stderr_thread()?;
        self.notify(&ExitParams {})?;

        if let Err(e) = self.process.wait() {
            warn!("Unable to wait on language server process exit: {}", e);
        }

        Ok(())
    }

    /// Terminates the thread processing stderr output.
    fn terminate_stderr_thread(&self) -> Result<(), Error> {
        self.stderr_tx.send(()).map_err(Error::from)
    }

    /// Sends a request with `params` to the language server process and waits for a response.
    fn request(&mut self, params: &(impl Request + Serialize)) -> Result<(), Failure> {
        self.transmitter.send_request(params)?;

        while !self
            .transmitter
            .confirm_id(self.response_rx.recv().map_err(Error::from)?)
        {}

        Ok(())
    }

    /// Sends `notification` to the language server process.
    fn notify(&mut self, notification: &(impl Notification + Serialize)) -> Result<(), Failure> {
        self.transmitter.send_notification(notification)
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
#[derive(Clone, Debug)]
struct LspTransmitter {
    /// The input of the language server process.
    stdin: Arc<Mutex<ChildStdin>>,
    /// Current request id.
    id: u64,
}

impl LspTransmitter {
    /// Creates a new `LspTransmitter`.
    fn new(stdin: ChildStdin) -> Self {
        Self {
            id: 0,
            stdin: Arc::new(Mutex::new(stdin)),
        }
    }

    /// Returns if `id` matches the current request id.
    ///
    /// If it does match, then increment the next id by 1.
    fn confirm_id(&mut self, id: u64) -> bool {
        let result = id == self.id;

        if result {
            self.id = self.id.wrapping_add(1);
        }

        result
    }

    /// Sends `notification` to language server protocol.
    fn send_notification(
        &mut self,
        notification: &(impl Notification + Serialize),
    ) -> Result<(), Failure> {
        self.send_call(&Call::Notification(jsonrpc_core::Notification {
            jsonrpc: Some(Version::V2),
            method: notification.method(),
            params: notification.params()?,
        }))?;

        Ok(())
    }

    /// Sends `request` to language server protocol.
    fn send_request(&mut self, request: &(impl Request + Serialize)) -> Result<(), Failure> {
        self.send_call(&Call::MethodCall(MethodCall {
            jsonrpc: Some(Version::V2),
            method: request.method(),
            params: request.params()?,
            id: Id::Num(self.id),
        }))?;

        Ok(())
    }

    /// Sends `call` to the language server process.
    fn send_call(&mut self, call: &Call) -> Result<(), Error> {
        self.send_string(&serde_json::to_string(call)?)
    }

    /// Sends `response` to the language server process.
    fn send_response(&mut self, response: &Response) -> Result<(), Error> {
        self.send_string(&serde_json::to_string(response)?)
    }

    /// Sends `s` to language server process.
    fn send_string(&mut self, s: &str) -> Result<(), Error> {
        trace!("Sending: {}", s);

        write!(
            self.stdin.lock().map_err(|_| Error::Mutex)?,
            "Content-Length: {}\r\n\r\n{}",
            s.len(),
            s
        )?;
        Ok(())
    }
}

/// Parameters of notification and request messsages.
trait Callable {
    /// The parameters of the message.
    fn params(&self) -> Result<Params, Failure>
    where
        Self: Serialize,
    {
        Ok(match serde_json::to_value(self)? {
            Value::Object(object) => Ok(Params::Map(object)),
            Value::Null => Ok(Params::None),
            Value::Bool(..) | Value::Number(..) | Value::String(..) | Value::Array(..) => {
                Err(Error::InvalidRequestParams)
            }
        }?)
    }

    /// The method of the message.
    fn method(&self) -> String;
}

/// Parameters of notification messages.
trait Notification: Callable {}

impl Notification for InitializedParams {}

impl Callable for InitializedParams {
    fn method(&self) -> String {
        "initialized".to_string()
    }
}

/// Params field for the exit notification.
// lsp_types does not define a Params for "exit".
#[derive(Serialize)]
struct ExitParams {}

impl Notification for ExitParams {}

impl Callable for ExitParams {
    fn method(&self) -> String {
        "exit".to_string()
    }
}

/// Parameters of request messages.
trait Request: Callable {}

impl Callable for InitializeParams {
    fn method(&self) -> String {
        "initialize".to_string()
    }
}

impl Request for InitializeParams {}

/// Params field for the shutdown request.
// lsp_types does not define a Params for "shutdown".
#[derive(Serialize)]
struct ShutdownParams {}

impl Callable for ShutdownParams {
    fn method(&self) -> String {
        "shutdown".to_string()
    }
}

impl Request for ShutdownParams {}

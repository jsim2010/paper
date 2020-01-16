//! Implements management and use of language servers.
use {
    jsonrpc_core::{Id, Value, Version},
    log::{error, trace, warn},
    lsp_types::{
        notification::{DidCloseTextDocument, DidOpenTextDocument, Exit, Initialized},
        request::{Initialize, RegisterCapability, Shutdown},
        ClientCapabilities, DidCloseTextDocumentParams, DidOpenTextDocumentParams,
        InitializeParams, InitializeResult, InitializedParams, TextDocumentIdentifier,
        TextDocumentItem, TextDocumentSyncCapability, TextDocumentSyncKind, Url,
    },
    serde::{
        ser::SerializeStruct,
        {Deserialize, Serialize, Serializer},
    },
    serde_json::error::Error as SerdeJsonError,
    std::{
        fmt,
        io::{self, BufRead, BufReader, Read, Write},
        process::{self, Child, ChildStderr, ChildStdin, ChildStdout, Command, Stdio},
        string::FromUtf8Error,
        sync::{
            mpsc::{self, Receiver, Sender},
            Arc, Mutex, MutexGuard,
        },
        thread,
    },
    thiserror::Error,
};

/// The header field name that maps to the length of the content.
static HEADER_CONTENT_LENGTH: &str = "Content-Length";

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
    /// Serde json error.
    #[error("serde json: {0}")]
    SerdeJson(#[from] SerdeJsonError),
    /// Could not wait for language server process.
    #[error("unable to wait for language server process exit: {0}")]
    ProcessWait(#[source] io::Error),
    /// Could not kill language server process.
    #[error("unable to kill language server process: {0}")]
    ProcessKill(#[source] io::Error),
    /// Error reading server output.
    #[error("unable to read output of language server process: {0}")]
    ServerRead(#[source] io::Error),
    /// That language server output data that was not UTF-8.
    #[error("output of language server process was not UTF-8: {0}")]
    UTF8(#[from] FromUtf8Error),
}

/// Represents a language server process.
#[derive(Debug)]
pub(crate) struct LspServer {
    /// The language server process.
    server: ServerProcess,
    /// Transmits messages to the language server process.
    transmitter: LspTransmitter,
    /// Processes output from the stderr of the language server.
    error_processor: LspErrorProcessor,
    /// Controls settings for the language server.
    settings: LspSettings,
    /// Receives messages from the language server.
    receiver: LspReceiver,
}

impl LspServer {
    /// Creates a new `LspServer` represented by `process_cmd`.
    pub(crate) fn new(process_cmd: &str, root: &Url) -> Result<Self, Fault> {
        let mut server = ServerProcess::new(process_cmd)?;
        let mut transmitter = LspTransmitter::new(server.stdin()?);
        let receiver = LspReceiver::new(server.stdout()?, &transmitter);

        #[allow(deprecated)] // root_path is a required field.
        let settings = LspSettings::from(transmitter.request::<Initialize>(
            InitializeParams {
                process_id: Some(u64::from(process::id())),
                root_path: None,
                root_uri: Some(root.clone()),
                initialization_options: None,
                capabilities: ClientCapabilities::default(),
                trace: None,
                workspace_folders: None,
                client_info: None,
            },
            &receiver,
        )?);

        transmitter.notify::<Initialized>(InitializedParams {})?;

        Ok(Self {
            // error_processor must be created before server is moved.
            error_processor: LspErrorProcessor::new(server.stderr()?),
            server,
            transmitter,
            settings,
            receiver,
        })
    }

    /// Sends the didOpen notification, if appropriate.
    pub(crate) fn did_open(&mut self, text_document: &TextDocumentItem) -> Result<(), Fault> {
        if self.settings.notify_open_close {
            self.transmitter
                .notify::<DidOpenTextDocument>(DidOpenTextDocumentParams {
                    text_document: text_document.clone(),
                })?;
        }

        Ok(())
    }

    /// Sends the didClose notification, if appropriate.
    pub(crate) fn did_close(&mut self, text_document: &TextDocumentItem) -> Result<(), Fault> {
        if self.settings.notify_open_close {
            self.transmitter
                .notify::<DidCloseTextDocument>(DidCloseTextDocumentParams {
                    text_document: TextDocumentIdentifier::new(text_document.uri.clone()),
                })?;
        }

        Ok(())
    }

    /// Attempts to cleanly kill the language server process.
    fn shutdown_and_exit(&mut self) -> Result<(), Fault> {
        self.transmitter.request::<Shutdown>((), &self.receiver)?;
        self.error_processor.terminate()?;
        self.transmitter.notify::<Exit>(())?;
        self.server.wait()
    }
}

impl Drop for LspServer {
    fn drop(&mut self) {
        if let Err(e) = self.shutdown_and_exit() {
            warn!("Unable to cleanly shutdown and exit language server: {}", e);

            if let Err(kill_error) = self.server.kill() {
                warn!("{}", kill_error);
            }
        }
    }
}

/// Signifies a language server process.
#[derive(Debug)]
struct ServerProcess(Child);

impl ServerProcess {
    /// Creates a new [`ServerProcess`].
    fn new(process_cmd: &str) -> Result<Self, Fault> {
        Ok(Self(
            Command::new(process_cmd)
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()
                .map_err(|_| Fault::InvalidCommand)?,
        ))
    }

    /// Returns the stderr of the process.
    fn stderr(&mut self) -> Result<ChildStderr, Fault> {
        self.0
            .stderr
            .take()
            .ok_or_else(|| Fault::UnaccessableIo("stderr".to_string()))
    }

    /// Returns the stdin of the process.
    fn stdin(&mut self) -> Result<ChildStdin, Fault> {
        self.0
            .stdin
            .take()
            .ok_or_else(|| Fault::UnaccessableIo("stdin".to_string()))
    }

    /// Returns the stdout of the process.
    fn stdout(&mut self) -> Result<ChildStdout, Fault> {
        self.0
            .stdout
            .take()
            .ok_or_else(|| Fault::UnaccessableIo("stdout".to_string()))
    }

    /// Kills the process.
    fn kill(&mut self) -> Result<(), Fault> {
        self.0.kill().map_err(Fault::ProcessKill)
    }

    /// Blocks until the proccess ends.
    fn wait(&mut self) -> Result<(), Fault> {
        self.0.wait().map(|_| ()).map_err(Fault::ProcessWait)
    }
}

/// Sends messages to the language server process.
#[derive(Clone, Debug)]
struct LspTransmitter(Arc<Mutex<AtomicTransmitter>>);

impl LspTransmitter {
    /// Creates a new `LspTransmitter`.
    fn new(stdin: ChildStdin) -> Self {
        Self(Arc::new(Mutex::new(AtomicTransmitter { id: 0, stdin })))
    }

    /// Sends a notification with `params`.
    fn notify<T: lsp_types::notification::Notification>(
        &mut self,
        params: T::Params,
    ) -> Result<(), Fault>
    where
        T::Params: Serialize,
    {
        self.lock()?.send(&Message::notification::<T>(params)?)
    }

    /// Sends a response with `id` and `result`.
    fn respond<T: lsp_types::request::Request>(
        &mut self,
        id: u64,
        result: T::Result,
    ) -> Result<(), Fault>
    where
        T::Result: Serialize,
    {
        self.lock()?.send(&Message::response::<T>(id, result)?)
    }

    /// Sends `request` to the lsp server and waits for the response.
    #[allow(single_use_lifetimes)] // 'de is needed to compile.
    fn request<T: lsp_types::request::Request>(
        &mut self,
        params: T::Params,
        receiver: &LspReceiver,
    ) -> Result<T::Result, Fault>
    where
        T::Params: Serialize,
        for<'de> T::Result: Deserialize<'de>,
    {
        let mut transmitter = self.lock()?;
        let current_id = transmitter.id;
        transmitter.send(&Message::request::<T>(current_id, params)?)?;

        let response: T::Result;

        loop {
            if let Message::Response {
                id,
                outcome: Outcome::Success(value),
            } = receiver.recv()?
            {
                if transmitter.id == id {
                    transmitter.id = transmitter.id.wrapping_add(1);
                    response = serde_json::from_value(value)?;
                    break;
                }
            }
        }

        Ok(response)
    }

    /// Locks the [`AtomicTransmitter`] to prevent race conditions.
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
    /// Sends `message` to the language server process.
    fn send(&mut self, message: &Message) -> Result<(), Fault> {
        trace!("sending: {:?}", message);
        write!(self.stdin, "{}", message.to_protocol()?).map_err(Fault::LanguageServerWrite)
    }
}

/// Signifies the receiver of LSP messages.
#[derive(Debug)]
struct LspReceiver(Receiver<Message>);

impl LspReceiver {
    /// Creates a new [`LspReceiver`].
    fn new(stdout: ChildStdout, transmitter: &LspTransmitter) -> Self {
        let (tx, rx) = mpsc::channel();
        let mut processor = LspProcessor::new(stdout, tx, transmitter.clone());

        let _ = thread::spawn(move || {
            if let Err(error) = processor.process() {
                error!("processing language server output: {}", error);
            }
        });

        Self(rx)
    }

    /// Receives a [`Message`] from that was read by [`LspProcessor`].
    fn recv(&self) -> Result<Message, Fault> {
        self.0
            .recv()
            .map_err(|_| Fault::DisconnectedSender("language server stdout".to_string()))
    }
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
    fn new(stdout: ChildStdout, response_tx: Sender<Message>, transmitter: LspTransmitter) -> Self {
        Self {
            reader: BufReader::new(stdout),
            response_tx,
            is_quitting: false,
            transmitter,
        }
    }

    /// Processes data from the language server.
    fn process(&mut self) -> Result<(), Fault> {
        while !self.is_quitting {
            if let Some(message) = self.read_message()? {
                trace!("received: {:?}", message);

                match message {
                    Message::Response { .. } => self
                        .response_tx
                        .send(message)
                        .map_err(|_| Fault::DisconnectedReceiver("response".to_string()))?,
                    Message::Request(MessageRequest { id, .. }) => {
                        self.transmitter.respond::<RegisterCapability>(id, ())?
                    }
                    Message::Notification { .. } => {}
                }
            }
        }

        Ok(())
    }

    /// Reads a message.
    fn read_message(&mut self) -> Result<Option<Message>, Fault> {
        let length = self.read_header()?;
        let content = self.read_content(length)?;

        let value = serde_json::from_str::<Value>(&content)?;

        Ok(
            if let Some(id) = value
                .get("id")
                .and_then(|id_value| serde_json::from_value(id_value.to_owned()).ok())
            {
                if let Some(result) = value.get("result") {
                    // Success response
                    Some(Message::Response {
                        id,
                        outcome: Outcome::Success(result.to_owned()),
                    })
                } else if value.get("error").is_some() {
                    // Error response
                    None
                } else if let Some(method) = value
                    .get("method")
                    .and_then(|method_value| serde_json::from_value(method_value.to_owned()).ok())
                {
                    if let Some(params) = value.get("params").and_then(|params_value| {
                        serde_json::from_value(params_value.to_owned()).ok()
                    }) {
                        // Request
                        Some(Message::Request(MessageRequest { id, method, params }))
                    } else {
                        // Invalid
                        None
                    }
                } else {
                    // Invalid
                    None
                }
            } else {
                // Notification
                None
            },
        )
    }

    /// Reads through the header of a message and returns the length of the content.
    fn read_header(&mut self) -> Result<usize, Fault> {
        let mut length = None;

        loop {
            let mut line = String::new();
            let _ = self
                .reader
                .read_line(&mut line)
                .map_err(Fault::ServerRead)?;

            if line == "\r\n" {
                if let Some(len) = length {
                    return Ok(len);
                }
            } else {
                let mut split = line.trim().split(": ");

                if split.next() == Some(HEADER_CONTENT_LENGTH) {
                    length = split.next().and_then(|s| s.parse().ok());
                }
            }
        }
    }

    /// Reads the content of a message with known `length`.
    fn read_content(&mut self, length: usize) -> Result<String, Fault> {
        let mut content = vec![0; length];

        self.reader
            .read_exact(&mut content)
            .map_err(Fault::ServerRead)?;
        Ok(String::from_utf8(content)?)
    }
}

impl Drop for LspProcessor {
    fn drop(&mut self) {
        self.is_quitting = true;
    }
}

/// Signifies an LSP message.
#[allow(dead_code)] // Bug in lint thinks Notification is never constructed.
enum Message {
    /// A notification.
    Notification {
        /// The method of the notification.
        method: &'static str,
        /// The parameters of the notification.
        params: Value,
    },
    /// A request for information.
    Request(MessageRequest),
    /// A response to a request.
    Response {
        /// The id that matches with the corresponding request.
        id: u64,
        /// The outcome of the response.
        outcome: Outcome,
    },
}

impl Message {
    /// Creates a notification message.
    fn notification<T: lsp_types::notification::Notification>(
        params: T::Params,
    ) -> Result<Self, Fault>
    where
        <T as lsp_types::notification::Notification>::Params: Serialize,
    {
        Ok(Self::Notification {
            method: T::METHOD,
            params: serde_json::to_value(params)?,
        })
    }

    /// Creates a request message.
    fn request<T: lsp_types::request::Request>(id: u64, params: T::Params) -> Result<Self, Fault>
    where
        <T as lsp_types::request::Request>::Params: Serialize,
    {
        Ok(Self::Request(MessageRequest {
            id,
            method: T::METHOD.to_string(),
            params: serde_json::to_value(params)?,
        }))
    }

    /// Creates a response message.
    fn response<T: lsp_types::request::Request>(id: u64, result: T::Result) -> Result<Self, Fault>
    where
        <T as lsp_types::request::Request>::Result: Serialize,
    {
        Ok(Self::Response {
            id,
            outcome: Outcome::Success(serde_json::to_value(result)?),
        })
    }

    /// Returns `self` in its raw format.
    fn to_protocol(&self) -> Result<String, Fault> {
        let content = serde_json::to_string(&self)?;

        Ok(format!(
            "{}: {}\r\n\r\n{}",
            HEADER_CONTENT_LENGTH,
            content.len(),
            content
        ))
    }
}

impl fmt::Debug for Message {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Request(request) => write!(f, "Request {{ {:?} }}", request),
            Self::Notification { method, params } => write!(
                f,
                "Notification {{ method: {:?}, params: {:?} }}",
                method, params
            ),
            Self::Response { id, outcome } => {
                write!(f, "Response {{ id: {:?}, outcome: {:?} }}", id, outcome)
            }
        }
    }
}

impl Serialize for Message {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut state = serializer.serialize_struct(
            "Content",
            match self {
                Self::Notification { .. } | Self::Response { .. } => 3,
                Self::Request(..) => 4,
            },
        )?;

        state.serialize_field("jsonrpc", &Some(Version::V2))?;

        match self {
            Self::Notification { method, params } => {
                state.serialize_field("method", method)?;
                state.serialize_field("params", params)?;
            }
            Self::Request(MessageRequest { id, method, params }) => {
                state.serialize_field("id", &Id::Num(*id))?;
                state.serialize_field("method", method)?;
                state.serialize_field("params", params)?;
            }
            Self::Response { id, .. } => {
                state.serialize_field("id", &Id::Num(*id))?;
                state.serialize_field("result", &Value::Null)?;
            }
        }

        state.end()
    }
}

impl fmt::Display for Message {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let content = serde_json::to_string(&self).map_err(|_| fmt::Error)?;

        write!(f, "Content-Length: {}\r\n\r\n{}", content.len(), content)
    }
}

/// Signifies an LSP Request Message.
struct MessageRequest {
    /// An identifier of a request.
    id: u64,
    /// The method of the request.
    method: String,
    /// The parameters of the request.
    params: Value,
}

/// Implemented so prevent the repetition of the inner class name.
impl fmt::Debug for MessageRequest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "id: {:?}, method: {:?}, params: {:?}",
            self.id, self.method, self.params
        )
    }
}

/// The outcome of the response.
#[derive(Debug)]
enum Outcome {
    /// The result was successful.
    Success(Value),
}

/// Processes output from stderr.
#[derive(Debug)]
struct LspErrorProcessor(Sender<()>);

impl LspErrorProcessor {
    /// Creates a new [`LspErrorProcessor`].
    fn new(stderr: ChildStderr) -> Self {
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

        Self(tx)
    }

    /// Terminates the error processor thread.
    fn terminate(&self) -> Result<(), Fault> {
        self.0
            .send(())
            .map_err(|_| Fault::DisconnectedReceiver("language server stderr".to_string()))
    }
}

/// Settings of the language server.
#[derive(Debug, Default)]
struct LspSettings {
    /// The client should send open and close notifications.
    notify_open_close: bool,
}

impl From<InitializeResult> for LspSettings {
    fn from(value: InitializeResult) -> Self {
        let mut settings = Self::default();

        if let Some(text_document_sync) = value.capabilities.text_document_sync {
            match text_document_sync {
                TextDocumentSyncCapability::Kind(kind) => {
                    if kind != TextDocumentSyncKind::None {
                        settings.notify_open_close = true;
                    }
                }
                TextDocumentSyncCapability::Options(options) => {
                    if let Some(open_close) = options.open_close {
                        settings.notify_open_close = open_close;
                    }
                }
            }
        }

        settings
    }
}

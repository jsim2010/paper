//! Implements language server utilities.
use {
    jsonrpc_core::{Id, Value, Version},
    log::{error, trace, warn},
    lsp_types::{notification::Notification, request::RegisterCapability},
    serde::{
        de::DeserializeOwned,
        ser::SerializeStruct,
        {Serialize, Serializer},
    },
    serde_json::error::Error as SerdeJsonError,
    std::{
        fmt,
        io::{self, BufRead, BufReader, Read, Write},
        process::{ChildStderr, ChildStdin, ChildStdout},
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

/// An error from which a language server utility was unable to recover.
#[derive(Debug, Error)]
pub enum Fault {
    /// An error while receiving data over a channel.
    #[error("unable to receive from {0} channel, sender disconnected")]
    Receive(String),
    /// An error while sending data over a channel.
    #[error("unable to send over {0} channel, receiver disconnected")]
    Send(String),
    /// An error while writing input to a language server process.
    #[error("unable to write to language server process: {0}")]
    Input(#[source] io::Error),
    /// An error while acquiring the mutex protecting the stdin of a language server process.
    #[error("unable to acquire mutex of language server stdin")]
    Mutex,
    /// An error while serializing a language server message.
    #[error("unable to serialize language server message: {0}")]
    Serialize(#[from] SerdeJsonError),
}

/// Signifies an LSP message.
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

/// The outcome of the response.
#[derive(Debug)]
enum Outcome {
    /// The result was successful.
    Success(Value),
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

/// Sends messages to the language server process.
#[derive(Clone, Debug)]
pub(crate) struct LspTransmitter(Arc<Mutex<AtomicTransmitter>>);

impl LspTransmitter {
    /// Creates a new `LspTransmitter`.
    pub(crate) fn new(stdin: ChildStdin) -> Self {
        Self(Arc::new(Mutex::new(AtomicTransmitter { id: 0, stdin })))
    }

    /// Sends a notification with `params`.
    pub(crate) fn notify<T: Notification>(&mut self, params: T::Params) -> Result<(), Fault>
    where
        T::Params: Serialize,
    {
        self.lock()?.send(&Message::Notification {
            method: T::METHOD,
            params: serde_json::to_value(params)?,
        })
    }

    /// Sends a response with `id` and `result`.
    pub(crate) fn respond<T: lsp_types::request::Request>(
        &mut self,
        id: u64,
        result: T::Result,
    ) -> Result<(), Fault>
    where
        T::Result: Serialize,
    {
        self.lock()?.send(&Message::Response {
            id,
            outcome: Outcome::Success(serde_json::to_value(result)?),
        })
    }

    /// Sends `request` to the lsp server and waits for the response.
    pub(crate) fn request<T: lsp_types::request::Request>(
        &mut self,
        params: T::Params,
        receiver: &LspReceiver,
    ) -> Result<T::Result, Fault>
    where
        T::Params: Serialize,
        T::Result: DeserializeOwned,
    {
        let mut transmitter = self.lock()?;
        let current_id = transmitter.id;

        transmitter.send(&Message::Request(MessageRequest {
            id: current_id,
            method: T::METHOD.to_string(),
            params: serde_json::to_value(params)?,
        }))?;

        let response: T::Result;

        loop {
            if let Message::Response {
                id,
                outcome: Outcome::Success(value),
            } = receiver.recv()?
            {
                if transmitter.id == id {
                    transmitter.id = transmitter.id.wrapping_add(1);
                    match serde_json::from_value(value.clone()) {
                        Ok(result) => {
                            response = result;
                            break;
                        }
                        Err(e) => {
                            warn!("Failed to convert `{}` to result: {}", value, e);
                        }
                    }
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
        write!(self.stdin, "{}", message.to_protocol()?).map_err(Fault::Input)
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
            if let Some(message) = self.read_message() {
                trace!("received: {:?}", message);

                match message {
                    Message::Response { .. } => self
                        .response_tx
                        .send(message)
                        .map_err(|_| Fault::Send("response message".to_string()))?,
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
    fn read_message(&mut self) -> Option<Message> {
        let length = self.read_header();

        if let Some(content) = self.read_content(length) {
            match serde_json::from_str::<Value>(&content) {
                Ok(value) => {
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
                        } else if let Some(method) = value.get("method").and_then(|method_value| {
                            serde_json::from_value(method_value.to_owned()).ok()
                        }) {
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
                    }
                }
                Err(e) => {
                    warn!("failed to convert `{}` to json value: {}", content, e);
                    None
                }
            }
        } else {
            None
        }
    }

    /// Finds the first valid message header and returns the length of the content.
    fn read_header(&mut self) -> usize {
        let mut length = None;

        loop {
            let mut line = String::new();

            if let Err(e) = self.reader.read_line(&mut line) {
                warn!("failed to read line from language server process: {}", e);
            }

            if line == "\r\n" {
                if let Some(len) = length {
                    return len;
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
    fn read_content(&mut self, length: usize) -> Option<String> {
        let mut content = vec![0; length];

        if let Err(e) = self.reader.read_exact(&mut content) {
            warn!("failed to read message content: {}", e);
        }

        match String::from_utf8(content) {
            Ok(s) => Some(s),
            Err(e) => {
                warn!("received message that is not valid UTF8: {}", e);
                None
            }
        }
    }
}

impl Drop for LspProcessor {
    fn drop(&mut self) {
        self.is_quitting = true;
    }
}

/// Signifies the receiver of LSP messages.
#[derive(Debug)]
pub(crate) struct LspReceiver(Receiver<Message>);

impl LspReceiver {
    /// Creates a new [`LspReceiver`].
    pub(crate) fn new(stdout: ChildStdout, transmitter: &LspTransmitter) -> Self {
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
            .map_err(|_| Fault::Receive("response message".to_string()))
    }
}

/// Processes output from stderr.
#[derive(Debug)]
pub(crate) struct LspErrorProcessor(Sender<()>);

impl LspErrorProcessor {
    /// Creates a new [`LspErrorProcessor`].
    pub(crate) fn new(stderr: ChildStderr) -> Self {
        let (tx, rx) = mpsc::channel();
        let _ = thread::spawn(move || {
            let mut reader = BufReader::new(stderr);
            let mut line = String::new();

            while rx.try_recv().is_err() {
                // Rust's language server (rls) seems to send empty lines over stderr after shutdown request so skip those.
                if reader.read_line(&mut line).is_ok() && !line.is_empty() {
                    error!("lsp stderr: {}", line);
                    line.clear();
                }
            }
        });

        Self(tx)
    }

    /// Terminates the error processor thread.
    pub(crate) fn terminate(&self) -> Result<(), Fault> {
        self.0
            .send(())
            .map_err(|_| Fault::Send("language server stderr".to_string()))
    }
}

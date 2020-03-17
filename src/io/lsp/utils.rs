//! Implements language server utilities.
use {
    jsonrpc_core::{Id, Value, Version},
    log::{error, trace},
    market::{Readable, Writable},
    serde::{Serialize, Deserialize},
    lsp_types::{request::Request, notification::Notification},
    serde_json::error::Error as SerdeJsonError,
    std::{
        num::ParseIntError,
        io::{self, Write, BufRead, BufReader},
        process::ChildStderr,
        sync::{
            mpsc::{self, TryRecvError, Sender},
        },
        str::Utf8Error,
        thread,
    },
    thiserror::Error,
};

/// The header field name that maps to the length of the content.
static HEADER_CONTENT_LENGTH: &str = "Content-Length";
static HEADER_END: &str = "\r\n\r\n";

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
    Input(#[from] io::Error),
    /// An error while acquiring the mutex protecting the stdin of a language server process.
    #[error("unable to acquire mutex of language server stdin")]
    AcquireLock(#[from] AcquireLockError),
    /// An error while serializing a language server message.
    #[error("unable to serialize language server message: {0}")]
    Serialize(#[from] SerdeJsonError),
    /// Failed to send message.
    #[error("{0}")]
    SendMessage(#[from] SendMessageError),
    #[error("")]
    ContentLengthNotFound,
    #[error("")]
    ContentLengthInvalid,
    #[error("")]
    BufferNotComplete,
    #[error("")]
    InvalidUtf8(#[from] Utf8Error),
    #[error("")]
    ContentLengthParse(#[from] ParseIntError),
}

/// Failed to send notification.
#[derive(Debug, Error)]
pub enum SendNotificationError {
    /// An error while acquiring the mutex protecting the stdin of a language server process.
    #[error("{0}")]
    AcquireLock(#[from] AcquireLockError),
    /// An error while serializing message parameters.
    #[error("failed to serialize notification parameters: {0}")]
    SerializeParameters(#[from] SerdeJsonError),
    /// An error while sending a message to the language server.
    #[error("{0}")]
    SendMessage(#[from] SendMessageError),
}

/// An error while acquiring the mutex protecting the stdin of the language server process.
#[derive(Clone, Copy, Debug, Error)]
#[error("lock on stdin of language server process is poisoned")]
pub struct AcquireLockError();

/// Failed to send message.
#[derive(Debug, Error)]
pub enum SendMessageError {
    /// Failed to serialize message.
    #[error("{0}")]
    Serialize(#[from] SerializeMessageError),
    /// Failed to send message.
    #[error("failed to send message to language server: {0}")]
    Io(#[from] io::Error),
}

/// Failed to serialize message.
#[derive(Debug, Error)]
#[error("failed to serialize message: {error}")]
pub struct SerializeMessageError {
    /// The error.
    #[from]
    error: SerdeJsonError,
}

/// Failed to request a response.
#[derive(Debug, Error)]
pub enum RequestResponseError {
    /// An error while acquiring the mutex protecting the stdin of a language server process.
    #[error("{0}")]
    AcquireLock(#[from] AcquireLockError),
    /// An error while serializing message parameters.
    #[error("failed to serialize request parameters: {0}")]
    SerializeParameters(#[from] SerdeJsonError),
    /// An error while sending a message to the language server.
    #[error("{0}")]
    Send(#[from] SendMessageError),
    /// Failed to receive a message.
    #[error("{0}")]
    Receive(#[from] TryRecvError),
    /// Write
    #[error("")]
    Write(#[from] io::Error),
}

#[derive(Debug, Deserialize, Serialize)]
pub struct Message {
    jsonrpc: Version,
    #[serde(flatten)]
    pub(crate) object: Object,
}

impl Message {
    fn new(object: Object) -> Self {
        Self {
            jsonrpc: Version::V2,
            object,
        }
    }

    pub(crate) fn request<T>(params: T::Params, id: u64) -> Result<Self, SerdeJsonError>
    where
        T: Request,
        <T as Request>::Params: Serialize,
    {
        Object::request::<T>(params, Id::Num(id)).map(|object| Self::new(object))
    }

    pub(crate) fn notification<T>(params: T::Params) -> Result<Self, SerdeJsonError>
    where
        T: Notification,
        <T as Notification>::Params: Serialize,
    {
        Object::notification::<T>(params).map(|object| Self::new(object))
    }

    pub(crate) fn response<T>(result: T::Result, id: u64) -> Result<Self, SerdeJsonError>
    where
        T: Request,
        <T as Request>::Result: Serialize,
    {
        Object::response::<T>(result, Id::Num(id)).map(|object| Self::new(object))
    }
}

impl Readable for Message {
    type Error = Fault;

    fn from_bytes(bytes: &[u8]) -> (usize, Result<Self, Self::Error>) {
        match std::str::from_utf8(bytes) {
            Ok(buffer) => {
                if let Some(header_length) = buffer.find(HEADER_END) {
                    let mut content_length: Result<usize, Self::Error> = Err(Self::Error::ContentLengthNotFound);
                    let header = &buffer[..header_length];
                    let content_start = header_length + HEADER_END.len();

                    for field in header.split("\r\n") {
                        let mut items = field.split(": ");

                        if items.next() == Some(HEADER_CONTENT_LENGTH) {
                            content_length = items.next().ok_or(Self::Error::ContentLengthNotFound).and_then(|value| Ok(value.parse()?))
                        }
                    }

                    match content_length {
                        Err(error) => {
                            (content_start, Err(error))
                        }
                        Ok(content_length) => {
                            let total_len = content_start + content_length;

                            if bytes.len() < total_len {
                                (0, Err(Self::Error::BufferNotComplete))
                            } else {
                                if let Some(content) = buffer.get(content_start..total_len) {
                                    (total_len, serde_json::from_str(content).map_err(Self::Error::Serialize))
                                } else {
                                    // Length of content was not valid. Skip over current header to restart checking for next header.
                                    (content_start, Err(Self::Error::ContentLengthInvalid))
                                }
                            }
                        }
                    }
                } else {
                    (0, Err(Self::Error::BufferNotComplete))
                }
            }
            Err(error) => (0, Err(Self::Error::InvalidUtf8(error))),
        }
    }
}

impl Writable for Message {
    type Error = Fault;

    fn write_to<W: Write>(&self, writer: &mut W) -> Result<(), Self::Error> {
        let content = serde_json::to_string(&self)?;
        trace!("write content: {}", content);
        Ok(write!(writer, "{}: {}\r\n\r\n{}", HEADER_CONTENT_LENGTH, content.len(), content)?)
    }
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(untagged)]
#[allow(dead_code)] // False positive.
pub(crate) enum Object {
    Request {
        // TODO: Convert this to &str.
        method: String,
        params: Value,
        #[serde(skip_serializing_if = "Option::is_none")]
        id: Option<Id>,
    },
    Response {
        #[serde(flatten)]
        outcome: Outcome,
        id: Id,
    },
}

impl Object {
    fn request<T>(params: T::Params, id: Id) -> Result<Self, SerdeJsonError>
    where
        T: Request,
        <T as Request>::Params: Serialize,
    {
        Ok(Self::Request {
            method: T::METHOD.to_string(),
            params: serde_json::to_value(params)?,
            id: Some(id),
        })
    }

    fn notification<T>(params: T::Params) -> Result<Self, SerdeJsonError>
    where
        T: Notification,
        <T as Notification>::Params: Serialize,
    {
        Ok(Self::Request {
            method: T::METHOD.to_string(),
            params: serde_json::to_value(params)?,
            id: None,
        })
    }

    fn response<T>(result: T::Result, id: Id) -> Result<Self, SerdeJsonError>
    where
        T: Request,
        <T as Request>::Result: Serialize,
    {
        Ok(Self::Response {
            outcome: Outcome::Result(serde_json::to_value(result)?),
            id,
        })
    }
}

/// The outcome of the response.
#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum Outcome {
    /// The result was successful.
    Result(Value),
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

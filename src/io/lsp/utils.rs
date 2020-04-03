//! Implements language server utilities.
use {
    jsonrpc_core::{Id, Value, Version},
    log::{error, trace},
    lsp_types::{notification::Notification, request::Request},
    market::{ComposeFrom, StripFrom},
    serde::{Deserialize, Serialize},
    serde_json::error::Error as SerdeJsonError,
    std::{
        io::{self, BufRead, BufReader},
        num::ParseIntError,
        process::ChildStderr,
        str::Utf8Error,
        sync::mpsc::{self, Sender, TryRecvError},
        thread,
    },
    thiserror::Error,
};

/// The header field name that maps to the length of the content.
static HEADER_CONTENT_LENGTH: &str = "Content-Length";
/// Indicates the end of the header
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
    /// Length of content not found.
    #[error("")]
    ContentLengthNotFound,
    /// Length of content is invalid.
    #[error("")]
    ContentLengthInvalid,
    /// Buffer is not complete
    #[error("")]
    BufferNotComplete,
    /// Invalid utf8.
    #[error("")]
    InvalidUtf8(#[from] Utf8Error),
    /// Content length was not parsed.
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

/// The content of an LSP message.
#[derive(Debug, Deserialize, Serialize)]
pub struct Message {
    /// The JSON version.
    jsonrpc: Version,
    /// The items included in the content.
    #[serde(flatten)]
    pub(crate) object: Object,
}

impl Message {
    /// Creates a new [`Message`].
    const fn new(object: Object) -> Self {
        Self {
            jsonrpc: Version::V2,
            object,
        }
    }

    /// Creates a request [`Message`].
    pub(crate) fn request<T>(params: T::Params, id: u64) -> Result<Self, SerdeJsonError>
    where
        T: Request,
        <T as Request>::Params: Serialize,
    {
        Object::request::<T>(params, Id::Num(id)).map(Self::new)
    }

    /// Creates a notification [`Message`].
    pub(crate) fn notification<T>(params: T::Params) -> Result<Self, SerdeJsonError>
    where
        T: Notification,
        <T as Notification>::Params: Serialize,
    {
        Object::notification::<T>(params).map(Self::new)
    }

    /// Creates a response [`Message`].
    pub(crate) fn response<T>(result: T::Result, id: Id) -> Result<Self, SerdeJsonError>
    where
        T: Request,
        <T as Request>::Result: Serialize,
    {
        Object::response::<T>(result, id).map(Self::new)
    }
}

impl ComposeFrom<u8> for Message {
    fn compose_from(parts: &mut Vec<u8>) -> Option<Self> {
        let mut length = 0;

        let message = std::str::from_utf8(parts).ok().and_then(|buffer| {
            buffer.find(HEADER_END).and_then(|header_length| {
                let mut content_length: Option<usize> = None;

                buffer.get(..header_length).and_then(|header| {
                    let content_start = header_length.saturating_add(HEADER_END.len());

                    for field in header.split("\r\n") {
                        let mut items = field.split(": ");

                        if items.next() == Some(HEADER_CONTENT_LENGTH) {
                            if let Some(content_length_str) = items.next() {
                                if let Ok(value) = content_length_str.parse() {
                                    content_length = Some(value);
                                }
                            }

                            break;
                        }
                    }

                    match content_length {
                        None => {
                            length = header_length;
                            None
                        }
                        Some(content_length) => {
                            if let Some(total_len) = content_start.checked_add(content_length) {
                                if parts.len() < total_len {
                                    None
                                } else if let Some(content) = buffer.get(content_start..total_len) {
                                    length = total_len;
                                    serde_json::from_str(content).ok()
                                } else {
                                    length = content_start;
                                    None
                                }
                            } else {
                                length = content_start;
                                None
                            }
                        }
                    }
                })
            })
        });

        #[allow(unused_results)] // No intent to use drained elements.
        {
            parts.drain(..length);
        }

        message
    }
}

impl StripFrom<Message> for u8 {
    #[inline]
    fn strip_from(good: &Message) -> Vec<Self> {
        serde_json::to_string(good).map_or(Vec::new(), |content| {
            trace!("write content: {}", content);
            format!(
                "{}: {}\r\n\r\n{}",
                HEADER_CONTENT_LENGTH,
                content.len(),
                content
            )
            .as_bytes()
            .to_vec()
        })
    }
}

/// A json-rpc object.
#[derive(Debug, Deserialize, Serialize)]
#[serde(untagged)]
#[allow(dead_code)] // False positive.
pub(crate) enum Object {
    /// A request json-rpc object.
    Request {
        /// The method identifier.
        // TODO: Convert this to &str.
        method: String,
        /// The parameters.
        params: Value,
        /// The id.
        #[serde(skip_serializing_if = "Option::is_none")]
        id: Option<Id>,
    },
    /// A response json-rpc object.
    Response {
        /// The outcome.
        #[serde(flatten)]
        outcome: Outcome,
        /// The id.
        id: Id,
    },
}

impl Object {
    /// Creates a request of type `T` with `id`.
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

    /// Creates a notification of type `T`.
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

    /// Creates a response to a request of type `T` with `id`.
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

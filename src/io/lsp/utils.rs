//! Implements language server utilities.
use {
    serde_json::error::Error as SerdeJsonError,
    std::{
        io,
        num::ParseIntError,
        str::Utf8Error,
        sync::mpsc::TryRecvError,
    },
    thiserror::Error,
};

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

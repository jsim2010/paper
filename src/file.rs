//! Defines the interaction with files.
pub(crate) mod local;

use crate::lsp::{self, ProgressParams};
use lsp_types::TextDocumentItem;
use std::{
    fmt::{self, Debug, Display, Formatter},
    io,
};

/// Specifies the type returned by `Explorer` functions.
pub type Effect<T> = Result<T, Error>;

/// Defines the interface between the application and documents.
pub trait Explorer: Debug {
    /// Initializes all functionality needed by the Explorer.
    fn start(&mut self) -> Effect<()>;
    /// Returns the text from a file.
    fn read(&mut self, path: &String) -> Effect<TextDocumentItem>;
    /// Writes text to a file.
    fn write(&self, doc: &TextDocumentItem) -> Effect<()>;
    /// Returns the oldest notification from `Explorer`.
    fn receive_notification(&mut self) -> Option<ProgressParams>;
}

/// Specifies an error within the `Explorer`.
#[derive(Debug)]
pub enum Error {
    /// Error caused by I/O.
    Io(io::Error),
    /// Error caused by LSP.
    Lsp(lsp::Error),
}

impl Display for Error {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Error::Io(e) => write!(f, "I/O Error caused by {}", e),
            Error::Lsp(e) => write!(f, "Lsp Error {}", e),
        }
    }
}

impl From<io::Error> for Error {
    fn from(error: io::Error) -> Self {
        Error::Io(error)
    }
}

impl From<lsp::Error> for Error {
    fn from(error: lsp::Error) -> Self {
        Error::Lsp(error)
    }
}

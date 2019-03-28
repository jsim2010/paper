//! Implements the functionality to interact with data located in different storages.
use crate::mode::Flag;
use crate::{fmt, Display, Formatter};
use serde_json;
use std::error;
use std::io;
use std::sync::mpsc::RecvError;

/// An error within the Language Server Protocol functionality.
#[derive(Clone, Copy, Debug)]
pub enum LspError {
    /// An error caused by serde_json.
    SerdeJson {
        /// The index of the line where the error occurred.
        line: usize,
        /// The index of the column where the error occurred.
        column: usize,
    },
    /// An error in IO.
    Io,
    /// An error in parsing LSP data.
    Parse,
    /// An error in the LSP protocol.
    Protocol,
    /// An error caused while managing threads.
    Thread(RecvError),
}

impl From<serde_json::Error> for LspError {
    #[inline]
    fn from(error: serde_json::Error) -> Self {
        LspError::SerdeJson {
            line: error.line(),
            column: error.column(),
        }
    }
}

impl From<io::Error> for LspError {
    #[inline]
    fn from(_error: io::Error) -> Self {
        LspError::Io
    }
}

impl From<RecvError> for LspError {
    #[inline]
    fn from(_error: RecvError) -> Self {
        LspError::Thread(RecvError)
    }
}

impl Display for LspError {
    #[inline]
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            LspError::SerdeJson { line, column } => {
                write!(f, "Serde Json Error at ({}, {})", line, column)
            }
            LspError::Io => write!(f, "IO Error"),
            LspError::Thread(error) => write!(f, "Thread Error {}", error),
            LspError::Parse => write!(f, "Parse Error"),
            LspError::Protocol => write!(f, "Protocol Error"),
        }
    }
}

impl error::Error for LspError {}

impl From<LspError> for Flag {
    #[inline]
    fn from(error: LspError) -> Self {
        Flag::Lsp(error)
    }
}

/// Signifies an [`Error`] from an [`Explorer`].
// Needed due to io::Error not implementing Clone for double.
#[derive(Clone, Copy, Debug)]
pub struct Error {
    /// The kind of the [`io::Error`].
    kind: io::ErrorKind,
}

impl error::Error for Error {}

impl Display for Error {
    #[inline]
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "IO Error")
    }
}

impl From<io::Error> for Error {
    #[inline]
    fn from(value: io::Error) -> Self {
        Self { kind: value.kind() }
    }
}

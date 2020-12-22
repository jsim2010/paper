//! Implements errors thrown by the user interface.
#![allow(clippy::module_name_repetitions)] // It is appropriate for items to end with `Error`.
use {crossterm::ErrorKind, thiserror::Error as ThisError};

/// An error creating a [`Terminal`].
#[derive(Debug, ThisError)]
#[error(transparent)]
pub enum CreateTerminalError {
    /// An error initializing the terminal output.
    Init(#[from] InitError),
}

/// A failure consuming a [`UserAction`].
#[derive(Debug, market::ConsumeFault, ThisError)]
#[error(transparent)]
pub enum UserActionFailure {
    /// A failure polling for a [`UserAction`].
    Poll(#[from] PollFailure),
    /// A failure reading a [`UserAction`].
    Read(#[from] ReadFailure),
}

/// A failure producing terminal output.
#[derive(Debug, market::ProduceFault, ThisError)]
#[error(transparent)]
pub enum DisplayCmdFailure {
    /// A failure writing text.
    Write(#[from] WriteFailure),
    /// A failure incrementing a row.
    End(#[from] ReachedEnd),
}

/// A failure writing to stdout.
#[derive(Debug, ThisError)]
#[error("writing: {error}")]
pub struct WriteFailure {
    /// The error.
    #[from]
    error: ErrorKind,
}

/// An error initializing the terminal.
#[derive(Debug, ThisError)]
#[error("clearing screen: {error}")]
pub struct InitError {
    /// The error.
    #[from]
    error: ErrorKind,
}

/// An error polling for a [`UserAction`].
#[derive(Debug, ThisError)]
#[error("unable to poll: {error}")]
pub struct PollFailure {
    /// The error.
    #[from]
    error: ErrorKind,
}

/// An error reading a [`UserAction`].
#[derive(Debug, ThisError)]
#[error("unable to read: {error}")]
pub struct ReadFailure {
    /// The error.
    #[from]
    error: ErrorKind,
}

/// An error destroying the terminal.
#[derive(Debug, ThisError)]
#[error("leaving alternate screen: {error}")]
pub(crate) struct DestroyError {
    /// The error.
    #[from]
    error: ErrorKind,
}

/// When the [`RowId`] has reached its end.
#[derive(Clone, Copy, Debug, ThisError)]
#[error("")]
pub struct ReachedEnd;

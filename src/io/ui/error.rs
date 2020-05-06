//! Implements errors thrown by the user interface.
#![allow(clippy::module_name_repetitions)] // Okay for items to end with `Error`.
use {crossterm::ErrorKind, thiserror::Error as ThisError};

/// An error creating a [`Terminal`].
#[derive(Debug, ThisError)]
pub enum CreateTerminalError {
    /// An error initializing the terminal output.
    #[error(transparent)]
    Init(#[from] InitError),
}

/// A failure consuming a [`UserAction`].
#[derive(Debug, ThisError)]
#[error(transparent)]
pub enum UserActionFailure {
    /// A failure polling for a [`UserAction`].
    Poll(#[from] PollFailure),
    /// A failure reading a [`UserAction`].
    Read(#[from] ReadFailure),
}

/// An error producing terminal output.
#[derive(Debug, ThisError)]
pub enum DisplayCmdFailure {
    /// An error writing text.
    #[error(transparent)]
    Write(#[from] WriteFailure),
}

/// An error writing to stdout.
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

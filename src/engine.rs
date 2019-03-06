//! Implements the state machine of the application.
use crate::{error, fmt, storage, ui, Display, Formatter, TryFromIntError};
use std::io;

/// Signifies a [`Result`] during the execution of an [`Operation`].
pub type Outcome<T> = Result<T, Failure>;

/// Signifies a state of the application.
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
pub(crate) enum Mode {
    /// Displays the current view.
    Display,
    /// Displays the current command.
    Command,
    /// Displays the current filter expression and highlights the characters that match the filter.
    Filter,
    /// Displays the highlighting that has been selected.
    Action,
    /// Displays the current view along with the current edits.
    Edit,
}

impl Display for Mode {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Mode::Display => write!(f, "Display"),
            Mode::Command => write!(f, "Command"),
            Mode::Filter => write!(f, "Filter"),
            Mode::Action => write!(f, "Action"),
            Mode::Edit => write!(f, "Edit"),
        }
    }
}

impl Default for Mode {
    fn default() -> Self {
        Mode::Display
    }
}

/// Signifies an [`error::Error`] that occurs during the execution of an [`Operation`].
#[derive(Clone, Copy, Debug)]
pub enum Failure {
    /// An error occurred during the execution of a [`ui`] command.
    Ui(ui::Error),
    /// An attempt to convert one type to another was unsuccessful.
    Conversion(TryFromIntError),
    /// An error occurred during the execution of File command.
    File(storage::Error),
    /// An error occurred during interaction with the language server.
    Lsp(storage::LspError),
    /// Notifies the application to quit.
    Quit,
}

impl error::Error for Failure {
    fn source(&self) -> Option<&(dyn error::Error + 'static)> {
        match self {
            Failure::Ui(error) => Some(error),
            Failure::Conversion(error) => Some(error),
            Failure::File(error) => Some(error),
            Failure::Lsp(error) => Some(error),
            Failure::Quit => None,
        }
    }
}

impl Display for Failure {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Failure::Ui(error) => write!(f, "{}", error),
            Failure::Conversion(error) => write!(f, "{}", error),
            Failure::File(error) => write!(f, "{}", error),
            Failure::Lsp(error) => write!(f, "{}", error),
            Failure::Quit => write!(f, "Quit"),
        }
    }
}

impl From<ui::Error> for Failure {
    fn from(error: ui::Error) -> Self {
        Failure::Ui(error)
    }
}

impl From<TryFromIntError> for Failure {
    fn from(error: TryFromIntError) -> Self {
        Failure::Conversion(error)
    }
}

impl From<io::Error> for Failure {
    fn from(error: io::Error) -> Self {
        Failure::File(storage::Error::from(error))
    }
}

impl From<storage::LspError> for Failure {
    fn from(error: storage::LspError) -> Self {
        Failure::Lsp(error)
    }
}

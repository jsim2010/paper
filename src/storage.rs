//! Implements the functionality to interact with data located in different storages.
use crate::{fmt, Debug, Display, Formatter, Outcome};
use std::error;
use std::fs;
use std::io;

/// Signifies a file.
#[derive(Clone, Debug)]
pub struct File<'a> {
    /// The path of the file.
    path: String,
    /// The [`Explorer`] used for interacting with the file.
    explorer: &'a dyn Explorer,
}

impl<'a> File<'a> {
    /// Creates a new `File`.
    pub fn new(explorer: &'a dyn Explorer, path: String) -> Self {
        Self { path, explorer }
    }

    /// Returns the data read from the `File`.
    pub(crate) fn read(&self) -> Outcome<String> {
        self.explorer.read(&self.path)
    }

    /// Writes the given data to the `File`.
    pub(crate) fn write(&self, data: &str) -> Outcome<()> {
        self.explorer.write(&self.path, data)
    }
}

impl Default for File<'_> {
    fn default() -> Self {
        Self {
            path: String::new(),
            explorer: &Local,
        }
    }
}

/// Interacts and processes file data.
pub trait Explorer: Debug {
    /// Reaturns the data from the file at a given path.
    fn read(&self, path: &str) -> Outcome<String>;
    /// Writes data to a file at the given path.
    fn write(&self, path: &str, data: &str) -> Outcome<()>;
}

/// Signifies an [`Explorer`] of the local storage.
#[derive(Debug)]
pub(crate) struct Local;

impl Explorer for Local {
    fn read(&self, path: &str) -> Outcome<String> {
        Ok(fs::read_to_string(path).map(|data| data.replace('\r', ""))?)
    }

    fn write(&self, path: &str, data: &str) -> Outcome<()> {
        fs::write(path, data)?;
        Ok(())
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
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "IO Error")
    }
}

impl From<io::Error> for Error {
    fn from(value: io::Error) -> Self {
        Self { kind: value.kind() }
    }
}

use crate::{fmt, Formatter, Display, Debug, Outcome};
use std::fs;
use std::io;
use std::error;

#[derive(Clone, Debug)]
pub struct File<'a> {
    path: String,
    explorer: &'a dyn Explorer,
}

impl<'a> File<'a> {
    pub fn new(explorer: &'a dyn Explorer, path: String) -> Self {
        Self {path, explorer}
    }

    pub(crate) fn read(&self) -> Outcome<String> {
        self.explorer.read(&self.path)
    }

    pub(crate) fn write(&self, data: &String) -> Outcome<()> {
        self.explorer.write(&self.path, data)
    }
}

impl Default for File<'_> {
    fn default() -> Self {
        Self {path: String::new(), explorer: &Local}
    }
}

pub trait Explorer: Debug {
    fn read(&self, path: &String) -> Outcome<String>;
    fn write(&self, path: &String, data: &String) -> Outcome<()>;
}

#[derive(Debug)]
pub(crate) struct Local;

impl Explorer for Local {
    fn read(&self, path: &String) -> Outcome<String> {
        Ok(fs::read_to_string(path).map(|data| data.replace('\r', ""))?)
    }

    fn write(&self, path: &String, data: &String) -> Outcome<()> {
        fs::write(path, data)?;
        Ok(())
    }
}

#[derive(Clone, Debug)]
pub struct Error {
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
        Self {
            kind: value.kind(),
        }
    }
}

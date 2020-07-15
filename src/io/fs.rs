//! Handles filesystem operations.
use {
    crate::io::LanguageId,
    fehler::throws,
    market::{ClosedMarketFailure, ConsumeError, Consumer, ProduceError, Producer, UnlimitedQueue},
    parse_display::Display as ParseDisplay,
    std::{
        env, fs,
        io::{self, ErrorKind},
        str::Lines,
    },
    thiserror::Error as ThisError,
    url::{ParseError, Url},
};

/// An error determining the root directory.
#[derive(Debug, ThisError)]
pub enum RootDirError {
    /// An error determing the current working directory.
    #[error("current working directory is invalid: {0}")]
    GetWorkingDir(#[from] io::Error),
    /// An error creating the root directory [`Purl`].
    #[error("unable to create URL of root directory `{0}`")]
    Create(String),
}

/// Returns the root directory.
#[throws(RootDirError)]
fn root_dir() -> Url {
    let dir = env::current_dir()?;

    Url::from_directory_path(&dir)
        .map_err(|_| RootDirError::Create(format!("{}", dir.display())))?
}

/// The interface to the file system.
#[derive(Debug)]
pub(crate) struct FileSystem {
    /// Queue of URLs to read.
    urls_to_read: UnlimitedQueue<Url>,
    /// The root directory of the file system.
    root_dir: Url,
}

impl FileSystem {
    /// Creates a new `FileSystem`.
    #[throws(RootDirError)]
    pub(crate) fn new() -> Self {
        Self {
            urls_to_read: UnlimitedQueue::new(),
            root_dir: root_dir()?,
        }
    }

    /// Returns the root directory.
    pub(crate) const fn root_dir(&self) -> &Url {
        &self.root_dir
    }
}

impl Consumer for FileSystem {
    type Good = File;
    type Failure = ConsumeFileError;

    #[throws(ConsumeError<Self::Failure>)]
    fn consume(&self) -> Self::Good {
        let path_url = self
            .urls_to_read
            .consume()
            .map_err(ConsumeError::map_into)?;

        File::read(path_url).map_err(Self::Failure::from)?
    }
}

impl Producer for FileSystem {
    type Good = FileCommand;
    type Failure = FileError;

    #[throws(ProduceError<Self::Failure>)]
    fn produce(&self, good: Self::Good) {
        match good {
            Self::Good::Read { path } => self
                .urls_to_read
                .produce(
                    self.root_dir
                        .join(&path)
                        .map_err(|error| ProduceError::Failure(error.into()))?,
                )
                .map_err(ProduceError::map_into)?,
            Self::Good::Write { url, text } => {
                fs::write(url.path(), text).map_err(|error| ProduceError::Failure(error.into()))?
            }
        }
    }
}

/// An error executing a file command.
#[derive(Debug, ThisError)]
pub enum FileError {
    /// The queue is closed.
    #[error(transparent)]
    Closed(#[from] ClosedMarketFailure),
    /// An IO error.
    #[error("")]
    Io(#[from] io::Error),
    /// An error creating a [`Purl`]
    #[error("")]
    Create(#[from] ParseError),
}

/// Specifies a command to be executed on a file.
#[derive(Debug, ParseDisplay)]
pub(crate) enum FileCommand {
    /// Reads from the file at `path`.
    #[display("Read `{path}`")]
    Read {
        /// The relative path of the file.
        path: String,
    },
    /// Writes `text` to the file at `url`.
    #[display("Write {url}")]
    Write {
        /// The URL of the file to be written.
        url: Url,
        /// The text to be written.
        text: String,
    },
}

/// A struct that represents a file.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct File {
    /// The URL of the file.
    url: Url,
    /// The text of a file.
    text: String,
}

impl File {
    /// Creates a file from the path of `url`.
    #[throws(ReadFileError)]
    fn read(url: Url) -> Self {
        Self {
            text: fs::read_to_string(&url.path()).map_err(|error| ReadFileError {
                file: url.to_string(),
                error: error.kind(),
            })?,
            url,
        }
    }

    /// Returns the  [`Lines`] of the text.
    pub(crate) fn lines(&self) -> Lines<'_> {
        self.text.lines()
    }

    /// Returns a reference to the text of `self`.
    pub(crate) const fn text(&self) -> &String {
        &self.text
    }

    /// Returns a reference to the URL of `self`.
    pub(crate) const fn url(&self) -> &Url {
        &self.url
    }

    /// Returns the language id of `self`.
    pub(crate) fn language_id(&self) -> Option<LanguageId> {
        if self.url.path().ends_with(".rs") {
            Some(LanguageId::Rust)
        } else {
            None
        }
    }
}

/// An error consuming a file.
#[derive(Debug, ThisError)]
pub enum ConsumeFileError {
    /// An error reading a file.
    #[error("")]
    Read(#[from] ReadFileError),
    /// The read queue has closed.
    #[error("")]
    Closed(#[from] ClosedMarketFailure),
}

/// An error while reading a file.
#[derive(Debug, ThisError)]
#[error("failed to read `{file}`: {error:?}")]
pub struct ReadFileError {
    /// The error.
    error: ErrorKind,
    /// The path of the file being read.
    file: String,
}

//! Handles filesystem operations.
use {
    docuglot::Language,
    fehler::throws,
    log::trace,
    market::{
        ClosedMarketError, ConsumeFailure, Consumer, ProduceFailure, Producer, UnlimitedQueue,
    },
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
    type Error = ConsumeFileError;

    #[throws(ConsumeFailure<Self::Error>)]
    fn consume(&self) -> Self::Good {
        let path_url = self
            .urls_to_read
            .consume()
            .map_err(ConsumeFailure::map_into)?;

        File::read(path_url).map_err(Self::Error::from)?
    }
}

impl Producer for FileSystem {
    type Good = FileCommand;
    type Error = FileError;

    #[throws(ProduceFailure<Self::Error>)]
    fn produce(&self, good: Self::Good) {
        match good {
            Self::Good::Read { path } => self
                .urls_to_read
                .produce(
                    self.root_dir
                        .join(&path)
                        .map_err(|error| ProduceFailure::Error(error.into()))?,
                )
                .map_err(ProduceFailure::map_into)?,
        }
    }
}

/// An error executing a file command.
#[derive(Debug, ThisError)]
pub enum FileError {
    /// The queue is closed.
    #[error(transparent)]
    Closed(#[from] ClosedMarketError),
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
        trace!("read {}", url.path());
        Self {
            text: fs::read_to_string(url.to_file_path().map_err(|_| ReadFileError {
                file: url.to_string(),
                error: ErrorKind::NotFound,
            })?)
            .map_err(|error| ReadFileError {
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

    /// Returns the `Language` of `self`.
    pub(crate) fn language(&self) -> Language {
        if self.url.path().ends_with(".rs") {
            Language::Rust
        } else {
            Language::Plaintext
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
    Closed(#[from] ClosedMarketError),
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

//! Handles filesystem operations.
use {
    docuglot::Language,
    fehler::throws,
    log::trace,
    market::{queue::Procurer, ConsumeFailure, ConsumeFault, Consumer, Failure, Producer},
    parse_display::Display as ParseDisplay,
    std::{
        env, fs,
        io::{self, ErrorKind},
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

    #[allow(clippy::map_err_ignore)] // Url::from_directory_path() returns Result<_, Err(())>.
    Url::from_directory_path(&dir)
        .map_err(|_| RootDirError::Create(format!("{}", dir.display())))?
}

/// Create the interface to the file system.
#[throws(RootDirError)]
pub(crate) fn create_file_system() -> (FileCommandProducer, FileConsumer) {
    let (url_producer, url_consumer) = market::queue::create_supply_chain();
    (
        FileCommandProducer {
            root_dir: root_dir()?,
            url_producer,
        },
        FileConsumer { url_consumer },
    )
}

/// Consumes [`Url`]s.
pub(crate) struct FileConsumer {
    /// The [`Consumer`].
    url_consumer: Procurer<Url>,
}

impl Consumer for FileConsumer {
    type Good = File;
    type Failure = ConsumeFailure<ConsumeFileError>;

    #[throws(Self::Failure)]
    fn consume(&self) -> Self::Good {
        #[allow(clippy::map_err_ignore)]
        // Currently unable to implement ConsumeFailure<T>: From<InsufficientStockFailure>.
        let path_url = self
            .url_consumer
            .consume()
            // consume() can throw InsufficientStockFailure
            .map_err(|_| ConsumeFailure::EmptyStock)?;

        File::read(path_url).map_err(ConsumeFileError::from)?
    }
}

/// Produces file commands.
pub(crate) struct FileCommandProducer {
    /// The root directory of the application.
    root_dir: Url,
    /// Sends the [`Url`]s to the file system handler.
    url_producer: market::queue::Supplier<Url>,
}

impl FileCommandProducer {
    /// Returns the root directory.
    pub(crate) const fn root_dir(&self) -> &Url {
        &self.root_dir
    }
}

impl Producer for FileCommandProducer {
    type Good = FileCommand;
    type Failure = FileError;

    #[allow(clippy::unwrap_in_result)] // Supplier::produce() cannot fail.
    #[throws(Self::Failure)]
    fn produce(&self, good: Self::Good) {
        match good {
            #[allow(clippy::unwrap_used)] // Supplier::produce() cannot fail.
            Self::Good::Read { path } => self
                .url_producer
                .produce(self.root_dir.join(&path)?)
                .unwrap(),
        }
    }
}

/// An error executing a file command.
#[derive(Debug, ThisError)]
pub enum FileError {
    /// An IO error.
    #[error("")]
    Io(#[from] io::Error),
    /// An error creating a [`Purl`]
    #[error("")]
    Create(#[from] ParseError),
}

impl Failure for FileError {
    type Fault = Self;
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
        #[allow(clippy::map_err_ignore)]
        // Url::to_file_path() returns () as Err type so the error has no helpful information.
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
#[derive(Debug, ConsumeFault, ThisError)]
pub enum ConsumeFileError {
    /// An error reading a file.
    #[error("")]
    Read(#[from] ReadFileError),
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

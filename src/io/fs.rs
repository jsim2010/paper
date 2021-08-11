//! Handles filesystem operations.
use {
    core::fmt::{self, Display, Formatter},
    docuglot::Language,
    fehler::throws,
    log::trace,
    market::{EmptyStock, Agent, Consumer, Flawless, ProduceFault, EmptyStockFailure, Return, Flaws, Producer},
    markets::queue::{self, Supplier, Procurer},
    parse_display::Display as ParseDisplay,
    std::{
        env, fs,
        io::{self, ErrorKind},
    },
    url::{ParseError, Url},
};

/// An error determining the root directory.
#[derive(Debug, thiserror::Error)]
pub enum RootDirError {
    /// An error determing the current working directory.
    #[error("find valid current working directory: {0}")]
    GetWorkingDir(#[from] io::Error),
    /// An error creating the root directory [`Purl`].
    #[error("create URL of root directory `{0}`")]
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
    let (url_producer, url_consumer) = queue::create();
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

impl Agent for FileConsumer {
    type Good = Result<File, ReadFileGlitch>;
}

impl Consumer for FileConsumer {
    type Flaws = EmptyStock;

    #[throws(Failure<Self::Flaws>)]
    fn consume(&self) -> Self::Good {
        File::read(self.url_consumer.consume()?)
    }
}

impl Display for FileConsumer {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "File Consumer")
    }
}

/// Produces file commands.
pub(crate) struct FileCommandProducer {
    /// The root directory of the application.
    root_dir: Url,
    /// Sends the [`Url`]s to the file system handler.
    url_producer: Supplier<Url>,
}

impl FileCommandProducer {
    /// Returns the root directory.
    pub(crate) const fn root_dir(&self) -> &Url {
        &self.root_dir
    }
}

impl Agent for FileCommandProducer {
    type Good = FileCommand;
}

impl Display for FileCommandProducer {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "File Command Producer")
    }
}

impl Producer for FileCommandProducer {
    type Flaws = FileError;

    #[allow(clippy::unwrap_in_result)] // Supplier::produce() cannot fail.
    #[throws(Return<Self::Good, Self::Failure>)]
    fn produce(&self, good: Self::Good) {
        match good {
            #[allow(clippy::unwrap_used)] // Supplier::produce() cannot fail.
            Self::Good::Read { path } => self
                .url_producer
                .produce(self.root_dir.join(&path).map_err(|err| Return::new(Self::Good::Read { path}, err.into()))?)
                .unwrap(),
        }
    }
}

/// An error executing a file command.
#[derive(Debug, thiserror::Error)]
pub enum FileError {
    /// An error parsing a [`Url`]
    #[error("parse URL: {0}")]
    Create(#[from] ParseError),
}

impl Flaws for FileError {
    type Insufficiency = Flawless;
    type Defect = Self;
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
    #[throws(ReadFileGlitch)]
    fn read(url: Url) -> Self {
        trace!("read {}", url.path());
        #[allow(clippy::map_err_ignore)]
        // Url::to_file_path() returns () as Err type so the error has no helpful information.
        Self {
            text: fs::read_to_string(url.to_file_path().map_err(|_| ReadFileGlitch {
                file: url.to_string(),
                error: ErrorKind::NotFound,
            })?)
            .map_err(|error| ReadFileGlitch {
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
    pub(crate) fn language(&self) -> Option<Language> {
        if self.url.path().ends_with(".rs") {
            Some(Language::Rust)
        } else {
            None
        }
    }
}

/// An error while reading a file.
#[derive(Debug, thiserror::Error)]
#[error("Failed to read `{file}`: {error:?}")]
pub struct ReadFileGlitch {
    /// The error.
    error: ErrorKind,
    /// The path of the file being read.
    file: String,
}

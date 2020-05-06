//! Handles filesystem operations.
use {
    crate::io::LanguageId,
    core::{
        convert::{TryFrom, TryInto},
        fmt::{self, Display},
    },
    fehler::throws,
    market::{ClosedMarketFailure, ConsumeError, Consumer, ProduceError, Producer, UnlimitedQueue},
    parse_display::Display as ParseDisplay,
    std::{
        ffi::OsStr,
        fs,
        io::{self, ErrorKind},
        path::{Path, PathBuf},
        str::Lines,
    },
    thiserror::Error,
    url::Url,
};

/// A **P**ath **URL** - a path and its appropriate URL.
///
/// Analysis that path converts to a valid URL is performed one time, when the `Purl` is created.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Purl {
    /// The path.
    path: PathBuf,
    /// The URL of `path`.
    url: Url,
}

impl Purl {
    /// Creates a new `Purl` by appending `child` to `self`.
    #[throws(CreatePurlError)]
    pub(crate) fn join(&self, child: &str) -> Self {
        let mut path = self.path.clone();

        path.push(child);
        path.try_into()?
    }

    /// Returns the language id of `self`.
    pub(crate) fn language_id(&self) -> Option<LanguageId> {
        self.path
            .extension()
            .and_then(OsStr::to_str)
            .and_then(|ext| match ext {
                "rs" => Some(LanguageId::Rust),
                _ => None,
            })
    }
}

impl AsRef<OsStr> for Purl {
    #[inline]
    #[must_use]
    fn as_ref(&self) -> &OsStr {
        self.path.as_ref()
    }
}

impl AsRef<Path> for Purl {
    #[inline]
    #[must_use]
    fn as_ref(&self) -> &Path {
        self.path.as_ref()
    }
}

impl AsRef<Url> for Purl {
    #[inline]
    #[must_use]
    fn as_ref(&self) -> &Url {
        &self.url
    }
}

impl Display for Purl {
    #[inline]
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.url)
    }
}

impl TryFrom<PathBuf> for Purl {
    type Error = CreatePurlError;

    #[inline]
    #[throws(Self::Error)]
    fn try_from(value: PathBuf) -> Self {
        Self {
            path: value.clone(),
            url: Url::from_file_path(&value).map_err(|_| Self::Error::Create { path: value })?,
        }
    }
}

/// An error creating a [`Purl`].
#[derive(Clone, Debug, Error)]
pub enum CreatePurlError {
    /// An error creating the URL from `path`.
    #[error("`{path}` is not absolute or has an invalid prefix")]
    Create {
        /// The path.
        path: PathBuf,
    },
}

/// The interface to the file system.
#[derive(Debug, Default)]
pub(crate) struct FileSystem {
    /// Queue of URLs to read.
    files_to_read: UnlimitedQueue<Purl>,
}

impl Consumer for FileSystem {
    type Good = File;
    type Failure = ConsumeFileError;

    #[throws(ConsumeError<Self::Failure>)]
    fn consume(&self) -> Self::Good {
        let path_url = self.files_to_read.consume().map_err(|error| match error {
            ConsumeError::EmptyStock => ConsumeError::EmptyStock,
            ConsumeError::Failure(failure) => ConsumeError::Failure(failure.into()),
        })?;

        File {
            text: fs::read_to_string(&path_url)
                .map_err(|error| ReadFileError {
                    file: path_url.to_string(),
                    error: error.kind(),
                })
                .map_err(|error| ConsumeError::Failure(error.into()))?,
            url: path_url,
        }
    }
}

impl Producer for FileSystem {
    type Good = FileCommand;
    type Failure = FileError;

    #[throws(ProduceError<Self::Failure>)]
    fn produce(&self, good: Self::Good) {
        match good {
            Self::Good::Read { url } => self
                .files_to_read
                .produce(url)
                .map_err(|error| error.map(Self::Failure::from))?,
            Self::Good::Write { url, text } => {
                fs::write(&url, text).map_err(|error| ProduceError::Failure(error.into()))?
            }
        }
    }
}

/// An error executing a file command.
#[derive(Debug, Error)]
pub enum FileError {
    /// The queue is closed.
    #[error(transparent)]
    Closed(#[from] ClosedMarketFailure),
    /// An IO error.
    #[error("")]
    Io(#[from] io::Error),
}

/// Specifies a command to be executed on a file.
#[derive(Debug, ParseDisplay)]
pub(crate) enum FileCommand {
    /// Reads from the file at `url`.
    #[display("Read {url}")]
    Read {
        /// The URL of the file to be read.
        url: Purl,
    },
    /// Writes `text` to the file at `url`.
    #[display("Write {url}")]
    Write {
        /// The URL of the file to be written.
        url: Purl,
        /// The text to be written.
        text: String,
    },
}

/// A struct that represents a file.
#[derive(Clone, Debug, PartialEq)]
pub struct File {
    /// The URL of the file.
    url: Purl,
    /// The text of a file.
    text: String,
}

impl File {
    /// Returns the  [`Lines`] of the text.
    pub(crate) fn lines(&self) -> Lines<'_> {
        self.text.lines()
    }

    /// Returns a reference to the text of `self`.
    pub(crate) const fn text(&self) -> &String {
        &self.text
    }

    /// Returns a reference to the URL of `self`.
    pub(crate) const fn url(&self) -> &Purl {
        &self.url
    }

    /// Returns the language id of `self`.
    pub(crate) fn language_id(&self) -> Option<LanguageId> {
        self.url.language_id()
    }
}

/// An error consuming a file.
#[derive(Debug, Error)]
pub enum ConsumeFileError {
    /// An error reading a file.
    #[error("")]
    Read(#[from] ReadFileError),
    /// The read queue has closed.
    #[error("")]
    Closed(#[from] ClosedMarketFailure),
}

/// An error while reading a file.
#[derive(Debug, Error)]
#[error("failed to read `{file}`: {error:?}")]
pub struct ReadFileError {
    /// The error.
    error: ErrorKind,
    /// The path of the file being read.
    file: String,
}

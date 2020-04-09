//! Handles filesystem operations.
use {
    crate::io::LanguageId,
    core::{
        convert::{TryFrom, TryInto},
        fmt::{self, Display},
    },
    market::{ClosedMarketError, Consumer, Producer, UnlimitedQueue},
    std::{
        ffi::OsStr,
        fs,
        io::{self, ErrorKind},
        path::{Path, PathBuf},
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
    pub(crate) fn join(&self, child: &str) -> Result<Self, CreatePurlError> {
        let mut path = self.path.clone();

        path.push(child);
        path.try_into()
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
    fn try_from(value: PathBuf) -> Result<Self, Self::Error> {
        Ok(Self {
            path: value.clone(),
            url: Url::from_file_path(&value).map_err(|_| Self::Error::Create{path: value})?,
        })
    }
}

/// An error creating a [`Purl`].
#[derive(Clone, Debug, Error)]
pub enum CreatePurlError {
    /// An error creating the URL from `path`.
    #[error("`{path}` is not absolute or has an invalid prefix")]
    Create {
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
    type Error = ConsumeFileError;

    fn consume(&self) -> Result<Option<Self::Good>, Self::Error> {
        let path_url = self.files_to_read.consume()?;

        Ok(if let Some(url) = path_url {
            Some(File {
                text: fs::read_to_string(&url).map_err(|error| ReadFileError {
                    file: url.to_string(),
                    error: error.kind(),
                })?,
                url,
            })
        } else {
            None
        })
    }
}

impl Producer for FileSystem {
    type Good = FileCommand;
    type Error = FileError;

    fn produce(&self, good: Self::Good) -> Result<Option<Self::Good>, Self::Error> {
        Ok(match good {
            Self::Good::Read { url } => self
                .files_to_read
                .produce(url)
                .map(|result| result.map(|url| Self::Good::Read { url }))?,
            Self::Good::Write { url, text } => fs::write(&url, text).map(|_| None)?,
        })
    }
}

/// An error executing a file command.
#[derive(Debug, Error)]
pub enum FileError {
    /// The queue is closed.
    #[error("")]
    Closed(#[from] ClosedMarketError),
    /// An IO error.
    #[error("")]
    Io(#[from] io::Error),
}

/// Specifies a command to be executed on a file.
pub(crate) enum FileCommand {
    /// Reads from the file at `url`.
    Read {
        /// The URL of the file to be read.
        url: Purl,
    },
    /// Writes `text` to the file at `url`.
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
    /// Returns if the text of `self` is empty.
    pub(crate) fn is_empty(&self) -> bool {
        self.text.is_empty()
    }

    /// Deletes the text between `start_line` and `end_line` from the text of `self`.
    pub(crate) fn delete_selection(&mut self, start_line: usize, end_line: usize) {
        let mut newline_indices = self.text.match_indices('\n');

        if let Some(start_index) = if start_line == 0 {
            Some(0)
        } else {
            newline_indices
                .nth(start_line.saturating_sub(1))
                .map(|index| index.0.saturating_add(1))
        } {
            if let Some((end_index, ..)) =
                newline_indices.nth(end_line.saturating_sub(start_line.saturating_add(1)))
            {
                let _ = self.text.drain(start_index..=end_index);
            }
        }
    }

    /// Returns the number of lines in `self`.
    pub(crate) fn line_count(&self) -> usize {
        self.text.lines().count()
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
    Closed(#[from] ClosedMarketError),
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

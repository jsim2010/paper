pub mod ui;

use {
    clap::ArgMatches,
    core::{
        convert::{TryFrom, TryInto},
        fmt,
    },
    log::trace,
    std::{
        collections::VecDeque,
        env,
        io,
        ffi::OsStr,
        path::{Path, PathBuf},
    },
    ui::{Terminal, Change},
    url::Url,
    thiserror::Error,
};

#[derive(Debug, Error)]
pub enum IntoArgumentsError{
    #[error("current working directory is invalid: {0}")]
    RootDir(#[from] io::Error),
    #[error("root directory is invalid: {0}")]
    Url(#[from] UrlError),
}

/// Configures the initialization of `paper`.
#[derive(Clone, Debug, Default)]
pub struct Arguments {
    /// The file to be viewed.
    ///
    /// [`None`] indicates that the display should be empty.
    ///
    /// [`None`]: https://doc.rust-lang.org/std/option/enum.Option.html#variant.None
    pub file: Option<String>,
    /// The working directory of `paper`.
    pub working_dir: PathUrl,
}

impl TryFrom<ArgMatches<'_>> for Arguments {
    type Error = IntoArgumentsError;

    #[inline]
    fn try_from(value: ArgMatches<'_>) -> Result<Self, Self::Error> {
        Ok(Self {
            file: value.value_of("file").map(str::to_string),
            working_dir: PathUrl::try_from(env::current_dir().map_err(IntoArgumentsError::from)?)?,
        })
    }
}

#[derive(Debug, Error)]
pub enum CreateInterfaceError {
    /// An error while working with a Url.
    #[error("{0}")]
    Url(#[from] UrlError),
    /// An error while creating the user interface.
    #[error("{0}")]
    Ui(#[from] ui::Fault),
}

#[derive(Debug, Error)]
pub enum PullError {
    /// An error within the user interface.
    #[error("{0}")]
    Ui(#[from] ui::Fault),
}

#[derive(Debug, Error)]
pub enum PushError {
    #[error("{0}")]
    Ui(#[from] ui::Fault),
}

#[derive(Debug)]
pub(crate) struct Interface {
    /// Manages the user interface.
    ui: Terminal,
    inputs: VecDeque<Input>,
}

impl Interface {
    pub(crate) fn new(arguments: Arguments) -> Result<Self, CreateInterfaceError> {
        Terminal::new().map(|ui| {
            let mut inputs = VecDeque::new();

            if let Some(file) = arguments.file {
                inputs.push_back(Input::File(file));
            }

            Self {
                ui,
                inputs,
            }
        }).map_err(|e| e.into())
    }

    pub(crate) fn pull(&mut self) -> Result<Option<Input>, PullError> {
        if let Some(input) = self.inputs.pop_front() {
            Ok(Some(input))
        } else {
            Ok(self.ui.pull()?.map(Input::from))
        }
    }

    pub(crate) fn push(&mut self, output: Output<'_>) -> Result<bool, PushError> {
        match output {
            Output::Change(change) => self.ui.apply(change).map_err(|e| e.into()),
            Output::SetHeader(header) => {
                self.ui.write_header(header);
                Ok(true)
            }
        }
    }
}

/// An error occurred while converting a directory path to a URL.
#[derive(Debug, Error)]
#[error("while converting `{0}` to a URL")]
pub struct UrlError(String);

/// A URL that is a valid path.
///
/// Useful for preventing repeat translations between URL and path formats.
#[derive(Clone, Debug)]
pub struct PathUrl {
    /// The path.
    path: PathBuf,
    /// The URL.
    url: Url,
}

impl PathUrl {
    /// Joins `path` to `self`.
    pub(crate) fn join(&self, path: &str) -> Result<Self, UrlError> {
        let mut joined_path = self.path.clone();

        joined_path.push(path);
        joined_path.try_into()
    }

    /// Returns the language identification of the path.
    pub(crate) fn language_id(&self) -> &str {
        self.path
            .extension()
            .and_then(OsStr::to_str)
            .map_or("", |ext| match ext {
                "rs" => "rust",
                x => x,
            })
    }
}

impl AsRef<OsStr> for PathUrl {
    #[inline]
    #[must_use]
    fn as_ref(&self) -> &OsStr {
        self.path.as_ref()
    }
}

impl AsRef<Path> for PathUrl {
    #[inline]
    #[must_use]
    fn as_ref(&self) -> &Path {
        self.path.as_ref()
    }
}

impl AsRef<Url> for PathUrl {
    #[inline]
    #[must_use]
    fn as_ref(&self) -> &Url {
        &self.url
    }
}

impl Default for PathUrl {
    #[inline]
    #[must_use]
    fn default() -> Self {
        #[allow(clippy::result_expect_used)]
        // Default path should not fail and failure cannot be propogated.
        Self::try_from(PathBuf::default()).expect("creating default `PathUrl`")
    }
}

impl fmt::Display for PathUrl {
    #[inline]
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.url)
    }
}

impl TryFrom<PathBuf> for PathUrl {
    type Error = UrlError;

    #[inline]
    fn try_from(value: PathBuf) -> Result<Self, Self::Error> {
        Ok(Self {
            url: Url::from_directory_path(value.clone())
                .map_err(|_| UrlError(value.to_string_lossy().to_string()))?,
            path: value,
        })
    }
}

#[derive(Debug)]
pub(crate) enum Input {
    File(String),
    User(ui::Input),
}

impl From<ui::Input> for Input {
    fn from(value: ui::Input) -> Self {
        Input::User(value)
    }
}

#[derive(Debug)]
pub(crate) enum Output<'a> {
    Change(Change<'a>),
    SetHeader(String),
}

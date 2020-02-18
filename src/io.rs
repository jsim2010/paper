//! Implements the interface for all input and output to the application.
pub mod ui;

pub(crate) use ui::FlushError;

use {
    clap::ArgMatches,
    core::{
        convert::{TryFrom, TryInto},
        fmt,
        time::Duration,
    },
    log::LevelFilter,
    lsp_types::{ShowMessageParams, ShowMessageRequestParams},
    notify::{DebouncedEvent, RecommendedWatcher, RecursiveMode, Watcher},
    serde::Deserialize,
    std::{
        collections::VecDeque,
        env,
        ffi::OsStr,
        fs, io,
        path::{Path, PathBuf},
        sync::mpsc::{self, Receiver, TryRecvError},
    },
    thiserror::Error,
    ui::{CommandError, Selection, Size, Terminal},
    url::Url,
};

/// An error while parsing arguments.
#[derive(Debug, Error)]
pub enum IntoArgumentsError {
    /// An error determing the root directory.
    #[error("current working directory is invalid: {0}")]
    RootDir(#[from] io::Error),
    /// An error with a URL.
    #[error("root directory is invalid: {0}")]
    Url(#[from] UrlError),
}

/// An error while pushing output.
#[derive(Debug, Error)]
pub enum PushError {
    /// An error in the ui.
    #[error("{0}")]
    Ui(#[from] CommandError),
}

/// An error while pulling input.
#[derive(Debug, Error)]
pub enum PullError {
    /// An error from the ui.
    #[error("{0}")]
    Ui(#[from] CommandError),
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

/// An error while creating the interface.
#[derive(Debug, Error)]
pub enum CreateInterfaceError {
    /// An error while working with a Url.
    #[error("{0}")]
    Url(#[from] UrlError),
    /// An error while creating the user interface.
    #[error("{0}")]
    CreateUi(#[from] CommandError),
    /// An error while retrieving the home directory of the user.
    #[error("unable to determine home directory of user")]
    HomeDir,
    /// An error while creating the config file watcher.
    #[error("while creating config file watcher: {0}")]
    Watcher(#[from] notify::Error),
}

/// An error in the user interface that is recoverable.
///
/// Until a glitch is resolved, certain functionality may not be properly completed.
#[derive(Debug, Error)]
pub(crate) enum Glitch {
    /// Config file watcher disconnected.
    #[error("config file watcher disconnected")]
    WatcherConnection,
    /// Unable to read config file.
    #[error("unable to read config file: {0}")]
    ReadConfig(#[source] io::Error),
    /// Unable to convert config file to Config.
    #[error("config file invalid format: {0}")]
    ConfigFormat(#[from] toml::de::Error),
}

/// The interface.
#[derive(Debug)]
pub(crate) struct Interface {
    /// Manages the user interface.
    user_interface: Terminal,
    /// The inputs of the interface.
    inputs: VecDeque<Input>,
    /// Notifies `self` of any events to the config file.
    watcher: ConfigWatcher,
    /// The current configuration of the application.
    config: Config,
}

impl Interface {
    /// Creates a new interface.
    pub(crate) fn new(arguments: Arguments) -> Result<Self, CreateInterfaceError> {
        let config_file = dirs::home_dir()
            .ok_or(CreateInterfaceError::HomeDir)?
            .join(".config/paper.toml");
        let watcher = ConfigWatcher::new(&config_file)?;
        Terminal::new()
            .map(|user_interface| {
                let mut interface = Self {
                    user_interface,
                    inputs: VecDeque::new(),
                    watcher,
                    config: Config::default(),
                };

                interface.add_config_updates(config_file);
                interface
                    .inputs
                    .push_back(Input::User(Terminal::size().into()));

                if let Some(file) = arguments.file {
                    interface.inputs.push_back(Input::File(file));
                }

                interface
            })
            .map_err(|e| e.into())
    }

    /// Checks for updates to [`Config`] and adds any changes the changed settings list.
    fn add_config_updates(&mut self, config_file: PathBuf) {
        match self.config.update(config_file) {
            Ok(settings) => {
                self.inputs.append(
                    &mut settings
                        .iter()
                        .map(|setting| Input::Config(*setting))
                        .collect(),
                );
            }
            Err(glitch) => {
                self.inputs.push_back(Input::Glitch(glitch));
            }
        }
    }

    /// Pulls an [`Input`].
    pub(crate) fn pull(&mut self) -> Result<Option<Input>, PullError> {
        match self.watcher.notify.try_recv() {
            Ok(event) => {
                if let DebouncedEvent::Write(config_file) = event {
                    self.add_config_updates(config_file);
                }
            }
            Err(TryRecvError::Empty) => (),
            Err(TryRecvError::Disconnected) => {
                self.inputs
                    .push_back(Input::Glitch(Glitch::WatcherConnection));
            }
        }

        if let Some(input) = self.inputs.pop_front() {
            Ok(Some(input))
        } else {
            Ok(self.user_interface.pull()?.map(Input::from))
        }
    }

    /// Pushes `output`.
    pub(crate) fn push(&mut self, output: Output<'_>) -> Result<bool, PushError> {
        let mut keep_running = true;

        match output {
            Output::OpenDoc { text, .. } => {
                self.user_interface.open_doc(text)?;
            }
            Output::Wrap {
                is_wrapped,
                selection,
            } => {
                self.user_interface.wrap(is_wrapped, selection)?;
            }
            Output::EditDoc {
                new_text,
                selection,
            } => {
                self.user_interface.edit(&new_text, selection)?;
            }
            Output::MoveSelection { selection } => {
                self.user_interface.move_selection(selection)?;
            }
            Output::SetHeader { header } => {
                self.user_interface.set_header(header)?;
            }
            Output::Notify { message } => {
                self.user_interface.notify(&message)?;
            }
            Output::Question { request } => {
                self.user_interface.question(&request)?;
            }
            Output::StartIntake { title } => {
                self.user_interface.start_intake(title)?;
            }
            Output::Reset { selection } => {
                self.user_interface.reset(selection)?;
            }
            Output::Resize { size } => {
                self.user_interface.resize(size);
            }
            Output::Write { ch } => {
                self.user_interface.write(ch)?;
            }
            Output::Quit => {
                keep_running = false;
            }
        }

        Ok(keep_running)
    }

    /// Flushes the application I/O.
    pub(crate) fn flush(&mut self) -> Result<(), FlushError> {
        self.user_interface.flush()
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

/// Signifies any configurable parameter of the application.
#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct Config {
    /// If the application wraps long lines.
    wrap: Wrap,
    /// The [`LevelFilter`] of the starship library.
    starship_log: StarshipLog,
}

impl Config {
    /// Reads the config file into a `Config`.
    fn read(config_file: PathBuf) -> Result<Self, Glitch> {
        fs::read_to_string(config_file)
            .map_err(Glitch::ReadConfig)
            .and_then(|config_string| toml::from_str(&config_string).map_err(Glitch::ConfigFormat))
    }

    /// Updates `self` to match paper's config file, returning any changed [`Setting`]s.
    fn update(&mut self, config_file: PathBuf) -> Result<VecDeque<Setting>, Glitch> {
        let mut settings = VecDeque::new();
        let config = Self::read(config_file)?;

        if self.wrap != config.wrap {
            self.wrap = config.wrap;
            settings.push_back(Setting::Wrap(self.wrap.0));
        }

        if self.starship_log != config.starship_log {
            self.starship_log = config.starship_log;
            settings.push_back(Setting::StarshipLog(self.starship_log.0));
        }

        Ok(settings)
    }
}

macro_rules! def_config {
    ($name:ident: $ty:ty = $default:expr) => {
        #[derive(Debug, Deserialize, PartialEq)]
        struct $name($ty);

        impl Default for $name {
            fn default() -> Self {
                Self($default)
            }
        }
    };
}

def_config!(Wrap: bool = false);
def_config!(StarshipLog: LevelFilter = LevelFilter::Off);

/// Signifies a configuration.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Setting {
    /// If the document shall wrap long text.
    Wrap(bool),
    /// The level at which starship records shall be logged.
    StarshipLog(LevelFilter),
}

/// Triggers a callback if the config file is updated.
struct ConfigWatcher {
    /// Watches for events on the config file.
    #[allow(dead_code)] // watcher must must be owned to avoid being dropped.
    watcher: RecommendedWatcher,
    /// Receives events generated by `watcher`.
    notify: Receiver<DebouncedEvent>,
}

impl ConfigWatcher {
    /// Creates a new [`ConfigWatcher`].
    fn new(config_file: &PathBuf) -> Result<Self, notify::Error> {
        let (tx, notify) = mpsc::channel();
        let mut watcher = notify::watcher(tx, Duration::from_secs(0))?;

        if config_file.is_file() {
            watcher.watch(config_file, RecursiveMode::NonRecursive)?;
        }

        Ok(Self { watcher, notify })
    }
}

impl fmt::Debug for ConfigWatcher {
    fn fmt(&self, _f: &mut fmt::Formatter<'_>) -> fmt::Result {
        Ok(())
    }
}

/// An input.
#[derive(Debug)]
pub(crate) enum Input {
    /// A file to be opened.
    File(String),
    /// An input from the user.
    User(ui::Input),
    /// A configuration.
    Config(Setting),
    /// A glitch.
    Glitch(Glitch),
}

impl From<ui::Input> for Input {
    fn from(value: ui::Input) -> Self {
        Self::User(value)
    }
}

/// An output.
#[derive(Debug)]
pub(crate) enum Output<'a> {
    /// Opens a document.
    OpenDoc {
        /// The URL of the document.
        url: &'a PathUrl,
        /// The language id of the document.
        language_id: &'a str,
        /// The version of the document.
        version: i64,
        /// The full text of the document
        text: &'a str,
    },
    /// Sets the wrapping of the text.
    Wrap {
        /// If the text shall be wrapped.
        is_wrapped: bool,
        /// The selection.
        selection: &'a Selection,
    },
    /// Edits the document.
    EditDoc {
        /// The new text.
        new_text: String,
        /// The selection.
        selection: &'a Selection,
    },
    /// Moves the selection.
    MoveSelection {
        /// The selection.
        selection: &'a Selection,
    },
    /// Sets the header of the application.
    SetHeader {
        /// The header.
        header: String,
    },
    /// Notifies the user of a message.
    Notify {
        /// The message.
        message: ShowMessageParams,
    },
    /// Asks the user a question.
    Question {
        /// The request to be answered.
        request: ShowMessageRequestParams,
    },
    /// Adds an intake box.
    StartIntake {
        /// The prompt of the intake box.
        title: String,
    },
    /// Resets the output of the application.
    Reset {
        /// The selection.
        selection: &'a Selection,
    },
    /// Resizes the application display.
    Resize {
        /// The new [`Size`].
        size: Size,
    },
    /// Write a [`char`] to the application.
    Write {
        /// The [`char`] to be written.
        ch: char,
    },
    /// Quit the application.
    Quit,
}

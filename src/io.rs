//! Implements the interface for all input and output to the application.
pub mod config;
pub mod logging;
pub mod lsp;
pub mod ui;

pub(crate) use ui::FlushCommandsError;

use {
    config::ChangeFilter,
    crate::kyoo::{Consumer, ConsumeError},
    clap::ArgMatches,
    core::{
        cell::RefCell,
        convert::{TryFrom, TryInto},
        fmt,
    },
    log::{error, LevelFilter},
    logging::LogConfig,
    lsp::{CreateLangClientError, Fault, LspServer, SendNotificationError},
    lsp_types::{MessageType, ShowMessageParams, ShowMessageRequestParams, TextEdit},
    serde::Deserialize,
    starship::{context::Context, print},
    std::{
        collections::{hash_map::Entry, HashMap},
        env,
        ffi::OsStr,
        fs,
        io::{self, ErrorKind},
        path::{Path, PathBuf},
        rc::Rc,
    },
    thiserror::Error,
    toml::{value::Table, Value},
    ui::{InitTerminalError, CommandError, Selection, SelectionConversionError, Size, Terminal},
    url::Url,
};

/// Defines how to initialize `paper`.
#[derive(Clone, Debug, Default)]
pub struct Arguments<'a> {
    /// The file to be viewed.
    ///
    /// [`None`] indicates that no file will be viewed.
    ///
    /// [`None`]: https://doc.rust-lang.org/std/option/enum.Option.html#variant.None
    pub file: Option<&'a str>,
}

impl<'a> From<&'a ArgMatches<'a>> for Arguments<'a> {
    #[inline]
    fn from(value: &'a ArgMatches<'a>) -> Self {
        Self {
            file: value.value_of("file"),
        }
    }
}

/// An error creating an [`Interface`].
#[derive(Debug, Error)]
pub enum CreateInterfaceError {
    /// An error creating drain.
    #[error("{0}")]
    CreateDrain(#[from] CreateInterfaceDrainError),
    /// An error initilizing the [`Terminal`].
    #[error("initializing terminal: {0}")]
    InitTerminal(#[from] InitTerminalError),
    /// An error determing the root directory.
    #[error("current working directory is invalid: {0}")]
    RootDir(#[from] io::Error),
    /// An error while working with a Url.
    #[error("{0}")]
    Url(#[from] UrlError),
    /// An error while reading a file.
    #[error("{0}")]
    CreateFile(#[from] CreateFileError),
    /// An error while creating the logging configuration.
    #[error("{0}")]
    CreateLogConfig(#[from] logging::Fault),
}

/// An error creating interface drain.
#[derive(Debug, Error)]
pub enum CreateInterfaceDrainError {
    /// An error determining the home directory of the current user.
    #[error("home directory of current user is unknown")]
    HomeDir,
    /// An error creating the config file watcher.
    #[error("while creating config file watcher: {0}")]
    Watcher(#[from] notify::Error),
}

/// An error while writing output.
#[derive(Debug, Error)]
pub enum WriteOutputError {
    /// An error in the ui.
    #[error("{0}")]
    Ui(#[from] CommandError),
    /// An error in the lsp.
    #[error("{0}")]
    Lsp(#[from] Fault),
    /// An error while converting from a [`Selection`].
    #[error("{0}")]
    SelectionConversion(#[from] SelectionConversionError),
    /// Failed to create language client.
    #[error("{0}")]
    CreateLangClient(#[from] CreateLangClientError),
    /// Failed to send notification.
    #[error("{0}")]
    SendNotification(#[from] SendNotificationError),
    /// An error while reading a file.
    #[error("{0}")]
    CreateFile(#[from] CreateFileError),
    /// An error while configuring the logger.
    #[error("{0}")]
    Log(#[from] logging::Fault),
}

/// An error while pulling input.
#[derive(Debug, Error)]
pub enum ReadInputError {
    /// An error from the ui.
    #[error("{0}")]
    Ui(#[from] CommandError),
}

/// An error while creating a file.
#[derive(Debug, Error)]
pub enum CreateFileError {
    /// An error while generating the URL of the file.
    #[error("{0}")]
    CreateUrl(#[from] UrlError),
    /// An error while reading the text of the file.
    #[error("{0}")]
    ReadFile(#[from] ReadFileError),
    /// An error sending an input.
    #[error("{0}")]
    Send(#[from] crossbeam_channel::SendError<Input>)
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

/// An error in the user interface that is recoverable.
///
/// Until a glitch is resolved, certain functionality may not be properly completed.
#[derive(Debug, Error)]
pub enum Glitch {
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

/// The interface between the application and all external components.
#[derive(Debug)]
pub(crate) struct Interface {
    /// Notifies `self` of any events to the config file.
    config_drain: ChangeFilter,
    rx: crossbeam_channel::Receiver<Input>,
    /// Manages the user interface.
    user_interface: RefCell<Terminal>,
    /// The [`LspServer`]s managed by the application.
    lsp_servers: Rc<RefCell<HashMap<String, Option<LspServer>>>>,
    /// The root directory of the application.
    root_dir: PathUrl,
    /// The configuration of the logger.
    log_config: LogConfig,
    tx: crossbeam_channel::Sender<Input>,
}

impl Interface {
    /// Creates a new interface.
    pub(crate) fn new(arguments: Arguments<'_>) -> Result<Self, CreateInterfaceError> {
        let mut user_interface = Terminal::new();
        user_interface.init()?;
        let (tx, rx) = crossbeam_channel::unbounded();
        let config_file = dirs::home_dir()
            .ok_or(CreateInterfaceDrainError::HomeDir)?
            .join(".config/paper.toml");
        let root_dir = PathUrl::try_from(env::current_dir().map_err(CreateInterfaceError::from)?)?;

        let interface = Self {
            config_drain: ChangeFilter::new(&config_file),
            rx,
            user_interface: RefCell::new(user_interface),
            lsp_servers: Rc::new(RefCell::new(HashMap::default())),
            root_dir,
            log_config: LogConfig::new()?,
            tx,
        };

        if let Some(file) = arguments.file {
            interface.add_file(file).unwrap();
        }

        Ok(interface)
    }

    /// Pushes `output`.
    pub(crate) fn write(&self, output: &Output<'_>) -> Result<bool, WriteOutputError> {
        let mut keep_running = true;
        let mut lsp_servers = self.lsp_servers.borrow_mut();
        let mut user_interface = self.user_interface.borrow_mut();

        match output {
            Output::GetFile { path } => {
                self.add_file(path)?;
            }
            Output::EditDoc { url, edit } => {
                let language_id = url.language_id();

                if let DocEdit::Open { .. } = edit {
                    if let Entry::Vacant(entry) = lsp_servers.entry(language_id.to_string()) {
                        let _ = entry.insert(LspServer::new(language_id, &self.root_dir)?);
                    }
                }

                match edit {
                    DocEdit::Open { text, version } => {
                        if let Some(Some(lsp_server)) = lsp_servers.get_mut(language_id) {
                            lsp_server.did_open(&url, language_id, *version, text)?;
                        }

                        user_interface.open_doc(text)?;
                    }
                    DocEdit::Save { text } => {
                        if let Some(lsp_server) = lsp_servers.get_mut(url.language_id()).map(Option::as_mut).flatten() {
                            if let Err(error) = lsp_server.will_save(url) {
                                user_interface.notify(&error.into())?;
                            }
                        }

                        user_interface.notify(&match fs::write(url, text) {
                            Ok(..) => ShowMessageParams {
                                typ: MessageType::Info,
                                message: format!("Saved document `{}`", url),
                            },
                            Err(error) => ShowMessageParams {
                                typ: MessageType::Error,
                                message: format!("Failed to save document `{}`: {}", url, error),
                            },
                        })?;
                    }
                    DocEdit::Change {
                        new_text,
                        selection,
                        version,
                        text,
                    } => {
                        user_interface.edit(new_text, selection)?;

                        if let Some(Some(lsp_server)) = lsp_servers.get_mut(url.language_id())
                        {
                            if let Err(error) = lsp_server.did_change(
                                url,
                                *version,
                                text,
                                TextEdit::new(selection.range()?, new_text.to_string()),
                            ) {
                                user_interface.notify(&error.into())?;
                            }
                        }
                    }
                    DocEdit::Close => {
                        if let Some(Some(lsp_server)) = lsp_servers.get_mut(url.language_id())
                        {
                            if let Err(error) = lsp_server.did_close(url) {
                                error!(
                                    "failed to inform language server process about closing: {}",
                                    error,
                                );
                            }
                        }
                    }
                }
            }
            Output::Wrap {
                is_wrapped,
                selection,
            } => {
                user_interface.wrap(*is_wrapped, selection)?;
            }
            Output::MoveSelection { selection } => {
                user_interface.move_selection(selection)?;
            }
            Output::UpdateHeader => {
                let mut context = Context::new_with_dir(ArgMatches::new(), &self.root_dir);

                // config will always be Some after Context::new_with_dir().
                if let Some(mut config) = context.config.config.clone() {
                    if let Some(table) = config.as_table_mut() {
                        let _ = table.insert("add_newline".to_string(), Value::Boolean(false));

                        if let Some(line_break) = table
                            .entry("line_break")
                            .or_insert(Value::Table(Table::new()))
                            .as_table_mut()
                        {
                            let _ = line_break.insert("disabled".to_string(), Value::Boolean(true));
                        }
                    }

                    context.config.config = Some(config);
                }
                user_interface.set_header(print::get_prompt(context))?;
            }
            Output::Notify { message } => {
                user_interface.notify(message)?;
            }
            Output::Question { request } => {
                user_interface.question(request)?;
            }
            Output::StartIntake { title } => {
                user_interface.start_intake(title.to_string())?;
            }
            Output::Reset { selection } => {
                user_interface.reset(selection)?;
            }
            Output::Resize { size } => {
                user_interface.resize(size.clone());
            }
            Output::Write { ch } => {
                user_interface.write(*ch)?;
            }
            Output::Log { starship_level } => {
                self.log_config.writer()?.starship_level = *starship_level;
            }
            Output::Quit => {
                // TODO: Force drain.iter() to return None.
                keep_running = false;
            }
        }

        Ok(keep_running)
    }

    /// Generates an Input for opening the file at `path`.
    fn add_file(&self, path: &str) -> Result<(), CreateFileError> {
        let url = self.root_dir.join(path)?;

        self.tx.send(Input::File {
            text: fs::read_to_string(&url).map_err(|error| ReadFileError {
                file: url.to_string(),
                error: error.kind(),
            })?,
            url,
        })?;
        error!("added file");
        Ok(())
    }

    /// Flushes the application I/O.
    pub(crate) fn flush(&self) -> Result<(), FlushCommandsError> {
        self.user_interface.borrow_mut().flush()
    }
}

impl Consumer for Interface {
    type Record = Input;

    fn can_consume(&self) -> bool {
        !self.rx.is_empty() && self.user_interface.borrow().can_consume() && self.config_drain.can_consume()
    }

    fn consume(&self) -> Result<Self::Record, ConsumeError> {
        let mut active_sink = None;

        while active_sink.is_none() {
            if self.user_interface.borrow().can_consume() {
                active_sink = Some(SinkId::Ui);
            } else if self.config_drain.can_consume() {
                active_sink = Some(SinkId::Watcher);
            } else if !self.rx.is_empty() {
                active_sink = Some(SinkId::Own);
            }
        }

        match active_sink {
            Some(SinkId::Ui) => self.user_interface.borrow().consume().map(|x| x.into()).map_err(|e| e.into()),
            Some(SinkId::Watcher) => self.config_drain.consume().map(|x| x.into()).map_err(|e| e.into()),
            Some(SinkId::Own) => self.rx.recv().map_err(|_| ConsumeError),
            None => Err(ConsumeError),
        }
    }
}

#[derive(Debug)]
enum SinkId {
    Ui,
    Watcher,
    Own,
}

/// An error receiving input.
#[derive(Debug, Error)]
pub enum RecvInputError {
    /// An error receiving config input.
    #[error("{0}")]
    Watcher(#[from] ConfigError),
    /// An error receiving arguments.
    #[error("{0}")]
    Own(#[from] ConsumeError),
    /// An invalid error.
    #[error("invalid")]
    Invalid,
}

/// An error occurred while converting a directory path to a URL.
#[derive(Debug, Error)]
#[error("while converting `{0}` to a URL")]
pub struct UrlError(String);

/// A URL that is a valid path.
///
/// Useful for preventing repeat translations between URL and path formats.
#[derive(Clone, Debug, PartialEq)]
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
pub enum Setting {
    /// If the document shall wrap long text.
    Wrap(bool),
    /// The level at which starship records shall be logged.
    StarshipLog(LevelFilter),
}

/// An error with the config.
#[derive(Debug, Error)]
pub enum ConfigError {
    /// An error sending an input.
    #[error("{0}")]
    Send(#[from] crossbeam_channel::SendError<Input>),
    /// An error receiving an input.
    #[error("{0}")]
    Rx(#[from] ConsumeError),
}

/// An input.
#[derive(Debug)]
pub enum Input {
    /// A file to be opened.
    File {
        /// The URL of the file.
        url: PathUrl,
        /// The text of the file.
        text: String,
    },
    /// An input from the user.
    User(ui::Input),
    /// A configuration.
    Config(config::Setting),
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
    /// Retrieves the URL and text of a file.
    GetFile {
        /// The relative path of the file.
        path: String,
    },
    /// Edits a document.
    EditDoc {
        /// The URL of the document.
        url: PathUrl,
        /// The edit to be performed.
        edit: DocEdit<'a>,
    },
    /// Sets the wrapping of the text.
    Wrap {
        /// If the text shall be wrapped.
        is_wrapped: bool,
        /// The selection.
        selection: &'a Selection,
    },
    /// Moves the selection.
    MoveSelection {
        /// The selection.
        selection: &'a Selection,
    },
    /// Sets the header of the application.
    UpdateHeader,
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
    /// Configure the logger.
    Log {
        /// The level for starship logs.
        starship_level: LevelFilter,
    },
}

/// Edits a document.
#[derive(Debug)]
pub(crate) enum DocEdit<'a> {
    /// Opens a document.
    Open {
        /// The version of the document.
        version: i64,
        /// The full text of the document
        text: &'a str,
    },
    /// Saves the document.
    Save {
        /// The text of the document.
        text: &'a str,
    },
    /// Edits the document.
    Change {
        /// The new text.
        new_text: String,
        /// The selection.
        selection: &'a Selection,
        /// The version.
        version: i64,
        /// The full text of the document.
        text: &'a str,
    },
    /// Closes the document.
    Close,
}

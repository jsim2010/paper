//! Implements the interface for all input and output to the application.
pub mod config;
pub mod logging;
pub mod lsp;
pub mod ui;

use {
    clap::ArgMatches,
    config::{ChangeFilter, ConsumeChangeError},
    core::{
        cell::RefCell,
        convert::{TryFrom, TryInto},
        fmt,
    },
    log::{error, LevelFilter},
    logging::LogConfig,
    lsp::{CreateLangClientError, Fault, LspServer, SendNotificationError},
    lsp_types::{MessageType, ShowMessageParams, ShowMessageRequestParams, TextEdit},
    market::{Consumer, Producer, Queue},
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
    ui::{
        BodySize, CommandError, CreateTerminalError, ProduceTerminalOutputError, Selection,
        SelectionConversionError, Terminal,
    },
    url::Url,
};

/// A configuration of the initialization of a [`Paper`].
///
/// [`Paper`]: ../struct.Paper.html
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
///
/// [`Interface`]: struct.Interface.html
#[derive(Debug, Error)]
pub enum CreateInterfaceError {
    /// An error creating a [`Terminal`].
    ///
    /// [`Terminal`]: ui/struct.Terminal.html
    #[error("creating terminal: {0}")]
    CreateTerminal(#[from] CreateTerminalError),
    /// An error determining the home directory of the current user.
    #[error("home directory of current user is unknown")]
    HomeDir,
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

/// An error while writing output.
#[derive(Debug, Error)]
pub enum ProduceOutputError {
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
    /// Produce error from ui.
    #[error("{0}")]
    UiProduce(#[from] ProduceTerminalOutputError),
    /// Add to queue.
    #[error("{0}")]
    Queue(#[source] <Queue<Input> as Producer<'static>>::Error),
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
    #[error("sending error")]
    Send,
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

/// An error consuming input.
#[derive(Debug, Error)]
pub enum ConsumeInputError {
    /// Quit.
    #[error("")]
    Quit,
    /// Ui.
    #[error("")]
    Ui(#[from] ui::ConsumeInputError),
    /// Add to queue.
    #[error("{0}")]
    Queue(#[source] <Queue<Input> as Producer<'static>>::Error),
    /// Change
    #[error("")]
    Change(#[from] ConsumeChangeError),
    /// Consume.
    #[error("")]
    Consume(#[source] <Queue<Input> as Consumer>::Error),
}

/// The interface between the application and all external components.
#[derive(Debug)]
pub(crate) struct Interface {
    /// Notifies `self` of any events to the config file.
    config_drain: ChangeFilter,
    /// Queues [`Input`]s.
    queue: Queue<Input>,
    /// Manages the user interface.
    user_interface: Terminal,
    /// The [`LspServer`]s managed by the application.
    lsp_servers: Rc<RefCell<HashMap<String, Option<LspServer>>>>,
    /// The root directory of the application.
    root_dir: PathUrl,
    /// The configuration of the logger.
    log_config: LogConfig,
}

impl Interface {
    /// Creates a new interface.
    pub(crate) fn new(arguments: &Arguments<'_>) -> Result<Self, CreateInterfaceError> {
        let interface = Self {
            // Create log_config first as this is where the logger is initialized.
            log_config: LogConfig::new()?,
            config_drain: ChangeFilter::new(
                &dirs::home_dir()
                    .ok_or(CreateInterfaceError::HomeDir)?
                    .join(".config/paper.toml"),
            ),
            queue: Queue::new(),
            user_interface: Terminal::new()?,
            lsp_servers: Rc::new(RefCell::new(HashMap::default())),
            root_dir: PathUrl::try_from(env::current_dir().map_err(CreateInterfaceError::from)?)?,
        };

        if let Some(file) = arguments.file {
            interface.add_file(file)?;
        }

        Ok(interface)
    }

    /// Generates an Input for opening the file at `path`.
    fn add_file(&self, path: &str) -> Result<(), CreateFileError> {
        let url = self.root_dir.join(path)?;

        self.queue
            .produce(Input::File {
                text: fs::read_to_string(&url).map_err(|error| ReadFileError {
                    file: url.to_string(),
                    error: error.kind(),
                })?,
                url,
            })
            .map_err(|_| CreateFileError::Send)?;
        Ok(())
    }

    /// Edits the doc at `url`.
    fn edit_doc(&self, url: PathUrl, edit: DocEdit<'_>) -> Result<(), ProduceOutputError> {
        let language_id = url.language_id();
        let mut lsp_servers = self.lsp_servers.borrow_mut();

        if let DocEdit::Open { .. } = edit {
            if let Entry::Vacant(entry) = lsp_servers.entry(language_id.to_string()) {
                let _ = entry.insert(LspServer::new(language_id, &self.root_dir)?);
            }
        }

        match edit {
            DocEdit::Open { text, version } => {
                if let Some(Some(lsp_server)) = lsp_servers.get_mut(language_id) {
                    lsp_server.did_open(&url, language_id, version, text)?;
                }

                self.user_interface.produce(ui::Output::OpenDoc { text })?;
            }
            DocEdit::Save { text } => {
                if let Some(lsp_server) = lsp_servers
                    .get_mut(url.language_id())
                    .map(Option::as_mut)
                    .flatten()
                {
                    if let Err(error) = lsp_server.will_save(&url) {
                        self.user_interface.produce(ui::Output::Notify {
                            message: error.into(),
                        })?;
                    }
                }

                self.user_interface.produce(ui::Output::Notify {
                    message: match fs::write(&url, text) {
                        Ok(..) => ShowMessageParams {
                            typ: MessageType::Info,
                            message: format!("Saved document `{}`", url),
                        },
                        Err(error) => ShowMessageParams {
                            typ: MessageType::Error,
                            message: format!("Failed to save document `{}`: {}", url, error),
                        },
                    },
                })?;
            }
            DocEdit::Change {
                new_text,
                selection,
                version,
                text,
            } => {
                self.user_interface.produce(ui::Output::Edit {
                    new_text: new_text.clone(),
                    selection,
                })?;

                if let Some(Some(lsp_server)) = lsp_servers.get_mut(url.language_id()) {
                    if let Err(error) = lsp_server.did_change(
                        url,
                        version,
                        text,
                        TextEdit::new(selection.range()?, new_text),
                    ) {
                        self.user_interface.produce(ui::Output::Notify {
                            message: error.into(),
                        })?;
                    }
                }
            }
            DocEdit::Close => {
                if let Some(Some(lsp_server)) = lsp_servers.get_mut(url.language_id()) {
                    if let Err(error) = lsp_server.did_close(url) {
                        error!(
                            "failed to inform language server process about closing: {}",
                            error,
                        );
                    }
                }
            }
        }

        Ok(())
    }
}

impl Consumer for Interface {
    type Good = Input;
    type Error = ConsumeInputError;

    fn can_consume(&self) -> bool {
        self.queue.can_consume()
            || self.user_interface.can_consume()
            || self.config_drain.can_consume()
    }

    fn consume(&self) -> Result<Self::Good, Self::Error> {
        while !self.queue.can_consume() {
            if let Some(ui_input) = self.user_interface.optional_consume()? {
                self.queue
                    .produce(ui_input.into())
                    .map_err(Self::Error::Queue)?;
            }

            if let Some(config_input) = self
                .config_drain
                .optional_consume()
                .map_err(Self::Error::Change)?
            {
                self.queue
                    .produce(config_input)
                    .map_err(Self::Error::Queue)?;
            }
        }

        let mut good = Ok(self.queue.consume().map_err(Self::Error::Consume)?);

        if let Ok(Input::Quit) = good {
            good = Err(Self::Error::Quit);
        }

        good
    }
}

impl<'a> Producer<'a> for Interface {
    type Good = Output<'a>;
    type Error = ProduceOutputError;

    fn produce(&self, output: Self::Good) -> Result<(), Self::Error> {
        match output {
            Output::GetFile { path } => {
                self.add_file(&path)?;
            }
            Output::EditDoc { url, edit } => {
                self.edit_doc(url, edit)?;
            }
            Output::Wrap {
                is_wrapped,
                selection,
            } => {
                self.user_interface.produce(ui::Output::Wrap {
                    is_wrapped,
                    selection,
                })?;
            }
            Output::MoveSelection { selection } => {
                self.user_interface
                    .produce(ui::Output::MoveSelection { selection })?;
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
                self.user_interface.produce(ui::Output::SetHeader {
                    header: print::get_prompt(context),
                })?;
            }
            Output::Notify { message } => {
                self.user_interface
                    .produce(ui::Output::Notify { message })?;
            }
            Output::Question { request } => {
                self.user_interface
                    .produce(ui::Output::Question { request })?;
            }
            Output::StartIntake { title } => {
                self.user_interface
                    .produce(ui::Output::StartIntake { title })?;
            }
            Output::Reset { selection } => {
                self.user_interface
                    .produce(ui::Output::Reset { selection })?;
            }
            Output::Resize { size } => {
                self.user_interface.produce(ui::Output::Resize { size })?;
            }
            Output::Write { ch } => {
                self.user_interface.produce(ui::Output::Write { ch })?;
            }
            Output::Log { starship_level } => {
                self.log_config.writer()?.starship_level = starship_level;
            }
            Output::Quit => {
                self.queue
                    .produce(Input::Quit)
                    .map_err(Self::Error::Queue)?;
            }
        }

        Ok(())
    }
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
    /// Quit.
    Quit,
}

impl From<ui::Input> for Input {
    #[inline]
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
        size: BodySize,
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

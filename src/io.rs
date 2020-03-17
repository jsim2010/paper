//! Implements the interface for all input and output to the application.
pub mod config;
pub mod logging;
pub mod lsp;
pub mod ui;

use {
    clap::ArgMatches,
    config::{ConsumeSettingError, CreateSettingConsumerError, Setting, SettingConsumer},
    core::{
        convert::{TryFrom, TryInto},
        fmt::{self, Display},
    },
    enum_map::Enum,
    log::{error, LevelFilter},
    logging::LogManager,
    lsp::{CreateLangClientError, Fault, LanguageTool, ClientMessage, ToolMessage, SendNotificationError, ServerMessage},
    lsp_types::{MessageType, ShowMessageParams, ShowMessageRequestParams},
    market::{ReadGoodError, Consumer, Producer, UnlimitedQueue},
    starship::{context::Context, print},
    std::{
        env,
        ffi::OsStr,
        fs,
        io::{self, ErrorKind},
        path::{Path, PathBuf},
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
    /// An error creating the setting consumer.
    #[error("")]
    CreateConfig(#[from] CreateSettingConsumerError),
    /// Failed to create language client.
    #[error("{0}")]
    CreateLangClient(#[from] CreateLangClientError),
    /// An error with client.
    #[error("")]
    Client(#[from] lsp::CreateLanguageToolError),
}

/// An error while writing output.
#[derive(Debug, Error)]
pub enum ProduceOutputError {
    /// An error with client.
    #[error("")]
    Client(#[from] lsp::ProduceProtocolError),
    /// An error with client.
    #[error("")]
    OldClient(#[from] lsp::EditLanguageToolError),
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
    Queue(#[source] <UnlimitedQueue<Input> as Producer<'static>>::Error),
    /// Change
    #[error("")]
    Change(#[source] <UnlimitedQueue<Setting> as Consumer>::Error),
    /// Consume.
    #[error("")]
    Consume(#[source] <UnlimitedQueue<Input> as Consumer>::Error),
    /// Setting.
    #[error("{0}")]
    Setting(#[from] ConsumeSettingError),
    /// Produce
    #[error("")]
    Produce(#[from] lsp::ProduceProtocolError),
    #[error("")]
    Io(#[from] io::Error),
    #[error("")]
    Read(#[from] ReadGoodError<lsp::utils::Message>),
}

/// The interface between the application and all external components.
#[derive(Debug)]
pub(crate) struct Interface {
    /// Manages the user interface.
    user_interface: Terminal,
    /// Notifies `self` of any events to the config file.
    setting_consumer: SettingConsumer,
    /// Queues [`Input`]s.
    queue: UnlimitedQueue<Input>,
    /// The interface of the application with all language servers.
    language_tool: LanguageTool,
    /// The root directory of the application.
    root_dir: PathUrl,
    /// The configuration of the logger.
    log_manager: LogManager,
}

impl Interface {
    /// Creates a new interface.
    pub(crate) fn new(arguments: &Arguments<'_>) -> Result<Self, CreateInterfaceError> {
        // Create log_manager first as this is where the logger is initialized.
        let log_manager = LogManager::new()?;
        let root_dir = PathUrl::try_from(env::current_dir().map_err(CreateInterfaceError::from)?)?;

        let interface = Self {
            log_manager,
            setting_consumer: SettingConsumer::new(
                &dirs::home_dir()
                    .ok_or(CreateInterfaceError::HomeDir)?
                    .join(".config/paper.toml"),
            )?,
            queue: UnlimitedQueue::new(),
            user_interface: Terminal::new()?,
            language_tool: LanguageTool::new(&root_dir)?,
            root_dir,
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
    fn edit_doc(&self, url: &PathUrl, edit: &DocEdit<'_>) -> Result<(), ProduceOutputError> {
        match edit {
            DocEdit::Open { text, .. } => {
                self.user_interface.produce(ui::Output::OpenDoc { text })?;
            }
            DocEdit::Save { text } => {
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
                ..
            } => {
                self.user_interface.produce(ui::Output::Edit {
                    new_text: new_text.to_string(),
                    selection,
                })?;
            }
            DocEdit::Close => {}
        }

        Ok(())
    }
}

impl Consumer for Interface {
    type Good = Input;
    type Error = ConsumeInputError;

    fn consume(&self) -> Result<Option<Self::Good>, Self::Error> {
        if let Some(lang_input) = self.language_tool.consume()? {
            match lang_input.message {
                ServerMessage::Initialize => {
                    self.language_tool.produce(ToolMessage{
                        language_id: lang_input.language_id,
                        message: ClientMessage::Initialized,
                    })?;
                }
                ServerMessage::Request{id} => {
                    self.language_tool.produce(ToolMessage{language_id: lang_input.language_id, message: ClientMessage::RegisterCapability{id}})?;
                }
                ServerMessage::Shutdown => {}
            }

            Ok(None)
        } else if let Some(ui_input) = self.user_interface.consume()? {
            Ok(Some(Self::Good::from(ui_input)))
        } else if let Some(setting) = self.setting_consumer.consume()? {
            Ok(Some(Self::Good::from(setting)))
        } else {
            self.queue
                .consume().map_err(Self::Error::Consume)
        }
    }
}

impl Drop for Interface {
    fn drop(&mut self) {
        for language_id in self.language_tool.language_ids() {
            if let Err(error) = self.language_tool.produce(ToolMessage{language_id, message: ClientMessage::Shutdown}) {
                error!("Failed to send shutdown message to {} language server: {}", language_id, error);
            }
        }

        // TODO: Need to check for reception from all clients.
        while let Some(lang_input) = self.language_tool.consume().expect("waiting for shutdown") {
            if let ServerMessage::Shutdown = lang_input.message {
                break;
            }
        }

        for language_id in self.language_tool.language_ids() {
            if let Err(error) = self.language_tool.produce(ToolMessage{language_id, message: ClientMessage::Exit}) {
                error!("Failed to send exit message to {} language server: {}", language_id, error);
            }

            // TODO: This should probably be a consume call.
            #[allow(clippy::indexing_slicing)] // enum_map ensures indexing will not fail.
            let server = &self.language_tool.clients[language_id];

            if let Err(error) = server.borrow_mut().server.wait() {
                error!("Failed to wait for {} language server process to finish: {}", language_id, error);
            }
        }
    }
}

impl<'a> Producer<'a> for Interface {
    type Good = Output<'a>;
    type Error = ProduceOutputError;

    fn produce(&self, output: Self::Good) -> Result<(), Self::Error> {
        if let Ok(protocol) = ToolMessage::try_from(output.clone()) {
            if let Err(error) = self.language_tool.produce(protocol) {
                self.user_interface.produce(ui::Output::Notify {
                    message: error.into(),
                })?;
            }
        }

        match output {
            Output::GetFile { path } => {
                self.add_file(&path)?;
            }
            Output::EditDoc { url, edit } => {
                self.edit_doc(&url, edit.as_ref())?;
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
                self.log_manager
                    .produce(logging::Output::StarshipLevel(starship_level))?;
            }
            Output::Quit => {
                self.queue.close();
            }
        }

        Ok(())
    }
}

/// An error occurred while converting a directory path to a URL.
#[derive(Debug, Error)]
#[error("while converting `{0}` to a URL")]
pub struct UrlError(String);

/// The language ids supported by `paper`.
#[derive(Clone, Copy, Debug, Enum, Eq, Hash, PartialEq)]
pub(crate) enum LanguageId {
    /// The rust language.
    Rust,
}

impl LanguageId {
    /// Returns the server cmd for `self`.
    #[allow(clippy::missing_const_for_fn)] // For stable rust, match is not allowed in const fn.
    fn server_cmd(&self) -> &str {
        match self {
            Self::Rust => "rls",
        }
    }
}

impl Display for LanguageId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}",
            match self {
                Self::Rust => "rust",
            }
        )
    }
}

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

impl Display for PathUrl {
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
    Setting(Setting),
    /// A glitch.
    Glitch(Glitch),
}

impl From<ui::Input> for Input {
    #[inline]
    fn from(value: ui::Input) -> Self {
        Self::User(value)
    }
}

impl From<Setting> for Input {
    #[inline]
    fn from(value: Setting) -> Self {
        Self::Setting(value)
    }
}

/// An output.
#[derive(Clone, Debug)]
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
        edit: Box<DocEdit<'a>>,
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

impl TryFrom<Output<'_>> for ToolMessage<ClientMessage> {
    type Error = TryIntoProtocolError;

    fn try_from(value: Output<'_>) -> Result<Self, Self::Error> {
        match value {
            Output::EditDoc { url, edit } => {
                if let Some(language_id) = url.language_id() {
                    let doc_edit: DocEdit<'_> = edit.as_ref().clone();
                    Ok(Self {
                        language_id,
                        message: ClientMessage::Doc{url, message: Box::new(doc_edit.try_into()?)},
                    })
                } else {
                    Err(TryIntoProtocolError::InvalidOutput)
                }
            }
            Output::GetFile { .. }
            | Output::Wrap { .. }
            | Output::MoveSelection { .. }
            | Output::UpdateHeader
            | Output::Notify { .. }
            | Output::Question { .. }
            | Output::StartIntake { .. }
            | Output::Reset { .. }
            | Output::Resize { .. }
            | Output::Write { .. }
            | Output::Quit
            | Output::Log { .. } => Err(TryIntoProtocolError::InvalidOutput),
        }
    }
}

/// An error converting [`Output`] into a [`Protocol`].
#[derive(Clone, Copy, Debug, Error)]
pub enum TryIntoProtocolError {
    /// Invalid [`Output`].
    #[error("")]
    InvalidOutput,
    /// Invalid edit doc.
    #[error("")]
    InvalidEditDoc(#[from] TryIntoMessageError),
}

/// Edits a document.
#[derive(Clone, Debug)]
pub(crate) enum DocEdit<'a> {
    /// Opens a document.
    Open {
        /// The URL of the document.
        url: PathUrl,
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

impl TryFrom<DocEdit<'_>> for lsp::DocMessage {
    type Error = TryIntoMessageError;

    fn try_from(value: DocEdit<'_>) -> Result<Self, Self::Error> {
        Ok(match value {
            DocEdit::Open { url, version, text } => url.language_id().map(|language_id| Self::Open {
                language_id,
                version,
                text: text.to_string(),
            }).ok_or(Self::Error::UnknownLanguage)?,
            DocEdit::Save { .. } => Self::Save,
            DocEdit::Change {
                version,
                text,
                selection,
                new_text,
            } => Self::Change {
                version,
                text: text.to_string(),
                range: selection.range()?,
                new_text,
            },
            DocEdit::Close => Self::Close,
        })
    }
}

/// An error converting [`DocEdit`] into [`Message`].
#[derive(Clone, Copy, Debug, Error)]
pub enum TryIntoMessageError {
    /// Selection.
    #[error(transparent)]
    Selection(#[from] SelectionConversionError),
    /// Unknown language.
    #[error("")]
    UnknownLanguage,
}

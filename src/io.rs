//! Implements the interface for all input and output to the application.
pub mod config;
pub mod fs;
pub mod logging;
pub mod lsp;
pub mod ui;

use {
    clap::ArgMatches,
    config::{ConsumeSettingError, CreateSettingConsumerError, Setting, SettingConsumer},
    core::{
        convert::TryFrom,
        fmt::{self, Display},
        sync::atomic::{AtomicBool, Ordering},
    },
    enum_map::Enum,
    fs::{PathUrl, File, ConsumeFileError, FileSystem, FileCommand},
    log::{error, LevelFilter},
    logging::LogManager,
    lsp::{
        ClientMessage, CreateLangClientError, DocMessage, Fault, LanguageTool, SendNotificationError,
        ServerMessage, ToolMessage,
    },
    lsp_types::{MessageType, ShowMessageParams, ShowMessageRequestParams},
    market::{ClosedMarketError, Consumer, Producer},
    starship::{context::Context, print},
    std::{
        env,
        io::{self, ErrorKind},
    },
    thiserror::Error,
    toml::{value::Table, Value},
    ui::{
        UserAction, ConsumeUserActionError,
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
    /// An error while creating the logging configuration.
    #[error("{0}")]
    CreateLogConfig(#[from] logging::Fault),
    /// An error determing the root directory.
    #[error("current working directory is invalid: {0}")]
    RootDir(#[from] io::Error),
    /// An error while working with a Url.
    #[error("")]
    Url,
    /// An error creating a [`Terminal`].
    ///
    /// [`Terminal`]: ui/struct.Terminal.html
    #[error("creating terminal: {0}")]
    CreateTerminal(#[from] CreateTerminalError),
    /// An error determining the home directory of the current user.
    #[error("home directory of current user is unknown")]
    HomeDir,
    /// An error while reading a file.
    #[error("{0}")]
    CreateFile(#[from] CreateFileError),
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
    #[error("")]
    Url,
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

/// An event that prevents [`Interface`] from consuming.
#[derive(Debug, Error)]
pub enum ConsumeInputIssue {
    /// The application has quit.
    #[error("")]
    Quit,
    /// The application has encountered an error while consuming.
    #[error("")]
    Error(#[from] ConsumeInputError),
}

/// An error consuming input.
#[derive(Debug, Error)]
pub enum ConsumeInputError {
    /// An error reading a message from the language tool.
    #[error("")]
    Read(#[from] Fault),
    /// An error producing a language tool protocol.
    #[error("")]
    Produce(#[from] lsp::ProduceProtocolError),
    /// An error consuming a user input.
    #[error("")]
    Ui(#[from] ConsumeUserActionError),
    /// An error consuming a setting.
    #[error("{0}")]
    Setting(#[from] ConsumeSettingError),
    /// The queue is closed.
    #[error("")]
    Closed(#[from] ClosedMarketError),
    /// An error consuming a file.
    #[error("")]
    File(#[from] ConsumeFileError),
}

/// The interface between the application and all external components.
#[derive(Debug)]
pub(crate) struct Interface {
    /// Manages the user interface.
    user_interface: Terminal,
    /// Notifies `self` of any events to the config file.
    setting_consumer: SettingConsumer,
    /// The interface of the application with all language servers.
    language_tool: LanguageTool,
    /// The root directory of the application.
    root_dir: PathUrl,
    /// The configuration of the logger.
    log_manager: LogManager,
    /// The interface with the file system.
    file_system: FileSystem,
    /// The application has quit.
    has_quit: AtomicBool,
}

impl Interface {
    /// Creates a new interface.
    pub(crate) fn new(arguments: &Arguments<'_>) -> Result<Self, CreateInterfaceError> {
        // Create log_manager first as this is where the logger is initialized.
        let log_manager = LogManager::new()?;
        let root_dir = PathUrl::try_from(env::current_dir()?).map_err(|_| CreateInterfaceError::Url)?;

        let interface = Self {
            log_manager,
            setting_consumer: SettingConsumer::new(
                &dirs::home_dir()
                    .ok_or(CreateInterfaceError::HomeDir)?
                    .join(".config/paper.toml"),
            )?,
            user_interface: Terminal::new()?,
            language_tool: LanguageTool::new(&root_dir)?,
            file_system: FileSystem::default(),
            root_dir,
            has_quit: AtomicBool::new(false),
        };

        if let Some(file) = arguments.file {
            interface.add_file(file)?;
        }

        Ok(interface)
    }

    /// Generates an Input for opening the file at `path`.
    fn add_file(&self, path: &str) -> Result<(), CreateFileError> {
        let url = self.root_dir.join(path).map_err(|_| CreateFileError::Url)?;

        if let Err(error) = self.file_system.produce(FileCommand::Read{ url: url.clone()}) {
            error!("Failed to store file `{}` to be read: {}", url, error);
        }

        Ok(())
    }

    /// Edits the doc at `url`.
    fn edit_doc(&self, file: &File, edit: DocEdit) -> Result<(), ProduceOutputError> {
        match edit {
            DocEdit::Open { .. } => {
                self.user_interface.force(ui::Output::OpenDoc {
                    text: file.text().to_string(),
                })?;
            }
            DocEdit::Save => {
                self.user_interface.force(ui::Output::Notify {
                    message: match self.file_system.produce(FileCommand::Write{url: file.url().clone(), text: file.text().to_string()}) {
                        Ok(..) => ShowMessageParams {
                            typ: MessageType::Info,
                            message: format!("Saved document `{}`", file.url()),
                        },
                        Err(error) => ShowMessageParams {
                            typ: MessageType::Error,
                            message: format!("Failed to save document `{}`: {}", file.url(), error),
                        },
                    },
                })?;
            }
            DocEdit::Change {
                new_text,
                selection,
                ..
            } => {
                self.user_interface.force(ui::Output::Edit {
                    new_text,
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
    type Error = ConsumeInputIssue;

    fn consume(&self) -> Result<Option<Self::Good>, Self::Error> {
        if let Some(ui_input) = self.user_interface.consume().map_err(ConsumeInputError::from)? {
            Ok(Some(Self::Good::from(ui_input)))
        } else if let Some(lang_input) = self.language_tool.consume().map_err(ConsumeInputError::from)? {
            Ok(Some(Self::Good::from(lang_input)))
        } else if let Some(setting) = self.setting_consumer.consume().map_err(ConsumeInputError::from)? {
            Ok(Some(Self::Good::from(setting)))
        } else if let Some(file) = self.file_system.consume().map_err(ConsumeInputError::from)? {
            Ok(Some(Self::Good::from(file)))
        } else if self.has_quit.load(Ordering::Relaxed) {
            Err(Self::Error::Quit)
        } else {
            Ok(None)
        }
    }
}

impl Drop for Interface {
    fn drop(&mut self) {
        for language_id in self.language_tool.language_ids() {
            if let Err(error) = self.language_tool.force(ToolMessage {
                language_id,
                message: ClientMessage::Shutdown,
            }) {
                error!(
                    "Failed to send shutdown message to {} language server: {}",
                    language_id, error
                );
            }
        }

        loop {
            // TODO: Need to check for reception from all clients.
            match self.language_tool.consume() {
                Ok(lang_input) => {
                    if let Some(ToolMessage {
                        message: ServerMessage::Shutdown,
                        ..
                    }) = lang_input
                    {
                        break;
                    }
                }
                Err(error) => {
                    error!("Error while waiting for shutdown: {}", error);
                    break;
                }
            }
        }

        for language_id in self.language_tool.language_ids() {
            if let Err(error) = self.language_tool.force(ToolMessage {
                language_id,
                message: ClientMessage::Exit,
            }) {
                error!(
                    "Failed to send exit message to {} language server: {}",
                    language_id, error
                );
            }

            // TODO: This should probably be a consume call.
            #[allow(clippy::indexing_slicing)] // EnumMap guarantees that index is valid.
            let server = &self.language_tool.clients[language_id];

            if let Err(error) = server.borrow_mut().server.wait() {
                error!(
                    "Failed to wait for {} language server process to finish: {}",
                    language_id, error
                );
            }
        }
    }
}

impl Producer for Interface {
    type Good = Output;
    type Error = ProduceOutputError;

    fn produce(&self, output: Self::Good) -> Result<Option<Self::Good>, Self::Error> {
        if let Ok(protocol) = ToolMessage::try_from(output.clone()) {
            if let Err(error) = self.language_tool.produce(protocol) {
                if let Err(produce_error) = self.user_interface.produce(ui::Output::Notify {
                    message: error.into(),
                }) {
                    error!("Unable to display error: {}", produce_error);
                }
            }
        }

        let result = match output.clone() {
            Output::SendLsp(..) => None,
            Output::GetFile { path } => {
                self.add_file(&path)?;
                None
            }
            Output::EditDoc { file, edit } => {
                self.edit_doc(&file, edit)?;
                None
            }
            Output::Wrap {
                is_wrapped,
                selection,
            } => self.user_interface.produce(ui::Output::Wrap {
                is_wrapped,
                selection,
            })?,
            Output::MoveSelection { selection } => self
                .user_interface
                .produce(ui::Output::MoveSelection { selection })?,
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
                })?
            }
            Output::Notify { message } => self
                .user_interface
                .produce(ui::Output::Notify { message })?,
            Output::Question { request } => self
                .user_interface
                .produce(ui::Output::Question { request })?,
            Output::StartIntake { title } => self
                .user_interface
                .produce(ui::Output::StartIntake { title })?,
            Output::Reset { selection } => self
                .user_interface
                .produce(ui::Output::Reset { selection })?,
            Output::Resize { size } => self.user_interface.produce(ui::Output::Resize { size })?,
            Output::Write { ch } => self.user_interface.produce(ui::Output::Write { ch })?,
            Output::Log { starship_level } => {
                if let Err(error) = self.log_manager
                    .produce(logging::Output::StarshipLevel(starship_level)) {
                    error!("Unable to set startship log-level: {}", error);
                }
                None
            }
            Output::Quit => {
                self.has_quit.store(true, Ordering::Relaxed);
                None
            }
        };

        error!("io: {:?} -> {:?}", output, result);
        Ok(result.map(|_| output))
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

/// An input.
#[derive(Debug)]
pub enum Input {
    /// A file to be opened.
    File(File),
    /// An input from the user.
    User(UserAction),
    /// A configuration.
    Setting(Setting),
    /// A glitch.
    Glitch(Glitch),
    /// A message from the language server.
    Lsp(ToolMessage<ServerMessage>),
}

impl From<File> for Input {
    #[inline]
    fn from(value: File) -> Self {
        Self::File(value)
    }
}

impl From<ToolMessage<ServerMessage>> for Input {
    #[inline]
    fn from(value: ToolMessage<ServerMessage>) -> Self {
        Self::Lsp(value)
    }
}

impl From<UserAction> for Input {
    #[inline]
    fn from(value: UserAction) -> Self {
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
pub(crate) enum Output {
    /// Sends message to language server.
    SendLsp(ToolMessage<ClientMessage>),
    /// Retrieves the URL and text of a file.
    GetFile {
        /// The relative path of the file.
        path: String,
    },
    /// Edits a document.
    EditDoc {
        /// The file that is edited.
        file: File,
        /// The edit to be performed.
        edit: DocEdit,
    },
    /// Sets the wrapping of the text.
    Wrap {
        /// If the text shall be wrapped.
        is_wrapped: bool,
        /// The selection.
        selection: Selection,
    },
    /// Moves the selection.
    MoveSelection {
        /// The selection.
        selection: Selection,
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
        selection: Selection,
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

impl TryFrom<Output> for ToolMessage<ClientMessage> {
    type Error = TryIntoProtocolError;

    #[inline]
    fn try_from(value: Output) -> Result<Self, Self::Error> {
        match value {
            Output::EditDoc { file, edit } => {
                if let Some(language_id) = file.language_id() {
                    let url: &Url = file.url().as_ref();

                    Ok(Self {
                        language_id,
                        message: ClientMessage::Doc {
                            url: url.clone(),
                            message: match edit {
                                DocEdit::Open {version} => DocMessage::Open {
                                        language_id,
                                        version,
                                        text: file.text().to_string(),
                                    },
                                DocEdit::Save { .. } => DocMessage::Save,
                                DocEdit::Change {
                                    version,
                                    selection,
                                    new_text,
                                } => DocMessage::Change {
                                    version,
                                    text: file.text().to_string(),
                                    range: selection.range()?,
                                    new_text,
                                },
                                DocEdit::Close => DocMessage::Close,
                            },
                        },
                    })
                } else {
                    Err(TryIntoProtocolError::InvalidOutput)
                }
            }
            Output::SendLsp(message) => Ok(message),
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
    InvalidEditDoc(#[from] SelectionConversionError),
}

/// Edits a document.
#[derive(Clone, Debug)]
pub(crate) enum DocEdit {
    /// Opens a document.
    Open{
        /// The version of the document.
        version: i64,
    },
    /// Saves the document.
    Save,
    /// Edits the document.
    Change {
        /// The new text.
        new_text: String,
        /// The selection.
        selection: Selection,
        /// The version.
        version: i64,
    },
    /// Closes the document.
    Close,
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

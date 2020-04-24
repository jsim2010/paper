//! Implements the interface for all input and output to the application.
pub mod config;
pub mod fs;
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
    fehler::{throw, throws},
    fs::{ConsumeFileError, CreatePurlError, File, FileCommand, FileError, FileSystem, Purl},
    log::error,
    lsp::{
        ClientMessage, DocConfiguration, DocMessage, Fault, LanguageTool, SendNotificationError, ServerMessage,
        ToolMessage,
    },
    lsp_types::{MessageType, ShowMessageParams, ShowMessageRequestParams},
    market::{ClosedMarketError, Collector, Consumer, OneShotError, Producer},
    starship::{context::Context, print},
    std::{
        env,
        io::{self, ErrorKind},
    },
    thiserror::Error,
    toml::{value::Table, Value},
    ui::{
        BodySize, CommandError, ConsumeUserActionError, CreateTerminalError,
        ProduceTerminalOutputError, Selection, SelectionConversionError, Terminal, UserAction,
        UserActionConsumer,
    },
    url::Url,
};

/// An error creating an [`Interface`].
///
/// [`Interface`]: struct.Interface.html
#[derive(Debug, Error)]
pub enum CreateInterfaceError {
    /// An error determing the current working directory.
    #[error("current working directory is invalid: {0}")]
    WorkingDir(#[from] io::Error),
    /// An error creating the root directory [`Purl`].
    #[error("unable to create URL of root directory: {0}")]
    RootDir(#[from] CreatePurlError),
    /// An error determining the home directory of the current user.
    #[error("home directory of current user is unknown")]
    HomeDir,
    /// An error creating the setting consumer.
    #[error(transparent)]
    Config(#[from] CreateSettingConsumerError),
    /// An error creating a [`Terminal`].
    ///
    /// [`Terminal`]: ui/struct.Terminal.html
    #[error(transparent)]
    Terminal(#[from] CreateTerminalError),
    /// An error creating a [`LangaugeTool`].
    #[error(transparent)]
    LanguageTool(#[from] lsp::CreateLanguageToolError),
    /// An error creating a file.
    #[error(transparent)]
    CreateFile(#[from] CreateFileError),
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
    /// Failed to send notification.
    #[error("{0}")]
    SendNotification(#[from] SendNotificationError),
    /// An error while reading a file.
    #[error("{0}")]
    CreateFile(#[from] CreateFileError),
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

/// An error creating a file.
#[derive(Debug, Error)]
pub enum CreateFileError {
    /// An error generating the [`Purl`] of the file.
    #[error(transparent)]
    Purl(#[from] CreatePurlError),
    /// An error triggering a file read.
    #[error(transparent)]
    Read(#[from] OneShotError<FileError>),
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
    /// A [`Collector`] of all input [`Consumer`]s.
    consumers: Collector<Input, ConsumeInputError>,
    /// Manages the user interface.
    user_interface: Terminal,
    /// The interface of the application with all language servers.
    language_tool: LanguageTool,
    /// The root directory of the application.
    root_dir: Purl,
    /// The interface with the file system.
    file_system: FileSystem,
    /// The application has quit.
    has_quit: AtomicBool,
}

impl Interface {
    /// Creates a new interface.
    #[throws(CreateInterfaceError)]
    pub(crate) fn new(initial_file: Option<&'_ str>) -> Self {
        let root_dir = Purl::try_from(env::current_dir()?)?;
        let mut consumers = Collector::new();
        consumers.convert_into_and_push(UserActionConsumer::new());
        consumers.convert_into_and_push(SettingConsumer::new(
            &dirs::home_dir()
                .ok_or(CreateInterfaceError::HomeDir)?
                .join(".config/paper.toml"),
        )?);

        let interface = Self {
            consumers,
            user_interface: Terminal::new()?,
            language_tool: LanguageTool::new(&root_dir)?,
            file_system: FileSystem::default(),
            root_dir,
            has_quit: AtomicBool::new(false),
        };

        if let Some(file) = initial_file {
            interface.add_file(file)?;
        }

        interface
    }

    /// Reads the file at `path`.
    #[throws(CreateFileError)]
    fn add_file(&self, path: &str) {
        self.file_system.attempt(
            FileCommand::Read {
                url: self.root_dir.join(path)?,
            },
            0,
        )?
    }

    /// Edits the doc at `url`.
    #[throws(ProduceOutputError)]
    fn edit_doc(&self, file: &File, edit: DocEdit) {
        match edit {
            DocEdit::Open { .. } => {
                self.user_interface.force(ui::Output::OpenDoc {
                    text: file.text().to_string(),
                })?;
            }
            DocEdit::Save => {
                self.user_interface.force(ui::Output::Notify {
                    message: match self.file_system.produce(FileCommand::Write {
                        url: file.url().clone(),
                        text: file.text().to_string(),
                    }) {
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
            DocEdit::Change (doc_change) => {
                self.user_interface.force(ui::Output::Edit {
                    new_text: doc_change.new_text,
                    selection: doc_change.selection,
                })?;
            }
            DocEdit::Close => {}
        }
    }
}

impl Consumer for Interface {
    type Good = Input;
    type Error = ConsumeInputIssue;

    #[throws(Self::Error)]
    fn consume(&self) -> Option<Self::Good> {
        if let Some(input) = self.consumers.consume()? {
            Some(input)
        } else if let Some(lang_input) = self
            .language_tool
            .consume()
            .map_err(ConsumeInputError::from)?
        {
            Some(Self::Good::from(lang_input))
        } else if let Some(file) = self
            .file_system
            .consume()
            .map_err(ConsumeInputError::from)?
        {
            Some(Self::Good::from(file))
        } else if self.has_quit.load(Ordering::Relaxed) {
            throw!(Self::Error::Quit);
        } else {
            None
        }
    }
}

impl Drop for Interface {
    fn drop(&mut self) {
        for language_id in self.language_tool.language_ids() {
            if let Err(error) = self.language_tool.attempt(
                ToolMessage {
                    language_id,
                    message: ClientMessage::Shutdown,
                },
                0,
            ) {
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
            if let Err(error) = self.language_tool.attempt(
                ToolMessage {
                    language_id,
                    message: ClientMessage::Exit,
                },
                0,
            ) {
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

    #[throws(Self::Error)]
    fn produce(&self, output: Self::Good) -> Option<Self::Good> {
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
            Output::Quit => {
                self.has_quit.store(true, Ordering::Relaxed);
                None
            }
        };

        result.map(|_| output)
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
    File(Box<File>),
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
        Self::File(Box::new(value))
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
}

impl TryFrom<Output> for ToolMessage<ClientMessage> {
    type Error = TryIntoProtocolError;

    #[inline]
    #[throws(Self::Error)]
    fn try_from(value: Output) -> Self {
        match value {
            Output::EditDoc { file, edit } => {
                if let Some(language_id) = file.language_id() {
                    let url: &Url = file.url().as_ref();

                    Self {
                        language_id,
                        message: ClientMessage::Doc (Box::new(DocConfiguration::new(
                            url.clone(),
                            match edit {
                                DocEdit::Open { version } => DocMessage::Open {
                                    language_id,
                                    version,
                                    text: file.text().to_string(),
                                },
                                DocEdit::Save { .. } => DocMessage::Save,
                                DocEdit::Change (doc_change) => {
                                    DocMessage::Change {
                                    version: doc_change.version,
                                    text: file.text().to_string(),
                                    range: doc_change.selection.range()?,
                                    new_text: doc_change.new_text,
                                    }
                                },
                                DocEdit::Close => DocMessage::Close,
                            },
                        ))),
                    }
                } else {
                    throw!(TryIntoProtocolError::InvalidOutput);
                }
            }
            Output::SendLsp(message) => message,
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
            | Output::Quit => throw!(TryIntoProtocolError::InvalidOutput),
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
    Open {
        /// The version of the document.
        version: i64,
    },
    /// Saves the document.
    Save,
    /// Edits the document.
    Change(Box<DocChange>),
    /// Closes the document.
    Close,
}

/// The changes to be made to a document.
#[derive(Clone, Debug)]
pub(crate) struct DocChange {
    /// The new text.
    new_text: String,
    /// The selection.
    selection: Selection,
    /// The version.
    version: i64,
}

impl DocChange {
    /// Creates a new [`DocChange`].
    pub(crate) const fn new(selection: Selection, version: i64) -> Self {
        Self {
            new_text: String::new(),
            selection,
            version,
        }
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

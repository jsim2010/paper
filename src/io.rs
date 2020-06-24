//! Implements the interface for all input and output to the application.
mod config;
mod fs;
mod lsp;
mod ui;

pub(crate) use {
    config::Setting,
    fs::{CreatePurlError, File, Purl},
    lsp::{ClientMessage, ServerMessage, ToolMessage},
    ui::{Dimensions, Unit, UserAction},
};

use {
    crate::app::Document,
    clap::ArgMatches,
    config::{ConsumeSettingError, CreateSettingConsumerError, SettingConsumer},
    core::{
        convert::TryFrom,
        sync::atomic::{AtomicBool, Ordering},
    },
    enum_map::Enum,
    fehler::{throw, throws},
    fs::{ConsumeFileError, FileCommand, FileError, FileSystem},
    log::error,
    lsp::{
        DocConfiguration, DocMessage, Fault, LanguageTool, SendNotificationError,
    },
    lsp_types::{ShowMessageParams, ShowMessageRequestParams},
    market::{ClosedMarketFailure, Collector, ConsumeError, Consumer, ProduceError, Producer},
    parse_display::Display as ParseDisplay,
    starship::{context::Context, print},
    std::{
        env,
        io::{self, ErrorKind},
    },
    thiserror::Error,
    toml::{value::Table, Value},
    ui::{
        CreateTerminalError, DisplayCmd, DisplayCmdFailure, Terminal,
        UserActionConsumer, UserActionFailure,
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
    /// An error with a file.
    #[error("")]
    File(#[from] FileError),
    /// An error in the lsp.
    #[error("{0}")]
    Lsp(#[from] Fault),
    /// Failed to send notification.
    #[error("{0}")]
    SendNotification(#[from] SendNotificationError),
    /// An error creating a [`Purl`].
    #[error("")]
    CreatePurl(#[from] CreatePurlError),
    /// An error while reading a file.
    #[error("{0}")]
    CreateFile(#[from] CreateFileError),
    /// Produce error from ui.
    #[error("{0}")]
    UiProduce(#[from] DisplayCmdFailure),
}

/// An error creating a file.
#[derive(Debug, Error)]
pub enum CreateFileError {
    /// An error generating the [`Purl`] of the file.
    #[error(transparent)]
    Purl(#[from] CreatePurlError),
    /// An error triggering a file read.
    #[error(transparent)]
    Read(#[from] ProduceError<FileError>),
}

/// An error while reading a file.
#[derive(Debug, Error)]
#[error("failed to read `{file}`: {error:?}")]
struct ReadFileError {
    /// The error.
    error: ErrorKind,
    /// The path of the file being read.
    file: String,
}

/// An error in the user interface that is recoverable.
///
/// Until a glitch is resolved, certain functionality may not be properly completed.
#[derive(Debug, Error)]
enum Glitch {
    /// Unable to convert config file to Config.
    #[error("config file invalid format: {0}")]
    ConfigFormat(#[from] toml::de::Error),
}

/// An event that prevents [`Interface`] from consuming.
#[derive(Debug, Error)]
pub(crate) enum ConsumeInputIssue {
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
    Ui(#[from] UserActionFailure),
    /// An error consuming a setting.
    #[error("{0}")]
    Setting(#[from] ConsumeSettingError),
    /// The queue is closed.
    #[error("")]
    Closed(#[from] ClosedMarketFailure),
    /// An error consuming a file.
    #[error("")]
    File(#[from] ConsumeFileError),
}

/// An error determining the root directory.
#[derive(Debug, Error)]
pub enum RootDirError {
    /// An error determing the current working directory.
    #[error("current working directory is invalid: {0}")]
    GetWorkingDir(#[from] io::Error),
    /// An error creating the root directory [`Purl`].
    #[error("unable to create URL of root directory: {0}")]
    Create(#[from] CreatePurlError),
}

/// Returns the root directory.
#[throws(RootDirError)]
pub(crate) fn root_dir() -> Purl {
    Purl::try_from(env::current_dir()?)?
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
    pub(crate) fn new(initial_file: Option<Purl>) -> Self {
        let root_dir = Purl::try_from(env::current_dir()?)?;
        let mut consumers = Collector::new();
        consumers.convert_into_and_push(UserActionConsumer::new());
        consumers.convert_into_and_push(SettingConsumer::new(
            &home::home_dir()
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
            interface.open_file(file)?;
        }

        interface
    }

    /// Reads the file at `url`.
    #[throws(CreateFileError)]
    fn open_file(&self, url: Purl) {
        self.file_system.produce(FileCommand::Read { url })?
    }

    /// Edits the doc at `url`.
    #[throws(ProduceError<ProduceOutputError>)]
    fn edit_doc(&self, doc: &Document, edit: &DocEdit) {
        match edit {
            DocEdit::Open { .. } => {
                self.user_interface
                    .produce(DisplayCmd::Rows { rows: doc.rows() })
                    .map_err(|error| error.map(ProduceOutputError::from))?;
            }
            DocEdit::Save => {
                self.file_system
                    .produce(FileCommand::Write {
                        url: doc.url().clone(),
                        text: doc.text(),
                    })
                    .map_err(|error| error.map(ProduceOutputError::from))?;
            }
            DocEdit::Update => {
                self.user_interface
                    .produce(DisplayCmd::Rows { rows: doc.rows() })
                    .map_err(|error| error.map(ProduceOutputError::from))?;
            }
            DocEdit::Close => {}
        }
    }
}

impl Consumer for Interface {
    type Good = Input;
    type Failure = ConsumeInputIssue;

    #[throws(ConsumeError<Self::Failure>)]
    fn consume(&self) -> Self::Good {
        match self.consumers.consume() {
            Ok(input) => input,
            Err(ConsumeError::Failure(failure)) => {
                throw!(ConsumeError::Failure(Self::Failure::Error(failure)))
            }
            Err(ConsumeError::EmptyStock) => match self.language_tool.consume() {
                Ok(lang_input) => lang_input.into(),
                Err(ConsumeError::Failure(failure)) => {
                    throw!(ConsumeError::Failure(Self::Failure::Error(failure.into())))
                }
                Err(ConsumeError::EmptyStock) => match self.file_system.consume() {
                    Ok(file) => file.into(),
                    Err(ConsumeError::Failure(failure)) => {
                        throw!(ConsumeError::Failure(Self::Failure::Error(failure.into())))
                    }
                    Err(ConsumeError::EmptyStock) => {
                        if self.has_quit.load(Ordering::Relaxed) {
                            throw!(ConsumeError::Failure(Self::Failure::Quit));
                        } else {
                            throw!(ConsumeError::EmptyStock);
                        }
                    }
                },
            },
        }
    }
}

impl Drop for Interface {
    fn drop(&mut self) {
        for language_id in self.language_tool.language_ids() {
            if let Err(error) = self.language_tool.produce(ToolMessage {
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
                Ok(ToolMessage {
                    message: ServerMessage::Shutdown,
                    ..
                }) => {
                    break;
                }
                Err(error) => {
                    error!("Error while waiting for shutdown: {}", error);
                    break;
                }
                _ => {}
            }
        }

        for language_id in self.language_tool.language_ids() {
            if let Err(error) = self.language_tool.produce(ToolMessage {
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
    type Failure = ProduceOutputError;

    #[throws(ProduceError<Self::Failure>)]
    fn produce(&self, output: Self::Good) {
        if let Ok(protocol) = ToolMessage::try_from(output.clone()) {
            if let Err(error) = self.language_tool.produce(protocol) {
                error!("Unable to write to language server: {}", error);
            }
        }

        match output {
            Output::SendLsp(..) => {}
            Output::OpenFile { path } => {
                let file = self
                    .root_dir
                    .join(&path)
                    .map_err(|error| ProduceError::Failure(Self::Failure::from(error)))?;
                self.open_file(file)
                    .map_err(|error| ProduceError::Failure(Self::Failure::from(error)))?;
            }
            Output::EditDoc { doc, edit } => {
                self.edit_doc(&doc, &edit)?;
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
                self.user_interface
                    .produce(DisplayCmd::Header {
                        header: print::get_prompt(context),
                    })
                    .map_err(|error| error.map(Self::Failure::from))?
            }
            Output::Notify { message } => self
                .user_interface
                .produce(DisplayCmd::Rows {
                    rows: vec![message.message],
                })
                .map_err(|error| error.map(Self::Failure::from))?,
            Output::Question { request } => self
                .user_interface
                .produce(DisplayCmd::Rows {
                    rows: vec![request.message],
                })
                .map_err(|error| error.map(Self::Failure::from))?,
            Output::Command { command } => self
                .user_interface
                .produce(DisplayCmd::Rows {
                    rows: vec![command],
                })
                .map_err(|error| error.map(Self::Failure::from))?,
            Output::Quit => {
                self.has_quit.store(true, Ordering::Relaxed);
            }
        }
    }
}

/// An error occurred while converting a directory path to a URL.
#[derive(Debug, Error)]
#[error("while converting `{0}` to a URL")]
struct UrlError(String);

/// The language ids supported by `paper`.
#[derive(Clone, Copy, Debug, Enum, Eq, Hash, ParseDisplay, PartialEq)]
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

/// An input.
#[derive(Debug)]
pub(crate) enum Input {
    /// A file to be opened.
    File(File),
    /// An input from the user.
    User(UserAction),
    /// A setting.
    Setting(Setting),
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
#[derive(Clone, Debug, ParseDisplay)]
pub(crate) enum Output {
    /// Sends message to language server.
    #[display("Send to language server `{0}`")]
    SendLsp(ToolMessage<ClientMessage>),
    /// Retrieves the URL and text of a file.
    #[display("Get file `{path}`")]
    OpenFile {
        /// The relative path of the file.
        path: String,
    },
    #[display("")]
    /// Edits a document.
    EditDoc {
        /// The file that is edited.
        doc: Document,
        /// The edit to be performed.
        edit: DocEdit,
    },
    /// Sets the header of the application.
    #[display("")]
    UpdateHeader,
    /// Notifies the user of a message.
    #[display("")]
    Notify {
        /// The message.
        message: ShowMessageParams,
    },
    /// Asks the user a question.
    #[display("")]
    Question {
        /// The request to be answered.
        request: ShowMessageRequestParams,
    },
    /// Adds an intake box.
    #[display("")]
    Command {
        /// The prompt of the intake box.
        command: String,
    },
    /// Quit the application.
    #[display("")]
    Quit,
}

impl TryFrom<Output> for ToolMessage<ClientMessage> {
    type Error = TryIntoProtocolError;

    #[inline]
    #[throws(Self::Error)]
    fn try_from(value: Output) -> Self {
        match value {
            Output::EditDoc { doc, edit } => {
                if let Some(language_id) = doc.language_id() {
                    let url: &Url = doc.url().as_ref();

                    Self {
                        language_id,
                        message: ClientMessage::Doc(DocConfiguration::new(
                            url.clone(),
                            match edit {
                                DocEdit::Open { version } => DocMessage::Open {
                                    language_id,
                                    version,
                                    text: doc.text(),
                                },
                                DocEdit::Save { .. } => DocMessage::Save,
                                DocEdit::Close => DocMessage::Close,
                                DocEdit::Update => throw!(TryIntoProtocolError::InvalidOutput),
                            },
                        )),
                    }
                } else {
                    throw!(TryIntoProtocolError::InvalidOutput);
                }
            }
            Output::SendLsp(message) => message,
            Output::OpenFile { .. }
            | Output::Command { .. }
            | Output::UpdateHeader
            | Output::Notify { .. }
            | Output::Question { .. }
            | Output::Quit => throw!(TryIntoProtocolError::InvalidOutput),
        }
    }
}

/// An error converting [`Output`] into a [`Protocol`].
#[derive(Clone, Copy, Debug, Error)]
pub(crate) enum TryIntoProtocolError {
    /// Invalid [`Output`].
    #[error("")]
    InvalidOutput,
}

/// Edits a document.
#[derive(Clone, Copy, Debug)]
pub(crate) enum DocEdit {
    /// Opens a document.
    Open {
        /// The version of the document.
        version: i64,
    },
    /// Saves the document.
    Save,
    /// Updates the display of the document.
    Update,
    /// Closes the document.
    Close,
}

/// The changes to be made to a document.
#[derive(Clone, Debug)]
pub(crate) struct DocChange {
    /// The new text.
    new_text: String,
    /// The version.
    version: i64,
}

//! Implements the interface for all input and output to the application.
#![allow(clippy::pattern_type_mismatch)]
mod fs;
mod ui;

pub(crate) use {
    fs::File,
    ui::{Dimensions, UserAction},
};

use {
    crate::app::Document,
    clap::ArgMatches,
    core::{
        convert::TryFrom,
        sync::atomic::{AtomicBool, Ordering},
    },
    docuglot::{ClientStatement, ServerStatement, Tongue},
    fehler::{throw, throws},
    fs::{
        create_file_system, ConsumeFileError, FileCommand, FileCommandProducer, FileConsumer,
        FileError, RootDirError,
    },
    log::error,
    lsp_types::ShowMessageRequestParams,
    market::{
        channel::{CrossbeamConsumer, CrossbeamProducer, WithdrawnDemand, WithdrawnSupply},
        vec::{Collector, Distributor},
        ConsumeFailure, ConsumeFault, Consumer, ProduceFailure, Producer,
    },
    parse_display::Display as ParseDisplay,
    starship::{context::Context, print},
    std::io::{self, ErrorKind},
    thiserror::Error as ThisError,
    toml::{value::Table, Value},
    ui::{
        CreateTerminalError, DisplayCmd, DisplayCmdFailure, Terminal, UserActionConsumer,
        UserActionFailure,
    },
};

/// An error creating an [`Interface`].
///
/// [`Interface`]: struct.Interface.html
#[derive(Debug, ThisError)]
pub enum CreateInterfaceError {
    /// An error determing the current working directory.
    #[error("current working directory is invalid: {0}")]
    WorkingDir(#[from] io::Error),
    /// An error creating the root directory [`Purl`].
    #[error("unable to create URL of root directory: {0}")]
    RootDir(#[from] RootDirError),
    /// An error determining the home directory of the current user.
    #[error("home directory of current user is unknown")]
    HomeDir,
    /// An error creating a [`Terminal`].
    ///
    /// [`Terminal`]: ui/struct.Terminal.html
    #[error(transparent)]
    Terminal(#[from] CreateTerminalError),
    /// An error creating a file.
    #[error(transparent)]
    CreateFile(#[from] FileError),
}

/// An error while writing output.
#[derive(Debug, market::ProduceFault, ThisError)]
pub enum ProduceOutputError {
    /// An error with a file.
    #[error("")]
    File(#[from] FileError),
    /// An error while reading a file.
    #[error("{0}")]
    CreateFile(#[from] CreateFileError),
    /// Produce error from ui.
    #[error("{0}")]
    UiProduce(#[from] DisplayCmdFailure),
    /// A thread was dropped.
    #[error(transparent)]
    Withdrawn(#[from] WithdrawnDemand),
}

/// An error creating a file.
#[derive(Debug, ThisError)]
pub enum CreateFileError {
    /// An error triggering a file read.
    #[error(transparent)]
    Read(#[from] ProduceFailure<FileError>),
}

/// An error while reading a file.
#[derive(Debug, ThisError)]
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
#[derive(Debug, ThisError)]
enum Glitch {
    /// Unable to convert config file to Config.
    #[error("config file invalid format: {0}")]
    ConfigFormat(#[from] toml::de::Error),
}

/// An event that prevents [`Interface`] from consuming.
#[derive(Debug, ConsumeFault, ThisError)]
pub(crate) enum ConsumeInputIssue {
    /// The application has quit.
    #[error("")]
    Quit,
    /// The application has encountered an error while consuming.
    #[error("")]
    Error(#[from] ConsumeInputError),
}

/// An error consuming input.
#[derive(Debug, ConsumeFault, ThisError)]
pub enum ConsumeInputError {
    /// An error consuming a user input.
    #[error("")]
    Ui(#[from] UserActionFailure),
    /// An error consuming a file.
    #[error("")]
    File(#[from] ConsumeFileError),
    /// A thread was dropped.
    #[error(transparent)]
    Withdrawn(#[from] WithdrawnSupply),
}

/// An error validating a file string.
#[derive(Debug, ThisError)]
pub(crate) enum InvalidFileStringError {
    /// An error determining the root directory.
    #[error("")]
    RootDir(#[from] RootDirError),
}

/// Implements [`Terminal`] that can be pushed to [`Interface.producers`].
struct InternalTerminal(Terminal);

impl Producer for InternalTerminal {
    type Good = DisplayCmd;
    type Failure = ProduceFailure<ProduceOutputError>;

    #[throws(Self::Failure)]
    fn produce(&self, good: Self::Good) {
        self.0.produce(good).map_err(ProduceFailure::map_fault)?
    }
}

/// Implements [`UserActionConsumer`] that can be pushed to [`Interface.consumers`].
struct InternalUserActionConsumer(UserActionConsumer);

impl Consumer for InternalUserActionConsumer {
    type Good = UserAction;
    type Failure = ConsumeFailure<ConsumeInputError>;

    #[throws(Self::Failure)]
    fn consume(&self) -> Self::Good {
        self.0.consume().map_err(ConsumeFailure::map_fault)?
    }
}

/// Implements [`CrossbeamProducer<ClientStatement>`] that can be pushed to [`Interface.producers`].
struct InternalLspProducer(CrossbeamProducer<ClientStatement>);

impl Producer for InternalLspProducer {
    type Good = ClientStatement;
    type Failure = ProduceFailure<ProduceOutputError>;

    #[throws(Self::Failure)]
    fn produce(&self, good: Self::Good) {
        self.0.produce(good).map_err(ProduceFailure::map_fault)?
    }
}

/// Implements [`CrossbeamConsumer<ServerStatement>`] that can be pushed to [`Interface.consumers`].
struct InternalLspConsumer(CrossbeamConsumer<ServerStatement>);

impl Consumer for InternalLspConsumer {
    type Good = ServerStatement;
    type Failure = ConsumeFailure<ConsumeInputError>;

    #[throws(Self::Failure)]
    fn consume(&self) -> Self::Good {
        self.0.consume().map_err(ConsumeFailure::map_fault)?
    }
}

/// Implements [`FileCommandProducer`] that can be pushed to [`Interface.producers`].
struct InternalFileProducer(FileCommandProducer);

impl Producer for InternalFileProducer {
    type Good = FileCommand;
    type Failure = ProduceFailure<ProduceOutputError>;

    #[throws(Self::Failure)]
    fn produce(&self, good: Self::Good) {
        self.0
            .produce(good)
            .map_err(|error| ProduceFailure::Fault(error.into()))?
    }
}

/// Implements [`FileConsumer`] that can be pushed to [`Interface.consumers`].
struct InternalFileConsumer(FileConsumer);

impl Consumer for InternalFileConsumer {
    type Good = File;
    type Failure = ConsumeFailure<ConsumeInputError>;

    #[throws(Self::Failure)]
    fn consume(&self) -> Self::Good {
        self.0.consume().map_err(ConsumeFailure::map_fault)?
    }
}

/// The interface between the application and all external components.
#[derive(Debug)]
pub(crate) struct Interface {
    /// A [`Collector`] of all input [`Consumer`]s.
    consumers: Collector<Input, ConsumeInputError>,
    /// A [`Distributor`] of all output [`Producer`]s.
    producers: Distributor<Output, ProduceOutputError>,
    /// The interface of the application with all language servers.
    tongue: Tongue,
    /// The application has quit.
    has_quit: AtomicBool,
}

impl Interface {
    /// Creates a new interface.
    #[throws(CreateInterfaceError)]
    pub(crate) fn new(initial_file: Option<String>) -> Self {
        let user_interface = InternalTerminal(Terminal::new()?);
        let mut consumers = Collector::new();
        let mut producers = Distributor::new();
        let (file_command_producer, file_consumer) = create_file_system()?;
        let (lsp_producer, lsp_consumer, tongue) = docuglot::init(file_command_producer.root_dir());

        if let Some(file) = initial_file {
            file_command_producer.produce(FileCommand::Read { path: file })?
        }

        consumers.push(InternalUserActionConsumer(UserActionConsumer));
        consumers.push(InternalLspConsumer(lsp_consumer));
        consumers.push(InternalFileConsumer(file_consumer));

        producers.push(InternalLspProducer(lsp_producer));
        producers.push(user_interface);
        producers.push(InternalFileProducer(file_command_producer));

        let interface = Self {
            consumers,
            producers,
            tongue,
            has_quit: AtomicBool::new(false),
        };

        interface
    }

    /// Waits until `self.tongue` finishes.
    pub(crate) fn join(&self) {
        // TODO: This should be a part of the consume after detecting the app is quitting.
        self.tongue.join()
    }
}

impl Consumer for Interface {
    type Good = Input;
    type Failure = ConsumeFailure<ConsumeInputIssue>;

    #[throws(Self::Failure)]
    fn consume(&self) -> Self::Good {
        match self.consumers.consume() {
            Ok(input) => input,
            Err(market::ConsumeFailure::Fault(failure)) => {
                throw!(market::ConsumeFailure::Fault(ConsumeInputIssue::Error(
                    failure
                )))
            }
            Err(market::ConsumeFailure::EmptyStock) => {
                if self.has_quit.load(Ordering::Relaxed) {
                    throw!(market::ConsumeFailure::Fault(ConsumeInputIssue::Quit));
                } else {
                    throw!(market::ConsumeFailure::EmptyStock);
                }
            }
        }
    }
}

impl Producer for Interface {
    type Good = Output;
    type Failure = ProduceFailure<ProduceOutputError>;

    #[throws(Self::Failure)]
    fn produce(&self, output: Self::Good) {
        self.producers.produce(output.clone())?;

        // TODO: Instead of has_quit, use market::Trigger stored in producers.
        match output {
            Output::OpenFile { .. }
            | Output::UpdateView { .. }
            | Output::EditDoc { .. }
            | Output::UpdateHeader
            | Output::Question { .. }
            | Output::Command { .. } => {}
            Output::Quit => {
                self.has_quit.store(true, Ordering::Relaxed);
            }
        }
    }
}

/// An error occurred while converting a directory path to a URL.
#[derive(Debug, ThisError)]
#[error("while converting `{0}` to a URL")]
struct UrlError(String);

/// An input.
#[derive(Debug)]
pub(crate) enum Input {
    /// A file to be opened.
    File(File),
    /// An input from the user.
    User(UserAction),
    /// A message from the language server.
    Lsp(ServerStatement),
}

impl From<File> for Input {
    #[inline]
    fn from(value: File) -> Self {
        Self::File(value)
    }
}

impl From<ServerStatement> for Input {
    #[inline]
    fn from(value: ServerStatement) -> Self {
        Self::Lsp(value)
    }
}

impl From<UserAction> for Input {
    #[inline]
    fn from(value: UserAction) -> Self {
        Self::User(value)
    }
}

/// An output.
#[derive(Clone, Debug, ParseDisplay)]
pub(crate) enum Output {
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
    /// Updates the area of the document that is shown.
    #[display("")]
    UpdateView {
        /// The rows of the document to be shown.
        rows: Vec<String>,
    },
    /// Sets the header of the application.
    #[display("")]
    UpdateHeader,
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

impl TryFrom<Output> for FileCommand {
    type Error = TryIntoFileCommandError;

    #[inline]
    #[throws(Self::Error)]
    fn try_from(value: Output) -> Self {
        match value {
            Output::OpenFile { path } => Self::Read { path },
            Output::Command { .. }
            | Output::EditDoc { .. }
            | Output::UpdateHeader
            | Output::UpdateView { .. }
            | Output::Question { .. }
            | Output::Quit => throw!(TryIntoFileCommandError::InvalidOutput),
        }
    }
}

impl TryFrom<Output> for ClientStatement {
    type Error = TryIntoProtocolError;

    #[inline]
    #[throws(Self::Error)]
    fn try_from(value: Output) -> Self {
        match value {
            Output::EditDoc { doc, edit } => match edit {
                DocEdit::Open { .. } => ClientStatement::open_doc(doc.into()),
                DocEdit::Close => ClientStatement::close_doc(doc.into()),
                DocEdit::Update => throw!(TryIntoProtocolError::InvalidOutput),
            },
            Output::OpenFile { .. }
            | Output::Command { .. }
            | Output::UpdateHeader
            | Output::UpdateView { .. }
            | Output::Question { .. }
            | Output::Quit => throw!(TryIntoProtocolError::InvalidOutput),
        }
    }
}

impl TryFrom<Output> for DisplayCmd {
    type Error = TryIntoDisplayCmdError;

    #[inline]
    #[throws(Self::Error)]
    fn try_from(value: Output) -> Self {
        match value {
            Output::EditDoc { doc, edit } => match edit {
                DocEdit::Open { .. } | DocEdit::Update => DisplayCmd::Rows { rows: doc.rows() },
                DocEdit::Close => throw!(TryIntoDisplayCmdError::InvalidOutput),
            },
            Output::UpdateView { rows } => DisplayCmd::Rows { rows },
            Output::Question { request } => DisplayCmd::Rows {
                rows: vec![request.message],
            },
            Output::Command { command } => DisplayCmd::Command { command },
            Output::UpdateHeader => {
                let mut context = Context::new(ArgMatches::new());

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

                DisplayCmd::Header {
                    header: print::get_prompt(context),
                }
            }
            Output::OpenFile { .. } | Output::Quit => throw!(TryIntoDisplayCmdError::InvalidOutput),
        }
    }
}

/// An error converting [`Output`] into a [`Protocol`].
#[derive(Debug, ThisError)]
pub(crate) enum TryIntoFileCommandError {
    /// Invalid `Output`.
    #[error("")]
    InvalidOutput,
}

/// An error converting [`Output`] into a [`Protocol`].
#[derive(Debug, ThisError)]
pub(crate) enum TryIntoDisplayCmdError {
    /// Invalid `Output`.
    #[error("")]
    InvalidOutput,
}

/// An error converting [`Output`] into a [`Protocol`].
#[derive(Clone, Copy, Debug, ThisError)]
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

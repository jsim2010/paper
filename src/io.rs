//! Implements the interface for all input and output to the application.
#![allow(clippy::pattern_type_mismatch)]
mod fs;
mod ui;

pub(crate) use {
    fs::File,
    ui::{Dimensions, RowText, Style, StyledText, Unit, UserAction},
};

use {
    crate::app::{Document, ScopeFromRangeError},
    clap::ArgMatches,
    core::{
        convert::TryFrom,
        sync::atomic::{AtomicBool, Ordering},
    },
    docuglot::{Reception, Tongue, TranslationError, Transmission},
    fehler::{throw, throws},
    fs::{
        create_file_system, ConsumeFileError, FileCommand, FileCommandProducer, FileConsumer,
        FileError, RootDirError,
    },
    log::error,
    lsp_types::{ShowMessageRequestParams, TextDocumentIdentifier},
    market::{
        channel::{WithdrawnDemandFault, WithdrawnSupplyFault},
        vec::{Collector, Distributor},
        ConsumeFailure, ConsumeFault, Consumer, ProduceFailure, Producer,
    },
    parse_display::Display as ParseDisplay,
    starship::{context::Context, print},
    std::{
        io::{self, ErrorKind},
        rc::Rc,
    },
    toml::{value::Table, Value},
    ui::{
        CreateTerminalError, DisplayCmd, DisplayCmdFailure, Terminal, UserActionConsumer,
        UserActionFailure,
    },
};

/// An error creating an [`Interface`].
///
/// [`Interface`]: struct.Interface.html
#[derive(Debug, thiserror::Error)]
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
#[derive(Debug, market::ProduceFault, thiserror::Error)]
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
    Withdrawn(#[from] WithdrawnDemandFault),
}

/// An error creating a file.
#[derive(Debug, thiserror::Error)]
pub enum CreateFileError {
    /// An error triggering a file read.
    #[error(transparent)]
    Read(#[from] ProduceFailure<FileError>),
}

/// An error while reading a file.
#[derive(Debug, thiserror::Error)]
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
#[derive(Debug, thiserror::Error)]
enum Glitch {
    /// Unable to convert config file to Config.
    #[error("config file invalid format: {0}")]
    ConfigFormat(#[from] toml::de::Error),
}

/// An event that prevents [`Interface`] from consuming.
#[derive(Debug, ConsumeFault, thiserror::Error)]
pub(crate) enum ConsumeInputIssue {
    /// The application has quit.
    #[error("")]
    Quit,
    /// The application has encountered an error while consuming.
    #[error("")]
    Error(#[from] ConsumeInputError),
}

/// An error consuming input.
#[derive(Debug, ConsumeFault, thiserror::Error)]
pub enum ConsumeInputError {
    /// An error consuming a user input.
    #[error("")]
    Ui(#[from] UserActionFailure),
    /// An error consuming a file.
    #[error("")]
    File(#[from] ConsumeFileError),
    /// An error in [`Tongue`].
    #[error(transparent)]
    Translation(#[from] TranslationError),
    /// A thread was dropped.
    #[error(transparent)]
    Withdrawn(#[from] WithdrawnSupplyFault),
}

/// An error validating a file string.
#[derive(Debug, thiserror::Error)]
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

/// Implements [`CrossbeamProducer<Transmission>`] that can be pushed to [`Interface.producers`].
struct InternalLspProducer(Rc<Tongue>);

impl Producer for InternalLspProducer {
    type Good = Vec<Transmission>;
    type Failure = ProduceFailure<ProduceOutputError>;

    #[throws(Self::Failure)]
    fn produce(&self, good: Self::Good) {
        for transmission in good {
            self.0
                .transmitter()
                .produce(transmission)
                .map_err(ProduceFailure::map_fault)?
        }
    }
}

/// Implements [`CrossbeamConsumer<Reception>`] that can be pushed to [`Interface.consumers`].
struct InternalLspConsumer(Rc<Tongue>);

impl Consumer for InternalLspConsumer {
    type Good = Reception;
    type Failure = ConsumeFailure<ConsumeInputError>;

    #[throws(Self::Failure)]
    fn consume(&self) -> Self::Good {
        match self.0.receiver().consume() {
            Ok(good) => good,
            Err(ConsumeFailure::EmptyStock) => {
                throw!(ConsumeFailure::EmptyStock)
            }
            // If failure is fault, check status from thread to get more information.
            Err(ConsumeFailure::Fault(fault)) => {
                throw!(ConsumeInputError::from(match self.0.thread().consume() {
                    // Ok = thread terminated without error; EmptyStock = thread still running.
                    // TODO: Investigate if better to send indication of quitting if thread terminated.
                    Ok(_) | Err(ConsumeFailure::EmptyStock) => {
                        fault.into()
                    }
                    Err(ConsumeFailure::Fault(f)) => {
                        f
                    }
                }))
            }
        }
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
    tongue: Rc<Tongue>,
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
        let tongue = Rc::new(Tongue::new(file_command_producer.root_dir()));

        if let Some(file) = initial_file {
            file_command_producer.produce(FileCommand::Read { path: file })?
        }

        consumers.push(InternalUserActionConsumer(UserActionConsumer));
        consumers.push(InternalLspConsumer(Rc::clone(&tongue)));
        consumers.push(InternalFileConsumer(file_consumer));

        producers.push(InternalLspProducer(Rc::clone(&tongue)));
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
}

impl Consumer for Interface {
    type Good = Input;
    type Failure = ConsumeFailure<ConsumeInputIssue>;

    #[throws(Self::Failure)]
    fn consume(&self) -> Self::Good {
        match self.consumers.consume() {
            Ok(input) => {
                log::trace!("INPUT: {:?}", input);
                input
            }
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
        log::trace!("OUTPUT: {}", output);
        self.producers.produce(output.clone())?;

        // TODO: Instead of has_quit, use market::Trigger stored in producers.
        match output {
            Output::OpenFile { .. }
            | Output::UpdateView { .. }
            | Output::EditDoc { .. }
            | Output::UpdateHeader
            | Output::Question { .. }
            | Output::CloseDoc { .. }
            | Output::Command { .. } => {}
            Output::Quit => {
                self.has_quit.store(true, Ordering::Relaxed);
            }
        }
    }
}

/// An error occurred while converting a directory path to a URL.
#[derive(Debug, thiserror::Error)]
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
    Lsp(Reception),
}

impl From<File> for Input {
    #[inline]
    fn from(value: File) -> Self {
        Self::File(value)
    }
}

impl From<Reception> for Input {
    #[inline]
    fn from(value: Reception) -> Self {
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
    /// Closes a document.
    #[display("Close doc `{doc:?}`")]
    CloseDoc {
        /// The document.
        doc: TextDocumentIdentifier,
    },
    #[display("Edit doc `{doc:?}`: {edit:?}")]
    /// Edits a document.
    EditDoc {
        /// The file that is edited.
        doc: Document,
        /// The edit to be performed.
        edit: DocEdit,
    },
    /// Updates the area of the document that is shown.
    #[display("Update view to:\n{rows:?}")]
    UpdateView {
        /// The rows of the document to be shown.
        rows: Vec<RowText>,
    },
    /// Sets the header of the application.
    #[display("Update header")]
    UpdateHeader,
    /// Asks the user a question.
    #[display("Ask `{request:?}`")]
    Question {
        /// The request to be answered.
        request: ShowMessageRequestParams,
    },
    /// Adds an intake box.
    #[display("Command `{command}`")]
    Command {
        /// The prompt of the intake box.
        command: String,
    },
    /// Quit the application.
    #[display("Quit")]
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
            | Output::CloseDoc { .. }
            | Output::Quit => throw!(TryIntoFileCommandError::InvalidOutput),
        }
    }
}

impl TryFrom<Output> for Vec<Transmission> {
    type Error = TryIntoProtocolError;

    #[inline]
    #[throws(Self::Error)]
    fn try_from(value: Output) -> Self {
        match value {
            Output::CloseDoc { doc } => vec![Transmission::close_doc(doc)],
            Output::EditDoc { doc, edit } => match edit {
                DocEdit::Open { .. } => vec![Transmission::OpenDoc { doc: doc.into() }],
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
                DocEdit::Open { .. } | DocEdit::Update => Self::Rows { rows: doc.rows()? },
            },
            Output::UpdateView { rows } => Self::Rows { rows },
            Output::Question { request } => Self::Rows {
                rows: vec![RowText::new(vec![StyledText::new(
                    request.message,
                    Style::Default,
                )])],
            },
            Output::Command { command } => Self::Command { command },
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

                Self::Header {
                    header: print::get_prompt(context),
                }
            }
            Output::CloseDoc { .. } | Output::OpenFile { .. } | Output::Quit => {
                throw!(TryIntoDisplayCmdError::InvalidOutput)
            }
        }
    }
}

/// An error converting [`Output`] into a [`Protocol`].
#[derive(Debug, thiserror::Error)]
pub(crate) enum TryIntoFileCommandError {
    /// Invalid `Output`.
    #[error("")]
    InvalidOutput,
}

/// An error converting [`Output`] into a [`DisplayCmd`].
#[derive(Debug, thiserror::Error)]
pub(crate) enum TryIntoDisplayCmdError {
    /// Invalid `Output`.
    #[error("")]
    InvalidOutput,
    /// Conversion attempt failed due to [`ScopeFromRangeError`].
    #[error(transparent)]
    App(#[from] ScopeFromRangeError),
}

/// An error converting [`Output`] into a [`Protocol`].
#[derive(Clone, Copy, Debug, thiserror::Error)]
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
        version: i32,
    },
    /// Updates the display of the document.
    Update,
}

/// The changes to be made to a document.
#[derive(Clone, Debug)]
pub(crate) struct DocChange {
    /// The new text.
    new_text: String,
    /// The version.
    version: i64,
}

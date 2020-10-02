//! Implements the interface for all input and output to the application.
mod fs;
mod ui;

pub(crate) use {
    fs::File,
    ui::{Dimensions, Unit, UserAction},
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
    fs::{ConsumeFileError, FileCommand, FileError, FileSystem, RootDirError},
    log::error,
    lsp_types::ShowMessageRequestParams,
    market::{ClosedMarketError, Collector, ConsumeFailure, Consumer, ProduceFailure, Producer},
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
    CreateFile(#[from] CreateFileError),
}

/// An error while writing output.
#[derive(Debug, ThisError)]
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
#[derive(Debug, ThisError)]
pub(crate) enum ConsumeInputIssue {
    /// The application has quit.
    #[error("")]
    Quit,
    /// The application has encountered an error while consuming.
    #[error("")]
    Error(#[from] ConsumeInputError),
}

/// An error consuming input.
#[derive(Debug, ThisError)]
pub enum ConsumeInputError {
    /// An error consuming a user input.
    #[error("")]
    Ui(#[from] UserActionFailure),
    /// The queue is closed.
    #[error("")]
    Closed(#[from] ClosedMarketError),
    /// An error consuming a file.
    #[error("")]
    File(#[from] ConsumeFileError),
}

/// An error validating a file string.
#[derive(Debug, ThisError)]
pub(crate) enum InvalidFileStringError {
    /// An error determining the root directory.
    #[error("")]
    RootDir(#[from] RootDirError),
}

/// The interface between the application and all external components.
#[derive(Debug)]
pub(crate) struct Interface {
    /// A [`Collector`] of all input [`Consumer`]s.
    consumers: Collector<Input, ConsumeInputError>,
    /// Manages the user interface.
    user_interface: Terminal,
    /// The interface of the application with all language servers.
    tongue: Tongue,
    /// The interface with the file system.
    file_system: FileSystem,
    /// The application has quit.
    has_quit: AtomicBool,
}

impl Interface {
    /// Creates a new interface.
    #[throws(CreateInterfaceError)]
    pub(crate) fn new(initial_file: Option<String>) -> Self {
        let mut consumers = Collector::new();
        consumers.convert_into_and_push(UserActionConsumer::new());
        let file_system = FileSystem::new()?;

        let interface = Self {
            consumers,
            user_interface: Terminal::new()?,
            tongue: Tongue::new(file_system.root_dir()),
            file_system,
            has_quit: AtomicBool::new(false),
        };

        if let Some(file) = initial_file {
            interface.open_file(file)?;
        }

        interface
    }

    /// Reads the file at `url`.
    #[throws(CreateFileError)]
    fn open_file(&self, path: String) {
        self.file_system.produce(FileCommand::Read { path })?
    }

    /// Edits the doc at `url`.
    #[throws(ProduceFailure<ProduceOutputError>)]
    fn edit_doc(&self, doc: &Document, edit: &DocEdit) {
        if let DocEdit::Open { .. } | DocEdit::Update = edit {
            self.user_interface
                .produce(DisplayCmd::Rows { rows: doc.rows() })
                .map_err(ProduceFailure::map_into)?;
        }
    }

    /// Waits until `self.tongue` finishes.
    pub(crate) fn join(&mut self) {
        self.tongue.join();
    }
}

impl Consumer for Interface {
    type Good = Input;
    type Error = ConsumeInputIssue;

    #[throws(ConsumeFailure<Self::Error>)]
    fn consume(&self) -> Self::Good {
        match self.consumers.consume() {
            Ok(input) => input,
            Err(ConsumeFailure::Error(failure)) => {
                throw!(ConsumeFailure::Error(Self::Error::Error(failure)))
            }
            Err(ConsumeFailure::EmptyStock) => match self.tongue.consume() {
                Ok(lang_input) => lang_input.into(),
                Err(_) => match self.file_system.consume() {
                    Ok(file) => file.into(),
                    Err(ConsumeFailure::Error(failure)) => {
                        throw!(ConsumeFailure::Error(Self::Error::Error(failure.into())))
                    }
                    Err(ConsumeFailure::EmptyStock) => {
                        if self.has_quit.load(Ordering::Relaxed) {
                            throw!(ConsumeFailure::Error(Self::Error::Quit));
                        } else {
                            throw!(ConsumeFailure::EmptyStock);
                        }
                    }
                },
            },
        }
    }
}

impl Producer for Interface {
    type Good = Output;
    type Error = ProduceOutputError;

    #[throws(ProduceFailure<Self::Error>)]
    fn produce(&self, output: Self::Good) {
        if let Ok(procedure) = ClientStatement::try_from(output.clone()) {
            if let Err(error) = self.tongue.produce(procedure) {
                error!("Unable to write to language server: {}", error);
            }
        }

        match output {
            Output::OpenFile { path } => {
                self.open_file(path)
                    .map_err(|error| ProduceFailure::Error(Self::Error::from(error)))?;
            }
            Output::EditDoc { doc, edit } => {
                self.edit_doc(&doc, &edit)?;
            }
            Output::UpdateHeader => {
                let mut context =
                    Context::new_with_dir(ArgMatches::new(), self.file_system.root_dir().path());

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
                    .map_err(ProduceFailure::map_into)?
            }
            Output::Question { request } => self
                .user_interface
                .produce(DisplayCmd::Rows {
                    rows: vec![request.message],
                })
                .map_err(ProduceFailure::map_into)?,
            Output::Command { command } => self
                .user_interface
                .produce(DisplayCmd::Rows {
                    rows: vec![command],
                })
                .map_err(ProduceFailure::map_into)?,
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
            | Output::Question { .. }
            | Output::Quit => throw!(TryIntoProtocolError::InvalidOutput),
        }
    }
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

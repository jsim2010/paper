//! Implements the interface for all input and output to the application.
#![allow(clippy::pattern_type_mismatch)]
mod fs;
mod ui;

pub(crate) use {
    fs::File,
    ui::{Dimensions, RowText, Style, StyledText, Unit, UserAction},
};

use {
    clap::ArgMatches,
    core::{
        convert::TryFrom,
        fmt::{self, Display, Formatter},
    },
    docuglot::{Reception, Tongue, Transmission},
    fehler::{throw, throws},
    fs::{
        create_file_system, ReadFileGlitch, FileCommand,
        FileError, RootDirError,
    },
    lsp_types::{TextDocumentItem, ShowMessageRequestParams, TextDocumentIdentifier},
    market::{
        Agent, Consumer, Failure, ConsumptionFlaws, Recall, ProductionFlaws, Producer, channel::{WithdrawnDemand, WithdrawnSupply},
    },
    markets::{channel_std::StdFiniteChannel, convert::SpecificationFlaws, collections::{Collector, Distributor}},
    starship::{context::Context, print},
    toml::{value::Table, Value},
    ui::{
        InitTerminalError, DisplayCmd, DisplayCmdFailure, 
        ConsumeActionFault,
    },
};

/// An error creating an [`Interface`].
#[derive(Debug, thiserror::Error)]
#[error(transparent)]
pub enum CreateInterfaceError {
    /// An error creating a [`Terminal`].
    Terminal(#[from] InitTerminalError),
    /// An error generating the root directory [`Url`].
    RootDir(#[from] RootDirError),
    /// An error triggering the read of a file.
    ReadFile(#[from] FileError),
}

#[throws(CreateInterfaceError)]
pub(crate) fn create(initial_file: Option<String>) -> (InputConsumer, OutputProducer) {
    let (presenter, listener) = ui::init()?;
    let mut consumers: Collector<Input, ConsumeInputFault> = Collector::new("InputCollector");
    //let mut producers: Distributor<Output, ProduceOutputFault> = Distributor::new();
    let (file_command_producer, file_consumer) = create_file_system()?;
    let (transmitter, receiver, tongue) = docuglot::create_tongue(file_command_producer.root_dir());
    let (quitter, quit_consumer) = StdFiniteChannel::establish("Quitter", 1);

    if let Some(file) = initial_file {
        file_command_producer.produce(FileCommand::Read { path: file })?
    }

    consumers.push(listener);
    consumers.push(receiver);
    consumers.push(file_consumer);
    consumers.push(quit_consumer);

    //producers.push(transmitter);
    //producers.push(presenter);
    //producers.push(file_command_producer);
    //producers.push(quitter);
}

pub(crate) struct InputConsumer {
    /// All [`Consumer`]s.
    consumers: Collector<Input, ConsumeInputFault>,
}

impl Agent for InputConsumer {
    type Good = Input;
}

impl Consumer for InputConsumer {
    type Flaws = ConsumptionFlaws<ConsumeInputFault>;

    #[throws(Failure<Self::Flaws>)]
    fn consume(&self) -> Self::Good {
        let input = self.consumers.consume()?;

        log::trace!("INPUT: {:?}", input);
        input
    }
}

impl Display for InputConsumer {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "Input Consumer")
    }
}

pub(crate) struct OutputProducer {
    /// All [`Producer`]s.
    producers: Distributor<OutputKind, Output, ProduceOutputFault>,
}

impl Agent for OutputProducer {
    type Good = Output;
}

impl Display for OutputProducer {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "Output Producer")
    }
}

impl Producer for OutputProducer {
    type Flaws = SpecificationFlaws<ProductionFlaws<ProduceOutputFault>>;

    #[throws(Recall<Self::Flaws, Self::Good>)]
    fn produce(&self, output: Self::Good) {
        log::trace!("OUTPUT: {}", output);
        self.producers.produce(output)?;
    }
}

/// An error consuming input.
#[derive(Debug, thiserror::Error)]
#[error(transparent)]
pub enum ConsumeInputFault {
    /// An error consuming a user input.
    Ui(#[from] ConsumeActionFault),
    Withdrawn(#[from] WithdrawnSupply),
}

/// An error while writing output.
#[derive(Debug, thiserror::Error)]
pub enum ProduceOutputFault {
    /// An error with a file.
    #[error("")]
    File(#[from] FileError),
    /// Produce error from ui.
    #[error("{0}")]
    UiProduce(#[from] DisplayCmdFailure),
    #[error(transparent)]
    WithdrawnLanguage(#[from] WithdrawnDemand),
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

/// An error validating a file string.
#[derive(Debug, thiserror::Error)]
pub(crate) enum InvalidFileStringError {
    /// An error determining the root directory.
    #[error("")]
    RootDir(#[from] RootDirError),
}

#[derive(Eq, Hash, PartialEq)]
enum OutputKind {
}

/// The interface between the application and all external processes.
#[derive(Debug)]
pub(crate) struct Interface {
    /// All [`Consumer`]s.
    consumers: Collector<Input, ConsumeInputFault>,
    /// All [`Producer`]s.
    //producers: Distributor<OutputKind, Output, ProduceOutputFault>,
    /// The interface of the application with all language servers.
    tongue: Tongue,
}

impl Interface {
    /// Creates a new [`Interface`].
    ///
    /// If `initial_file` is [`Some`], the file at the given path shall be opened.
    #[throws(CreateInterfaceError)]
    //pub(crate) fn new(initial_file: Option<String>) -> Self {
    //    //let (presenter, listener) = ui::init()?;
    //    let mut consumers: Collector<Input, ConsumeInputFault> = Collector::new();
    //    //let mut producers: Distributor<Output, ProduceOutputFault> = Distributor::new();
    //    //let (file_command_producer, file_consumer) = create_file_system()?;
    //    let (transmitter, receiver, tongue) = docuglot::create_tongue(file_command_producer.root_dir());
    //    //let (quit_trigger, quit_hammer) = sync::create_lock();

    //    //if let Some(file) = initial_file {
    //    //    file_command_producer.produce(FileCommand::Read { path: file })?
    //    //}

    //    //consumers.push(listener);
    //    //consumers.push(receiver);
    //    //consumers.push(file_consumer);
    //    //consumers.push(quit_hammer);

    //    //producers.push(transmitter);
    //    //producers.push(presenter);
    //    //producers.push(file_command_producer);
    //    //producers.push(quit_trigger);

    //    Self {
    //        consumers,
    //        //producers,
    //        tongue,
    //    }
    //}
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
    Glitch(ReadFileGlitch),
    Quit,
}

impl From<Result<File, ReadFileGlitch>> for Input {
    #[inline]
    fn from(result: Result<File, ReadFileGlitch>) -> Self {
        match result {
            Ok(file) => Self::File(file),
            Err(glitch) => Self::Glitch(glitch),
        }
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

impl From<()> for Input {
    #[inline]
    fn from(_: ()) -> Self {
        Self::Quit
    }
}

/// An output.
#[derive(Clone, Debug, parse_display::Display)]
pub(crate) enum Output {
    /// Retrieves the URL and text of a file.
    #[display("Get file `{path}`")]
    ReadFile {
        /// The relative path of the file.
        path: String,
    },
    /// Closes a document.
    #[display("Close doc `{doc:?}`")]
    CloseDoc {
        /// The document.
        doc: TextDocumentIdentifier,
    },
    /// Opens a document.
    #[display("Open doc `{doc:?}`")]
    OpenDoc {
        doc: TextDocumentItem,
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
    #[display("GetDocSymbol")]
    GetDocSymbol{
        doc: TextDocumentIdentifier,
    },
    #[display("Shutdown")]
    Shutdown,
}

impl TryFrom<Output> for FileCommand {
    type Error = Output;

    #[inline]
    #[throws(Self::Error)]
    fn try_from(value: Output) -> Self {
        match value {
            Output::ReadFile { path } => Self::Read { path },
            Output::Command { .. }
            | Output::OpenDoc { .. }
            | Output::UpdateHeader
            | Output::UpdateView { .. }
            | Output::Question { .. }
            | Output::CloseDoc { .. }
            | Output::GetDocSymbol { .. }
            | Output::Shutdown
            | Output::Quit => throw!(value),
        }
    }
}

impl From<FileCommand> for Output {
    fn from(file_cmd: FileCommand) -> Self {
        match file_cmd {
            FileCommand::Read{path} => Output::ReadFile{path},
        }
    }
}

impl TryFrom<Output> for Transmission {
    type Error = Output;

    #[inline]
    #[throws(Self::Error)]
    fn try_from(value: Output) -> Self {
        match value {
            Output::CloseDoc { doc } => Transmission::close_doc(doc),
            Output::OpenDoc { doc } => Transmission::OpenDoc { doc },
            Output::GetDocSymbol { doc } => Transmission::GetDocumentSymbol { doc },
            Output::Shutdown => Transmission::Shutdown,
            Output::ReadFile { .. }
            | Output::Command { .. }
            | Output::UpdateHeader
            | Output::UpdateView { .. }
            | Output::Question { .. }
            | Output::Quit => throw!(value),
        }
    }
}

impl From<Transmission> for Output {
    fn from(transmission: Transmission) -> Self {
        match transmission {
            Transmission::OpenDoc{doc} => Output::OpenDoc{doc},
            Transmission::CloseDoc{doc} => Output::CloseDoc{doc},
            Transmission::GetDocumentSymbol{doc} => Output::GetDocSymbol {doc},
            Transmission::Shutdown => Output::Shutdown,
        }
    }
}

impl TryFrom<Output> for DisplayCmd {
    type Error = Output;

    #[inline]
    #[throws(Self::Error)]
    fn try_from(value: Output) -> Self {
        match value {
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
            Output::OpenDoc { .. } | Output::GetDocSymbol{..} | Output::Shutdown | Output::CloseDoc { .. } | Output::ReadFile { .. } | Output::Quit => {
                throw!(value)
            }
        }
    }
}

impl From<DisplayCmd> for Output {
    fn from(display_cmd: DisplayCmd) -> Self {
        match display_cmd {
            DisplayCmd::Rows{rows} => Self::UpdateView{rows},
            DisplayCmd::Command{command} => Self::Command{command},
            DisplayCmd::Header{..} => Self::UpdateHeader,
        }
    }
}

impl TryFrom<Output> for () {
    type Error = Output;

    #[throws(Self::Error)]
    fn try_from(output: Output) -> Self {
        if let Output::Quit = output {
            ()
        } else {
            throw!(output)
        }
    }
}

impl From<()> for Output {
    fn from(_: ()) -> Self {
        Self::Quit
    }
}

/// The changes to be made to a document.
#[derive(Clone, Debug)]
pub(crate) struct DocChange {
    /// The new text.
    new_text: String,
    /// The version.
    version: i64,
}

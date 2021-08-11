//! A terminal-based text editor with goals to maximize simplicity and efficiency.
//!
//! # Design Goals
//! 1) All functionality shall be able to be performed via the keys reachable from the home row. Where it makes sense, functionality may additionally be performed via the mouse and other keys.
//! 2) All user input shall be modal, i.e. keys may implement different functionality depending on the current mode of the application.
//! 3) Paper shall utilize already implemented tools and commands wherever possible; specifically paper shall support the [Language Server Protocol].
//!
//! [Language Server Protocol]: https://microsoft.github.io/language-server-protocol/
mod app;
mod io;
mod logging;
mod orient;

// Export so that other crates can build Arguments.
pub use logging::LogConfig;

use {
    app::{Processor, ScopeFromRangeError},
    // Avoid use of std Option.
    core::option::Option,
    fehler::{throw, throws},
    io::{
        Input, ConsumeInputFault, CreateInterfaceError, Interface, ProduceOutputFault,
    },
    logging::InitLoggerError,
    market::{Consumer, Producer, Recall},
    markets::convert::SpecificationDefect,
    structopt::StructOpt,
};

/// An error from which `paper` is unable to recover.
#[derive(Debug, thiserror::Error)]
pub enum Failure {
    /// An error creating `paper`.
    #[error(transparent)]
    Create(#[from] CreateError),
    /// An error running `paper`.
    #[error(transparent)]
    Run(#[from] RunError),
}

/// An error creating a [`Paper`].
///
/// [`Paper`]: struct.Paper.html
#[derive(Debug, thiserror::Error)]
pub enum CreateError {
    /// An error initializing the application logger.
    #[error(transparent)]
    Logger(#[from] InitLoggerError),
    /// An error creating the [`Interface`].
    #[error("Failed to {0}")]
    Interface(#[from] CreateInterfaceError),
}

/// An error running `paper`.
#[derive(Debug, thiserror::Error)]
pub enum RunError {
    /// An error consuming an input.
    #[error("Failed to consume input: {0}")]
    Consume(#[from] ConsumeInputFault),
    /// An error producing an output.
    #[error("Failed to produce output: {0}")]
    Produce(#[from] SpecificationDefect<ProduceOutputFault>),
    /// An error occurred processing an input.
    #[error("Failed to process input: {0}")]
    Process(#[from] ScopeFromRangeError),
}

/// Arguments for [`Paper`] initialization.
#[derive(Clone, Debug, Default, StructOpt)]
pub struct Arguments {
    /// The file to be viewed.
    #[structopt(value_name("FILE"))]
    file: Option<String>,
    #[allow(clippy::missing_docs_in_private_items)] // Flattened structs do not allow doc comments.
    #[structopt(flatten)]
    log_config: LogConfig,
}

/// An instance of the `paper` application.
///
/// When [`Paper`] is dropped, it shall kill all spawned processes and return the user interface to its previous state.
///
/// # Example(s)
///
/// ```no_run
/// use paper::{Arguments, Failure, Paper};
/// # fn main() -> Result<(), Failure> {
///
/// Paper::new(Arguments::default())?.run()?;
/// # Ok(())
/// # }
/// ```
#[derive(Debug)]
pub struct Paper {
    /// The interface with external processes.
    io: Interface,
    /// The application processor.
    processor: Processor,
}

impl Paper {
    /// Creates a new [`Paper`].
    ///
    /// # Errors
    ///
    /// Shall throw [`CreateError`] if any unrecoverable error is caught.
    #[inline]
    #[throws(CreateError)]
    pub fn new(arguments: Arguments) -> Self {
        // Logger initialization is done first so all other code can use it.
        logging::init(arguments.log_config)?;

        Self {
            io: Interface::new(arguments.file)?,
            processor: Processor::new(),
        }
    }

    /// Runs the application.
    ///
    /// This function shall run until `paper` has been terminated. Termination occurs when the application is ordered to quit or a [`Failure`] is thrown.
    ///
    /// # Errors
    ///
    /// If any unrecoverable error is thrown, a [`RunError`] shall be thrown.
    #[inline]
    #[throws(RunError)]
    pub fn run(&mut self) {
        if let Err(error) = self.execute() {
            log::error!("{}", error);
            throw!(error);
        }
    }

    /// Loops through execution until `paper` has been **terminated**.
    ///
    /// # Errors
    ///
    /// If any unrecoverable error is thrown, a [`RunError`] shall be thrown.
    #[throws(RunError)]
    fn execute(&mut self) {
        loop {
            match self.io.demand()? {
                Input::Quit => {
                    log::info!("Application quitting");
                    break;
                }
                input => self.io.force_all(self.processor.process(input)?).map_err(Recall::into_error)?,
            }
        }
    }
}

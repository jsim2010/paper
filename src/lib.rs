//! A terminal-based text editor with goals to maximize simplicity and efficiency.
//!
//! # Design Goals
//! 1) All functionality shall be able to be performed via the keys reachable from the home row. Where it makes sense, functionality may additionally be performed via the mouse and other keys.
//! 2) All user input shall be modal, i.e. keys may implement different functionality depending on the current mode of the application.
//! 3) Paper shall utilize already implemented tools and commands wherever possible; specifically paper shall support the [Language Server Protocol].
//!
//! [Language Server Protocol]: https://microsoft.github.io/language-server-protocol/
#![allow(
    clippy::unreachable, // unreachable added by derive(Enum).
    clippy::use_self, // False positive on format macro.
)]

mod app;
pub mod io;
mod logging;

use {
    app::Processor,
    clap::ArgMatches,
    core::option::Option,
    fehler::{throw, throws},
    io::{
        ConsumeInputError, ConsumeInputIssue, CreateInterfaceError, Interface, ProduceOutputError,
    },
    log::{error, info},
    logging::{InitLoggerError, LogConfig},
    market::{Consumer, Producer},
    thiserror::Error as ThisError,
};

/// Arguments for [`Paper`] initialization.
///
/// [`Paper`]: ../struct.Paper.html
#[derive(Clone, Debug)]
pub struct Arguments<'a> {
    /// The file to be viewed.
    ///
    /// [`None`] indicates that no file will be viewed.
    ///
    /// [`None`]: https://doc.rust-lang.org/core/option/enum.Option.html#variant.None
    pub file: Option<&'a str>,
    /// The configuration of the logger.
    pub log_config: LogConfig,
}

impl<'a> From<&'a ArgMatches<'a>> for Arguments<'a> {
    #[inline]
    fn from(value: &'a ArgMatches<'a>) -> Self {
        Self {
            file: value.value_of("file"),
            log_config: LogConfig::from(value),
        }
    }
}

impl Default for Arguments<'_> {
    #[inline]
    fn default() -> Self {
        Self {
            file: None,
            log_config: LogConfig::default(),
        }
    }
}

/// An instance of the `paper` application.
///
/// Once [`Paper`] has started running, it shall continue until it has been terminated. **Termination** occurs when the application is ordered to quit or an unrecoverable error is thrown.
///
/// When [`Paper`] is dropped, it shall kill all spawned processes and return the user interface to its previous state.
///
/// # Examples
///
/// ```no_run
/// use paper::{Arguments, Failure, Paper};
/// # fn main() -> Result<(), Failure> {
///
/// Paper::new(&Arguments::default())?.run()?;
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
    /// Creates a new instance of `paper`.
    #[inline]
    #[throws(CreateError)]
    pub fn new(arguments: &Arguments<'_>) -> Self {
        // Logger is created first so all other parts can use it.
        logging::init(arguments.log_config)?;

        Self {
            io: Interface::new(arguments.file)?,
            processor: Processor::new(),
        }
    }

    /// Runs the application.
    ///
    /// This function shall run until `paper` has been **terminated**.
    ///
    /// # Errors
    ///
    /// If any unrecoverable error is thrown, a [`RunError`] shall be thrown.
    ///
    /// [`RunError`]: enum.RunError.html
    #[inline]
    #[throws(RunError)]
    pub fn run(&mut self) {
        if let Err(error) = self.execute() {
            error!("{}", error);
            throw!(error);
        }

        info!("Application quitting");
    }

    /// Loops through execution until `paper` has been **terminated**.
    ///
    /// # Errors
    ///
    /// If any unrecoverable error is thrown, a [`RunError`] shall be thrown.
    ///
    /// [`RunError`]: enum.RunError.html
    #[throws(RunError)]
    fn execute(&mut self) {
        loop {
            match self.io.demand() {
                Ok(input) => self.io.force_all(self.processor.process(input))?,
                Err(issue) => {
                    if let ConsumeInputIssue::Error(error) = issue {
                        throw!(error);
                    }

                    break;
                }
            }
        }
    }
}

/// An error from which `paper` is unable to recover.
#[derive(Debug, ThisError)]
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
#[derive(Debug, ThisError)]
pub enum CreateError {
    /// An error creating the application logger.
    #[error("Failed to initialize logger: {0}")]
    Logger(#[from] InitLoggerError),
    /// An error creating the [`Interface`].
    ///
    /// [`Interface`]: io/struct.Interface.html
    #[error("Failed to create application: {0}")]
    Interface(#[from] CreateInterfaceError),
}

/// An error running `paper`.
#[derive(Debug, ThisError)]
pub enum RunError {
    /// An error consuming an input.
    #[error("Failed to consume input: {0}")]
    Consume(#[from] ConsumeInputError),
    /// An error producing an output.
    #[error("Failed to produce output: {0}")]
    Produce(#[from] ProduceOutputError),
}

//! A terminal-based text editor with goals to maximize simplicity and efficiency.
//!
//! # Goals
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
    fehler::{throw, throws},
    io::{
        ConsumeInputError, ConsumeInputIssue, CreateInterfaceError, Interface, ProduceOutputError,
    },
    log::{error, info},
    logging::{Config, InitLoggerError},
    market::{Consumer, Producer},
    thiserror::Error as ThisError,
};

/// A configuration of the initialization of a [`Paper`].
///
/// [`Paper`]: ../struct.Paper.html
#[derive(Clone, Debug)]
pub struct Arguments<'a> {
    /// The file to be viewed.
    ///
    /// [`None`] indicates that no file will be viewed.
    ///
    /// [`None`]: https://doc.rust-lang.org/std/option/enum.Option.html#variant.None
    pub file: Option<&'a str>,
    /// The configuration of the logger.
    pub log_config: Config,
}

impl<'a> From<&'a ArgMatches<'a>> for Arguments<'a> {
    #[inline]
    fn from(value: &'a ArgMatches<'a>) -> Self {
        Self {
            file: value.value_of("file"),
            log_config: Config::from(value),
        }
    }
}

impl Default for Arguments<'_> {
    #[inline]
    fn default() -> Self {
        Self {
            file: None,
            log_config: Config::default(),
        }
    }
}

/// An instance of the `paper` program.
///
/// # Examples
///
/// ```no_run
/// use paper::{Arguments, Failure, Paper};
/// # fn main() -> Result<(), Failure> {
///
/// Paper::new(&Arguments::default())?.run()?;
/// Ok(())
/// # }
/// ```
#[derive(Debug)]
pub struct Paper {
    /// The interface.
    io: Interface,
    /// The processor.
    processor: Processor,
}

impl Paper {
    /// Creates a new instance of `paper`.
    ///
    /// # Errors
    ///
    /// If any error is encountered during creation, a [`CreatePaperError`] shall be returned.
    ///
    /// [`CreatePaperError`]: enum.CreatePaperError.html
    #[inline]
    #[throws(CreatePaperError)]
    pub fn new(arguments: &Arguments<'_>) -> Self {
        // First step is to create logger.
        logging::init(arguments.log_config)?;

        Self {
            io: Interface::new(arguments.file)?,
            processor: Processor::new(),
        }
    }

    /// Runs the program and logs the result.
    ///
    /// # Errors
    ///
    /// If any error from which `paper` is unable to recover is encountered, a [`RunPaperError`] shall be returned. In case of a failure, `paper` shall make all efforts to cleanly exit (i.e. kill all processes and return the terminal to a clean state), but a clean exit shall not be guaranteed.
    #[inline]
    #[throws(RunPaperError)]
    pub fn run(&mut self) {
        if let Err(error) = self.execute() {
            error!("Encountered error: {}", error);
            throw!(error);
        }

        info!("Application quitting");
    }

    /// Loops through program execution until a failure occurs or the application quits.
    ///
    /// # Errors
    ///
    /// If any error from which `paper` is unable to recover is encountered, a [`RunPaperError`] shall be returned. In case of a failure, `paper` shall make all efforts to cleanly exit (i.e. kill all processes and return the terminal to a clean state), but a clean exit shall not be guaranteed.
    ///
    /// [`RunPaperError`]: enum.RunPaperError.html
    #[throws(RunPaperError)]
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
    Create(#[from] CreatePaperError),
    /// An error running `paper`.
    #[error(transparent)]
    Run(#[from] RunPaperError),
}

/// An error creating a [`Paper`].
///
/// [`Paper`]: struct.Paper.html
#[derive(Debug, ThisError)]
pub enum CreatePaperError {
    /// An error creating the application logger.
    #[error("failed to initialize logger: {0}")]
    Logger(#[from] InitLoggerError),
    /// An error creating an [`Interface`].
    ///
    /// [`Interface`]: io/struct.Interface.html
    #[error("failed to create application: {0}")]
    Interface(#[from] CreateInterfaceError),
}

/// An error running `paper`.
#[derive(Debug, ThisError)]
pub enum RunPaperError {
    /// An error consuming an input.
    #[error("failed to consume input: {0}")]
    Consume(#[from] ConsumeInputError),
    /// An error producing an output.
    #[error("failed to produce output: {0}")]
    Produce(#[from] ProduceOutputError),
}

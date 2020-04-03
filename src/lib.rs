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
    clippy::use_self, // Flags format macro.
)]

mod app;
pub mod io;

pub use io::Arguments;

use {
    app::Processor,
    io::{
        ConsumeInputError, ConsumeInputIssue, CreateInterfaceError, Interface, ProduceOutputError,
    },
    market::{Consumer, Producer},
    thiserror::Error,
};

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
    pub fn new(arguments: &Arguments<'_>) -> Result<Self, CreatePaperError> {
        Ok(Self {
            // Create Interface first to create logger as early as possible.
            io: Interface::new(arguments)?,
            processor: Processor::new(),
        })
    }

    /// Loops through program execution until a failure occurs or the application quits.
    ///
    /// # Errors
    ///
    /// If any error from which `paper` is unable to recover is encountered, a [`RunPaperError`] shall be returned. In case of a failure, `paper` shall make all efforts to cleanly exit (i.e. kill all processes and return the terminal to a clean state), but a clean exit shall not be guaranteed.
    ///
    /// [`RunPaperError`]: enum.RunPaperError.html
    #[inline]
    pub fn run(&mut self) -> Result<(), RunPaperError> {
        let mut result = Ok(());

        loop {
            match self.io.demand() {
                Ok(input) => self.io.force_all(self.processor.process(input))?,
                Err(ConsumeInputIssue::Quit) => {
                    break;
                }
                Err(ConsumeInputIssue::Error(error)) => {
                    result = Err(error.into());
                    break;
                }
            }
        }

        result
    }
}

/// An error from which `paper` is unable to recover.
#[derive(Debug, Error)]
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
#[derive(Debug, Error)]
pub enum CreatePaperError {
    /// An error creating an [`Interface`].
    ///
    /// [`Interface`]: io/struct.Interface.html
    #[error("failed to create interface: {0}")]
    Interface(#[from] CreateInterfaceError),
}

/// An error running `paper`.
#[derive(Debug, Error)]
pub enum RunPaperError {
    /// An error consuming an input.
    #[error("failed to consume input: {0}")]
    Consume(#[from] ConsumeInputError),
    /// An error producing an output.
    #[error("failed to produce output: {0}")]
    Produce(#[from] ProduceOutputError),
}

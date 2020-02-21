//! A terminal-based text editor with goals to maximize simplicity and efficiency.
//!
//! # Functional Overview
//! 1) All functionality shall be able to be performed via the keys reachable from the home row. Where it makes sense, functionality may additionally be performed via the mouse and other keys.
//! 2) All input shall be modal, i.e. keys shall implement different functionality depending on the current mode of the application.
//! 3) Paper shall utilize already implemented tools and commands wherever possible; specifically paper shall support the [Language Server Protocol].
//!
//! ## Upcoming
//! - Text manipulation shall involve a 3-step process of identifying the location should occur, marking that location, and then performing the desired edit.
//!
//! [Language Server Protocol]: https://microsoft.github.io/language-server-protocol/
#![warn(
    absolute_paths_not_starting_with_crate,
    anonymous_parameters,
    bare_trait_objects,
    box_pointers,
    deprecated_in_future,
    elided_lifetimes_in_paths,
    ellipsis_inclusive_range_patterns,
    explicit_outlives_requirements,
    keyword_idents,
    macro_use_extern_crate,
    missing_copy_implementations,
    missing_debug_implementations,
    missing_docs,
    missing_doc_code_examples,
    private_doc_tests,
    single_use_lifetimes,
    trivial_casts,
    trivial_numeric_casts,
    unreachable_pub,
    unsafe_code,
    unstable_features,
    unused_extern_crates,
    unused_import_braces,
    unused_labels,
    unused_lifetimes,
    unused_qualifications,
    unused_results,
    variant_size_differences,
    clippy::cargo,
    clippy::nursery,
    clippy::pedantic,
    clippy::restriction
)]
#![allow(
    clippy::implicit_return, // Goes against rust convention.
    clippy::suspicious_arithmetic_impl, // Not always valid; issues should be detected by tests or other lints.
    clippy::suspicious_op_assign_impl, // Not always valid; issues should be detected by tests or other lints.
    box_pointers, // Generally okay.
    variant_size_differences, // Generally okay.
)]
// Temporary allows.
#![allow(
    clippy::multiple_crate_versions, // Requires redox_users update to avoid multiple versions of rand_core.
    // See <https://gitlab.redox-os.org/redox-os/users/merge_requests/30>
    clippy::unreachable, // unreachable added by derive(Enum).
    clippy::use_debug, // Flags debug formatting in Debug trait.
    single_use_lifetimes, // Flags PartialEq derive.
)]

mod app;
pub mod io;

pub use io::Arguments;

use {
    app::Processor,
    io::{CreateInterfaceError, FlushCommandsError, Interface, RecvInputError, WriteOutputError},
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
/// Paper::new(Arguments::default())?.run()?;
///
/// Ok(())
/// # }
/// ```
#[derive(Debug)]
pub struct Paper {
    /// The interface between the application and everything else.
    io: Interface,
    /// The processor of the application.
    processor: Processor,
}

impl Paper {
    /// Creates a new instance of `paper`.
    ///
    /// # Errors
    ///
    /// If any error is encountered during creation, a [`CreateInterfaceError`] shall be returned.
    ///
    /// [`CreateInterfaceError`]: io/struct.CreateInterfaceError.html
    #[inline]
    pub fn new(arguments: Arguments<'_>) -> Result<Self, CreateInterfaceError> {
        Ok(Self {
            processor: Processor::new(),
            io: Interface::new(arguments)?,
        })
    }

    /// Loops through program execution until a [`RunPaperError`] occurs or the application quits.
    ///
    /// # Errors
    ///
    /// If any error from which `paper` is unable to recover is encountered, a [`RunPaperError`] shall be returned after `paper` makes all efforts to kill all processes and return the terminal to a clean state. In this case, a clean exit shall not be guaranteed.
    ///
    /// [`RunPaperError`]: struct.RunPaperError.html
    #[inline]
    pub fn run(&mut self) -> Result<(), RunPaperError> {
        loop {
            if !self.step()? {
                break;
            }
        }

        Ok(())
    }

    /// Executes a single run of the runtime loop and returns if the program should keep running.
    #[inline]
    fn step(&mut self) -> Result<bool, RunPaperError> {
        let mut keep_running = true;

        let input = self.io.recv()?;
            for output in self.processor.process(input) {
                keep_running &= self
                    .io
                    .write(&output)
                    .map_err(|error| RunPaperError::Write {
                        output: format!("{:?}", output),
                        error,
                    })?;
            }

            self.io.flush()?;

        Ok(keep_running)
    }
}

use crate::io::ui::Drain;

/// An error from which `paper` is unable to recover.
#[derive(Debug, Error)]
pub enum Failure {
    /// An error while creating a [`Paper`].
    #[error("failed to create interface: {0}")]
    Create(#[from] CreateInterfaceError),
    /// An error while `paper` is running.
    #[error("{0}")]
    Run(#[from] RunPaperError),
}

/// An error while `paper` is running.
#[derive(Debug, Error)]
pub enum RunPaperError {
    /// An error receiving an [`Input`].
    ///
    /// [`Input`]: io/struct.Input.html
    #[error("failed to receive input: {0}")]
    Recv(#[from] RecvInputError),
    /// An error writing an [`Output`].
    ///
    /// [`Output`]: io/struct.Output.html
    #[error("failed to write `{output}`: {error}")]
    Write {
        /// The error.
        #[source]
        error: WriteOutputError,
        /// The [`Output`] being written.
        ///
        /// [`Output`]: io/struct.Output.html
        output: String,
    },
    /// An error flushing the output.
    #[error("failed to flush output: {0}")]
    Flush(#[from] FlushCommandsError),
}

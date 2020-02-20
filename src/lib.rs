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

pub mod app;
pub mod io;

pub use io::Arguments;

use {
    app::Processor,
    io::{CreateInterfaceError, FlushError, Interface, IntoArgumentsError, PullError, PushError},
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
    /// If any error is encountered during creation, a [`CreateInterfaceError`] will be returned.
    ///
    /// [`CreateInterfaceError`]: io/struct.CreateInterfaceError.html
    #[inline]
    pub fn new(arguments: Arguments) -> Result<Self, CreateInterfaceError> {
        Ok(Self {
            processor: Processor::new(),
            io: Interface::new(arguments)?,
        })
    }

    /// Loops through program execution until a [`Failure`] occurs or the application quits.
    ///
    /// # Errors
    ///
    /// If any error from which the program is unable to recover is encountered, a [`Failure`] will be returned; in this case, the program will make all efforts to kill all processes and return the terminal to a clean state but these cannot be guaranteed.
    ///
    /// [`Failure`]: struct.Failure.html
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

        if let Some(input) = self.io.read()? {
            for output in self.processor.process(input) {
                keep_running &= self.io.push(output)?;
            }

            self.io.flush()?;
        }

        Ok(keep_running)
    }
}

/// An error from which `paper` was unable to recover.
#[derive(Debug, Error)]
pub enum Failure {
    /// An error while parsing the arguments.
    #[error("failed to read arguments: {0}")]
    Arguments(#[from] IntoArgumentsError),
    /// An error while creating `paper`.
    #[error("failed to create interface: {0}")]
    Create(#[from] CreateInterfaceError),
    /// An error while running `paper`.
    #[error("{0}")]
    Run(#[from] RunPaperError),
}

/// An error while `paper` is running.
#[derive(Debug, Error)]
pub enum RunPaperError {
    /// An error while pulling the input.
    #[error("failed to retrieve input: {0}")]
    Input(#[from] PullError),
    /// An error while pushing output.
    #[error("failed to apply output: {0}")]
    Output(#[from] PushError),
    /// An error while flushing output.
    #[error("failed to flush output: {0}")]
    Flush(#[from] FlushError),
}

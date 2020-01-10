//! A terminal-based text editor with goals to maximize simplicity and efficiency.
//!
//! ## Functional Overview
//! 1) All functionality shall be able to be performed via the keys reachable from the home row. Where it makes sense, functionality may additionally be performed via the mouse and other keys.
//! 2) All input shall be modal, i.e. keys shall implement different functionality depending on the current mode of the application.
//! 3) Paper shall support the Language Server Protocol.
//!
//! - Text manipulation shall involve a 3-step process of identifying the location should occur, marking that location, and then performing the desired edit.
//! - Paper shall reuse already implemented tools wherever possible.
//! - Paper shall follow all cargo-format conventions.
//! - Paper shall follow as many clippy lints as reasonably possible.
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
    clippy::fallible_impl_from, // Not always valid; issues should be detected by tests or other lints.
    clippy::implicit_return, // Goes against rust convention and requires return calls in places it is not helpful (e.g. closures).
    clippy::large_enum_variant, // Seems to be the same as variant_size_differences.
    clippy::suspicious_arithmetic_impl, // Not always valid; issues should be detected by tests or other lints.
    clippy::suspicious_op_assign_impl, // Not always valid; issues should be detected by tests or other lints.
    variant_size_differences, // Generally okay.
)]
// Temporary allows.
#![allow(
    clippy::missing_inline_in_public_items, // Flags methods in derived traits.
    clippy::multiple_crate_versions, // Requires redox_users update to avoid multiple versions of rand_core.
    // See <https://gitlab.redox-os.org/redox-os/users/merge_requests/30>
    clippy::unreachable, // Added by derive(Enum).
)]

mod app;
mod translate;
mod ui;

pub use ui::Arguments;

use {
    app::{LspError, Operation, Sheet},
    log::SetLoggerError,
    simplelog::{Config, LevelFilter, WriteLogger},
    std::{fs::File, io},
    thiserror::Error,
    translate::Interpreter,
    ui::Terminal,
};

/// Initializes the application logger.
///
/// The logger writes all logs to the file `paper.log`.
fn init_logger() -> Result<(), Failure> {
    let log_filename = "paper.log".to_string();

    WriteLogger::init(
        LevelFilter::Trace,
        Config::default(),
        File::create(&log_filename).map_err(|e| Failure::CreateLogFile(log_filename, e))?,
    )?;
    Ok(())
}

/// Manages the execution of the application.
///
/// The runtime loop of the application is as follows:
/// 1. Receive input.
/// 2. Translate input into operations.
/// 3. Execute operations and determine the appropriate changes.
/// 4. Output the changes.
#[derive(Debug, Default)]
pub struct Paper {
    /// Translates input into operations.
    interpreter: Interpreter,
    /// Interface between the application and the user.
    ui: Terminal,
    /// Processes application operations.
    sheet: Sheet,
}

impl Paper {
    /// Creates a new instance of the application.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Configures the application and then starts its runtime loop.
    #[inline]
    pub fn run(&mut self, arguments: Arguments) -> Result<(), Failure> {
        init_logger()?;
        self.ui.init(arguments)?;

        loop {
            if !self.step()? {
                break;
            }
        }

        Ok(())
    }

    /// Processes a single input, returning if the application should keep running.
    #[inline]
    fn step(&mut self) -> Result<bool, Failure> {
        let mut keep_running = true;

        if let Some(input) = self.ui.input()? {
            for operation in self.interpreter.translate(input) {
                if let Operation::Quit = operation {
                    keep_running = false;
                    break;
                }

                if let Some(change) = self.sheet.operate(operation)? {
                    self.ui.apply(change)?;
                }
            }
        }

        Ok(keep_running)
    }
}

/// An event that causes the application to stop running.
#[derive(Debug, Error)]
pub enum Failure {
    /// A failure in the user interface.
    #[error("user interface: {0}")]
    Ui(#[from] ui::Fault),
    /// A failure in the translator.
    #[error("translator: {0}")]
    Translator(#[from] translate::Fault),
    /// A failure in the language server protocol client.
    #[error("language server protocol: {0}")]
    Lsp(#[from] LspError),
    /// A failure to create the log file.
    #[error("failed to create log file `{0}`: {1}")]
    CreateLogFile(String, #[source] io::Error),
    /// A failure to initialize the logger.
    #[error("failed to initialize logger: {0}")]
    InitLogger(#[from] SetLoggerError),
}

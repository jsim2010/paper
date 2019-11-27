//! A terminal-based editor with goals to maximize simplicity and efficiency.
//!
//! This project is very much in an alpha state.
//!
//! Its features include:
//! - Modal editing (keys implement different functionality depending on the current mode).
//! - Adding support for Language Server Protocol.
//! - Extensive but relatively simple filter grammar that allows user to select any text.
//!
//! Future items on the Roadmap:
//! - Utilize external tools for filters.
//! - Add more filter grammar.
//! - Implement suggestions for commands to improve user experience.
//!
//! ## Development
//!
//! Clone the repository and enter the directory:
//!
//! ```sh
//! git clone https://github.com/jsim2010/paper.git
//! cd paper
//! ```
//!
//! If `cargo-make` is not already installed on your system, install it:
//!
//! ```sh
//! cargo install --force cargo-make
//! ```
//!
//! Install all dependencies needed for development:
//!
//! ```sh
//! cargo make dev
//! ```
//!
//! Now you can run the following commands:
//! - Evaluate all checks, lints and tests: `cargo make eval`
//! - Fix stale README and formatting: `cargo make fix`

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
    clippy::suspicious_arithmetic_impl, // Not always valid; issues should be detected by tests or other lints.
    clippy::suspicious_op_assign_impl, // Not always valid; issues should be detected by tests or other lints.
    variant_size_differences, // Generally okay.
)]
// Temporary allows.
#![allow(
    clippy::missing_const_for_fn, // Flags methods in derived traits.
    clippy::missing_inline_in_public_items, // Flags methods in derived traits.
    clippy::multiple_crate_versions, // Requires redox_users update to avoid multiple versions of rand_core.
    // See <https://gitlab.redox-os.org/redox-os/users/merge_requests/30>
)]

mod app;
mod num;
mod translate;
mod ui;

use app::{Mode, Operation, Outcome, Sheet};
use clap::ArgMatches;
use displaydoc::Display as DisplayDoc;
use log::SetLoggerError;
use simplelog::{Config, LevelFilter, WriteLogger};
use std::{collections::HashMap, fs::File, io};
use translate::{Interpreter, ViewInterpreter};
use ui::{Change, Input, Terminal};

/// An event that causes the application to stop running.
#[derive(Debug, DisplayDoc)]
pub enum Failure {
    /// user interface error: `{0}`
    Ui(ui::Error),
    /// file error: `{0}`
    File(io::Error),
    /// unknown mode: `{0}`
    UnknownMode(Mode),
    /// logger error: `{0}`
    Logger(SetLoggerError),
    /// user quit application
    Quit,
}

impl From<ui::Error> for Failure {
    fn from(value: ui::Error) -> Self {
        Self::Ui(value)
    }
}

impl From<io::Error> for Failure {
    fn from(value: io::Error) -> Self {
        Self::File(value)
    }
}

impl From<SetLoggerError> for Failure {
    fn from(value: SetLoggerError) -> Self {
        Self::Logger(value)
    }
}

/// Maps [`Mode`]s to their respective [`Interpreter`].
#[derive(Debug)]
struct InterpreterMap {
    /// Map of [`Interpreter`]s.
    map: HashMap<Mode, &'static dyn Interpreter>,
}

impl InterpreterMap {
    /// Returns the [`Interpreter`] associated with `mode`.
    ///
    /// Returns [`Err`]([`Failure`]) if `mode` is not in `self.map`.
    fn get(&self, mode: Mode) -> Result<&&dyn Interpreter, Failure> {
        self.map.get(&mode).ok_or(Failure::UnknownMode(mode))
    }
}

impl Default for InterpreterMap {
    fn default() -> Self {
        /// The [`Interpreter`] for [`Mode::View`].
        static VIEW_INTERPRETER: ViewInterpreter = ViewInterpreter::new();
        // Must use local variable to annotate that view_interpreter has type &dyn
        // Interpreter. The compiler infers all subsequent pointers.
        let view_interpreter: &dyn Interpreter = &VIEW_INTERPRETER;

        Self {
            map: [
                (Mode::View, view_interpreter),
                //(Mode::Command, &CommandInterpreter::new()),
                //(Mode::Edit, &EditInterpreter::new()),
            ]
            .iter()
            .cloned()
            .collect(),
        }
    }
}

/// Describes the paper application.
#[derive(Debug, Default)]
pub struct Paper {
    /// Current [`Mode`] of the application.
    mode: Mode,
    /// [`Interpreter`]s supported by the application.
    interpreters: InterpreterMap,
    /// Interface between the application and the user.
    ui: Terminal,
    /// The [`Sheet`] of the application.
    sheet: Sheet,
}

impl Paper {
    /// Runs the application.
    #[inline]
    pub fn run(&mut self, args: &ArgMatches<'_>) -> Result<(), Failure> {
        let mut result;

        self.initialize_logger()?;
        self.ui.init(args)?;

        loop {
            result = self.step();

            if let Err(error) = &result {
                // Quit indicates user requested to end application, which is not not a
                // true Failure.
                if let Failure::Quit = error {
                    result = Ok(());
                }

                break;
            }
        }

        self.ui.stop()?;
        result
    }

    /// Processes a single input from the user.
    #[inline]
    fn step(&mut self) -> Result<(), Failure> {
        if let Some(input) = self.ui.input() {
            for operation in self.translate(input)? {
                if let Some(outcome) = self.sheet.operate(operation)? {
                    match outcome {
                        Outcome::SwitchMode(mode) => {
                            self.mode = mode;
                        }
                        Outcome::EditText(edits) => {
                            self.ui.apply(Change::Text(edits))?;
                        }
                    }
                }
            }
        }

        Ok(())
    }

    /// Initializes the logger for the application.
    fn initialize_logger(&self) -> Result<(), Failure> {
        WriteLogger::init(
            LevelFilter::Trace,
            Config::default(),
            File::create("paper.log")?,
        )?;
        Ok(())
    }

    /// Converts `input` to the appropriate [`Vec`] of [`Operation`]s.
    fn translate(&self, input: Input) -> Result<Vec<Operation>, Failure> {
        Ok(self.interpreters.get(self.mode)?.decode(input, &self.sheet))
    }
}

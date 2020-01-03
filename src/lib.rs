//! A terminal-based text viewer/editor with goals to maximize simplicity and efficiency.
//!
//! ## Organization
//! 1) Name
//! GIVEN:
//!     - The goal of the application is to provide a way to view and edit text that is as simple as using a pencil and paper.
//! THEREFORE
//!     - This application shall be known as `paper`.
//!
//! ## Functional Overview
//! 1) Primary Keys
//! GIVEN:
//!     - It is desirable to perform viewing and editing actions as fast as possible.
//!     - The best speed performance is generally achieved when the user's hands avoid unnecessary movements.
//!     - The keyboard is the best tool for a user to send a variety of easily understood inputs.
//!     - The **primary keys** are the keys that are generally accesible from the home row on a majority of keyboards, defined as by the 4 following corners: `\``, `Backspace`, `Left Ctrl`, `Right Ctrl`, and with 1 addtional exception of the `Esc` key.
//! THEREFORE:
//!     - All functionality shall able to be performed via the primary keys.
//!     - Functionality for the mouse and other keys may be added as long as the functionality is also accessible via the primary keys.
//!
//! 2) Modal Operation
//! GIVEN:
//!     - All functionality shall be implemented by the keyboard.
//!     - Most keys already have an accepted function for inputing text.
//!     - Adding inputs for more complex operations can be done via either modifiers or modes.
//! THEREFORE:
//!     - All input shall be modal, i.e. keys shall implement different operations depending on the current mode of the application.
//!
//! 3) Filtering
//! THEREFORE:
//!     - Text manipulation shall involve a 3-step process of identifying the location an edit should occur, marking that the location, and then performing the desired edit.
//!
//! 4) Language Server Support
//! GIVEN:
//!     - The `Language Server Protocol` standardizes the interface between a tool and a language.
//! THEREFORE:
//!     - The application shall support the Language Server Protocol for all reasonably acceptable languages.
//!
//! ## Implementation
//! 1) Reuse existing tools
//! THEREFORE:
//!     - Paper shall reuse already implemented tools wherever possible.
//!
//! 2) Code Quality
//! THEREFORE:
//!     - Paper shall follow all cargo-format conventions. Paper shall follow as many clippy lints as reasonably possible.
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
    clippy::missing_inline_in_public_items, // Flags methods in derived traits.
    clippy::multiple_crate_versions, // Requires redox_users update to avoid multiple versions of rand_core.
    // See <https://gitlab.redox-os.org/redox-os/users/merge_requests/30>
)]

mod app;
mod translate;
mod ui;

pub use ui::Settings;

use {
    app::{LspError, Mode, Operation, Sheet},
    log::SetLoggerError,
    simplelog::{Config, LevelFilter, WriteLogger},
    std::{collections::HashMap, fs::File},
    thiserror::Error,
    translate::{ConfirmInterpreter, Interpreter, ViewInterpreter},
    ui::{Change, Input, Terminal},
};

/// Initializes the application logger.
fn init_logger() -> Result<(), LogError> {
    let log_filename = "paper.log".to_string();

    WriteLogger::init(
        LevelFilter::Trace,
        Config::default(),
        File::create(&log_filename).map_err(|_| LogError::CreateLogFile(log_filename))?,
    )?;
    Ok(())
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
    /// Creates a new instance of the application.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Configures the application and then starts its runtime loop.
    #[inline]
    pub fn run(&mut self, settings: Settings) -> Result<(), Failure> {
        init_logger()?;
        self.ui.init(settings)?;

        loop {
            if !self.step()? {
                break;
            }
        }

        Ok(())
    }

    /// Processes a single event from the user, returning if the application should keep running.
    #[inline]
    fn step(&mut self) -> Result<bool, Failure> {
        let mut keep_running = true;

        if let Some(input) = self.ui.input()? {
            for operation in self.translate(input)? {
                match operation {
                    Operation::Quit => {
                        keep_running = false;
                        break;
                    }
                    Operation::Reset => self.mode = Mode::View,
                    Operation::Confirm(..) => self.mode = Mode::Confirm,
                    Operation::UpdateConfig(_) => {}
                }

                if let Some(change) = self.sheet.operate(operation)? {
                    self.ui.apply(change)?;
                }
            }
        }

        Ok(keep_running)
    }

    /// Converts `input` to the appropriate [`Vec`] of [`Operation`]s.
    fn translate(&self, input: Input) -> Result<Vec<Operation>, Failure> {
        Ok(self.interpreters.get(self.mode)?.decode(input))
    }
}

/// An event that causes the application to stop running.
#[derive(Debug, Error)]
pub enum Failure {
    /// A failure in the user interface.
    #[error("user interface: {0}")]
    Ui(#[from] ui::Error),
    /// A mode is not stored in [`InterpreterMap`].
    #[error("mode `{0}` is unknown")]
    UnknownMode(Mode),
    /// A failure in the language server protocol client.
    #[error("language server protocol: {0}")]
    Lsp(#[from] LspError),
    /// A failure in the logger.
    #[error("logger: {0}")]
    Log(#[from] LogError),
    //#[error("logger: `{0}`")]
    //Logger(#[from]SetLoggerError),
}

/// A failure in the logger.
#[derive(Debug, Error)]
pub enum LogError {
    /// A failure to create the log file.
    #[error("failed to create log file `{0}`")]
    CreateLogFile(String),
    /// A failure to initialize the logger.
    #[error("failed to initialize logger: {0}")]
    Init(#[from] SetLoggerError),
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
        /// The [`Interpreter`] for [`Mode::Confirm`].
        static CONFIRM_INTERPRETER: ConfirmInterpreter = ConfirmInterpreter::new();

        let mut map: HashMap<Mode, &'static dyn Interpreter> = HashMap::new();

        let _ = map.insert(Mode::View, &VIEW_INTERPRETER);
        let _ = map.insert(Mode::Confirm, &CONFIRM_INTERPRETER);
        Self { map }
    }
}

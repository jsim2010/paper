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
    clippy::cargo,
    clippy::nursery,
    clippy::pedantic,
    clippy::restriction
)]
// Rustc lints that are not warned:
// box_pointers: Boxes are generally okay.
// single_use_lifetimes: Flags methods in derived traits.
// variant_size_differences: This is generally not a bad thing.
#![allow(
    clippy::fallible_impl_from, // Not always valid; issues should be detected by tests or other lints.
    clippy::implicit_return, // Goes against rust convention and requires return calls in places it is not helpful (e.g. closures).
    clippy::missing_const_for_fn, // Flags methods in derived traits.
    clippy::missing_inline_in_public_items, // Flags methods in derived traits.
    clippy::suspicious_arithmetic_impl, // Not always valid; issues should be detected by tests or other lints.
    clippy::suspicious_op_assign_impl, // Not always valid; issues should be detected by tests or other lints.
)]
//#![allow(clippy::multiple_crate_versions)]

mod app;
mod file;
mod lsp;
mod num;
mod translate;
mod ui;

use app::{Direction, Operation, Sheet};
use core::borrow::Borrow;
use displaydoc::Display as DisplayDoc;
use file::Explorer;
use lsp_types::Position;
use parse_display::Display as ParseDisplay;
use std::{collections::HashMap, path::PathBuf};
use translate::{
    ActionInterpreter, CommandInterpreter, DisplayInterpreter, EditInterpreter, FilterInterpreter,
    Interpreter,
};
use ui::Terminal;

/// The [`Interpreter`] when mode is [`Mode::Display`].
static DISPLAY_INTERPRETER: DisplayInterpreter = DisplayInterpreter::new();
/// The [`Interpreter`] when mode is [`Mode::Command`].
static COMMAND_INTERPRETER: CommandInterpreter = CommandInterpreter::new();
/// The [`Interpreter`] when mode is [`Mode::Filter`].
static FILTER_INTERPRETER: FilterInterpreter = FilterInterpreter::new();
/// The [`Interpreter`] when mode is [`Mode::Action`].
static ACTION_INTERPRETER: ActionInterpreter = ActionInterpreter::new();
/// The [`Interpreter`] when mode is [`Mode::Edit`].
static EDIT_INTERPRETER: EditInterpreter = EditInterpreter::new();

/// Signifies the mode of the application.
#[derive(Copy, Clone, Eq, ParseDisplay, PartialEq, Hash, Debug)]
#[display(style = "CamelCase")]
pub enum Mode {
    /// Displays the current view.
    Display,
    /// Displays the current command.
    Command,
    /// Displays the current filter expression and highlights the characters that match the filter.
    Filter,
    /// Displays the highlighting that has been selected.
    Action,
    /// Displays the current view along with the current edits.
    Edit,
}

impl Default for Mode {
    #[inline]
    fn default() -> Self {
        Self::Display
    }
}

/// Describes the paper application.
#[derive(Debug)]
pub struct Paper {
    /// The current [`Mode`] of the application.
    mode: Mode,
    /// Maps [`Mode`]s to their respective [`Interpreter`].
    interpreters: HashMap<Mode, &'static dyn Interpreter>,
    /// User interface of the application.
    ui: Terminal,
    /// Interface between the application and the file system.
    explorer: Explorer,
    /// The [`Sheet`] of the application.
    sheet: Sheet,
}

impl Paper {
    /// Creates a new paper application.
    pub fn new() -> Result<Self, Failure> {
        // Must first create ui to pass info to Sheet.
        let ui = Terminal::new();

        // Must use local variable to annotate that DISPLAY_INTERPRETER has type &dyn
        // Interpreter. The compiler infers all subsequent pointers.
        let display_interpreter: &dyn Interpreter = &DISPLAY_INTERPRETER;

        Ok(Self {
            sheet: Sheet::new(ui.grid_height()?)?,
            explorer: Explorer::new()?,
            ui,
            mode: Mode::default(),
            interpreters: [
                (Mode::Display, display_interpreter),
                (Mode::Command, &COMMAND_INTERPRETER),
                (Mode::Filter, &FILTER_INTERPRETER),
                (Mode::Action, &ACTION_INTERPRETER),
                (Mode::Edit, &EDIT_INTERPRETER),
            ]
            .iter()
            .cloned()
            .collect(),
        })
    }

    /// Runs the application.
    #[inline]
    pub fn run(&mut self) -> Result<(), Failure> {
        let mut result;

        self.ui.init()?;
        self.explorer.init()?;

        loop {
            result = self.step();

            if result.is_err() {
                break;
            }
        }

        self.ui.close()?;
        result
    }

    /// Processes 1 input from the user.
    #[inline]
    fn step(&mut self) -> Result<(), Failure> {
        let mut alerts = Vec::new();

        if let Some(input) = self.ui.input() {
            for operation in self.interpreter()?.decode(input, &self.sheet) {
                if let Some(alert) = self.operate(operation)? {
                    alerts.push(alert);
                }
            }
        }

        self.sheet.process_notifications();

        for change in self.sheet.changes() {
            self.ui.apply(change)?;
        }

        Ok(())
    }

    /// Returns the [`Interpreter`] for the current [`Mode`].
    fn interpreter(&self) -> Result<&&dyn Interpreter, Failure> {
        self.interpreters.get(&self.mode).ok_or(Failure::UnknownMode(self.mode))
    }

    /// Executes an [`Operation`].
    #[inline]
    pub fn operate(&mut self, operation: Operation) -> Result<Option<Alert>, Failure> {
        match operation {
            Operation::Scroll(direction) => {
                match direction {
                    Direction::Up => self.sheet.scroll_up(),
                    Direction::Down => self.sheet.scroll_down(),
                }

                Ok(None)
            }
            Operation::EnterMode(mode) => {
                self.mode = mode;

                match mode {
                    Mode::Display => {
                        self.sheet.wipe();
                    }
                    Mode::Command => {
                        self.sheet.reset_control_panel(None);
                    }
                    Mode::Action | Mode::Edit | Mode::Filter => {}
                }

                Ok(None)
            }
            Operation::ResetControlPanel(c) => {
                self.sheet.reset_control_panel(c);
                Ok(None)
            }
            Operation::DisplayFile(path) => {
                Ok(self.sheet.change(path.clone(), self.explorer.read(path)?).err())
            }
            Operation::Save => {
                Ok(self.explorer.write(self.sheet.path(), self.sheet.file()).map_err(Alert::from).err())
            }
            Operation::AddToControlPanel(c) => {
                self.sheet.input_to_control_panel(c);
                Ok(None)
            }
            Operation::Add(c) => {
                // TODO: Need to determine position.
                Ok(self.sheet.add(
                    &mut Position {
                        line: 0,
                        character: 0,
                    },
                    c,
                ).err())
            }
            Operation::Quit => {
                Err(Failure::Quit)
            }
            Operation::UserError => {
                Ok(None)
            }
        }
    }
}

/// An event that causes the application to stop running.
#[derive(Debug, DisplayDoc)]
pub enum Failure {
    /// user interface error: `{0}`
    Ui(ui::Error),
    /// file error: `{0}`
    File(file::Error),
    /// unknown mode: `{0}`
    UnknownMode(Mode),
    /// user quit application
    Quit
}

impl From<ui::Error> for Failure {
    fn from(value: ui::Error) -> Self {
        Self::Ui(value)
    }
}

impl From<file::Error> for Failure {
    fn from(value: file::Error) -> Self {
        Self::File(value)
    }
}

/// Signifies an alert that the application needs to process.
#[derive(Debug, DisplayDoc)]
pub enum Alert {
    /// user interface error: `{0}`
    Ui(ui::Error),
    /// file explorer error: `{0}`
    Explorer(file::Error),
    /// language server protocol: `{0}`
    Lsp(lsp::Error),
    /// mode `{0}` is unknown
    UnknownMode(Mode),
    /// unable to convert `{0:?}` to `Url`
    InvalidPath(PathBuf),
    /// invalid input from user
    User,
    /// user quit application
    Quit,
}

impl From<ui::Error> for Alert {
    fn from(value: ui::Error) -> Self {
        Self::Ui(value)
    }
}

impl From<file::Error> for Alert {
    fn from(value: file::Error) -> Self {
        Self::Explorer(value)
    }
}

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
    variant_size_differences,
    clippy::cargo,
    clippy::nursery,
    clippy::pedantic,
    clippy::restriction
)]
// Rustc lints that are not warned:
// box_pointers: Boxes are generally okay.
// single_use_lifetimes: Flags methods in derived traits.
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

use app::{Direction, Operation, Output, Sheet};
use displaydoc::Display as DisplayDoc;
use lsp_msg::Position;
use parse_display::Display as ParseDisplay;
use std::{borrow::Borrow, collections::HashMap};
use translate::{Interpreter, DisplayInterpreter, CommandInterpreter, FilterInterpreter, ActionInterpreter, EditInterpreter};
use ui::Terminal;

/// Signifies a [`Result`] with [`Alert`] as its Error.
type Outcome<T> = Result<T, Alert>;

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
    /// The [`Sheet`] of the application.
    sheet: Sheet,
}

impl Paper {
    /// Creates a new paper application.
    pub fn new() -> Outcome<Self> {
        // Must first create ui to pass info to Sheet.
        let ui = Terminal::new();

        // Must use local variable to annotate that DISPLAY_INTERPRETER has type &dyn
        // Interpreter. The compiler infers all subsequent pointers.
        let display_interpreter: &dyn Interpreter = &DISPLAY_INTERPRETER;

        Ok(Self {
            sheet: Sheet::new(ui.grid_height()?)?,
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
    pub fn run(&mut self) -> Outcome<()> {
        self.ui.init()?;
        self.sheet.init()?;

        loop {
            if let Err(Alert::Quit) = self.step() {
                break;
            }
        }

        self.ui.close()?;
        Ok(())
    }

    /// Processes 1 input from the user.
    #[inline]
    fn step(&mut self) -> Output<()> {
        if let Some(input) = self.ui.input() {
            for operation in self
                .interpreters
                .get(&self.mode)
                .ok_or(Alert::UnknownMode(self.mode))?
                .decode(input, &self.sheet)?
            {
                self.operate(operation)?;
            }
        }

        self.sheet.process_notifications();

        for change in self.sheet.changes() {
            self.ui.apply(change)?;
        }

        Ok(())
    }

    /// Executes an [`Operation`].
    #[inline]
    pub fn operate(&mut self, operation: Operation) -> Output<()> {
        match operation {
            Operation::Scroll(direction) => match direction {
                Direction::Up => self.sheet.scroll_up(),
                Direction::Down => self.sheet.scroll_down(),
            },
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
            }
            Operation::ResetControlPanel(c) => self.sheet.reset_control_panel(c),
            Operation::DisplayFile(path) => {
                self.sheet.change(path.borrow())?;
            }
            Operation::Save => {
                self.sheet.save()?;
            }
            Operation::AddToControlPanel(c) => {
                self.sheet.input_to_control_panel(c);
            }
            Operation::Add(c) => {
                // TODO: Need to determine position.
                self.sheet.add(
                    &mut Position {
                        line: 0,
                        character: 0,
                    },
                    c,
                )?;
            }
        }

        Ok(())
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
    /// `{0}`
    Custom(&'static str),
    /// mode `{0}` is unknown
    UnknownMode(Mode),
    /// invalid input from user
    User,
    /// quit the application
    ///
    /// This does not necessarily mean that an error occurred, ex: the user commands the application to quit.
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

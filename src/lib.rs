//! A terminal-based editor with goals to maximize simplicity and efficiency.
//!
//! This project is very much in an alpha state.
//!
//! Its features include:
//! - Modal editing (keys implement different functionality depending on the current mode).
//! - Extensive but relatively simple filter grammar that allows user to select any text.
//!
//! Future items on the Roadmap:
//! - Add more filter grammar.
//! - Implement suggestions for commands to improve user experience.
//! - Support Language Server Protocol.
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

#![doc(html_root_url = "https://docs.rs/paper/0.3.0")]
#![warn(
    rust_2018_idioms,
    future_incompatible,
    unused,
    macro_use_extern_crate,
    missing_copy_implementations,
    missing_debug_implementations,
    missing_docs,
    trivial_casts,
    trivial_numeric_casts,
    unreachable_pub,
    unused_import_braces,
    unused_lifetimes,
    unused_qualifications,
    unused_results,
    clippy::nursery,
    clippy::pedantic,
    clippy::restriction
)]
#![allow(
    clippy::suspicious_op_assign_impl,
    clippy::suspicious_arithmetic_impl,
    clippy::fallible_impl_from
)] // These lints are not always correct; issues should be detected by tests or other lints.
#![allow(clippy::implicit_return)]
// This goes against rust convention and would require return calls in places it is not helpful (i.e. closures).
#![allow(clippy::missing_inline_in_public_items)] // Mistakenly marks derived traits.

// Lint checks currently not defined: missing_doc_code_examples, variant_size_differences, single_use_lifetimes: issue rust-lang/rust#55057, box_pointers

macro_rules! add_trait_child {
    ($trait:ident, $child:ident, $name:ident) => {
        mod $child;
        pub(crate) use $child::$trait as $name;
    };
}

#[macro_use]
mod ptr;

// file uses a macro from ptr and thus must be loaded after ptr.
pub mod file;
pub mod lsp;
pub mod mode;
pub mod num;
pub mod ui;

pub use file::local::Explorer as LocalExplorer;
pub use file::Explorer;

use mode::{Flag, Operation, Pane, Processor};
use pancurses::Input;
use ptr::Mrc;
use std::collections::HashMap;
use ui::UserInterface;

/// Defines a [`Result`] with [`Flag`] as its Error.
pub type Output<T> = Result<T, Flag>;

/// The paper application.
#[derive(Debug)]
pub struct Paper {
    /// User interface of the application.
    ui: Mrc<dyn UserInterface>,
    /// The [`Pane`] of the application.
    pane: Mrc<Pane>,
    /// The [`Explorer`] of the application.
    explorer: Mrc<dyn Explorer>,
    /// The current [`mode::Name`] of the application.
    mode: mode::Name,
    /// Maps modes to their respective [`Processor`].
    processors: HashMap<mode::Name, Mrc<dyn Processor>>,
}

impl Paper {
    /// Creates a new paper application.
    #[inline]
    pub fn new(ui: Mrc<dyn UserInterface>, explorer: Mrc<dyn Explorer>) -> Self {
        explorer.borrow_mut().start().expect("Starting explorer");
        let pane = mrc!(Pane::new(
            ui.borrow_mut()
                .grid_height()
                .expect("Accessing height of user interface")
        ));
        let display_mode_handler: Mrc<dyn Processor> =
            mrc!(mode::DisplayProcessor::new(&pane, &explorer));
        let command_mode_handler: Mrc<dyn Processor> = mrc!(mode::CommandProcessor::new(&pane));
        let filter_mode_handler: Mrc<dyn Processor> = mrc!(mode::FilterProcessor::new(&pane));
        let action_mode_handler: Mrc<dyn Processor> = mrc!(mode::ActionProcessor::new());
        let edit_mode_handler: Mrc<dyn Processor> = mrc!(mode::EditProcessor::new(&pane));

        Self {
            ui,
            pane,
            explorer,
            mode: mode::Name::default(),
            processors: [
                (mode::Name::Display, display_mode_handler),
                (mode::Name::Command, command_mode_handler),
                (mode::Name::Filter, filter_mode_handler),
                (mode::Name::Action, action_mode_handler),
                (mode::Name::Edit, edit_mode_handler),
            ]
            .iter()
            .cloned()
            .collect(),
        }
    }

    /// Runs the application.
    #[inline]
    pub fn run(&mut self) -> Output<()> {
        self.ui.borrow_mut().init()?;

        loop {
            if let Err(Flag::Quit) = self.step() {
                break;
            }
        }

        self.ui.borrow_mut().close()?;
        Ok(())
    }

    /// Returns the input from the `UserInterface`.
    fn get_input(&mut self) -> Option<Input> {
        self.ui.borrow_mut().receive_input()
    }

    /// Processes 1 input from the user.
    #[inline]
    pub fn step(&mut self) -> Output<()> {
        let operation = if let Some(Input::Character(input)) = self.get_input() {
            self.current_processor_mut().borrow_mut().decode(input)?
        } else {
            Operation::maintain()
        };

        if let Some(notification) = self.explorer.borrow_mut().receive_notification() {
            self.pane.borrow_mut().add_notification(notification);
        }

        self.operate(&operation)
    }

    /// Executes an [`Operation`].
    #[inline]
    pub fn operate(&mut self, operation: &Operation) -> Output<()> {
        if let Some(new_mode) = operation.mode() {
            self.mode = *new_mode;
            self.current_processor_mut()
                .borrow_mut()
                .enter(operation.initiation())?;
        }

        let edits = self.pane.borrow_mut().edits();

        for edit in edits {
            self.ui.borrow_mut().apply(edit)?;
        }

        Ok(())
    }

    /// Return a mutable reference to the processor of the current mode.
    fn current_processor_mut(&mut self) -> &mut Mrc<dyn Processor> {
        self.processors.get_mut(&self.mode).unwrap()
    }
}

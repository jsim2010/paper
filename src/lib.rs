//! A terminal-based editor with goals to maximize simplicity and efficiency.
//!
//! This project is very much in an alpha state.
//!
//! [![Waffle.io - Columns and their card
//! count](https://badge.waffle.io/jsim2010/paper.svg?columns=all)](https://waffle.io/jsim2010/paper)
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
    box_pointers,
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
#![allow(clippy::suspicious_op_assign_impl, clippy::suspicious_arithmetic_impl)] // These lints are not always correct; issues should be detected by tests.
#![allow(clippy::implicit_return)]
// This goes against rust convention and would require return calls in places it is not helpful (i.e. closures).
#![allow(clippy::missing_inline_in_public_items)]
// Lint checks currently not defined: missing_doc_code_examples, variant_size_differences
// single_use_lifetimes: issue rust-lang/rust#55057

pub mod num;
pub mod ui;
pub mod storage;

mod mode;

pub use storage::Explorer;

use mode::{Pane, Flag, Operation, Output, Processor};
use pancurses::Input;
use std::borrow::Borrow;
use std::cell::RefCell;
use std::cmp::Ordering;
use std::collections::HashMap;
use std::fmt::{self, Debug, Display, Formatter};
use std::ops::{Add, AddAssign};
use std::rc::Rc;
use try_from::{TryFrom, TryFromIntError};
use ui::UserInterface;

/// A Mutable Reference Counter.
///
/// This is just a ([`Rc`]) of a [`RefCell`].
type Mrc<T> = Rc<RefCell<T>>;

macro_rules! mrc {
    ($item:expr) => {
        Rc::new(RefCell::new($item))
    }
}

/// The paper application.
#[derive(Debug)]
pub struct Paper {
    /// User interface of the application.
    ui: Rc<dyn UserInterface>,
    pane: Mrc<Pane>,
    explorer: Mrc<dyn Explorer>,
    /// The current [`mode::Name`] of the application.
    mode: mode::Name,
    mode_handlers: HashMap<mode::Name, Mrc<dyn Processor>>,
}

impl Paper {
    /// Creates a new paper application.
    #[inline]
    pub fn new(ui: Rc<dyn UserInterface>, explorer: Mrc<dyn Explorer>) -> Self {
        explorer.borrow_mut().start().unwrap();
        let pane = mrc!(Pane::new(ui.grid_height().unwrap()));
        let display_mode_handler: Mrc<dyn Processor> = mrc!(mode::DisplayProcessor::new(&pane, &explorer));
        let command_mode_handler: Mrc<dyn Processor> = mrc!(mode::CommandProcessor::new());
        let filter_mode_handler: Mrc<dyn Processor> = mrc!(mode::FilterProcessor::new(&pane));
        let action_mode_handler: Mrc<dyn Processor> = mrc!(mode::ActionProcessor::new(&pane));
        let edit_mode_handler: Mrc<dyn Processor> = mrc!(mode::EditProcessor::new(&pane));

        Self {
            ui,
            pane,
            explorer,
            mode: mode::Name::default(),
            mode_handlers: [
                (mode::Name::Display, display_mode_handler),
                (mode::Name::Command, command_mode_handler),
                (mode::Name::Filter, filter_mode_handler),
                (mode::Name::Action, action_mode_handler),
                (mode::Name::Edit, edit_mode_handler),
            ].iter().cloned().collect(),
        }
    }

    /// Runs the application.
    #[inline]
    pub fn run(&mut self) -> Output<()> {
        self.ui.init()?;

        loop {
            if let Err(Flag::Quit) = self.step() {
                break;
            }
        }

        self.ui.close()?;
        Ok(())
    }

    /// Processes 1 input from the user.
    pub fn step(&mut self) -> Output<()> {
        if let Some(Input::Character(input)) = self.ui.receive_input() {
            let operation = self.current_processor_mut().borrow_mut().decode(input)?;

            let edits = match operation {
                Operation::EnterMode(new_mode, initiation) => {
                    self.mode = new_mode;
                    self.current_processor_mut().borrow_mut().enter(initiation)?
                }
                Operation::EditUi(ui_edits) => ui_edits,
                _ => vec![],
            };

            for edit in edits {
                self.ui.apply(edit)?;
            }
        }

        Ok(())
    }

    fn current_processor_mut(&mut self) -> &mut Mrc<dyn Processor> {
        self.mode_handlers.get_mut(&self.mode).unwrap()
    }
}

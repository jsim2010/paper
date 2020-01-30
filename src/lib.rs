//! A terminal-based text editor with goals to maximize simplicity and efficiency.
//!
//! # Functional Overview
//! 1) All functionality shall be able to be performed via the keys reachable from the home row. Where it makes sense, functionality may additionally be performed via the mouse and other keys.
//! 2) All input shall be modal, i.e. keys shall implement different functionality depending on the current mode of the application.
//! 3) Paper shall utilize already implemented tools and commands wherever possible; specifically paper shall support the [Language Server Protocol].
//! 4) Paper shall follow [rustfmt] and as many [clippy] lints as reasonably possible.
//!
//! ## Upcoming
//! - Text manipulation shall involve a 3-step process of identifying the location should occur, marking that location, and then performing the desired edit.
//!
//! [Language Server Protocol]: https://microsoft.github.io/language-server-protocol/
//! [rustfmt]: https://github.com/rust-lang/rustfmt
//! [clippy]: https://rust-lang.github.io/rust-clippy/current/index.html
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
    clippy::missing_inline_in_public_items, // Flags methods in derived traits.
    clippy::multiple_crate_versions, // Requires redox_users update to avoid multiple versions of rand_core.
    // See <https://gitlab.redox-os.org/redox-os/users/merge_requests/30>
    clippy::unreachable, // unreachable added by derive(Enum).
    clippy::use_debug, // Flags debug formatting in Debug trait.
)]

pub mod app;
mod translate;
pub mod ui;

pub use app::Arguments;

use {
    app::{Operation, Sheet},
    thiserror::Error,
    translate::Interpreter,
    ui::Terminal,
};

/// An instance of the `paper` program.
///
/// # Examples
///
/// ```no_run
/// use paper::{Arguments, Failure, Paper};
/// # fn main() -> Result<(), Failure> {
///
/// let result: Result<(), Failure> = Paper::new(Arguments::default())?.run();
/// # Ok(())
/// # }
/// ```
#[derive(Debug)]
pub struct Paper {
    /// Translates input into operations.
    interpreter: Interpreter,
    /// Manages the user interface.
    ui: Terminal,
    /// Processes application operations.
    sheet: Sheet,
}

impl Paper {
    /// Creates a new instance of `paper`.
    ///
    /// # Errors
    ///
    /// If any error is encountered during creation, a [`Failure`] will be returned.
    ///
    /// [`Failure`]: struct.Failure.html
    #[inline]
    pub fn new(arguments: Arguments) -> Result<Self, Failure> {
        Ok(Self {
            sheet: Sheet::new(&arguments),
            interpreter: Interpreter::default(),
            ui: Terminal::new(arguments)?,
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
    pub fn run(&mut self) -> Result<(), Failure> {
        loop {
            if !self.step()? {
                break;
            }
        }

        Ok(())
    }

    /// Executes a single run of the runtime loop and returns if the program should keep running.
    #[inline]
    fn step(&mut self) -> Result<bool, Failure> {
        let mut keep_running = true;

        if let Some(input) = self.ui.input()? {
            for operation in self.interpreter.translate(input) {
                if let Operation::Quit = operation {
                    keep_running = false;
                }

                if let Some(update) = self.sheet.operate(operation)? {
                    self.ui.apply(update)?;
                }
            }
        }

        Ok(keep_running)
    }
}

/// An error from which `paper` was unable to recover.
#[derive(Debug, Error)]
pub enum Failure {
    /// An error from [`ui`].
    ///
    /// [`ui`]: ui/index.html
    #[error("{0}")]
    Ui(#[from] ui::Fault),
    /// An error from [`app`].
    ///
    /// [`app`]: app/index.html
    #[error("{0}")]
    App(#[from] app::Fault),
}

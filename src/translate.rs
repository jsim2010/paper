//! Implements the functionality of converting [`Input`] to [`Operation`]s.
use crate::{
    app::{Operation, Sheet},
    ui::{Argument, Input},
    Mode,
};
use std::fmt::Debug;

/// Defines the functionality to convert [`Input`] to [`Operation`]s.
pub(crate) trait Interpreter: Debug {
    /// Converts `input` to [`Operation`]s.
    fn decode(&self, input: Input, sheet: &Sheet) -> Vec<Operation>;
}

/// The [`Interpreter`] for [`Mode::View`].
#[derive(Clone, Debug)]
pub(crate) struct ViewInterpreter {}

impl ViewInterpreter {
    /// Creates a `ViewInterpreter`.
    pub(crate) const fn new() -> Self {
        Self {}
    }
}

impl Interpreter for ViewInterpreter {
    fn decode(&self, input: Input, _sheet: &Sheet) -> Vec<Operation> {
        match input {
            Input::Arg(Argument::File(file)) => vec![Operation::ViewFile(file)],
            // Temporary mapping to provide basic functionality prior to adding Mode::Command.
            Input::Backspace => vec![Operation::Quit],
            Input::Escape => vec![Operation::SwitchMode(Mode::View)],
            Input::Char(_) | Input::Enter => vec![],
        }
    }
}

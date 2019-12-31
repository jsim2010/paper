//! Implements the functionality of converting [`Input`] to [`Operation`]s.
use std::fmt::Debug;
use {
    crate::{
        app::{Operation, Sheet},
        ui::Input,
        Mode,
    },
    crossterm::event::{Event, KeyCode},
};

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
            Input::Config(config) => vec![Operation::UpdateConfig(config)],
            Input::User(event) => match event {
                Event::Key(key) => match key.code {
                    // Temporary mapping to provide basic functionality prior to adding Mode::Command.
                    KeyCode::Backspace => vec![Operation::Quit],
                    KeyCode::Esc => vec![Operation::SwitchMode(Mode::View)],
                    KeyCode::Enter
                    | KeyCode::Left
                    | KeyCode::Right
                    | KeyCode::Up
                    | KeyCode::Down
                    | KeyCode::Home
                    | KeyCode::End
                    | KeyCode::PageUp
                    | KeyCode::PageDown
                    | KeyCode::Tab
                    | KeyCode::BackTab
                    | KeyCode::Delete
                    | KeyCode::Insert
                    | KeyCode::F(..)
                    | KeyCode::Char(..)
                    | KeyCode::Null => vec![],
                },
                Event::Mouse(..) | Event::Resize(..) => vec![],
            },
        }
    }
}

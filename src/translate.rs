//! Implements the functionality of converting [`Input`] to [`Operation`]s.
use std::fmt::Debug;
use {
    crate::{
        app::{Operation, ConfirmAction},
        ui::Input,
    },
    crossterm::event::{Event, KeyCode},
};

/// Defines the functionality to convert [`Input`] to [`Operation`]s.
pub(crate) trait Interpreter: Debug {
    /// Converts `input` to [`Operation`]s.
    fn decode(&self, input: Input) -> Vec<Operation>;
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
    fn decode(&self, input: Input) -> Vec<Operation> {
        match input {
            Input::Config(config) => vec![Operation::UpdateConfig(config)],
            Input::User(event) => match event {
                Event::Key(key) => match key.code {
                    // Temporary mapping to provide basic functionality prior to adding Mode::Command.
                    KeyCode::Backspace => vec![Operation::Quit],
                    KeyCode::Esc => vec![Operation::Reset],
                    KeyCode::Char('q') => vec![Operation::Confirm(ConfirmAction::Quit)],
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

/// Tests decoding user input while in the View mode.
#[cfg(test)]
mod test_view {
    use super::*;
    use crossterm::event::{KeyEvent, KeyModifiers};

    /// The `q` key shall confirm the user wants to quit.
    #[test]
    fn quit() {
        let view = ViewInterpreter::new();

        assert_eq!(view.decode(Input::User(Event::Key(KeyEvent::new(KeyCode::Char('q'), KeyModifiers::empty())))), vec![Operation::Confirm(ConfirmAction::Quit)]);
    }
}

#[derive(Clone, Debug)]
pub(crate) struct ConfirmInterpreter {}

impl ConfirmInterpreter {
    pub(crate) const fn new() -> Self {
        Self {}
    }
}

impl Interpreter for ConfirmInterpreter {
    fn decode(&self, input: Input) -> Vec<Operation> {
        match input {
            Input::User(event) => match event {
                Event::Key(key) => match key.code {
                    KeyCode::Char('y') => vec![Operation::Quit],
                    _ => vec![Operation::Reset],
                }
                _ => vec![],
            }
            _ => vec![],
        }
    }
}

/// Tests decoding user input while in the View mode.
#[cfg(test)]
mod test_confirm {
    use super::*;
    use crossterm::event::{KeyEvent, KeyModifiers};
    
    static INTERPRETER: ConfirmInterpreter = ConfirmInterpreter::new();

    /// The `y` key shall confirm the action.
    #[test]
    fn confirm() {
        assert_eq!(INTERPRETER.decode(Input::User(Event::Key(KeyEvent::new(KeyCode::Char('y'), KeyModifiers::empty())))), vec![Operation::Quit]);
    }

    /// Any other key shall cancel the action.
    #[test]
    fn cancel() {
        assert_eq!(INTERPRETER.decode(Input::User(Event::Key(KeyEvent::new(KeyCode::Char('n'), KeyModifiers::empty())))), vec![Operation::Reset]);
        assert_eq!(INTERPRETER.decode(Input::User(Event::Key(KeyEvent::new(KeyCode::Char('c'), KeyModifiers::empty())))), vec![Operation::Reset]);
        assert_eq!(INTERPRETER.decode(Input::User(Event::Key(KeyEvent::new(KeyCode::Char('1'), KeyModifiers::empty())))), vec![Operation::Reset]);
    }
}

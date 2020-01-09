//! Implements the functionality of converting [`Input`] to [`Operation`]s.
use {
    crate::{
        app::{ConfirmAction, Operation},
        ui::Input,
        Failure,
    },
    core::fmt::Debug,
    crossterm::event::{Event, KeyCode},
    parse_display::Display as ParseDisplay,
    std::collections::HashMap,
    thiserror::Error,
};

/// Maps [`Mode`]s to their respective [`ModeInterpreter`].
#[derive(Debug)]
pub(crate) struct Interpreter {
    /// Current [`Mode`] of the `Interpreter`.
    mode: Mode,
    /// Map of [`ModeInterpreter`]s.
    map: HashMap<Mode, &'static dyn ModeInterpreter>,
}

impl Interpreter {
    /// Returns the [`ModeInterpreter`] associated with `mode`.
    ///
    /// Returns [`Err`]([`Failure`]) if `mode` is not in `self.map`.
    pub(crate) fn translate(&mut self, input: Input) -> Result<Vec<Operation>, Failure> {
        let (operations, new_mode) = self
            .map
            .get(&self.mode)
            .ok_or(Fault(self.mode))?
            .decode(input);

        if let Some(mode) = new_mode {
            self.mode = mode;
        }

        Ok(operations)
    }
}

impl Default for Interpreter {
    fn default() -> Self {
        /// The [`ModeInterpreter`] for [`Mode::View`].
        static VIEW_INTERPRETER: ViewInterpreter = ViewInterpreter::new();
        /// The [`ModeInterpreter`] for [`Mode::Confirm`].
        static CONFIRM_INTERPRETER: ConfirmInterpreter = ConfirmInterpreter::new();

        let mut map: HashMap<Mode, &'static dyn ModeInterpreter> = HashMap::new();

        let _ = map.insert(Mode::View, &VIEW_INTERPRETER);
        let _ = map.insert(Mode::Confirm, &CONFIRM_INTERPRETER);
        Self {
            map,
            mode: Mode::default(),
        }
    }
}

/// Signifies the mode of the application.
#[derive(Copy, Clone, Eq, ParseDisplay, PartialEq, Hash, Debug)]
#[display(style = "CamelCase")]
// Mode is pub due to being a member of Failure::UnknownMode.
pub(crate) enum Mode {
    /// Displays the current file.
    View,
    /// Confirms the user's action
    Confirm,
}

impl Default for Mode {
    #[inline]
    fn default() -> Self {
        Self::View
    }
}

/// A mode is not stored in [`Interpreter`].
#[derive(Clone, Copy, Debug, Error)]
#[error("mode `{0}` is unknown")]
pub struct Fault(Mode);

/// Defines the functionality to convert [`Input`] to [`Operation`]s.
pub(crate) trait ModeInterpreter: Debug {
    /// Converts `input` to [`Operation`]s.
    fn decode(&self, input: Input) -> (Vec<Operation>, Option<Mode>);
}

/// The [`ModeInterpreter`] for [`Mode::View`].
#[derive(Clone, Debug)]
pub(crate) struct ViewInterpreter {}

impl ViewInterpreter {
    /// Creates a `ViewInterpreter`.
    pub(crate) const fn new() -> Self {
        Self {}
    }
}

impl ModeInterpreter for ViewInterpreter {
    fn decode(&self, input: Input) -> (Vec<Operation>, Option<Mode>) {
        match input {
            Input::Setting(config) => (vec![Operation::UpdateConfig(config)], None),
            Input::User(event) => match event {
                Event::Key(key) => match key.code {
                    KeyCode::Esc => (vec![Operation::Reset], None),
                    KeyCode::Char('q') => (
                        vec![Operation::Confirm(ConfirmAction::Quit)],
                        Some(Mode::Confirm),
                    ),
                    KeyCode::Enter
                    | KeyCode::Backspace
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
                    | KeyCode::Null => (vec![], None),
                },
                Event::Mouse(..) | Event::Resize(..) => (vec![], None),
            },
        }
    }
}

/// The [`ModeInterpreter`] for [`Mode::Confirm`].
#[derive(Clone, Debug)]
pub(crate) struct ConfirmInterpreter {}

impl ConfirmInterpreter {
    /// Creates a new `ConfirmInterpreter`.
    pub(crate) const fn new() -> Self {
        Self {}
    }
}

impl ModeInterpreter for ConfirmInterpreter {
    fn decode(&self, input: Input) -> (Vec<Operation>, Option<Mode>) {
        match input {
            Input::User(event) => match event {
                Event::Key(key) => match key.code {
                    KeyCode::Char('y') => (vec![Operation::Quit], None),
                    KeyCode::Char(..)
                    | KeyCode::Backspace
                    | KeyCode::Enter
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
                    | KeyCode::Null
                    | KeyCode::Esc => (vec![Operation::Reset], Some(Mode::View)),
                },
                Event::Mouse(..) | Event::Resize(..) => (vec![], None),
            },
            Input::Setting(..) => (vec![], None),
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

        assert_eq!(
            view.decode(Input::User(Event::Key(KeyEvent::new(
                KeyCode::Char('q'),
                KeyModifiers::empty()
            )))),
            (
                vec![Operation::Confirm(ConfirmAction::Quit)],
                Some(Mode::Confirm)
            )
        );
    }
}

/// Tests decoding user input while in the Confirm mode.
#[cfg(test)]
mod test_confirm {
    use super::*;
    use crossterm::event::{KeyEvent, KeyModifiers};

    static INTERPRETER: ConfirmInterpreter = ConfirmInterpreter::new();

    /// The `y` key shall confirm the action.
    #[test]
    fn confirm() {
        assert_eq!(
            INTERPRETER.decode(Input::User(Event::Key(KeyEvent::new(
                KeyCode::Char('y'),
                KeyModifiers::empty()
            )))),
            (vec![Operation::Quit], None)
        );
    }

    /// Any other key shall cancel the action, resetting the application to View mode.
    #[test]
    fn cancel() {
        assert_eq!(
            INTERPRETER.decode(Input::User(Event::Key(KeyEvent::new(
                KeyCode::Char('n'),
                KeyModifiers::empty()
            )))),
            (vec![Operation::Reset], Some(Mode::View))
        );
        assert_eq!(
            INTERPRETER.decode(Input::User(Event::Key(KeyEvent::new(
                KeyCode::Char('c'),
                KeyModifiers::empty()
            )))),
            (vec![Operation::Reset], Some(Mode::View))
        );
        assert_eq!(
            INTERPRETER.decode(Input::User(Event::Key(KeyEvent::new(
                KeyCode::Char('1'),
                KeyModifiers::empty()
            )))),
            (vec![Operation::Reset], Some(Mode::View))
        );
    }
}

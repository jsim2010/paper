//! Implements the functionality of converting an [`Input`] to [`Operation`]s.
use {
    crate::{
        app::{ConfirmAction, Operation},
        ui::{Input, Key},
    },
    core::fmt::Debug,
    enum_map::{enum_map, Enum, EnumMap},
    lsp_types::{MessageType, ShowMessageParams},
    parse_display::Display as ParseDisplay,
    std::rc::Rc,
    thiserror::Error,
};

/// Maps [`Mode`]s to their respective [`ModeInterpreter`].
#[derive(Debug)]
pub(crate) struct Interpreter {
    /// Current [`Mode`] of the `Interpreter`.
    mode: Mode,
    /// Map of [`ModeInterpreter`]s.
    map: EnumMap<Mode, ConcreteInterpreter>,
}

impl Interpreter {
    /// Returns the [`Operation`]s that map to `input` given the current [`Mode`].
    pub(crate) fn translate(&mut self, input: Input) -> Vec<Operation> {
        #[allow(clippy::indexing_slicing)] // EnumMap guarantees indexing will not panic.
        let (operations, new_mode) = self.map[self.mode].decode(input);

        if let Some(mode) = new_mode {
            self.mode = mode;
        }

        operations
    }
}

impl Default for Interpreter {
    fn default() -> Self {
        Self {
            map: enum_map! {
                Mode::View => ConcreteInterpreter(Rc::new(ViewInterpreter::new())),
                Mode::Confirm => ConcreteInterpreter(Rc::new(ConfirmInterpreter::new())),
            },
            mode: Mode::default(),
        }
    }
}

/// Signifies the mode of the application.
#[derive(Copy, Clone, Debug, Enum, Eq, ParseDisplay, PartialEq, Hash)]
#[display(style = "CamelCase")]
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

/// A concrete representation of a [`ModeInterpreter`].
#[derive(Debug)]
struct ConcreteInterpreter(Rc<dyn ModeInterpreter>);

impl ConcreteInterpreter {
    /// Translates `input` into [`Operation`]s also returning the new [`Mode`].
    fn decode(&self, input: Input) -> (Vec<Operation>, Option<Mode>) {
        self.0.decode(input)
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
    const fn new() -> Self {
        Self {}
    }
}

impl ModeInterpreter for ViewInterpreter {
    fn decode(&self, input: Input) -> (Vec<Operation>, Option<Mode>) {
        match input {
            Input::Setting(config) => (vec![Operation::UpdateConfig(config)], None),
            Input::Key { key: Key::Esc, .. } => (vec![Operation::Reset], None),
            Input::Key {
                key: Key::Char('q'),
                ..
            } => (
                vec![Operation::Confirm(ConfirmAction::Quit)],
                Some(Mode::Confirm),
            ),
            Input::Glitch(fault) => (
                vec![Operation::Alert(ShowMessageParams {
                    typ: MessageType::Error,
                    message: format!("{}", fault),
                })],
                None,
            ),
            Input::Key { .. } | Input::Mouse | Input::Resize { .. } => (vec![], None),
        }
    }
}

/// The [`ModeInterpreter`] for [`Mode::Confirm`].
#[derive(Clone, Debug)]
pub(crate) struct ConfirmInterpreter {}

impl ConfirmInterpreter {
    /// Creates a new `ConfirmInterpreter`.
    const fn new() -> Self {
        Self {}
    }
}

impl ModeInterpreter for ConfirmInterpreter {
    fn decode(&self, input: Input) -> (Vec<Operation>, Option<Mode>) {
        match input {
            Input::Key {
                key: Key::Char('y'),
                ..
            } => (vec![Operation::Quit], None),
            Input::Key { .. }
            | Input::Mouse
            | Input::Resize { .. }
            | Input::Setting(..)
            | Input::Glitch(..) => (vec![Operation::Reset], Some(Mode::View)),
        }
    }
}

/// Tests decoding user input while in the View mode.
#[cfg(test)]
mod test_view {
    use super::*;
    use crate::ui::Modifiers;

    /// The `q` key shall confirm the user wants to quit.
    #[test]
    fn quit() {
        let view = ViewInterpreter::new();

        assert_eq!(
            view.decode(Input::Key {
                key: Key::Char('q'),
                modifiers: Modifiers::empty()
            }),
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
    use crate::ui::Modifiers;

    static INTERPRETER: ConfirmInterpreter = ConfirmInterpreter::new();

    /// The `y` key shall confirm the action.
    #[test]
    fn confirm() {
        assert_eq!(
            INTERPRETER.decode(Input::Key {
                key: Key::Char('y'),
                modifiers: Modifiers::empty()
            }),
            (vec![Operation::Quit], None)
        );
    }

    /// Any other key shall cancel the action, resetting the application to View mode.
    #[test]
    fn cancel() {
        assert_eq!(
            INTERPRETER.decode(Input::Key {
                key: Key::Char('n'),
                modifiers: Modifiers::empty()
            }),
            (vec![Operation::Reset], Some(Mode::View))
        );
        assert_eq!(
            INTERPRETER.decode(Input::Key {
                key: Key::Char('c'),
                modifiers: Modifiers::empty()
            }),
            (vec![Operation::Reset], Some(Mode::View))
        );
        assert_eq!(
            INTERPRETER.decode(Input::Key {
                key: Key::Char('1'),
                modifiers: Modifiers::empty()
            }),
            (vec![Operation::Reset], Some(Mode::View))
        );
    }
}

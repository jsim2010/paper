//! Implements the functionality of converting an [`Input`] to [`Operation`]s.
use {
    crate::{
        app::{Command, ConfirmAction, Operation},
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
        let output = self.map[self.mode].decode(input);

        if let Some(mode) = output.new_mode {
            self.mode = mode;
        }

        output.operations
    }
}

impl Default for Interpreter {
    fn default() -> Self {
        Self {
            map: enum_map! {
                Mode::View => ConcreteInterpreter(Rc::new(ViewInterpreter::new())),
                Mode::Confirm => ConcreteInterpreter(Rc::new(ConfirmInterpreter::new())),
                Mode::Collect => ConcreteInterpreter(Rc::new(CollectInterpreter::new())),
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
    /// Collects input from the user.
    Collect,
}

impl Default for Mode {
    #[inline]
    fn default() -> Self {
        Self::View
    }
}

/// Signifies the data gleaned from user input.
#[derive(Debug, Default, PartialEq)]
pub(crate) struct Output {
    /// The operations to be run.
    operations: Vec<Operation>,
    /// The mode to switch to.
    ///
    /// If None, interpreter should not switch modes.
    new_mode: Option<Mode>,
}

impl Output {
    /// Creates a new `Output`.
    fn new() -> Self {
        Self::default()
    }

    /// Adds `operation` to `self`.
    fn add_op(&mut self, operation: Operation) {
        self.operations.push(operation);
    }

    /// Modifies to the [`Operation::Reset`].
    fn reset(&mut self) {
        self.add_op(Operation::Reset);
        self.new_mode = Some(Mode::View);
    }
}

/// A concrete representation of a [`ModeInterpreter`].
#[derive(Debug)]
struct ConcreteInterpreter(Rc<dyn ModeInterpreter>);

impl ConcreteInterpreter {
    /// Translates `input` into [`Output`].
    fn decode(&self, input: Input) -> Output {
        self.0.decode(input)
    }
}

/// A mode is not stored in [`Interpreter`].
#[derive(Clone, Copy, Debug, Error)]
#[error("mode `{0}` is unknown")]
pub struct Fault(Mode);

/// Defines the functionality to convert [`Input`] to [`Output`].
pub(crate) trait ModeInterpreter: Debug {
    /// Converts `input` to [`Operation`]s.
    fn decode(&self, input: Input) -> Output;
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
    fn decode(&self, input: Input) -> Output {
        let mut output = Output::new();

        match input {
            Input::Setting(config) => {
                output.add_op(Operation::UpdateConfig(config));
            }
            Input::Key { key: Key::Esc, .. } => {
                output.add_op(Operation::Reset);
            }
            Input::Key {
                key: Key::Char('q'),
                ..
            } => {
                output.add_op(Operation::Confirm(ConfirmAction::Quit));
                output.new_mode = Some(Mode::Confirm);
            }
            Input::Key {
                key: Key::Char('o'),
                ..
            } => {
                output.add_op(Operation::StartCommand(Command::Open));
                output.new_mode = Some(Mode::Collect);
            }
            Input::Glitch(fault) => {
                output.add_op(Operation::Alert(ShowMessageParams {
                    typ: MessageType::Error,
                    message: format!("{}", fault),
                }));
            }
            Input::Key { .. } | Input::Mouse | Input::Resize { .. } => {}
        }

        output
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
    fn decode(&self, input: Input) -> Output {
        let mut output = Output::new();

        match input {
            Input::Key {
                key: Key::Char('y'),
                ..
            } => {
                output.add_op(Operation::Quit);
            }
            Input::Key { .. }
            | Input::Mouse
            | Input::Resize { .. }
            | Input::Setting(..)
            | Input::Glitch(..) => {
                output.reset();
            }
        }

        output
    }
}

/// The [`ModeInterpreter`] for [`Mode::Collect`].
#[derive(Clone, Debug)]
struct CollectInterpreter {}

impl CollectInterpreter {
    /// Creates a new `CollectInterpreter`.
    const fn new() -> Self {
        Self {}
    }
}

impl ModeInterpreter for CollectInterpreter {
    fn decode(&self, input: Input) -> Output {
        let mut output = Output::new();

        match input {
            Input::Key { key: Key::Esc, .. } => {
                output.reset();
            }
            Input::Key {
                key: Key::Char(c), ..
            } => {
                output.add_op(Operation::Collect(c));
            }
            Input::Key { .. }
            | Input::Mouse
            | Input::Resize { .. }
            | Input::Setting(..)
            | Input::Glitch(..) => {}
        }

        output
    }
}

/// Tests decoding user input while in the View mode.
#[cfg(test)]
mod test_view {
    use super::*;

    static INTERPRETER: ViewInterpreter = ViewInterpreter::new();

    /// The `q` key shall confirm the user wants to quit.
    #[test]
    fn quit() {
        assert_eq!(
            INTERPRETER.decode(helpers::key_input(Key::Char('q'))),
            Output {
                operations: vec![Operation::Confirm(ConfirmAction::Quit)],
                new_mode: Some(Mode::Confirm)
            }
        );
    }

    /// The `o` key shall request the name of the document to be opened.
    #[test]
    fn open() {
        assert_eq!(
            INTERPRETER.decode(helpers::key_input(Key::Char('o'))),
            Output {
                operations: vec![Operation::StartCommand(Command::Open)],
                new_mode: Some(Mode::Collect)
            }
        );
    }
}

/// Tests decoding user input while in the Confirm mode.
#[cfg(test)]
mod test_confirm {
    use super::*;

    static INTERPRETER: ConfirmInterpreter = ConfirmInterpreter::new();

    /// The `y` key shall confirm the action.
    #[test]
    fn confirm() {
        assert_eq!(
            INTERPRETER.decode(helpers::key_input(Key::Char('y'))),
            Output {
                operations: vec![Operation::Quit],
                new_mode: None
            }
        );
    }

    /// Any other key shall cancel the action, resetting the application to View mode.
    #[test]
    fn cancel() {
        assert_eq!(
            INTERPRETER.decode(helpers::key_input(Key::Char('n'))),
            Output {
                operations: vec![Operation::Reset],
                new_mode: Some(Mode::View)
            }
        );
        assert_eq!(
            INTERPRETER.decode(helpers::key_input(Key::Char('c'))),
            Output {
                operations: vec![Operation::Reset],
                new_mode: Some(Mode::View)
            }
        );
        assert_eq!(
            INTERPRETER.decode(helpers::key_input(Key::Char('1'))),
            Output {
                operations: vec![Operation::Reset],
                new_mode: Some(Mode::View)
            }
        );
    }
}

/// Tests decoding user input while mode is [`Mode::Collect`].
#[cfg(test)]
mod test_collect {
    use super::*;

    static INTERPRETER: CollectInterpreter = CollectInterpreter::new();

    /// The `Esc` key shall return to [`Mode::View`].
    #[test]
    fn reset() {
        assert_eq!(
            INTERPRETER.decode(helpers::key_input(Key::Esc)),
            Output {
                operations: vec![Operation::Reset],
                new_mode: Some(Mode::View)
            },
        );
    }

    /// All char keys shall be collected.
    #[test]
    fn collect() {
        assert_eq!(
            INTERPRETER.decode(helpers::key_input(Key::Char('a'))),
            Output {
                operations: vec![Operation::Collect('a')],
                new_mode: None
            },
        );
        assert_eq!(
            INTERPRETER.decode(helpers::key_input(Key::Char('.'))),
            Output {
                operations: vec![Operation::Collect('.')],
                new_mode: None
            },
        );
        assert_eq!(
            INTERPRETER.decode(helpers::key_input(Key::Char('1'))),
            Output {
                operations: vec![Operation::Collect('1')],
                new_mode: None
            },
        );
    }
}

/// Helper functions for testing.
#[cfg(test)]
mod helpers {
    use super::*;
    use crate::ui::Modifiers;

    pub(crate) fn key_input(key: Key) -> Input {
        Input::Key {
            key,
            modifiers: Modifiers::empty(),
        }
    }
}

//! Implements the functionality of interpreting an [`Input`] into [`Operation`]s.
use {
    crate::{
        app::{Command, ConfirmAction, Movement, Operation},
        ui::{Input, Key},
    },
    core::fmt::Debug,
    enum_map::{enum_map, Enum, EnumMap},
    lsp_types::{MessageType, ShowMessageParams},
    parse_display::Display as ParseDisplay,
};

/// Manages interpretation for the application.
///
/// How an [`Input`] maps to [`Operation`]s is determined by the [`Mode`] of the [`Interpreter`]. Each mode defines a struct to implement [`ModeInterpreter`].
#[derive(Debug)]
pub(crate) struct Interpreter {
    /// Signifies the current [`Mode`] of the [`Interpreter`].
    mode: Mode,
    /// Map of [`ModeInterpreter`]s.
    map: EnumMap<Mode, &'static dyn ModeInterpreter>,
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
        /// The [`ModeInterpreter`] for [`Mode::View`].
        static VIEW_INTERPRETER: ViewInterpreter = ViewInterpreter::new();
        /// The [`ModeInterpreter`] for [`Mode::Confirm`].
        static CONFIRM_INTERPRETER: ConfirmInterpreter = ConfirmInterpreter::new();
        /// The [`ModeInterpreter`] for [`Mode::Collect`].
        static COLLECT_INTERPRETER: CollectInterpreter = CollectInterpreter::new();

        // Required to establish value type in enum_map.
        let view_interpreter: &dyn ModeInterpreter = &VIEW_INTERPRETER;

        Self {
            map: enum_map! {
                Mode::View => view_interpreter,
                Mode::Confirm => &CONFIRM_INTERPRETER,
                Mode::Collect => &COLLECT_INTERPRETER,
            },
            mode: Mode::default(),
        }
    }
}

/// Signifies the mode of the application.
#[allow(clippy::unreachable)] // unreachable added by derive(Enum).
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

    /// Sets the mode of `self` to `mode`.
    fn set_mode(&mut self, mode: Mode) {
        self.new_mode = Some(mode);
    }

    /// Modifies to the [`Operation::Reset`].
    fn reset(&mut self) {
        self.add_op(Operation::Reset);
        self.set_mode(Mode::View);
    }
}

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

    /// Converts `output` appropriate to `key`.
    fn decode_key(key: Key, output: &mut Output) {
        match key {
            Key::Esc => {
                output.add_op(Operation::Reset);
            }
            Key::Char('q') => {
                output.add_op(Operation::Confirm(ConfirmAction::Quit));
                output.set_mode(Mode::Confirm);
            }
            Key::Char('o') => {
                output.add_op(Operation::StartCommand(Command::Open));
                output.set_mode(Mode::Collect);
            }
            Key::Char('j') => {
                output.add_op(Operation::Move(Movement::SingleDown));
            }
            Key::Char('k') => {
                output.add_op(Operation::Move(Movement::SingleUp));
            }
            Key::Char('J') => {
                output.add_op(Operation::Move(Movement::HalfDown));
            }
            Key::Char('K') => {
                output.add_op(Operation::Move(Movement::HalfUp));
            }
            Key::Char('d') => {
                output.add_op(Operation::Delete);
            }
            Key::Backspace
            | Key::Enter
            | Key::Left
            | Key::Right
            | Key::Up
            | Key::Down
            | Key::Home
            | Key::End
            | Key::PageUp
            | Key::PageDown
            | Key::Tab
            | Key::BackTab
            | Key::Delete
            | Key::Insert
            | Key::F(..)
            | Key::Null
            | Key::Char(..) => {}
        }
    }
}

impl ModeInterpreter for ViewInterpreter {
    fn decode(&self, input: Input) -> Output {
        let mut output = Output::new();

        match input {
            Input::Setting(config) => {
                output.add_op(Operation::UpdateConfig(config));
            }
            Input::Key { key, .. } => {
                Self::decode_key(key, &mut output);
            }
            Input::Glitch(fault) => {
                output.add_op(Operation::Alert(ShowMessageParams {
                    typ: MessageType::Error,
                    message: format!("{}", fault),
                }));
            }
            Input::Mouse | Input::Resize { .. } => {}
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
                key: Key::Enter, ..
            } => {
                output.add_op(Operation::Execute);
                output.set_mode(Mode::View);
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

/// Testing of the translate module.
#[cfg(test)]
mod test {
    use super::*;
    use crate::ui::Modifiers;

    fn char_input(c: char) -> Input {
        key_input(Key::Char(c))
    }

    fn key_input(key: Key) -> Input {
        Input::Key {
            key,
            modifiers: Modifiers::empty(),
        }
    }

    fn output(operation: Operation, mode: Mode) -> Output {
        Output {
            operations: vec![operation],
            new_mode: Some(mode),
        }
    }

    fn keep_mode(operation: Operation) -> Output {
        Output {
            operations: vec![operation],
            new_mode: None,
        }
    }

    /// Tests decoding user input while in the View mode.
    mod view {
        use super::*;

        static INTERPRETER: ViewInterpreter = ViewInterpreter::new();

        /// The `q` key shall confirm the user wants to quit.
        #[test]
        fn quit() {
            assert_eq!(
                INTERPRETER.decode(char_input('q')),
                output(Operation::Confirm(ConfirmAction::Quit), Mode::Confirm)
            );
        }

        /// The `o` key shall request the name of the document to be opened.
        #[test]
        fn open() {
            assert_eq!(
                INTERPRETER.decode(key_input(Key::Char('o'))),
                output(Operation::StartCommand(Command::Open), Mode::Collect)
            );
        }

        /// The 'j' key shall move the cursor down.
        #[test]
        fn move_down() {
            assert_eq!(
                INTERPRETER.decode(key_input(Key::Char('j'))),
                keep_mode(Operation::Move(Movement::SingleDown))
            );
        }

        /// The 'k' key shall move the cursor up.
        #[test]
        fn move_up() {
            assert_eq!(
                INTERPRETER.decode(key_input(Key::Char('k'))),
                keep_mode(Operation::Move(Movement::SingleUp))
            );
        }

        /// The 'J' key shall scroll the document down.
        #[test]
        fn scroll_down() {
            assert_eq!(
                INTERPRETER.decode(key_input(Key::Char('J'))),
                keep_mode(Operation::Move(Movement::HalfDown))
            );
        }

        /// The 'K' key shall scroll the document up.
        #[test]
        fn scroll_up() {
            assert_eq!(
                INTERPRETER.decode(key_input(Key::Char('K'))),
                keep_mode(Operation::Move(Movement::HalfUp))
            );
        }

        /// The 'd' key shall delete the current selection.
        #[test]
        fn delete() {
            assert_eq!(
                INTERPRETER.decode(char_input('d')),
                keep_mode(Operation::Delete)
            );
        }
    }

    /// Tests decoding user input while in the Confirm mode.
    mod confirm {
        use super::*;

        static INTERPRETER: ConfirmInterpreter = ConfirmInterpreter::new();

        /// The `y` key shall confirm the action.
        #[test]
        fn confirm() {
            assert_eq!(
                INTERPRETER.decode(key_input(Key::Char('y'))),
                keep_mode(Operation::Quit)
            );
        }

        /// Any other key shall cancel the action, resetting the application to View mode.
        #[test]
        fn cancel() {
            assert_eq!(
                INTERPRETER.decode(key_input(Key::Char('n'))),
                output(Operation::Reset, Mode::View)
            );
            assert_eq!(
                INTERPRETER.decode(key_input(Key::Char('c'))),
                output(Operation::Reset, Mode::View)
            );
            assert_eq!(
                INTERPRETER.decode(key_input(Key::Char('1'))),
                output(Operation::Reset, Mode::View)
            );
        }
    }

    /// Tests decoding user input while mode is [`Mode::Collect`].
    #[cfg(test)]
    mod collect {
        use super::*;

        static INTERPRETER: CollectInterpreter = CollectInterpreter::new();

        /// The `Esc` key shall return to [`Mode::View`].
        #[test]
        fn reset() {
            assert_eq!(
                INTERPRETER.decode(key_input(Key::Esc)),
                output(Operation::Reset, Mode::View)
            );
        }

        /// All char keys shall be collected.
        #[test]
        fn collect() {
            assert_eq!(
                INTERPRETER.decode(key_input(Key::Char('a'))),
                keep_mode(Operation::Collect('a'))
            );
            assert_eq!(
                INTERPRETER.decode(key_input(Key::Char('.'))),
                keep_mode(Operation::Collect('.'))
            );
            assert_eq!(
                INTERPRETER.decode(key_input(Key::Char('1'))),
                keep_mode(Operation::Collect('1'))
            );
        }

        /// The `Enter` key shall execute the command and return to [`Mode::View`].
        #[test]
        fn execute() {
            assert_eq!(
                INTERPRETER.decode(key_input(Key::Enter)),
                output(Operation::Execute, Mode::View)
            );
        }
    }
}

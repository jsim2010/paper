//! Implements the functionality of interpreting an [`Input`] into [`Operation`]s.
use {
    crate::{
        app::{Command, ConfirmAction, Direction, DocOp, Magnitude, Operation, Vector},
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
enum Mode {
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
struct Output {
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
trait ModeInterpreter: Debug {
    /// Converts `input` to [`Operation`]s.
    fn decode(&self, input: Input) -> Output;
}

/// The [`ModeInterpreter`] for [`Mode::View`].
#[derive(Clone, Debug)]
struct ViewInterpreter {}

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
                output.add_op(Operation::Document(DocOp::Move(Vector::new(
                    Direction::Down,
                    Magnitude::Single,
                ))));
            }
            Key::Char('k') => {
                output.add_op(Operation::Document(DocOp::Move(Vector::new(
                    Direction::Up,
                    Magnitude::Single,
                ))));
            }
            Key::Char('J') => {
                output.add_op(Operation::Document(DocOp::Move(Vector::new(
                    Direction::Down,
                    Magnitude::Half,
                ))));
            }
            Key::Char('K') => {
                output.add_op(Operation::Document(DocOp::Move(Vector::new(
                    Direction::Up,
                    Magnitude::Half,
                ))));
            }
            Key::Char('d') => {
                output.add_op(Operation::Document(DocOp::Delete));
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
                output.add_op(Operation::UpdateSetting(config));
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
struct ConfirmInterpreter {}

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
    use crate::ui::{Glitch, Setting, Modifiers};

    /// Tests decoding user input while the [`Interpreter`] is in [`Mode::View`].
    mod view {
        use super::*;

        fn view_mode() -> Interpreter {
            Interpreter::default()
        }

        /// Receiving a glitch shall display the message.
        #[test]
        fn glitch() {
            let mut int = view_mode();

            assert_eq!(
                int.translate(Input::Glitch(Glitch::WatcherConnection)),
                vec![Operation::Alert(ShowMessageParams {
                    typ: MessageType::Error,
                    message: "config file watcher disconnected".to_string(),
                })]
            );
            assert_eq!(int.mode, Mode::View);
        }

        /// A new setting shall be forwarded to the application.
        #[test]
        fn setting() {
            let mut int = view_mode();

            assert_eq!(
                int.translate(Input::Setting(Setting::Wrap(true))),
                vec![Operation::UpdateSetting(Setting::Wrap(true))]
            );
            assert_eq!(int.mode, Mode::View);
        }

        /// The `q` key shall confirm the user wants to quit.
        #[test]
        fn quit() {
            let mut int = view_mode();

            assert_eq!(
                int.translate(Input::Key {
                    key: Key::Char('q'),
                    modifiers: Modifiers::empty(),
                }),
                vec![Operation::Confirm(ConfirmAction::Quit)],
            );
            assert_eq!(int.mode, Mode::Confirm);
        }

        /// The `o` key shall request the name of the document to be opened.
        #[test]
        fn open() {
            let mut int = view_mode();

            assert_eq!(
                int.translate(Input::Key {
                    key: Key::Char('o'),
                    modifiers: Modifiers::empty(),
                }),
                vec![Operation::StartCommand(Command::Open)]
            );
            assert_eq!(int.mode, Mode::Collect);
        }

        /// The 'j' key shall move the cursor down.
        #[test]
        fn move_down() {
            let mut int = view_mode();

            assert_eq!(
                int.translate(Input::Key {
                    key: Key::Char('j'),
                    modifiers: Modifiers::empty(),
                }),
                vec![Operation::Document(DocOp::Move(Vector::new(
                    Direction::Down,
                    Magnitude::Single
                )))]
            );
            assert_eq!(int.mode, Mode::View);
        }

        /// The 'k' key shall move the cursor up.
        #[test]
        fn move_up() {
            let mut int = view_mode();

            assert_eq!(
                int.translate(Input::Key {
                    key: Key::Char('k'),
                    modifiers: Modifiers::empty(),
                }),
                vec![Operation::Document(DocOp::Move(Vector::new(
                    Direction::Up,
                    Magnitude::Single
                )))]
            );
            assert_eq!(int.mode, Mode::View);
        }

        /// The 'J' key shall scroll the document down.
        #[test]
        fn scroll_down() {
            let mut int = view_mode();

            assert_eq!(
                int.translate(Input::Key {
                    key: Key::Char('J'),
                    modifiers: Modifiers::empty(),
                }),
                vec![Operation::Document(DocOp::Move(Vector::new(
                    Direction::Down,
                    Magnitude::Half
                )))]
            );
            assert_eq!(int.mode, Mode::View);
        }

        /// The 'K' key shall scroll the document up.
        #[test]
        fn scroll_up() {
            let mut int = view_mode();

            assert_eq!(
                int.translate(Input::Key {
                    key: Key::Char('K'),
                    modifiers: Modifiers::empty(),
                }),
                vec![Operation::Document(DocOp::Move(Vector::new(
                    Direction::Up,
                    Magnitude::Half
                )))]
            );
            assert_eq!(int.mode, Mode::View);
        }

        /// The 'd' key shall delete the current selection.
        #[test]
        fn delete() {
            let mut int = view_mode();

            assert_eq!(
                int.translate(Input::Key {
                    key: Key::Char('d'),
                    modifiers: Modifiers::empty(),
                }),
                vec![Operation::Document(DocOp::Delete)]
            );
            assert_eq!(int.mode, Mode::View);
        }
    }

    /// Tests decoding user input while in the Confirm mode.
    mod confirm {
        use super::*;

        fn confirm_mode() -> Interpreter {
            let mut int = Interpreter::default();
            int.mode = Mode::Confirm;
            int
        }

        /// The `y` key shall confirm the action.
        #[test]
        fn confirm() {
            let mut int = confirm_mode();

            assert_eq!(
                int.translate(Input::Key {
                    key: Key::Char('y'),
                    modifiers: Modifiers::empty(),
                }),
                vec![Operation::Quit],
            );
        }

        /// Any other key shall cancel the action, resetting the application to View mode.
        #[test]
        fn cancel() {
            let mut int = confirm_mode();

            assert_eq!(
                int.translate(Input::Key {
                    key: Key::Char('n'),
                    modifiers: Modifiers::empty(),
                }),
                vec![Operation::Reset],
            );
            assert_eq!(int.mode, Mode::View);

            int = confirm_mode();

            assert_eq!(
                int.translate(Input::Key {
                    key: Key::Char('1'),
                    modifiers: Modifiers::empty(),
                }),
                vec![Operation::Reset],
            );
            assert_eq!(int.mode, Mode::View);
        }
    }

    /// Tests decoding user input while mode is [`Mode::Collect`].
    #[cfg(test)]
    mod collect {
        use super::*;

        fn collect_mode() -> Interpreter {
            let mut int = Interpreter::default();
            int.mode = Mode::Collect;
            int
        }

        /// The `Esc` key shall return to [`Mode::View`].
        #[test]
        fn reset() {
            let mut int = collect_mode();

            assert_eq!(
                int.translate(Input::Key {
                    key: Key::Esc,
                    modifiers: Modifiers::empty(),
                }),
                vec![Operation::Reset]
            );
            assert_eq!(int.mode, Mode::View);
        }

        /// All char keys shall be collected.
        #[test]
        fn collect() {
            let mut int = collect_mode();

            assert_eq!(
                int.translate(Input::Key {
                    key: Key::Char('a'),
                    modifiers: Modifiers::empty(),
                }),
                vec![Operation::Collect('a')]
            );
            assert_eq!(int.mode, Mode::Collect);

            int = collect_mode();

            assert_eq!(
                int.translate(Input::Key {
                    key: Key::Char('.'),
                    modifiers: Modifiers::empty(),
                }),
                vec![Operation::Collect('.')]
            );
            assert_eq!(int.mode, Mode::Collect);

            int = collect_mode();

            assert_eq!(
                int.translate(Input::Key {
                    key: Key::Char('1'),
                    modifiers: Modifiers::empty(),
                }),
                vec![Operation::Collect('1')]
            );
            assert_eq!(int.mode, Mode::Collect);
        }

        /// The `Enter` key shall execute the command and return to [`Mode::View`].
        #[test]
        fn execute() {
            let mut int = collect_mode();

            assert_eq!(
                int.translate(Input::Key {
                    key: Key::Enter,
                    modifiers: Modifiers::empty(),
                }),
                vec![Operation::Execute]
            );
            assert_eq!(int.mode, Mode::View);
        }
    }
}

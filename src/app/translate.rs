//! Implements the functionality of interpreting an [`Input`] into [`Operation`]s.
#![allow(clippy::pattern_type_mismatch)] // False positive.
use {
    crate::{
        io::{Dimensions, File, Input, UserAction},
        orient,
    },
    core::fmt::{self, Debug},
    crossterm::event::KeyCode,
    enum_map::{enum_map, Enum, EnumMap},
    lsp_types::{MessageType, ShowMessageRequestParams},
    parse_display::Display as ParseDisplay,
};

/// Signifies actions that can be performed by the application.
#[derive(Debug, PartialEq)]
pub(crate) enum Operation {
    /// Updates the display to `size`.
    Resize {
        /// The new [`Dimensions`].
        dimensions: Dimensions,
    },
    /// Resets the application.
    Reset,
    /// Confirms that the action is desired.
    Confirm(ConfirmAction),
    /// Quits the application.
    Quit,
    /// Open input box for a command.
    StartCommand,
    /// Input to input box.
    Collect(char),
    /// Executes the current command.
    Execute,
    /// Creates a document from the file.
    CreateDoc(File),
    /// Scrolls the document.
    Scroll(orient::ScreenDirection),
    /// Changes the selection.
    ChangeSelection(SelectionMovement),
}

/// Describes the movement of a selection.
#[derive(Debug, PartialEq)]
pub(crate) enum SelectionMovement {
    /// Moves to the first child of the current selection.
    Descend,
    /// Moves to the parent of the current selection.
    Ascend,
    /// Moves to the next child of the parent of the current selection.
    Increment,
    /// Moves to the previous child of the parent of the current selection.
    Decrement,
}

/// Signifies actions that require a confirmation prior to their execution.
#[derive(Debug, PartialEq)]
pub(crate) enum ConfirmAction {
    /// Quit the application.
    Quit,
}

impl fmt::Display for ConfirmAction {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "You have input that you want to quit the application.\nPlease confirm this action by pressing `y`. To cancel this action, press any other key.")
    }
}

impl From<ConfirmAction> for ShowMessageRequestParams {
    #[inline]
    #[must_use]
    fn from(value: ConfirmAction) -> Self {
        Self {
            typ: MessageType::Info,
            message: value.to_string(),
            actions: None,
        }
    }
}

/// An operation performed on a document.
#[derive(Debug, PartialEq)]
pub(crate) enum DocOp {
    /// Saves the document.
    Save,
}

impl fmt::Display for DocOp {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}",
            match *self {
                Self::Save => "save",
            }
        )
    }
}

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
    /// Returns the [`Operation`] that maps to `input` given the current [`Mode`].
    pub(crate) fn translate(&mut self, input: Input) -> Option<Operation> {
        let mut output = Output::new();

        match input {
            Input::File(file) => {
                output.add_op(Operation::CreateDoc(file));
            }
            Input::Lsp(_reception) => {
                // Processing of receptions to be added here.
            }
            Input::User(user_input) => {
                #[allow(clippy::indexing_slicing)] // EnumMap guarantees that index is in bounds.
                let mode_interpreter = self.map[self.mode];

                output = mode_interpreter.decode(user_input);
            }
        }

        if let Some(mode) = output.new_mode {
            self.mode = mode;
        }

        output.operation
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
    /// The operation to be run.
    operation: Option<Operation>,
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
        let _ = self.operation.replace(operation);
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
    fn decode(&self, input: UserAction) -> Output;
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
    fn decode_key(key: KeyCode, output: &mut Output) {
        match key {
            KeyCode::Esc => {
                output.add_op(Operation::Reset);
            }
            KeyCode::Char('w') => {
                output.add_op(Operation::Confirm(ConfirmAction::Quit));
                output.set_mode(Mode::Confirm);
            }
            // TODO: Change this to Ctrl+L.
            KeyCode::Char(';') => {
                output.add_op(Operation::StartCommand);
                output.set_mode(Mode::Collect);
            }
            // TODO: Change this to Space.
            KeyCode::Char('d') => {
                output.add_op(Operation::Scroll(orient::ScreenDirection::Down));
            }
            // TODO: Change this to Shift+Space.
            KeyCode::Char('u') => {
                output.add_op(Operation::Scroll(orient::ScreenDirection::Up));
            }
            KeyCode::Char('h') => {
                output.add_op(Operation::ChangeSelection(SelectionMovement::Ascend));
            }
            KeyCode::Char('j') => {
                output.add_op(Operation::ChangeSelection(SelectionMovement::Increment));
            }
            KeyCode::Char('k') => {
                output.add_op(Operation::ChangeSelection(SelectionMovement::Decrement));
            }
            KeyCode::Char('l') => {
                output.add_op(Operation::ChangeSelection(SelectionMovement::Descend));
            }
            KeyCode::Backspace
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
            | KeyCode::Char(..) => {}
        }
    }
}

impl ModeInterpreter for ViewInterpreter {
    fn decode(&self, input: UserAction) -> Output {
        let mut output = Output::new();

        match input {
            UserAction::Key { code, .. } => {
                Self::decode_key(code, &mut output);
            }
            UserAction::Resize { dimensions } => {
                output.add_op(Operation::Resize { dimensions });
            }
            UserAction::Mouse => {}
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
    fn decode(&self, input: UserAction) -> Output {
        let mut output = Output::new();

        match input {
            UserAction::Key {
                code: KeyCode::Char('y'),
                ..
            } => {
                output.add_op(Operation::Quit);
            }
            UserAction::Key { .. } | UserAction::Mouse | UserAction::Resize { .. } => {
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
    fn decode(&self, input: UserAction) -> Output {
        let mut output = Output::new();

        match input {
            UserAction::Key {
                code: KeyCode::Esc, ..
            } => {
                output.reset();
            }
            UserAction::Key {
                code: KeyCode::Enter,
                ..
            } => {
                output.add_op(Operation::Execute);
                output.set_mode(Mode::View);
            }
            UserAction::Key {
                code: KeyCode::Char(c),
                ..
            } => {
                output.add_op(Operation::Collect(c));
            }
            UserAction::Key { .. } | UserAction::Mouse | UserAction::Resize { .. } => {}
        }

        output
    }
}

/// Testing of the translate module.
#[cfg(test)]
mod test {
    use {super::*, crossterm::event::KeyModifiers};

    /// Tests decoding user input while the [`Interpreter`] is in [`Mode::View`].
    mod view {
        use super::*;

        fn view_mode() -> Interpreter {
            Interpreter::default()
        }

        /// The `Ctrl-w` key shall confirm the user wants to quit.
        #[test]
        fn quit() {
            let mut int = view_mode();

            assert_eq!(
                int.translate(Input::User(UserAction::Key {
                    code: KeyCode::Char('w'),
                    modifiers: KeyModifiers::CONTROL,
                })),
                Some(Operation::Confirm(ConfirmAction::Quit))
            );
            assert_eq!(int.mode, Mode::Confirm);
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
                int.translate(Input::User(UserAction::Key {
                    code: KeyCode::Char('y'),
                    modifiers: KeyModifiers::empty(),
                })),
                Some(Operation::Quit)
            );
        }

        /// Any other key shall cancel the action, resetting the application to View mode.
        #[test]
        fn cancel() {
            let mut int = confirm_mode();

            assert_eq!(
                int.translate(Input::User(UserAction::Key {
                    code: KeyCode::Char('n'),
                    modifiers: KeyModifiers::empty(),
                })),
                Some(Operation::Reset)
            );
            assert_eq!(int.mode, Mode::View);

            int = confirm_mode();

            assert_eq!(
                int.translate(Input::User(UserAction::Key {
                    code: KeyCode::Char('1'),
                    modifiers: KeyModifiers::empty(),
                })),
                Some(Operation::Reset)
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
                int.translate(Input::User(UserAction::Key {
                    code: KeyCode::Esc,
                    modifiers: KeyModifiers::empty(),
                })),
                Some(Operation::Reset)
            );
            assert_eq!(int.mode, Mode::View);
        }

        /// All char keys shall be collected.
        #[test]
        fn collect() {
            let mut int = collect_mode();

            assert_eq!(
                int.translate(Input::User(UserAction::Key {
                    code: KeyCode::Char('a'),
                    modifiers: KeyModifiers::empty(),
                })),
                Some(Operation::Collect('a'))
            );
            assert_eq!(int.mode, Mode::Collect);

            int = collect_mode();

            assert_eq!(
                int.translate(Input::User(UserAction::Key {
                    code: KeyCode::Char('.'),
                    modifiers: KeyModifiers::empty(),
                })),
                Some(Operation::Collect('.'))
            );
            assert_eq!(int.mode, Mode::Collect);

            int = collect_mode();

            assert_eq!(
                int.translate(Input::User(UserAction::Key {
                    code: KeyCode::Char('1'),
                    modifiers: KeyModifiers::empty(),
                })),
                Some(Operation::Collect('1'))
            );
            assert_eq!(int.mode, Mode::Collect);
        }

        /// The `Enter` key shall execute the command and return to [`Mode::View`].
        #[test]
        fn execute() {
            let mut int = collect_mode();

            assert_eq!(
                int.translate(Input::User(UserAction::Key {
                    code: KeyCode::Enter,
                    modifiers: KeyModifiers::empty(),
                })),
                Some(Operation::Execute)
            );
            assert_eq!(int.mode, Mode::View);
        }
    }
}

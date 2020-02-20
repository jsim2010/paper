//! Implements the functionality of interpreting an [`Input`] into [`Operation`]s.
use {
    crate::io::{
        ui::{self, Key},
        Input, PathUrl, Setting,
    },
    core::fmt::{self, Debug},
    enum_map::{enum_map, Enum, EnumMap},
    lsp_types::{MessageType, ShowMessageParams, ShowMessageRequestParams},
    parse_display::Display as ParseDisplay,
};

/// Signifies actions that can be performed by the application.
#[derive(Debug, PartialEq)]
pub(crate) enum Operation {
    /// Resizes the user interface.
    Size(ui::Size),
    /// Resets the application.
    Reset,
    /// Confirms that the action is desired.
    Confirm(ConfirmAction),
    /// Quits the application.
    Quit,
    /// Updates a setting.
    UpdateSetting(Setting),
    /// Alerts the user with a message.
    Alert(ShowMessageParams),
    /// Open input box for a command.
    StartCommand(Command),
    /// Input to input box.
    Collect(char),
    /// Executes the current command.
    Execute,
    /// An operation to edit the text or selection of the document.
    Document(DocOp),
    /// Opens a file.
    OpenDoc {
        /// The URL of the file.
        url: PathUrl,
        /// The text of the file.
        text: String,
    },
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

/// Signifies a command that a user can give to the application.
#[derive(Debug, ParseDisplay, PartialEq)]
pub(crate) enum Command {
    /// Opens a given file.
    #[display("Open <file>")]
    Open,
}

/// An operation performed on a document.
#[derive(Debug, PartialEq)]
pub(crate) enum DocOp {
    /// Moves the cursor.
    Move(Vector),
    /// Deletes the current selection.
    Delete,
    /// Saves the document.
    Save,
}

impl fmt::Display for DocOp {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}",
            match self {
                Self::Move(..) => "selection movement",
                Self::Delete => "deletion",
                Self::Save => "save",
            }
        )
    }
}

/// A movement to the cursor.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct Vector {
    /// The direction of the movement.
    direction: Direction,
    /// The magnitude of the movement.
    magnitude: Magnitude,
}

impl Vector {
    /// Creates a new [`Vector`].
    pub(crate) const fn new(direction: Direction, magnitude: Magnitude) -> Self {
        Self {
            direction,
            magnitude,
        }
    }

    /// Returns the direction of `self`.
    pub(crate) const fn direction(&self) -> &Direction {
        &self.direction
    }

    /// Returns the magnitude of `self`.
    pub(crate) const fn magnitude(&self) -> &Magnitude {
        &self.magnitude
    }
}

/// Describes the direction of a movement.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum Direction {
    /// Towards the bottom.
    Down,
    /// Towards the top.
    Up,
}

/// Describes the magnitude of a movement.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum Magnitude {
    /// Move a single line.
    Single,
    /// Move roughly half of a screen.
    Half,
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
        #[allow(clippy::indexing_slicing)] // EnumMap guarantees indexing will not panic.
        let output = self.map[self.mode].decode(input);

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
            Key::Char('w') => {
                output.add_op(Operation::Confirm(ConfirmAction::Quit));
                output.set_mode(Mode::Confirm);
            }
            Key::Char('s') => {
                output.add_op(Operation::Document(DocOp::Save));
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
            Input::User(user_input) => match user_input {
                ui::Input::Key { key, .. } => {
                    Self::decode_key(key, &mut output);
                }
                ui::Input::Resize(size) => {
                    output.add_op(Operation::Size(size));
                }
                ui::Input::Mouse => {}
            },
            Input::File { url, text } => {
                output.add_op(Operation::OpenDoc { url, text });
            }
            Input::Glitch(glitch) => {
                output.add_op(Operation::Alert(ShowMessageParams {
                    typ: MessageType::Error,
                    message: format!("{}", glitch),
                }));
            }
            Input::Config(setting) => {
                output.add_op(Operation::UpdateSetting(setting));
            }
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
            Input::User(user_input) => match user_input {
                ui::Input::Key {
                    key: Key::Char('y'),
                    ..
                } => {
                    output.add_op(Operation::Quit);
                }
                ui::Input::Key { .. } | ui::Input::Mouse | ui::Input::Resize { .. } => {
                    output.reset();
                }
            },
            Input::File { .. } | Input::Glitch(..) | Input::Config(..) => {}
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
            Input::User(user_input) => match user_input {
                ui::Input::Key { key: Key::Esc, .. } => {
                    output.reset();
                }
                ui::Input::Key {
                    key: Key::Enter, ..
                } => {
                    output.add_op(Operation::Execute);
                    output.set_mode(Mode::View);
                }
                ui::Input::Key {
                    key: Key::Char(c), ..
                } => {
                    output.add_op(Operation::Collect(c));
                }
                ui::Input::Key { .. } | ui::Input::Mouse | ui::Input::Resize { .. } => {}
            },
            Input::File { .. } | Input::Glitch(..) | Input::Config(..) => {}
        }

        output
    }
}

/// Testing of the translate module.
#[cfg(test)]
mod test {
    use super::*;
    use crate::io::{ui::Modifiers, Glitch, Setting};

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
                Some(Operation::Alert(ShowMessageParams {
                    typ: MessageType::Error,
                    message: "config file watcher disconnected".to_string(),
                }))
            );
            assert_eq!(int.mode, Mode::View);
        }

        /// A new setting shall be forwarded to the application.
        #[test]
        fn setting() {
            let mut int = view_mode();

            assert_eq!(
                int.translate(Input::Config(Setting::Wrap(true))),
                Some(Operation::UpdateSetting(Setting::Wrap(true)))
            );
            assert_eq!(int.mode, Mode::View);
        }

        /// The `Ctrl-w` key shall confirm the user wants to quit.
        #[test]
        fn quit() {
            let mut int = view_mode();

            assert_eq!(
                int.translate(Input::User(ui::Input::Key {
                    key: Key::Char('w'),
                    modifiers: Modifiers::CONTROL,
                })),
                Some(Operation::Confirm(ConfirmAction::Quit))
            );
            assert_eq!(int.mode, Mode::Confirm);
        }

        /// The `Ctrl-o` key shall request the name of the document to be opened.
        #[test]
        fn open() {
            let mut int = view_mode();

            assert_eq!(
                int.translate(Input::User(ui::Input::Key {
                    key: Key::Char('o'),
                    modifiers: Modifiers::CONTROL,
                })),
                Some(Operation::StartCommand(Command::Open))
            );
            assert_eq!(int.mode, Mode::Collect);
        }

        /// The `Ctrl-s` key shall save the document.
        #[test]
        fn save() {
            let mut int = view_mode();

            assert_eq!(
                int.translate(Input::User(ui::Input::Key {
                    key: Key::Char('s'),
                    modifiers: Modifiers::CONTROL,
                })),
                Some(Operation::Document(DocOp::Save))
            );
            assert_eq!(int.mode, Mode::View);
        }

        /// The 'j' key shall move the cursor down.
        #[test]
        fn move_down() {
            let mut int = view_mode();

            assert_eq!(
                int.translate(Input::User(ui::Input::Key {
                    key: Key::Char('j'),
                    modifiers: Modifiers::empty(),
                })),
                Some(Operation::Document(DocOp::Move(Vector::new(
                    Direction::Down,
                    Magnitude::Single
                ))))
            );
            assert_eq!(int.mode, Mode::View);
        }

        /// The 'k' key shall move the cursor up.
        #[test]
        fn move_up() {
            let mut int = view_mode();

            assert_eq!(
                int.translate(Input::User(ui::Input::Key {
                    key: Key::Char('k'),
                    modifiers: Modifiers::empty(),
                })),
                Some(Operation::Document(DocOp::Move(Vector::new(
                    Direction::Up,
                    Magnitude::Single
                ))))
            );
            assert_eq!(int.mode, Mode::View);
        }

        /// The 'J' key shall scroll the document down.
        #[test]
        fn scroll_down() {
            let mut int = view_mode();

            assert_eq!(
                int.translate(Input::User(ui::Input::Key {
                    key: Key::Char('J'),
                    modifiers: Modifiers::empty(),
                })),
                Some(Operation::Document(DocOp::Move(Vector::new(
                    Direction::Down,
                    Magnitude::Half
                ))))
            );
            assert_eq!(int.mode, Mode::View);
        }

        /// The 'K' key shall scroll the document up.
        #[test]
        fn scroll_up() {
            let mut int = view_mode();

            assert_eq!(
                int.translate(Input::User(ui::Input::Key {
                    key: Key::Char('K'),
                    modifiers: Modifiers::empty(),
                })),
                Some(Operation::Document(DocOp::Move(Vector::new(
                    Direction::Up,
                    Magnitude::Half
                ))))
            );
            assert_eq!(int.mode, Mode::View);
        }

        /// The 'd' key shall delete the current selection.
        #[test]
        fn delete() {
            let mut int = view_mode();

            assert_eq!(
                int.translate(Input::User(ui::Input::Key {
                    key: Key::Char('d'),
                    modifiers: Modifiers::empty(),
                })),
                Some(Operation::Document(DocOp::Delete))
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
                int.translate(Input::User(ui::Input::Key {
                    key: Key::Char('y'),
                    modifiers: Modifiers::empty(),
                })),
                Some(Operation::Quit)
            );
        }

        /// Any other key shall cancel the action, resetting the application to View mode.
        #[test]
        fn cancel() {
            let mut int = confirm_mode();

            assert_eq!(
                int.translate(Input::User(ui::Input::Key {
                    key: Key::Char('n'),
                    modifiers: Modifiers::empty(),
                })),
                Some(Operation::Reset)
            );
            assert_eq!(int.mode, Mode::View);

            int = confirm_mode();

            assert_eq!(
                int.translate(Input::User(ui::Input::Key {
                    key: Key::Char('1'),
                    modifiers: Modifiers::empty(),
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
                int.translate(Input::User(ui::Input::Key {
                    key: Key::Esc,
                    modifiers: Modifiers::empty(),
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
                int.translate(Input::User(ui::Input::Key {
                    key: Key::Char('a'),
                    modifiers: Modifiers::empty(),
                })),
                Some(Operation::Collect('a'))
            );
            assert_eq!(int.mode, Mode::Collect);

            int = collect_mode();

            assert_eq!(
                int.translate(Input::User(ui::Input::Key {
                    key: Key::Char('.'),
                    modifiers: Modifiers::empty(),
                })),
                Some(Operation::Collect('.'))
            );
            assert_eq!(int.mode, Mode::Collect);

            int = collect_mode();

            assert_eq!(
                int.translate(Input::User(ui::Input::Key {
                    key: Key::Char('1'),
                    modifiers: Modifiers::empty(),
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
                int.translate(Input::User(ui::Input::Key {
                    key: Key::Enter,
                    modifiers: Modifiers::empty(),
                })),
                Some(Operation::Execute)
            );
            assert_eq!(int.mode, Mode::View);
        }
    }
}

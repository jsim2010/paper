//! Implements the state machine of the application.

mod add_to_sketch;
mod change_mode;
mod draw_sketch;
mod execute_command;
mod filter_signals;
mod mark_at;
mod quit;
mod reduce_noise;
mod scroll;
mod update_view;

use crate::{
    error, fmt, some, storage, tkn, ui, Any, Debug, Display, Edge, Element, End, Formatter,
    HashMap, Paper, Pattern, TryFromIntError, BACKSPACE, ENTER,
};
use pancurses::Input;
use rec::ChCls::{Not, Whitespace};
use rec::{lazy_some, opt, var};
use std::io;
use std::mem::{discriminant, Discriminant};
use std::rc::Rc;
use ui::ESC;

/// Signifies a [`Result`] during the execution of an [`Operation`].
pub type Outcome<T> = Result<T, Failure>;
/// Signifies the final [`Outcome`] of an [`Operation`].
type Output = Outcome<Option<Notice>>;

#[derive(Debug, Default)]
pub(crate) struct Interpreter {
    mode: Mode,
}

impl Interpreter {
    /// Returns the [`Operation`]s to be executed based on the current [`Mode`].
    pub(crate) fn interpret(&self, input: Option<Input>) -> Vec<OpCode> {
        match (self.mode, input) {
            (_, Some(Input::KeyClose)) => vec![OpCode::Quit],
            (Mode::Display, Some(Input::Character(c))) => match c {
                '.' => vec![OpCode::ChangeMode(Mode::Command)],
                '#' | '/' => vec![OpCode::AddToSketch(c), OpCode::ChangeMode(Mode::Filter)],
                'j' => vec![OpCode::Scroll(Direction::Down)],
                'k' => vec![OpCode::Scroll(Direction::Up)],
                _ => Vec::with_capacity(0),
            },
            (Mode::Command, Some(Input::Character(c))) => match c {
                ENTER => vec![OpCode::ExecuteCommand, OpCode::ChangeMode(Mode::Display)],
                ESC => vec![OpCode::ChangeMode(Mode::Display)],
                _ => vec![OpCode::AddToSketch(c), OpCode::DrawSketch],
            },
            (Mode::Filter, Some(Input::Character(c))) => match c {
                ENTER => vec![OpCode::ChangeMode(Mode::Action)],
                '\t' => vec![
                    OpCode::ReduceNoise,
                    OpCode::AddToSketch('&'),
                    OpCode::AddToSketch('&'),
                    OpCode::DrawSketch,
                    OpCode::FilterSignals,
                ],
                ESC => vec![OpCode::ChangeMode(Mode::Display)],
                _ => vec![
                    OpCode::AddToSketch(c),
                    OpCode::DrawSketch,
                    OpCode::FilterSignals,
                ],
            },
            (Mode::Action, Some(Input::Character(c))) => match c {
                ESC => vec![OpCode::ChangeMode(Mode::Display)],
                'i' => vec![OpCode::MarkAt(Edge::Start), OpCode::ChangeMode(Mode::Edit)],
                'I' => vec![OpCode::MarkAt(Edge::End), OpCode::ChangeMode(Mode::Edit)],
                _ => Vec::with_capacity(0),
            },
            (Mode::Edit, Some(Input::Character(c))) => match c {
                ESC => vec![OpCode::ChangeMode(Mode::Display)],
                _ => vec![OpCode::UpdateView(c)],
            },
            (_, _) => Vec::with_capacity(0),
        }
    }

    /// Sets the current [`Mode`].
    pub(crate) fn set_mode(&mut self, mode: Mode) {
        self.mode = mode;
    }
}

/// Signifies a state of the application.
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
pub enum Mode {
    /// Displays the current view.
    Display,
    /// Displays the current command.
    Command,
    /// Displays the current filter expression and highlights the characters that match the filter.
    Filter,
    /// Displays the highlighting that has been selected.
    Action,
    /// Displays the current view along with the current edits.
    Edit,
}

impl Display for Mode {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Mode::Display => write!(f, "Display"),
            Mode::Command => write!(f, "Command"),
            Mode::Filter => write!(f, "Filter"),
            Mode::Action => write!(f, "Action"),
            Mode::Edit => write!(f, "Edit"),
        }
    }
}

impl Default for Mode {
    fn default() -> Self {
        Mode::Display
    }
}

/// Signifies a representation of an [`Operation`].
#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
pub enum OpCode {
    /// Executes [`change_mode::Op`].
    ChangeMode(Mode),
    /// Executes [`add_to_sketch::Op`].
    AddToSketch(char),
    /// Executes [`scroll::Op`].
    Scroll(Direction),
    /// Executes [`execute_command::Op`].
    ExecuteCommand,
    /// Executes [`draw_sketch::Op`].
    DrawSketch,
    /// Executes [`filter_signals::Op`].
    FilterSignals,
    /// Executes [`reduce_noise::Op`].
    ReduceNoise,
    /// Executes [`mark_at::Op`].
    MarkAt(Edge),
    /// Executes [`update_view::Op`].
    UpdateView(char),
    /// Executes [`quit::Op`].
    Quit,
}

impl OpCode {
    /// The [`Discriminant`] that signifies the `OpCode`.
    fn id(self) -> Discriminant<Self> {
        discriminant(&self)
    }
}

impl Display for OpCode {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            OpCode::ChangeMode(mode) => write!(f, "Change to mode {}", mode),
            OpCode::AddToSketch(c) => write!(f, "Add '{}' to sketch", c),
            OpCode::Scroll(d) => write!(f, "Scroll {}", d),
            OpCode::ExecuteCommand => write!(f, "Execute command"),
            OpCode::DrawSketch => write!(f, "Draw sketch"),
            OpCode::FilterSignals => write!(f, "Filter signals"),
            OpCode::ReduceNoise => write!(f, "Reduce noise"),
            OpCode::MarkAt(edge) => write!(f, "Mark at {}", edge),
            OpCode::UpdateView(c) => write!(f, "Update view with '{}'", c),
            OpCode::Quit => write!(f, "Quit"),
        }
    }
}

/// Stores all [`Operation`]s for the application.
#[derive(Debug)]
pub(crate) struct Operations {
    /// The map associating [`OpCode`]s with [`Operation`]s.
    ops: HashMap<Discriminant<OpCode>, Rc<dyn Operation>>,
}

impl Operations {
    /// Executes the [`Operation`] described by an [`OpCode`].
    pub(crate) fn operate(&self, paper: &mut Paper<'_>, opcode: OpCode) -> Output {
        self.ops.get(&opcode.id()).map_or(
            Err(Failure::InvalidOpCode {
                operation: String::from("N/A"),
                opcode,
            }),
            |x| x.operate(paper, opcode),
        )
    }
}

impl Default for Operations {
    fn default() -> Self {
        let add_to_sketch: Rc<dyn Operation> = Rc::new(add_to_sketch::Op);
        let change_mode: Rc<dyn Operation> = Rc::new(change_mode::Op);
        let draw_sketch: Rc<dyn Operation> = Rc::new(draw_sketch::Op);
        let execute_command: Rc<dyn Operation> = Rc::new(execute_command::Op::new());
        let filter_signals: Rc<dyn Operation> = Rc::new(filter_signals::Op::new());
        let mark_at: Rc<dyn Operation> = Rc::new(mark_at::Op);
        let quit: Rc<dyn Operation> = Rc::new(quit::Op);
        let reduce_noise: Rc<dyn Operation> = Rc::new(reduce_noise::Op);
        let scroll: Rc<dyn Operation> = Rc::new(scroll::Op);
        let update_view: Rc<dyn Operation> = Rc::new(update_view::Op);

        Self {
            ops: [
                (OpCode::AddToSketch(Default::default()).id(), add_to_sketch),
                (OpCode::ChangeMode(Mode::default()).id(), change_mode),
                (OpCode::DrawSketch.id(), draw_sketch),
                (OpCode::ExecuteCommand.id(), execute_command),
                (OpCode::FilterSignals.id(), filter_signals),
                (OpCode::MarkAt(Edge::default()).id(), mark_at),
                (OpCode::Quit.id(), quit),
                (OpCode::ReduceNoise.id(), reduce_noise),
                (OpCode::Scroll(Direction::default()).id(), scroll),
                (OpCode::UpdateView(Default::default()).id(), update_view),
            ]
            .iter()
            .cloned()
            .collect(),
        }
    }
}

/// Signifies functionality for the application to implement.
trait Operation: Debug {
    /// Returns the name of the `Operation`.
    fn name(&self) -> String;
    /// Executes the `Operation`.
    fn operate(&self, paper: &mut Paper<'_>, opcode: OpCode) -> Output;
}

/// Signifies an [`error::Error`] that occurs during the execution of an [`Operation`].
#[derive(Clone, Debug)]
pub enum Failure {
    /// An [`OpCode`] does not match what was expected.
    InvalidOpCode {
        /// The name of the [`Operation`] during which `Failure` occurred.
        operation: String,
        /// The [`OpCode`] that was received.
        opcode: OpCode,
    },
    /// An error occurred during the execution of a [`ui`] command.
    Ui(ui::Error),
    /// An attempt to convert one type to another was unsuccessful.
    Conversion(TryFromIntError),
    /// An error occurred during the execution of File command.
    File(storage::Error),
}

impl error::Error for Failure {
    fn source(&self) -> Option<&(dyn error::Error + 'static)> {
        match self {
            Failure::InvalidOpCode { .. } => None,
            Failure::Ui(error) => Some(error),
            Failure::Conversion(error) => Some(error),
            Failure::File(error) => Some(error),
        }
    }
}

impl Display for Failure {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Failure::InvalidOpCode {
                ref operation,
                ref opcode,
            } => write!(
                f,
                "Attempted to execute Operation '{}' with OpCode '{}'.",
                operation, opcode
            ),
            Failure::Ui(error) => write!(f, "{}", error),
            Failure::Conversion(error) => write!(f, "{}", error),
            Failure::File(error) => write!(f, "{}", error),
        }
    }
}

impl From<ui::Error> for Failure {
    fn from(error: ui::Error) -> Self {
        Failure::Ui(error)
    }
}

impl From<TryFromIntError> for Failure {
    fn from(error: TryFromIntError) -> Self {
        Failure::Conversion(error)
    }
}

impl From<io::Error> for Failure {
    fn from(error: io::Error) -> Self {
        Failure::File(storage::Error::from(error))
    }
}

/// Signifies a direction in which the application can scroll.
#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
pub enum Direction {
    /// Moves the window up, effectively moving the view down.
    Up,
    /// Moves the window down, effectively moving the view up.
    Down,
}

impl Default for Direction {
    fn default() -> Self {
        Direction::Up
    }
}

impl Display for Direction {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Direction::Up => write!(f, "Up"),
            Direction::Down => write!(f, "Down"),
        }
    }
}

/// Signifies an action requested by an [`Operation`].
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
pub(crate) enum Notice {
    /// Ends the application.
    Quit,
    /// Flashes the screen.
    ///
    /// Used as a brief indicator that the current input is having no effect.
    Flash,
}

impl Display for Notice {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Notice::Quit => write!(f, "Quit"),
            Notice::Flash => write!(f, "Flash"),
        }
    }
}

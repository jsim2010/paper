//! Implements the state machine of the application.
use crate::{
    error, io, fmt, some, tkn, ui, Any, Debug, Display, Edge, Element, End, Formatter, HashMap,
    IndexType, Paper, Pattern, TryFrom, TryFromIntError, BACKSPACE, ENTER,
};
use rec::ChCls::{Not, Whitespace};
use rec::{lazy_some, opt, var};
use std::mem::{discriminant, Discriminant};
use std::rc::Rc;
use ui::ESC;

/// Signifies a [`Result`] during the execution of an [`Operation`].
pub(crate) type Outcome<T> = Result<T, Failure>;
/// Signifies the final [`Outcome`] of an [`Operation`].
type Output = Outcome<Option<Notice>>;

/// Manages the functionality of the different [`Mode`]s.
#[derive(Debug, Default)]
pub(crate) struct Controller {
    /// The current [`Mode`].
    mode: Mode,
    /// The implementation of [`DisplayMode`].
    display: DisplayMode,
    /// The implementation of [`CommandMode`].
    command: CommandMode,
    /// The implementation of [`FilterMode`].
    filter: FilterMode,
    /// The implementation of [`ActionMode`].
    action: ActionMode,
    /// The implementation of [`EditMode`].
    edit: EditMode,
}

impl Controller {
    /// Returns the [`Operation`]s to be executed based on the current [`Mode`].
    pub(crate) fn process_input(&self, input: Option<char>) -> Vec<OpCode> {
        if let Some(c) = input {
            return self.mode().process_input(c);
        }

        Vec::with_capacity(0)
    }

    /// Sets the current [`Mode`].
    pub(crate) fn set_mode(&mut self, mode: Mode) {
        self.mode = mode;
    }

    /// Returns the [`ModeHandler`] of the current [`Mode`].
    fn mode(&self) -> &dyn ModeHandler {
        match self.mode {
            Mode::Display => &self.display,
            Mode::Command => &self.command,
            Mode::Filter => &self.filter,
            Mode::Action => &self.action,
            Mode::Edit => &self.edit,
        }
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
    /// Executes [`ChangeModeOperation`].
    ChangeMode(Mode),
    /// Executes [`AddToSketchOperation`].
    AddToSketch(char),
    /// Executes [`ScrollOperation`].
    Scroll(Direction),
    /// Executes [`ExecuteCommandOperation`].
    ExecuteCommand,
    /// Executes [`DrawSketchOperation`].
    DrawSketch,
    /// Executes [`FilterSignalsOperation`].
    FilterSignals,
    /// Executes [`ReduceNoiseOperation`].
    ReduceNoise,
    /// Executes [`MarkAtOperation`].
    MarkAt(Edge),
    /// Executes [`UpdateViewOperation`].
    UpdateView(char),
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
        }
    }
}

/// Defines the functionality implemented while the application is in a [`Mode`].
trait ModeHandler {
    /// Returns the [`Operation`]s appropriate for an input.
    fn process_input(&self, input: char) -> Vec<OpCode>;
}

/// Implements the functionality while the application is in [`Mode::Display`].
#[derive(Debug, Default)]
struct DisplayMode;

impl ModeHandler for DisplayMode {
    fn process_input(&self, input: char) -> Vec<OpCode> {
        match input {
            '.' => vec![OpCode::ChangeMode(Mode::Command)],
            '#' | '/' => vec![OpCode::AddToSketch(input), OpCode::ChangeMode(Mode::Filter)],
            'j' => vec![OpCode::Scroll(Direction::Down)],
            'k' => vec![OpCode::Scroll(Direction::Up)],
            _ => Vec::with_capacity(0),
        }
    }
}

/// Implements the functionality while the application is in [`Mode::Command`].
#[derive(Debug, Default)]
struct CommandMode;

impl ModeHandler for CommandMode {
    fn process_input(&self, input: char) -> Vec<OpCode> {
        match input {
            ENTER => vec![OpCode::ExecuteCommand, OpCode::ChangeMode(Mode::Display)],
            ESC => vec![OpCode::ChangeMode(Mode::Display)],
            _ => vec![OpCode::AddToSketch(input), OpCode::DrawSketch],
        }
    }
}

/// Implements the functionality while the application is in [`Mode::Filter`].
#[derive(Debug, Default)]
struct FilterMode;

impl ModeHandler for FilterMode {
    fn process_input(&self, input: char) -> Vec<OpCode> {
        match input {
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
                OpCode::AddToSketch(input),
                OpCode::DrawSketch,
                OpCode::FilterSignals,
            ],
        }
    }
}

/// Implements the functionality while the application is in [`Mode::Action`].
#[derive(Debug, Default)]
struct ActionMode;

impl ModeHandler for ActionMode {
    fn process_input(&self, input: char) -> Vec<OpCode> {
        match input {
            ESC => vec![OpCode::ChangeMode(Mode::Display)],
            'i' => vec![OpCode::MarkAt(Edge::Start), OpCode::ChangeMode(Mode::Edit)],
            'I' => vec![OpCode::MarkAt(Edge::End), OpCode::ChangeMode(Mode::Edit)],
            _ => Vec::with_capacity(0),
        }
    }
}

/// Implements the functionality while the application in in [`Mode::Edit`].
#[derive(Debug, Default)]
struct EditMode;

impl ModeHandler for EditMode {
    fn process_input(&self, input: char) -> Vec<OpCode> {
        match input {
            ESC => vec![OpCode::ChangeMode(Mode::Display)],
            _ => vec![OpCode::UpdateView(input)],
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
    pub(crate) fn operate(&self, paper: &mut Paper, opcode: OpCode) -> Output {
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
        let change_mode: Rc<dyn Operation> = Rc::new(ChangeModeOperation);
        let add_to_sketch: Rc<dyn Operation> = Rc::new(AddToSketchOperation);
        let scroll: Rc<dyn Operation> = Rc::new(ScrollOperation);
        let execute_command: Rc<dyn Operation> = Rc::new(ExecuteCommandOperation::new());
        let draw_sketch: Rc<dyn Operation> = Rc::new(DrawSketchOperation);
        let filter_signals: Rc<dyn Operation> = Rc::new(FilterSignalsOperation::new());
        let reduce_noise: Rc<dyn Operation> = Rc::new(ReduceNoiseOperation);
        let mark_at: Rc<dyn Operation> = Rc::new(MarkAtOperation);
        let update_view: Rc<dyn Operation> = Rc::new(UpdateViewOperation);

        Self {
            ops: [
                (OpCode::ChangeMode(Mode::default()).id(), change_mode),
                (OpCode::AddToSketch(Default::default()).id(), add_to_sketch),
                (OpCode::Scroll(Direction::default()).id(), scroll),
                (OpCode::ExecuteCommand.id(), execute_command),
                (OpCode::DrawSketch.id(), draw_sketch),
                (OpCode::FilterSignals.id(), filter_signals),
                (OpCode::ReduceNoise.id(), reduce_noise),
                (OpCode::MarkAt(Edge::default()).id(), mark_at),
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
    /// Executes the `Operation`.
    fn operate(&self, paper: &mut Paper, opcode: OpCode) -> Output;
}

/// Changes the [`Mode`] of the application.
#[derive(Clone, Debug)]
struct ChangeModeOperation;

impl ChangeModeOperation {
    /// Returns the [`Mode`] stored in the [`OpCode`].
    fn arg(&self, opcode: OpCode) -> Outcome<Mode> {
        if let OpCode::ChangeMode(mode) = opcode {
            Ok(mode)
        } else {
            Err(Failure::InvalidOpCode {
                operation: String::from("ChangeMode"),
                opcode,
            })
        }
    }
}

impl Operation for ChangeModeOperation {
    fn operate(&self, paper: &mut Paper, opcode: OpCode) -> Output {
        let mode = self.arg(opcode)?;

        match mode {
            Mode::Display => {
                paper.sketch_mut().clear();
                paper.display_view()?;
            }
            Mode::Command | Mode::Filter => {
                paper.draw_sketch()?;
            }
            Mode::Action => {}
            Mode::Edit => {
                paper.display_view()?;
            }
        }

        paper.change_mode(mode);
        Ok(None)
    }
}

/// Adds a character to the sketch.
#[derive(Clone, Debug)]
struct AddToSketchOperation;

impl AddToSketchOperation {
    /// Returns the [`char`] stored in the [`OpCode`].
    fn arg(&self, opcode: OpCode) -> Outcome<char> {
        if let OpCode::AddToSketch(c) = opcode {
            Ok(c)
        } else {
            Err(Failure::InvalidOpCode {
                operation: String::from("AddToSketch"),
                opcode,
            })
        }
    }
}

impl Operation for AddToSketchOperation {
    fn operate(&self, paper: &mut Paper, opcode: OpCode) -> Output {
        match self.arg(opcode)? {
            BACKSPACE => {
                if paper.sketch_mut().pop().is_none() {
                    return Ok(Some(Notice::Flash));
                }
            }
            c => {
                paper.sketch_mut().push(c);
            }
        }

        Ok(None)
    }
}

/// Sets signals to match filters described in the sketch.
#[derive(Debug)]
struct FilterSignalsOperation {
    /// The [`Pattern`] that matches the first feature.
    first_feature_pattern: Pattern,
}

impl FilterSignalsOperation {
    /// Creates a new `FilterSignalsOperation`.
    fn new() -> Self {
        Self {
            first_feature_pattern: Pattern::define(tkn!(var(Not("&")) => "feature") + opt("&&")),
        }
    }
}

impl Operation for FilterSignalsOperation {
    fn operate(&self, paper: &mut Paper, _opcode: OpCode) -> Output {
        let filter = paper.sketch().clone();

        if let Some(last_feature) = self
            .first_feature_pattern
            .tokenize_iter(&filter)
            .last()
            .and_then(|x| x.get("feature"))
        {
            paper.filter_signals(last_feature)?;
        }

        paper.clear_background()?;
        paper.draw_filter_backgrounds()?;
        Ok(None)
    }
}

/// Sets the location of [`Mark`]s at an [`Edge`] of every signal.
#[derive(Debug)]
struct MarkAtOperation;

impl MarkAtOperation {
    /// Returns the [`Edge`] stored in the [`OpCode`].
    fn arg(&self, opcode: OpCode) -> Outcome<Edge> {
        if let OpCode::MarkAt(edge) = opcode {
            Ok(edge)
        } else {
            Err(Failure::InvalidOpCode {
                operation: String::from("MarkAt"),
                opcode,
            })
        }
    }
}

impl Operation for MarkAtOperation {
    fn operate(&self, paper: &mut Paper, opcode: OpCode) -> Output {
        paper.set_marks(self.arg(opcode)?);
        Ok(None)
    }
}

/// Executes the command stored in sketch.
#[derive(Debug)]
struct ExecuteCommandOperation {
    /// The [`Pattern`] that matches the name of a command.
    command_pattern: Pattern,
    /// The [`Pattern`] that matches the `see <path>` command.
    see_pattern: Pattern,
}

impl ExecuteCommandOperation {
    /// Creates a new `ExecuteCommandOperation`.
    fn new() -> Self {
        Self {
            command_pattern: Pattern::define(
                tkn!(lazy_some(Any) => "command")
                    + (End | (some(Whitespace) + tkn!(var(Any) => "args"))),
            ),
            see_pattern: Pattern::define("see" + some(Whitespace) + tkn!(var(Any) => "path")),
        }
    }
}

impl Operation for ExecuteCommandOperation {
    fn operate(&self, paper: &mut Paper, _opcode: OpCode) -> Output {
        let command = paper.sketch().clone();
        let command_tokens = self.command_pattern.tokenize(&command);

        match command_tokens.get("command") {
            Some("see") => {
                if let Some(path) = command_tokens.get("args") {
                    paper.change_view(path)?;
                }
            }
            Some("put") => {
                paper.save_view()?;
            }
            Some("end") => return Ok(Some(Notice::Quit)),
            Some(_) | None => {}
        }

        Ok(None)
    }
}

/// Draws the current sketch.
#[derive(Debug)]
struct DrawSketchOperation;

impl Operation for DrawSketchOperation {
    fn operate(&self, paper: &mut Paper, _opcode: OpCode) -> Output {
        paper.draw_sketch()?;
        Ok(None)
    }
}

/// Sets the noise equal to the signals that match the current filter.
#[derive(Debug)]
struct ReduceNoiseOperation;

impl Operation for ReduceNoiseOperation {
    fn operate(&self, paper: &mut Paper, _opcode: OpCode) -> Output {
        paper.reduce_noise();
        Ok(None)
    }
}

/// Updates the view with a given character.
#[derive(Debug)]
struct UpdateViewOperation;

impl UpdateViewOperation {
    /// Returns the [`char`] stored in the [`OpCode`].
    fn arg(&self, opcode: OpCode) -> Outcome<char> {
        if let OpCode::UpdateView(c) = opcode {
            Ok(c)
        } else {
            Err(Failure::InvalidOpCode {
                operation: String::from("UpdateView"),
                opcode,
            })
        }
    }
}

impl Operation for UpdateViewOperation {
    fn operate(&self, paper: &mut Paper, opcode: OpCode) -> Output {
        paper.update_view(self.arg(opcode)?)?;
        Ok(None)
    }
}

/// Changes the part of the view that is visible.
#[derive(Debug)]
struct ScrollOperation;

impl ScrollOperation {
    /// Returns the [`Direction`] stored in the [`OpCode`].
    fn arg(&self, opcode: OpCode) -> Outcome<Direction> {
        if let OpCode::Scroll(direction) = opcode {
            Ok(direction)
        } else {
            Err(Failure::InvalidOpCode {
                operation: String::from("Scroll"),
                opcode,
            })
        }
    }

    /// Returns the number of rows to scroll.
    ///
    /// A positive number signifies scrolling up while a negative signifies scrolling down.
    fn get_scroll_movement(height: usize, direction: Direction) -> Outcome<IndexType> {
        let movement = IndexType::try_from(height)?;

        if let Direction::Up = direction {
            movement
                .checked_neg()
                .ok_or(Failure::Conversion(TryFromIntError::Overflow))
        } else {
            Ok(movement)
        }
    }
}

impl Operation for ScrollOperation {
    fn operate(&self, paper: &mut Paper, opcode: OpCode) -> Output {
        paper.scroll(Self::get_scroll_movement(
            paper.scroll_height(),
            self.arg(opcode)?,
        )?)?;
        paper.display_view()?;
        Ok(None)
    }
}

/// Signifies an [`error::Error`] that occurs during the execution of an [`Operation`].
#[derive(Debug)]
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
    Io(io::Error),
}

impl error::Error for Failure {
    fn source(&self) -> Option<&(dyn error::Error + 'static)> {
        match self {
            Failure::InvalidOpCode { .. } => None,
            Failure::Ui(error) => Some(error),
            Failure::Conversion(error) => Some(error),
            Failure::Io(error) => Some(error),
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
            Failure::Io(error) => write!(f, "{}", error),
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
        Failure::Io(error)
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

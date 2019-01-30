//! Implements the state machine of the application.
use crate::ui::{BACKSPACE, ENTER, ESC, Fault};
use crate::{Debug, Display, Edge, FmtResult, Formatter, Paper};
use rec::{Element, tkn, opt, lazy_some, some, var, Pattern};
use rec::ChCls::{Any, End, Not, Whitespace};
use std::collections::HashMap;
use std::error::Error;
use std::mem::{discriminant, Discriminant};
use std::rc::Rc;
use try_from::{TryFrom, TryFromIntError};

/// Signifies the result of executing an [`Operation`].
pub(crate) type Outcome = Result<Option<Notice>, Failure>;

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
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
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

#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
pub enum OpCode {
    ChangeMode(Mode),
    AddToSketch(char),
    Scroll(Direction),
    ExecuteCommand,
    DrawPopup,
    FilterSignals,
    ReduceNoise,
    MarkAt(Edge),
    UpdateView(char),
}

impl OpCode {
    fn id(self) -> Discriminant<Self> {
        discriminant(&self)
    }
}

impl Display for OpCode {
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        match self {
            OpCode::ChangeMode(mode) => write!(f, "Change to mode {}", mode),
            OpCode::AddToSketch(c) => write!(f, "Add '{}' to sketch", c),
            OpCode::Scroll(d) => write!(f, "Scroll {}", d),
            OpCode::ExecuteCommand => write!(f, "Execute command"),
            OpCode::DrawPopup => write!(f, "Draw popup"),
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
            _ => vec![OpCode::AddToSketch(input), OpCode::DrawPopup],
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
                OpCode::DrawPopup,
                OpCode::FilterSignals,
            ],
            ESC => vec![OpCode::ChangeMode(Mode::Display)],
            _ => vec![
                OpCode::AddToSketch(input),
                OpCode::DrawPopup,
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

#[derive(Debug)]
pub(crate) struct Operations {
    ops: HashMap<Discriminant<OpCode>, Rc<dyn Operation>>,
}

impl Operations {
    pub(crate) fn execute(&self, paper: &mut Paper, opcode: OpCode) -> Outcome {
        self.ops
            .get(&opcode.id())
            .map_or(Err(Failure::InvalidOpCode{operation: String::from("N/A"), opcode}), |x| {
                x.operate(paper, opcode)
            })
    }
}

impl Default for Operations {
    fn default() -> Self {
        let change_mode: Rc<dyn Operation> = Rc::new(ChangeMode);
        let add_to_sketch: Rc<dyn Operation> = Rc::new(AddToSketch);
        let scroll: Rc<dyn Operation> = Rc::new(Scroll);
        let execute_command: Rc<dyn Operation> = Rc::new(ExecuteCommand::new());
        let draw_popup: Rc<dyn Operation> = Rc::new(DrawPopup);
        let filter_signals: Rc<dyn Operation> = Rc::new(FilterSignals::new());
        let reduce_noise: Rc<dyn Operation> = Rc::new(ReduceNoise);
        let mark_at: Rc<dyn Operation> = Rc::new(MarkAt);
        let update_view: Rc<dyn Operation> = Rc::new(UpdateView);

        Self {
            ops: [
                (OpCode::ChangeMode(Mode::default()).id(), change_mode),
                (OpCode::AddToSketch(Default::default()).id(), add_to_sketch),
                (OpCode::Scroll(Direction::default()).id(), scroll),
                (OpCode::ExecuteCommand.id(), execute_command),
                (OpCode::DrawPopup.id(), draw_popup),
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

trait Operation: Debug {
    fn operate(&self, paper: &mut Paper, opcode: OpCode) -> Outcome;
}

#[derive(Clone, Debug)]
struct ChangeMode;

impl ChangeMode {
    fn arg(&self, opcode: OpCode) -> Result<Mode, Failure> {
        if let OpCode::ChangeMode(mode) = opcode {
            Ok(mode)
        } else {
            Err(Failure::InvalidOpCode{operation: String::from("ChangeMode"), opcode})
        }
    }
}

impl Operation for ChangeMode {
    fn operate(&self, paper: &mut Paper, opcode: OpCode) -> Outcome {
        let mode = self.arg(opcode)?;

        match mode {
            Mode::Display => {
                paper.sketch_mut().clear();
                paper.display_view()?;
            }
            Mode::Command | Mode::Filter => {
                paper.draw_popup()?;
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

#[derive(Clone, Debug)]
struct AddToSketch;

impl AddToSketch {
    fn arg(&self, opcode: OpCode) -> Result<char, Failure> {
        if let OpCode::AddToSketch(c) = opcode {
            Ok(c)
        } else {
            Err(Failure::InvalidOpCode{operation: String::from("AddToSketch"), opcode})
        }
    }
}

impl Operation for AddToSketch {
    fn operate(&self, paper: &mut Paper, opcode: OpCode) -> Outcome {
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

#[derive(Debug)]
struct FilterSignals {
    first_feature_pattern: Pattern,
}

impl FilterSignals {
    fn new() -> Self {
        Self {
            first_feature_pattern: Pattern::define(
                tkn!(var(Not("&")) => "feature") + opt("&&"),
            ),
        }
    }
}

impl Operation for FilterSignals {
    fn operate(&self, paper: &mut Paper, _opcode: OpCode) -> Outcome {
        let filter = paper.sketch().clone();

        if let Some(last_feature) = self
            .first_feature_pattern
            .tokenize_iter(&filter)
            .last()
            .and_then(|x| x.get("feature"))
        {
            paper.filter_signals(last_feature);
        }

        paper.clear_background()?;
        paper.draw_filter_backgrounds()?;
        Ok(None)
    }
}

#[derive(Debug)]
struct MarkAt;

impl MarkAt {
    fn arg(&self, opcode: OpCode) -> Result<Edge, Failure> {
        if let OpCode::MarkAt(edge) = opcode {
            Ok(edge)
        } else {
            Err(Failure::InvalidOpCode{operation: String::from("MarkAt"), opcode})
        }
    }
}

impl Operation for MarkAt {
    fn operate(&self, paper: &mut Paper, opcode: OpCode) -> Outcome {
        paper.set_marks(self.arg(opcode)?);
        Ok(None)
    }
}

#[derive(Debug)]
struct ExecuteCommand {
    command_pattern: Pattern,
    see_pattern: Pattern,
}

impl ExecuteCommand {
    fn new() -> Self {
        Self {
            command_pattern: Pattern::define(
                tkn!(lazy_some(Any) => "command") + (Whitespace | End),
            ),
            see_pattern: Pattern::define(
                "see" + some(Whitespace) + tkn!(var(Any) => "path")
            ),
        }
    }
}

impl Operation for ExecuteCommand {
    fn operate(&self, paper: &mut Paper, _opcode: OpCode) -> Outcome {
        let command = paper.sketch().clone();

        match self.command_pattern.tokenize(&command).get("command") {
            Some("see") => {
                if let Some(path) = self.see_pattern.tokenize(&command).get("path") {
                    paper.change_view(path);
                }
            }
            Some("put") => {
                paper.save_view();
            }
            Some("end") => return Ok(Some(Notice::Quit)),
            Some(_) | None => {}
        }

        Ok(None)
    }
}

#[derive(Debug)]
struct DrawPopup;

impl Operation for DrawPopup {
    fn operate(&self, paper: &mut Paper, _opcode: OpCode) -> Outcome {
        paper.draw_popup()?;
        Ok(None)
    }
}

#[derive(Debug)]
struct ReduceNoise;

impl Operation for ReduceNoise {
    fn operate(&self, paper: &mut Paper, _opcode: OpCode) -> Outcome {
        paper.reduce_noise();
        Ok(None)
    }
}

#[derive(Debug)]
struct UpdateView;

impl UpdateView {
    fn arg(&self, opcode: OpCode) -> Result<char, Failure> {
        if let OpCode::UpdateView(c) = opcode {
            Ok(c)
        } else {
            Err(Failure::InvalidOpCode{operation: String::from("UpdateView"), opcode})
        }
    }
}

impl Operation for UpdateView {
    fn operate(&self, paper: &mut Paper, opcode: OpCode) -> Outcome {
        paper.update_view(self.arg(opcode)?)?;
        Ok(None)
    }
}

#[derive(Debug)]
struct Scroll;

impl Scroll {
    fn arg(&self, opcode: OpCode) -> Result<Direction, Failure> {
        if let OpCode::Scroll(direction) = opcode {
            Ok(direction)
        } else {
            Err(Failure::InvalidOpCode{operation: String::from("Scroll"), opcode})
        }
    }

    #[allow(clippy::integer_arithmetic)]
    fn get_scroll_movement(height: usize, direction: Direction) -> Result<isize, Failure> {
        let mut movement = isize::try_from(height)?;

        if let Direction::Up = direction {
            movement = -movement;
        }

        Ok(movement)
    }
}

impl Operation for Scroll {
    fn operate(&self, paper: &mut Paper, opcode: OpCode) -> Result<Option<Notice>, Failure> {
        paper.scroll(Self::get_scroll_movement(
            paper.scroll_height(),
            self.arg(opcode)?,
        )?);
        paper.display_view()?;
        Ok(None)
    }
}

#[derive(Debug)]
pub enum Failure {
    InvalidOpCode { operation: String, opcode: OpCode },
    Ui(Fault),
    Conversion(TryFromIntError),
}

impl Error for Failure {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Failure::InvalidOpCode {..} => None,
            Failure::Ui(error) => Some(error),
            Failure::Conversion(error) => Some(error),
        }
    }
}

impl Display for Failure {
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        match self {
            Failure::InvalidOpCode {ref operation, ref opcode} => write!(f, "Attempted to execute Operation '{}' with OpCode '{}'.", operation, opcode),
            Failure::Ui(error) => write!(f, "{}", error),
            Failure::Conversion(error) => write!(f, "{}", error),
        }
    }
}

impl From<Fault> for Failure {
    fn from(error: Fault) -> Self {
        Failure::Ui(error)
    }
}

impl From<TryFromIntError> for Failure {
    fn from(error: TryFromIntError) -> Self {
        Failure::Conversion(error)
    }
}

#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
pub enum Direction {
    Up,
    Down,
}

impl Default for Direction {
    fn default() -> Self {
        Direction::Up
    }
}

impl Display for Direction {
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
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
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        match self {
            Notice::Quit => write!(f, "Quit"),
            Notice::Flash => write!(f, "Flash"),
        }
    }
}

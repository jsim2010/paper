//! Implements the state machine of the application.
use crate::{Edge, Paper};
use crate::ui::{ENTER, ESC};
use rec::{Atom, ChCls, Pattern, Quantifier, OPT, SOME, VAR};
use std::fmt::{Debug, Display, Formatter, Result as FmtResult};
use std::rc::Rc;

/// Signifies the result of executing an [`Operation`].
type Outcome = Result<Option<Notice>, String>;

/// Manages the functionality of the different [`Mode`]s.
#[derive(Debug)]
pub(crate) struct Controller {
    /// The current [`Mode`].
    mode: Mode,
    /// The implementation of [`DisplayMode`].
    display: Rc<dyn ModeHandler>,
    /// The implementation of [`CommandMode`].
    command: Rc<dyn ModeHandler>,
    /// The implementation of [`FilterMode`].
    filter: Rc<dyn ModeHandler>,
    /// The implementation of [`ActionMode`].
    action: Rc<dyn ModeHandler>,
    /// The implementation of [`EditMode`].
    edit: Rc<dyn ModeHandler>,
}

impl Default for Controller {
    fn default() -> Controller {
        Controller {
            mode: Default::default(),
            display: Rc::new(DisplayMode::new()),
            command: Rc::new(CommandMode::new()),
            filter: Rc::new(FilterMode::new()),
            action: Rc::new(ActionMode),
            edit: Rc::new(EditMode),
        }
    }
}

impl Controller {
    /// Returns the [`Operation`]s to be executed based on the current [`Mode`].
    pub(crate) fn process_input(&self, input: Option<char>) -> Vec<Rc<dyn Operation>> {
        if let Some(c) = input {
            return self.mode().process_input(c);
        }

        Vec::new()
    }

    /// Sets the current [`Mode`].
    pub(crate) fn set_mode(&mut self, mode: Mode) {
        self.mode = mode;
    }

    /// Returns the [`ModeHandler`] of the current [`Mode`].
    fn mode(&self) -> Rc<dyn ModeHandler> {
        Rc::clone(match self.mode {
            Mode::Display => &self.display,
            Mode::Command => &self.command,
            Mode::Filter => &self.filter,
            Mode::Action => &self.action,
            Mode::Edit => &self.edit,
        })
    }
}

/// Signifies a state of the application.
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
pub(crate) enum Mode {
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
        write!(f, "{:?}", self)
    }
}

impl Default for Mode {
    fn default() -> Mode {
        Mode::Display
    }
}

/// Defines the functionality implemented while the application is in a [`Mode`].
trait ModeHandler: Debug {
    /// Returns the [`Operation`]s appropriate for an input.
    fn process_input(&self, input: char) -> Vec<Rc<dyn Operation>>;
}

/// Implements the functionality while the application is in [`Mode::Display`].
#[derive(Debug)]
struct DisplayMode {
    change_to_command: Rc<dyn Operation>,
    change_to_filter: Rc<dyn Operation>,
    scroll_down: Rc<dyn Operation>,
    scroll_up: Rc<dyn Operation>,
}

impl DisplayMode {
    fn new() -> DisplayMode {
        DisplayMode {
            change_to_command: Rc::new(ChangeMode(Mode::Command)),
            change_to_filter: Rc::new(ChangeMode(Mode::Filter)),
            scroll_down: Rc::new(Scroll(Direction::Down)),
            scroll_up: Rc::new(Scroll(Direction::Up)),
        }
    }
}

impl ModeHandler for DisplayMode {
    fn process_input(&self, input: char) -> Vec<Rc<dyn Operation>> {
        match input {
            '.' => vec![Rc::clone(&self.change_to_command)],
            '#' | '/' => vec![
                Rc::new(AddToSketch(input.to_string())),
                Rc::clone(&self.change_to_filter),
            ],
            'j' => vec![Rc::clone(&self.scroll_down)],
            'k' => vec![Rc::clone(&self.scroll_up)],
            _ => Vec::new(),
        }
    }
}

/// Implements the functionality while the application is in [`Mode::Command`].
#[derive(Debug)]
struct CommandMode {
    execute_command: Rc<dyn Operation>,
    change_to_display: Rc<dyn Operation>,
    draw_popup: Rc<dyn Operation>,
}

impl CommandMode {
    /// Creates a new `CommandMode`.
    fn new() -> CommandMode {
        CommandMode {
            execute_command: Rc::new(ExecuteCommand::new()),
            change_to_display: Rc::new(ChangeMode(Mode::Display)),
            draw_popup: Rc::new(DrawPopup),
        }
    }
}

impl ModeHandler for CommandMode {
    fn process_input(&self, input: char) -> Vec<Rc<dyn Operation>> {
        match input {
            ENTER => vec![
                Rc::clone(&self.execute_command),
                Rc::clone(&self.change_to_display),
            ],
            ESC => vec![Rc::clone(&self.change_to_display)],
            _ => vec![Rc::new(AddToSketch(input.to_string())), Rc::clone(&self.draw_popup)],
        }
    }
}

/// Implements the functionality while the application is in [`Mode::Filter`].
#[derive(Debug)]
struct FilterMode {
    draw_popup: Rc<dyn Operation>,
    filter_signals: Rc<dyn Operation>,
}

impl FilterMode {
    fn new() -> FilterMode {
        FilterMode {
            draw_popup: Rc::new(DrawPopup),
            filter_signals: Rc::new(FilterSignals::new()),
        }
    }
}

impl ModeHandler for FilterMode {
    fn process_input(&self, input: char) -> Vec<Rc<dyn Operation>> {
        match input {
            ENTER => vec![Rc::new(ChangeMode(Mode::Action))],
            '\t' => vec![
                Rc::new(ReduceNoise),
                Rc::new(AddToSketch(String::from("&&"))),
                Rc::clone(&self.draw_popup),
                Rc::clone(&self.filter_signals),
            ],
            ESC => vec![Rc::new(ChangeMode(Mode::Display))],
            _ => vec![Rc::new(AddToSketch(input.to_string())), Rc::clone(&self.draw_popup), Rc::clone(&self.filter_signals)],
        }
    }
}

/// Implements the functionality while the application is in [`Mode::Action`].
#[derive(Debug)]
struct ActionMode;

impl ModeHandler for ActionMode {
    fn process_input(&self, input: char) -> Vec<Rc<dyn Operation>> {
        match input {
            ESC => vec![Rc::new(ChangeMode(Mode::Display))],
            'i' => vec![
                Rc::new(MarkAt(Edge::Start)),
                Rc::new(ChangeMode(Mode::Edit)),
            ],
            'I' => vec![Rc::new(MarkAt(Edge::End)), Rc::new(ChangeMode(Mode::Edit))],
            _ => Vec::new(),
        }
    }
}

/// Implements the functionality while the application in in [`Mode::Edit`].
#[derive(Debug)]
struct EditMode;

impl ModeHandler for EditMode {
    fn process_input(&self, input: char) -> Vec<Rc<dyn Operation>> {
        match input {
            ESC => vec![Rc::new(ChangeMode(Mode::Display))],
            _ => vec![Rc::new(UpdateView(input))],
        }
    }
}

/// Signifies a command that [`Paper`] can execute.
pub(crate) trait Operation: Debug {
    /// Executes the command.
    fn operate(&self, paper: &mut Paper) -> Outcome;
}

#[derive(Debug)]
struct ChangeMode(Mode);

impl Operation for ChangeMode {
    fn operate(&self, paper: &mut Paper) -> Outcome {
        match self.0 {
            Mode::Display => {
                paper.reset_sketch();
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

        paper.change_mode(self.0);
        Ok(None)
    }
}

#[derive(Debug)]
struct FilterSignals {
    first_feature_pattern: Pattern,
}

impl FilterSignals {
    fn new() -> FilterSignals {
        FilterSignals {
            first_feature_pattern: Pattern::define(
                ChCls::None("&").rpt(VAR).name("feature") + "&&".rpt(OPT),
            ),
        }
    }
}

impl Operation for FilterSignals {
    fn operate(&self, paper: &mut Paper) -> Outcome {
        let filter = paper.sketch().clone();

        if let Some(last_feature) = self.first_feature_pattern.tokenize_iter(&filter).last().and_then(|x| x.get("feature")) {
            paper.filter_signals(last_feature);
        }

        paper.clear_background()?;
        paper.draw_filter_backgrounds()?;
        Ok(None)
    }
}

#[derive(Debug)]
struct MarkAt(Edge);

impl Operation for MarkAt {
    fn operate(&self, paper: &mut Paper) -> Outcome {
        paper.set_marks(&self.0);
        Ok(None)
    }
}

#[derive(Debug)]
struct ExecuteCommand {
    command_pattern: Pattern,
    see_pattern: Pattern,
}

impl ExecuteCommand {
    fn new() -> ExecuteCommand {
        ExecuteCommand {
            command_pattern: Pattern::define(
                ChCls::Any.rpt(SOME.lazy()).name("command") + (ChCls::WhSpc | ChCls::End),
            ),
            see_pattern: Pattern::define(
                "see" + ChCls::WhSpc.rpt(SOME) + ChCls::Any.rpt(VAR).name("path"),
            ),
        }
    }
}

impl Operation for ExecuteCommand {
    fn operate(&self, paper: &mut Paper) -> Outcome {
        let command = paper.sketch().clone();

        match self.command_pattern.tokenize(&command).get("command") {
            Some("see") => match self.see_pattern.tokenize(&command).get("path") {
                Some(path) => paper.change_view(path),
                None => {}
            },
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
    fn operate(&self, paper: &mut Paper) -> Outcome {
        paper.draw_popup()?;
        Ok(None)
    }
}

#[derive(Debug)]
struct ReduceNoise;

impl Operation for ReduceNoise {
    fn operate(&self, paper: &mut Paper) -> Outcome {
        paper.reduce_noise();
        Ok(None)
    }
}

#[derive(Debug)]
struct AddToSketch(String);

impl Operation for AddToSketch {
    fn operate(&self, paper: &mut Paper) -> Outcome {
        if !paper.add_to_sketch(&self.0) {
            return Ok(Some(Notice::Flash));
        }

        Ok(None)
    }
}

#[derive(Debug)]
struct UpdateView(char);

impl Operation for UpdateView {
    fn operate(&self, paper: &mut Paper) -> Outcome {
        paper.update_view(self.0)?;
        Ok(None)
    }
}

#[derive(Debug)]
struct Scroll(Direction);

impl Operation for Scroll {
    fn operate(&self, paper: &mut Paper) -> Outcome {
        let height = paper.scroll_height() as isize;

        match self.0 {
            Direction::Up => paper.scroll(-height),
            Direction::Down => paper.scroll(height),
        }

        paper.display_view()?;
        Ok(None)
    }
}

#[derive(Debug)]
enum Direction {
    Up,
    Down,
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
        write!(f, "{:?}", self)
    }
}

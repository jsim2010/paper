use crate::ui;
use crate::{
    Notice, DrawSketch, Edge, IdentifyNoise,
    Outcome, Operation, Paper, SetMarks, UpdateView,
};
use std::fmt;
use std::rc::Rc;
use rec::{Pattern, ChCls, SOME, VAR, Quantifier, Atom};

#[derive(Debug)]
pub(crate) struct Controller {
    mode: Mode,
    display: Rc<dyn ModeHandler>,
    command: Rc<dyn ModeHandler>,
    filter: Rc<dyn ModeHandler>,
    action: Rc<dyn ModeHandler>,
    edit: Rc<dyn ModeHandler>,
}

impl Default for Controller {
    fn default() -> Controller {
        Controller {
            mode: Default::default(),
            display: Rc::new(DisplayMode),
            command: Rc::new(CommandMode::new()),
            filter: Rc::new(FilterMode),
            action: Rc::new(ActionMode),
            edit: Rc::new(EditMode),
        }
    }
}

impl Controller {
    pub(crate) fn process_input(&self, input: Option<char>) -> Vec<Rc<dyn Operation>> {
        if let Some(c) = input {
            return self.mode().process_input(c)
        }
        
        Vec::new()
    }

    pub(crate) fn enhancements(&self) -> Vec<Rc<dyn Enhancement>> {
        self.mode().enhancements()
    }

    pub(crate) fn set_mode(&mut self, mode: Mode) {
        self.mode = mode;
    }

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

/// Specifies the functionality of the editor for a given state.
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

impl fmt::Display for Mode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}", self)
    }
}

impl Default for Mode {
    fn default() -> Mode {
        Mode::Display
    }
}

trait ModeHandler: fmt::Debug {
    fn process_input(&self, input: char) -> Vec<Rc<dyn Operation>>;
    fn enhancements(&self) -> Vec<Rc<dyn Enhancement>>;
}

#[derive(Debug)]
struct DisplayMode;

impl ModeHandler for DisplayMode {
    fn process_input(&self, input: char) -> Vec<Rc<dyn Operation>> {
        match input {
            '.' => vec![Rc::new(ChangeMode(Mode::Command))],
            '#' | '/' => vec![
                Rc::new(ChangeMode(Mode::Filter)),
                Rc::new(AddToSketch(input.to_string())),
            ],
            'j' => vec![Rc::new(Scroll(Direction::Down))],
            'k' => vec![Rc::new(Scroll(Direction::Up))],
            _ => Vec::new(),
        }
    }

    fn enhancements(&self) -> Vec<Rc<dyn Enhancement>> {
        Vec::new()
    }
}

#[derive(Debug)]
struct CommandMode {
    execute_command: Rc<dyn Operation>,
    change_to_display: Rc<dyn Operation>,
}

impl CommandMode {
    fn new() -> CommandMode {
        CommandMode {
            execute_command: Rc::new(ExecuteCommand::new()),
            change_to_display: Rc::new(ChangeMode(Mode::Display)),
        }
    }
}

impl ModeHandler for CommandMode {
    fn process_input(&self, input: char) -> Vec<Rc<dyn Operation>> {
        match input {
            ui::ENTER => vec![
                Rc::clone(&self.execute_command),
                Rc::clone(&self.change_to_display),
            ],
            ui::ESC => vec![Rc::clone(&self.change_to_display)],
            _ => vec![Rc::new(AddToSketch(input.to_string())), Rc::new(DrawSketch)],
        }
    }

    fn enhancements(&self) -> Vec<Rc<dyn Enhancement>> {
        Vec::new()
    }
}

#[derive(Debug)]
struct EditMode;

impl ModeHandler for EditMode {
    fn process_input(&self, input: char) -> Vec<Rc<dyn Operation>> {
        match input {
            ui::ESC => vec![Rc::new(ChangeMode(Mode::Display))],
            _ => vec![
                Rc::new(AddToSketch(input.to_string())),
                Rc::new(UpdateView(input)),
            ],
        }
    }

    fn enhancements(&self) -> Vec<Rc<dyn Enhancement>> {
        Vec::new()
    }
}

#[derive(Debug)]
struct ActionMode;

impl ModeHandler for ActionMode {
    fn process_input(&self, input: char) -> Vec<Rc<dyn Operation>> {
        match input {
            ui::ESC => vec![Rc::new(ChangeMode(Mode::Display))],
            'i' => vec![
                Rc::new(SetMarks(Edge::Start)),
                Rc::new(ChangeMode(Mode::Edit)),
            ],
            'I' => vec![
                Rc::new(SetMarks(Edge::End)),
                Rc::new(ChangeMode(Mode::Edit)),
            ],
            _ => Vec::new(),
        }
    }

    fn enhancements(&self) -> Vec<Rc<dyn Enhancement>> {
        Vec::new()
    }
}

#[derive(Debug)]
struct FilterMode;

impl ModeHandler for FilterMode {
    fn process_input(&self, input: char) -> Vec<Rc<dyn Operation>> {
        match input {
            ui::ENTER => vec![Rc::new(ChangeMode(Mode::Action))],
            '\t' => vec![
                Rc::new(IdentifyNoise),
                Rc::new(AddToSketch(String::from("&&"))),
                Rc::new(DrawSketch),
            ],
            ui::ESC => vec![Rc::new(ChangeMode(Mode::Display))],
            _ => vec![Rc::new(AddToSketch(input.to_string())), Rc::new(DrawSketch)],
        }
    }

    fn enhancements(&self) -> Vec<Rc<dyn Enhancement>> {
        vec![Rc::new(FilterNoise), Rc::new(DrawPopup)]
    }
}

#[derive(Debug)]
struct ChangeMode(Mode);

impl Operation for ChangeMode {
    fn operate(&self, paper: &mut Paper) -> Outcome {
        match self.0 {
            Mode::Display => {
                paper.display_view()?;
            }
            Mode::Command | Mode::Filter => {
                paper.reset_sketch();
            }
            Mode::Action => {}
            Mode::Edit => {
                paper.display_view()?;
                paper.reset_sketch();
            }
        }

        paper.change_mode(self.0);
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
struct AddToSketch(String);

impl Operation for AddToSketch {
    fn operate(&self, paper: &mut Paper) -> Outcome {
        if !paper.add_to_sketch(&self.0) {
            return Ok(Some(Notice::Flash))
        }

        for enhancement in paper.enhancements() {
            enhancement.enhance(paper)?;
        }

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

pub(crate) trait Enhancement {
    fn enhance(&self, paper: &mut Paper) -> Result<(), String>;
}

struct FilterNoise;

impl Enhancement for FilterNoise {
    fn enhance(&self, paper: &mut Paper) -> Result<(), String> {
        paper.filter_noise();
        paper.clear_background()?;
        paper.draw_filter_backgrounds()
    }
}

struct DrawPopup;

impl Enhancement for DrawPopup {
    fn enhance(&self, paper: &mut Paper) -> Result<(), String> {
        paper.draw_popup()
    }
}

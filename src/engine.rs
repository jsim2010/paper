use crate::ui;
use crate::{
    Notice, DrawSketch, Edge, Enhancement, ExecuteCommand, IdentifyNoise,
    Outcome, Operation, Paper, ScrollDown, ScrollUp, SetMarks, UpdateView,
};
use std::fmt;
use std::rc::Rc;

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

    pub(crate) fn enhance(&self, paper: &Paper) -> Option<Enhancement> {
        self.mode().enhance(paper)
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
    fn enhance(&self, paper: &Paper) -> Option<Enhancement>;
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
                Rc::new(DrawSketch),
            ],
            'j' => vec![Rc::new(ScrollDown)],
            'k' => vec![Rc::new(ScrollUp)],
            _ => Vec::new(),
        }
    }

    fn enhance(&self, _paper: &Paper) -> Option<Enhancement> {
        None
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

    fn enhance(&self, _paper: &Paper) -> Option<Enhancement> {
        None
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

    fn enhance(&self, _paper: &Paper) -> Option<Enhancement> {
        None
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

    fn enhance(&self, paper: &Paper) -> Option<Enhancement> {
        let mut sections = paper.noises.clone();

        if let Some(last_feature) = paper
            .patterns
            .first_feature
            .tokenize_iter(&paper.sketch)
            .last()
            .and_then(|x| x.get("feature"))
        {
            if let Some(id) = last_feature.chars().nth(0) {
                for filter in paper.filters.iter() {
                    if id == filter.id() {
                        filter.extract(last_feature, &mut sections, &paper.view);
                        break;
                    }
                }
            }
        }

        Some(Enhancement::FilterSections(sections))
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

    fn enhance(&self, _paper: &Paper) -> Option<Enhancement> {
        None
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
struct AddToSketch(String);

impl Operation for AddToSketch {
    fn operate(&self, paper: &mut Paper) -> Outcome {
        if !paper.add_to_sketch(self.0) {
            return Ok(Some(Notice::Flash))
        }

        match paper.enhancements() {
            Some(Enhancement::FilterSections(sections)) => {
                paper.clear_background()?;

                // Add back in the noise
                for noise in paper.noises.iter() {
                    if let Some(region) = noise.to_region(&paper.view.origin) {
                        paper
                            .ui
                            .apply(Edit::new(region, Change::Format(Color::Blue)))?;
                    }
                }

                for section in sections.iter() {
                    if let Some(region) = section.to_region(&paper.view.origin) {
                        paper
                            .ui
                            .apply(Edit::new(region, Change::Format(Color::Red)))?;
                    }
                }

                paper.signals = sections;
            }
            None => {}
        }

        Ok(None)
    }
}

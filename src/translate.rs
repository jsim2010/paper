//! Implements the functionality of converting [`Input`] to [`Operation`]s.
use crate::{
    app::{Direction, Operation, Sheet},
    ui::Input,
    Alert, Mode, Effect,
};
use lazy_static::lazy_static;
use regex::Regex;
use std::{fmt::Debug, path::PathBuf};

/// Defines the functionality to convert [`Input`] to [`Operation`]s.
pub(crate) trait Interpreter: Debug {
    /// Converts `input` to [`Operation`]s.
    fn decode(&self, input: Input, sheet: &Sheet) -> Vec<Operation>;
}

/// The [`Interpreter`] for [`Mode::Display`].
#[derive(Clone, Debug)]
pub(crate) struct DisplayInterpreter {}

impl DisplayInterpreter {
    /// Creates a `DisplayInterpreter`.
    pub(crate) const fn new() -> Self {
        Self {}
    }
}

impl Interpreter for DisplayInterpreter {
    fn decode(&self, input: Input, _sheet: &Sheet) -> Vec<Operation> {
        if let Input::Char(c) = input {
            match c {
                '.' => vec![Operation::EnterMode(Mode::Command)],
                '#' | '/' => vec![
                    Operation::EnterMode(Mode::Filter),
                    Operation::ResetControlPanel(Some(c)),
                ],
                'j' => vec![Operation::Scroll(Direction::Down)],
                'k' => vec![Operation::Scroll(Direction::Up)],
                _ => vec![],
            }
        } else {
            vec![]
        }
    }
}

/// The [`Interpreter`] for [`Mode::Command`].
#[derive(Debug)]
pub(crate) struct CommandInterpreter {}

impl CommandInterpreter {
    /// Creates a `CommandInterpreter`.
    pub(crate) const fn new() -> Self {
        Self {}
    }
}

impl Interpreter for CommandInterpreter {
    fn decode(&self, input: Input, sheet: &Sheet) -> Vec<Operation> {
        lazy_static! {
            static ref RE: Regex = Regex::new(r"(?P<command>\S+)(\s+(?P<args>.*))")
                .expect("compiling command regular expression");
        }

        match input {
            Input::Enter => {
                let mut operations = Vec::new();

                if let Some(captures) = RE.captures(sheet.control_panel()) {
                    match captures.name("command").map(|cmd_match| cmd_match.as_str()) {
                        None => {}
                        Some(command) => match command {
                            "see" => {
                                if let Some(path) =
                                    captures.name("args").map(|args_match| args_match.as_str())
                                {
                                    operations.push(Operation::DisplayFile(Box::new(
                                        PathBuf::from(path),
                                    )));
                                }
                            }
                            "put" => {
                                operations.push(Operation::Save);
                            }
                            "end" => {
                                operations.push(Operation::Quit);
                            }
                            _ => {
                                operations.push(Operation::UserError);
                            }
                        },
                    }

                    operations.push(Operation::EnterMode(Mode::Display));
                    operations
                } else {
                    vec![]
                }
            }
            Input::Escape => vec![Operation::EnterMode(Mode::Display)],
            Input::Char(c) => vec![Operation::AddToControlPanel(c)],
        }
    }
}

/// The [`Interpreter`] for [`Mode::Filter`].
#[derive(Debug)]
pub(crate) struct FilterInterpreter {}

impl FilterInterpreter {
    /// Creates a `FilterInterpreter`.
    pub(crate) const fn new() -> Self {
        Self {}
    }
}

impl Interpreter for FilterInterpreter {
    fn decode(&self, input: Input, _sheet: &Sheet) -> Vec<Operation> {
        if let Input::Escape = input {
            vec![Operation::EnterMode(Mode::Display)]
        } else {
            vec![]
        }
    }
}

/// The [`Interpreter`] for [`Mode::Action`].
#[derive(Debug)]
pub(crate) struct ActionInterpreter {}

impl ActionInterpreter {
    /// Creates a `ActionInterpreter`.
    pub(crate) const fn new() -> Self {
        Self {}
    }
}

impl Interpreter for ActionInterpreter {
    fn decode(&self, input: Input, _sheet: &Sheet) -> Vec<Operation> {
        if let Input::Escape = input {
            vec![Operation::EnterMode(Mode::Display)]
        } else {
            vec![]
        }
    }
}

/// The [`Interpreter`] for [`Mode::Edit`].
#[derive(Debug)]
pub(crate) struct EditInterpreter {}

impl EditInterpreter {
    /// Creates a `EditInterpreter`.
    pub(crate) const fn new() -> Self {
        Self {}
    }
}

impl Interpreter for EditInterpreter {
    fn decode(&self, input: Input, _sheet: &Sheet) -> Vec<Operation> {
        if let Input::Escape = input {
            vec![Operation::EnterMode(Mode::Display)]
        } else {
            vec![]
        }
    }
}

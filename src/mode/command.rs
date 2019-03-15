use super::{EditableString, Flag, Initiation, Name, Operation, Output};
use crate::ui::{Edit, ENTER, ESC};
use rec::ChCls::{Any, End, Whitespace};
use rec::{lazy_some, some, tkn, var, Element, Pattern};
use std::path::PathBuf;

#[derive(Clone, Debug)]
pub(crate) struct Processor {
    command: EditableString,
    command_pattern: Pattern,
}

impl Processor {
    pub(crate) fn new() -> Self {
        Self {
            command: EditableString::new(),
            command_pattern: Pattern::define(
                tkn!(lazy_some(Any) => "command")
                    + (End | (some(Whitespace) + tkn!(var(Any) => "args"))),
            ),
        }
    }
}

impl super::Processor for Processor {
    fn enter(&mut self, _initiation: Option<Initiation>) -> Output<Vec<Edit>> {
        self.command.clear();
        Ok(self.command.edits())
    }

    fn decode(&mut self, input: char) -> Output<Operation> {
        match input {
            ENTER => {
                let mut initiation = None;
                let command_tokens = self.command_pattern.tokenize(&self.command);

                match command_tokens.get("command") {
                    Some("see") => {
                        if let Some(path) = command_tokens.get("args") {
                            initiation = Some(Initiation::SetView(PathBuf::from(path)))
                        }
                    }
                    Some("put") => {
                        initiation = Some(Initiation::Save);
                    }
                    Some("end") => {
                        return Err(Flag::Quit);
                    }
                    _ => (),
                }

                Ok(Operation::EnterMode(Name::Display, initiation))
            }
            ESC => Ok(Operation::EnterMode(Name::Display, None)),
            _ => Ok(Operation::EditUi(self.command.edits_after_add(input))),
        }
    }
}

//! Implements the [`Operation`] to execute a command.
use crate::engine::{
    lazy_some, some, tkn, var, Any, Element, End, Notice, OpCode, Operation, Output, Paper,
    Pattern, Whitespace,
};

/// Executes the command stored in the sketch.
#[derive(Clone, Debug)]
pub(crate) struct Op {
    /// The [`Pattern`] that matches the name of a command.
    command_pattern: Pattern,
    /// The [`Pattern`] that matches the `see <path>` command.
    see_pattern: Pattern,
}

impl Op {
    /// Creates a new `Op`.
    pub(crate) fn new() -> Self {
        Self {
            command_pattern: Pattern::define(
                tkn!(lazy_some(Any) => "command")
                    + (End | (some(Whitespace) + tkn!(var(Any) => "args"))),
            ),
            see_pattern: Pattern::define("see" + some(Whitespace) + tkn!(var(Any) => "path")),
        }
    }
}

impl Operation for Op {
    fn name(&self) -> String {
        String::from("ExecuteCommand")
    }

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

#[cfg(test)]
mod tests {
    use super::*;
    use spectral::prelude::*;
    use std::rc::Rc;
    use crate::ui::TestableUserInterface;

    #[test]
    fn end_returns_quit() {
        let mut paper = Paper::with_ui(Rc::new(TestableUserInterface));
        paper.sketch.push_str("end");
        let output = Op::new().operate(&mut paper, OpCode::ExecuteCommand);

        asserting!("ExecuteCommand output")
            .that(&output)
            .is_ok()
            .is_some()
            .is_equal_to(Notice::Quit);
    }
}

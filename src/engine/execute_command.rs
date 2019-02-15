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

    fn operate(&self, paper: &mut Paper<'_>, _opcode: OpCode) -> Output {
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

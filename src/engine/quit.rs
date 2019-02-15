//! Implements the [`Operation`] to quit.
use crate::engine::{Notice, OpCode, Operation, Output, Paper};

/// Quits the application.
#[derive(Clone, Debug)]
pub(crate) struct Op;

impl Operation for Op {
    fn name(&self) -> String {
        String::from("Quit")
    }

    fn operate(&self, _paper: &mut Paper<'_>, _opcode: OpCode) -> Output {
        Ok(Some(Notice::Quit))
    }
}

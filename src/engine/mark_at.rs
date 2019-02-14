//! Implements the [`Operation`] to set [`Mark`]s.
use crate::engine::{OpCode, Operation, Output, Paper};

/// Sets the location of [`Mark`]s at an [`Edge`] of every signal.
#[derive(Debug)]
pub(crate) struct Op;

impl Operation for Op {
    fn name(&self) -> String {
        String::from("MarkAt")
    }

    fn operate(&self, paper: &mut Paper<'_>, opcode: OpCode) -> Output {
        if let OpCode::MarkAt(edge) = opcode {
            paper.set_marks(edge);
        }

        Ok(None)
    }
}

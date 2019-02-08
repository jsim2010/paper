//! Implements the [`Operation`] to update the view.
use crate::engine::{OpCode, Operation, Output, Paper};

/// Updates the view.
#[derive(Debug)]
pub(crate) struct Op;

impl Operation for Op {
    fn name(&self) -> String {
        String::from("UpdateView")
    }

    fn operate(&self, paper: &mut Paper, opcode: OpCode) -> Output {
        if let OpCode::UpdateView(input) = opcode {
            paper.update_view(input)?;
        }

        Ok(None)
    }
}

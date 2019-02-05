use crate::engine::{Operation, Paper, OpCode, Output};

/// Sets the location of [`Mark`]s at an [`Edge`] of every signal.
#[derive(Debug)]
pub(crate) struct Op;

impl Operation for Op {
    fn name(&self) -> String {
        String::from("MarkAt")
    }

    fn operate(&self, paper: &mut Paper, opcode: OpCode) -> Output {
        if let OpCode::MarkAt(edge) = opcode {
            paper.set_marks(edge);
            Ok(None)
        } else {
            Err(self.invalid_opcode_error(opcode))
        }
    }
}

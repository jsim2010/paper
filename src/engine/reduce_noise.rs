use crate::engine::{Operation, Paper, OpCode, Output};

/// Sets the noise equal to the signals that match the current filter.
#[derive(Debug)]
pub(crate) struct Op;

impl Operation for Op {
    fn name(&self) -> String {
        String::from("ReduceNoise")
    }

    fn operate(&self, paper: &mut Paper, _opcode: OpCode) -> Output {
        paper.reduce_noise();
        Ok(None)
    }
}

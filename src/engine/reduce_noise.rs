//! Implements the [`Operation`] to reduce noise.
use crate::engine::{OpCode, Operation, Output, Paper};

/// Sets the noise equal to the signals that match the current filter.
#[derive(Debug)]
pub(crate) struct Op;

impl Operation for Op {
    fn name(&self) -> String {
        String::from("ReduceNoise")
    }

    fn operate(&self, paper: &mut Paper<'_>, _opcode: OpCode) -> Output {
        paper.reduce_noise();
        Ok(None)
    }
}

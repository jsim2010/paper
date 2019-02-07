//! Implements the [`Operation`] to draw the sketch.
use crate::engine::{OpCode, Operation, Output, Paper};

/// Draws the current sketch.
#[derive(Clone, Debug)]
pub(crate) struct Op;

impl Operation for Op {
    fn name(&self) -> String {
        String::from("DrawSketch")
    }

    fn operate(&self, paper: &mut Paper, _opcode: OpCode) -> Output {
        paper.draw_sketch()?;
        Ok(None)
    }
}

//! Implements the [`Operation`] to add to the sketch.
use crate::engine::{Notice, OpCode, Operation, Output, Paper, BACKSPACE};

/// Adds a character to the sketch.
#[derive(Clone, Debug)]
pub(crate) struct Op;

impl Operation for Op {
    fn name(&self) -> String {
        String::from("AddToSketch")
    }

    fn operate(&self, paper: &mut Paper, opcode: OpCode) -> Output {
        if let OpCode::AddToSketch(input) = opcode {
            if let BACKSPACE = input {
                if paper.sketch_mut().pop().is_none() {
                    return Ok(Some(Notice::Flash));
                }
            } else {
                paper.sketch_mut().push(input);
            }

            Ok(None)
        } else {
            Err(self.invalid_opcode_error(opcode))
        }
    }
}

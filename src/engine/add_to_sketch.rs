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
        }

        Ok(None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use spectral::prelude::*;

    fn add_to_sketch(paper: &mut Paper, input: char) -> Output {
        Op.operate(paper, OpCode::AddToSketch(input))
    }

    #[test]
    fn flash_if_remove_from_empty_sketch() {
        let mut paper = Paper::new();
        let output = add_to_sketch(&mut paper, BACKSPACE);

        asserting!("AddToSketch output")
            .that(&output)
            .is_ok()
            .is_some()
            .is_equal_to(Notice::Flash);
    }

    #[test]
    fn remove_char_if_backspace() {
        let mut paper = Paper::new();
        paper.sketch.push_str("abc");
        let output = add_to_sketch(&mut paper, BACKSPACE);

        asserting!("AddToSketch output")
            .that(&output)
            .is_ok()
            .is_none();
        asserting!("paper.sketch")
            .that(&paper.sketch)
            .is_equal_to(String::from("ab"));
    }

    #[test]
    fn add_char() {
        let mut paper = Paper::new();
        paper.sketch.push_str("abc");
        let output = add_to_sketch(&mut paper, 'd');

        asserting!("AddToSketch output")
            .that(&output)
            .is_ok()
            .is_none();
        asserting!("paper.sketch")
            .that(&paper.sketch)
            .is_equal_to(String::from("abcd"));
    }
}

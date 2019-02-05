use crate::{IndexType, TryFrom, TryFromIntError};
use crate::engine::{Direction, Failure, Operation, Paper, OpCode, Output};

/// Changes the part of the view that is visible.
#[derive(Debug)]
pub(crate) struct Op;

impl Operation for Op {
    fn name(&self) -> String {
        String::from("Scroll")
    }

    fn operate(&self, paper: &mut Paper, opcode: OpCode) -> Output {
        if let OpCode::Scroll(direction) = opcode {
            let mut movement = IndexType::try_from(paper.scroll_height())?;

            if let Direction::Up = direction {
                movement = movement.checked_neg().ok_or(Failure::Conversion(TryFromIntError::Overflow))?;
            }

            paper.scroll(movement)?;
            paper.display_view()?;
            Ok(None)
        } else {
            Err(self.invalid_opcode_error(opcode))
        }
    }
}

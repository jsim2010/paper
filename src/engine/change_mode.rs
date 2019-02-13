//! Implements the [`Operation`] to change the mode.
use crate::engine::{Mode, OpCode, Operation, Output, Paper};

/// Changes the [`Mode`] of the application.
#[derive(Clone, Debug)]
pub(crate) struct Op;

impl Operation for Op {
    fn name(&self) -> String {
        String::from("ChangeMode")
    }

    fn operate(&self, paper: &mut Paper, opcode: OpCode) -> Output {
        if let OpCode::ChangeMode(mode) = opcode {
            match mode {
                Mode::Display => {
                    paper.sketch_mut().clear();
                    paper.display_view()?;
                }
                Mode::Command | Mode::Filter => {
                    paper.draw_sketch()?;
                }
                Mode::Action => {}
                Mode::Edit => {
                    paper.display_view()?;
                }
            }

            paper.change_mode(mode);
        }

        Ok(None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::mock::UserInterface as MockUserInterface;
    use spectral::prelude::*;
    use std::rc::Rc;

    #[test]
    fn sets_mode() {
        let mock_ui = MockUserInterface::new(Ok(()), Ok(()), Ok(()), Ok(()), Ok(0), None);
        let mut paper = Paper::with_ui(Rc::new(mock_ui));
        paper.controller.mode = Mode::Display;
        let output = Op.operate(&mut paper, OpCode::ChangeMode(Mode::Command));

        asserting!("ChangeMode output")
            .that(&output)
            .is_ok()
            .is_none();
        asserting!("paper.controller.mode")
            .that(&paper.controller.mode)
            .is_equal_to(Mode::Command);
    }

    #[test]
    fn display_clears_sketch() {
        let mock_ui = MockUserInterface::new(Ok(()), Ok(()), Ok(()), Ok(()), Ok(0), None);
        let mut paper = Paper::with_ui(Rc::new(mock_ui));
        paper.sketch.push_str("abc");
        let output = Op.operate(&mut paper, OpCode::ChangeMode(Mode::Display));

        asserting!("ChangeMode output")
            .that(&output)
            .is_ok()
            .is_none();
        asserting!("paper.sketch")
            .that(&paper.sketch)
            .is_equal_to(String::from(""));
        asserting!("paper.controller.mode")
            .that(&paper.controller.mode)
            .is_equal_to(Mode::Display);
    }
}

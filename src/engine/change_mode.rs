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

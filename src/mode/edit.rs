//! Implements functionality for the application while in edit mode.
use super::{Initiation, Operation, Output, Pane, Position};
use crate::{ptr::Mrc, ui::ESC};

/// The [`Processor`] of the edit mode.
#[derive(Debug)]
pub(crate) struct Processor {
    /// The [`Pane`] of the application.
    pane: Mrc<Pane>,
    /// All [`Position`]s where edits should be executed.
    positions: Vec<Position>,
}

impl Processor {
    /// Creates a new `Processor`.
    pub(crate) fn new(pane: &Mrc<Pane>) -> Self {
        Self {
            pane: Mrc::clone(pane),
            positions: Vec::new(),
        }
    }
}

impl super::Processor for Processor {
    fn enter(&mut self, initiation: &Option<Initiation>) -> Output<()> {
        if let Some(Initiation::Mark(positions)) = initiation {
            self.positions = positions.clone();
            // TextEdits are applied from bottom to top.
            self.positions.reverse();
        }

        Ok(())
    }

    fn decode(&mut self, input: char) -> Output<Operation> {
        let mut pane = self.pane.borrow_mut();

        if input == ESC {
            Ok(Operation::enter_display())
        } else {
            for mut position in &mut self.positions {
                pane.add(&mut position, input)?;
            }

            Ok(Operation::maintain())
        }
    }
}

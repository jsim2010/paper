//! Implements functionality for the application while in display mode.
use super::{Initiation, Operation, Output, Pane};
use crate::ptr::Mrc;

/// The [`Processor`] of the display mode.
#[derive(Clone, Debug)]
pub(crate) struct Processor {
    /// The [`Pane`] of the application.
    pane: Mrc<Pane>,
}

impl Processor {
    /// Creates a new `Processor`.
    pub(crate) fn new(pane: &Mrc<Pane>) -> Self {
        Self {
            pane: Mrc::clone(pane),
        }
    }
}

impl super::Processor for Processor {
    fn enter(&mut self, initiation: &Option<Initiation>) -> Output<()> {
        let mut pane = self.pane.borrow_mut();

        match initiation {
            Some(Initiation::SetView(path)) => {
                pane.change(path)?;
            }
            Some(Initiation::Save) => {
                pane.save()?;
            }
            _ => (),
        }

        pane.wipe();

        Ok(())
    }

    fn decode(&mut self, input: char) -> Output<Operation> {
        let mut pane = self.pane.borrow_mut();

        match input {
            '.' => Ok(Operation::enter_command()),
            '#' | '/' => Ok(Operation::enter_filter(input)),
            'j' => {
                pane.scroll_down();
                Ok(Operation::maintain())
            }
            'k' => {
                pane.scroll_up();
                Ok(Operation::maintain())
            }
            _ => Ok(Operation::maintain()),
        }
    }
}

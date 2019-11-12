//! Implements functionality for the application while in edit mode.
use super::{Operation, Output, Mode, Sheet, Position};
use crate::ui::{Input, ESC};

/// The [`Processor`] of the edit mode.
#[derive(Debug)]
pub(crate) struct Processor {
    /// All [`Position`]s where edits should be executed.
    positions: Vec<Position>,
}

impl Processor {
    /// Creates a new `Processor`.
    pub(crate) const fn new() -> Self {
        Self {
            positions: Vec::new(),
        }
    }
}

impl super::Processor for Processor {
    fn decode(&self, _sheet: &Sheet, input: Input) -> Output<Vec<Operation>> {
        if let Input::Key(key) = input {
            if key == ESC {
                Ok(vec![Operation::EnterMode(Mode::Display)])
            } else {
                Ok(vec![Operation::Add(key)])
            }
        } else {
            Ok(vec![])
        }
    }
}

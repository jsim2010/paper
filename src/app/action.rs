//! Implements functionality for the application while in action mode.
use super::{Mode, Operation, Output, Sheet};
use crate::ui::{Input, ESC};
use lsp_msg::Range;

/// The [`Processor`] of the action mode.
#[derive(Debug)]
pub(crate) struct Processor {
    /// The [`Range`]s of the signals.
    signals: Vec<Range>,
}

impl Processor {
    /// Creates a new `Processor`.
    pub(crate) const fn new() -> Self {
        Self {
            signals: Vec::new(),
        }
    }
}

impl super::Processor for Processor {
    fn decode(&self, _sheet: &Sheet, input: Input) -> Output<Vec<Operation>> {
        if let Input::Key(key) = input {
            match key {
                ESC => Ok(vec![Operation::EnterMode(Mode::Display)]),
                // TODO: Add functionality back in.
                //'i' => Ok(vec![Operation::Old(OldOperation::enter_edit(
                //    self.signals.iter().map(|signal| signal.start).collect(),
                //))]),
                //'I' => Ok(vec![Operation::Old(OldOperation::enter_edit(
                //    self.signals.iter().map(|signal| signal.end).collect(),
                //))]),
                _ => Ok(vec![])
            }
        } else {
            Ok(vec![])
        }
    }
}

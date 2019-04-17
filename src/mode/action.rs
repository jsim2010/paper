//! Implements functionality for the application while in action mode.
use super::{Initiation, Operation, Output};
use crate::ui::ESC;
use lsp_msg::Range;

/// The [`Processor`] of the action mode.
#[derive(Debug)]
pub(crate) struct Processor {
    /// The [`Range`]s of the signals.
    signals: Vec<Range>,
}

impl Processor {
    /// Creates a new `Processor`.
    pub(crate) fn new() -> Self {
        Self {
            signals: Vec::new(),
        }
    }
}

impl super::Processor for Processor {
    fn enter(&mut self, initiation: &Option<Initiation>) -> Output<()> {
        if let Some(Initiation::SetSignals(signals)) = initiation {
            self.signals = signals.to_vec();
        }

        Ok(())
    }

    fn decode(&mut self, input: char) -> Output<Operation> {
        match input {
            ESC => Ok(Operation::enter_display()),
            'i' => Ok(Operation::enter_edit(
                self.signals.iter().map(|signal| signal.start).collect(),
            )),
            'I' => Ok(Operation::enter_edit(
                self.signals.iter().map(|signal| signal.end).collect(),
            )),
            _ => Ok(Operation::maintain()),
        }
    }
}

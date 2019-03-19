//! Implements functionality for the application while in action mode.
use super::{Initiation, Name, Operation, Output};
use crate::ui::{Edit, ESC};
use lsp_types::Range;

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
    fn enter(&mut self, initiation: Option<Initiation>) -> Output<Vec<Edit>> {
        if let Some(Initiation::SetSignals(signals)) = initiation {
            self.signals = signals;
        }

        Ok(vec![])
    }

    fn decode(&mut self, input: char) -> Output<Operation> {
        match input {
            ESC => Ok(Operation::EnterMode(Name::Display, None)),
            'i' => Ok(Operation::EnterMode(
                Name::Edit,
                Some(Initiation::Mark(self.signals.iter().map(|signal| signal.start).collect())),
            )),
            'I' => Ok(Operation::EnterMode(
                Name::Edit,
                Some(Initiation::Mark(self.signals.iter().map(|signal| signal.end).collect())),
            )),
            _ => Ok(Operation::Noop),
        }
    }
}

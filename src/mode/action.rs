//! Implements functionality for the application while in action mode.
use super::{Initiation, Mark, Name, Operation, Output};
use crate::ui::{Edit, ESC};
use lsp_types::Range;
use std::fmt::{self, Display, Formatter};

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

    /// Returns the [`Marks`] at the given [`Edge`] of the current signals.
    fn get_marks(&mut self, edge: Edge) -> Vec<Mark> {
        let mut marks = Vec::new();

        for signal in &self.signals {
            let position = if edge == Edge::Start {
                signal.start
            } else {
                signal.end
            };

            marks.push(Mark { position });
        }

        marks
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
                Some(Initiation::Mark(self.get_marks(Edge::Start))),
            )),
            'I' => Ok(Operation::EnterMode(
                Name::Edit,
                Some(Initiation::Mark(self.get_marks(Edge::End))),
            )),
            _ => Ok(Operation::Noop),
        }
    }
}

/// Indicates a specific Place of a given Range.
#[derive(Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Debug)]
enum Edge {
    /// Indicates the first Place of the Range.
    Start,
    /// Indicates the last Place of the Range.
    End,
}

impl Default for Edge {
    #[inline]
    fn default() -> Self {
        Edge::Start
    }
}

impl Display for Edge {
    #[inline]
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Edge::Start => write!(f, "Starting edge"),
            Edge::End => write!(f, "Ending edge"),
        }
    }
}

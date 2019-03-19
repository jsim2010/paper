//! Implements functionality for the application while in action mode.
use super::{Initiation, Mark, Name, Operation, Output, Pane, Section};
use crate::ui::{Edit, ESC};
use crate::Mrc;
use std::fmt::{self, Display, Formatter};
use try_from::TryFrom;

/// The [`Processor`] of the action mode.
#[derive(Debug)]
pub(crate) struct Processor {
    /// The [`Section`]s of the signals.
    signals: Vec<Section>,
    /// The [`Pane`] of the application.
    pane: Mrc<Pane>,
}

impl Processor {
    /// Creates a new `Processor`.
    pub(crate) fn new(pane: &Mrc<Pane>) -> Self {
        Self {
            signals: Vec::new(),
            pane: Mrc::clone(pane),
        }
    }

    /// Returns the [`Marks`] at the given [`Edge`] of the current signals.
    fn get_marks(&mut self, edge: Edge) -> Vec<Mark> {
        let mut marks = Vec::new();
        let pane: &Pane = &self.pane.borrow();

        for signal in &self.signals {
            let mut position = signal.start;

            if edge == Edge::End {
                position.character += u64::try_from(signal.length)
                    .unwrap_or_else(|_| pane.line_length(signal.start).unwrap_or_default());
            }
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

/// Indicates a specific Place of a given Section.
#[derive(Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Debug)]
enum Edge {
    /// Indicates the first Place of the Section.
    Start,
    /// Indicates the last Place of the Section.
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

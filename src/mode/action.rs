use super::{Mark, Section, Pane, Index, Pointer, Initiation, Operation, Name, Output};
use std::fmt::{self, Display, Formatter};
use try_from::TryFrom;
use crate::ui::{ESC, Edit};
use crate::Mrc;

#[derive(Debug)]
pub(crate) struct Processor {
    signals: Vec<Section>,
    pane: Mrc<Pane>,
}

impl Processor {
    pub(crate) fn new(pane: &Mrc<Pane>) -> Self {
        Self {
            signals: Vec::new(),
            pane: pane.clone(),
        }
    }

    fn get_marks(&mut self, edge: Edge) -> Vec<Mark> {
        let mut marks = Vec::new();
        let pane: &Pane = &self.pane.borrow();

        for signal in &self.signals {
            let mut place = signal.start;

            if edge == Edge::End {
                place.column += Index::try_from(signal.length)
                    .unwrap_or_else(|_| pane.line_length(signal.start).unwrap_or_default());
            }

            let pointer = Pointer::new(
                pane.line_indices().nth(place.line.row()).and_then(|index_value| Index::try_from(index_value).ok())) + place.column;
            marks.push(Mark {place, pointer} );
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
        return match input {
            ESC => Ok(Operation::EnterMode(Name::Display, None)),
            'i' => Ok(Operation::EnterMode(Name::Edit, Some(Initiation::Mark(self.get_marks(Edge::Start))))),
            'I' => Ok(Operation::EnterMode(Name::Edit, Some(Initiation::Mark(self.get_marks(Edge::End))))),
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

//! Implements functionality for the application while in edit mode.
use super::{Adjustment, Change, Initiation, Mark, Name, Operation, Output, Pane};
use crate::ui::{Edit, ESC};
use crate::Mrc;

/// The [`Processor`] of the edit mode.
#[derive(Debug)]
pub(crate) struct Processor {
    /// The [`Pane`] of the application.
    pane: Mrc<Pane>,
    /// All [`Mark`]s where edits should be executed.
    marks: Vec<Mark>,
}

impl Processor {
    /// Creates a new `Processor`.
    pub(crate) fn new(pane: &Mrc<Pane>) -> Self {
        Self {
            pane: Mrc::clone(pane),
            marks: Vec::new(),
        }
    }
}

impl super::Processor for Processor {
    fn enter(&mut self, initiation: Option<Initiation>) -> Output<Vec<Edit>> {
        if let Some(Initiation::Mark(marks)) = initiation {
            self.marks = marks;
        }

        Ok(vec![])
    }

    fn decode(&mut self, input: char) -> Output<Operation> {
        let mut pane = self.pane.borrow_mut();

        if input == ESC {
            Ok(Operation::EnterMode(Name::Display, None))
        } else {
            let mut adjustment = Adjustment::default();
            let mut edits = Vec::new();

            for mark in &mut self.marks {
                if let Some(new_adjustment) = Adjustment::create(input, mark.position, &pane) {
                    adjustment += new_adjustment;

                    if adjustment.change != Change::Clear {
                        if let Some(region) = pane.region_at(&mark.position) {
                            edits.push(Edit::new(region, adjustment.change.clone()));
                        }
                    }

                    mark.adjust(&adjustment);
                    pane.add(mark, input)?;
                }
            }

            if adjustment.change == Change::Clear {
                pane.clean();
                return Ok(Operation::EditUi(pane.redraw_edits().collect()));
            }

            Ok(Operation::EditUi(edits))
        }
    }
}

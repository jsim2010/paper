use super::{Adjustment, Change, Initiation, Mark, Name, Operation, Output, Pane};
use crate::ui::{Edit, ESC};
use crate::Mrc;

#[derive(Debug)]
pub(crate) struct Processor {
    pane: Mrc<Pane>,
    marks: Vec<Mark>,
}

impl Processor {
    pub(crate) fn new(pane: &Mrc<Pane>) -> Self {
        Self {
            pane: pane.clone(),
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
                if let Some(new_adjustment) = Adjustment::create(input, mark.place, &pane) {
                    adjustment += new_adjustment;

                    if adjustment.change != Change::Clear {
                        if let Some(region) = pane.region_at(&mark.place) {
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

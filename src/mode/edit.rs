//! Implements functionality for the application while in edit mode.
use super::{Initiation, Name, Operation, Output, Pane, Position};
use crate::ui::{Change, Edit, BACKSPACE, ENTER, ESC};
use crate::Mrc;
use lsp_types::{Range, TextEdit};

/// The [`Processor`] of the edit mode.
#[derive(Debug)]
pub(crate) struct Processor {
    /// The [`Pane`] of the application.
    pane: Mrc<Pane>,
    /// All [`Position`]s where edits should be executed.
    positions: Vec<Position>,
}

impl Processor {
    /// Creates a new `Processor`.
    pub(crate) fn new(pane: &Mrc<Pane>) -> Self {
        Self {
            pane: Mrc::clone(pane),
            positions: Vec::new(),
        }
    }
}

impl super::Processor for Processor {
    fn enter(&mut self, initiation: Option<Initiation>) -> Output<Vec<Edit>> {
        if let Some(Initiation::Mark(positions)) = initiation {
            self.positions = positions;
            // TextEdits are applied from bottom to top.
            self.positions.reverse();
        }

        Ok(vec![])
    }

    fn decode(&mut self, input: char) -> Output<Operation> {
        let mut pane = self.pane.borrow_mut();

        if input == ESC {
            Ok(Operation::EnterMode(Name::Display, None))
        } else {
            let mut text_edits = Vec::new();
            let mut edits = Vec::new();
            let mut is_clearing = false;

            for &position in self.positions.iter() {
                let mut new_text = String::new();
                let mut range = Range::new(position, position);

                if input == BACKSPACE {
                    dbg!(&range);
                    if range.start.character == 0 {
                        if range.start.line != 0 {
                            range.start.line -= 1;
                            range.start.character = u64::max_value();
                            is_clearing = true;
                        }
                    } else {
                        range.start.character -= 1;
                        
                        let address = pane.address_at(position);

                        if address.is_some() {
                            dbg!("push Backspace");
                            edits.push(Edit::new(address, Change::Backspace));
                        }
                    }
                } else {
                    new_text.push(input);

                    if input == ENTER {
                        is_clearing = true;
                    } else {
                        let address = pane.address_at(position);

                        if address.is_some() {
                            edits.push(Edit::new(address, Change::Insert(input)));
                        }
                    }
                }

                text_edits.push(TextEdit::new(range, new_text));

                //if let Some(new_adjustment) = Adjustment::create(input, *position, &pane) {
                //    adjustment += new_adjustment;

                //    if adjustment.change != Change::Clear {
                //        let address = pane.address_at(*position);

                //        if address.is_some() {
                //            edits.push(Edit::new(address, adjustment.change.clone()));
                //        }
                //    }

                //    if adjustment.line_change.is_negative() {
                //        position.line -= u64::try_from(-adjustment.line_change).unwrap();
                //    } else {
                //        position.line += u64::try_from(adjustment.line_change).unwrap();
                //    }

                //    for (&line, &change) in &adjustment.indexes_changed {
                //        if line == position.line {
                //            if change.is_negative() {
                //                position.character -= u64::try_from(-change).unwrap();
                //            } else {
                //                position.character += u64::try_from(change).unwrap();
                //            }
                //        }
                //    }

                pane.add(&position, input)?;
                //}
            }

            if is_clearing {
                pane.clean();
                return Ok(Operation::EditUi(pane.redraw_edits().collect()));
            }

            Ok(Operation::EditUi(edits))
        }
    }
}

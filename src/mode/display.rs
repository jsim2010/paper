use crate::storage::Explorer;
use crate::ui::Edit;
use try_from::{TryFrom, TryFromIntError};
use super::{Flag, Output, IndexType, Initiation, Pane, Operation, Name};
use crate::Mrc;

#[derive(Clone, Debug)]
pub(crate) struct Processor {
    explorer: Mrc<dyn Explorer>,
    pane: Mrc<Pane>,
}

impl Processor {
    pub(crate) fn new(pane: &Mrc<Pane>, explorer: &Mrc<dyn Explorer>) -> Self {
        Self {
            explorer: explorer.clone(),
            pane: pane.clone(),
        }
    }
}

impl super::Processor for Processor {
    fn enter(&mut self, initiation: Option<Initiation>) -> Output<Vec<Edit>> {
        let mut pane = self.pane.borrow_mut();

        match initiation {
            Some(Initiation::SetView(path)) => {
                let absolute_path = if path.is_absolute() {
                    path.to_path_buf()
                } else {
                    let mut new_path = std::env::current_dir()?;
                    new_path.push(path);
                    new_path
                };

                pane.change(
                    self.explorer.clone(),
                    absolute_path,
                )?;
            }
            Some(Initiation::Save) => {
                let explorer: std::cell::Ref<'_, (dyn Explorer)> = self.explorer.borrow();
                explorer.write(&pane.path, &pane.data)?;
            }
            _ => (),
        }

        Ok(pane.redraw_edits().collect())
    }

    fn decode(&mut self, input: char) -> Output<Operation> {
        let mut pane = self.pane.borrow_mut();
        let scroll_length = IndexType::try_from(pane.height / 4)?;

        return match input {
            '.' => {
                Ok(Operation::EnterMode(Name::Command, None))
            }
            '#' | '/' => {
                Ok(Operation::EnterMode(Name::Filter, Some(Initiation::StartFilter(input))))
            }
            'j' => {
                let mut operation = Operation::Noop;

                if pane.scroll(scroll_length) {
                    operation = Operation::EditUi(pane.redraw_edits().collect())
                }

                Ok(operation)
            }
            'k' => {
                let mut operation = Operation::Noop;

                if pane.scroll(scroll_length.checked_neg().ok_or(Flag::Conversion(TryFromIntError::Overflow))?) {
                    operation = Operation::EditUi(pane.redraw_edits().collect());
                }

                Ok(operation)
            }
            _ => Ok(Operation::Noop),
        }
    }
}

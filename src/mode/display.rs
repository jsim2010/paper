//! Implements functionality for the application while in display mode.
use super::{Flag, Initiation, Operation, Output, Pane};
use crate::storage::Explorer;
use crate::Mrc;
use try_from::TryFromIntError;

/// The [`Processor`] of the display mode.
#[derive(Clone, Debug)]
pub(crate) struct Processor {
    /// The [`Explorer`] of the application.
    explorer: Mrc<dyn Explorer>,
    /// The [`Pane`] of the application.
    pane: Mrc<Pane>,
}

impl Processor {
    /// Creates a new `Processor`.
    pub(crate) fn new(pane: &Mrc<Pane>, explorer: &Mrc<dyn Explorer>) -> Self {
        Self {
            explorer: Mrc::clone(explorer),
            pane: Mrc::clone(pane),
        }
    }
}

impl super::Processor for Processor {
    fn enter(&mut self, initiation: &Option<Initiation>) -> Output<()> {
        let mut pane = self.pane.borrow_mut();

        match initiation {
            Some(Initiation::SetView(path)) => {
                let absolute_path = if path.is_absolute() {
                    path.clone()
                } else {
                    let mut new_path = std::env::current_dir()?;
                    new_path.push(path);
                    new_path
                };

                pane.change(&self.explorer, &absolute_path)?;
            }
            Some(Initiation::Save) => {
                let explorer: std::cell::Ref<'_, (dyn Explorer)> = self.explorer.borrow();
                explorer.write(&pane.path, &pane.data)?;
            }
            _ => (),
        }

        pane.wipe();

        Ok(())
    }

    fn decode(&mut self, input: char) -> Output<Operation> {
        let mut pane = self.pane.borrow_mut();

        match input {
            '.' => Ok(Operation::enter_command()),
            '#' | '/' => Ok(Operation::enter_filter(input)),
            'j' | 'k' => {
                let mut scroll_length = pane.scroll_length()?;

                if input == 'k' {
                    scroll_length = scroll_length
                        .checked_neg()
                        .ok_or(Flag::Conversion(TryFromIntError::Overflow))?;
                }

                pane.scroll(scroll_length as i128);
                Ok(Operation::maintain())
            }
            _ => Ok(Operation::maintain()),
        }
    }
}

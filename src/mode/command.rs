//! Implements functionality for the application while in command mode.
use super::{Flag, Initiation, Operation, Output, Pane};
use crate::{ptr::Mrc, ui::{ENTER, ESC}};
use rec::{ChCls::{Any, End, Whitespace}, lazy_some, some, tkn, var, Element, Pattern};

/// The [`Processor`] of the command mode.
#[derive(Clone, Debug)]
pub(crate) struct Processor {
    /// Matches commands.
    command_pattern: Pattern,
    /// The [`Pane`] of the application.
    pane: Mrc<Pane>,
}

impl Processor {
    /// Creates a new `Processor`.
    pub(crate) fn new(pane: &Mrc<Pane>) -> Self {
        Self {
            pane: Mrc::clone(pane),
            command_pattern: Pattern::new(
                tkn!(lazy_some(Any) => "command")
                    + (End | (some(Whitespace) + tkn!(var(Any) => "args"))),
            ),
        }
    }
}

impl super::Processor for Processor {
    fn enter(&mut self, _initiation: &Option<Initiation>) -> Output<()> {
        self.pane.borrow_mut().reset_control_panel(None);
        Ok(())
    }

    fn decode(&mut self, input: char) -> Output<Operation> {
        let mut pane = self.pane.borrow_mut();

        match input {
            ENTER => {
                let mut operation = Operation::enter_display();
                let command_tokens = self.command_pattern.tokenize(&pane.control_panel);

                match command_tokens.get("command") {
                    Some("see") => {
                        if let Some(path) = command_tokens.get("args") {
                            operation = Operation::display_file(path);
                        }
                    }
                    Some("put") => {
                        operation = Operation::save_file();
                    }
                    Some("end") => {
                        return Err(Flag::Quit);
                    }
                    Some(_) => {
                        return Err(Flag::User);
                    }
                    None => (),
                }

                Ok(operation)
            }
            ESC => Ok(Operation::enter_display()),
            _ => {
                pane.input_to_control_panel(input);
                Ok(Operation::maintain())
            }
        }
    }
}

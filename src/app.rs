//! Implements the application logic of `paper`, converting an [`Input`] into a list of [`Output`]s.
mod translate;

use {
    // TODO: Move everything out of ui.
    crate::io::{
        config::Setting,
        fs::File,
        ui::{BodySize, Selection},
        DocEdit, Input, Output,
    },
    log::trace,
    lsp_types::{MessageType, ShowMessageParams, ShowMessageRequestParams},
    std::{cell::RefCell, rc::Rc},
    translate::{Command, Direction, DocOp, Interpreter, Magnitude, Operation, Vector},
};

/// An empty [`Selection`].
static EMPTY_SELECTION: Selection = Selection::empty();

/// The processor of the application.
#[derive(Debug, Default)]
pub(crate) struct Processor {
    /// The currently visible pane.
    pane: Pane,
    /// The input of a command.
    input: String,
    /// The current command to be implemented.
    command: Option<Command>,
    /// Translates input into operations.
    interpreter: Interpreter,
}

impl Processor {
    /// Creates a new [`Processor`].
    pub(crate) fn new() -> Self {
        Self::default()
    }

    /// Processes `input` and generates [`Output`].
    pub(crate) fn process(&mut self, input: Input) -> Vec<Output> {
        if let Some(operation) = self.interpreter.translate(input) {
            self.operate(operation)
        } else {
            Vec::new()
        }
    }

    /// Performs `operation` and returns the appropriate [`Output`]s.
    pub(crate) fn operate(&mut self, operation: Operation) -> Vec<Output> {
        let mut outputs = Vec::new();
        // Retrieve here to avoid error. This will not work once changes start modifying the working dir.
        match operation {
            Operation::UpdateSetting(setting) => {
                outputs.append(&mut self.update_setting(setting));
            }
            Operation::Size(size) => {
                outputs.push(self.pane.update_size(size));
            }
            Operation::Confirm(action) => {
                outputs.push(Output::Question {
                    request: ShowMessageRequestParams::from(action),
                });
            }
            Operation::Reset => {
                self.input.clear();
                outputs.push(Output::Reset {
                    selection: self
                        .pane
                        .doc
                        .as_ref()
                        .map_or(EMPTY_SELECTION, |doc| doc.selection),
                });
            }
            Operation::Alert(message) => {
                outputs.push(Output::Notify { message });
            }
            Operation::StartCommand(command) => {
                let prompt = command.to_string();

                self.command = Some(command);
                outputs.push(Output::StartIntake { title: prompt });
            }
            Operation::Collect(ch) => {
                self.input.push(ch);
                outputs.push(Output::Write { ch });
            }
            Operation::Execute => {
                if self.command.is_some() {
                    outputs.push(Output::GetFile {
                        path: self.input.clone(),
                    });
                    self.input.clear();
                }
            }
            Operation::Document(doc_op) => {
                outputs.push(self.pane.operate(doc_op));
            }
            Operation::Quit => {
                if let Some(output) = self.pane.close_doc() {
                    outputs.push(output);
                }

                outputs.push(Output::Quit);
            }
            Operation::OpenDoc { file } => {
                outputs.append(&mut self.pane.open_doc(file));
            }
            Operation::SendLsp(message) => {
                outputs.push(Output::SendLsp(message));
            }
        };

        outputs.push(Output::UpdateHeader);
        trace!("outputs: {:?}", outputs);

        outputs
    }

    /// Updates `self` based on `setting`.
    fn update_setting(&mut self, setting: Setting) -> Vec<Output> {
        let mut outputs = Vec::new();

        match setting {
            Setting::Wrap(is_wrapped) => {
                outputs.push(Output::Wrap {
                    is_wrapped,
                    selection: self
                        .pane
                        .doc
                        .as_ref()
                        .map_or(EMPTY_SELECTION, |doc| doc.selection),
                });
            }
        }

        outputs
    }
}

/// A view of the document.
#[derive(Debug, Default)]
struct Pane {
    /// The document in the pane.
    doc: Option<Document>,
    /// The number of lines by which a scroll moves.
    scroll_amount: Rc<RefCell<Amount>>,
}

impl Pane {
    /// Performs `operation` on `self`.
    fn operate(&mut self, operation: DocOp) -> Output {
        if let Some(doc) = &mut self.doc {
            match operation {
                DocOp::Move(vector) => doc.move_selection(&vector),
                DocOp::Delete => doc.delete_selection(),
                DocOp::Save => doc.save(),
            }
        } else {
            Output::Notify {
                message: ShowMessageParams {
                    typ: MessageType::Info,
                    message: format!(
                        "There is no open document on which to perform {}",
                        operation
                    ),
                },
            }
        }
    }

    /// Opens a document at `path`.
    fn open_doc(&mut self, file: File) -> Vec<Output> {
        let mut outputs = Vec::new();
        let doc = self.create_doc(file.clone());
        if let Some(old_doc) = self.doc.take() {
            outputs.push(old_doc.close());
        }

        outputs.push(Output::EditDoc {
            file,
            edit: DocEdit::Open {
                version: doc.version,
            },
        });
        self.doc = Some(doc);
        outputs
    }

    /// Creates a [`Document`] from `path`.
    fn create_doc(&mut self, file: File) -> Document {
        Document::new(file, &self.scroll_amount)
    }

    /// Updates the size of `self` to match `size`;
    fn update_size(&mut self, size: BodySize) -> Output {
        self.scroll_amount
            .borrow_mut()
            .set(usize::from(size.0.rows.wrapping_div(3)));
        Output::Resize { size }
    }

    /// Returns the [`Output`] to close the [`Document`] of `self`.
    fn close_doc(&mut self) -> Option<Output> {
        self.doc.take().map(Document::close)
    }
}

/// A file and the user's current interactions with it.
#[derive(Debug)]
struct Document {
    /// The file of the document.
    file: File,
    /// The current user selection.
    selection: Selection,
    /// The number of lines that a scroll will move.
    scroll_amount: Rc<RefCell<Amount>>,
    /// The version of the document.
    version: i64,
}

impl Document {
    /// Creates a new [`Document`].
    fn new(file: File, scroll_amount: &Rc<RefCell<Amount>>) -> Self {
        let mut selection = Selection::default();

        if !file.is_empty() {
            selection.init();
        }

        Self {
            file,
            selection,
            scroll_amount: Rc::clone(scroll_amount),
            version: 0,
        }
    }

    /// Saves the document.
    fn save(&self) -> Output {
        Output::EditDoc {
            file: self.file.clone(),
            edit: DocEdit::Save,
        }
    }

    /// Deletes the text of the [`Selection`].
    fn delete_selection(&mut self) -> Output {
        self.file
            .delete_selection(self.selection.start_line(), self.selection.end_line());
        self.version = self.version.wrapping_add(1);
        Output::EditDoc {
            file: self.file.clone(),
            edit: DocEdit::Change {
                new_text: String::new(),
                selection: self.selection,
                version: self.version,
            },
        }
    }

    /// Returns the number of lines in `self`.
    fn line_count(&self) -> usize {
        self.file.line_count()
    }

    /// Moves the [`Selection`] as described by [`Vector`].
    fn move_selection(&mut self, vector: &Vector) -> Output {
        let amount = match vector.magnitude() {
            Magnitude::Single => 1,
            Magnitude::Half => self.scroll_amount.borrow().value(),
        };
        match vector.direction() {
            Direction::Down => {
                self.selection.move_down(amount, self.line_count());
            }
            Direction::Up => {
                self.selection.move_up(amount);
            }
        }

        Output::MoveSelection {
            selection: self.selection,
        }
    }

    /// Returns the output to close `self`.
    fn close(self) -> Output {
        Output::EditDoc {
            file: self.file,
            edit: DocEdit::Close,
        }
    }
}

/// A wrapper around [`u64`].
///
/// Used for storing and modifying within a [`RefCell`].
#[derive(Debug, Default)]
struct Amount(usize);

impl Amount {
    /// Returns the value of `self`.
    const fn value(&self) -> usize {
        self.0
    }

    /// Sets `self` to `amount`.
    fn set(&mut self, amount: usize) {
        self.0 = amount;
    }
}

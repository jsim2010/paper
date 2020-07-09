//! Implements the `paper` application logic for converting an [`Input`] into [`Output`]s.
mod translate;

use {
    crate::io::{Dimensions, DocEdit, File, Input, LanguageId, Output, Purl, Unit},
    log::trace,
    lsp_types::{MessageType, ShowMessageParams, ShowMessageRequestParams},
    std::{cell::RefCell, mem, rc::Rc},
    translate::{Command, DocOp, Interpreter, Operation},
};

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

        match operation {
            Operation::Resize { dimensions } => {
                self.pane.update_size(dimensions, &mut outputs);
            }
            Operation::Confirm(action) => {
                outputs.push(Output::Question {
                    request: ShowMessageRequestParams::from(action),
                });
            }
            Operation::Reset => {
                self.input.clear();
                self.pane.update(&mut outputs);
            }
            Operation::StartCommand(command) => {
                let prompt = command.to_string();

                self.command = Some(command);
                outputs.push(Output::Command { command: prompt });
            }
            Operation::Collect(ch) => {
                self.input.push(ch);
                outputs.push(Output::Command {
                    command: self.input.clone(),
                });
            }
            Operation::Execute => {
                if self.command.is_some() {
                    let mut path = String::new();
                    mem::swap(&mut path, &mut self.input);

                    outputs.push(Output::OpenFile { path });
                }
            }
            Operation::Document(doc_op) => {
                outputs.push(self.pane.operate(&doc_op));
            }
            Operation::Quit => {
                if let Some(output) = self.pane.close_doc() {
                    outputs.push(output);
                }

                outputs.push(Output::Quit);
            }
            Operation::CreateDoc(file) => {
                outputs.append(&mut self.pane.create_doc(file));
            }
            Operation::SendLsp(message) => {
                outputs.push(Output::SendLsp(message));
            }
        };

        outputs.push(Output::UpdateHeader);
        trace!("outputs: {:?}", outputs);

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
    /// The length of a row.
    row_length: Unit,
    /// The [`Dimensions`] of the pane.
    size: Dimensions,
}

impl Pane {
    /// Updates `self`.
    fn update(&mut self, outputs: &mut Vec<Output>) {
        if let Some(doc) = &mut self.doc {
            outputs.push(doc.change_output());
        }
    }

    /// Updates the size of `self` to match `dimensions`;
    fn update_size(&mut self, dimensions: Dimensions, outputs: &mut Vec<Output>) {
        self.size = dimensions;
        self.scroll_amount
            .borrow_mut()
            .set(usize::from(dimensions.height.wrapping_div(3)));

        if let Some(doc) = &mut self.doc {
            doc.dimensions = dimensions;
            outputs.push(doc.change_output());
        }
    }

    /// Performs `operation` on `self`.
    fn operate(&mut self, operation: &DocOp) -> Output {
        if let Some(doc) = &mut self.doc {
            match operation {
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
    fn create_doc(&mut self, file: File) -> Vec<Output> {
        let mut outputs = Vec::new();
        let doc = Document::new(file, self.size);
        let output = doc.open_output();

        if let Some(old_doc) = self.doc.replace(doc) {
            outputs.push(old_doc.close());
        }

        outputs.push(output);
        outputs
    }

    /// Returns the [`Output`] to close the [`Document`] of `self`.
    fn close_doc(&mut self) -> Option<Output> {
        self.doc.take().map(Document::close)
    }
}

/// A file and the user's current interactions with it.
#[derive(Clone, Debug)]
pub(crate) struct Document {
    /// The file of the document.
    file: File,
    /// The [`Dimensions`] of the Document.
    dimensions: Dimensions,
    /// The version of the document.
    version: i64,
}

impl Document {
    /// Creates a new [`Document`].
    const fn new(file: File, dimensions: Dimensions) -> Self {
        Self {
            file,
            dimensions,
            version: 0,
        }
    }

    /// Returns the [`Output`] for opening `self`.
    fn open_output(&self) -> Output {
        Output::EditDoc {
            doc: self.clone(),
            edit: DocEdit::Open {
                version: self.version,
            },
        }
    }

    /// Returns the [`Output`] for changing `self`.
    fn change_output(&mut self) -> Output {
        Output::EditDoc {
            doc: self.clone(),
            edit: DocEdit::Update,
        }
    }

    /// Saves the document.
    fn save(&self) -> Output {
        Output::EditDoc {
            doc: self.clone(),
            edit: DocEdit::Save,
        }
    }

    /// Returns the [`Purl`] of `self`.
    pub(crate) const fn url(&self) -> &Purl {
        self.file.url()
    }

    /// Returns the [`LanguageId`] of `self`.
    pub(crate) fn language_id(&self) -> Option<LanguageId> {
        self.file.language_id()
    }

    /// Returns the text of `self`.
    pub(crate) fn text(&self) -> String {
        self.file.text().to_string()
    }

    /// Returns a [`Vec`] of the rows of `self`.
    pub(crate) fn rows(&self) -> Vec<String> {
        let mut rows = Vec::new();
        let row_length = (*self.dimensions.width).into();

        for line in self.file.lines() {
            if line.len() <= row_length {
                rows.push(line.to_string());
            } else {
                let mut line_remainder = line;

                while line_remainder.len() > row_length {
                    let (row, remainder) = line_remainder.split_at(row_length);
                    line_remainder = remainder;
                    rows.push(row.to_string());
                }

                rows.push(line_remainder.to_string());
            }
        }

        rows.into_iter()
            .take((*self.dimensions.height).into())
            .collect()
    }

    /// Returns the output to close `self`.
    fn close(self) -> Output {
        Output::EditDoc {
            doc: self,
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
    /// Sets `self` to `amount`.
    fn set(&mut self, amount: usize) {
        self.0 = amount;
    }
}

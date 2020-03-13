//! Implements the application logic of `paper`, converting an [`Input`] into a list of [`Output`]s.
mod translate;

use {
    // TODO: Move everything out of ui.
    crate::io::{
        config::Setting,
        ui::{BodySize, Selection},
        DocEdit, Input, Output, PathUrl,
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
    pub(crate) fn process(&mut self, input: Input) -> Vec<Output<'_>> {
        if let Some(operation) = self.interpreter.translate(input) {
            self.operate(operation)
        } else {
            Vec::new()
        }
    }

    /// Performs `operation` and returns the appropriate [`Display`]s.
    pub(crate) fn operate(&mut self, operation: Operation) -> Vec<Output<'_>> {
        let mut outputs = Vec::new();
        // Retrieve here to avoid error. This will not work once changes start modifying the working dir.
        match operation {
            Operation::UpdateSetting(setting) => {
                outputs.append(&mut self.update_setting(setting));
            }
            Operation::Size(size) => {
                trace!("resize {:?}", size);
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
                        .map_or(&EMPTY_SELECTION, |doc| &doc.selection),
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
            Operation::OpenDoc { url, text } => {
                outputs.append(&mut self.pane.open_doc(url, text));
            }
        };

        outputs.push(Output::UpdateHeader);
        trace!("output {:?}", outputs);

        outputs
    }

    /// Updates `self` based on `setting`.
    fn update_setting(&mut self, setting: Setting) -> Vec<Output<'_>> {
        let mut outputs = Vec::new();

        match setting {
            Setting::Wrap(is_wrapped) => {
                trace!("setting wrap to `{}`", is_wrapped);
                outputs.push(Output::Wrap {
                    is_wrapped,
                    selection: self
                        .pane
                        .doc
                        .as_ref()
                        .map_or(&EMPTY_SELECTION, |doc| &doc.selection),
                });
            }
            Setting::StarshipLog(starship_level) => {
                trace!("updating starship log level to `{}`", starship_level);
                outputs.push(Output::Log { starship_level });
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
    fn operate(&mut self, operation: DocOp) -> Output<'_> {
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
    fn open_doc(&mut self, url: PathUrl, text: String) -> Vec<Output<'_>> {
        let mut outputs = Vec::new();
        let doc = self.create_doc(url, text);
        if let Some(old_doc) = self.doc.replace(doc) {
            outputs.push(old_doc.close());
        }

        #[allow(clippy::option_expect_used)] // Replace guarantees that self.doc is Some.
        outputs.push(
            self.doc
                .as_ref()
                .map(|doc| Output::EditDoc {
                    url: doc.path.clone(),
                    edit: Box::new(DocEdit::Open {
                        url: doc.path.clone(),
                        version: doc.version,
                        text: &doc.text,
                    }),
                })
                .expect("retrieving `Document` in `Pane`"),
        );
        outputs
    }

    /// Creates a [`Document`] from `path`.
    fn create_doc(&mut self, url: PathUrl, text: String) -> Document {
        Document::new(url, text, &self.scroll_amount)
    }

    /// Updates the size of `self` to match `size`;
    fn update_size(&mut self, size: BodySize) -> Output<'_> {
        self.scroll_amount
            .borrow_mut()
            .set(usize::from(size.0.rows.wrapping_div(3)));
        Output::Resize { size }
    }

    /// Returns the [`Output`] to close the [`Document`] of `self`.
    fn close_doc(&mut self) -> Option<Output<'_>> {
        self.doc.take().map(Document::close)
    }
}

/// A file and the user's current interactions with it.
#[derive(Debug)]
struct Document {
    /// The path of the document.
    path: PathUrl,
    /// The text of the document.
    text: String,
    /// The current user selection.
    selection: Selection,
    /// The number of lines that a scroll will move.
    scroll_amount: Rc<RefCell<Amount>>,
    /// The version of the text.
    version: i64,
}

impl Document {
    /// Creates a new [`Document`].
    fn new(path: PathUrl, text: String, scroll_amount: &Rc<RefCell<Amount>>) -> Self {
        let mut selection = Selection::default();

        if !text.is_empty() {
            selection.init();
        }

        Self {
            path,
            text,
            selection,
            scroll_amount: Rc::clone(scroll_amount),
            version: 0,
        }
    }

    /// Saves the document.
    fn save(&self) -> Output<'_> {
        Output::EditDoc {
            url: self.path.clone(),
            edit: Box::new(DocEdit::Save { text: &self.text }),
        }
    }

    /// Deletes the text of the [`Selection`].
    fn delete_selection(&mut self) -> Output<'_> {
        let mut newline_indices = self.text.match_indices('\n');
        let start_line = self.selection.start_line();
        if let Some(start_index) = if start_line == 0 {
            Some(0)
        } else {
            newline_indices
                .nth(start_line.saturating_sub(1))
                .map(|index| index.0.saturating_add(1))
        } {
            if let Some((end_index, ..)) = newline_indices.nth(
                self.selection
                    .end_line()
                    .saturating_sub(start_line.saturating_add(1)),
            ) {
                let _ = self.text.drain(start_index..=end_index);
            }
        }
        self.version = self.version.wrapping_add(1);
        Output::EditDoc {
            url: self.path.clone(),
            edit: Box::new(DocEdit::Change {
                new_text: String::new(),
                selection: &self.selection,
                version: self.version,
                text: &self.text,
            }),
        }
    }

    /// Returns the number of lines in `self`.
    fn line_count(&self) -> usize {
        self.text.lines().count()
    }

    /// Moves the [`Selection`] as described by [`Vector`].
    fn move_selection(&mut self, vector: &Vector) -> Output<'_> {
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
            selection: &self.selection,
        }
    }

    /// Returns the output to close `self`.
    fn close(self) -> Output<'static> {
        Output::EditDoc {
            url: self.path,
            edit: Box::new(DocEdit::Close),
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

//! Implements the `paper` application logic for converting an [`Input`] into [`Output`]s.
pub(crate) mod translate;

use {
    crate::{
        io::{Dimensions, DocEdit, File, Input, Output},
        orient,
    },
    log::trace,
    lsp_types::{ShowMessageRequestParams, TextDocumentIdentifier, TextDocumentItem},
    translate::{Interpreter, Operation},
    url::Url,
};

/// The processor of the application.
#[derive(Debug, Default)]
pub(crate) struct Processor {
    /// The currently visible pane.
    pane: Pane,
    /// The current command.
    command: String,
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
        self.interpreter
            .translate(input)
            .map_or_else(Vec::new, |operation| self.operate(operation))
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
                self.command.clear();
                self.pane.update(&mut outputs);
            }
            Operation::StartCommand => {
                self.command = ":".to_string();
                outputs.push(Output::Command {
                    command: self.command.clone(),
                });
            }
            Operation::Collect(ch) => {
                self.command.push(ch);
                outputs.push(Output::Command {
                    command: self.command.clone(),
                });
            }
            Operation::Execute => {
                if let Some(path) = self.command.strip_prefix(":open ") {
                    outputs.push(Output::OpenFile {
                        path: path.to_string(),
                    });
                }
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
            Operation::Scroll(direction) => {
                if let Some(output) = self.pane.scroll(direction) {
                    outputs.push(output);
                }
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
    /// The [`Dimensions`] of the pane.
    size: Dimensions,
}

impl Pane {
    /// Updates `self`.
    fn update(&mut self, outputs: &mut Vec<Output>) {
        if let Some(doc) = self.doc.as_mut() {
            outputs.push(doc.change_output());
        }
    }

    /// Updates the size of `self` to match `dimensions`;
    fn update_size(&mut self, dimensions: Dimensions, outputs: &mut Vec<Output>) {
        self.size = dimensions;

        if let Some(doc) = self.doc.as_mut() {
            doc.dimensions = dimensions;
            outputs.push(doc.change_output());
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

    /// Scrolls `self` towards `direction`.
    fn scroll(&mut self, direction: orient::ScreenDirection) -> Option<Output> {
        self.doc.as_mut().map(|doc| {
            doc.scroll(direction);
            Output::UpdateView { rows: doc.rows() }
        })
    }
}

/// A file and the user's current interactions with it.
#[derive(Clone, Debug)]
pub(crate) struct Document {
    /// The file of the document.
    file: File,
    /// The start and end indices within the text for each row.
    row_indices: Vec<(usize, usize)>,
    /// The [`Dimensions`] of the screen showing the document.
    dimensions: Dimensions,
    /// The first row that is visible.
    first_visible_row: usize,
    /// The last row that is visible.
    max_visible_row: usize,
    /// The version of the document.
    version: i32,
}

impl Document {
    /// Creates a new [`Document`].
    fn new(file: File, dimensions: Dimensions) -> Self {
        let max_length = usize::from(dimensions.width);
        let mut prev_index = 0;
        let mut row_end_index = max_length;
        let mut row_indices = Vec::new();
        let text = file.text();

        for (index, _) in text.match_indices('\n') {
            let mut end_index = index;
            let before_end_index = end_index.saturating_sub(1);

            if text.get(before_end_index..end_index) == Some("\r") {
                end_index = before_end_index;
            }

            while row_end_index < end_index {
                row_indices.push((prev_index, row_end_index));
                prev_index = row_end_index;
                row_end_index = row_end_index.saturating_add(max_length);
            }

            row_indices.push((prev_index, end_index));
            prev_index = index.saturating_add(1);
            row_end_index = prev_index.saturating_add(max_length);
        }

        let text_length = text.len();

        while row_end_index < text_length {
            row_indices.push((prev_index, row_end_index));
            prev_index = row_end_index;
            row_end_index = row_end_index.saturating_add(max_length);
        }

        row_indices.push((prev_index, text_length));

        Self {
            max_visible_row: row_indices.len().saturating_sub(dimensions.height.into()),
            file,
            row_indices,
            dimensions,
            first_visible_row: 0,
            version: 0,
        }
    }

    /// Scrolls `self` towards `direction`.
    fn scroll(&mut self, direction: orient::ScreenDirection) {
        if let Some(vertical_direction) = direction.vertical_direction() {
            self.first_visible_row = match vertical_direction {
                orient::AxialDirection::Positive => self.first_visible_row.saturating_add(5),
                orient::AxialDirection::Negative => self.first_visible_row.saturating_sub(5),
            };

            if self.first_visible_row > self.max_visible_row {
                self.first_visible_row = self.max_visible_row;
            }
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

    /// Returns the [`Purl`] of `self`.
    pub(crate) const fn url(&self) -> &Url {
        self.file.url()
    }

    /// Returns a [`Vec`] of the rows of `self`.
    pub(crate) fn rows(&self) -> Vec<String> {
        self.row_indices
            .iter()
            .skip(self.first_visible_row)
            .take((*self.dimensions.height).into())
            .map(|&(start, end)| {
                self.file
                    .text()
                    .get(start..end)
                    .map(ToString::to_string)
                    .unwrap_or_default()
            })
            .collect()
    }

    /// Returns the output to close `self`.
    const fn close(self) -> Output {
        Output::EditDoc {
            doc: self,
            edit: DocEdit::Close,
        }
    }
}

impl From<Document> for TextDocumentItem {
    #[inline]
    fn from(value: Document) -> Self {
        Self::new(
            value.url().clone(),
            value.file.language().to_string(),
            value.version,
            value.file.text().to_string(),
        )
    }
}

impl From<Document> for TextDocumentIdentifier {
    #[inline]
    fn from(value: Document) -> Self {
        Self::new(value.url().clone())
    }
}

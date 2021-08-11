//! Implements the `paper` application logic for converting an [`Input`] into [`Output`]s.
pub(crate) mod translate;

use {
    crate::{
        io::{Dimensions, File, Input, Output, RowText, Style, StyledText, Unit},
        orient,
    },
    core::{
        convert::{TryFrom, TryInto},
        num::TryFromIntError,
        slice::Iter,
    },
    fehler::throws,
    log::trace,
    lsp_types::{
        DocumentSymbol, Position, Range, ShowMessageRequestParams, TextDocumentIdentifier,
        TextDocumentItem,
    },
    translate::{Interpreter, Operation, SelectionMovement},
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
    #[throws(ScopeFromRangeError)]
    pub(crate) fn process(&mut self, input: Input) -> Vec<Output> {
        self.interpreter
            .translate(input)
            .map_or_else(|| Ok(Vec::new()), |operation| self.operate(operation))?
    }

    /// Performs `operation` and returns the appropriate [`Output`]s.
    #[throws(ScopeFromRangeError)]
    pub(crate) fn operate(&mut self, operation: Operation) -> Vec<Output> {
        let mut outputs = Vec::new();

        match operation {
            Operation::Resize { dimensions } => {
                self.pane.update_size(dimensions, &mut outputs)?;
            }
            Operation::Confirm(action) => {
                outputs.push(Output::Question {
                    request: ShowMessageRequestParams::from(action),
                });
            }
            Operation::Reset => {
                self.command.clear();
                self.pane.update(&mut outputs)?;
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
                    outputs.push(Output::ReadFile {
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
                outputs.append(&mut self.pane.create_doc(file)?);
            }
            Operation::Scroll(direction) => {
                if let Some(output) = self.pane.scroll(direction)? {
                    outputs.push(output);
                }
            }
            Operation::ChangeSelection(movement) => {
                if let Some(output) = self.pane.change_selection(&movement)? {
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
    #[throws(ScopeFromRangeError)]
    fn update(&mut self, outputs: &mut Vec<Output>) {
        if let Some(doc) = self.doc.as_mut() {
            outputs.push(doc.change_output()?);
        }
    }

    /// Updates the size of `self` to match `dimensions`;
    #[throws(ScopeFromRangeError)]
    fn update_size(&mut self, dimensions: Dimensions, outputs: &mut Vec<Output>) {
        self.size = dimensions;

        if let Some(doc) = self.doc.as_mut() {
            doc.dimensions = dimensions;
            outputs.push(doc.change_output()?);
        }
    }

    /// Opens a document at `path`.
    #[throws(ScopeFromRangeError)]
    fn create_doc(&mut self, file: File) -> Vec<Output> {
        let mut outputs = Vec::new();
        let mut doc = Document::new(file, self.size)?;
        let open_output = doc.open_output()?;
        let view_output = doc.change_output()?;

        if let Some(old_doc) = self.doc.replace(doc) {
            outputs.push(old_doc.close());
        }

        outputs.push(open_output);
        outputs.push(view_output);
        outputs
    }

    /// Change selection of `self` as described by `movement`.
    #[throws(ScopeFromRangeError)]
    fn change_selection(&mut self, movement: &SelectionMovement) -> Option<Output> {
        self.doc
            .as_mut()
            .map(|doc| {
                doc.change_selection(movement)?;
                doc.rows().map(|rows| Output::UpdateView { rows })
            })
            .transpose()?
    }

    /// Returns the [`Output`] to close the [`Document`] of `self`.
    fn close_doc(&mut self) -> Option<Output> {
        self.doc.take().map(Document::close)
    }

    /// Scrolls `self` towards `direction`.
    #[throws(ScopeFromRangeError)]
    fn scroll(&mut self, direction: orient::ScreenDirection) -> Option<Output> {
        self.doc
            .as_mut()
            .map(|doc| {
                doc.scroll(direction);
                doc.rows().map(|rows| Output::UpdateView { rows })
            })
            .transpose()?
    }
}

/// A [`Range`] of text that can be selected.
#[derive(Clone, Debug)]
struct Symbol {
    /// The [`Range`] of `Self`.
    range: Range,
    /// The children [`Symbol`]s of `Self`.
    children: Vec<Symbol>,
}

impl Symbol {
    /// Creates the default root [`Symbol`].
    #[throws(OverflowError)]
    fn create_root(text: &str, last_line_length: u32) -> Self {
        u32::try_from(text.lines().count()).and_then(|line_count| {
            Ok(Self {
                range: Range::new(
                    Position::new(0, 0),
                    Position::new(line_count.saturating_sub(1), last_line_length),
                ),
                children: (0_u32..)
                    .zip(text.lines())
                    .map(|(line, line_text)| {
                        line_text.len().try_into().map(|line_len| Self {
                            range: Range::new(
                                Position::new(line, 0),
                                Position::new(line, line_len),
                            ),
                            children: (0_u32..0)
                                .zip(line_text.chars())
                                .map(|(character, _)| Self {
                                    range: Range::new(
                                        Position::new(line, character),
                                        Position::new(line, character.saturating_add(1)),
                                    ),
                                    children: Vec::new(),
                                })
                                .collect(),
                        })
                    })
                    .collect::<Result<Vec<Self>, _>>()?,
            })
        })?
    }
}

impl From<DocumentSymbol> for Symbol {
    fn from(symbol: DocumentSymbol) -> Self {
        let mut children = Vec::new();

        if let Some(symbol_children) = symbol.children {
            for child in symbol_children.into_iter().rev() {
                children.push(child.into());
            }
        }

        Self {
            range: symbol.range,
            children,
        }
    }
}

/// A line in a [`Document`].
#[derive(Clone, Debug)]
struct Line {
    /// The index within the [`Document`] of the first row of `Self`.
    first_row: Row,
    /// The start and end indexes of each row in `Self`.
    rows: Vec<(usize, usize)>,
}

impl Line {
    /// Creates a new [`Line`].
    fn new(first_row: Row, rows: Vec<(usize, usize)>) -> Self {
        Self { first_row, rows }
    }
}

/// A file and the user's current interactions with it.
#[derive(Clone, Debug)]
pub(crate) struct Document {
    /// The file of the document.
    file: File,
    /// The lines in `Self`.
    lines: U32Vec<Line>,
    /// The [`Dimensions`] of the screen showing the document.
    dimensions: Dimensions,
    /// The first row that is visible.
    first_visible_row: Row,
    /// The last row that is visible.
    max_visible_row: Row,
    /// The version of the document.
    version: i32,
    /// The root of all [`Symbol`]s in `Self`.
    root_symbol: Symbol,
    /// Describes the [`Symbol`] that is selected.
    selection: Vec<usize>,
}

impl Document {
    /// Creates a new [`Document`].
    #[throws(OverflowError)]
    fn new(file: File, dimensions: Dimensions) -> Self {
        let max_length = usize::from(dimensions.width);
        let mut prev_index = 0;
        let mut row_end_index = max_length;
        let mut lines = U32Vec::new();
        let mut row_count: u64 = 0;
        let text = file.text();

        for (index, _) in text.match_indices('\n') {
            let first_row = row_count;
            let mut row_indices = Vec::new();
            let mut end_index = index;
            let before_end_index = end_index.saturating_sub(1);

            if text.get(before_end_index..end_index) == Some("\r") {
                end_index = before_end_index;
            }

            while row_end_index < end_index {
                row_indices.push((prev_index, row_end_index));
                row_count = row_count.saturating_add(1);
                prev_index = row_end_index;
                row_end_index = row_end_index.saturating_add(max_length);
            }

            row_indices.push((prev_index, end_index));
            row_count = row_count.saturating_add(1);
            prev_index = index.saturating_add(1);
            row_end_index = prev_index.saturating_add(max_length);
            lines.push(Line::new(Row(first_row), row_indices));
        }

        let first_row = row_count;
        let mut row_indices = Vec::new();
        let text_length = text.len();
        let last_line_length = u32::try_from(text_length.saturating_sub(prev_index))?;

        while row_end_index < text_length {
            row_indices.push((prev_index, row_end_index));
            row_count = row_count.saturating_add(1);
            prev_index = row_end_index;
            row_end_index = row_end_index.saturating_add(max_length);
        }

        row_indices.push((prev_index, text_length));
        row_count = row_count.saturating_add(1);
        lines.push(Line::new(Row(first_row), row_indices));

        Self {
            max_visible_row: Row(row_count.saturating_sub(dimensions.height.into())),
            lines,
            dimensions,
            first_visible_row: Row(0),
            version: 0,
            root_symbol: Symbol::create_root(text, last_line_length)?,
            file,
            selection: Vec::new(),
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
    #[throws(ScopeFromRangeError)]
    fn open_output(&self) -> Output {
        Output::OpenDoc {
            doc: self.clone().into(),
        }
    }

    /// Returns the [`Output`] for changing `self`.
    #[throws(ScopeFromRangeError)]
    fn change_output(&mut self) -> Output {
        Output::UpdateView {
            rows: self.rows()?,
        }
    }

    /// Returns the [`Purl`] of `self`.
    pub(crate) const fn url(&self) -> &Url {
        self.file.url()
    }

    /// Converts `character` into an [`Address`] relative to the first character of the line.
    #[throws(DivideByZeroError)]
    fn relative_address_from_character(&self, character: Character) -> Address {
        Address {
            row: character.try_div(self.dimensions.width)?,
            column: usize::from(character.try_rem(self.dimensions.width)?),
        }
    }

    /// Converts `range` into a [`Scope`].
    #[throws(ScopeFromRangeError)]
    fn scope_from_range(&self, range: &Range) -> Scope {
        let start_character_relative_address =
            self.relative_address_from_character(range.start.character.into())?;
        let end_character_relative_address =
            self.relative_address_from_character(range.end.character.into())?;

        Scope {
            start: Address {
                row: self
                    .lines
                    .get(range.start.line)?
                    .first_row
                    .try_add(start_character_relative_address.row)?,
                column: start_character_relative_address.column,
            },
            end: Address {
                row: self
                    .lines
                    .get(range.end.line)?
                    .first_row
                    .try_add(end_character_relative_address.row)?,
                column: end_character_relative_address.column,
            },
        }
    }

    /// Returns a [`Vec`] of the rows of `self`.
    #[throws(ScopeFromRangeError)]
    pub(crate) fn rows(&self) -> Vec<RowText> {
        let selection_scope = self.scope_from_range(&self.selected_symbol()?.range)?;
        (0_u64..)
            .map(Row)
            .zip(self.lines.iter().flat_map(|line| line.rows.iter()))
            .skip(self.first_visible_row.try_into()?)
            .take((*self.dimensions.height).into())
            .map(|(row, &(start, end))| {
                let text = self.file.text().get(start..end).unwrap_or_default();
                let mut styled_texts = Vec::new();

                if row < selection_scope.start.row || row > selection_scope.end.row {
                    styled_texts.push(StyledText::new(text.to_string(), Style::Default));
                } else {
                    if row == selection_scope.start.row && selection_scope.start.column > 0 {
                        styled_texts.push(StyledText::new(
                            text.get(..selection_scope.start.column)
                                .map(ToString::to_string)
                                .unwrap_or_default(),
                            Style::Default,
                        ));
                    }

                    styled_texts.push(StyledText::new(
                        text.get(
                            if row == selection_scope.start.row {
                                selection_scope.start.column
                            } else {
                                0
                            }..if row == selection_scope.end.row {
                                selection_scope.end.column
                            } else {
                                text.len()
                            },
                        )
                        .map(ToString::to_string)
                        .unwrap_or_default(),
                        Style::Selection,
                    ));

                    if row == selection_scope.end.row && selection_scope.end.column < text.len() {
                        styled_texts.push(StyledText::new(
                            text.get(selection_scope.end.column..)
                                .map(ToString::to_string)
                                .unwrap_or_default(),
                            Style::Default,
                        ));
                    }
                }

                RowText::new(styled_texts)
            })
            .collect()
    }

    /// Returns the output to close `self`.
    fn close(self) -> Output {
        Output::CloseDoc { doc: self.into() }
    }

    /// Returns the selected symbol.
    #[throws(OutOfBoundsError)]
    fn selected_symbol(&self) -> &Symbol {
        let mut symbol = &self.root_symbol;

        for index in &self.selection {
            symbol = symbol.children.get(*index).ok_or(OutOfBoundsError)?;
        }

        symbol
    }

    /// Changes the current selection as specified by `movement`.
    #[throws(OutOfBoundsError)]
    fn change_selection(&mut self, movement: &SelectionMovement) {
        log::trace!("Move selection {:?}", movement);

        match *movement {
            SelectionMovement::Descend => {
                if !self.selected_symbol()?.children.is_empty() {
                    self.selection.push(0);
                }
            }
            SelectionMovement::Ascend => {
                if self.selection.pop().is_none() {
                    // TODO: Alert user that selection is already at root symbol.
                }
            }
            SelectionMovement::Increment => {
                if let Some(index) = self.selection.pop() {
                    self.selection.push(std::cmp::min(
                        index.saturating_add(1),
                        self.selected_symbol()?.children.len().saturating_sub(1),
                    ));
                }
            }
            SelectionMovement::Decrement => {
                if let Some(index) = self.selection.pop() {
                    self.selection.push(index.saturating_sub(1));
                }
            }
        }

        log::trace!("Selection {:?}", self.selected_symbol()?.range);
    }
}

impl From<Document> for TextDocumentItem {
    #[inline]
    fn from(value: Document) -> Self {
        Self::new(
            value.url().clone(),
            value
                .file
                .language()
                .map_or(String::new(), |language| language.to_string()),
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

/// All addresses between two addresses.
struct Scope {
    /// The start [`Address`].
    start: Address,
    /// The end.
    end: Address,
}

/// An address within a [`Document`].
struct Address {
    /// The row.
    row: Row,
    /// The column.
    column: usize,
}

/// An index of rows in a [`Document`].
#[derive(Clone, Copy, PartialEq, PartialOrd, Debug)]
struct Row(u64);

impl Row {
    /// Adds `rhs` to `self`.
    ///
    /// # Error(s)
    ///
    /// If result of addition overflows the bounds of [`Row`], throws [`OverflowError`].
    #[throws(OverflowError)]
    fn try_add(self, rhs: Self) -> Self {
        self.0.checked_add(rhs.0).map(Self).ok_or(OverflowError)?
    }

    /// Adds `rhs` to `self`, saturating at [`u64::MAX`].
    const fn saturating_add(self, rhs: u64) -> Self {
        Self(self.0.saturating_add(rhs))
    }

    /// Subtracts `rhs` from `self`, saturating at 0.
    const fn saturating_sub(self, rhs: u64) -> Self {
        Self(self.0.saturating_sub(rhs))
    }
}

impl TryFrom<Row> for usize {
    type Error = OverflowError;

    #[inline]
    fn try_from(row: Row) -> Result<Self, Self::Error> {
        Ok(row.0.try_into()?)
    }
}

/// An index of a single line of text.
#[derive(Clone, Copy)]
struct Character(u32);

impl Character {
    /// Returns the value from dividing `self` by `rhs`.
    #[throws(DivideByZeroError)]
    fn try_div(self, rhs: Unit) -> Row {
        self.0
            .checked_div(u32::from(rhs))
            .map(|result| Row(u64::from(result)))
            .ok_or(DivideByZeroError)?
    }

    /// Returns the remainder of dividing `self` by `rhs`.
    #[throws(DivideByZeroError)]
    fn try_rem(self, rhs: Unit) -> Unit {
        #[allow(clippy::unwrap_used)] // rhs <= u16::MAX, thus result <= u16::MAX.
        self.0
            .checked_rem(u32::from(rhs))
            .map(|result| Unit::from(u16::try_from(result).unwrap()))
            .ok_or(DivideByZeroError)?
    }
}

impl From<u32> for Character {
    fn from(value: u32) -> Self {
        Self(value)
    }
}

/// Error converting a [`Range`] into a [`Scope`].
#[derive(Copy, Clone, Debug, thiserror::Error)]
pub enum ScopeFromRangeError {
    /// Attempting to increment past the highest value.
    #[error(transparent)]
    Overflow(#[from] OverflowError),
    /// Attempting to access element outside of array.
    #[error(transparent)]
    OutOfBounds(#[from] OutOfBoundsError),
    /// Attempting to divide by zero.
    #[error(transparent)]
    DivideByZero(#[from] DivideByZeroError),
}

/// Error when incrementing past the highest value.
#[derive(Clone, Copy, Debug, thiserror::Error)]
#[error("Overflow error occurred")]
pub struct OverflowError;

impl From<TryFromIntError> for OverflowError {
    fn from(_: TryFromIntError) -> Self {
        Self
    }
}

/// Error when attempting to access element that is outside the array.
#[derive(Clone, Copy, Debug, thiserror::Error)]
#[error("Out of bounds error occurred")]
pub struct OutOfBoundsError;

/// Error when attempting to divide by zero.
#[derive(Clone, Copy, Debug, thiserror::Error)]
#[error("Division by zero occurred")]
pub struct DivideByZeroError;

#[cfg(any(target_pointer_width = "32", target_pointer_width = "64"))]
/// A wrapper around [`Vec`] where the index is [`u32`].
///
/// Additionally adds some error handling where appropriate.
#[derive(Clone, Debug)]
struct U32Vec<T> {
    /// The underlying [`Vec`].
    vec: Vec<T>,
}

#[cfg(any(target_pointer_width = "32", target_pointer_width = "64"))]
impl<T> U32Vec<T> {
    /// Creates a new empty [`U32Vec`].
    const fn new() -> Self {
        Self { vec: Vec::new() }
    }

    /// Returns a reference to the element at `index`.
    #[allow(clippy::unwrap_in_result)] // usize::try_from(u32) will always pass due to cfg attribute on target_pointer_width.
    #[throws(OutOfBoundsError)]
    fn get(&self, index: u32) -> &T {
        #[allow(clippy::unwrap_used)]
        // usize::try_from(u32) will always pass due to cfg attribute on target_pointer_width.
        self.vec
            .get(usize::try_from(index).unwrap())
            .ok_or(OutOfBoundsError)?
    }

    /// Returns the [`Iter`] of `self`.
    fn iter(&self) -> Iter<'_, T> {
        self.vec.iter()
    }

    /// Adds `value` to the end of `self`.
    fn push(&mut self, value: T) {
        self.vec.push(value);
    }
}

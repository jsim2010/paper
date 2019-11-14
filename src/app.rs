//! Implements the modality of the application.
use crate::{
    file::Explorer,
    ui::{Address, Change, Color, Index, Span, BACKSPACE, ENTER},
    Alert, Mode, Failure
};
use core::{
    convert::TryFrom,
    num::{NonZeroUsize, TryFromIntError},
};
use either::Either;
use std::{
    cmp,
    fmt::{self, Debug, Display, Formatter},
    iter,
    ops::Deref,
    path::PathBuf,
    rc::Rc,
};

use lsp_types::{Position, Range, TextDocumentItem};

/// Defines the type that identifies a line.
///
/// Defined by [`Position`].
type Line = u64;
/// Defines the type that indexes a collection of lines.
///
/// The value of a `LineIndex` is equal to its respective [`Line`].
type LineIndex = usize;
/// Defines a [`Result`] with [`Alert`] as its Error.
pub(crate) type Output<T> = Result<T, Alert>;

/// The control panel of a [`Sheet`].
#[derive(Clone, Debug, Default, Hash)]
pub(crate) struct ControlPanel {
    /// The [`String`] to be edited.
    string: String,
    /// The height of the `Sheet`.
    height: Rc<Index>,
}

impl ControlPanel {
    /// Creates a new `ControlPanel`.
    fn new(height: &Rc<Index>) -> Self {
        Self {
            height: Rc::clone(height),
            string: String::default(),
        }
    }

    /// Returns the `Change`s needed to display the `ControlPanel`.
    fn changes(&self) -> Vec<Change> {
        let row = self.height.sub_one();

        // TODO: Could potentially improve to change only the chars that have been changed.
        vec![Change::Text(
            Span::new(
                Address::new(row, Index::min_value()),
                Address::new(row, Index::max_value()),
            ),
            self.string.clone(),
        )]
    }

    /// Clears the string.
    fn clear(&mut self) {
        self.string.clear();
    }

    /// Adds a character and returns the success of doing so.
    fn add(&mut self, input: char) -> bool {
        if input == BACKSPACE {
            if self.string.pop().is_none() {
                return false;
            }
        } else {
            self.add_non_bs(input);
        }

        true
    }

    /// Adds a character that is not [`BACKSPACE`].
    fn add_non_bs(&mut self, input: char) {
        self.string.push(input);
    }

    /// Adds the input and returns the appropriate `Change`s.
    fn changes_after_add(&mut self, input: char) -> Vec<Change> {
        if self.add(input) {
            self.changes()
        } else {
            self.flash_changes()
        }
    }

    /// Returns the `Change`s needed to flash the user interface.
    fn flash_changes(&self) -> Vec<Change> {
        vec![Change::Flash]
    }
}

impl Deref for ControlPanel {
    type Target = str;

    fn deref(&self) -> &str {
        self.string.deref()
    }
}

/// Signfifies display of the current file.
#[derive(Clone, Debug)]
pub(crate) struct Sheet {
    /// The first line that is displayed in the user interface.
    first_line: Line,
    /// The number of columns needed to display the margin.
    margin_width: u8,
    /// The number of rows visible in the pane.
    height: Rc<Index>,
    /// The number of lines in the document.
    line_count: Line,
    /// The control panel of the `Sheet`.
    control_panel: ControlPanel,
    /// The `Change`s `Sheet` needs to make to update the [`UserInterface`].
    changes: Vec<Change>,
    /// If `Sheet` will clear and redraw on next update.
    will_wipe: bool,
    /// The `Explorer` used by `Sheet`.
    explorer: Explorer,
    /// The document being represented by `Sheet`.
    doc: Option<TextDocumentItem>,
}

impl Sheet {
    /// Creates a new Sheet with a given height.
    pub(crate) fn new(height: Index) -> Result<Self, Failure> {
        let height = Rc::new(height);

        Ok(Self {
            control_panel: ControlPanel::new(&height),
            explorer: Explorer::new()?,
            height,
            first_line: Line::default(),
            margin_width: u8::default(),
            line_count: Line::default(),
            changes: Vec::default(),
            will_wipe: bool::default(),
            doc: None,
        })
    }

    pub(crate) fn control_panel(&self) -> &ControlPanel {
        &self.control_panel
    }

    /// Initializes the `Sheet`.
    pub(crate) fn init(&mut self) -> Result<(), Failure> {
        self.explorer.start()?;
        Ok(())
    }

    /// Returns the `Change`s needed to update `Sheet`.
    pub(crate) fn changes(&mut self) -> Vec<Change> {
        if self.will_wipe {
            self.changes.clear();
            self.changes.push(Change::Clear);

            if let Ok(start_line_number) = LineNumber::try_from(self.first_line) {
                for row in self.visible_rows() {
                    if let Some(line_number) = LineNumber::try_from(row)
                        .ok()
                        .and_then(|addend| start_line_number.checked_add(addend))
                    {
                        if let Some(line_data) = self.clone().line_data(line_number) {
                            self.changes.push(Change::Text(
                                Span::new(
                                    Address::new(row, Index::min_value()),
                                    Address::new(row, Index::max_value()),
                                ),
                                format!(
                                    "{: >width$} {}",
                                    line_number,
                                    line_data,
                                    width = usize::from(self.margin_width)
                                ),
                            ));
                        } else {
                            break;
                        }
                    } else {
                        break;
                    }
                }
            }

            self.will_wipe = false;
        }

        let changes = self.changes.clone();
        self.changes.clear();
        changes
    }

    /// Sets [`Sheet`] to be wiped on the next call to `changes`().
    pub(crate) fn wipe(&mut self) {
        self.will_wipe = true;
    }

    /// Adds the `Change`s to display a notification.
    pub(crate) fn process_notifications(&mut self) {
        // For now, we don't have a good way for notifications to be displayed.
        //if let Some(notification) = self.explorer.receive_notification() {
        //    if let Some(message) = notification.message {
        //        self.changes.push(Change::Text(
        //            Span::new(
        //                Address::new(Index::min_value(), Index::min_value()),
        //                Address::new(Index::min_value(), Index::max_value()),
        //            ),
        //            message,
        //        ));
        //    }
        //}
    }

    /// Resets the [`ControlPanel`].
    pub(crate) fn reset_control_panel(&mut self, id: Option<char>) {
        self.control_panel.clear();

        if let Some(filter_id) = id {
            // TODO: It is assumed that filter_id is not BACKSPACE.
            self.control_panel.add_non_bs(filter_id);
        }

        self.changes.append(&mut self.control_panel.changes());
    }

    /// Adds an input to the control panel.
    pub(crate) fn input_to_control_panel(&mut self, input: char) {
        self.changes
            .append(&mut self.control_panel.changes_after_add(input));
    }

    /// Returns an [`IndexIterator`] of the all visible rows.
    fn visible_rows(&self) -> IndexIterator {
        IndexIterator::new(Index::min_value(), *self.height.deref())
    }

    /// Applies filter highlighting to the given [`Range`]s.
    fn apply_filter(&mut self, noises: &[Range], signals: &[Range]) {
        for row in self.visible_rows() {
            self.changes.push(Change::Format(
                Span::new(
                    Address::new(row, Index::min_value()),
                    Address::new(row, Index::max_value()),
                ),
                Color::Default,
            ));
        }

        for noise in noises {
            if let Some(span) = self.span_at(noise) {
                self.changes.push(Change::Format(span, Color::Blue));
            }
        }

        for signal in signals {
            if let Some(span) = self.span_at(signal) {
                self.changes.push(Change::Format(span, Color::Red));
            }
        }
    }

    /// Changes the pane to a new path.
    pub(crate) fn change(&mut self, path: &PathBuf) -> Output<()> {
        self.doc = Some(self.explorer.read(path)?);
        self.refresh();
        Ok(())
    }

    /// Saves the document of `Sheet` to its file system.
    pub(crate) fn save(&self) -> Output<()> {
        if let Some(doc) = &self.doc {
            self.explorer.write(doc)?;
        }

        Ok(())
    }

    /// Adds a character at a [`Position`].
    pub(crate) fn add(&mut self, position: &mut Position, input: char) -> Output<()> {
        let mut new_text = String::new();
        let mut range = Range::new(*position, *position);

        if input == BACKSPACE {
            if range.start.character == 0 {
                if !range.start.line == 0 {
                    range.start.line -= 1;
                    range.start.character = u64::max_value();
                    self.will_wipe = true;
                    self.refresh();
                }
            } else {
                range.start.character -= 1;

                if let Some(span) = self.span_at(&range) {
                    self.changes.push(Change::Text(span, new_text.clone()));
                }
            }
        } else {
            new_text.push(input);

            if input == ENTER {
                self.will_wipe = true;
                self.refresh();
            } else if let Some(span) = self.span_at(&range) {
                self.changes.push(Change::Text(span, new_text.clone()));
            } else {
                // Do nothing.
            }
        }

        let pointer = self
            .line_indices()
            // TODO: Update the Alert.
            .nth(LineIndex::try_from(range.start.line).map_err(|_| Alert::User)?);

        if let Some(doc) = &mut self.doc {
            if let Some(index) = pointer {
                // TODO: Update the Alert.
                let mut index =
                    u64::try_from(index).map_err(|_| Alert::User)?;
                index += range.start.character;
                // TODO: Update the Alert.
                let data_index =
                    usize::try_from(index).map_err(|_| Alert::User)?;

                if input == BACKSPACE {
                    // TODO: For now, do not care to check what is removed. But this may become important for
                    // multi-byte characters.
                    match doc.text.remove(data_index) {
                        _ => {}
                    }
                    *position = range.start;
                } else {
                    doc.text.insert(data_index, input);
                    position.character += 1;
                }
            }

            self.explorer.change(doc, &range, &new_text)?;
        }

        Ok(())
    }

    /// Iterates through the indexes that indicate where each line starts.
    pub(crate) fn line_indices(&self) -> impl Iterator<Item = Index> + '_ {
        if let Some(doc) = &self.doc {
            Either::Left(iter::once(Index::min_value()).chain(
                doc.text.match_indices(ENTER).flat_map(|(index, _)| {
                    index
                        .checked_add(1)
                        .and_then(|value| Index::try_from(value).ok())
                        .into_iter()
                }),
            ))
        } else {
            Either::Right(iter::empty())
        }
    }

    /// Returns the value signifying the first column at which pane data can be written.
    #[allow(clippy::integer_arithmetic)] // self.margin_width: u8 + 1 < u64.max_value()
    fn origin_character(&self) -> u64 {
        u64::from(self.margin_width) + 1
    }

    /// Returns the row at which a [`Position`] is located.
    ///
    /// [`None`] indicates that the [`Position`] is not visible in the user interface.
    fn row_at(&self, position: &Position) -> Option<Index> {
        position
            .line
            .checked_sub(self.first_line)
            .map(Index::saturating_from_u64)
    }

    /// Returns the column at which a [`Position`] is located.
    fn column_at(&self, position: &Position) -> Index {
        Index::saturating_from_u64(position.character.saturating_add(self.origin_character()))
    }

    /// Returns the [`Address`] associated with the given [`Position`].
    fn address_at(&self, position: Position) -> Option<Address> {
        self.row_at(&position)
            .map(|row| Address::new(row, self.column_at(&position)))
    }

    /// Returns the `Span` associated with the given `Range`.
    fn span_at(&self, range: &Range) -> Option<Span> {
        self.address_at(range.start).and_then(|first| {
            self.address_at(range.end)
                .map(|last| Span::new(first, last))
        })
    }

    /// An [`Iterator`] of all lines in the pane's data.
    fn lines(&self) -> std::str::Lines<'_> {
        if let Some(doc) = &self.doc {
            doc.text.lines()
        } else {
            "".lines()
        }
    }

    /// The data stored at the given line.
    fn line_data(&self, line: LineNumber) -> Option<&str> {
        self.lines().nth(line.row())
    }

    /// Updates the pane's metadata.
    fn refresh(&mut self) {
        self.line_count = self.lines().count() as u64;
        self.update_margin_width()
    }

    /// Updates the margin width of pane.
    #[allow(clippy::cast_possible_truncation)] // usize.log10().ceil() < usize.max_value()
    #[allow(clippy::cast_precision_loss)] // self.line_count is small enough to be precisely represented by f64
    #[allow(clippy::cast_sign_loss)] // self.line_count >= 0, thus log10().ceil() >= 0.0
    fn update_margin_width(&mut self) {
        self.margin_width = (((self.line_count.saturating_add(1)) as f64).log10().ceil()) as u8;
    }

    /// Return the length of scrolling movements.
    fn scroll_delta(&self) -> u64 {
        u64::from(
            self.height
                .checked_div(unsafe { Index::new_unchecked(4) })
                .unwrap_or_else(Index::min_value),
        )
    }

    /// Scrolls the data of `Sheet` up.
    pub(crate) fn scroll_up(&mut self) {
        self.set_first_line(self.first_line.saturating_sub(self.scroll_delta()));
    }

    /// Scrolls the data of `Sheet` down.
    pub(crate) fn scroll_down(&mut self) {
        self.set_first_line(cmp::min(
            self.first_line.saturating_add(self.scroll_delta()),
            Line::try_from(self.line_count.saturating_sub(1)).unwrap_or(Line::max_value()),
        ));
    }

    /// Sets first
    fn set_first_line(&mut self, first_line: Line) {
        if first_line != self.first_line {
            self.first_line = first_line;
            self.will_wipe = true;
        }
    }
}

/// An [`Iterator`] of [`Index`]es.
struct IndexIterator {
    /// The current [`Index`].
    current: Index,
    /// The first [`Index`] that is not valid.
    end: Index,
}

impl IndexIterator {
    /// Creates a new `IndexIterator`.
    const fn new(start: Index, end: Index) -> Self {
        Self {
            current: start,
            end,
        }
    }
}

impl Iterator for IndexIterator {
    type Item = Index;

    fn next(&mut self) -> Option<Self::Item> {
        if self.current == self.end {
            return None;
        }

        let next_index = self.current;
        self.current = self.current.add_one();
        Some(next_index)
    }
}

#[derive(Debug)]
pub enum Operation {
    EnterMode(Mode),
    ResetControlPanel(Option<char>),
    Scroll(Direction),
    DisplayFile(Box<PathBuf>),
    AddToControlPanel(char),
    Save,
    Add(char),
    Quit,
    UserError,
}

#[derive(Clone, Copy, Debug)]
pub enum Direction {
    Up,
    Down,
}

/// Signifies a line number.
#[derive(Copy, Clone, PartialEq, Eq, Ord, PartialOrd, Hash, Debug)]
pub(crate) struct LineNumber(NonZeroUsize);

impl LineNumber {
    /// Converts `LineNumber` to its row index - assuming line number `1` as at row `0`.
    #[allow(clippy::integer_arithmetic)] // self.0 > 0
    pub(crate) const fn row(self) -> usize {
        self.0.get() - 1
    }

    /// Adds `rhs` to `self`.
    // Follows precedent of [`usize::checked_add`].
    fn checked_add(self, rhs: Self) -> Option<Self> {
        self.0
            .get()
            .checked_add(rhs.0.get())
            .map(|sum| Self(unsafe { NonZeroUsize::new_unchecked(sum) }))
    }

    /// Returns `LineNumber` that is `self` moved by `other` lines.
    fn move_by(self, other: isize) -> Result<Self, ()> {
        let addend = match usize::try_from(other) {
            Ok(v) => v,
            Err(_) => usize::try_from(other.abs()).expect("converting `isize::abs()` to `usize`"),
        };
        self.0
            .get()
            .checked_add(addend)
            .ok_or(())
            .map(|sum| Self(unsafe { NonZeroUsize::new_unchecked(sum) }))
    }
}

impl Display for LineNumber {
    #[inline]
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.pad(&format!("{}", self.0))
    }
}

impl Default for LineNumber {
    #[inline]
    fn default() -> Self {
        Self(unsafe { NonZeroUsize::new_unchecked(1) })
    }
}

impl std::str::FromStr for LineNumber {
    type Err = ParseLineNumberError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(Self(unsafe {
            NonZeroUsize::new_unchecked(s.parse::<usize>()?)
        }))
    }
}

impl IntoIterator for LineNumber {
    type Item = Self;
    type IntoIter = LineNumberIterator;

    fn into_iter(self) -> Self::IntoIter {
        LineNumberIterator {
            current: Some(self),
        }
    }
}

impl TryFrom<u64> for LineNumber {
    type Error = TryFromIntError;

    fn try_from(value: u64) -> Result<Self, Self::Error> {
        usize::try_from(value).map(|v| Self(unsafe { NonZeroUsize::new_unchecked(v) }))
    }
}

impl TryFrom<Index> for LineNumber {
    type Error = <Index as TryFrom<usize>>::Error;

    fn try_from(value: Index) -> Result<Self, Self::Error> {
        usize::try_from(value).map(|v| Self(unsafe { NonZeroUsize::new_unchecked(v) }))
    }
}

/// Signifies an [`Iterator`] of [`LineNumber`]s that steps by 1.
pub(crate) struct LineNumberIterator {
    /// The current [`LineNumber`].
    current: Option<LineNumber>,
}

impl Iterator for LineNumberIterator {
    type Item = LineNumber;

    fn next(&mut self) -> Option<Self::Item> {
        let item = self.current;

        if let Some(line) = item {
            self.current = line
                .0
                .get()
                .checked_add(1)
                .map(|sum| LineNumber(unsafe { NonZeroUsize::new_unchecked(sum) }));
        }

        item
    }
}

/// Signifies an error that occurs while parsing a [`LineNumber`] from a [`String`].
#[derive(Debug)]
pub(crate) enum ParseLineNumberError {
    /// The parsed number was not a valid line number.
    InvalidValue,
    /// There was an issue parsing the given string to an integer.
    ParseInt(std::num::ParseIntError),
}

impl std::error::Error for ParseLineNumberError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match *self {
            Self::InvalidValue => None,
            Self::ParseInt(ref err) => Some(err),
        }
    }
}

impl Display for ParseLineNumberError {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match *self {
            Self::InvalidValue => write!(f, "Invalid line number provided."),
            Self::ParseInt(ref err) => write!(f, "{}", err),
        }
    }
}

impl From<std::num::ParseIntError> for ParseLineNumberError {
    fn from(error: std::num::ParseIntError) -> Self {
        Self::ParseInt(error)
    }
}

impl From<TryFromIntError> for ParseLineNumberError {
    fn from(_error: TryFromIntError) -> Self {
        Self::InvalidValue
    }
}

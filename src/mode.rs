//! Implements the modality of the application.
mod action;
mod command;
mod display;
mod edit;
mod filter;

pub(crate) use action::Processor as ActionProcessor;
pub(crate) use command::Processor as CommandProcessor;
pub(crate) use display::Processor as DisplayProcessor;
pub(crate) use edit::Processor as EditProcessor;
pub(crate) use filter::Processor as FilterProcessor;

use crate::storage::{self, Explorer, LspError};
use crate::ui::{self, Address, Change, Edit, Index, IndexType, Length, Region, BACKSPACE, ENTER};
use crate::Mrc;
use std::borrow::Borrow;
use std::cmp::{self, Ordering};
use std::collections::HashMap;
use std::fmt::{self, Debug, Display, Formatter};
use std::hash::{Hash, Hasher};
use std::io;
use std::iter;
use std::ops::{Add, AddAssign, Deref, Shr, ShrAssign, Sub};
use std::path::PathBuf;
use try_from::{TryFrom, TryFromIntError};

/// Defines a [`Result`] with [`Flag`] as its Error.
pub type Output<T> = Result<T, Flag>;

/// An [`IndexType`] with a value of `-1`.
const NEGATIVE_ONE: IndexType = -1;

/// Signifies the name of an application mode.
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
pub enum Name {
    /// Displays the current view.
    Display,
    /// Displays the current command.
    Command,
    /// Displays the current filter expression and highlights the characters that match the filter.
    Filter,
    /// Displays the highlighting that has been selected.
    Action,
    /// Displays the current view along with the current edits.
    Edit,
}

impl Display for Name {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Name::Display => write!(f, "Display"),
            Name::Command => write!(f, "Command"),
            Name::Filter => write!(f, "Filter"),
            Name::Action => write!(f, "Action"),
            Name::Edit => write!(f, "Edit"),
        }
    }
}

impl Default for Name {
    fn default() -> Self {
        Name::Display
    }
}

/// Defines the functionality of a processor of a mode.
pub(crate) trait Processor: Debug {
    /// Enters the application into its mode.
    fn enter(&mut self, initiation: Option<Initiation>) -> Output<Vec<Edit>>;
    /// Generates an [`Operation`] from the given input.
    fn decode(&mut self, input: char) -> Output<Operation>;
}

/// Signifies a function to be performed when the application enters a mode.
///
/// In general, only certain modes can implement certain Initiations; for example: only Filter
/// implements [`StartFilter`].
#[derive(Clone, Eq, PartialEq, Hash, Debug)]
pub enum Initiation {
    /// Sets the view.
    SetView(PathBuf),
    /// Saves the current data of the view.
    Save,
    /// Starts a filter.
    StartFilter(char),
    /// Sets a list of Sections.
    SetSignals(Vec<Section>),
    /// Sets a list of marks.
    Mark(Vec<Mark>),
}

/// An String that is editable within a View.
///
/// Generally this is used to enter commands or filters.
#[derive(Clone, Debug)]
struct EditableString {
    /// The [`String`] to be edited.
    string: String,
}

impl EditableString {
    /// Creates a new `EditableString`.
    fn new() -> Self {
        Self {
            string: String::new(),
        }
    }

    /// Returns the edits needed to write the string.
    fn edits(&self) -> Vec<Edit> {
        vec![Edit::new(
            Region::with_row(0).expect("Accessing region for editable string"),
            Change::Row(self.string.clone()),
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

    /// Adds the input and returns the appropriate user interface edits.
    fn edits_after_add(&mut self, input: char) -> Vec<Edit> {
        if self.add(input) {
            self.edits()
        } else {
            self.flash_edits()
        }
    }

    /// Returns the edits needed to flash the user interface.
    fn flash_edits(&self) -> Vec<Edit> {
        vec![Edit::new(Region::default(), Change::Flash)]
    }
}

impl Deref for EditableString {
    type Target = str;

    fn deref(&self) -> &str {
        self.string.deref()
    }
}

/// Signifies an alert to stop the application.
#[derive(Clone, Copy, Debug)]
pub enum Flag {
    /// An error with the user interface.
    Ui(ui::Error),
    /// An error with an attempt to convert values.
    Conversion(TryFromIntError),
    /// An error with the file interaction.
    File(storage::Error),
    /// An error with the Language Server Protocol.
    Lsp(LspError),
    /// Quits the application.
    ///
    /// This is not actually an error, just a way to kill the application.
    Quit,
}

impl Display for Flag {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Flag::Ui(error) => write!(f, "{}", error),
            Flag::Conversion(error) => write!(f, "{}", error),
            Flag::File(error) => write!(f, "{}", error),
            Flag::Lsp(error) => write!(f, "{}", error),
            Flag::Quit => write!(f, "Quit"),
        }
    }
}

impl From<TryFromIntError> for Flag {
    fn from(error: TryFromIntError) -> Self {
        Flag::Conversion(error)
    }
}

impl From<ui::Error> for Flag {
    fn from(error: ui::Error) -> Self {
        Flag::Ui(error)
    }
}

impl From<LspError> for Flag {
    fn from(error: LspError) -> Self {
        Flag::Lsp(error)
    }
}

impl From<io::Error> for Flag {
    fn from(error: io::Error) -> Self {
        Flag::File(storage::Error::from(error))
    }
}

/// Signfifies the pane of the current file.
#[derive(Clone, Debug, Default)]
pub(crate) struct Pane {
    /// The path that makes up the pane.
    path: PathBuf,
    /// The data.
    data: String,
    /// The first line that is displayed in the ui.
    first_line: LineNumber,
    /// The number of columns needed to display the margin.
    margin_width: usize,
    /// The number of rows visible in the pane.
    height: usize,
    /// The number of lines in the data.
    line_count: usize,
}

impl Pane {
    /// Creates a new Pane with a given height.
    pub(crate) fn new(height: usize) -> Self {
        Self {
            height,
            ..Self::default()
        }
    }

    /// Changes the pane to a new path.
    fn change(&mut self, explorer: &Mrc<dyn Explorer>, path: PathBuf) -> Output<()> {
        self.data = explorer.borrow_mut().read(&path)?;
        self.path = path;
        self.clean();
        Ok(())
    }

    /// Adds a character at a [`Mark`].
    pub(crate) fn add(&mut self, mark: &Mark, c: char) -> Result<(), TryFromIntError> {
        if let Some(index) = mark.pointer.0 {
            let data_index = usize::try_from(index)?;

            if c == BACKSPACE {
                // For now, do not care to check what is removed. But this may become important for
                // multi-byte characters.
                match self.data.remove(data_index) {
                    _ => {}
                }
            } else {
                self.data.insert(data_index.saturating_sub(1), c);
            }
        }

        Ok(())
    }

    /// Iterates through the indexes that indicate where each line starts.
    pub(crate) fn line_indices(&self) -> impl Iterator<Item = IndexType> + '_ {
        iter::once(0).chain(self.data.match_indices(ENTER).flat_map(|(index, _)| {
            index
                .checked_add(1)
                .and_then(|i| IndexType::try_from(i).ok())
                .into_iter()
        }))
    }

    /// Returns the first column at which pane data can be written.
    #[allow(clippy::integer_arithmetic)] // self.margin_width < usize.max_value()
    fn first_data_column(&self) -> Result<Index, TryFromIntError> {
        Index::try_from(self.margin_width + 1)
    }

    /// Returns the [`Address`] associated with the given [`Place`].
    fn address_at(&self, place: Place) -> Option<Address> {
        match Index::try_from(place.line - self.first_line) {
            Ok(row) => self
                .first_data_column()
                .ok()
                .map(|origin| Address::new(row, place.column + origin)),
            _ => None,
        }
    }

    /// Returns the [`Region`] associated with the given [`Area`].
    pub(crate) fn region_at<T: Area>(&self, area: &T) -> Option<Region> {
        self.address_at(area.start())
            .map(|address| Region::new(address, area.length()))
    }

    /// Updates the ui with the pane's current data.
    pub(crate) fn redraw_edits(&self) -> impl Iterator<Item = Edit> + '_ {
        // Clear the screen, then add each row.
        iter::once(Edit::new(Region::default(), Change::Clear)).chain(
            self.first_line
                .into_iter()
                .zip(self.lines().skip(self.first_line.row()))
                .flat_map(move |(line_number, line)| {
                    self.region_at(&Section::line(line_number))
                        .map(|region| {
                            Edit::new(
                                region,
                                Change::Row(format!(
                                    "{:>width$} {}",
                                    line_number,
                                    line,
                                    width = self.margin_width
                                )),
                            )
                        })
                        .into_iter()
                })
                .take(self.height),
        )
    }

    /// An [`Iterator`] of all lines in the pane's data.
    fn lines(&self) -> std::str::Lines<'_> {
        self.data.lines()
    }

    /// The data stored at the given [`LineNumber`].
    pub(crate) fn line(&self, line_number: LineNumber) -> Option<&str> {
        self.lines().nth(line_number.row())
    }

    /// Updates the pane's metadata.
    pub(crate) fn clean(&mut self) {
        self.line_count = self.lines().count();
        self.update_margin_width()
    }

    /// Updates the margin width of pane.
    #[allow(clippy::cast_possible_truncation)] // usize.log10().ceil() < usize.max_value()
    #[allow(clippy::cast_precision_loss)] // self.line_count is small enough to be precisely represented by f64
    #[allow(clippy::cast_sign_loss)] // self.line_count >= 0, thus log10().ceil() >= 0.0
    fn update_margin_width(&mut self) {
        self.margin_width = (((self.line_count.saturating_add(1)) as f64).log10().ceil()) as usize;
    }

    /// Return the length of scrolling movements.
    fn scroll_length(&self) -> Output<IndexType> {
        Ok(IndexType::try_from(
            self.height
                .checked_div(4)
                .ok_or(Flag::Conversion(TryFromIntError::Overflow))?,
        )?)
    }

    /// Scrolls the pane's data.
    pub(crate) fn scroll(&mut self, movement: IndexType) -> IsChanging {
        let new_first_line = cmp::min(
            self.first_line + movement,
            LineNumber::new(self.line_count).unwrap_or_default(),
        );

        if new_first_line == self.first_line {
            false
        } else {
            self.first_line = new_first_line;
            true
        }
    }

    /// The length of the line that has a given [`Place`].
    pub(crate) fn line_length(&self, place: Place) -> Option<Index> {
        self.line(place.line)
            .and_then(|x| Index::try_from(x.len()).ok())
    }
}

/// Represents if the user interface display needs to change.
type IsChanging = bool;

impl Hash for Pane {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.data.hash(state);
        self.first_line.hash(state);
        self.margin_width.hash(state);
        self.line_count.hash(state);
    }
}

impl PartialEq for Pane {
    fn eq(&self, other: &Self) -> bool {
        self.data == other.data
            && self.first_line == other.first_line
            && self.margin_width == other.margin_width
            && self.line_count == other.line_count
    }
}

impl Eq for Pane {}

/// Signifies an action to be performed by the application.
#[derive(Debug)]
pub enum Operation {
    /// Enters a new mode.
    EnterMode(Name, Option<Initiation>),
    /// Edits the user interface.
    EditUi(Vec<Edit>),
    /// Does nothing.
    Noop,
}

/// Signifies a type that can be converted to a [`Region`].
pub(crate) trait Area {
    /// Returns the starting `Place`.
    fn start(&self) -> Place;
    /// Returns the [`Length`].
    fn length(&self) -> Length;
}

/// Signifies the location of a character within a pane.
#[derive(Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Debug, Default)]
pub(crate) struct Place {
    /// The [`LineNumber`] of `Place`.
    line: LineNumber,
    /// The [`Index`] of the column of `Place`.
    column: Index,
}

impl Area for Place {
    fn start(&self) -> Place {
        *self
    }

    fn length(&self) -> Length {
        Length::from(1)
    }
}

impl Shr<IndexType> for Place {
    type Output = Self;

    #[inline]
    fn shr(self, rhs: IndexType) -> Self {
        let mut new_place = self;
        new_place >>= rhs;
        new_place
    }
}

impl ShrAssign<IndexType> for Place {
    #[inline]
    fn shr_assign(&mut self, rhs: IndexType) {
        self.column += rhs;
    }
}

impl Display for Place {
    #[inline]
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "ln {}, idx {}", self.line, self.column)
    }
}

/// Signifies adjacent [`Place`]s.
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug, Default)]
pub struct Section {
    /// The [`Place`] at which `Section` starts.
    start: Place,
    /// The [`Length`] of `Section`.
    length: Length,
}

impl Section {
    /// Creates a new `Section` that signifies an entire line.
    #[inline]
    pub(crate) fn line(line: LineNumber) -> Self {
        Self {
            start: Place {
                line,
                column: Index::from(0),
            },
            length: Length::End,
        }
    }
}

impl Area for Section {
    fn start(&self) -> Place {
        self.start
    }

    fn length(&self) -> Length {
        self.length
    }
}

impl Display for Section {
    #[inline]
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "{}->{}", self.start, self.length)
    }
}

/// The type of the value stored in [`LineNumber`].
type LineNumberType = u32;

/// Signifies a line number.
#[derive(Copy, Clone, PartialEq, Eq, Ord, PartialOrd, Hash, Debug)]
pub(crate) struct LineNumber(LineNumberType);

impl LineNumber {
    /// Creates a new `LineNumber`.
    pub(crate) fn new(value: usize) -> Option<Self> {
        if value == 0 {
            None
        } else {
            LineNumberType::try_from(value).ok().map(Self)
        }
    }

    /// Converts `LineNumber` to its row index - assuming line number `1` as at row `0`.
    #[allow(clippy::integer_arithmetic)] // self.0 > 0
    pub(crate) fn row(self) -> usize {
        (self.0 - 1) as usize
    }
}

impl Add<IndexType> for LineNumber {
    type Output = Self;

    fn add(self, other: IndexType) -> Self::Output {
        #[allow(clippy::integer_arithmetic)] // i64::min_value() <= u32 + i32 <= i64::max_value()
        match usize::try_from(i64::from(self.0) + i64::from(other)) {
            Ok(sum) => Self::new(sum).unwrap_or_default(),
            Err(TryFromIntError::Underflow) => Self::default(),
            Err(TryFromIntError::Overflow) => Self(LineNumberType::max_value()),
        }
    }
}

impl Sub for LineNumber {
    type Output = i64;

    #[allow(clippy::integer_arithmetic)] // self.0 and other.0 <= u32::MAX
    fn sub(self, other: Self) -> Self::Output {
        i64::from(self.0) - i64::from(other.0)
    }
}

impl Display for LineNumber {
    #[inline]
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl Default for LineNumber {
    #[inline]
    fn default() -> Self {
        Self(1)
    }
}

impl std::str::FromStr for LineNumber {
    type Err = ParseLineNumberError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::new(s.parse::<usize>()?).ok_or(ParseLineNumberError::InvalidValue)
    }
}

impl IntoIterator for LineNumber {
    type Item = Self;
    type IntoIter = LineNumberIterator;

    fn into_iter(self) -> Self::IntoIter {
        LineNumberIterator { current: self }
    }
}

/// Signifies an [`Iterator`] of [`LineNumber`]s that steps by 1.
pub(crate) struct LineNumberIterator {
    /// The current [`LineNumber`].
    current: LineNumber,
}

impl Iterator for LineNumberIterator {
    type Item = LineNumber;

    fn next(&mut self) -> Option<Self::Item> {
        let line_number = LineNumber(self.current.0);
        self.current.0 += 1;
        Some(line_number)
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
            ParseLineNumberError::InvalidValue => None,
            ParseLineNumberError::ParseInt(ref err) => Some(err),
        }
    }
}

impl Display for ParseLineNumberError {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match *self {
            ParseLineNumberError::InvalidValue => write!(f, "Invalid line number provided."),
            ParseLineNumberError::ParseInt(ref err) => write!(f, "{}", err),
        }
    }
}

impl From<std::num::ParseIntError> for ParseLineNumberError {
    fn from(error: std::num::ParseIntError) -> Self {
        ParseLineNumberError::ParseInt(error)
    }
}

/// An address and its respective pointer in a pane.
#[derive(Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Debug, Default)]
pub struct Mark {
    /// Pointer in pane that corresponds with mark.
    pointer: Pointer,
    /// Place of mark.
    place: Place,
}

impl Mark {
    /// Moves `Mark` as specified by the given [`Adjustment`].
    pub(crate) fn adjust(&mut self, adjustment: &Adjustment) {
        self.pointer += adjustment.shift;
        self.place.line = self.place.line + adjustment.line_change;

        for (&line, &change) in &adjustment.indexes_changed {
            if line == self.place.line {
                self.place >>= change;
            }
        }
    }
}

impl Display for Mark {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "{}{}", self.place, self.pointer)
    }
}

/// Signifies an index of a character within [`Pane`].
#[derive(Copy, Clone, Eq, PartialEq, PartialOrd, Ord, Hash, Debug)]
pub(crate) struct Pointer(Option<Index>);

impl Pointer {
    /// Returns a new `Pointer`.
    fn new(index: Option<Index>) -> Self {
        Self(index)
    }
}

impl PartialEq<IndexType> for Pointer {
    fn eq(&self, other: &IndexType) -> bool {
        self.0.map_or(false, |x| x == *other)
    }
}

impl PartialOrd<IndexType> for Pointer {
    fn partial_cmp(&self, other: &IndexType) -> Option<Ordering> {
        self.0.and_then(|x| x.partial_cmp(other))
    }
}

impl<T: Borrow<IndexType>> Add<T> for Pointer {
    type Output = Self;

    fn add(self, other: T) -> Self::Output {
        Self(self.0.map(|x| x + *other.borrow()))
    }
}

impl<T: Borrow<IndexType>> AddAssign<T> for Pointer {
    fn add_assign(&mut self, other: T) {
        self.0 = self.0.map(|x| x + *other.borrow());
    }
}

impl Default for Pointer {
    fn default() -> Self {
        Self(Some(Index::from(0)))
    }
}

impl Display for Pointer {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "[{}]",
            match self.0 {
                None => String::from("None"),
                Some(i) => format!("{}", i),
            }
        )
    }
}

impl PartialEq<Pointer> for IndexType {
    #[inline]
    fn eq(&self, other: &Pointer) -> bool {
        other == self
    }
}

impl PartialOrd<Pointer> for IndexType {
    #[inline]
    fn partial_cmp(&self, other: &Pointer) -> Option<Ordering> {
        other.partial_cmp(self).map(|x| x.reverse())
    }
}

/// Signifies a modification of the pane.
#[derive(Clone, Debug, Default)]
pub(crate) struct Adjustment {
    /// The change made to the current line.
    shift: IndexType,
    /// The changes made to the number of lines.
    line_change: IndexType,
    /// A map of the indexes where a change was made.
    indexes_changed: HashMap<LineNumber, IndexType>,
    /// The [`Change`] that best represents the `Adjustment`.
    change: Change,
}

impl Adjustment {
    /// Creates a new `Adjustment`.
    fn new(line: LineNumber, shift: IndexType, index_change: IndexType, change: Change) -> Self {
        let line_change = if change == Change::Clear { shift } else { 0 };

        Self {
            shift,
            line_change,
            indexes_changed: [(line + line_change, index_change)]
                .iter()
                .cloned()
                .collect(),
            change,
        }
    }

    /// Creates an `Adjustment` based on the given context.
    pub(crate) fn create(c: char, place: Place, pane: &Pane) -> Option<Self> {
        match c {
            BACKSPACE => {
                if place.column == 0 {
                    pane.line_length(place).map(|x| {
                        Self::new(place.line, NEGATIVE_ONE, IndexType::from(x), Change::Clear)
                    })
                } else {
                    Some(Self::new(
                        place.line,
                        NEGATIVE_ONE,
                        NEGATIVE_ONE,
                        Change::Backspace,
                    ))
                }
            }
            ENTER => Some(Self::new(
                place.line,
                1,
                place.column.negate(),
                Change::Clear,
            )),
            _ => Some(Self::new(place.line, 1, 1, Change::Insert(c))),
        }
    }
}

impl AddAssign for Adjustment {
    fn add_assign(&mut self, other: Self) {
        self.shift += other.shift;
        self.line_change += other.line_change;

        for (line, change) in other.indexes_changed {
            *self.indexes_changed.entry(line).or_default() += change;
        }

        if self.change != Change::Clear {
            self.change = other.change
        }
    }
}

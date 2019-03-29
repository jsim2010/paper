//! Implements the modality of the application.
add_trait_child!(Processor, action, ActionProcessor);
add_trait_child!(Processor, command, CommandProcessor);
add_trait_child!(Processor, display, DisplayProcessor);
add_trait_child!(Processor, edit, EditProcessor);
add_trait_child!(Processor, filter, FilterProcessor);

use crate::file::Explorer;
use crate::lsp::ProgressParams;
use crate::num::Length;
use crate::storage::{self, LspError};
use crate::ui::{self, Address, Change, Color, Edit, Index, BACKSPACE, ENTER};
use crate::Mrc;
use crate::Output;
use lsp_types::{Position, Range};
use std::cmp;
use std::fmt::{self, Debug, Display, Formatter};
use std::io;
use std::iter;
use std::ops::{Add, Deref, Sub};
use std::path::PathBuf;
use std::rc::Rc;
use try_from::{TryFrom, TryFromIntError};

/// Defines the type that identifies a line.
///
/// Defined by [`Position`].
type Line = u64;

/// Defines the type that indexes a collection of lines.
///
/// The value of a `LineIndex` is equal to its respective [`Line`].
type LineIndex = usize;

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
    #[inline]
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
    #[inline]
    fn default() -> Self {
        Name::Display
    }
}

/// Defines the functionality of a processor of a mode.
pub(crate) trait Processor: Debug {
    /// Enters the application into its mode.
    fn enter(&mut self, initiation: &Option<Initiation>) -> Output<()>;
    /// Generates an [`Operation`] from the given input.
    fn decode(&mut self, input: char) -> Output<Operation>;
}

/// Signifies a function to be performed when the application enters a mode.
///
/// In general, only certain modes can implement certain Initiations; for example: only Filter
/// implements [`StartFilter`].
#[derive(Clone, Eq, PartialEq, Debug)]
pub enum Initiation {
    /// Sets the view.
    SetView(PathBuf),
    /// Saves the current data of the view.
    Save,
    /// Starts a filter.
    StartFilter(char),
    /// Sets a list of [`Range`]s.
    SetSignals(Vec<Range>),
    /// Marks a list of [`Position`]s.
    Mark(Vec<Position>),
}

/// The control panel of a [`Pane`].
#[derive(Clone, Debug, Default, Hash)]
struct ControlPanel {
    /// The [`String`] to be edited.
    string: String,
    /// The height of the `Pane`.
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

    /// Returns the edits needed to write the string.
    fn edits(&self) -> Vec<Edit> {
        vec![Edit::new(
            Some(Address::new(self.height.sub_one(), Index::zero())),
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
        vec![Edit::new(None, Change::Flash)]
    }
}

impl Deref for ControlPanel {
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
    #[inline]
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
    #[inline]
    fn from(error: TryFromIntError) -> Self {
        Flag::Conversion(error)
    }
}

impl From<ui::Error> for Flag {
    #[inline]
    fn from(error: ui::Error) -> Self {
        Flag::Ui(error)
    }
}

impl From<io::Error> for Flag {
    #[inline]
    fn from(error: io::Error) -> Self {
        Flag::File(storage::Error::from(error))
    }
}

/// Signfifies the pane of the current file.
#[derive(Clone, Debug, Default, Hash)]
pub(crate) struct Pane {
    /// The path that makes up the pane.
    path: PathBuf,
    /// The data.
    data: String,
    /// The first line that is displayed in the ui.
    first_line: Line,
    /// The number of columns needed to display the margin.
    margin_width: u8,
    /// The number of rows visible in the pane.
    height: Rc<Index>,
    /// The number of lines in the data.
    line_count: Line,
    /// The control panel of the `Pane`.
    control_panel: ControlPanel,
    /// The edits `Pane` needs to make to update the [`UserInterface`].
    edits: Vec<Edit>,
    /// If `Pane` will clear and redraw on next update.
    will_wipe: bool,
}

impl Pane {
    /// Creates a new Pane with a given height.
    pub(crate) fn new(height: Index) -> Self {
        let height = Rc::new(height);

        Self {
            control_panel: ControlPanel::new(&height),
            height,
            ..Self::default()
        }
    }

    /// Returns the edits needed to update `Pane`.
    pub(crate) fn edits(&mut self) -> Vec<Edit> {
        if self.will_wipe {
            self.edits.clear();
            self.edits.push(Edit::new(None, Change::Clear));

            if let Ok(start_line_index) = LineIndex::try_from(self.first_line) {
                for row in self.visible_rows() {
                    if let Some(line_index) = LineIndex::try_from(row)
                        .ok()
                        .and_then(|row_index| start_line_index.checked_add(row_index))
                    {
                        if let Some(line_data) = self.line_data(line_index) {
                            if let Ok(line_number) = LineNumber::try_from(line_index) {
                                self.edits.push(Edit::new(
                                    Some(Address::new(row, Index::zero())),
                                    Change::Row(format!(
                                        "{: >width$} {}",
                                        line_number,
                                        line_data,
                                        width = usize::from(self.margin_width)
                                    )),
                                ));
                            } else {
                                break;
                            }
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

        let edits = self.edits.clone();
        self.edits.clear();
        edits
    }

    /// Sets [`Pane`] to be wiped on the next call to [`edits`]().
    fn wipe(&mut self) {
        self.will_wipe = true;
    }

    /// Adds the edits to display a notification.
    pub(crate) fn add_notification(&mut self, notification: ProgressParams) {
        if let Some(message) = notification.message {
            self.edits.push(Edit::new(
                Some(Address::new(Index::zero(), Index::zero())),
                Change::Row(message),
            ));
        }
    }

    /// Resets the [`ControlPanel`].
    fn reset_control_panel(&mut self, id: Option<char>) {
        self.control_panel.clear();

        if let Some(filter_id) = id {
            // TODO: It is assumed that filter_id is not BACKSPACE.
            self.control_panel.add_non_bs(filter_id);
        }

        self.edits.append(&mut self.control_panel.edits());
    }

    /// Adds an input to the control panel.
    fn input_to_control_panel(&mut self, input: char) {
        self.edits
            .append(&mut self.control_panel.edits_after_add(input));
    }

    /// Returns an [`IndexIterator`] of the all visible rows.
    fn visible_rows(&self) -> IndexIterator {
        IndexIterator::new(Index::zero(), *self.height.deref())
    }

    /// Applies filter highlighting to the given [`Range`]s.
    fn apply_filter(&mut self, noises: &[Range], signals: &[Range]) {
        for row in self.visible_rows() {
            self.edits.push(Edit::new(
                Some(Address::new(row, Index::zero())),
                Change::Format(Length::End, Color::Default),
            ));
        }

        for noise in noises {
            let address = self.address_at(noise.start);
            let length = Length::from(noise.end.character.saturating_sub(noise.start.character));

            if address.is_some() && !length.is_zero() {
                self.edits
                    .push(Edit::new(address, Change::Format(length, Color::Blue)));
            }
        }

        for signal in signals {
            let address = self.address_at(signal.start);
            let length = Length::from(signal.end.character.saturating_sub(signal.start.character));

            if address.is_some() && !length.is_zero() {
                self.edits
                    .push(Edit::new(address, Change::Format(length, Color::Red)));
            }
        }
    }

    /// Changes the pane to a new path.
    fn change(&mut self, explorer: &Mrc<dyn Explorer>, path: &PathBuf) -> Output<()> {
        self.data = explorer.borrow_mut().read(path)?;
        self.path = path.clone();
        self.refresh();
        Ok(())
    }

    /// Adds a character at a [`Position`].
    pub(crate) fn add(&mut self, position: Position, input: char) -> Result<(), TryFromIntError> {
        //let mut new_text = String::new();
        //let mut range = Range::new(position, position);

        if input == BACKSPACE {
            if position.character == 0 {
                if position.line != 0 {
                    //range.start.line -= 1;
                    //range.start.character = u64::max_value();
                    self.will_wipe = true;
                    self.refresh();
                }
            } else {
                //range.start.character -= 1;
                let address = self.address_at(position);

                if address.is_some() {
                    self.edits.push(Edit::new(address, Change::Backspace));
                }
            }
        } else {
            //new_text.push(input);

            if input == ENTER {
                self.will_wipe = true;
                self.refresh();
            } else {
                let address = self.address_at(position);

                if address.is_some() {
                    self.edits.push(Edit::new(address, Change::Insert(input)));
                }
            }
        }

        let pointer = self.line_indices().nth(LineIndex::try_from(position.line)?);

        if let Some(index) = pointer {
            let mut index = usize::try_from(index)? as u64;
            index += position.character;
            let data_index = usize::try_from(index)?;

            if input == BACKSPACE {
                // TODO: For now, do not care to check what is removed. But this may become important for
                // multi-byte characters.
                match self.data.remove(data_index) {
                    _ => {}
                }
            } else {
                self.data.insert(data_index.saturating_sub(1), input);
            }
        }

        Ok(())
    }

    /// Iterates through the indexes that indicate where each line starts.
    pub(crate) fn line_indices(&self) -> impl Iterator<Item = Index> + '_ {
        iter::once(Index::zero()).chain(self.data.match_indices(ENTER).flat_map(|(index, _)| {
            index
                .checked_add(1)
                .and_then(|value| Index::try_from(value).ok())
                .into_iter()
        }))
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
            .and_then(|line| Index::try_from(line).ok())
    }

    /// Returns the column at which a [`Position`] is located.
    ///
    /// [`None`] indicates that the [`Position`] is not visible in the user interface.
    fn column_at(&self, position: &Position) -> Option<Index> {
        position
            .character
            .checked_add(self.origin_character())
            .and_then(|character| Index::try_from(character).ok())
    }

    /// Returns the [`Address`] associated with the given [`Position`].
    fn address_at(&self, position: Position) -> Option<Address> {
        self.row_at(&position).and_then(|row| {
            self.column_at(&position)
                .map(|column| Address::new(row, column))
        })
    }

    /// An [`Iterator`] of all lines in the pane's data.
    fn lines(&self) -> std::str::Lines<'_> {
        self.data.lines()
    }

    /// The data stored at the given line.
    fn line_data(&self, line_index: LineIndex) -> Option<&str> {
        self.lines().nth(line_index)
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
                .unwrap_or_else(Index::zero),
        )
    }

    /// Scrolls the data of `Pane` up.
    fn scroll_up(&mut self) {
        self.set_first_line(self.first_line.saturating_sub(self.scroll_delta()));
    }

    /// Scrolls the data of `Pane` down.
    fn scroll_down(&mut self) {
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
    fn new(start: Index, end: Index) -> Self {
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

/// Defines operation to be performed by [`Processor`].
#[derive(Debug)]
pub struct Operation {
    /// The [`Name`] of the desired mode.
    ///
    /// [`None`] indicates the application should keep the current mode.
    mode: Option<Name>,
    /// The [`Initiation`] to be run if changing modes.
    initiation: Option<Initiation>,
}

impl Operation {
    /// Returns [`Name`] of `Operation`.
    pub(crate) fn mode(&self) -> &Option<Name> {
        &self.mode
    }

    /// Returns [`Initiation`] of `Operation`.
    pub(crate) fn initiation(&self) -> &Option<Initiation> {
        &self.initiation
    }

    /// Creates a new `Operation` to enter Command mode.
    fn enter_command() -> Self {
        Self {
            mode: Some(Name::Command),
            initiation: None,
        }
    }

    /// Creates a new `Operation` to enter Action mode.
    fn enter_filter(id: char) -> Self {
        Self {
            mode: Some(Name::Filter),
            initiation: Some(Initiation::StartFilter(id)),
        }
    }

    /// Creates a new `Operation` to continue execution with no special action.
    pub(crate) fn maintain() -> Self {
        Self {
            mode: None,
            initiation: None,
        }
    }

    /// Creates a new `Operation` to display a new file.
    ///
    /// The application enters Display mode as a consequence of this `Operation`.
    #[inline]
    pub fn display_file(path: &str) -> Self {
        Self {
            mode: Some(Name::Display),
            initiation: Some(Initiation::SetView(PathBuf::from(path))),
        }
    }

    /// Creates a new `Operation` to save current file.
    ///
    /// The application enters Display mode as a consequence of this `Operation`.
    fn save_file() -> Self {
        Self {
            mode: Some(Name::Display),
            initiation: Some(Initiation::Save),
        }
    }

    /// Creates a new `Operation` to enter Edit mode.
    fn enter_display() -> Self {
        Self {
            mode: Some(Name::Display),
            initiation: None,
        }
    }

    /// Creates a new `Operation` to enter Action mode.
    fn enter_action(signals: Vec<Range>) -> Self {
        Self {
            mode: Some(Name::Action),
            initiation: Some(Initiation::SetSignals(signals)),
        }
    }

    /// Creates a new `Operation` to enter Edit mode.
    fn enter_edit(positions: Vec<Position>) -> Self {
        Self {
            mode: Some(Name::Edit),
            initiation: Some(Initiation::Mark(positions)),
        }
    }
}

/// The type of the value stored in [`LineNumber`].
type LineNumberType = u32;

/// Signifies a line number.
#[derive(Copy, Clone, PartialEq, Eq, Ord, PartialOrd, Hash, Debug)]
pub(crate) struct LineNumber(LineNumberType);

impl LineNumber {
    /// Creates a new `LineNumber`.
    pub(crate) fn new(value: usize) -> Result<Self, TryFromIntError> {
        if value == 0 {
            Err(TryFromIntError::Underflow)
        } else {
            LineNumberType::try_from(value).map(Self)
        }
    }

    /// Converts `LineNumber` to its row index - assuming line number `1` as at row `0`.
    #[allow(clippy::integer_arithmetic)] // self.0 > 0
    pub(crate) fn row(self) -> usize {
        (self.0 - 1) as usize
    }
}

impl Add<i128> for LineNumber {
    type Output = Self;

    fn add(self, other: i128) -> Self::Output {
        #[allow(clippy::integer_arithmetic)] // i64::min_value() <= u32 + i32 <= i64::max_value()
        match usize::try_from(i128::from(self.0) + other) {
            Ok(sum) => Self::new(sum).unwrap_or_default(),
            Err(TryFromIntError::Underflow) => Self::default(),
            Err(TryFromIntError::Overflow) => Self(LineNumberType::max_value()),
        }
    }
}

impl TryFrom<LineIndex> for LineNumber {
    type Err = TryFromIntError;

    fn try_from(value: LineIndex) -> Result<Self, Self::Err> {
        value
            .checked_add(1)
            .ok_or(TryFromIntError::Overflow)
            .and_then(Self::new)
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
        f.pad(&format!("{}", self.0))
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
        Ok(Self::new(s.parse::<usize>()?)?)
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

impl From<TryFromIntError> for ParseLineNumberError {
    fn from(_error: TryFromIntError) -> Self {
        ParseLineNumberError::InvalidValue
    }
}

//! Implements how the user interfaces with the application.

use crate::{Display, FmtResult, Formatter};
use pancurses::Input;
use std::error::Error;
use try_from::{TryFrom, TryFromIntError, TryInto};

/// The [`Result`] returned by functions of this module.
type UiResult = Result<(), Fault>;

/// The character that represents the `Backspace` key.
pub(crate) const BACKSPACE: char = '\u{08}';
/// The character that represents the `Enter` key.
pub(crate) const ENTER: char = '\n';
// Currently ESC is set to Ctrl-C to allow manual testing within vim terminal where ESC is already
// mapped.
/// The character that represents the `Esc` key.
pub(crate) const ESC: char = '';

/// Represents the default color.
const DEFAULT_COLOR: i16 = -1;

/// The interface between the user and the application.
///
/// All output is displayed in a grid of cells. Each cell contains one character and can change its
/// background color.
#[derive(Debug)]
pub(crate) struct UserInterface {
    /// The window that interfaces with the application.
    window: pancurses::Window,
}

impl UserInterface {
    /// Sets up the user interface for use.
    pub(crate) fn init(&self) -> UiResult {
        self.start_color()?;
        self.use_default_colors()?;
        self.disable_echo()?;
        self.define_color(Color::Red, pancurses::COLOR_RED)?;
        self.define_color(Color::Blue, pancurses::COLOR_BLUE)?;

        Ok(())
    }

    /// Gets input from the user.
    ///
    /// Returns [`None`] if no character input is provided.
    ///
    /// [`None`]: https://doc.rust-lang.org/std/option/enum.Option.html#variant.None
    pub(crate) fn receive_input(&self) -> Option<char> {
        match self.window.getch() {
            Some(Input::Character(c)) => Some(c),
            _ => None,
        }
    }

    /// Closes the user interface.
    pub(crate) fn close(&self) -> UiResult {
        Self::check_result(pancurses::endwin(), Fault::Endwin)
    }

    /// Applies the edit to the output.
    pub(crate) fn apply(&self, edit: Edit) -> UiResult {
        self.move_to(edit.region.start)?;

        match edit.change {
            Change::Backspace => {
                // Add BACKSPACE (move cursor 1 cell to the left) and delete that character.
                self.add_char(BACKSPACE)?;
                self.delete_char()?;
            }
            Change::Insert(c) => {
                self.insert_char(c)?;
            }
            Change::Row(s) => {
                self.add_str(s)?;
                self.clear_to_row_end()?;
            }
            Change::Clear => {
                self.clear_all()?;
            }
            Change::Format(color) => {
                self.format(edit.region.length, color)?;
            }
            Change::Nothing => {}
        }

        Ok(())
    }

    /// Flashes the output.
    pub(crate) fn flash(&self) -> UiResult {
        Self::check_result(pancurses::flash(), Fault::Flash)
    }

    // TODO: Store this value and update when size is changed.
    /// Returns the number of cells that make up the height of the grid.
    pub(crate) fn grid_height(&self) -> Result<usize, TryFromIntError> {
        self.window.get_max_y().try_into()
    }

    /// Initializes color processing.
    ///
    /// Must be called before any other color manipulation routine is called.
    fn start_color(&self) -> UiResult {
        Self::check_result(pancurses::start_color(), Fault::StartColor)
    }

    /// Initializes the default colors.
    fn use_default_colors(&self) -> UiResult {
        Self::check_result(pancurses::use_default_colors(), Fault::UseDefaultColors)
    }

    /// Disables echoing received characters on the screen.
    fn disable_echo(&self) -> UiResult {
        Self::check_result(pancurses::noecho(), Fault::Noecho)
    }

    /// Defines [`Color`] as having a background color.
    fn define_color(&self, color: Color, background: i16) -> UiResult {
        Self::check_result(
            pancurses::init_pair(color.cp(), DEFAULT_COLOR, background),
            Fault::InitPair,
        )
    }

    /// Moves the cursor to an [`Address`].
    fn move_to(&self, address: Address) -> UiResult {
        Self::check_result(self.window.mv(address.y(), address.x()), Fault::Wmove)
    }

    /// Overwrites the block at cursor with a character.
    fn add_char(&self, c: char) -> UiResult {
        Self::check_result(self.window.addch(c), Fault::Waddch)
    }

    /// Deletes the character at the cursor.
    ///
    /// All subseqent characters are shifted to the left and a blank block is added at the end.
    fn delete_char(&self) -> UiResult {
        Self::check_result(self.window.delch(), Fault::Wdelch)
    }

    /// Inserts a character at the cursor, shifting all subsequent blocks to the right.
    fn insert_char(&self, c: char) -> UiResult {
        Self::check_result(self.window.insch(c), Fault::Winsch)
    }

    /// Writes a string starting at the cursor.
    fn add_str(&self, s: String) -> UiResult {
        Self::check_result(self.window.addstr(s), Fault::Waddstr)
    }

    /// Clears all blocks from the cursor to the end of the row.
    fn clear_to_row_end(&self) -> UiResult {
        Self::check_result(self.window.clrtoeol(), Fault::Wcleartoeol)
    }

    /// Clears the entire window.
    fn clear_all(&self) -> UiResult {
        Self::check_result(self.window.clear(), Fault::Wclear)
    }

    /// Sets the color of the next specified number of blocks from the cursor.
    fn format(&self, length: Length, color: Color) -> UiResult {
        Self::check_result(self.window.chgat(length.0, pancurses::A_NORMAL, color.cp()), Fault::Wchgat)
    }

    /// Converts given result of ui function to a [`UiResult`].
    fn check_result(result: i32, error: Fault) -> UiResult {
        if result == pancurses::OK {
            Ok(())
        } else {
            Err(error)
        }
    }
}

/// Describes possible errors during ui functions.
#[derive(Copy, Clone, Debug)]
pub enum Fault {
    /// Describes a possible error during call to `wchgat()`.
    Wchgat,
    /// Describes a possible error during call to `wclear()`.
    Wclear,
    /// Describes a possible error during call to `wcleartoeol()`.
    Wcleartoeol,
    /// Describes a possible error during call to `waddstr()`.
    Waddstr,
    /// Describes a possible error during call to `winsch()`.
    Winsch,
    /// Describes a possible error during call to `wdelch()`.
    Wdelch,
    /// Describes a possible error during call to `waddch()`.
    Waddch,
    /// Describes a possible error during call to `wmove()`.
    Wmove,
    /// Describes a possible error during call to `init_pair()`.
    InitPair,
    /// Describes a possible error during call to `noecho()`.
    Noecho,
    /// Describes a possible error during call to `use_default_colors()`.
    UseDefaultColors,
    /// Describes a possible error during call to `start_color()`.
    StartColor,
    /// Describes a possible error during call to `endwin()`.
    Endwin,
    /// Describes a possible error during call to `flash()`.
    Flash,
}

impl Fault {
    /// Returns the function that caused the current [`Fault`].
    fn get_function(&self) -> &str {
        match self {
            Fault::Wchgat => "wchgat",
            Fault::Wclear => "wclear",
            Fault::Wcleartoeol => "wcleartoeol",
            Fault::Waddstr => "waddstr",
            Fault::Winsch => "winsch",
            Fault::Wdelch => "wdelch",
            Fault::Waddch => "waddch",
            Fault::Wmove => "wmove",
            Fault::InitPair => "init_pair",
            Fault::Noecho => "noecho",
            Fault::UseDefaultColors => "use_default_colors",
            Fault::StartColor => "start_color",
            Fault::Endwin => "endwin",
            Fault::Flash => "flash",
        }
    }
}

impl Error for Fault {}

impl Display for Fault {
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        write!(f, "Failed while calling {}().", self.get_function())
    }
}

impl Default for UserInterface {
    fn default() -> Self {
        Self {
            // Must call initscr() first.
            window: pancurses::initscr(),
        }
    }
}

/// Signifies a [`Change`] to make to a [`Region`].
///
/// [`Change`]s that act on a single [`Address`] are executed on the starting [`Address`] of the
/// [`Region`].
///
/// [`Change`]: enum.Change.html
/// [`Region`]: struct.Region.html
/// [`Address`]: struct.Address.html
#[derive(Clone, Debug, Default)]
pub(crate) struct Edit {
    /// The [`Change`] to be made.
    change: Change,
    /// The [`Region`] on which the [`Change`] is intended.
    region: Region,
}

impl Edit {
    /// Creates a new `Edit` with a given [`Region`] and [`Change`].
    ///
    /// [`Region`]: struct.Region.html
    /// [`Change`]: enum.Change.html
    pub(crate) fn new(region: Region, change: Change) -> Self {
        Self { region, change }
    }
}

/// Signifies a modification to the grid.
#[derive(Clone, Eq, PartialEq, Hash, Debug)]
pub(crate) enum Change {
    /// Does nothing.
    Nothing,
    /// Removes the previous cell, moving all subsequent cells to the left.
    Backspace,
    /// Inserts a cell containing a character, moving all subsequent cells to the right.
    Insert(char),
    /// Writes the characters of a string in sequence and clears all subsequent cells.
    Row(String),
    /// Clears all cells.
    Clear,
    /// Sets the color of all cells in a [`Region`].
    ///
    /// [`Region`]: struct.Region.html
    Format(Color),
}

impl Default for Change {
    fn default() -> Self {
        Change::Nothing
    }
}

impl Display for Change {
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        match self {
            Change::Nothing => write!(f, "Nothing"),
            Change::Backspace => write!(f, "Backspace"),
            Change::Insert(c) => write!(f, "Insert '{}'", c),
            Change::Row(s) => write!(f, "Write row '{}'", s),
            Change::Clear => write!(f, "Clear"),
            Change::Format(c) => write!(f, "Format to {}", c),
        }
    }
}

/// Signifies a color.
#[derive(Clone, Eq, PartialEq, Hash, Debug)]
pub(crate) enum Color {
    /// The default foreground on the default background.
    Default,
    /// The default foreground on a red background.
    Red,
    /// The default foreground on a blue background.
    Blue,
}

impl Color {
    /// Converts `self` to a `color-pair` as specified in [`pancurses`].
    ///
    /// [`pancurses`]: ../../pancurses/index.html
    fn cp(self) -> i16 {
        self as i16
    }
}

impl Display for Color {
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        match self {
            Color::Default => write!(f, "Default"),
            Color::Red => write!(f, "Red"),
            Color::Blue => write!(f, "Blue"),
        }
    }
}

/// Signifies a group of adjacent [`Address`]es.
///
/// [`Address`]: struct.Address.html
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug, Default)]
pub(crate) struct Region {
    /// The first [`Address`].
    ///
    /// [`Address`]: struct.Address.html
    start: Address,
    /// The [`Length`] of the `Region`.
    ///
    /// [`Length`]: struct.Length.html
    length: Length,
}

impl Region {
    /// Creates a new `Region` with a given starting [`Address`] and [`Length`].
    ///
    /// [`Address`]: struct.Address.html
    /// [`Length`]: struct.Length.html
    pub(crate) fn new(start: Address, length: Length) -> Self {
        Self { start, length }
    }

    /// Creates a new `Region` that signifies an entire row.
    pub(crate) fn row(row: Index) -> Self {
        Self {
            start: Address::new(row, Index::from(0)),
            length: END,
        }
    }
}

impl Display for Region {
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        write!(f, "{}->{}", self.start, self.length)
    }
}

/// Signifies a specific cell in the grid.
#[derive(Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Debug, Default)]
pub(crate) struct Address {
    /// The index of the row that contains the cell (starts at 0).
    row: Index,
    /// The index of the column that contains the cell (starts at 0).
    column: Index,
}

impl Address {
    /// Creates a new `Address` with a given row and column.
    pub(crate) fn new(row: Index, column: Index) -> Self {
        Self { row, column }
    }

    /// Returns the column of `self`.
    ///
    /// Used with [`pancurses`].
    ///
    /// [`pancurses`]: ../../pancurses/index.html
    fn x(self) -> i32 {
        i32::from(self.column)
    }

    /// Returns the row of `self`.
    ///
    /// Used with [`pancurses`].
    ///
    /// [`pancurses`]: ../../pancurses/index.html
    fn y(self) -> i32 {
        i32::from(self.row)
    }
}

impl Display for Address {
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        write!(f, "({}, {})", self.row, self.column)
    }
}

/// Signifies the index of a row or column in the grid.
///
/// Given `x` is an Index value:
///     `x >= 0`
///     `x <= i32::max_value`
#[derive(Copy, Clone, Debug, Default, Hash, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct Index(i32);

impl From<u8> for Index {
    fn from(value: u8) -> Self {
        Index(i32::from(value))
    }
}

impl TryFrom<u32> for Index {
    type Err = TryFromIntError;

    fn try_from(value: u32) -> Result<Self, Self::Err>{ 
        value.try_into().map(Index)
    }
}


impl TryFrom<usize> for Index {
    type Err = TryFromIntError;

    fn try_from(value: usize) -> Result<Self, Self::Err> {
        value.try_into().map(Index)
    }
}

impl Display for Index {
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        write!(f, "{}", self.0)
    }
}

impl From<Index> for i32 {
    #[inline]
    fn from(value: Index) -> Self {
        value.0
    }
}

/// Signifies a number of adjacent [`Address`]es.
///
/// Generally this is an unsigned number. However, there is a special `Length` called [`END`] that
/// signifies the number of [`Address`]es between a start [`Address`] and the end of that row.
///
/// [`Address`]: struct.Address.html
/// [`END`]: constant.END.html
#[derive(Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Debug, Default)]
pub(crate) struct Length(i32);

/// The internal value that represents the number of characters until the end of the row.
///
/// Specified by [`pancurses`].
const END_VALUE: i32 = -1;

/// The `Length` that represents the number of characters until the end of the row.
pub(crate) const END: Length = Length(END_VALUE);

impl TryFrom<usize> for Length {
    type Err = TryFromIntError;

    fn try_from(value: usize) -> Result<Self, Self::Err> {
        value.try_into().map(Length)
    }
}

impl TryFrom<Length> for u32 {
    type Err = TryFromIntError;

    #[inline]
    fn try_from(value: Length) -> Result<Self, Self::Err> {
        value.0.try_into()
    }
}

impl Display for Length {
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        match self.0 {
            END_VALUE => write!(f, "END"),
            x => write!(f, "{}", x),
        }
    }
}

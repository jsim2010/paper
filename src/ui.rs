//! Implements how the user interfaces with the application.
use crate::{Display, FmtResult, Formatter};
use pancurses::Input;

type UiResult = Result<(), String>;

/// The character that represents the `Backspace` key.
pub(crate) const BACKSPACE: char = '\u{08}';
/// The character that represents the `Enter` key.
pub(crate) const ENTER: char = '\n';
// Currently ESC is set to Ctrl-C to allow manual testing within vim terminal where ESC is already
// mapped.
/// The character that represents the `Esc` key.
pub(crate) const ESC: char = '';

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
        self.noecho()?;
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
        UserInterface::check_result(pancurses::endwin(), "endwin")
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
        UserInterface::check_result(pancurses::flash(), "flash")
    }

    // TODO: Store this value and update when size is changed.
    /// Returns the number of cells that make up the height of the grid.
    pub(crate) fn grid_height(&self) -> usize {
        self.window.get_max_y() as usize
    }

    /// Initializes color processing.
    ///
    /// Must be called before any other color manipulation routine is called.
    fn start_color(&self) -> UiResult {
        UserInterface::check_result(pancurses::start_color(), "start_color")
    }

    fn use_default_colors(&self) -> UiResult {
        UserInterface::check_result(pancurses::use_default_colors(), "use_default_colors")
    }

    fn noecho(&self) -> UiResult {
        UserInterface::check_result(pancurses::noecho(), "noecho")
    }

    fn define_color(&self, color: Color, background: i16) -> UiResult {
        UserInterface::check_result(
            pancurses::init_pair(color.cp(), -1, background),
            "init_pair",
        )
    }

    fn move_to(&self, address: Address) -> UiResult {
        UserInterface::check_result(self.window.mv(address.y(), address.x()), "wmove")
    }

    fn add_char(&self, c: char) -> UiResult {
        UserInterface::check_result(self.window.addch(c), "waddch")
    }

    fn delete_char(&self) -> UiResult {
        UserInterface::check_result(self.window.delch(), "wdelch")
    }

    fn insert_char(&self, c: char) -> UiResult {
        UserInterface::check_result(self.window.insch(c), "winsch")
    }

    fn add_str(&self, s: String) -> UiResult {
        UserInterface::check_result(self.window.addstr(s), "waddstr")
    }

    fn clear_to_row_end(&self) -> UiResult {
        UserInterface::check_result(self.window.clrtoeol(), "wcleartoeol")
    }

    fn clear_all(&self) -> UiResult {
        UserInterface::check_result(self.window.clear(), "wclear")
    }

    fn format(&self, length: Length, color: Color) -> UiResult {
        UserInterface::check_result(
            self.window.chgat(length.0, pancurses::A_NORMAL, color.cp()),
            "wchgat",
        )
    }

    fn check_result(result: i32, call: &str) -> UiResult {
        match result {
            pancurses::OK => Ok(()),
            _ => Err(format!("Failed while calling {}().", call)),
        }
    }
}

impl Default for UserInterface {
    fn default() -> UserInterface {
        UserInterface {
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
    pub(crate) fn new(region: Region, change: Change) -> Edit {
        Edit { region, change }
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
    fn default() -> Change {
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
    pub(crate) fn new(start: Address, length: Length) -> Region {
        Region { start, length }
    }

    /// Creates a new `Region` that signifies an entire row.
    pub(crate) fn row(row: usize) -> Region {
        Region {
            start: Address::new(row, 0),
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
    row: usize,
    /// The index of the column that contains the cell (starts at 0).
    column: usize,
}

impl Address {
    /// Creates a new `Address` with a given row and column.
    pub(crate) fn new(row: usize, column: usize) -> Address {
        Address { row, column }
    }

    /// Returns the column of `self`.
    ///
    /// Used with [`pancurses`].
    ///
    /// [`pancurses`]: ../../pancurses/index.html
    fn x(&self) -> i32 {
        self.column as i32
    }

    /// Returns the row of `self`.
    ///
    /// Used with [`pancurses`].
    ///
    /// [`pancurses`]: ../../pancurses/index.html
    fn y(&self) -> i32 {
        self.row as i32
    }
}

impl Display for Address {
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        write!(f, "({}, {})", self.row, self.column)
    }
}

/// Signifies a number of adjacent [`Address`]es.
///
/// Generally this is an unsigned number. However, there is a special `Length` called [`END`] that
/// signifies the number of [`Address`]es between a start [`Address`] and the end of that row.
///
/// To ensure safe behavior, `Length` should only be created by using [`from`].
///
/// [`Address`]: struct.Address.html
/// [`END`]: constant.END.html
/// [`from`]: struct.Length.html#method.from

// Use a tuple instead of a type so that a custom Display trait can be implemented.
#[derive(Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Debug, Default)]
pub(crate) struct Length(i32);

/// The internal value that represents the number of characters until the end of the row.
///
/// Specified by [`pancurses`].
///
/// [`pancurses`]: ../../pancurses/index.html
const END_VALUE: i32 = -1;
/// The `Length` that represents the number of characters until the end of the row.
pub(crate) const END: Length = Length(END_VALUE);

impl Length {
    /// Converts to usize.
    pub(crate) fn into_usize(self) -> usize {
        // Given that Length was created by from(), this conversion is safe.
        self.0 as usize
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

// TODO: This should be changed to TryFrom once that has stablized.
impl From<usize> for Length {
    fn from(value: usize) -> Length {
        Length(value as i32)
    }
}

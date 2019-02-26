//! Implements how the user interfaces with the application.

pub(crate) use crate::num::{Length, NonNegativeI32};

use crate::{fmt, Debug, Display, Formatter, TryFrom, TryFromIntError};
use pancurses::Input;
use std::error;

/// The [`Result`] returned by functions of this module.
pub type Outcome = Result<(), Error>;
/// The type specified by all grid index values
///
/// Specified by [`pancurses`].
pub(crate) type IndexType = i32;
/// The type of all grid index values.
pub type Index = NonNegativeI32;

/// The character that represents the `Backspace` key.
pub const BACKSPACE: char = '\u{08}';
/// The character that represents the `Enter` key.
pub(crate) const ENTER: char = '\n';
// Currently ESC is set to Ctrl-C to allow manual testing within vim terminal where ESC is already
// mapped.
/// The character that represents the `Esc` key.
pub const ESC: char = '';

/// Represents the default color.
const DEFAULT_COLOR: i16 = -1;

/// Describes possible errors during ui functions.
#[derive(Clone, Copy, Debug)]
pub enum Error {
    /// Describes an error due to no user interface being created.
    NoUi,
    /// Describes a possible error during call to `endwin()`.
    Endwin,
    /// Describes a possible error during call to `flash()`.
    Flash,
    /// Describes a possible error during call to `init_pair()`.
    InitPair,
    /// Describes a possible error during call to `noecho()`.
    Noecho,
    /// Describes a possible error during call to `start_color()`.
    StartColor,
    /// Describes a possible error during call to `use_default_colors()`.
    UseDefaultColors,
    /// Describes a possible error during call to `waddch()`.
    Waddch,
    /// Describes a possible error during call to `waddstr()`.
    Waddstr,
    /// Describes a possible error during call to `wchgat()`.
    Wchgat,
    /// Describes a possible error during call to `wclear()`.
    Wclear,
    /// Describes a possible error during call to `wcleartoeol()`.
    Wcleartoeol,
    /// Describes a possible error during call to `wdelch()`.
    Wdelch,
    /// Describes a possible error during call to `winsch()`.
    Winsch,
    /// Describes a possible error during call to `wmove()`.
    Wmove,
}

impl Error {
    /// Returns the function that caused the current `Error`.
    fn get_function(&self) -> &str {
        match self {
            Error::Endwin => "endwin",
            Error::Flash => "flash",
            Error::InitPair => "init_pair",
            Error::Noecho => "noecho",
            Error::StartColor => "start_color",
            Error::UseDefaultColors => "use_default_colors",
            Error::Waddch => "waddch",
            Error::Waddstr => "waddstr",
            Error::Wchgat => "wchgat",
            Error::Wclear => "wclear",
            Error::Wcleartoeol => "wcleartoeol",
            Error::Wdelch => "wdelch",
            Error::Winsch => "winsch",
            Error::Wmove => "wmove",
            Error::NoUi => "",
        }
    }
}

impl Display for Error {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Error::NoUi => write!(f, "No UserInterface was created."),
            _ => write!(f, "Failed while calling {}().", self.get_function()),
        }
    }
}

impl error::Error for Error {}

/// Signifies a specific cell in the grid.
#[derive(Clone, Copy, Eq, Debug, Default, Hash, Ord, PartialEq, PartialOrd)]
pub struct Address {
    /// The index of the row that contains the cell (starts at 0).
    row: Index,
    /// The index of the column that contains the cell (starts at 0).
    column: Index,
}

impl Address {
    /// Creates a new `Address` with a given row and column.
    pub fn new(row: Index, column: Index) -> Self {
        Self { row, column }
    }

    /// Returns the column of `self`.
    ///
    /// Used with [`pancurses`].
    ///
    /// [`pancurses`]: ../../pancurses/index.html
    fn x(self) -> IndexType {
        IndexType::from(self.column)
    }

    /// Returns the row of `self`.
    ///
    /// Used with [`pancurses`].
    ///
    /// [`pancurses`]: ../../pancurses/index.html
    fn y(self) -> IndexType {
        IndexType::from(self.row)
    }
}

impl Display for Address {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "({}, {})", self.row, self.column)
    }
}

/// Signifies a modification to the grid.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub enum Change {
    /// Removes the previous cell, moving all subsequent cells to the left.
    Backspace,
    /// Clears all cells.
    Clear,
    /// Sets the color of all cells in a [`Region`].
    ///
    /// [`Region`]: struct.Region.html
    Format(Color),
    /// Inserts a cell containing a character, moving all subsequent cells to the right.
    Insert(char),
    /// Does nothing.
    Nothing,
    /// Writes the characters of a string in sequence and clears all subsequent cells.
    Row(String),
}

impl Default for Change {
    fn default() -> Self {
        Change::Nothing
    }
}

impl Display for Change {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Change::Backspace => write!(f, "Backspace"),
            Change::Clear => write!(f, "Clear"),
            Change::Format(color) => write!(f, "Format to {}", color),
            Change::Insert(input) => write!(f, "Insert '{}'", input),
            Change::Nothing => write!(f, "Nothing"),
            Change::Row(row_str) => write!(f, "Write row '{}'", row_str),
        }
    }
}

/// Signifies a color.
// Order must be kept as defined to match pancurses.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum Color {
    /// The default foreground on the default background.
    Default,
    /// The default foreground on a red background.
    Red,
    /// The default foreground on a green background.
    Green,
    /// The default foreground on a yellow background.
    Yellow,
    /// The default foreground on a blue background.
    Blue,
}

impl Color {
    /// Converts `self` to a `color-pair` as specified in [`pancurses`].
    fn cp(self) -> i16 {
        self as i16
    }
}

impl Display for Color {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Color::Default => write!(f, "Default"),
            Color::Red => write!(f, "Red"),
            Color::Green => write!(f, "Green"),
            Color::Yellow => write!(f, "Yellow"),
            Color::Blue => write!(f, "Blue"),
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
#[derive(Clone, Debug, Default, Eq, Hash, PartialEq)]
pub struct Edit {
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
    pub fn new(region: Region, change: Change) -> Self {
        Self { region, change }
    }
}

/// Signifies a group of adjacent [`Address`]es.
///
/// [`Address`]: struct.Address.html
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub struct Region {
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
    pub fn new(start: Address, length: Length) -> Self {
        Self { start, length }
    }

    /// Creates a new `Region` that signifies an entire row.
    pub(crate) fn with_row(row: usize) -> Result<Self, TryFromIntError> {
        Index::try_from(row).map(|row_index| Self {
            start: Address::new(row_index, Index::from(0)),
            length: Length::End,
        })
    }
}

impl Display for Region {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "{}->{}", self.start, self.length)
    }
}

/// The interface between the user and the application.
///
/// All output is displayed in a grid of cells. Each cell contains one character and can change its
/// background color.
pub trait UserInterface: Debug {
    /// Sets up the user interface for use.
    fn init(&self) -> Outcome;
    /// Closes the user interface.
    fn close(&self) -> Outcome;
    /// Returns the number of cells that make up the height of the grid.
    fn grid_height(&self) -> Result<usize, TryFromIntError>;
    /// Applies the edit to the output.
    fn apply(&self, edit: Edit) -> Outcome;
    /// Flashes the output.
    fn flash(&self) -> Outcome;
    /// Gets input from the user.
    ///
    /// Returns [`None`] if no character input is provided.
    ///
    /// [`None`]: https://doc.rust-lang.org/std/option/enum.Option.html#variant.None
    fn receive_input(&self) -> Option<Input>;
}

/// An empty instance of a [`UserInterface`].
#[derive(Debug)]
pub(crate) struct NullUserInterface;

impl UserInterface for NullUserInterface {
    fn init(&self) -> Outcome {
        Err(Error::NoUi)
    }

    fn close(&self) -> Outcome {
        Err(Error::NoUi)
    }

    fn grid_height(&self) -> Result<usize, TryFromIntError> {
        Ok(0)
    }

    fn apply(&self, _: Edit) -> Outcome {
        Err(Error::NoUi)
    }

    fn flash(&self) -> Outcome {
        Err(Error::NoUi)
    }

    fn receive_input(&self) -> Option<Input> {
        None
    }
}

/// The user interface provided by a terminal.
#[derive(Debug)]
pub struct Terminal {
    /// The window that interfaces with the application.
    window: pancurses::Window,
}

impl Terminal {
    /// Creates a new `Terminal`.
    pub fn new() -> Self {
        Self::default()
    }
    /// Converts given result of ui function to a [`Outcome`].
    fn process(result: i32, error: Error) -> Outcome {
        if result == pancurses::OK {
            Ok(())
        } else {
            Err(error)
        }
    }

    /// Overwrites the block at cursor with a character.
    fn add_char(&self, c: char) -> Outcome {
        Self::process(self.window.addch(c), Error::Waddch)
    }

    /// Writes a string starting at the cursor.
    fn add_str(&self, s: String) -> Outcome {
        Self::process(self.window.addstr(s), Error::Waddstr)
    }

    /// Clears the entire window.
    fn clear_all(&self) -> Outcome {
        Self::process(self.window.clear(), Error::Wclear)
    }

    /// Clears all blocks from the cursor to the end of the row.
    fn clear_to_row_end(&self) -> Outcome {
        Self::process(self.window.clrtoeol(), Error::Wcleartoeol)
    }

    /// Defines [`Color`] as having a background color.
    fn define_color(&self, color: Color, background: i16) -> Outcome {
        dbg!(color.cp());
        Self::process(
            pancurses::init_pair(color.cp(), DEFAULT_COLOR, background),
            Error::InitPair,
        )
    }

    /// Deletes the character at the cursor.
    ///
    /// All subseqent characters are shifted to the left and a blank block is added at the end.
    fn delete_char(&self) -> Outcome {
        Self::process(self.window.delch(), Error::Wdelch)
    }

    /// Disables echoing received characters on the screen.
    fn disable_echo(&self) -> Outcome {
        Self::process(pancurses::noecho(), Error::Noecho)
    }

    /// Sets the color of the next specified number of blocks from the cursor.
    fn format(&self, length: Length, color: Color) -> Outcome {
        Self::process(
            self.window
                .chgat(length.into(), pancurses::A_NORMAL, color.cp()),
            Error::Wchgat,
        )
    }

    /// Inserts a character at the cursor, shifting all subsequent blocks to the right.
    fn insert_char(&self, c: char) -> Outcome {
        Self::process(self.window.insch(c), Error::Winsch)
    }

    /// Moves the cursor to an [`Address`].
    fn move_to(&self, address: Address) -> Outcome {
        Self::process(self.window.mv(address.y(), address.x()), Error::Wmove)
    }

    /// Initializes color processing.
    ///
    /// Must be called before any other color manipulation routine is called.
    fn start_color(&self) -> Outcome {
        Self::process(pancurses::start_color(), Error::StartColor)
    }

    /// Initializes the default colors.
    fn use_default_colors(&self) -> Outcome {
        Self::process(pancurses::use_default_colors(), Error::UseDefaultColors)
    }
}

impl Default for Terminal {
    fn default() -> Self {
        Self {
            // Must call initscr() first.
            window: pancurses::initscr(),
        }
    }
}

impl UserInterface for Terminal {
    fn init(&self) -> Outcome {
        self.start_color()?;
        self.use_default_colors()?;
        self.disable_echo()?;
        self.define_color(Color::Red, pancurses::COLOR_RED)?;
        self.define_color(Color::Blue, pancurses::COLOR_BLUE)?;

        Ok(())
    }

    fn close(&self) -> Outcome {
        Self::process(pancurses::endwin(), Error::Endwin)
    }

    fn flash(&self) -> Outcome {
        Self::process(pancurses::flash(), Error::Flash)
    }

    fn apply(&self, edit: Edit) -> Outcome {
        self.move_to(edit.region.start)?;

        match edit.change {
            Change::Backspace => {
                // Add BACKSPACE (move cursor 1 cell to the left) and delete that character.
                self.add_char(BACKSPACE)?;
                self.delete_char()
            }
            Change::Clear => self.clear_all(),
            Change::Format(color) => self.format(edit.region.length, color),
            Change::Insert(c) => self.insert_char(c),
            Change::Nothing => Ok(()),
            Change::Row(s) => {
                self.add_str(s)?;
                self.clear_to_row_end()
            }
        }
    }

    // TODO: Store this value and update when size is changed.
    fn grid_height(&self) -> Result<usize, TryFromIntError> {
        usize::try_from(self.window.get_max_y())
    }

    fn receive_input(&self) -> Option<Input> {
        self.window.getch()
    }
}

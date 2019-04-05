//! Implements how the user interfaces with the application.
pub use crate::num::NonNegativeI32 as Index;

use crate::ptr::Mrc;
use pancurses::Input;
use std::{
    cell::RefCell,
    error,
    fmt::{self, Debug, Display, Formatter},
    rc::Rc,
};
use try_from::{TryFrom, TryFromIntError};

/// The [`Result`] returned by functions of this module.
pub type Effect = Result<(), Error>;

/// The character that represents the `Backspace` key.
pub const BACKSPACE: char = '\u{08}';
/// The character that represents the `Enter` key.
pub(crate) const ENTER: char = '\n';
/// The character that represents the `Esc` key.
// Currently ESC is set to Ctrl-C to allow manual testing within vim terminal where ESC is already
// mapped.
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
    /// Describes a possible error during call to `nodelay()`.
    Nodelay,
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
            Error::Nodelay => "nodelay",
            Error::NoUi => "",
        }
    }
}

impl Display for Error {
    #[inline]
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
    #[inline]
    pub fn new(row: Index, column: Index) -> Self {
        Self { row, column }
    }

    fn is_end_of_row(&self) -> bool {
        self.column == Index::max_value()
    }

    /// Returns the column of `self`.
    ///
    /// Used with [`pancurses`].
    fn x(self) -> i32 {
        i32::from(self.column)
    }

    /// Returns the row of `self`.
    ///
    /// Used with [`pancurses`].
    fn y(self) -> i32 {
        i32::from(self.row)
    }
}

impl Display for Address {
    #[inline]
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "({}, {})", self.row, self.column)
    }
}

/// Signifies a sequence of `Address`es.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Span {
    first: Address,
    last: Address,
}

impl Span {
    /// Creates a new `Span`.
    pub fn new(first: Address, last: Address) -> Self {
        Self { first, last }
    }

    fn length(&self) -> i32 {
        self.last.x() - self.first.x()
    }
}

impl Display for Span {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "[{} -> {}]", self.first, self.last)
    }
}

/// Signifies a modification to the grid.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub enum Change {
    /// Clears all cells.
    Clear,
    /// Sets the color of a given number of cells.
    Format(Span, Color),
    /// Does nothing.
    Nothing,
    /// Flashes the display.
    Flash,
    /// Sets the text for a given `Span` of `Addresses`.
    Text(Span, String),
}

impl Default for Change {
    #[inline]
    fn default() -> Self {
        Change::Nothing
    }
}

impl Display for Change {
    #[inline]
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Change::Clear => write!(f, "Clear"),
            Change::Format(span, color) => write!(f, "Format {} to {}", span, color),
            Change::Nothing => write!(f, "Nothing"),
            Change::Flash => write!(f, "Flash"),
            Change::Text(span, text) => write!(f, "Set {} to {}", span, text),
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
    /// Converts `self` to a `color-pair` as specified in `pancurses`.
    fn cp(self) -> i16 {
        self as i16
    }
}

impl Display for Color {
    #[inline]
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

/// Signifies a [`Change`] to make to an [`Address`].
///
/// [`Change`]: enum.Change.html
/// [`Address`]: struct.Address.html
#[derive(Clone, Debug, Default, Eq, Hash, PartialEq)]
pub struct Edit {
    /// The [`Change`] to be made.
    change: Change,
    /// The [`Address`] on which the [`Change`] is intended.
    address: Option<Address>,
}

impl Edit {
    /// Creates a new `Edit`.
    #[inline]
    pub fn new(address: Option<Address>, change: Change) -> Self {
        Self { address, change }
    }
}

/// The interface between the user and the application.
///
/// All output is displayed in a grid of cells. Each cell contains one character and can change its
/// background color.
pub trait UserInterface: Debug {
    /// Sets up the user interface for use.
    fn init(&self) -> Effect;
    /// Closes the user interface.
    fn close(&self) -> Effect;
    /// Returns the number of cells that make up the height of the grid.
    fn grid_height(&self) -> Result<Index, TryFromIntError>;
    /// Applies the edit to the output.
    fn apply(&self, edit: Edit) -> Effect;
    /// Flashes the output.
    fn flash(&self) -> Effect;
    /// Returns the input from the user.
    ///
    /// Returns [`None`] if no character input is provided.
    fn receive_input(&self) -> Option<Input>;
}

/// The user interface provided by a terminal.
#[derive(Debug)]
pub struct Terminal {
    /// The window that interfaces with the application.
    window: pancurses::Window,
}

impl Terminal {
    /// Creates a new `Terminal`.
    #[inline]
    pub fn new() -> Mrc<Self> {
        Rc::new(RefCell::new(Self {
            // Must call initscr() first.
            window: pancurses::initscr(),
        }))
    }

    /// Converts the given result of a `Terminal` function to a [`Effect`].
    fn process(result: i32, error: Error) -> Effect {
        if result == pancurses::OK {
            Ok(())
        } else {
            Err(error)
        }
    }

    /// Writes a string starting at the cursor.
    fn add_str(&self, s: String) -> Effect {
        Self::process(self.window.addstr(s), Error::Waddstr)
    }

    /// Clears the entire window.
    fn clear_all(&self) -> Effect {
        Self::process(self.window.clear(), Error::Wclear)
    }

    /// Clears all blocks from the cursor to the end of the row.
    fn clear_to_row_end(&self) -> Effect {
        Self::process(self.window.clrtoeol(), Error::Wcleartoeol)
    }

    /// Defines [`Color`] as having a background color.
    fn define_color(&self, color: Color, background: i16) -> Effect {
        Self::process(
            pancurses::init_pair(color.cp(), DEFAULT_COLOR, background),
            Error::InitPair,
        )
    }

    /// Deletes the character at the cursor.
    ///
    /// All subseqent characters are shifted to the left and a blank block is added at the end.
    fn delete_char(&self) -> Effect {
        Self::process(self.window.delch(), Error::Wdelch)
    }

    /// Disables echoing received characters on the screen.
    fn disable_echo(&self) -> Effect {
        Self::process(pancurses::noecho(), Error::Noecho)
    }

    /// Sets user interface to not wait for an input.
    fn enable_nodelay(&self) -> Effect {
        Self::process(self.window.nodelay(true), Error::Nodelay)
    }

    /// Sets the color of the next specified number of blocks from the cursor.
    fn format(&self, length: i32, color: Color) -> Effect {
        Self::process(
            self.window.chgat(length, pancurses::A_NORMAL, color.cp()),
            Error::Wchgat,
        )
    }

    /// Inserts a character at the cursor, shifting all subsequent blocks to the right.
    fn insert_char(&self, c: char) -> Effect {
        Self::process(self.window.insch(c), Error::Winsch)
    }

    /// Moves the cursor to an [`Address`].
    fn move_to(&self, address: Address) -> Effect {
        Self::process(self.window.mv(address.y(), address.x()), Error::Wmove)
    }

    /// Initializes color processing.
    ///
    /// Must be called before any other color manipulation routine is called.
    fn start_color(&self) -> Effect {
        Self::process(pancurses::start_color(), Error::StartColor)
    }

    /// Initializes the default colors.
    fn use_default_colors(&self) -> Effect {
        Self::process(pancurses::use_default_colors(), Error::UseDefaultColors)
    }
}

impl UserInterface for Terminal {
    #[inline]
    fn init(&self) -> Effect {
        self.start_color()?;
        self.use_default_colors()?;
        self.disable_echo()?;
        self.enable_nodelay()?;
        self.define_color(Color::Red, pancurses::COLOR_RED)?;
        self.define_color(Color::Blue, pancurses::COLOR_BLUE)?;
        Ok(())
    }

    #[inline]
    fn close(&self) -> Effect {
        Self::process(pancurses::endwin(), Error::Endwin)
    }

    #[inline]
    fn flash(&self) -> Effect {
        Self::process(pancurses::flash(), Error::Flash)
    }

    #[inline]
    fn apply(&self, edit: Edit) -> Effect {
        if let Some(address) = edit.address {
            self.move_to(address)?;
        }

        match edit.change {
            Change::Clear => self.clear_all(),
            Change::Format(span, color) => {
                self.move_to(span.first)?;
                self.format(span.length(), color)
            }
            Change::Nothing => Ok(()),
            Change::Flash => self.flash(),
            Change::Text(span, text) => {
                // Currently only support
                // - removing a single character (not ENTER)
                // - inserting text that does not include ENTER
                // - overwriting to the end of the row
                self.move_to(span.first)?;

                if text.is_empty() {
                    self.delete_char()
                } else {
                    if span.first == span.last {
                        for c in text.chars().rev() {
                            self.insert_char(c)?;
                        }
                    } else if span.last.is_end_of_row() {
                        self.add_str(text)?;
                        self.clear_to_row_end()?;
                    }

                    Ok(())
                }
            }
        }
    }

    // TODO: Store this value and update when size is changed.
    #[inline]
    fn grid_height(&self) -> Result<Index, TryFromIntError> {
        Index::try_from(self.window.get_max_y())
    }

    #[inline]
    fn receive_input(&self) -> Option<Input> {
        self.window.getch()
    }
}

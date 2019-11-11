//! Implements the interface between the user and the application.
pub use crate::num::NonNegI32 as Index;

use core::{convert::TryFrom, num::TryFromIntError};
use displaydoc::Display as DisplayDoc;
use pancurses::Input;
use std::{
    error,
    fmt::{self, Debug, Display, Formatter},
};

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
#[derive(Clone, Copy, Debug, DisplayDoc)]
pub enum Error {
    /// error during call to `endwin()`
    Endwin,
    /// error during call to `flash()`
    Flash,
    /// error during call to `init_pair()`
    InitPair,
    /// error during call to `noecho()`
    Noecho,
    /// error during call to `start_color()`
    StartColor,
    /// error during call to `use_default_colors()`
    UseDefaultColors,
    /// error during call to `waddch()`
    Waddch,
    /// error during call to `waddstr()`
    Waddstr,
    /// error during call to `wchgat()`
    Wchgat,
    /// error during call to `wclear()`
    Wclear,
    /// error during call to `wcleartoeol()`
    Wcleartoeol,
    /// error during call to `wdelch()`
    Wdelch,
    /// error during call to `winsch()`
    Winsch,
    /// error during call to `wmove()`
    Wmove,
    /// error during call to `nodelay()`
    Nodelay,
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
    pub const fn new(row: Index, column: Index) -> Self {
        Self { row, column }
    }

    /// Returns if `Address` represents the end of a row.
    fn is_end_of_row(self) -> bool {
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
    /// The first `Address` included in the `Span`.
    first: Address,
    /// The first `Address` not included in the `Span`.
    last: Address,
}

impl Span {
    /// Creates a new `Span`.
    pub const fn new(first: Address, last: Address) -> Self {
        Self { first, last }
    }

    /// Returns the length of the `Span`.
    ///
    /// Assumes that the `Span` only covers 1 row.
    fn length(&self) -> i32 {
        self.last.x().saturating_sub(self.first.x())
    }
}

impl Display for Span {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "[{} -> {}]", self.first, self.last)
    }
}

/// Signifies a modification to the grid.
#[derive(Clone, Debug, DisplayDoc, Eq, Hash, PartialEq)]
pub enum Change {
    /// Clears all cells.
    Clear,
    /// Formats `{0}` to `{1}`.
    Format(Span, Color),
    /// Does nothing.
    Nothing,
    /// Flashes the display.
    Flash,
    /// Sets `{0}` to display `{1}`.
    Text(Span, String),
}

impl Default for Change {
    #[inline]
    fn default() -> Self {
        Self::Nothing
    }
}

/// Signifies a color.
// Order must be kept as defined to match pancurses.
#[derive(Clone, Copy, Debug, DisplayDoc, Eq, Hash, PartialEq)]
pub enum Color {
    /// default foreground on default background
    Default,
    /// default foreground on red background
    Red,
    /// default foreground on green background
    Green,
    /// default foreground on yellow background
    Yellow,
    /// default foreground on blue background
    Blue,
}

impl Color {
    /// Converts `self` to a `color-pair` as specified in `pancurses`.
    const fn cp(self) -> i16 {
        self as i16
    }
}

/// The user interface provided by a terminal.
///
/// All output is displayed in a grid of cells. Each cell contains one character and can change its
/// background color.
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

    /// Sets up the user interface for use.
    pub fn init(&self) -> Effect {
        self.start_color()?;
        self.use_default_colors()?;
        self.disable_echo()?;
        self.enable_nodelay()?;
        self.define_color(Color::Red, pancurses::COLOR_RED)?;
        self.define_color(Color::Blue, pancurses::COLOR_BLUE)?;
        Ok(())
    }

    /// Closes the user interface.
    pub fn close(&self) -> Effect {
        Self::process(pancurses::endwin(), Error::Endwin)
    }

    /// Flashes the output.
    fn flash(&self) -> Effect {
        Self::process(pancurses::flash(), Error::Flash)
    }

    /// Applies the `Change` to the output.
    pub fn apply(&self, change: Change) -> Effect {
        match change {
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
                    } else {
                        self.add_str(text)?;
                    }

                    Ok(())
                }
            }
        }
    }

    /// Returns the number of cells that make up the height of the grid.
    // TODO: Store this value and update when size is changed.
    pub fn grid_height(&self) -> Result<Index, TryFromIntError> {
        Index::try_from(self.window.get_max_y())
    }

    /// Returns the input from the user.
    ///
    /// Returns [`None`] if no character input is provided.
    pub fn receive_input(&self) -> Option<Input> {
        self.window.getch()
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

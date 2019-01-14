//! The interface between the user and the application.
use pancurses::Input;
use std::fmt;

/// Character that represents the `Backspace` key.
pub(crate) const BACKSPACE: char = '\u{08}';
/// Character that represents the `Enter` key.
pub(crate) const ENTER: char = '\n';
// Currently ESC is set to Ctrl-C to allow manual testing within vim terminal where ESC is already
// mapped.
/// Character that represents the `Esc` key.
pub(crate) const ESC: char = '';

/// The interface between the application and the user.
///
/// All output is displayed in a grid of cells.
#[derive(Debug)]
pub(crate) struct UserInterface {
    /// Interface to the terminal.
    window: pancurses::Window,
}

impl UserInterface {
    /// Creates a new UserInterface.
    pub(crate) fn new() -> UserInterface {
        UserInterface {
            // Must call initscr() first.
            window: pancurses::initscr(),
        }
    }

    /// Sets up the user interface for use.
    pub(crate) fn init(&self) -> Result<(), String> {
        // start_color must be called before any other color manipulation routine is called.
        if pancurses::start_color() != pancurses::OK {
            return Err(String::from("Failed while calling start_color()."))
        }

        if pancurses::use_default_colors() != pancurses::OK {
            return Err(String::from("Failed while calling use_default_colors()."))
        }

        // Prevent curses from outputing keys.
        if pancurses::noecho() != pancurses::OK {
            return Err(String::from("Failed while calling noecho()."))
        }

        if pancurses::start_color() != pancurses::OK {
            return Err(String::from("Failed while calling start_color()."))
        }

        if pancurses::init_pair(Color::Red.cp(), -1, pancurses::COLOR_RED) != pancurses::OK {
            return Err(String::from("Failed while calling init_pair()."))
        }
        
        if pancurses::init_pair(Color::Blue.cp(), -1, pancurses::COLOR_BLUE) != pancurses::OK {
            return Err(String::from("Failed while calling init_pair()."))
        }

        Ok(())
    }

    /// Gets input from the user.
    ///
    /// Returns an [`Option<char>`]. Returns [`None`] if no input is provided.
    ///
    /// [`Option<char>`]: https://doc.rust-lang.org/std/option/enum.Option.html
    /// [`None`]: https://doc.rust-lang.org/std/option/enum.Option.html#variant.None
    pub(crate) fn receive_input(&self) -> Option<char> {
        match self.window.getch() {
            Some(Input::Character(c)) => Some(c),
            _ => None,
        }
    }

    /// Closes the user interface.
    pub(crate) fn close(&self) -> Result<(), String> {
        match pancurses::endwin() {
            pancurses::OK => Ok(()),
            _ => Err(String::from("Failed while calling endwin().")),
        }
    }

    /// Applies edit to display.
    pub(crate) fn apply(&self, edit: Edit) -> Result<(), String> {
        if self.window.mv(edit.region.y(), edit.region.x()) != pancurses::OK {
            return Err(String::from("Failed while calling wmove()"));
        }

        match edit.change {
            Change::Backspace => {
                // Add BACKSPACE (move cursor 1 cell to the left) and delete that character.
                if self.window.addch(BACKSPACE) != pancurses::OK {
                    return Err(String::from("Failed while calling waddch()."));
                }

                if self.window.delch() != pancurses::OK {
                    return Err(String::from("Failed while calling wdelch()."));
                }
            }
            Change::Insert(c) => {
                if self.window.insch(c) != pancurses::OK {
                    return Err(String::from("Failed while calling winsch()."));
                }
            }
            Change::Row(s) => {
                if self.window.addstr(s) != pancurses::OK {
                    return Err(String::from("Failed while calling waddstr()."));
                }

                if self.window.clrtoeol() != pancurses::OK {
                    return Err(String::from("Failed while calling wcleartoeol()."));
                }
            }
            Change::Clear => {
                if self.window.clear() != pancurses::OK {
                    return Err(String::from("Failed while calling wclear()."))
                }
            }
            Change::Format(color) => {
                if self.window.chgat(edit.region.n(), pancurses::A_NORMAL, color.cp()) != pancurses::OK {
                    return Err(String::from("Failed while calling chgat()."))
                }
            }
            Change::Nothing => {}
        }

        Ok(())
    }

    pub(crate) fn flash(&self) -> Result<(), String> {
        match pancurses::flash() {
            pancurses::OK => Ok(()),
            _ => Err(String::from("Failed while calling flash().")),
        }
    }

    // TODO: Store this value and update when size is changed.
    /// Returns the height of the pane.
    pub(crate) fn pane_height(&self) -> usize {
        self.window.get_max_y() as usize
    }
}

impl Default for UserInterface {
    fn default() -> UserInterface {
        UserInterface::new()
    }
}

/// Signifies a [`Change`] to make to a [`Region`].
///
/// [`Change`]s that act on an [`Address`] are executed on the starting [`Address`] of the
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

impl fmt::Display for Change {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}", self)
    }
}

/// Signifies a color.
#[derive(Clone, Eq, PartialEq, Hash, Debug)]
pub(crate) enum Color {
    Default,
    Red,
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

    /// Returns the column at which `self` starts.
    ///
    /// Used with [`pancurses`].
    ///
    /// [`pancurses`]: ../../pancurses/index.html
    fn x(&self) -> i32 {
        self.start.x()
    }

    /// Returns the row at which `self` starts.
    ///
    /// Used with [`pancurses`].
    ///
    /// [`pancurses`]: ../../pancurses/index.html
    fn y(&self) -> i32 {
        self.start.y()
    }

    /// Returns the length of the region as specified by [`pancurses`].
    ///
    /// [`pancurses`]: ../../pancurses/index.html
    fn n(&self) -> i32 {
        self.length.0
    }
}

impl fmt::Display for Region {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
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

impl fmt::Display for Address {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
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
    pub(crate) fn to_usize(&self) -> usize {
        // Given that Length was created by from(), this conversion is safe.
        self.0 as usize
    }
}

impl fmt::Display for Length {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
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

//! The interface between the user and the application.
use pancurses::Input;
use std::fmt;

/// Character that represents the `Backspace` key.
pub const BACKSPACE: char = '\u{08}';
/// Character that represents the `Enter` key.
pub const ENTER: char = '\n';
// Currently Ctrl + C to allow manual testing within vim terminal where ESC is already mapped.
/// Character that represents the `Esc` key.
pub const ESC: char = '';

/// The interface with the user.
///
/// All output is displayed in a grid. A cursor is tracked and used to specify where requested
/// outputs appear.
#[derive(Debug)]
pub struct UserInterface {
    /// Interface to the terminal.
    window: pancurses::Window,
}

impl UserInterface {
    /// Creates a new UserInterface.
    pub fn new() -> UserInterface {
        UserInterface {
            // Must call initscr() first.
            window: pancurses::initscr(),
        }
    }

    /// Sets up the user interface for use.
    pub fn init(&self) {
        // Prevent curses from outputing keys.
        pancurses::noecho();

        pancurses::start_color();
        pancurses::use_default_colors();
        pancurses::init_pair(0, -1, -1);
        pancurses::init_pair(1, -1, pancurses::COLOR_RED);
        pancurses::init_pair(2, -1, pancurses::COLOR_BLUE);
    }

    /// Gets input from the user.
    ///
    /// Returns an [`Option<char>`]. Returns [`None`] if no input is provided.
    ///
    /// [`Option<char>`]: https://doc.rust-lang.org/std/option/enum.Option.html
    /// [`None`]: https://doc.rust-lang.org/std/option/enum.Option.html#variant.None
    pub fn receive_input(&self) -> Option<char> {
        match self.window.getch() {
            Some(Input::Character(c)) => Some(c),
            _ => None,
        }
    }

    /// Closes the user interface.
    pub fn close(&self) {
        pancurses::endwin();
    }

    /// Applies edit to display.
    pub fn apply(&self, edit: Edit) {
        if let Some(region) = edit.region {
            self.window.mv(region.y(), region.x());
        }

        match edit.change {
            Change::Backspace => {
                // Adding BACKSPACE moves cursor 1 cell to the left but does not delete any
                // characters.
                self.window.addch(BACKSPACE);
                self.window.delch();
            }
            Change::Insert(c) => {
                self.window.insch(c);
            }
            Change::Row(s) => {
                self.window.addstr(s);
                self.window.clrtoeol();
            }
            Change::Clear => {
                self.window.clear();
            }
            Change::Format(color) => {
                if let Some(region) = edit.region {
                    self.window.chgat(region.n(), pancurses::A_NORMAL, color);
                }
            }
        }
    }

    /// Returns the height of the pane.
    pub fn window_height(&self) -> usize {
        self.window.get_max_y() as usize
    }
}

impl Default for UserInterface {
    fn default() -> UserInterface {
        UserInterface::new()
    }
}

/// Indicates a [`Change`] for the [`UserInterface`] to make at an [`Address`].
pub struct Edit {
    region: Option<Region>,
    change: Change,
}

impl Edit {
    /// Creates a new [`Edit`].
    pub fn new(region: Option<Region>, change: Change) -> Edit {
        Edit { region, change }
    }
}

/// Indicates a change for the [`UserInterface`] to make.
#[derive(Clone, Eq, PartialEq, Hash, Debug)]
pub enum Change {
    /// Removes the previous character, moving all subsequent characters to the left.
    Backspace,
    /// Inserts a character, moving all subsequent characters to the right.
    Insert(char),
    /// Adds a string at the current position, removing all subsequent characters.
    Row(String),
    /// Clears the entire window.
    Clear,
    /// Sets formatting of region.
    ///
    /// For now, this just handles color.
    Format(i16),
}

impl fmt::Display for Change {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{:?}", self)
    }
}

/// Specifies a group of adjacent Addresses.
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug, Default)]
pub struct Region {
    /// First Address in the region.
    start: Address,
    /// The number of included Addresses.
    length: Length,
}

impl Region {
    /// Creates a Region
    pub fn new(start: Address, length: Length) -> Region {
        Region { start, length }
    }

    pub fn address(address: Address) -> Region {
        Region {
            start: address,
            length: Length::from(1),
        }
    }

    pub fn row(row: usize) -> Region {
        Region {
            start: Address::new(row, 0),
            length: EOL,
        }
    }

    fn y(&self) -> i32 {
        self.start.y()
    }

    fn x(&self) -> i32 {
        self.start.x()
    }

    fn n(&self) -> i32 {
        *self.length.as_i32()
    }
}

impl fmt::Display for Region {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}->{}", self.start, self.length)
    }
}

/// Location of a block in the pane.
#[derive(Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Debug, Default)]
pub struct Address {
    /// Index of the row that contains the block (including 0).
    row: usize,
    /// Index of the column that contains the block (including 0).
    column: usize,
}

impl Address {
    /// Creates a new Address at a given row and column.
    pub fn new(row: usize, column: usize) -> Address {
        Address { row, column }
    }

    /// Returns the column of address.
    fn x(&self) -> i32 {
        self.column as i32
    }

    /// Returns the row of address.
    fn y(&self) -> i32 {
        self.row as i32
    }
}

impl fmt::Display for Address {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "({}, {})", self.row, self.column)
    }
}

/// Specifies the length of a Region.
#[derive(Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Debug, Default)]
pub struct Length(i32);

/// Value that represents the number of characters until the end of the line.
const EOL_VALUE: i32 = -1;
/// Length that represents the number of characters until the end of the line.
pub const EOL: Length = Length(EOL_VALUE);

impl Length {
    /// Converts to usize.
    pub fn to_usize(&self) -> usize {
        self.0 as usize
    }

    /// Converts to i32.
    pub fn as_i32(&self) -> &i32 {
        &self.0
    }
}

impl fmt::Display for Length {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self.0 {
            EOL_VALUE => write!(f, "EOL"),
            x => write!(f, "{}", x),
        }
    }
}

impl From<usize> for Length {
    fn from(value: usize) -> Length {
        Length(value as i32)
    }
}

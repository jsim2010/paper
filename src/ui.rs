//! The interface between the user and the application.
extern crate pancurses;

use self::pancurses::Input;
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
    pub fn get_input(&self) -> Option<char> {
        match self.window.getch() {
            Some(Input::Character(c)) => Some(c),
            _ => None,
        }
    }

    /// Closes the user interface.
    pub fn close(&self) {
        pancurses::endwin();
    }

    /// Clears the output.
    pub fn clear(&self) {
        self.window.clear();
    }

    /// Moves back 1 block and deletes the character there.
    ///
    /// All subsequent characters move one block to the left.
    pub fn delete_back(&self) {
        self.window.addch(BACKSPACE);
        self.window.delch();
    }

    /// Outputs a char, moving all subsequent characters to the right.
    pub fn insert_char(&self, c: char) {
        self.window.insch(c);
    }

    /// Changes the background color of a [`Region`].
    ///
    /// [`Region`]: struct.Region.html
    pub fn set_background(&self, region: &Region, color_pair: i16) {
        self.window.mvchgat(
            region.y(),
            region.x(),
            region.n(),
            pancurses::A_NORMAL,
            color_pair,
        );
    }

    /// Outputs a line.
    pub fn set_line(&self, row: usize, line: String) {
        self.window.mv(row as i32, 0);
        self.window.addstr(line);
    }

    /// Moves the cursor to an [`Address`].
    ///
    /// [`Address`]: .struct.Address.html
    pub fn move_to(&self, address: &Address) {
        self.window.mv(address.y(), address.x());
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

/// Specifies a group of adjacent Addresses.
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug, Default)]
pub struct Region {
    /// First Address in the region.
    start: Address,
    /// The number of included Addresses.
    length: Length,
}

impl Region {
    /// Creates a Region with given address and length.
    pub fn with_address_length(address: Address, length: Length) -> Region {
        Region {
            start: address,
            length,
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
    pub fn with_row_column(row: usize, column: usize) -> Address {
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
            _ => write!(f, "{}", self.0),
        }
    }
}

impl From<usize> for Length {
    fn from(value: usize) -> Length {
        Length(value as i32)
    }
}

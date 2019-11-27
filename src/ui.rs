//! Implements the interface between the user and the application.
pub(crate) use crate::num::NonNegI32 as Index;

use clap::ArgMatches;
use core::{
    convert::TryFrom,
    fmt::{self, Debug, Display, Formatter},
    num::TryFromIntError,
};
use lsp_types::{Position, TextEdit};
use pancurses;
use parse_display::Display as ParseDisplay;

/// A [`Result`] returned by a function that does not return anything but can error.
type UnitResult = Result<(), Error>;

/// The character that represents the `Backspace` key.
const BACKSPACE: char = '\u{08}';
/// The character that represents the `Enter` key.
const ENTER: char = '\n';
/// The character that represents the `Esc` key.
// Currently ESC is set to Ctrl-C to allow manual testing within vim terminal where ESC is already
// mapped.
const ESC: char = '';

/// Describes possible errors during ui functions.
#[derive(Clone, Copy, Debug, ParseDisplay)]
#[display("error during call to `{}()`")]
#[display(style = "snake_case")]
#[allow(clippy::missing_docs_in_private_items)] // Documentation would be repetitive.
pub enum Error {
    Endwin,
    Flash,
    InitPair,
    Noecho,
    StartColor,
    UseDefaultColors,
    Waddch,
    Waddstr,
    Wchgat,
    Wclear,
    Wcleartoeol,
    Wdelch,
    Winsch,
    Wmove,
    Nodelay,
    Getmaxy,
}

/// Signifies a command-line argument.
#[derive(Clone, Debug)]
pub(crate) enum Argument {
    /// The `file` command argument.
    File(String),
}

/// Signifies input provided by the user.
#[derive(Clone, Debug)]
pub(crate) enum Input {
    /// A key that represents a printable character.
    Char(char),
    /// The `Enter` key
    Enter,
    /// The `Esc` key.
    Escape,
    /// The `Backspace` key.
    Backspace,
    /// Command arguments.
    Arg(Argument),
}

/// Signifies a specific cell in the grid.
#[derive(Clone, Copy, Eq, Debug, Default, Hash, Ord, PartialEq, PartialOrd)]
pub(crate) struct Address {
    /// The index of the row that contains the cell (starts at 0).
    row: Index,
    /// The index of the column that contains the cell (starts at 0).
    column: Index,
}

impl Address {
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

impl TryFrom<Position> for Address {
    type Error = TryFromIntError;

    fn try_from(value: Position) -> Result<Self, Self::Error> {
        Ok(Self {
            row: Index::try_from(value.line)?,
            column: Index::try_from(value.line)?,
        })
    }
}

/// Signifies a modification to the grid.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum Change {
    /// Modifies the text of the current document.
    Text(Vec<TextEdit>),
}

/// The user interface provided by a terminal.
///
/// All output is displayed in a grid of cells. Each cell contains one character and can change its
/// background color.
#[derive(Debug, Default)]
pub(crate) struct Terminal {
    /// The window that interfaces with the application.
    api: PancursesWrapper,
    /// Inputs from command arguments.
    arg_inputs: Vec<Argument>,
}

impl Terminal {
    /// Sets up the user interface for use.
    pub(crate) fn init(&mut self, args: &ArgMatches<'_>) -> UnitResult {
        self.api.start_color()?;
        self.api.use_default_colors()?;
        self.api.disable_echo()?;
        self.api.enable_nodelay()?;

        if let Some(file) = args.value_of("file").map(str::to_string) {
            self.arg_inputs.push(Argument::File(file))
        }

        Ok(())
    }

    /// Closes the user interface.
    pub(crate) fn stop(&self) -> UnitResult {
        self.api.stop()
    }

    /// Applies `change` to the output.
    pub(crate) fn apply(&self, change: Change) -> UnitResult {
        match change {
            Change::Text(edits) => {
                for edit in edits {
                    if let Ok(start) = Address::try_from(edit.range.start) {
                        let mut new_text_len = edit.new_text.len() as u64;
                        self.api.move_to(start)?;
                        self.api.add_string(edit.new_text)?;

                        if edit.range.end.character == u64::max_value() {
                            self.api.clear_to_row_end()?;
                        } else {
                            new_text_len -= edit
                                .range
                                .end
                                .character
                                .saturating_sub(edit.range.start.character);

                            for _ in 0..new_text_len {
                                self.api.delete_char()?;
                            }
                        }
                    }
                }
            }
        }

        Ok(())
    }

    /// Returns the input from the user.
    ///
    /// Returns [`None`] if no input is provided.
    pub(crate) fn input(&mut self) -> Option<Input> {
        // First check arg inputs, then check for key input.
        self.arg_inputs
            .pop()
            .map(Input::Arg)
            .or_else(|| self.api.get_char())
    }
}

/// The interface with [`pancurses`].
///
/// Provides a wrapper to "Rustify" the input and return values of [`pancurses`] methods.
#[derive(Debug)]
struct PancursesWrapper {
    /// The [`pancurses`] window.
    window: pancurses::Window,
}

impl PancursesWrapper {
    /// Converts `status`, the return value of a `pancurses` function, to an [`UnitResult`].
    fn result(status: i32, error: Error) -> UnitResult {
        if status == pancurses::OK {
            Ok(())
        } else {
            Err(error)
        }
    }

    /// Writes a string starting at the cursor.
    fn add_string(&self, s: String) -> UnitResult {
        Self::result(self.window.addstr(s), Error::Waddstr)
    }

    /// Clears all blocks from the cursor to the end of the row.
    fn clear_to_row_end(&self) -> UnitResult {
        Self::result(self.window.clrtoeol(), Error::Wcleartoeol)
    }

    /// Deletes the character at the cursor.
    ///
    /// All subseqent characters are shifted to the left and a blank block is added at the end.
    fn delete_char(&self) -> UnitResult {
        Self::result(self.window.delch(), Error::Wdelch)
    }

    /// Disables echoing received characters on the screen.
    fn disable_echo(&self) -> UnitResult {
        Self::result(pancurses::noecho(), Error::Noecho)
    }

    /// Sets user interface to not wait for an input.
    fn enable_nodelay(&self) -> UnitResult {
        Self::result(self.window.nodelay(true), Error::Nodelay)
    }

    ///// Returns the height, in cells, of the terminal window.
    //fn get_height(&self) -> Result<i32, Error> {
    //    let max_y = self.window.get_max_y();

    //    if max_y.is_negative() {
    //        Err(Error::Getmaxy)
    //    } else {
    //        Ok(max_y)
    //    }
    //}

    /// Moves the cursor to an [`Address`].
    fn move_to(&self, address: Address) -> UnitResult {
        Self::result(self.window.mv(address.y(), address.x()), Error::Wmove)
    }

    /// Initializes color processing.
    ///
    /// Must be called before any other color manipulation routine is called.
    fn start_color(&self) -> UnitResult {
        Self::result(pancurses::start_color(), Error::StartColor)
    }

    /// Initializes the default colors.
    fn use_default_colors(&self) -> UnitResult {
        Self::result(pancurses::use_default_colors(), Error::UseDefaultColors)
    }

    /// Stops the user interface.
    pub(crate) fn stop(&self) -> UnitResult {
        Self::result(pancurses::endwin(), Error::Endwin)
    }

    /// Returns the input from the terminal window.
    pub(crate) fn get_char(&self) -> Option<Input> {
        self.window.getch().and_then(|input| {
            // TODO: Change this to be a try_from.
            if let pancurses::Input::Character(c) = input {
                Some(match c {
                    ENTER => Input::Enter,
                    ESC => Input::Escape,
                    BACKSPACE => Input::Backspace,
                    _ => Input::Char(c),
                })
            } else {
                None
            }
        })
    }
}

impl Default for PancursesWrapper {
    fn default() -> Self {
        Self {
            // Must call initscr() first.
            window: pancurses::initscr(),
        }
    }
}

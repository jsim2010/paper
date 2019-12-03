//! Implements the interface between the user and the application.
pub(crate) use api::{Error, Input, Config};

use api::{Address, Window};
use core::convert::TryFrom;
use clap::ArgMatches;
use core::fmt::Debug;
use log::trace;
use lsp_types::{ShowMessageParams, TextEdit};

/// Signifies a modification to the grid.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum Change {
    /// Modifies the text of the current document.
    Text(Vec<TextEdit>),
    /// Displays a message to the user.
    Alert(ShowMessageParams),
}

/// Settings of the application.
#[derive(Debug)]
pub struct Settings {
    /// The file to be viewed.
    file: Option<String>,
}

impl From<ArgMatches<'_>> for Settings {
    fn from(value: ArgMatches<'_>) -> Self {
        Self {
            file: value.value_of("file").map(str::to_string),
        }
    }
}

/// The user interface provided by a terminal.
///
/// All output is displayed in a grid of cells. Each cell contains one character and can change its
/// background color.
#[derive(Debug)]
pub(crate) struct Terminal {
    /// The main window.
    window: Window,
    /// The [`Window`] to display notifications.
    notification: Window,
    /// Inputs from command arguments.
    arg_inputs: Vec<Config>,
}

impl Terminal {
    /// Creates a new `Terminal`.
    pub(crate) fn new() -> Result<Self, Error> {
        // Must call initscr() first.
        let window = api::init_curses();

        Ok(Self {
            notification: window.create_subwindow()?,
            window,
            arg_inputs: Vec::new(),
        })
    }

    /// Sets up the user interface for use.
    pub(crate) fn init(&mut self, settings: Settings) -> api::UnitResult {
        api::start_color()?;
        api::use_default_colors()?;
        api::disable_echo()?;
        self.window.enable_nodelay()?;

        if let Some(file) = settings.file {
            self.arg_inputs.push(Config::File(file))
        }

        Ok(())
    }

    /// Closes the user interface.
    pub(crate) fn stop(&self) -> api::UnitResult {
        api::quit()
    }

    /// Applies `change` to the output.
    pub(crate) fn apply(&self, change: Change) -> api::UnitResult {
        match change {
            Change::Text(edits) => {
                for edit in edits {
                    if let Ok(start) = Address::try_from(edit.range.start) {
                        let mut new_text_len = edit.new_text.len() as u64;
                        self.window.move_to(start)?;
                        self.window.add_string(edit.new_text)?;

                        if edit.range.end.character == u64::max_value() {
                            self.window.clear_to_row_end()?;
                        } else {
                            new_text_len -= edit
                                .range
                                .end
                                .character
                                .saturating_sub(edit.range.start.character);

                            for _ in 0..new_text_len {
                                self.window.delete_char()?;
                            }
                        }
                    }
                }
            }
            Change::Alert(alert) => {
                trace!("alert: {:?} {}", alert.typ, alert.message);
                self.notification.add_string(alert.message)?;
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
            .map(Input::Config)
            .or_else(|| self.window.get_char())
    }
}

/// The interface with [`pancurses`].
///
/// Provides a wrapper to "Rustify" the input and return values of [`pancurses`] functions.
mod api {
    pub(crate) use crate::num::NonNegI32 as Index;

    use core::{
        convert::TryFrom,
        num::TryFromIntError,
    };
    use lsp_types::Position;
    use parse_display::Display as ParseDisplay;
    use std::fmt;

    /// The character that represents the `Backspace` key.
    const BACKSPACE: char = '\u{08}';
    /// The character that represents the `Enter` key.
    const ENTER: char = '\n';
    /// The character that represents the `Esc` key.
    // Currently ESC is set to Ctrl-C to allow manual testing within vim terminal where ESC is already
    // mapped.
    const ESC: char = '';

    /// A [`Result`] returned by a function that does not return anything but can error.
    pub(crate) type UnitResult = Result<(), Error>;

    /// Describes possible errors during ui functions.
    #[derive(Clone, Copy, Debug, ParseDisplay)]
    #[display("during call to `{}()`")]
    #[display(style = "snake_case")]
    #[allow(clippy::missing_docs_in_private_items)]// Documentation would be repetitive.
    pub enum Error {
        Endwin,
        Flash,
        InitPair,
        Noecho,
        StartColor,
        Subwin,
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
        Getmaxx,
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

    impl fmt::Display for Address {
        #[inline]
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
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

    /// Signifies a configuration.
    #[derive(Clone, Debug)]
    pub(crate) enum Config {
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
        /// Configuration.
        Config(Config),
    }

    /// A wrapper around [`pancurses::Window`].
    #[derive(Debug)]
    pub(crate) struct Window(pancurses::Window);

    impl Window {
        /// Writes a string starting at the cursor.
        pub(crate) fn add_string(&self, s: String) -> UnitResult {
            result(self.0.addstr(s), Error::Waddstr)
        }

        /// Clears all blocks from the cursor to the end of the row.
        pub(crate) fn clear_to_row_end(&self) -> UnitResult {
            result(self.0.clrtoeol(), Error::Wcleartoeol)
        }

        /// Creates a subwindow.
        pub(crate) fn create_subwindow(&self) -> Result<Self, Error> {
            self.0.subwin(10, i32::from(self.get_columns()?), 0, 0).map(Self).map_err(|_| Error::Subwin)
        }

        /// Deletes the character at the cursor.
        ///
        /// All subseqent characters are shifted to the left and a blank block is added at the end.
        pub(crate) fn delete_char(&self) -> UnitResult {
            result(self.0.delch(), Error::Wdelch)
        }

        /// Sets user interface to not wait for an input.
        pub(crate) fn enable_nodelay(&self) -> UnitResult {
            result(self.0.nodelay(true), Error::Nodelay)
        }

        /// Returns the number of columns of the `Window`.
        pub(crate) fn get_columns(&self) -> Result<Index, Error> {
            Index::try_from(self.0.get_max_x()).map_err(|_| Error::Getmaxx)
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
        pub(crate) fn move_to(&self, address: Address) -> UnitResult {
            result(self.0.mv(address.y(), address.x()), Error::Wmove)
        }

        /// Returns the input from the terminal window.
        pub(crate) fn get_char(&self) -> Option<Input> {
            self.0.getch().and_then(|input| {
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

    /// Converts `status`, the return value of a `pancurses` function, to an [`UnitResult`].
    pub(crate) fn result(status: i32, error: Error) -> UnitResult {
        if status == pancurses::OK {
            Ok(())
        } else {
            Err(error)
        }
    }

    /// Disables echoing received characters on the screen.
    pub(crate) fn disable_echo() -> UnitResult {
        result(pancurses::noecho(), Error::Noecho)
    }

    /// Initializes curses.
    pub(crate) fn init_curses() -> Window {
        Window(pancurses::initscr())
    }

    /// Stops the user interface.
    pub(crate) fn quit() -> UnitResult {
        result(pancurses::endwin(), Error::Endwin)
    }

    /// Initializes color processing.
    ///
    /// Must be called before any other color manipulation routine is called.
    pub(crate) fn start_color() -> UnitResult {
        result(pancurses::start_color(), Error::StartColor)
    }

    /// Initializes the default colors.
    pub(crate) fn use_default_colors() -> UnitResult {
        result(pancurses::use_default_colors(), Error::UseDefaultColors)
    }
}

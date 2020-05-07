//! Implements the interface between the user and the application.
//!
//! Visual output is organized as follows:
//! - A header is displayed on a single row at the top of the display. The header displays general information about the current state of the system.
//! - All remaining space on the screen is primarily used for displaying the text of the currently viewed document.
//! - If the application needs to alert the user, it may do so via a message box that will temporarily overlap the top rows of the document.
//! - If the application requires input from the user, it may do so via an input box that will temporarily overlap the bottom rows of the document.
mod error;

pub use error::{CreateTerminalError, DisplayCmdFailure, UserActionFailure};

use {
    core::{
        cell::{RefCell, RefMut},
        ops::Deref,
        time::Duration,
    },
    crossterm::{
        cursor::{Hide, MoveTo},
        event::{self, Event, KeyCode, KeyEvent, KeyModifiers},
        execute,
        style::Print,
        terminal::{EnterAlternateScreen, LeaveAlternateScreen},
    },
    error::{WrapFailure, DestroyError, InitError, PollFailure, ReadFailure, WriteFailure},
    fehler::{throw, throws},
    log::{trace, warn},
    market::{ConsumeError, Consumer, ProduceError, Producer},
    parse_display::Display as ParseDisplay,
    std::io::{self, Stdout, Write},
};

/// A instantaneous duration of time.
static NO_DURATION: Duration = Duration::from_secs(0);

/// Returns if a [`UserAction`] is available.
#[throws(PollFailure)]
fn is_action_available() -> bool {
    event::poll(NO_DURATION)?
}

/// Reads a current [`UserAction`], blocking until one is received.
#[throws(ReadFailure)]
fn read_action() -> UserAction {
    event::read().map(UserAction::from)?
}

/// Consumes all [`UserAction`]s from the user.
#[derive(Debug, Default)]
pub(crate) struct UserActionConsumer;

impl UserActionConsumer {
    /// Creates a new [`UserActionConsumer`].
    pub(crate) fn new() -> Self {
        Self::default()
    }
}

impl Consumer for UserActionConsumer {
    type Good = UserAction;
    type Failure = UserActionFailure;

    #[throws(ConsumeError<Self::Failure>)]
    fn consume(&self) -> Self::Good {
        if is_action_available().map_err(|error| ConsumeError::Failure(error.into()))? {
            read_action().map_err(|error| ConsumeError::Failure(error.into()))?
        } else {
            throw!(ConsumeError::EmptyStock);
        }
    }
}

/// Produces all [`DisplayCmd`]s via the stdout of the application.
#[derive(Debug, Default)]
pub(crate) struct Terminal {
    /// Presents [`DisplayCmd`]s to the user.
    presenter: Presenter,
}

impl Terminal {
    /// Creates and initializes a new [`Terminal`].
    #[throws(CreateTerminalError)]
    pub(crate) fn new() -> Self {
        let terminal = Self::default();

        terminal.presenter.init()?;
        terminal
    }
}

impl Drop for Terminal {
    fn drop(&mut self) {
        if let Err(error) = self.presenter.destroy() {
            warn!("Error while destroying user interface: {}", error);
        }
    }
}

impl Producer for Terminal {
    type Good = DisplayCmd;
    type Failure = DisplayCmdFailure;

    #[throws(ProduceError<Self::Failure>)]
    fn produce(&self, good: Self::Good) {
        match good {
            DisplayCmd::Rows { rows } => {
                let mut row = RowId(0);

                for text in rows {
                    self.presenter.single_line(row.into(), text.to_string()).map_err(|failure| ProduceError::Failure(failure.into()))?;
                    row.step_forward().map_err(|failure| ProduceError::Failure(failure.into()))?;
                }
            }
            DisplayCmd::Header { header } => {
                self.presenter.single_line(Unit(0), header).map_err(|failure| ProduceError::Failure(failure.into()))?;
            }
        }
    }
}

/// Controls the display to the user.
#[derive(Debug)]
struct Presenter {
    /// The stdout of the application.
    out: RefCell<Stdout>,
}

impl Presenter {
    /// Returns a mutable reference to the [`Stdout`] of the application.
    fn out_mut(&self) -> RefMut<'_, Stdout> {
        self.out.borrow_mut()
    }

    /// Initializes the interface, saving the current display and hiding the cursor.
    #[throws(InitError)]
    fn init(&self) {
        execute!(self.out_mut(), EnterAlternateScreen, Hide)?;
    }

    /// Closes out the interface display, returning to the display prior to initialization.
    #[throws(DestroyError)]
    fn destroy(&self) {
        execute!(self.out_mut(), LeaveAlternateScreen)?;
    }

    /// Writes `text` at `row`.
    #[throws(WriteFailure)]
    fn single_line(&self, row: Unit, text: String) {
        trace!("Writing to {}: `{}`", row, text);
        execute!(self.out_mut(), MoveTo(0, *Unit::from(row)), Print(text))?;
    }
}

impl Default for Presenter {
    fn default() -> Self {
        Self {
            out: RefCell::new(io::stdout()),
        }
    }
}

/// Input generated by the user.
#[derive(Clone, Copy, Debug)]
pub enum UserAction {
    /// The dimensions of the page have been updated.
    Resize {
        /// The new dimensions.
        dimensions: Dimensions,
    },
    /// A mouse event has occurred.
    Mouse,
    /// A key has been pressed.
    Key {
        /// The key.
        code: KeyCode,
        /// The modifiers held when the key was pressed.
        modifiers: KeyModifiers,
    },
}

impl From<Event> for UserAction {
    #[inline]
    fn from(value: Event) -> Self {
        match value {
            Event::Resize(columns, rows) => Self::Resize {
                dimensions: Dimensions {
                    // Reserve the top row for the header.
                    height: (rows - 1).into(),
                    width: columns.into(),
                },
            },
            Event::Mouse(..) => Self::Mouse,
            Event::Key(key) => key.into(),
        }
    }
}

impl From<KeyEvent> for UserAction {
    #[inline]
    fn from(value: KeyEvent) -> Self {
        Self::Key {
            code: value.code,
            modifiers: value.modifiers,
        }
    }
}

/// An output.
#[derive(Debug, ParseDisplay)]
#[display("DisplayCmd")]
pub(crate) enum DisplayCmd {
    /// Display rows of text.
    Rows {
        /// The rows to be displayed.
        rows: Vec<String>,
    },
    Header {
        header: String,
    },
}

/// The dimensions of a grid.
#[derive(Clone, Copy, Debug, Default, Eq, ParseDisplay, PartialEq)]
#[display("{height}h x {width}w")]
pub struct Dimensions {
    /// The number of rows.
    pub(crate) height: Unit,
    /// The number of columns.
    pub(crate) width: Unit,
}

/// The type used for quantities and indexes of cells.
#[derive(Clone, Copy, Debug, Default, Eq, ParseDisplay, PartialEq)]
#[display("{0}")]
pub struct Unit(u16);

impl Deref for Unit {
    type Target = u16;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl From<u16> for Unit {
    fn from(value: u16) -> Self {
        Self(value)
    }
}

impl From<RowId> for Unit {
    fn from(value: RowId) -> Self {
        // Account for header row.
        (*value + 1).into()
    }
}

#[derive(Clone, Copy, Debug, ParseDisplay)]
#[display("{0}")]
pub(crate) struct RowId(u16);

impl RowId {
    #[throws(WrapFailure)]
    fn step_forward(&mut self) {
        self.0 = self.0.checked_add(1).ok_or(WrapFailure)?;
    }
}

impl Deref for RowId {
    type Target = u16;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

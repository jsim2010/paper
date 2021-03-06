//! Implements the interface between the user and the application.
//!
//! Visual output is organized as follows:
//! - A header is displayed on a single row at the top of the display. The header displays general information about the current state of the system.
//! - A command bar is display on the row under the header. The command bar displays the current command being built by the user.
//! - A page is displayed in the remaining space of the display. The page displays the text of the currently viewed document.
mod error;

pub(crate) use error::{CreateTerminalError, DisplayCmdFailure, UserActionFailure};

use {
    core::{
        cell::{RefCell, RefMut},
        convert::TryFrom,
        ops::Deref,
        time::Duration,
    },
    crossterm::{
        cursor::{Hide, MoveTo},
        event::{self, Event, KeyCode, KeyEvent, KeyModifiers},
        execute, queue,
        style::{Color, Print, ResetColor, SetBackgroundColor},
        terminal::{Clear, ClearType, EnterAlternateScreen, LeaveAlternateScreen},
    },
    error::{DestroyError, InitError, PollFailure, ReachedEnd, ReadFailure, WriteFailure},
    fehler::{throw, throws},
    market::{Consumer, ProduceFailure, Producer},
    parse_display::Display as ParseDisplay,
    std::io::{self, Stdout},
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

/// Produces all [`DisplayCmd`]s via the stdout of the application.
#[derive(Debug, Default)]
pub(crate) struct Terminal {
    /// The presenter.
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
            log::warn!("Error while destroying user interface: {}", error);
        }
    }
}

impl Producer for Terminal {
    type Good = DisplayCmd;
    type Failure = ProduceFailure<DisplayCmdFailure>;

    #[throws(Self::Failure)]
    fn produce(&self, good: Self::Good) {
        match good {
            DisplayCmd::Rows { rows } => {
                // The top 2 rows are reserved for the header and command bar.
                let mut row_id = Unit(2);

                for row in rows {
                    self.presenter
                        .single_line(row_id, row.texts)
                        .map_err(|failure| market::ProduceFailure::Fault(failure.into()))?;

                    row_id = row_id
                        .forward_checked(1)
                        .ok_or_else(|| ProduceFailure::Fault(ReachedEnd.into()))?;
                }
            }
            DisplayCmd::Command { command } => {
                self.presenter
                    .single_line(Unit(1), vec![StyledText::new(command, Style::Default)])
                    .map_err(|failure| market::ProduceFailure::Fault(failure.into()))?;
            }
            DisplayCmd::Header { header } => {
                self.presenter
                    .single_line(Unit(0), vec![StyledText::new(header, Style::Default)])
                    .map_err(|failure| market::ProduceFailure::Fault(failure.into()))?;
            }
        }
    }
}

/// A Consumer of [`UserAction`]s.
pub(crate) struct UserActionConsumer;

impl Consumer for UserActionConsumer {
    type Good = UserAction;
    type Failure = market::ConsumeFailure<UserActionFailure>;

    #[throws(Self::Failure)]
    fn consume(&self) -> Self::Good {
        if is_action_available().map_err(|error| market::ConsumeFailure::Fault(error.into()))? {
            read_action().map_err(|error| market::ConsumeFailure::Fault(error.into()))?
        } else {
            throw!(market::ConsumeFailure::EmptyStock);
        }
    }
}

/// Manages the display to the user.
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
        // Required to store out due to macro calling out_mut() multiple times.
        let mut out = self.out_mut();
        execute!(out, EnterAlternateScreen, Hide)?;
    }

    /// Closes out the interface display, returning to the display prior to initialization.
    #[throws(DestroyError)]
    fn destroy(&self) {
        // Required to store out due to macro calling out_mut() multiple times.
        let mut out = self.out_mut();
        execute!(out, LeaveAlternateScreen)?;
    }

    /// Writes `text` at `row`.
    #[throws(WriteFailure)]
    fn single_line(&self, row: Unit, styled_texts: Vec<StyledText>) {
        // Required to store out due to macro calling out_mut() multiple times.
        let mut out = self.out_mut();
        queue!(out, MoveTo(0, *row),)?;

        for styled_text in styled_texts {
            queue!(
                out,
                SetBackgroundColor(styled_text.background()),
                Print(styled_text.text),
            )?;
        }

        execute!(out, ResetColor, Clear(ClearType::UntilNewLine))?;
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
pub(crate) enum UserAction {
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
                    // Reserve the top 2 rows for the header and command bar.
                    height: rows.saturating_sub(2).into(),
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
        rows: Vec<RowText>,
    },
    /// Display the command.
    Command {
        /// The text of the command bar.
        command: String,
    },
    /// Displays the header.
    Header {
        /// The header text.
        header: String,
    },
}

/// Describes the style of a text.
#[derive(Clone, Debug)]
pub(crate) enum Style {
    /// Text is default.
    Default,
    /// Text is selected by the user.
    Selection,
}

/// Describes a text with a given [`Style`].
#[derive(Clone, Debug)]
pub(crate) struct StyledText {
    /// The text.
    text: String,
    /// The [`Style`] of the text.
    style: Style,
}

impl StyledText {
    /// Creates a new [`StyledText`].
    pub(crate) const fn new(text: String, style: Style) -> Self {
        Self { text, style }
    }

    /// Returns the background color of `self`.
    const fn background(&self) -> Color {
        match self.style {
            Style::Default => Color::Reset,
            Style::Selection => Color::DarkGrey,
        }
    }
}

/// Describes the texts that make up a row.
#[derive(Clone, Debug)]
pub(crate) struct RowText {
    /// The [`StyledText`]s that make up a row.
    texts: Vec<StyledText>,
}

impl RowText {
    /// Creates a new [`RowText`].
    pub(crate) fn new(texts: Vec<StyledText>) -> Self {
        Self { texts }
    }
}

/// The dimensions of a grid.
#[derive(Clone, Copy, Debug, Default, Eq, ParseDisplay, PartialEq)]
#[display("{height}h x {width}w")]
pub(crate) struct Dimensions {
    /// The number of rows.
    pub(crate) height: Unit,
    /// The number of columns.
    pub(crate) width: Unit,
}

/// Represents a quantity of cells.
#[derive(Clone, Copy, Debug, Default, Eq, ParseDisplay, PartialEq)]
#[display("{0}")]
pub(crate) struct Unit(u16);

impl Unit {
    /// Moves `self` forward by `count`.
    fn forward_checked(self, count: usize) -> Option<Self> {
        u16::try_from(count)
            .ok()
            .and_then(|increment| self.0.checked_add(increment).map(Self::from))
    }
}

impl Deref for Unit {
    type Target = u16;

    #[inline]
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl From<u16> for Unit {
    #[inline]
    fn from(value: u16) -> Self {
        Self(value)
    }
}

impl From<Unit> for usize {
    #[inline]
    fn from(unit: Unit) -> Self {
        unit.0.into()
    }
}

impl From<Unit> for u16 {
    #[inline]
    fn from(unit: Unit) -> Self {
        unit.0
    }
}

impl From<Unit> for u32 {
    #[inline]
    fn from(unit: Unit) -> Self {
        unit.0.into()
    }
}

impl From<Unit> for u64 {
    #[inline]
    fn from(unit: Unit) -> Self {
        unit.0.into()
    }
}

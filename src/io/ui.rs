//! Implements the interface between the user and the application.
//!
//! Visual output is organized as follows:
//! - A header is displayed on a single row at the top of the display. The header displays general information about the current state of the system.
//! - A command bar is display on the row under the header. The command bar displays the current command being built by the user.
//! - A page is displayed in the remaining space of the display. The page displays the text of the currently viewed document.
use {
    core::{
        convert::TryFrom,
        fmt::{self, Display, Formatter},
        ops::Deref,
        time::Duration,
    },
    crossterm::{
        cursor::{Hide, MoveTo},
        event::{self, Event, KeyCode, KeyEvent, KeyModifiers},
        execute, queue, ErrorKind,
        style::{Color, Print, ResetColor, SetBackgroundColor},
        terminal::{Clear, ClearType, EnterAlternateScreen, LeaveAlternateScreen},
    },
    fehler::{throw, throws},
    market::{Agent, Consumer, Fault, EmptyStock, Failure, Flaws, Flawless, Producer, Recall},
    parse_display::Display as ParseDisplay,
};

/// An error initializing a [`Terminal`].
#[derive(Debug, thiserror::Error)]
#[error("create terminal interface: {0}")]
pub struct InitTerminalError(#[from] ErrorKind);

/// A failure producing terminal output.
#[derive(Debug, thiserror::Error)]
#[error(transparent)]
pub enum DisplayCmdFault {
    /// A failure printing text.
    Print(#[from] PrintFault),
    /// A failure incrementing a row.
    End(#[from] ReachedEnd),
}

impl Flaws for DisplayCmdFault {
    type Insufficiency = Flawless;
    type Defect = Self;
}

/// A [`Fault`] while writing to stdout.
#[derive(Debug, thiserror::Error)]
#[error("writing: {0}")]
pub struct PrintFault(#[from] ErrorKind);

/// A failure consuming a [`UserAction`].
#[derive(Debug, thiserror::Error)]
#[error(transparent)]
pub enum ConsumeActionFault {
    /// A failure polling for a [`UserAction`].
    Poll(#[from] PollFault),
    /// A failure reading a [`UserAction`].
    Read(#[from] ReadFault),
}

/// An error polling for a [`UserAction`].
#[derive(Debug, thiserror::Error)]
#[error("Failed to poll for user action: {0}")]
pub struct PollFault(#[from] ErrorKind);

/// An error reading a [`UserAction`].
#[derive(Debug, thiserror::Error)]
#[error("Failed to read user action: {0}")]
pub struct ReadFault(#[from] ErrorKind);

/// When the [`RowId`] has reached its end.
#[derive(Clone, Copy, Debug, thiserror::Error)]
#[error("")]
pub struct ReachedEnd;

/// Initializes the user interface and returns its [`Producer`] and [`Consumer`].
#[throws(InitTerminalError)]
pub(crate) fn init() -> (Presenter, Listener) {
    execute!(std::io::stdout(), EnterAlternateScreen, Hide)?;
    (Presenter, Listener)
}

/// Writes `text` at `row`.
#[throws(PrintFault)]
fn print(row: Unit, styled_texts: Vec<StyledText>) {
    let mut stdout = std::io::stdout();
    queue!(stdout, MoveTo(0, *row),)?;

    for styled_text in styled_texts {
        queue!(
            stdout,
            SetBackgroundColor(styled_text.background()),
            Print(styled_text.text),
        )?;
    }

    execute!(stdout, ResetColor, Clear(ClearType::UntilNewLine))?;
}

/// Returns if a [`UserAction`] is available.
#[throws(PollFault)]
fn is_action_available() -> bool {
    static NO_DURATION: Duration = Duration::from_secs(0);

    event::poll(NO_DURATION)?
}

/// Reads a current [`UserAction`], blocking until one is received.
#[throws(ReadFault)]
fn read_action() -> UserAction {
    event::read().map(UserAction::from)?
}

/// Produces all [`DisplayCmd`]s via the terminal.
#[derive(Debug)]
pub(crate) struct Presenter;

impl Drop for Presenter {
    fn drop(&mut self) {
        if let Err(error) = execute!(std::io::stdout(), LeaveAlternateScreen) {
            log::warn!("Failed to reset user interface: {}", error);
        }
    }
}

impl Agent for Presenter {
    type Good = DisplayCmd;
}

impl Display for Presenter {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "Presenter")
    }
}

impl Producer for Presenter {
    type Flaws = DisplayCmdFault;

    #[throws(Recall<Self::Flaws, Self::Good>)]
    fn produce(&self, good: Self::Good) {
        match good {
            DisplayCmd::Rows { rows } => {
                // The top 2 rows are reserved for the header and command bar.
                let mut row_id = Unit(2);

                for row in rows {
                    print(row_id, row.texts)
                        .map_err(|fault| self.recall(Fault::Defect(fault.into()), good))?;

                    row_id = row_id
                        .forward_checked(1)
                        .ok_or_else(|| self.recall(Fault::Defect(ReachedEnd.into()), good))?;
                }
            }
            DisplayCmd::Command { command } => {
                print(Unit(1), vec![StyledText::new(command, Style::Default)])
                    .map_err(|fault| self.recall(Fault::Defect(fault.into()), good))?;
            }
            DisplayCmd::Header { header } => {
                print(Unit(0), vec![StyledText::new(header, Style::Default)])
                    .map_err(|fault| self.recall(Fault::Defect(fault.into()), good))?;
            }
        }
    }
}

/// A Consumer of [`UserAction`]s.
pub(crate) struct Listener;

impl Agent for Listener {
    type Good = UserAction;
}

impl Consumer for Listener {
    type Flaws = EmptyStock;

    #[throws(Failure<Self::Flaws>)]
    fn consume(&self) -> Self::Good {
        //if is_action_available().map_err(|fault| ConsumeFailure::Fault(fault.into()))? {
        //    read_action().map_err(|fault| ConsumeFailure::Fault(fault.into()))?
        //} else {
            throw!(EmptyStock::default())
        //}
    }
}

impl Display for Listener {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "Listener")
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

//! Implements the interface between the user and the application.
//!
//! The user is able to provide input via any of the following methods:
//! - key press
//! - mouse event
//! - size change
//!
//! The application delivers the following output to the user via stdout of the command. Output is organized in the following visual manner:
//! - The first row of the screen is the header, which displays information generated by starship.
//! - All remaining space on the screen is primarily used for displaying the text of the currently viewed document.
//! - If the application needs to alert the user, it may do so via a message box that will temporarily overlap the top rows of the document.
//! - If the application requires input from the user, it may do so via an input box that will temporarily overlap the bottom rows of the document.
pub(crate) use crossterm::{
    event::{KeyCode as Key, KeyModifiers as Modifiers},
    ErrorKind,
};

use {
    crate::market::{Consumer, Producer, Queue},
    core::{
        cell::RefCell,
        cmp,
        convert::TryFrom,
        fmt::{self, Debug},
        num,
        ops::{Bound, RangeBounds},
        time::Duration,
    },
    crossterm::{
        cursor::{Hide, MoveTo, RestorePosition, SavePosition},
        event::{self, Event},
        execute, queue,
        style::{Color, Print, ResetColor, SetBackgroundColor},
        terminal::{self, Clear, ClearType, EnterAlternateScreen, LeaveAlternateScreen},
    },
    log::{error, warn},
    lsp_types::{MessageType, Position, Range, ShowMessageParams, ShowMessageRequestParams},
    std::{
        collections::VecDeque,
        io::{self, Stdout, Write},
    },
    thiserror::Error,
};

/// An error creating a [`Terminal`].
///
/// [`Terminal`]: struct.Terminal.html
#[derive(Debug, Error)]
pub enum CreateTerminalError {
    /// An error producing the size of the body.
    #[error("producing body size: {0}")]
    ProduceSize(#[from] <Queue<Input> as Producer<'static>>::Error),
    /// An error initializing the terminal output.
    #[error("initializing output: {0}")]
    Init(#[from] ProduceTerminalOutputError),
}

/// A span of time that equal to no time.
static INSTANT: Duration = Duration::from_secs(0);

/// An error producing terminal output.
#[derive(Debug, Error)]
pub enum ProduceTerminalOutputError {
    /// An error initializing the terminal output.
    #[error("{0}")]
    Init(#[source] ErrorKind),
    /// An error displaying a new document on the terminal.
    #[error("{0}")]
    OpenDoc(#[source] ErrorKind),
    /// An error displaying a document edit on the terminal.
    #[error("{0}")]
    Edit(#[source] ErrorKind),
    /// An error setting the wrap config on a document.
    #[error("{0}")]
    Wrap(#[source] ErrorKind),
    /// An error moving the selection.
    #[error("{0}")]
    MoveSelection(#[source] ErrorKind),
    /// An error setting the header
    #[error("{0}")]
    SetHeader(#[source] ErrorKind),
    /// An error adding a notification.
    #[error("{0}")]
    Notify(#[source] ErrorKind),
    /// An error adding a question.
    #[error("{0}")]
    Question(#[source] ErrorKind),
    /// An error starting an intake.
    #[error("{0}")]
    StartIntake(#[source] ErrorKind),
    /// An error resetting the body.
    #[error("{0}")]
    Reset(#[source] ErrorKind),
    /// An error writing a char.
    #[error("{0}")]
    Write(#[source] ErrorKind),
}

/// An error while executing or queueing a [`crossterm`] command.
///
/// [`crossterm`]: ../../crossterm/index.html
#[derive(Debug, Error)]
#[error("while executing or queueing a terminal command: {0}")]
pub struct CommandError(#[from] ErrorKind);

/// An error while flushing terminal output.
#[derive(Debug, Error)]
#[error("{0}")]
pub struct FlushCommandsError(#[from] io::Error);

/// An error while converting between Selection and Range units.
#[derive(Clone, Copy, Debug, Error)]
#[error("while converting between u64 and usize: {0}")]
pub struct SelectionConversionError(#[from] num::TryFromIntError);

/// An error consuming input.
#[derive(Debug, Error)]
pub enum ConsumeInputError {
    /// Read.
    #[error("")]
    Read(#[from] ErrorKind),
    /// Receives.
    #[error("")]
    Recv(#[from] <Queue<Input> as Consumer>::Error),
}

/// A user interface provided by a terminal.
pub(crate) struct Terminal {
    /// The output of the application.
    out: RefCell<Stdout>,
    /// The body of the screen, where all document text is displayed.
    body: RefCell<Body>,
    /// A queue of [`Input`]s.
    queue: Queue<Input>,
}

#[allow(clippy::unused_self)] // For pull(), will be used when user interface becomes a trait.
impl Terminal {
    /// Creates a new [`Terminal`].
    pub(crate) fn new() -> Result<Self, CreateTerminalError> {
        let queue = Queue::new();

        queue.produce(get_body_size().into())?;

        let terminal = Self {
            out: RefCell::new(io::stdout()),
            body: RefCell::new(Body::default()),
            queue,
        };

        terminal.produce(Output::Init)?;
        Ok(terminal)
    }
}

impl Debug for Terminal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Terminal")
    }
}

impl Drop for Terminal {
    fn drop(&mut self) {
        if execute!(self.out.borrow_mut(), LeaveAlternateScreen).is_err() {
            warn!("Failed to leave alternate screen");
        }
    }
}

impl Consumer for Terminal {
    type Good = Input;
    type Error = ConsumeInputError;

    fn can_consume(&self) -> bool {
        self.queue.can_consume()
            || match event::poll(INSTANT) {
                Ok(has_event) => has_event,
                Err(_) => true,
            }
    }

    fn consume(&self) -> Result<Self::Good, Self::Error> {
        if self.queue.can_consume() {
            Ok(self.queue.consume()?)
        } else {
            Ok(event::read()
                .map(|event| event.into())?)
        }
    }
}

impl<'a> Producer<'a> for Terminal {
    type Good = Output<'a>;
    type Error = ProduceTerminalOutputError;

    fn produce(&'a self, good: Self::Good) -> Result<(), Self::Error> {
        match good {
            Output::Init => execute!(self.out.borrow_mut(), EnterAlternateScreen, Hide)
                .map_err(Self::Error::Init),
            Output::OpenDoc { text } => self
                .body
                .borrow_mut()
                .open(text)
                .map_err(Self::Error::OpenDoc),
            Output::Wrap {
                is_wrapped,
                selection,
            } => {
                self.body.borrow_mut().is_wrapped = is_wrapped;
                self.body
                    .borrow_mut()
                    .refresh(selection)
                    .map_err(Self::Error::Wrap)
            }
            Output::Edit {
                new_text,
                selection,
            } => {
                self.body.borrow_mut().edit(&new_text, *selection);
                self.body
                    .borrow_mut()
                    .refresh(selection)
                    .map_err(Self::Error::Edit)
            }
            Output::MoveSelection { selection } => self
                .body
                .borrow_mut()
                .refresh(selection)
                .map_err(Self::Error::MoveSelection),
            Output::SetHeader { header } => execute!(
                self.out.borrow_mut(),
                SavePosition,
                MoveTo(0, 0),
                Print(header),
                RestorePosition
            )
            .map_err(Self::Error::SetHeader),
            Output::Resize { size } => {
                self.body.borrow_mut().size = size.0;
                Ok(())
            }
            Output::Notify { message } => self
                .body
                .borrow_mut()
                .add_alert(&message.message, message.typ)
                .map_err(Self::Error::Notify),
            Output::Question { request } => {
                // TODO: Add implementation to use actions.
                self.body
                    .borrow_mut()
                    .add_alert(&request.message, request.typ)
                    .map_err(Self::Error::Question)
            }
            Output::StartIntake { title } => self
                .body
                .borrow_mut()
                .add_intake(title)
                .map_err(Self::Error::StartIntake),
            Output::Reset { selection } => self
                .body
                .borrow_mut()
                .reset(selection)
                .map_err(Self::Error::Reset),
            Output::Write { ch } => {
                execute!(self.out.borrow_mut(), Print(ch)).map_err(Self::Error::Write)
            }
        }
    }
}

/// Input generated by the user.
#[derive(Clone, Copy, Debug)]
pub enum Input {
    /// The space available for display has been resized.
    ///
    /// The parameter is the new size of the body.
    #[allow(dead_code)] // False positive.
    Resize(BodySize),
    /// A mouse event has occurred.
    Mouse,
    /// A key has been pressed.
    #[allow(dead_code)] // False positive.
    Key {
        /// The keycode of the key.
        key: Key,
        /// All modifier keys that were held when the key was pressed.
        modifiers: Modifiers,
    },
}

impl From<Event> for Input {
    #[inline]
    fn from(value: Event) -> Self {
        match value {
            Event::Resize(columns, rows) => Self::Resize(TerminalSize::new(rows, columns).into()),
            Event::Mouse(..) => Self::Mouse,
            Event::Key(key) => Self::Key {
                key: key.code,
                modifiers: key.modifiers,
            },
        }
    }
}

impl From<BodySize> for Input {
    #[inline]
    fn from(value: BodySize) -> Self {
        Self::Resize(value)
    }
}

/// An output.
pub(crate) enum Output<'a> {
    /// Initializes the terminal.
    Init,
    /// Opens a doc.
    OpenDoc {
        /// The text.
        text: &'a str,
    },
    /// Sets the wrapped configuration.
    Wrap {
        /// If the text is wrapped.
        is_wrapped: bool,
        /// The selection.
        selection: &'a Selection,
    },
    /// Sets the text covered by `selection` to `new_text`.
    Edit {
        /// The new text.
        new_text: String,
        /// The selection.
        selection: &'a Selection,
    },
    /// Sets the selection.
    MoveSelection {
        /// The selection.
        selection: &'a Selection,
    },
    /// Sets the header to `header`.
    SetHeader {
        /// The header.
        header: String,
    },
    /// Resizes the body.
    Resize {
        /// The size.
        size: BodySize,
    },
    /// Adds `message`.
    Notify {
        /// The message.
        message: ShowMessageParams,
    },
    /// Adds `request`.
    Question {
        /// The request.
        request: ShowMessageRequestParams,
    },
    /// Adds an intake box to `self` with `title` as the prompt.
    StartIntake {
        /// The title.
        title: String,
    },
    /// Resets `self` with `selection`.
    Reset {
        /// The selection.
        selection: &'a Selection,
    },
    /// Writes `ch`.
    Write {
        /// The char.
        ch: char,
    },
}

/// The dimensions of a grid of [`char`]s.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct Size {
    /// The number of rows.
    pub(crate) rows: UiUnit,
    /// The number of columns.
    pub(crate) columns: UiUnit,
}

/// The size of the body.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BodySize(pub(crate) Size);

impl From<TerminalSize> for BodySize {
    #[inline]
    fn from(value: TerminalSize) -> Self {
        Self(Size {
            // Account for header in first row.
            rows: value.0.rows.saturating_sub(1),
            // Windows command prompt does not print a character in the last reported column.
            columns: value.0.columns.saturating_sub(1),
        })
    }
}

/// Returns the size of the body.
fn get_body_size() -> BodySize {
    match terminal::size() {
        Ok((columns, rows)) => TerminalSize::new(rows, columns),
        Err(e) => {
            warn!("unable to retrieve size of terminal: {}", e);
            TerminalSize::default()
        }
    }
    .into()
}

/// The [`Size`] of the terminal.
struct TerminalSize(Size);

impl TerminalSize {
    /// Creates a new [`TerminalSize`].
    const fn new(rows: UiUnit, columns: UiUnit) -> Self {
        Self(Size { rows, columns })
    }
}

impl Default for TerminalSize {
    /// Returns the default size for a terminal: 20x80.
    fn default() -> Self {
        Self::new(20, 80)
    }
}

/// The type the user interface uses.
type UiUnit = u16;

/// The part of the output that displays the content of the document.
#[derive(Default)]
struct Body {
    /// Prints the output.
    printer: Printer,
    /// Holds the current lines of the document.
    lines: Vec<String>,
    /// The number of rows currently covered by an alert.
    alert_rows: UiUnit,
    /// If the intake box is current active.
    is_intake_active: bool,
    /// The size of the body.
    size: Size,
    /// The index of the first line of the document to be displayed.
    top_line: usize,
    /// If the text is wrapped.
    is_wrapped: bool,
}

impl Body {
    /// Sets `text` and prints it.
    fn open(&mut self, text: &str) -> Result<(), ErrorKind> {
        self.lines = text.lines().map(ToString::to_string).collect();
        self.refresh(&Selection::default())
    }

    /// Returns the length at which a line will be wrapped.
    fn wrap_length(&self) -> UiUnit {
        if self.is_wrapped {
            self.size.columns
        } else {
            UiUnit::max_value()
        }
    }

    /// Modifies `self` according to `edit`.
    fn edit(&mut self, new_text: &str, selection: Selection) {
        let _ = self
            .lines
            .splice(selection, new_text.lines().map(ToString::to_string));
    }

    /// Prints all of `self` with `selection` marked.
    fn refresh(&mut self, selection: &Selection) -> Result<(), ErrorKind> {
        let start_line = selection.start_line;
        if start_line < self.top_line {
            self.top_line = start_line
        }

        let first_line = self.top_line;
        let last_line = selection.end_line;
        let mut rows =
            Rows::new(&self.lines, self.wrap_length()).skip_while(|row| row.line < first_line);
        let mut visible_rows = VecDeque::new();

        for _ in 0..self.size.rows.into() {
            if let Some(row) = rows.next() {
                visible_rows.push_back(row);
            }
        }

        for row in rows {
            if visible_rows.front().map(|r| r.line) != Some(self.top_line) {
                let _ = visible_rows.pop_front();
                visible_rows.push_back(row);
            } else if last_line < row.line {
                break;
            } else {
                self.top_line = self.top_line.saturating_add(1);
                let _ = visible_rows.pop_front();
                visible_rows.push_back(row);
            }
        }

        self.printer.print_rows(
            visible_rows.drain(..),
            Context::Document {
                selected_line: start_line,
            },
        )
    }

    /// Adds an alert box over the grid.
    fn add_alert(&mut self, message: &str, typ: MessageType) -> Result<(), ErrorKind> {
        for line in message.lines() {
            self.printer.print_row(
                self.alert_rows,
                Row {
                    text: line,
                    line: 0,
                },
                &Context::Message { typ },
            )?;
            self.alert_rows = self.alert_rows.saturating_add(1);
        }

        Ok(())
    }

    /// Adds an input box beginning with `prompt`
    fn add_intake(&mut self, mut prompt: String) -> Result<(), ErrorKind> {
        prompt.push_str(": ");
        self.printer.print_row(
            self.size.rows.saturating_sub(1),
            Row {
                text: &prompt,
                line: 0,
            },
            &Context::Intake,
        )?;
        self.is_intake_active = true;
        Ok(())
    }

    /// Removes all temporary boxes and re-displays the full grid.
    fn reset(&mut self, selection: &Selection) -> Result<(), ErrorKind> {
        if self.alert_rows != 0 {
            self.printer.print_rows(
                Rows::new(&self.lines, self.wrap_length()).take(self.alert_rows.into()),
                Context::Document {
                    selected_line: selection.end_line,
                },
            )?;
            self.alert_rows = 0;
        }

        if self.is_intake_active {
            let row = self.size.rows.saturating_sub(1);

            self.printer.print_row(
                row,
                Rows::new(&self.lines, self.wrap_length())
                    .nth(row.into())
                    .unwrap_or_default(),
                &Context::Document {
                    selected_line: selection.end_line,
                },
            )?;
            self.is_intake_active = false;
        }

        Ok(())
    }
}

/// Describes the context in which text is being printed.
#[derive(Clone, Copy)]
enum Context {
    /// A document.
    Document {
        /// The index of the line that is selected.
        selected_line: usize,
    },
    /// An intake text.
    Intake,
    /// A message to the user.
    Message {
        /// The type of the message.
        typ: MessageType,
    },
}

/// Prints text to the terminal.
// This serves to separate the [`Stdout`] from the rest of the [`Body`] so that it can be `mut`.
struct Printer {
    /// The output of the printer.
    out: Stdout,
}

impl Printer {
    /// Prints `row` at `index` of body with `context`.
    fn print_row<'a>(
        &mut self,
        index: UiUnit,
        row: Row<'a>,
        context: &Context,
    ) -> Result<(), ErrorKind> {
        // Add 1 to account for header.
        queue!(self.out, MoveTo(0, index.saturating_add(1)))?;

        let color = match context {
            Context::Document { selected_line } => {
                if row.line == *selected_line {
                    Some(Color::DarkGrey)
                } else {
                    None
                }
            }
            Context::Intake => None,
            Context::Message { typ } => Some(match typ {
                MessageType::Error => Color::Red,
                MessageType::Warning => Color::Yellow,
                MessageType::Info => Color::Blue,
                MessageType::Log => Color::DarkCyan,
            }),
        };

        if let Some(c) = color {
            queue!(self.out, SetBackgroundColor(c))?;
        }

        queue!(self.out, Print(row.text), Clear(ClearType::UntilNewLine))?;

        if color.is_some() {
            queue!(self.out, ResetColor)?;
        }

        self.out.flush().map_err(|e| e.into())
    }

    /// Prints `rows` with `context`.
    fn print_rows<'a>(
        &mut self,
        rows: impl Iterator<Item = Row<'a>>,
        context: Context,
    ) -> Result<(), ErrorKind> {
        for (index, row) in (0..).zip(rows) {
            self.print_row(index, row, &context)?;
        }

        execute!(self.out, Clear(ClearType::FromCursorDown))
    }
}

impl Default for Printer {
    fn default() -> Self {
        Self { out: io::stdout() }
    }
}

/// The text selected by the user.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) struct Selection {
    /// The index of the first line of the selection.
    start_line: usize,
    /// The index of the first line after the selection.
    end_line: usize,
}

impl Selection {
    /// Creates an empty selection.
    pub(crate) const fn empty() -> Self {
        Self {
            start_line: 0,
            end_line: 0,
        }
    }

    /// Returns the index of the start line.
    pub(crate) const fn start_line(&self) -> usize {
        self.start_line
    }

    /// Returns the index of the end line.
    pub(crate) const fn end_line(&self) -> usize {
        self.end_line
    }

    /// Initializes `self` to select the first line.
    pub(crate) fn init(&mut self) {
        self.start_line = 0;
        self.end_line = 1;
    }

    /// Returns the [`Range`] represented by `self`.
    pub(crate) fn range(&self) -> Result<Range, SelectionConversionError> {
        Ok(Range {
            start: Position {
                line: u64::try_from(self.start_line)?,
                character: 0,
            },
            end: Position {
                line: u64::try_from(self.end_line)?,
                character: 0,
            },
        })
    }

    /// Moves `self` down by `amount` lines up to `line_count`.
    pub(crate) fn move_down(&mut self, amount: usize, line_count: usize) {
        let end_line = cmp::min(self.end_line.saturating_add(amount), line_count);
        self.start_line = self
            .start_line
            .saturating_add(end_line.saturating_sub(self.end_line));
        self.end_line = end_line;
    }

    /// Moves `self` up by `amount` lines.
    pub(crate) fn move_up(&mut self, amount: usize) {
        let start_line = self.start_line.saturating_sub(amount);
        self.end_line = self
            .end_line
            .saturating_sub(self.start_line.saturating_sub(start_line));
        self.start_line = start_line;
    }
}

impl RangeBounds<usize> for Selection {
    fn start_bound(&self) -> Bound<&usize> {
        Bound::Included(&self.start_line)
    }

    fn end_bound(&self) -> Bound<&usize> {
        Bound::Excluded(&self.end_line)
    }
}

/// An iterator that yields [`Row`]s.
#[derive(Clone)]
struct Rows<'a> {
    /// The lines that will yield [`Row`]s.
    lines: &'a [String],
    /// The maximum length of every yielded [`Row`].
    max_len: usize,
    /// The index of the next [`Row`].
    row: usize,
    /// The index of `lines` that will be in the next [`Row`].
    line: usize,
    /// The index within `lines[line]` at which the next [`Row`] will start.
    index: usize,
}

impl<'a> Rows<'a> {
    /// Creates a new iterator of [`Row`]s.
    pub(crate) fn new(lines: &'a [String], max_len: UiUnit) -> Self {
        Self {
            lines,
            max_len: max_len.into(),
            row: 0,
            line: 0,
            index: 0,
        }
    }
}

impl<'a> Iterator for Rows<'a> {
    type Item = Row<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        if let Some(line_text) = self.lines.get(self.line) {
            let row_len = line_text.len().saturating_sub(self.index);
            let row = Row {
                line: self.line,
                text: if row_len > self.max_len {
                    let start = self.index;
                    self.index = self.index.saturating_add(self.max_len);

                    while !line_text.is_char_boundary(self.index) {
                        self.index = self.index.saturating_sub(1);
                    }

                    if self.index <= start {
                        error!(
                            "Failed to get row {} at index {} of line `{}`.",
                            self.row, self.index, line_text
                        );
                        ""
                    } else {
                        #[allow(unsafe_code)] // All preconditions of get_unchecked are satisfied.
                        unsafe {
                            line_text.get_unchecked(start..self.index)
                        }
                    }
                } else {
                    let start = self.index;
                    self.line = self.line.saturating_add(1);
                    self.index = 0;

                    #[allow(unsafe_code)] // All preconditions of get_unchecked are satisfied.
                    unsafe {
                        line_text.get_unchecked(start..)
                    }
                },
            };

            self.row = self.row.saturating_add(1);
            Some(row)
        } else {
            None
        }
    }
}

/// A row of the user interface.
#[derive(Clone, Copy, Debug, Default)]
struct Row<'a> {
    /// The index of the line of the row.
    line: usize,
    /// The text of the row.
    text: &'a str,
}

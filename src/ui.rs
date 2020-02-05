//! Implements the interface between the user and the application.
//!
//! The user is able to provide input via any of the following methods:
//! - Environment variables of the terminal running the command - $HOME, $CWD.
//! - An argument given with the command; this allows all processing of arguments to be performed within the main application loop.
//! - A terminal event (key press, mouse event, or size change).
//! - The config file (configs are read as input on startup - then any change to the config file is a new input).
//!
//! The application delivers the following output to the user:
//! - Failures are reported on stderr of the process running the paper command.
//! - Everything else is output on stdout of the process, and organized in the following visual manner:
//!     - The first row of the screen is the header, which displays information generated by starship.
//!     - All remaining space on the screen is primarily used for displaying the text of the currently viewed document.
//!     - If the application needs to alert the user, it may do so via a message box that will temporarily overlap the top rows of the document.
//!     - If the application requires input from the user, it may do so via an input box that will temporarily overlap the bottom rows of the document.
pub(crate) use crossterm::event::{KeyCode as Key, KeyModifiers as Modifiers};

use {
    crate::Arguments,
    core::{
        convert::TryFrom,
        fmt::{self, Debug},
        time::Duration,
    },
    crossterm::{
        cursor::{Hide, MoveTo, RestorePosition, SavePosition},
        event::{self, Event},
        execute, queue,
        style::{Color, Print, ResetColor, SetBackgroundColor},
        terminal::{self, Clear, ClearType, EnterAlternateScreen, LeaveAlternateScreen},
        ErrorKind,
    },
    log::{trace, warn, LevelFilter},
    lsp_types::{MessageType, Range, ShowMessageParams, ShowMessageRequestParams},
    notify::{DebouncedEvent, RecommendedWatcher, Watcher},
    serde::Deserialize,
    std::{
        collections::VecDeque,
        fs,
        io::{self, Stdout, Write},
        path::PathBuf,
        sync::mpsc::{self, Receiver, TryRecvError},
    },
    thiserror::Error,
};

/// Represents the return type of all functions that may fail.
type Outcome<T> = Result<T, Fault>;

/// An error from which the user interface was unable to recover.
#[derive(Debug, Error)]
pub enum Fault {
    /// An error while creating the config file watcher.
    #[error("while creating config file watcher: {0}")]
    Watcher(#[from] notify::Error),
    /// An error while retrieving the home directory of the user.
    #[error("unable to determine home directory of user")]
    HomeDir,
    /// An error while retrieving the size of the terminal.
    #[error("while determining terminal size: {0}")]
    TerminalSize(#[source] ErrorKind),
    /// An error while executing a [`crossterm`] command.
    ///
    /// [`crossterm`]: ../../crossterm/index.html
    #[error("while executing terminal command: {0}")]
    Command(#[source] ErrorKind),
    /// An error while reading terminal events.
    #[error("while reading terminal events: {0}")]
    Event(#[source] ErrorKind),
    /// Error while flushing terminal output.
    #[error("while flushing terminal output: {0}")]
    Flush(#[source] io::Error),
}

/// An error in the user interface that is recoverable.
///
/// Until a glitch is resolved, certain functionality may not be properly completed.
#[derive(Debug, Error)]
pub(crate) enum Glitch {
    /// Config file watcher disconnected.
    #[error("config file watcher disconnected")]
    WatcherConnection,
    /// Unable to read config file.
    #[error("unable to read config file: {0}")]
    ReadConfig(#[source] io::Error),
    /// Unable to convert config file to Config.
    #[error("config file invalid format: {0}")]
    ConfigFormat(#[from] toml::de::Error),
}

/// The user interface provided by a terminal.
pub(crate) struct Terminal {
    /// The output of the application.
    out: Stdout,
    /// The current configuration of the application.
    config: Config,
    /// Configs that have been input.
    ///
    /// Command arguments are viewed as config input so that all processing of arguments is performed within the main application loop.
    changed_settings: VecDeque<Setting>,
    /// A list of the glitches that have occurred in the user interface.
    glitches: Vec<Glitch>,
    /// The size of the terminal.
    size: Size,
    /// The index of the first line of the document that may be displayed.
    top_line: u64,
    /// Notifies `self` of any events to the config file.
    watcher: ConfigWatcher,
    /// The grid of the terminal.
    grid: Grid,
}

impl Terminal {
    /// Creates a new [`Terminal`].
    pub(crate) fn new(arguments: Arguments) -> Outcome<Self> {
        let config_file = dirs::home_dir()
            .ok_or(Fault::HomeDir)?
            .join(".config/paper.toml");
        let watcher = ConfigWatcher::new(&config_file)?;
        let mut term = Self {
            out: io::stdout(),
            changed_settings: VecDeque::default(),
            glitches: Vec::default(),
            size: Size::default(),
            top_line: 0,
            watcher,
            config: Config::default(),
            grid: Grid::default(),
        };

        term.changed_settings
            .push_back(Setting::Size(get_terminal_size()?));
        term.add_config_updates(config_file);

        if let Some(file) = arguments.file {
            term.changed_settings.push_back(Setting::File(file));
        }

        // Store all previous terminal output.
        execute!(term.out, EnterAlternateScreen, Hide).map_err(Fault::Command)?;
        Ok(term)
    }

    /// Applies `change` to the output.
    pub(crate) fn apply(&mut self, update: Update<'_>) -> Outcome<()> {
        let header = update.header;

        match update.change {
            Change::Text { rows, cursor } => {
                if cursor.start.line < self.top_line {
                    self.top_line = cursor.start.line;
                }

                let mut visible_rows: Vec<Row<'_>> = rows
                    .clone()
                    .filter(|row| row.line() >= self.top_line)
                    .collect();
                let mut end_line = cursor.end.line;

                if cursor.end.character == 0 {
                    end_line = end_line.saturating_sub(1);
                }

                while let Some(first_line_past_bottom) = visible_rows
                    .get(usize::from(self.grid.height))
                    .map(Row::line)
                {
                    if end_line < first_line_past_bottom {
                        break;
                    } else {
                        let line = visible_rows.remove(0).line();
                        self.top_line = self.top_line.saturating_add(1);

                        while visible_rows.get(0).map(Row::line) == Some(line) {
                            let _ = visible_rows.remove(0);
                        }
                    }
                }

                let top_line = self.top_line;

                for (index, row) in rows
                    .filter(|row| row.line() >= top_line)
                    .enumerate()
                    .take(usize::from(self.size.rows.saturating_sub(1)))
                {
                    //trace!("index {}, row {:?}", index, row);
                    self.grid
                        .replace_line(index, row.text(), row.line() == end_line)?;
                }

                queue!(self.out, Clear(ClearType::FromCursorDown)).map_err(Fault::Command)?;
            }
            Change::Message(alert) => {
                trace!("alert: {:?} {}", alert.typ, alert.message);
                self.grid.add_alert(&alert.message, alert.typ)?;
            }
            Change::Question(question) => {
                self.grid.add_alert(&question.message, question.typ)?;
            }
            Change::Reset => {
                self.grid.reset()?;
            }
            Change::Input(title) => {
                self.grid.add_input(title)?;
            }
            Change::Size(size) => {
                self.size = size;
                // Subtract 1 to account for header.
                self.grid.resize(self.size.rows.saturating_sub(1));
            }
            Change::InputChar(c) => {
                queue!(self.out, Print(c)).map_err(Fault::Command)?;
            }
        }

        queue!(
            self.out,
            SavePosition,
            MoveTo(0, 0),
            Print(header),
            RestorePosition,
        )
        .map_err(Fault::Command)?;

        self.out.flush().map_err(Fault::Flush)
    }

    /// Checks for updates to [`Config`] and adds any changes the changed settings list.
    fn add_config_updates(&mut self, config_file: PathBuf) {
        match self.config.update(config_file) {
            Ok(mut settings) => {
                self.changed_settings.append(&mut settings);
            }
            Err(glitch) => {
                self.glitches.push(glitch);
            }
        }
    }

    /// Returns the input from the user.
    ///
    /// Configuration modifications are returned prior to returning all other inputs.
    pub(crate) fn input(&mut self) -> Outcome<Option<Input>> {
        match self.watcher.notify.try_recv() {
            Ok(event) => {
                if let DebouncedEvent::Write(config_file) = event {
                    self.add_config_updates(config_file);
                }
            }
            Err(TryRecvError::Empty) => (),
            Err(TryRecvError::Disconnected) => {
                self.glitches.push(Glitch::WatcherConnection);
            }
        }

        // First check errors, then settings, then terminal input.
        Ok(if let Some(glitch) = self.glitches.pop() {
            Some(Input::Glitch(glitch))
        } else if let Some(setting) = self.changed_settings.pop_front() {
            trace!("retrieved setting: `{:?}`", setting);
            Some(Input::Setting(setting))
        } else if event::poll(Duration::from_secs(0)).map_err(Fault::Event)? {
            Some(event::read().map_err(Fault::Event)?.into())
        } else {
            None
        })
    }
}

impl Debug for Terminal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Terminal")
    }
}

impl Drop for Terminal {
    fn drop(&mut self) {
        if execute!(self.out, LeaveAlternateScreen).is_err() {
            warn!("Failed to leave alternate screen");
        }
    }
}

/// An [`Iterator`] over the rows of a string.
#[derive(Clone, Debug)]
pub(crate) struct Rows<'a> {
    /// The string being iterated over.
    s: &'a str,
    /// The maximum size of a row.
    max_len: usize,
    /// The index of the current line of the iterator.
    current_line: u64,
}

impl<'a> Rows<'a> {
    /// Creates a new [`Iterator`] over the rows of `s` with a `max_len`.
    pub(crate) fn new(s: &'a str, max_len: Option<usize>) -> Self {
        Rows {
            s,
            max_len: max_len.unwrap_or(usize::max_value()),
            current_line: 0,
        }
    }
}

impl<'a> Iterator for Rows<'a> {
    type Item = Row<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.s.is_empty() {
            None
        } else {
            let (line_len, extra_len) = if let Some(mut newline_len) = self.s.find('\n') {
                let mut extra = 1;

                if let Some(line_end) = newline_len.checked_sub(1) {
                    if self.s.get(line_end..=line_end) == Some("\r") {
                        newline_len = line_end;
                        extra = 2;
                    }
                }

                (newline_len, extra)
            } else {
                (self.s.len(), 0)
            };
            let (row_len, rm_len) = if line_len > self.max_len {
                (self.max_len, 0)
            } else {
                (line_len, extra_len)
            };
            let (row_text, remainder) = self.s.split_at(row_len);
            let (_, new_s) = remainder.split_at(rm_len);
            let row = Row {
                text: row_text,
                line: self.current_line,
            };

            if rm_len != 0 {
                self.current_line = self.current_line.saturating_add(1);
            }

            self.s = new_s;
            Some(row)
        }
    }
}

/// Represents a row in the user interface.
#[derive(Clone, Debug)]
pub(crate) struct Row<'a> {
    /// The line of the row.
    line: u64,
    /// The text of the row.
    text: &'a str,
}

impl Row<'_> {
    /// Returns the line of `self`.
    pub(crate) const fn line(&self) -> u64 {
        self.line
    }

    /// Returns the text of `self`.
    pub(crate) const fn text(&self) -> &str {
        self.text
    }
}

/// Triggers a callback if the config file is updated.
struct ConfigWatcher {
    /// Watches for events on the config file.
    #[allow(dead_code)] // watcher must must be owned to avoid being dropped.
    watcher: RecommendedWatcher,
    /// Receives events generated by `watcher`.
    notify: Receiver<DebouncedEvent>,
}

impl ConfigWatcher {
    /// Creates a new [`ConfigWatcher`].
    fn new(config_file: &PathBuf) -> Result<Self, notify::Error> {
        let (tx, notify) = mpsc::channel();
        let mut watcher = notify::watcher(tx, Duration::from_secs(0))?;

        watcher.watch(config_file, notify::RecursiveMode::NonRecursive)?;
        Ok(Self { watcher, notify })
    }
}

/// Signifies the primary output of the terminal.
struct Grid {
    /// The output of the application.
    out: Stdout,
    /// The lines that represent the primary output of the grid.
    lines: Vec<String>,
    /// The number of lines currrently covered by an alert.
    alert_line_count: u16,
    /// If the input box is current shown.
    is_showing_input: bool,
    /// The number of rows in the grid.
    height: u16,
}

impl Grid {
    /// Resizes grid to have `height` number of rows.
    fn resize(&mut self, height: u16) {
        for _ in self.height..height {
            self.lines.push(String::default());
        }

        self.height = height;
    }

    /// Replaces line in grid at `index` with `new_line`.
    fn replace_line(&mut self, index: usize, new_line: &str, is_cursor_line: bool) -> Outcome<()> {
        if let Some(line) = self.lines.get_mut(index) {
            line.replace_range(.., new_line);
        }

        self.print(
            u16::try_from(index).unwrap_or(u16::max_value()),
            new_line,
            if is_cursor_line {
                Some(Color::DarkGrey)
            } else {
                None
            },
        )?;
        Ok(())
    }

    /// Adds an alert box over the grid.
    fn add_alert(&mut self, message: &str, context: MessageType) -> Outcome<()> {
        trace!("lines {:?}", message.lines().next());
        for line in message.lines() {
            self.print(
                self.alert_line_count,
                line,
                Some(match context {
                    MessageType::Error => Color::Red,
                    MessageType::Warning => Color::Yellow,
                    MessageType::Info => Color::Blue,
                    MessageType::Log => Color::DarkCyan,
                }),
            )?;
            self.alert_line_count = self.alert_line_count.saturating_add(1);
        }

        Ok(())
    }

    /// Adds an input box beginning with `prompt`
    fn add_input(&mut self, mut prompt: String) -> Outcome<()> {
        prompt.push_str(": ");
        self.print(self.height.saturating_sub(1), &prompt, None)?;
        self.is_showing_input = true;
        Ok(())
    }

    /// Prints `s` at `row` of the grid.
    fn print(&mut self, row: u16, s: &str, background_color: Option<Color>) -> Outcome<()> {
        // Add 1 to account for header.
        queue!(self.out, MoveTo(0, row.saturating_add(1))).map_err(Fault::Command)?;

        if let Some(color) = background_color {
            queue!(self.out, SetBackgroundColor(color)).map_err(Fault::Command)?;
        }

        queue!(self.out, Print(s), Clear(ClearType::UntilNewLine)).map_err(Fault::Command)?;

        if background_color.is_some() {
            queue!(self.out, ResetColor).map_err(Fault::Command)?;
        }

        Ok(())
    }

    /// Removes all temporary boxes and re-displays the full grid.
    fn reset(&mut self) -> Outcome<()> {
        if self.alert_line_count != 0 {
            for row in 0..self.alert_line_count {
                self.print(
                    row,
                    &self
                        .lines
                        .get(usize::from(row))
                        .cloned()
                        .unwrap_or_default(),
                    None,
                )?;
            }

            self.alert_line_count = 0;
        }

        if self.is_showing_input {
            let row = self.height.saturating_sub(1);

            self.print(
                row,
                &self
                    .lines
                    .get(usize::from(row))
                    .cloned()
                    .unwrap_or_default(),
                None,
            )?;
            self.is_showing_input = false;
        }

        Ok(())
    }
}

impl Default for Grid {
    fn default() -> Self {
        Self {
            out: io::stdout(),
            lines: Vec::default(),
            alert_line_count: 0,
            is_showing_input: false,
            height: 0,
        }
    }
}

/// Signifies any configurable parameter of the application.
#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct Config {
    /// If the application wraps long lines.
    wrap: Wrap,
    /// The [`LevelFilter`] of the starship library.
    starship_log: StarshipLog,
}

impl Config {
    /// Reads the config file into a `Config`.
    fn read(config_file: PathBuf) -> Result<Self, Glitch> {
        // TODO: Replace Faults.
        fs::read_to_string(config_file)
            .map_err(Glitch::ReadConfig)
            .and_then(|config_string| toml::from_str(&config_string).map_err(Glitch::ConfigFormat))
    }

    /// Updates `self` to match paper's config file, returning any changed [`Setting`]s.
    fn update(&mut self, config_file: PathBuf) -> Result<VecDeque<Setting>, Glitch> {
        let mut settings = VecDeque::new();
        let config = Self::read(config_file)?;

        if self.wrap != config.wrap {
            self.wrap = config.wrap;
            settings.push_back(Setting::Wrap(self.wrap.0));
        }

        if self.starship_log != config.starship_log {
            self.starship_log = config.starship_log;
            settings.push_back(Setting::StarshipLog(self.starship_log.0));
        }

        Ok(settings)
    }
}

macro_rules! def_config {
    ($name:ident: $ty:ty = $default:expr) => {
        #[derive(Debug, Deserialize, PartialEq)]
        struct $name($ty);

        impl Default for $name {
            fn default() -> Self {
                Self($default)
            }
        }
    };
}

def_config!(Wrap: bool = false);
def_config!(StarshipLog: LevelFilter = LevelFilter::Off);

/// An update to the user interface.
pub(crate) struct Update<'a> {
    /// The update header of the ui.
    header: String,
    /// The change of the update.
    change: Change<'a>,
}

impl<'a> Update<'a> {
    /// Creates a new [`Update`].
    pub(crate) const fn new(header: String, change: Change<'a>) -> Self {
        Self { header, change }
    }
}

/// Signifies a potential modification to the output of the user interface.
///
/// It is not always true that a `Change` will require a modification of the user interface output. For example, if a range of the document that is not currently displayed is changed.
#[derive(Clone, Debug)]
pub(crate) enum Change<'a> {
    /// Text of the current document or how it was displayed was modified.
    Text {
        /// The rows of the current document.
        rows: Rows<'a>,
        /// The cursor.
        cursor: Range,
    },
    /// Message will be displayed to the user.
    Message(ShowMessageParams),
    /// Message will ask question to user and get a response.
    Question(ShowMessageRequestParams),
    /// Message will be cleared.
    Reset,
    /// Open an input box with the given prompt.
    Input(String),
    /// Change the size of the terminal.
    Size(Size),
    /// Add a char to the input box.
    InputChar(char),
}

/// Signifies a configuration.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum Setting {
    /// The file path of the document.
    File(String),
    /// If the document shall wrap long text.
    Wrap(bool),
    /// The size of the terminal.
    Size(Size),
    /// The level at which starship records shall be logged.
    StarshipLog(LevelFilter),
}

/// Signifies the size of a terminal.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct Size {
    /// The number of rows.
    pub(crate) rows: u16,
    /// The number of columns.
    pub(crate) columns: u16,
}

/// Signifies input provided by the user.
#[derive(Debug)]
pub(crate) enum Input {
    /// Signifies a new terminal size.
    #[allow(dead_code)] // False positive.
    Resize {
        /// The new number of rows.
        rows: u16,
        /// The new number of columns.
        columns: u16,
    },
    /// Signifies a mouse action.
    Mouse,
    /// Signifies a key being pressed.
    #[allow(dead_code)] // False positive.
    Key {
        /// The `key` that was pressed.
        key: Key,
        /// Modifier keys pressed at the same time as `key`.
        modifiers: Modifiers,
    },
    /// Signifies a changed [`Setting`].
    Setting(Setting),
    /// Signifies an error that is recoverable.
    Glitch(Glitch),
}

impl From<Event> for Input {
    fn from(value: Event) -> Self {
        match value {
            Event::Resize(columns, rows) => Self::Resize { rows, columns },
            Event::Mouse(..) => Self::Mouse,
            Event::Key(key) => Self::Key {
                key: key.code,
                modifiers: key.modifiers,
            },
        }
    }
}

/// Returns the size of the terminal.
fn get_terminal_size() -> Outcome<Size> {
    let (columns, rows) = terminal::size().map_err(Fault::TerminalSize)?;

    Ok(Size { rows, columns })
}

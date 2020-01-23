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
    crate::{app::Movement, Arguments},
    clap::ArgMatches,
    core::{
        convert::{TryFrom, TryInto},
        fmt::{self, Debug},
        time::Duration,
    },
    crossterm::{
        cursor::{MoveTo, RestorePosition, SavePosition},
        event::{self, Event},
        execute, queue,
        style::{Color, Print, ResetColor, SetBackgroundColor},
        terminal::{self, Clear, ClearType, EnterAlternateScreen, LeaveAlternateScreen},
        ErrorKind,
    },
    log::{trace, warn, LevelFilter},
    lsp_types::{MessageType, ShowMessageParams, ShowMessageRequestParams, TextEdit},
    notify::{DebouncedEvent, RecommendedWatcher, Watcher},
    serde::Deserialize,
    starship::{context::Context, print},
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
    first_line: u64,
    /// Notifies `self` of any events to the config file.
    watcher: ConfigWatcher,
    /// The working directory of the application.
    working_dir: PathBuf,
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
            first_line: 0,
            watcher,
            config: Config::default(),
            working_dir: arguments.working_dir.clone(),
            grid: Grid::default(),
        };

        term.changed_settings
            .push_back(Setting::Size(get_terminal_size()?));
        term.add_config_updates(config_file);

        if let Some(file) = arguments.file {
            term.changed_settings.push_back(Setting::File(file));
        }

        // Store all previous terminal output.
        execute!(term.out, EnterAlternateScreen).map_err(Fault::Command)?;
        Ok(term)
    }

    /// Applies `change` to the output.
    pub(crate) fn apply(&mut self, change: Change) -> Outcome<()> {
        match change {
            Change::Text { edits, is_wrapped, movement } => {
                if let Some(m) = movement {
                    match m {
                        Movement::Top => {
                            self.first_line = 0;
                        }
                        Movement::Down => {
                            self.first_line += u64::from(self.size.rows / 4);
                        }
                    }
                }

                for edit in edits {
                    let mut lines = edit
                        .new_text
                        .lines()
                        .skip(
                            self.first_line
                                .saturating_sub(edit.range.start.line)
                                .try_into()
                                .unwrap_or(usize::max_value()),
                        )
                        .take(
                            edit.range
                                .end
                                .line
                                .saturating_sub(edit.range.start.line)
                                .try_into()
                                .unwrap_or(usize::max_value()),
                        )
                        .collect::<Vec<&str>>()
                        .into_iter();
                    let mut row = edit
                        .range
                        .start
                        .line
                        .saturating_sub(self.first_line)
                        .try_into()
                        .unwrap_or(u16::max_value());

                    while row < self.size.rows {
                        if let Some(mut line) = lines.next() {
                            let mut last_row = row;

                            if is_wrapped {
                                last_row = last_row.saturating_add(
                                    u16::try_from(
                                        line.len()
                                            .saturating_sub(1)
                                            .wrapping_div(usize::from(self.size.columns)),
                                    )
                                    .unwrap_or(u16::max_value()),
                                );
                            }

                            for r in row..=last_row {
                                let printed_line = if line.len() > self.size.columns.into() {
                                    let split = line.split_at(self.size.columns.into());
                                    line = split.1;
                                    split.0
                                } else {
                                    line
                                };

                                self.grid.replace_line(r, printed_line)?;
                            }

                            row = last_row.saturating_add(1);
                        } else {
                            break;
                        }
                    }
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

        // For now, must deal with fact that StarshipConfig included in Context is very difficult to edit (must edit the TOML Value). Thus for now, the starship.toml config file must be configured correctly.
        queue!(
            self.out,
            SavePosition,
            MoveTo(0, 0),
            Print(
                print::get_prompt(Context::new_with_dir(
                    ArgMatches::default(),
                    &self.working_dir
                ))
                .replace("[J", "")
            ),
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
    fn replace_line(&mut self, index: u16, new_line: &str) -> Outcome<()> {
        if let Some(line) = self.lines.get_mut(usize::from(index)) {
            line.replace_range(.., new_line);
        }

        self.print(index, new_line, None)?;
        Ok(())
    }

    /// Adds an alert box over the grid.
    fn add_alert(&mut self, message: &str, context: MessageType) -> Outcome<()> {
        trace!("lines {:?}", message.lines().next());
        for line in message.lines() {
            self.print(self.alert_line_count, line, Some(context))?;
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
    fn print(&mut self, row: u16, s: &str, context: Option<MessageType>) -> Outcome<()> {
        // Add 1 to account for header.
        queue!(self.out, MoveTo(0, row.saturating_add(1))).map_err(Fault::Command)?;

        if let Some(t) = context {
            queue!(
                self.out,
                SetBackgroundColor(match t {
                    MessageType::Error => Color::Red,
                    MessageType::Warning => Color::Yellow,
                    MessageType::Info => Color::Blue,
                    MessageType::Log => Color::DarkCyan,
                })
            )
            .map_err(Fault::Command)?;
        }

        queue!(self.out, Print(s), Clear(ClearType::UntilNewLine)).map_err(Fault::Command)?;

        if context.is_some() {
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

/// Signifies a potential modification to the output of the user interface.
///
/// It is not always true that a `Change` will require a modification of the user interface output. For example, if a range of the document that is not currently displayed is changed.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum Change {
    /// Text of the current document or how it was displayed was modified.
    Text {
        /// Text that was modified.
        edits: Vec<TextEdit>,
        /// Long lines are wrapped.
        is_wrapped: bool,
        /// The change to first_line.
        movement: Option<Movement>
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
    #[allow(dead_code)] // This lint has an error.
    Resize {
        /// The new number of rows.
        rows: u16,
        /// The new number of columns.
        columns: u16,
    },
    /// Signifies a mouse action.
    Mouse,
    /// Signifies a key being pressed.
    #[allow(dead_code)] // This lint has an error.
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

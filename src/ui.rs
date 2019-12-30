//! Implements the interface between the user and the application.
use {
    clap::ArgMatches,
    core::{convert::TryFrom, fmt::Debug, time::Duration},
    crossterm::{
        terminal::{EnterAlternateScreen, LeaveAlternateScreen},
        execute,
        cursor::MoveTo,
        event::{self, Event},
        queue,
        style::Print,
        ErrorKind,
    },
    log::{trace, warn},
    lsp_types::{ShowMessageParams, TextEdit},
    std::io::{self, Stdout, Write},
};

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
    #[must_use]
    fn from(value: ArgMatches<'_>) -> Self {
        Self {
            file: value.value_of("file").map(str::to_string),
        }
    }
}

/// The user interface provided by a terminal.
///
/// All output is displayed in a grid of cells. Each cell contains one character.
#[derive(Debug)]
pub(crate) struct Terminal {
    /// The output of the application.
    out: Stdout,
    /// If the `Terminal` has been initialized.
    is_init: bool,
    /// Inputs from command arguments.
    arg_inputs: Vec<Config>,
}

impl Terminal {
    /// Sets up the user interface for use.
    pub(crate) fn init(&mut self, settings: Settings) -> crossterm::Result<()> {
        if let Some(file) = settings.file {
            self.arg_inputs.push(Config::File(file))
        }

        execute!(self.out, EnterAlternateScreen)?;
        self.is_init = true;
        Ok(())
    }

    /// Applies `change` to the output.
    pub(crate) fn apply(&mut self, change: Change) -> crossterm::Result<()> {
        match change {
            Change::Text(edits) => {
                for edit in edits {
                    queue!(
                        self.out,
                        MoveTo(
                            Self::u16_from(edit.range.start.character),
                            Self::u16_from(edit.range.start.line)
                        ),
                        Print(edit.new_text)
                    )?;
                }
            }
            Change::Alert(alert) => {
                trace!("alert: {:?} {}", alert.typ, alert.message);
                queue!(self.out, MoveTo(0, 0), Print(alert.message))?;
                trace!("added alert");
            }
        }

        self.out.flush().map_err(ErrorKind::IoError)
    }

    /// Returns the best u16 representation of `value`.
    fn u16_from(value: u64) -> u16 {
        u16::try_from(value).unwrap_or(u16::max_value())
    }

    /// Returns the input from the user.
    ///
    /// Returns [`None`] if no input is provided.
    pub(crate) fn input(&mut self) -> crossterm::Result<Option<Input>> {
        // First check arg inputs, then check for key input.
        match self.arg_inputs.pop() {
            Some(input) => Ok(Some(Input::Config(input))),
            None => Ok(if event::poll(Duration::from_secs(0))? {
                Some(Input::User(event::read()?))
            } else {
                None
            }),
        }
    }
}

impl Default for Terminal {
    fn default() -> Self {
        Self {
            out: io::stdout(),
            is_init: false,
            arg_inputs: Vec::default(),
        }
    }
}

impl Drop for Terminal {
    fn drop(&mut self) {
        if self.is_init && execute!(self.out, LeaveAlternateScreen).is_err() {
            warn!("Unable to leave alternate screen");
        }
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
    /// User input.
    User(Event),
    /// Configuration.
    Config(Config),
}

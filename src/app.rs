//! Implements the modality of the application.
use crate::{ui::Index, Failure};
use core::convert::TryFrom;
use log::trace;
use lsp_types::{Position, Range, TextEdit};
use parse_display::Display as ParseDisplay;
use std::{fmt::Debug, fs, io};
use url::{ParseError, Url};

/// Signifies the mode of the application.
#[derive(Copy, Clone, Eq, ParseDisplay, PartialEq, Hash, Debug)]
#[display(style = "CamelCase")]
// Mode is pub due to being a member of Failure::UnknownMode.
pub enum Mode {
    /// Displays the current file.
    ///
    /// Allows moving the screen or switching to other modes.
    View,
    /// Displays the current command.
    Command,
    /// Displays the current view along with the current edits.
    Edit,
}

impl Default for Mode {
    #[inline]
    fn default() -> Self {
        Self::View
    }
}

/// Signifies an action of the application after [`Sheet`] has performed an [`Operation`].
pub(crate) enum Outcome {
    /// Switches the [`Mode`] of the application.
    SwitchMode(Mode),
    /// Edits the text of the current document.
    EditText(Vec<TextEdit>),
}

/// Signifies errors associated with [`Document`].
enum DocumentError {
    /// An error attempting to parse a string to a [`Url`].
    Parse(ParseError),
    /// An error with the given file path.
    InvalidFilePath(),
    /// An IO Error.
    Io(io::Error),
}

impl From<io::Error> for DocumentError {
    fn from(value: io::Error) -> Self {
        Self::Io(value)
    }
}

impl From<ParseError> for DocumentError {
    fn from(value: ParseError) -> Self {
        Self::Parse(value)
    }
}

/// Signifies a text document.
#[derive(Clone, Debug)]
struct Document {
    /// The [`Url`] of the `Document`.
    url: Url,
    /// The text of the `Document`.
    text: String,
}

impl TryFrom<String> for Document {
    type Error = DocumentError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        trace!("Finding base URL");
        let base = Url::from_directory_path(std::env::current_dir()?)
            .map_err(|_| DocumentError::InvalidFilePath())?;
        trace!("Parsing `{}` to URL with base `{}`", value, base);
        let url = base.join(&value)?;

        trace!("Reading text at `{}`", url);
        Ok(Self {
            text: fs::read_to_string(
                url.clone()
                    .to_file_path()
                    .map_err(|_| DocumentError::InvalidFilePath())?,
            )?,
            url,
        })
    }
}

/// Signfifies display of the current file.
#[derive(Clone, Debug, Default)]
pub(crate) struct Sheet {
    /// The document being displayed.
    doc: Option<Document>,
    /// The number of lines in the document.
    line_count: usize,
}

impl Sheet {
    /// Performs `operation`.
    pub(crate) fn operate(&mut self, operation: Operation) -> Result<Option<Outcome>, Failure> {
        match operation {
            Operation::SwitchMode(mode) => {
                match mode {
                    Mode::View | Mode::Command | Mode::Edit => {}
                }

                Ok(Some(Outcome::SwitchMode(mode)))
            }
            Operation::Quit => Err(Failure::Quit),
            Operation::ViewFile(file) => {
                if let Ok(doc) = Document::try_from(file) {
                    let text = doc.text.clone();

                    self.doc = Some(doc);
                    Ok(Some(Outcome::EditText(vec![TextEdit::new(
                        Range::new(
                            Position::new(0, 0),
                            Position::new(u64::max_value(), u64::max_value()),
                        ),
                        text,
                    )])))
                } else {
                    Ok(None)
                }
            } //Operation::Save => {
              //    fs::write(path, file)
              //}
        }
    }
}

/// An [`Iterator`] of [`Index`]es.
struct IndexIterator {
    /// The current [`Index`].
    current: Index,
    /// The first [`Index`] that is not valid.
    end: Index,
}

impl Iterator for IndexIterator {
    type Item = Index;

    fn next(&mut self) -> Option<Self::Item> {
        if self.current == self.end {
            return None;
        }

        let next_index = self.current;
        self.current = self.current.add_one();
        Some(next_index)
    }
}

/// Signifies actions that can be performed by the application.
#[derive(Debug)]
pub(crate) enum Operation {
    /// Switches the mode of the application.
    SwitchMode(Mode),
    /// Quits the application.
    Quit,
    /// Displays the given file.
    ViewFile(String),
}

//! Implements the modality of the application.
mod lsp;

use {
    url::{ParseError, Url},
    crate::{ui::Config, Failure},
    core::convert::TryFrom,
    displaydoc::Display as DisplayDoc,
    lsp::LspServer,
    parse_display::Display as ParseDisplay,
    lsp_types::{MessageType, Position, Range, ShowMessageParams, TextEdit},
    std::{
        collections::{hash_map::Entry, HashMap},
        env,
        fmt::Debug,
        fs,
        io::{self, ErrorKind},
    },
};

/// A [`Range`] specifying the entire document.
const ENTIRE_DOCUMENT: Range = Range {
    start: Position {
        line: 0,
        character: 0,
    },
    end: Position {
        line: u64::max_value(),
        character: u64::max_value(),
    },
};

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
    /// Displays a message to the user.
    Alert(ShowMessageParams),
}

/// Signifies errors associated with [`Document`].
#[derive(DisplayDoc)]
enum DocumentError {
    /// unable to parse file `{0}`
    Parse(ParseError),
    /// invalid path
    InvalidFilePath(),
    /// cannot find file `{0}`
    NonExistantFile(String),
    /// io: {0}
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

impl From<DocumentError> for ShowMessageParams {
    #[must_use]
    fn from(value: DocumentError) -> Self {
        Self {
            typ: MessageType::Log,
            message: value.to_string(),
        }
    }
}

/// Signifies a text document.
#[derive(Clone, Debug)]
struct Document {
    /// The [`Url`] of the `Document`.
    url: Url,
    /// The text of the `Document`.
    text: String,
    /// The extension.
    extension: Option<String>,
}

impl Document {
    /// The extension of `self`.
    const fn extension(&self) -> &Option<String> {
        &self.extension
    }
}

impl TryFrom<String> for Document {
    type Error = DocumentError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        let base = Url::from_directory_path(env::current_dir()?)
            .map_err(|_| DocumentError::InvalidFilePath())?;
        let url = base.join(&value)?;

        let file_path = url
            .clone()
            .to_file_path()
            .map_err(|_| DocumentError::InvalidFilePath())?;
        Ok(Self {
            extension: file_path
                .extension()
                .map(|ext| ext.to_string_lossy().into_owned()),
            text: fs::read_to_string(file_path).map_err(|error| match error.kind() {
                ErrorKind::NotFound => DocumentError::NonExistantFile(value),
                ErrorKind::PermissionDenied
                | ErrorKind::ConnectionRefused
                | ErrorKind::ConnectionReset
                | ErrorKind::ConnectionAborted
                | ErrorKind::NotConnected
                | ErrorKind::AddrInUse
                | ErrorKind::AddrNotAvailable
                | ErrorKind::BrokenPipe
                | ErrorKind::AlreadyExists
                | ErrorKind::WouldBlock
                | ErrorKind::InvalidInput
                | ErrorKind::InvalidData
                | ErrorKind::TimedOut
                | ErrorKind::WriteZero
                | ErrorKind::Interrupted
                | ErrorKind::Other
                | ErrorKind::UnexpectedEof
                | _ => DocumentError::Io(error),
            })?,
            url,
        })
    }
}

/// Signfifies display of the current file.
#[derive(Debug, Default)]
pub(crate) struct Sheet {
    /// The document being displayed.
    doc: Option<Document>,
    /// The number of lines in the document.
    line_count: usize,
    /// The [`LspServer`]s managed by this document.
    lsp_servers: HashMap<String, LspServer>,
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
            Operation::UpdateConfig(Config::File(file)) => match Document::try_from(file) {
                Ok(doc) => {
                    if let Some(ext) = doc.extension() {
                        if let Entry::Vacant(entry) = self.lsp_servers.entry(ext.to_string()) {
                            entry.insert(LspServer::new("rls")?).initialize()?;
                        }
                    }

                    let text = doc.text.clone();

                    self.doc = Some(doc);
                    Ok(Some(Outcome::EditText(vec![TextEdit::new(
                        ENTIRE_DOCUMENT,
                        text,
                    )])))
                }
                Err(error) => Ok(Some(Outcome::Alert(ShowMessageParams::from(error)))),
            }, //Operation::Save => {
               //    fs::write(path, file)
               //}
        }
    }
}

/// Signifies actions that can be performed by the application.
#[derive(Debug)]
pub(crate) enum Operation {
    /// Switches the mode of the application.
    SwitchMode(Mode),
    /// Quits the application.
    Quit,
    /// Updates a configuration.
    UpdateConfig(Config),
}

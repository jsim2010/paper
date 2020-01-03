//! Implements the modality of the application.
mod lsp;

pub(crate) use lsp::Error as LspError;

use {
    crate::{ui::Config, Change, Failure},
    core::convert::TryFrom,
    displaydoc::Display as DisplayDoc,
    lsp::LspServer,
    lsp_types::{
        MessageType, Position, Range, ShowMessageParams, ShowMessageRequestParams, TextEdit,
    },
    parse_display::Display as ParseDisplay,
    std::{
        collections::{hash_map::Entry, HashMap},
        env, fmt, fs,
        io::{self, ErrorKind},
    },
    url::{ParseError, Url},
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
    View,
    /// Confirms the user's action
    Confirm,
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
    pub(crate) fn operate(&mut self, operation: Operation) -> Result<Option<Change>, Failure> {
        match operation {
            Operation::Reset => Ok(Some(Change::Reset)),
            Operation::Confirm(action) => Ok(Some(Change::Question(
                ShowMessageRequestParams::from(action),
            ))),
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
                    Ok(Some(Change::Text(vec![TextEdit::new(
                        ENTIRE_DOCUMENT,
                        text,
                    )])))
                }
                Err(error) => Ok(Some(Change::Message(ShowMessageParams::from(error)))),
            }, //Operation::Save => {
               //    fs::write(path, file)
               //}
        }
    }
}

/// Signifies actions that can be performed by the application.
#[derive(Debug, PartialEq)]
pub(crate) enum Operation {
    /// Resets the application.
    Reset,
    /// Confirms that the action is desired.
    Confirm(ConfirmAction),
    /// Quits the application.
    Quit,
    /// Updates a configuration.
    UpdateConfig(Config),
}

/// Signifies actions that require a confirmation prior to their execution.
#[derive(Debug, PartialEq)]
pub(crate) enum ConfirmAction {
    /// Quit the application.
    Quit,
}

impl fmt::Display for ConfirmAction {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "You have input that you want to quit the application.\nPlease confirm this action by pressing `y`. To cancel this action, press any other key.")
    }
}

impl From<ConfirmAction> for ShowMessageRequestParams {
    #[must_use]
    fn from(value: ConfirmAction) -> Self {
        Self {
            typ: MessageType::Info,
            message: value.to_string(),
            actions: None,
        }
    }
}

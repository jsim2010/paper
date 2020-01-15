//! Implements the modality of the application.
mod logging;
mod lsp;

use {
    crate::{
        ui::{Change, Setting},
    },
    clap::ArgMatches,
    core::convert::TryFrom,
    logging::LogConfig,
    log::trace,
    lsp::LspServer,
    lsp_types::{
        MessageType, Position, Range, ShowMessageParams, ShowMessageRequestParams,
        TextDocumentItem, TextEdit,
    },
    parse_display::Display as ParseDisplay,
    std::{
        collections::{hash_map::Entry, HashMap},
        env,
        ffi::OsStr,
        fmt,
        fs,
        io::{self, ErrorKind},
    },
    thiserror::Error,
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

/// Signifies a command that a user can give to the application.
#[derive(Debug, ParseDisplay, PartialEq)]
pub(crate) enum Command {
    /// Opens a given file.
    #[display("Open <file>")]
    Open,
}

/// Signifies errors associated with [`Document`].
#[derive(Debug, Error)]
enum DocumentError {
    /// Error while parsing url.
    #[error("unable to parse url: {0}")]
    Parse(#[from] ParseError),
    /// Path is invalid.
    #[error("path `{0}` is invalid")]
    InvalidPath(String),
    /// File does not exist.
    #[error("file `{0}` does not exist")]
    NonExistantFile(String),
    /// Invalid current working directory.
    #[error("unable to determine current working directory")]
    InvalidCwd,
    /// Io error.
    #[error("io: {0}")]
    Io(#[from] io::Error),
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

/// Creates a [`TextDocumentItem`] from `path`.
fn document_from_string(path: String) -> Result<TextDocumentItem, DocumentError> {
    let cwd = env::current_dir().map_err(|_| DocumentError::InvalidCwd)?;
    let base = Url::from_directory_path(cwd.clone())
        .map_err(|_| DocumentError::InvalidPath(cwd.to_string_lossy().to_string()))?;
    let uri = base.join(&path).map_err(DocumentError::from)?;
    let file_path = uri
        .clone()
        .to_file_path()
        .map_err(|_| DocumentError::InvalidPath(uri.to_string()))?;

    let language_id = match file_path.extension().and_then(OsStr::to_str) {
        Some("rs") => "rust",
        Some(x) => x,
        None => "",
    }
    .to_string();
    let text = fs::read_to_string(file_path).map_err(|error| match error.kind() {
        ErrorKind::NotFound => DocumentError::NonExistantFile(path),
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
        | _ => DocumentError::from(error),
    })?;

    Ok(TextDocumentItem::new(uri, language_id, 0, text))
}

/// Signifies settings of the application.
///
/// Using a custom struct rather than [`ArgMatches`] allows for external code to easily configure use of the application as a library.
#[derive(Clone, Debug)]
pub struct Arguments {
    /// The file to be viewed.
    pub file: Option<String>,
    /// The handle to configure the logger.
    pub log_config: LogConfig,
}

impl TryFrom<ArgMatches<'_>> for Arguments {
    type Error = Fault;

    fn try_from(value: ArgMatches<'_>) -> Result<Self, Self::Error> {
        Ok(Self {
            file: value.value_of("file").map(str::to_string),
            log_config: LogConfig::new()?,
        })
    }
}

/// Signfifies display of the current file.
#[derive(Debug)]
pub(crate) struct Sheet {
    /// The document being displayed.
    doc: Option<TextDocumentItem>,
    /// The document wraps long lines.
    is_wrapped: bool,
    /// The [`LspServer`]s managed by this document.
    lsp_servers: HashMap<String, LspServer>,
    /// Provides handle to be able to modify logger settings.
    log_config: LogConfig,
}

impl Sheet {
    pub(crate) fn new(log_config: LogConfig) -> Self {
        Self {
            doc: None,
            is_wrapped: false,
            lsp_servers: HashMap::default(),
            log_config,
        }
    }

    /// Performs `operation`.
    pub(crate) fn operate(&mut self, operation: Operation) -> Result<Option<Change>, Fault> {
        match operation {
            Operation::Reset => Ok(Some(Change::Reset)),
            Operation::Confirm(action) => Ok(Some(Change::Question(
                ShowMessageRequestParams::from(action),
            ))),
            Operation::Quit => {
                unreachable!("attempted to execute `Quit` operation");
            }
            Operation::UpdateConfig(Setting::File(file)) => match document_from_string(file) {
                Ok(doc) => {
                    if let Entry::Vacant(entry) = self.lsp_servers.entry(doc.language_id.clone()) {
                        if doc.language_id == "rust" {
                            entry.insert(LspServer::new("rls")?).initialize()?;
                        }
                    }

                    self.doc = Some(doc.clone());
                    Ok(Some(Change::Text {
                        edits: vec![TextEdit::new(ENTIRE_DOCUMENT, doc.text)],
                        is_wrapped: self.is_wrapped,
                    }))
                }
                Err(error) => Ok(Some(Change::Message(ShowMessageParams::from(error)))),
            },
            Operation::UpdateConfig(Setting::Wrap(is_wrapped)) => {
                if self.is_wrapped == is_wrapped {
                    Ok(None)
                } else {
                    self.is_wrapped = is_wrapped;
                    Ok(self.doc.as_ref().map(|doc| Change::Text {
                        edits: vec![TextEdit::new(ENTIRE_DOCUMENT, doc.text.clone())],
                        is_wrapped,
                    }))
                }
            }
            Operation::UpdateConfig(Setting::Size(size)) => Ok(Some(Change::Size(size))),
            Operation::UpdateConfig(Setting::StarshipLog(log_level)) => {
                trace!("Updating config of starship level");
                self.log_config.writer()?.starship_level = log_level;
                Ok(None)
            }
            Operation::Alert(alert) => Ok(Some(Change::Message(alert))),
            Operation::StartCommand(command) => Ok(Some(Change::Input(command.to_string()))),
            Operation::Collect(c) => Ok(Some(Change::InputChar(c))),
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
    UpdateConfig(Setting),
    /// Alerts the user with a message.
    Alert(ShowMessageParams),
    /// Open input box for a command.
    StartCommand(Command),
    /// Input to input box.
    Collect(char),
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

#[derive(Debug, Error)]
pub enum Fault {
    #[error("logger: {0}")]
    Log(#[from] logging::Fault),
    #[error("language server protocol: {0}")]
    Lsp(#[from] lsp::Fault),
}

//! Implements the business logic of the application.
mod logging;
mod lsp;

use {
    crate::ui::{Change, Setting},
    clap::ArgMatches,
    core::convert::TryFrom,
    log::{error, trace},
    logging::LogConfig,
    lsp::LspServer,
    lsp_types::{
        MessageType, Position, Range, ShowMessageParams, ShowMessageRequestParams,
        TextDocumentItem, TextEdit,
    },
    parse_display::Display as ParseDisplay,
    std::{
        collections::HashMap,
        env,
        ffi::OsStr,
        fmt, fs,
        io::{self, ErrorKind},
        path::PathBuf,
    },
    thiserror::Error,
    url::{ParseError, Url},
};

/// Defines a [`Range`] covering the entire document.
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

/// Signifies the configuration of the application initialization.
///
/// Using a custom struct rather than [`ArgMatches`] allows for external code to easily configure use of the application as a library.
#[derive(Clone, Debug)]
pub struct Arguments {
    /// Signifies the file to be viewed.
    pub file: Option<String>,
    /// Signifies the handle to configure the application logger.
    pub log_config: LogConfig,
    /// Signifies the current working directory of the application.
    pub working_dir: PathBuf,
}

impl TryFrom<ArgMatches<'_>> for Arguments {
    type Error = Fault;

    fn try_from(value: ArgMatches<'_>) -> Result<Self, Self::Error> {
        Ok(Self {
            file: value.value_of("file").map(str::to_string),
            log_config: LogConfig::new()?,
            working_dir: env::current_dir().map_err(Fault::WorkingDir)?,
        })
    }
}

/// Signifies the business logic of the application.
#[derive(Debug)]
pub(crate) struct Sheet {
    /// Signifies the document being viewed.
    doc: Option<TextDocumentItem>,
    /// The document wraps long lines.
    is_wrapped: bool,
    /// The [`LspServer`]s managed by this document.
    lsp_servers: HashMap<String, LspServer>,
    /// Provides handle to be able to modify logger settings.
    log_config: LogConfig,
    /// The input for `command`.
    input: String,
    /// The current command to be implemented.
    command: Option<Command>,
    /// The working directory of the application.
    working_dir: Url,
}

impl Sheet {
    /// Creates a new [`Sheet`].
    pub(crate) fn new(arguments: &Arguments) -> Result<Self, Fault> {
        Ok(Self {
            doc: None,
            is_wrapped: false,
            lsp_servers: HashMap::default(),
            input: String::new(),
            log_config: arguments.log_config.clone(),
            command: None,
            working_dir: Url::from_directory_path(arguments.working_dir.clone())
                .map_err(|_| Fault::InvalidPath)?,
        })
    }

    /// Performs `operation` and returns the appropriate [`Change`]s.
    pub(crate) fn operate(&mut self, operation: Operation) -> Result<Option<Change>, Fault> {
        match operation {
            Operation::Reset => Ok(Some(Change::Reset)),
            Operation::Confirm(action) => Ok(Some(Change::Question(
                ShowMessageRequestParams::from(action),
            ))),
            Operation::Quit => {
                unreachable!("attempted to execute `Quit` operation");
            }
            Operation::UpdateConfig(Setting::File(file)) => self.open_file(file),
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
                trace!("updating starship log level to `{}`", log_level);
                self.log_config.writer()?.starship_level = log_level;
                Ok(None)
            }
            Operation::Alert(alert) => Ok(Some(Change::Message(alert))),
            Operation::StartCommand(command) => {
                let prompt = command.to_string();

                self.command = Some(command);
                Ok(Some(Change::Input(prompt)))
            }
            Operation::Collect(c) => {
                self.input.push(c);
                Ok(Some(Change::InputChar(c)))
            }
            Operation::Execute => {
                if self.command.is_some() {
                    let file = self.input.clone();

                    self.input.clear();
                    self.open_file(file)
                } else {
                    Ok(None)
                }
            }
        }
    }

    /// Opens `file` as a [`Document`].
    fn open_file(&mut self, file: String) -> Result<Option<Change>, Fault> {
        match self.get_document(file) {
            Ok(doc) => {
                let process = match doc.language_id.as_str() {
                    "rust" => Ok("rls"),
                    _ => Err(Fault::MissingLanguage(doc.language_id.clone())),
                }?;
                let lsp_server = self
                    .lsp_servers
                    .entry(doc.language_id.clone())
                    .or_insert(LspServer::new(process, &self.working_dir)?);

                lsp_server.did_open(&doc)?;
                self.doc = Some(doc.clone());
                Ok(Some(Change::Text {
                    edits: vec![TextEdit::new(ENTIRE_DOCUMENT, doc.text)],
                    is_wrapped: self.is_wrapped,
                }))
            }
            Err(error) => Ok(Some(Change::Message(ShowMessageParams::from(error)))),
        }
    }

    /// Creates a [`TextDocumentItem`] from `path`.
    fn get_document(&self, path: String) -> Result<TextDocumentItem, DocumentError> {
        let uri = self.working_dir.join(&path).map_err(DocumentError::from)?;
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
}

impl Drop for Sheet {
    fn drop(&mut self) {
        if let Some(doc) = &self.doc {
            if let Some(lsp_server) = self.lsp_servers.get_mut(&doc.language_id) {
                if let Err(e) = lsp_server.did_close(doc) {
                    error!(
                        "failed to inform language server process about closing {}",
                        e
                    );
                }
            }
        }
    }
}

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
    /// Executes the current command.
    Execute,
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

/// An error in the application.
#[derive(Debug, Error)]
pub enum Fault {
    /// An error in the log.
    #[error("logger: {0}")]
    Log(#[from] logging::Fault),
    /// An error in the language server protocol.
    #[error("language server protocol: {0}")]
    Lsp(#[from] lsp::Fault),
    /// Application does not support language.
    #[error("paper does not currently implement language server protocol for `{0}`")]
    MissingLanguage(String),
    /// An error while attempting to retrieve the current working directory.
    #[error("invalid working directory: {0}")]
    WorkingDir(#[source] io::Error),
    /// Invalid path.
    #[error("invalid path")]
    InvalidPath,
}

//! Implements the modality of the application.
mod lsp;

pub(crate) use lsp::Fault as LspError;

use {
    crate::{
        ui::{Change, Setting},
        Failure,
    },
    log::{trace, LevelFilter, Log, Metadata, Record},
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
        fs::{self, File},
        io::{self, ErrorKind, Write},
        sync::{Arc, RwLock},
    },
    thiserror::Error,
    time::PrimitiveDateTime,
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

/// Createse a [`TextDocumentItem`] from `path`.
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

/// Provides a handle to dynamically configure the [`Logger`].
#[derive(Debug, Default)]
struct LogHandle {
    /// A pointer to the [`Writer`] used by the [`Logger`].
    writer: Arc<RwLock<Writer>>,
}

impl LogHandle {
    /// Sets the level at which starship records are logged.
    fn set_starship_level(&self, level: LevelFilter) -> Result<(), Failure> {
        trace!("set starship_level {}", level);
        // TODO: Replace Failure.
        self.writer
            .write()
            .map_err(|_| Failure::LogWriter)?
            .starship_level = level;
        Ok(())
    }
}

/// Implements writing logs to a file.
#[derive(Debug)]
struct Writer {
    /// Defines the file that stores logs.
    file: Option<File>,
    /// Defines the level at which logs from starship are allowed.
    starship_level: LevelFilter,
}

impl Writer {
    /// Creates the file to store logs.
    fn init(&mut self) -> Result<(), Failure> {
        let log_filename = "paper.log".to_string();

        self.file =
            Some(File::create(&log_filename).map_err(|e| Failure::CreateLogFile(log_filename, e))?);
        Ok(())
    }

    /// Writes `record` to the file of `self`.
    fn write(&self, record: &Record<'_>) {
        if let Some(mut file) = self.file.as_ref() {
            let _ = writeln!(
                file,
                "{} [{}] {}: {}",
                PrimitiveDateTime::now().format("%F %T"),
                record.level(),
                record.target(),
                record.args()
            );
        }
    }

    /// Flushes the buffer of the writer.
    fn flush(&self) {
        if let Some(mut file) = self.file.as_ref() {
            let _ = file.flush();
        }
    }
}

impl Default for Writer {
    fn default() -> Self {
        Self {
            file: None,
            starship_level: LevelFilter::Off,
        }
    }
}

/// Implements the logger of the application.
struct Logger {
    /// The [`Writer`] of the logger.
    writer: Arc<RwLock<Writer>>,
}

impl Logger {
    /// Creates a new [`Logger`].
    fn new(writer: &Arc<RwLock<Writer>>) -> Self {
        Self {
            writer: Arc::clone(writer),
        }
    }
}

impl Log for Logger {
    fn enabled(&self, metadata: &Metadata<'_>) -> bool {
        if let Ok(writer) = self.writer.read() {
            if metadata.target().starts_with("starship") {
                metadata.level() <= writer.starship_level
            } else {
                true
            }
        } else {
            false
        }
    }

    fn log(&self, record: &Record<'_>) {
        if self.enabled(record.metadata()) {
            if let Ok(writer) = self.writer.read() {
                writer.write(record);
            }
        }
    }

    fn flush(&self) {
        if let Ok(writer) = self.writer.read() {
            writer.flush();
        }
    }
}

/// Signfifies display of the current file.
#[derive(Debug, Default)]
pub(crate) struct Sheet {
    /// The document being displayed.
    doc: Option<TextDocumentItem>,
    /// The document wraps long lines.
    is_wrapped: bool,
    /// The [`LspServer`]s managed by this document.
    lsp_servers: HashMap<String, LspServer>,
    /// Provides handle to be able to modify logger settings.
    log_handle: LogHandle,
}

impl Sheet {
    /// Initizlies logger of application.
    pub(crate) fn init(&mut self) -> Result<(), Failure> {
        self.log_handle
            .writer
            .write()
            .map_err(|_| Failure::LogWriter)?
            .init()?;
        log::set_boxed_logger(Box::new(Logger::new(&self.log_handle.writer)))?;
        log::set_max_level(LevelFilter::Trace);
        trace!("Logger Initialized");
        Ok(())
    }

    /// Performs `operation`.
    pub(crate) fn operate(&mut self, operation: Operation) -> Result<Option<Change>, Failure> {
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
                self.log_handle.set_starship_level(log_level)?;
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

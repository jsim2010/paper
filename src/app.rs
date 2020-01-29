//! Implements the application logic of `paper`.
pub mod logging;
pub mod lsp;

use {
    crate::ui::{Change, Setting, Size, Update},
    clap::ArgMatches,
    core::{
        convert::TryFrom,
        ops::{Bound, RangeBounds},
    },
    log::{error, trace},
    logging::LogConfig,
    lsp::LspServer,
    lsp_types::{
        MessageType, Position, Range, ShowMessageParams, ShowMessageRequestParams,
        TextDocumentItem, TextEdit,
    },
    parse_display::Display as ParseDisplay,
    starship::{context::Context, print},
    std::{
        cmp,
        collections::{hash_map::Entry, HashMap},
        env,
        ffi::OsStr,
        fmt, fs,
        io::{self, ErrorKind},
        path::PathBuf,
    },
    thiserror::Error,
    url::{ParseError, Url},
};

/// Configures the initialization of `paper`.
#[derive(Clone, Debug, Default)]
pub struct Arguments {
    /// The file to be viewed.
    ///
    /// [`None`] indicates that the display should be empty.
    ///
    /// [`None`]: https://doc.rust-lang.org/std/option/enum.Option.html#variant.None
    pub file: Option<String>,
    /// Configures the logger.
    ///
    /// [`None`] indicates that `paper` will not configure logging during runtime.
    ///
    /// [`None`]: https://doc.rust-lang.org/std/option/enum.Option.html#variant.None
    pub log_config: Option<LogConfig>,
    /// The working directory of `paper`.
    pub working_dir: PathUrl,
}

impl TryFrom<ArgMatches<'_>> for Arguments {
    type Error = Fault;

    #[inline]
    fn try_from(value: ArgMatches<'_>) -> Result<Self, Self::Error> {
        Ok(Self {
            file: value.value_of("file").map(str::to_string),
            log_config: Some(LogConfig::new()?),
            working_dir: PathUrl::try_from(env::current_dir().map_err(Fault::WorkingDir)?)?,
        })
    }
}

/// A URL that is a valid path.
#[derive(Clone, Debug)]
pub struct PathUrl {
    path: PathBuf,
    url: Url,
}

impl Default for PathUrl {
    fn default() -> Self {
        let path = PathBuf::default();

        Self {
            url: Url::from_file_path(path.clone()).unwrap(),
            path,
        }
    }
}

impl TryFrom<PathBuf> for PathUrl {
    type Error = Fault;

    fn try_from(value: PathBuf) -> Result<Self, Self::Error> {
        Ok(Self {
            url: Url::from_directory_path(value.clone()).map_err(|_| {
                Fault::Url(value.to_string_lossy().to_string())
            })?,
            path: value,
        })
    }
}

/// An error from which the application was unable to recover.
#[derive(Debug, Error)]
pub enum Fault {
    /// An error from [`logging`].
    ///
    /// [`logging`]: logging/index.html
    #[error("{0}")]
    Log(#[from] logging::Fault),
    /// An error from [`lsp`].
    ///
    /// [`lsp`]: lsp/index.html
    #[error("{0}")]
    Lsp(#[from] lsp::Fault),
    /// An error while determining the current working directory.
    #[error("while determining working directory: {0}")]
    WorkingDir(#[source] io::Error),
    /// An error while converting a directory to a URL.
    #[error("while converting `{0}` to a URL")]
    Url(String),
    /// An error occurred while parsing a URL.
    #[error("while parsing URL: {0}")]
    ParseUrl(#[from] ParseError),
}

/// Signifies the business logic of the application.
#[derive(Debug)]
pub(crate) struct Sheet {
    /// Signifies the document being viewed.
    doc: Option<Document>,
    wrap_length: Option<usize>,
    /// The [`LspServer`]s managed by this document.
    lsp_servers: HashMap<String, LspServer>,
    /// Provides handle to be able to modify logger settings.
    log_config: Option<LogConfig>,
    /// The input for `command`.
    input: String,
    /// The current command to be implemented.
    command: Option<Command>,
    /// The working directory of the application.
    working_dir: PathUrl,
    /// The size of the user interface.
    size: Size,
}

impl Sheet {
    /// Creates a new [`Sheet`].
    pub(crate) fn new(arguments: &Arguments) -> Self {
        Self {
            doc: None,
            wrap_length: None,
            lsp_servers: HashMap::default(),
            input: String::new(),
            log_config: arguments.log_config.clone(),
            command: None,
            working_dir: arguments.working_dir.clone(),
            size: Size::default(),
        }
    }

    /// Performs `operation` and returns the appropriate [`Change`]s.
    pub(crate) fn operate(&mut self, operation: Operation) -> Result<Option<Update>, Fault> {
        let change = match operation {
            Operation::Reset => {
                self.input.clear();
                Some(Change::Reset)
            }
            Operation::Confirm(action) => Some(Change::Question(
                ShowMessageRequestParams::from(action),
            )),
            Operation::Quit => None,
            Operation::UpdateSetting(setting) => self.update_setting(setting)?,
            Operation::Alert(alert) => Some(Change::Message(alert)),
            Operation::StartCommand(command) => {
                let prompt = command.to_string();

                self.command = Some(command);
                Some(Change::Input(prompt))
            }
            Operation::Collect(c) => {
                self.input.push(c);
                Some(Change::InputChar(c))
            }
            Operation::Execute => {
                if self.command.is_some() {
                    let file = self.input.clone();

                    self.input.clear();
                    self.open_file(file)?
                } else {
                    None
                }
            }
            Operation::Move(movement) => {
                let scroll_value = self.scroll_value();

                if let Some(doc) = &mut self.doc {
                    match movement {
                        Movement::SingleDown => {
                            doc.move_selection_down(1);
                        }
                        Movement::SingleUp => {
                            doc.move_selection_up(1);
                        }
                        Movement::HalfDown => {
                            doc.move_selection_down(scroll_value);
                        }
                        Movement::HalfUp => {
                            doc.move_selection_up(scroll_value);
                        }
                    };
                }

                self.text_change()
            }
            Operation::Delete => {
                if let Some(doc) = &mut self.doc {
                    doc.item.version = doc.item.version.wrapping_add(1);
                    let mut lines: Vec<&str> = doc.item.text.lines().collect();
                    let edit = TextEdit::new(doc.selection.range, lines.drain(doc.selection).collect());
                    doc.item.text = lines.join("\n");
                    let cursor_line_count = u64::try_from(doc.selection.end_bound - doc.selection.start_bound).unwrap_or(u64::max_value());
                    doc.rows = doc.rows.iter().filter_map(|row| {
                        let row_line = usize::try_from(row.line()).unwrap_or(usize::max_value());
                        if doc.selection.contains(&row_line) {
                            None
                        } else {
                            let mut new_row = row.clone();

                            if row_line >= doc.selection.end_bound {
                                new_row.line -= cursor_line_count;
                            }

                            Some(new_row)
                        }
                    }).collect();

                    if let Some(lsp_server) = self.lsp_servers.get_mut(&doc.item.language_id.clone()) {
                        lsp_server.did_change(&doc.item, edit)?;
                    }
                };

                self.text_change()
            }
        };

        Ok(change.map(|c| Update::new(
            print::get_prompt(Context::new_with_dir(
                ArgMatches::default(),
                &self.working_dir.path,
            ))
            .replace("[J", ""),
            c,
        )))
    }

    /// Updates `self` based on `setting`.
    fn update_setting(&mut self, setting: Setting) -> Result<Option<Change>, Fault> {
        match setting {
            Setting::File(file) => self.open_file(file),
            Setting::Wrap(is_wrapped) => {
                if self.wrap_length.is_some() == is_wrapped {
                    Ok(None)
                } else {
                    self.wrap_length = if is_wrapped {Some(usize::from(self.size.columns))} else {None};
                    
                    if let Some(doc) = &mut self.doc {
                        doc.update_rows(self.wrap_length);
                    }

                    Ok(self.text_change())
                }
            }
            Setting::Size(size) => {
                self.size = size.clone();
                Ok(Some(Change::Size(size)))
            }
            Setting::StarshipLog(log_level) => {
                trace!("updating starship log level to `{}`", log_level);

                if let Some(log_config) = &self.log_config {
                    log_config.writer()?.starship_level = log_level;
                }

                Ok(None)
            }
        }
    }

    fn text_change(&self) -> Option<Change> {
        self.doc.as_ref().map(|doc| Change::Text {
            cursor: doc.selection.range,
            rows: doc.rows.clone(),
        })
    }

    /// Returns the amount to move for scrolling.
    fn scroll_value(&self) -> u64 {
        u64::from(self.size.rows.wrapping_div(3))
    }

    /// Opens `file` as a [`Document`].
    fn open_file(&mut self, file: String) -> Result<Option<Change>, Fault> {
        let uri = self.working_dir.url.join(&file)?;

        match Document::new(uri, self.wrap_length) {
            Ok(doc) => {
                if let Some(old_doc) = &self.doc {
                    if let Some(lsp_server) = self.lsp_servers.get_mut(&old_doc.item.language_id) {
                        lsp_server.did_close(&old_doc.item)?;
                    }
                }

                let cmd = match doc.item.language_id.as_str() {
                    "rust" => Some("rls"),
                    _ => None,
                };

                if let Some(process) = cmd {
                    let lsp_server = match self.lsp_servers.entry(doc.item.language_id.clone()) {
                        Entry::Vacant(entry) => {
                            entry.insert(LspServer::new(process, &self.working_dir.url)?)
                        }
                        Entry::Occupied(entry) => entry.into_mut(),
                    };

                    lsp_server.did_open(&doc.item)?;
                }

                self.doc = Some(doc);
                Ok(self.text_change())
            }
            Err(error) => Ok(Some(Change::Message(ShowMessageParams::from(error)))),
        }
    }
}

impl Drop for Sheet {
    fn drop(&mut self) {
        if let Some(doc) = &self.doc {
            if let Some(lsp_server) = self.lsp_servers.get_mut(&doc.item.language_id) {
                if let Err(e) = lsp_server.did_close(&doc.item) {
                    error!(
                        "failed to inform language server process about closing {}",
                        e
                    );
                }
            }
        }
    }
}

#[derive(Clone, Debug)]
pub(crate) struct Row {
    line: u64,
    text: String,
}

impl Row {
    pub(crate) fn line(&self) -> u64 {
        self.line
    }

    pub(crate) fn text(&self) -> &String {
        &self.text
    }
}

#[derive(Debug)]
struct Document {
    item: TextDocumentItem,
    rows: Vec<Row>,
    /// The current user selection.
    selection: Selection,
}

impl Document {
    fn new(uri: Url, wrap_length: Option<usize>) -> Result<Self, DocumentError> {
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
            ErrorKind::NotFound => DocumentError::NonExistantFile(uri.to_string()),
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

        let mut doc = Self {
            item: TextDocumentItem::new(uri, language_id, 0, text),
            rows: Vec::new(),
            selection: Selection::default(),
        };

        doc.update_rows(wrap_length);

        if !doc.item.text.is_empty() {
            doc.selection.set_end_bound(1);
        }

        Ok(doc)
    }

    fn update_rows(&mut self, wrap_length: Option<usize>) {
        self.rows = Vec::new();

        for (index, mut line) in self.item.text.lines().enumerate() {
            let mut is_final = false;

            loop {
                let row_text = if let Some(length) = wrap_length {
                    if line.len() > length {
                        let split = line.split_at(length);
                        line = split.1;
                        split.0
                    } else {
                        is_final = true;
                        line
                    }
                } else {
                    is_final = true;
                    line
                };

                self.rows.push(Row {
                    line: u64::try_from(index).unwrap_or(u64::max_value()),
                    text: row_text.to_string(),
                });

                if is_final {
                    break;
                }
            }
        }
    }

    /// Returns the number of lines in `self`.
    fn line_count(&self) -> u64 {
        u64::try_from(self.item.text.lines().count())
            .unwrap_or(u64::max_value())
    }

    fn move_selection_down(&mut self, amount: u64) {
        self.selection.move_down(amount, self.line_count());
    }

    fn move_selection_up(&mut self, amount: u64) {
        self.selection.move_up(amount);
    }
}

/// Represents a user selection.
#[derive(Clone, Copy, Debug)]
struct Selection {
    /// The text [`Range`] of the selection.
    range: Range,
    /// The start bound, included.
    start_bound: usize,
    /// The end bound, excluded.
    end_bound: usize,
}

impl Selection {
    /// Moves `self` down by `amount` lines.
    fn move_down(&mut self, amount: u64, line_count: u64) {
        let end_line = cmp::min(
            self.range.end.line.saturating_add(amount),
            line_count,
        );
        self.set_start_bound(
            self.range
                .start
                .line
                .saturating_add(end_line.saturating_sub(self.range.end.line)),
        );
        self.set_end_bound(end_line);
    }

    /// Moves `self` up by `amount` lines.
    fn move_up(&mut self, amount: u64) {
        let start_line = self.range.start.line.saturating_sub(amount);
        self.set_end_bound(
            self.range
                .end
                .line
                .saturating_sub(self.range.start.line.saturating_sub(start_line)),
        );
        self.set_start_bound(start_line);
    }

    /// Sets the start bound (included).
    fn set_start_bound(&mut self, value: u64) {
        self.range.start.line = value;
        self.start_bound = usize::try_from(value).unwrap_or(usize::max_value());
    }

    /// Sets the end bound (excluded).
    fn set_end_bound(&mut self, value: u64) {
        self.range.end.line = value;
        self.end_bound = usize::try_from(value).unwrap_or(usize::max_value());
    }
}

impl Default for Selection {
    fn default() -> Self {
        Self {
            range: Range::new(Position::new(0, 0), Position::new(1, 0)),
            start_bound: 0,
            end_bound: 0,
        }
    }
}

impl RangeBounds<usize> for Selection {
    fn start_bound(&self) -> Bound<&usize> {
        Bound::Included(&self.start_bound)
    }

    fn end_bound(&self) -> Bound<&usize> {
        Bound::Excluded(&self.end_bound)
    }
}

/// Signifies a command that a user can give to the application.
#[derive(Debug, ParseDisplay, PartialEq)]
pub(crate) enum Command {
    /// Opens a given file.
    #[display("Open <file>")]
    Open,
}

/// A movement to the cursor.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum Movement {
    /// Moves cursor down by a single line.
    SingleDown,
    /// Moves cursor up by a single line.
    SingleUp,
    /// Moves cursor down close to half the height.
    HalfDown,
    /// Moves cursor up close to half the height.
    HalfUp,
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
    #[inline]
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
    /// Updates a setting.
    UpdateSetting(Setting),
    /// Alerts the user with a message.
    Alert(ShowMessageParams),
    /// Open input box for a command.
    StartCommand(Command),
    /// Input to input box.
    Collect(char),
    /// Executes the current command.
    Execute,
    /// Moves the cursor.
    Move(Movement),
    /// Deletes the current selection.
    Delete,
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
    #[inline]
    #[must_use]
    fn from(value: ConfirmAction) -> Self {
        Self {
            typ: MessageType::Info,
            message: value.to_string(),
            actions: None,
        }
    }
}

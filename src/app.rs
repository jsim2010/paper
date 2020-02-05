//! Implements the application logic of `paper`.
pub mod logging;
pub mod lsp;

use {
    crate::ui::{Change, Setting, Size, Update},
    clap::ArgMatches,
    core::{
        cmp,
        convert::{TryFrom, TryInto},
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
        cell::RefCell,
        collections::HashMap,
        env,
        ffi::OsStr,
        fmt, fs,
        io::{self, ErrorKind},
        path::{Path, PathBuf},
        rc::Rc,
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
///
/// Useful for preventing repeat translations between URL and path formats.
#[derive(Clone, Debug)]
pub struct PathUrl {
    /// The path.
    path: PathBuf,
    /// The URL.
    url: Url,
}

impl PathUrl {
    fn join(&self, path: &str) -> Result<Self, Fault>{
        let mut joined_path = self.path.clone();

        joined_path.push(path);
        joined_path.try_into()
    }

    fn language_id(&self) -> &str {
        self.path.extension().and_then(OsStr::to_str).map(|ext| match ext {
            "rs" => "rust",
            x => x,
        }).unwrap_or("")
    }
}

impl AsRef<Path> for PathUrl {
    fn as_ref(&self) -> &Path {
        self.path.as_ref()
    }
}

impl Default for PathUrl {
    #[must_use]
    fn default() -> Self {
        #[allow(clippy::result_expect_used)] // Default path should not fail and failure cannot be propogated.
        Self::try_from(PathBuf::default()).expect("creating default `PathUrl`")
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

/// The processor of the application.
#[derive(Debug)]
pub(crate) struct Processor {
    /// The currently visible pane.
    pane: Pane,
    /// Handle to access and modify logger settings.
    ///
    /// [`None`] signifies that logger settings are not dynamically configurable.
    log_config: Option<LogConfig>,
    /// The input of a command.
    input: String,
    /// The current command to be implemented.
    command: Option<Command>,
    /// The working directory of the application.
    working_dir: Rc<PathUrl>,
}

impl Processor {
    /// Creates a new [`Processor`].
    pub(crate) fn new(arguments: &Arguments) -> Self {
        let working_dir = Rc::new(arguments.working_dir.clone());

        Self {
            pane: Pane::new(&working_dir),
            input: String::new(),
            log_config: arguments.log_config.clone(),
            command: None,
            working_dir,
        }
    }

    /// Performs `operation` and returns the appropriate [`Change`]s.
    pub(crate) fn operate(&mut self, operation: Operation) -> Result<Option<Update>, Fault> {
        let change = match operation {
            Operation::UpdateSetting(setting) => self.update_setting(setting)?,
            Operation::Confirm(action) => Some(Change::Question(
                ShowMessageRequestParams::from(action),
            )),
            Operation::Reset => {
                self.input.clear();
                Some(Change::Reset)
            }
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
                    let change = self.pane.open_doc(&self.input);

                    self.input.clear();
                    change
                } else {
                    None
                }
            }
            Operation::Document(doc_op) => self.pane.operate(doc_op),
            Operation::Quit => None,
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
            Setting::File(file) => {
                Ok(self.pane.open_doc(&file))
            }
            Setting::Wrap(is_wrapped) => {
                Ok(self.pane.control_wrap(is_wrapped))
            }
            Setting::Size(size) => {
                Ok(Some(self.pane.update_size(size)))
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
}

#[derive(Debug, Default)]
struct Pane {
    doc: Option<Document>,
    /// The number of lines by which a scroll moves.
    scroll_amount: Rc<RefCell<Amount>>,
    /// The length at which displayed lines may be wrapped.
    wrap_length: Rc<RefCell<Swival<usize>>>,
    working_dir: Rc<PathUrl>,
    /// The [`LspServer`]s managed by the application.
    lsp_servers: HashMap<String, Rc<RefCell<LspServer>>>,
}

impl Pane {
    fn new(working_dir: &Rc<PathUrl>) -> Self {
        Self {
            doc: None,
            scroll_amount: Rc::new(RefCell::new(Amount(0))),
            wrap_length: Rc::new(RefCell::new(Swival::default())),
            lsp_servers: HashMap::default(),
            working_dir: Rc::clone(working_dir),
        }
    }

    fn operate(&mut self, operation: DocOp) -> Option<Change> {
        self.doc.as_mut().map(|doc| {
            match operation {
                DocOp::Move(vector) => doc.move_selection(vector),
                DocOp::Delete => doc.delete_selection(),
            }
        })
    }

    fn open_doc(&mut self, path: &str) -> Option<Change> {
        Some(match self.create_doc(path) {
            Ok(doc) => {
                let change = doc.text_change();
                let _ = self.doc.replace(doc);

                change
            }
            Err(error) => Change::Message(ShowMessageParams::from(error)),
        })
    }

    fn create_doc(&mut self, path: &str) -> Result<Document, DocumentError> {
        let doc_path = self.working_dir.join(path)?;
        let language_id = doc_path.language_id();
        let lsp_server = self.lsp_servers.get(language_id).cloned();

        if lsp_server.is_none() {
            if let Some(lsp_server) = LspServer::new(language_id, &self.working_dir.url)?.map(|server| Rc::new(RefCell::new(server))) {
                let _ = self.lsp_servers.insert(language_id.to_string(), Rc::clone(&lsp_server));
            }
        }

        Document::new(doc_path, &self.wrap_length, lsp_server, &self.scroll_amount)
    }

    fn control_wrap(&mut self, is_wrapped: bool) -> Option<Change> {
        if self.wrap_length.borrow_mut().control(is_wrapped) {
            self.doc.as_mut().map(|doc| {
                doc.text_change()
            })
        } else {
            None
        }
    }

    fn update_size(&mut self, size: Size) -> Change {
        self.wrap_length.borrow_mut().set(size.columns.into());
        self.scroll_amount.borrow_mut().set(u64::from(size.rows.wrapping_div(3)));
        Change::Size(size)
    }
}

/// A file and the user's current interactions with it.
#[derive(Debug)]
struct Document {
    uri: Url,
    text: Text,
    /// The current user selection.
    selection: Selection,
    lsp_server: Option<Rc<RefCell<LspServer>>>,
    scroll_amount: Rc<RefCell<Amount>>,
}

impl Document {
    /// Creates a new [`Document`].
    fn new(path: PathUrl, wrap_length: &Rc<RefCell<Swival<usize>>>, lsp_server: Option<Rc<RefCell<LspServer>>>, scroll_amount: &Rc<RefCell<Amount>>) -> Result<Self, DocumentError> {
        let text = Text::new(&path, wrap_length)?;
        let mut selection = Selection::default();

        if !text.is_empty() {
            selection.set_end_bound(1);
        }

        if let Some(server) = &lsp_server {
            server.borrow_mut().did_open(&path.url, path.language_id(), text.version, &text.content).unwrap();
        }

        Ok(Self {
            uri: path.url,
            text,
            selection,
            lsp_server,
            scroll_amount: Rc::clone(scroll_amount),
        })
    }

    fn delete_selection(&mut self) -> Change {
        self.text.delete_selection(&self.selection);

        if let Some(server) = &self.lsp_server {
            server.borrow_mut().did_change(&self.uri, self.text.version, &self.text.content, TextEdit::new(self.selection.range, String::new())).unwrap();
        }

        self.text_change()
    }

    fn text_change(&self) -> Change {
        Change::Text {
            cursor: self.selection.range,
            rows: self.text.rows().collect(),
        }
    }

    /// Returns the number of lines in `self`.
    fn line_count(&self) -> u64 {
        u64::try_from(self.text.content.lines().count())
            .unwrap_or(u64::max_value())
    }

    fn move_selection(&mut self, vector: Vector) -> Change {
        let amount = match vector.magnitude {
            Magnitude::Single => 1,
            Magnitude::Half => self.scroll_amount.borrow().value(),
        };
        match vector.direction {
            Direction::Down => {
                self.selection.move_down(amount, self.line_count());
            }
            Direction::Up => {
                self.selection.move_up(amount);
            }
        }

        self.text_change()
    }
}

impl Drop for Document {
    fn drop(&mut self) {
        trace!("dropping {:?}", self.uri);
        if let Some(lsp_server) = &self.lsp_server {
            if let Err(e) = lsp_server.borrow_mut().did_close(&self.uri) {
                error!(
                    "failed to inform language server process about closing {}",
                    e
                );
            }
        }
    }
}

#[derive(Debug)]
struct Text {
    content: String,
    wrap_length: Rc<RefCell<Swival<usize>>>,
    version: i64,
}

impl Text {
    fn new(path: &PathUrl, wrap_length: &Rc<RefCell<Swival<usize>>>) -> Result<Self, DocumentError> {
        let content = fs::read_to_string(path.clone()).map_err(|error| match error.kind() {
            ErrorKind::NotFound => DocumentError::NonExistantFile(path.url.to_string()),
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

        Ok(Self {
            content,
            wrap_length: Rc::clone(wrap_length),
            version: 0,
        })
    }

    fn is_empty(&self) -> bool {
        self.content.is_empty()
    }

    fn rows<'a>(&'a self) -> Rows<'a> {
        Rows::new(&self.content, self.wrap_length.borrow().get().cloned())
    }

    fn delete_selection(&mut self, selection: &Selection) {
        let mut newline_indices = self.content.match_indices('\n');
        let start_index = if selection.start_bound == 0 {
            0
        } else {
            newline_indices.nth(selection.start_bound - 1).unwrap().0 + 1
        };
        let end_index = newline_indices.nth(selection.end_bound - 1 - selection.start_bound).unwrap().0;
        let _ = self.content.drain(start_index..=end_index);
        self.version = self.version.wrapping_add(1);
    }
}

struct Rows<'a> {
    s: &'a str,
    max_len: usize,
    current_line: u64,
}

impl<'a> Rows<'a> {
    fn new(s: &'a str, max_len: Option<usize>) -> Self {
        Rows {s, max_len: max_len.unwrap_or(usize::max_value()), current_line: 0}
    }
}

impl Iterator for Rows<'_> {
    type Item = Row;

    fn next(&mut self) -> Option<Self::Item> {
        if self.s.is_empty() {
            None
        } else {
            let (line_len, extra_len) = if let Some(newline_len) = self.s.find('\n') {
                let line_end = newline_len - 1;

                if self.s.get(line_end..=line_end) == Some("\r") {
                    (line_end, 2)
                } else {
                    (newline_len, 1)
                }
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
            let row = Row {text: row_text.to_string(), line: self.current_line};

            if rm_len != 0 {
                self.current_line += 1;
            }

            self.s = new_s;
            Some(row)
        }
    }
}

#[derive(Debug, Default)]
struct Amount(u64);

impl Amount {
    fn value(&self) -> u64 {
        self.0
    }

    fn set(&mut self, amount: u64) {
        self.0 = amount;
    }
}

#[derive(Debug, Default)]
struct Swival<T> {
    value: T,
    is_enabled: bool,
}

impl<T> Swival<T> {
    fn control(&mut self, enable: bool) -> bool {
        let is_toggled = enable != self.is_enabled;

        if is_toggled {
            self.is_enabled = enable;
        }

        is_toggled
    }

    fn get(&self) -> Option<&T> {
        if self.is_enabled {
            Some(&self.value)
        } else {
            None
        }
    }

    fn set(&mut self, value: T) {
        self.value = value;
    }
}

/// Represents a row in the user interface.
#[derive(Clone, Debug)]
pub(crate) struct Row {
    /// The line of the row.
    line: u64,
    /// The text of the row.
    text: String,
}

impl Row {
    /// Returns the line of `self`.
    pub(crate) const fn line(&self) -> u64 {
        self.line
    }

    /// Returns the text of `self`.
    pub(crate) const fn text(&self) -> &String {
        &self.text
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
pub(crate) struct Vector {
    direction: Direction,
    magnitude: Magnitude,
}

impl Vector {
    pub(crate) fn new(direction: Direction, magnitude: Magnitude) -> Self {
        Self {
            direction,
            magnitude,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum Direction {
    Down,
    Up,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum Magnitude {
    Single,
    Half,
}

/// Signifies errors associated with [`Document`].
#[derive(Debug, Error)]
enum DocumentError {
    /// Error while parsing url.
    #[error("unable to parse url: {0}")]
    Parse(#[from] ParseError),
    /// File does not exist.
    #[error("file `{0}` does not exist")]
    NonExistantFile(String),
    /// Io error.
    #[error("io: {0}")]
    Io(#[from] io::Error),
    #[error("lsp: {0}")]
    Lsp(#[from] lsp::Fault),
    #[error("{0}")]
    Fault(#[from] Fault),
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
    /// An operation to edit the text or selection of the document.
    Document(DocOp),
}

#[derive(Debug, PartialEq)]
pub(crate) enum DocOp {
    /// Moves the cursor.
    Move(Vector),
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

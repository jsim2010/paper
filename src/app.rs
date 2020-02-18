//! Implements the application logic of `paper`.
pub mod logging;
pub mod lsp;

mod translate;

use {
    // TODO: Move everything out of ui.
    crate::io::{
        ui::{Selection, SelectionConversionError, Size},
        Input, Output, PathUrl, Setting, UrlError,
    },
    clap::ArgMatches,
    log::{error, trace},
    logging::LogConfig,
    lsp::LspServer,
    lsp_types::{MessageType, ShowMessageParams, ShowMessageRequestParams, TextEdit},
    starship::{context::Context, print},
    std::{
        cell::RefCell,
        collections::HashMap,
        fs,
        io::{self, ErrorKind},
        rc::Rc,
    },
    thiserror::Error,
    toml::{value::Table, Value},
    translate::{Command, Direction, DocOp, Interpreter, Magnitude, Operation, Vector},
    url::ParseError,
};

/// An empty [`Selection`].
static EMPTY_SELECTION: Selection = Selection::empty();

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
    /// An error occurred while parsing a URL.
    #[error("while parsing URL: {0}")]
    ParseUrl(#[from] ParseError),
    /// An error while converting to or from a [`Selection`].
    #[error("{0}")]
    Conversion(#[from] SelectionConversionError),
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
    /// Translates input into operations.
    interpreter: Interpreter,
}

impl Processor {
    /// Creates a new [`Processor`].
    pub(crate) fn new(root_dir: &PathUrl) -> Result<Self, Fault> {
        let working_dir = Rc::new(root_dir.clone());

        LogConfig::new()
            .map(|log_config| Self {
                pane: Pane::new(&working_dir),
                input: String::new(),
                log_config: Some(log_config),
                command: None,
                working_dir,
                interpreter: Interpreter::default(),
            })
            .map_err(Fault::from)
    }

    /// Processes `input` and generates [`Output`].
    pub(crate) fn process(&mut self, input: Input) -> Result<Vec<Output<'_>>, Fault> {
        Ok(if let Some(operation) = self.interpreter.translate(input) {
            self.operate(operation)?
        } else {
            Vec::new()
        })
    }

    /// Performs `operation` and returns the appropriate [`Display`]s.
    pub(crate) fn operate(&mut self, operation: Operation) -> Result<Vec<Output<'_>>, Fault> {
        let mut outputs = Vec::new();
        // Retrieve here to avoid error. This will not work once changes start modifying the working dir.
        let working_dir: PathUrl = self.working_dir.as_ref().clone();
        match operation {
            Operation::UpdateSetting(setting) => {
                outputs.append(&mut self.update_setting(setting)?);
            }
            Operation::Size(size) => {
                trace!("resize {:?}", size);
                outputs.push(self.pane.update_size(size));
            }
            Operation::Confirm(action) => {
                outputs.push(Output::Question {
                    request: ShowMessageRequestParams::from(action),
                });
            }
            Operation::Reset => {
                self.input.clear();
                outputs.push(Output::Reset {
                    selection: self
                        .pane
                        .doc
                        .as_ref()
                        .map_or(&EMPTY_SELECTION, |doc| &doc.selection),
                });
            }
            Operation::Alert(message) => {
                outputs.push(Output::Notify { message });
            }
            Operation::StartCommand(command) => {
                let prompt = command.to_string();

                self.command = Some(command);
                outputs.push(Output::StartIntake { title: prompt });
            }
            Operation::Collect(ch) => {
                self.input.push(ch);
                outputs.push(Output::Write { ch });
            }
            Operation::Execute => {
                if self.command.is_some() {
                    let output = self.pane.open_doc(&self.input);

                    self.input.clear();
                    outputs.push(output);
                }
            }
            Operation::Document(doc_op) => {
                outputs.push(self.pane.operate(doc_op)?);
            }
            Operation::Quit => {
                outputs.push(Output::Quit);
            }
            Operation::OpenFile(file) => {
                outputs.push(self.pane.open_doc(&file));
            }
        };

        let mut context = Context::new_with_dir(ArgMatches::new(), &working_dir);

        // config will always be Some after Context::new_with_dir().
        if let Some(mut config) = context.config.config.clone() {
            if let Some(table) = config.as_table_mut() {
                let _ = table.insert("add_newline".to_string(), Value::Boolean(false));

                if let Some(line_break) = table
                    .entry("line_break")
                    .or_insert(Value::Table(Table::new()))
                    .as_table_mut()
                {
                    let _ = line_break.insert("disabled".to_string(), Value::Boolean(true));
                }
            }

            context.config.config = Some(config);
        }

        trace!("config {:?}", context.config.config);

        outputs.push(Output::SetHeader {
            // For now, must deal with fact that StarshipConfig included in Context is very difficult to edit (must edit the TOML Value). Thus for now, the starship.toml config file must be configured correctly.
            header: print::get_prompt(context),
        });
        trace!("output {:?}", outputs);

        Ok(outputs)
    }

    /// Updates `self` based on `setting`.
    fn update_setting(&mut self, setting: Setting) -> Result<Vec<Output<'_>>, Fault> {
        let mut outputs = Vec::new();

        match setting {
            Setting::Wrap(is_wrapped) => {
                trace!("setting wrap to `{}`", is_wrapped);
                outputs.push(Output::Wrap {
                    is_wrapped,
                    selection: self
                        .pane
                        .doc
                        .as_ref()
                        .map_or(&EMPTY_SELECTION, |doc| &doc.selection),
                });
            }
            Setting::StarshipLog(log_level) => {
                trace!("updating starship log level to `{}`", log_level);

                if let Some(log_config) = &self.log_config {
                    log_config.writer()?.starship_level = log_level;
                }
            }
        }

        Ok(outputs)
    }
}

/// A view of the document.
#[derive(Debug, Default)]
struct Pane {
    /// The document in the pane.
    doc: Option<Document>,
    /// The number of lines by which a scroll moves.
    scroll_amount: Rc<RefCell<Amount>>,
    /// The length at which displayed lines may be wrapped.
    // TODO: Remove wrap_length and move functionality into io::ui.
    wrap_length: Rc<RefCell<Swival<usize>>>,
    /// The current working directory.
    working_dir: Rc<PathUrl>,
    /// The [`LspServer`]s managed by the application.
    lsp_servers: HashMap<String, Rc<RefCell<LspServer>>>,
}

impl Pane {
    /// Creates a new [`Pane`].
    fn new(working_dir: &Rc<PathUrl>) -> Self {
        Self {
            doc: None,
            scroll_amount: Rc::new(RefCell::new(Amount(0))),
            wrap_length: Rc::new(RefCell::new(Swival::default())),
            lsp_servers: HashMap::default(),
            working_dir: Rc::clone(working_dir),
        }
    }

    /// Performs `operation` on `self`.
    fn operate(&mut self, operation: DocOp) -> Result<Output<'_>, Fault> {
        Ok(if let Some(doc) = &mut self.doc {
            match operation {
                DocOp::Move(vector) => doc.move_selection(&vector),
                DocOp::Delete => doc.delete_selection()?,
                DocOp::Save => doc.save(),
            }
        } else {
            Output::Notify {
                message: ShowMessageParams {
                    typ: MessageType::Info,
                    message: format!(
                        "There is no open document on which to perform {}",
                        operation
                    ),
                },
            }
        })
    }

    /// Opens a document at `path`.
    fn open_doc(&mut self, path: &str) -> Output<'_> {
        match self.create_doc(path) {
            Ok(doc) => {
                let _ = self.doc.replace(doc);
                #[allow(clippy::option_expect_used)] // Replace guarantees that self.doc is Some.
                self.doc
                    .as_ref()
                    .map(|doc| Output::OpenDoc {
                        url: &doc.path,
                        language_id: &doc.language_id,
                        version: doc.text.version,
                        text: &doc.text.content,
                    })
                    .expect("retrieving `Document` in `Pane`")
            }
            Err(error) => Output::Notify {
                message: ShowMessageParams::from(error),
            },
        }
    }

    /// Creates a [`Document`] from `path`.
    fn create_doc(&mut self, path: &str) -> Result<Document, DocumentError> {
        let doc_path = self.working_dir.join(path)?;
        let language_id = doc_path.language_id();
        let lsp_server = self.lsp_servers.get(language_id).cloned();

        if lsp_server.is_none() {
            if let Some(lsp_server) = LspServer::new(language_id, &self.working_dir.as_ref())?
                .map(|server| Rc::new(RefCell::new(server)))
            {
                let _ = self
                    .lsp_servers
                    .insert(language_id.to_string(), Rc::clone(&lsp_server));
            }
        }

        Document::new(doc_path, &self.wrap_length, lsp_server, &self.scroll_amount)
    }

    /// Updates the size of `self` to match `size`;
    fn update_size(&mut self, size: Size) -> Output<'_> {
        self.wrap_length.borrow_mut().set(size.columns.into());
        self.scroll_amount
            .borrow_mut()
            .set(usize::from(size.rows.wrapping_div(3)));
        Output::Resize { size }
    }
}

/// A file and the user's current interactions with it.
#[derive(Debug)]
struct Document {
    /// The path of the document.
    path: PathUrl,
    /// The language id of the document.
    language_id: String,
    /// The text of the document.
    text: Text,
    /// The current user selection.
    selection: Selection,
    /// The [`LspServer`] associated with the document.
    lsp_server: Option<Rc<RefCell<LspServer>>>,
    /// The number of lines that a scroll will move.
    scroll_amount: Rc<RefCell<Amount>>,
}

impl Document {
    /// Creates a new [`Document`].
    fn new(
        path: PathUrl,
        wrap_length: &Rc<RefCell<Swival<usize>>>,
        lsp_server: Option<Rc<RefCell<LspServer>>>,
        scroll_amount: &Rc<RefCell<Amount>>,
    ) -> Result<Self, DocumentError> {
        let text = Text::new(&path, wrap_length)?;
        let mut selection = Selection::default();

        if !text.is_empty() {
            selection.init();
        }

        if let Some(server) = &lsp_server {
            server
                .borrow_mut()
                .did_open(&path, path.language_id(), text.version, &text.content)?;
        }

        Ok(Self {
            language_id: path.language_id().to_string(),
            path,
            text,
            selection,
            lsp_server,
            scroll_amount: Rc::clone(scroll_amount),
        })
    }

    /// Saves the document.
    fn save(&self) -> Output<'_> {
        let change = self.lsp_server.as_ref().and_then(|server| {
            server
                .borrow_mut()
                .will_save(&self.path)
                .err()
                .map(|e| Output::Notify { message: e.into() })
        });

        change.unwrap_or_else(|| Output::Notify {
            message: match fs::write(&self.path, &self.text.content) {
                Ok(..) => ShowMessageParams {
                    typ: MessageType::Info,
                    message: format!("Saved document `{}`", self.path),
                },
                Err(e) => ShowMessageParams {
                    typ: MessageType::Error,
                    message: format!("Failed to save document `{}`: {}", self.path, e),
                },
            },
        })
    }

    /// Deletes the text of the [`Selection`].
    fn delete_selection(&mut self) -> Result<Output<'_>, Fault> {
        self.text.delete_selection(&self.selection);
        let mut output = Output::EditDoc {
            new_text: String::new(),
            selection: &self.selection,
        };

        if let Some(server) = &self.lsp_server {
            if let Err(e) = server.borrow_mut().did_change(
                &self.path,
                self.text.version,
                &self.text.content,
                TextEdit::new(self.selection.range()?, String::new()),
            ) {
                output = Output::Notify { message: e.into() };
            }
        }

        Ok(output)
    }

    /// Returns the number of lines in `self`.
    fn line_count(&self) -> usize {
        self.text.content.lines().count()
    }

    /// Moves the [`Selection`] as described by [`Vector`].
    fn move_selection(&mut self, vector: &Vector) -> Output<'_> {
        let amount = match vector.magnitude() {
            Magnitude::Single => 1,
            Magnitude::Half => self.scroll_amount.borrow().value(),
        };
        match vector.direction() {
            Direction::Down => {
                self.selection.move_down(amount, self.line_count());
            }
            Direction::Up => {
                self.selection.move_up(amount);
            }
        }

        Output::MoveSelection {
            selection: &self.selection,
        }
    }
}

impl Drop for Document {
    fn drop(&mut self) {
        trace!("dropping {:?}", self.path);
        if let Some(lsp_server) = &self.lsp_server {
            if let Err(e) = lsp_server.borrow_mut().did_close(&self.path) {
                error!(
                    "failed to inform language server process about closing {}",
                    e
                );
            }
        }
    }
}

/// The text of a document.
#[derive(Debug)]
struct Text {
    /// The text.
    content: String,
    /// The length at which the text will wrap.
    wrap_length: Rc<RefCell<Swival<usize>>>,
    /// The version of the text.
    version: i64,
}

impl Text {
    /// Creates a new [`Text`].
    fn new(
        path: &PathUrl,
        wrap_length: &Rc<RefCell<Swival<usize>>>,
    ) -> Result<Self, DocumentError> {
        let content = fs::read_to_string(path.clone()).map_err(|error| match error.kind() {
            ErrorKind::NotFound => DocumentError::NonExistantFile(path.to_string()),
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

    /// Returns if `self` is empty.
    fn is_empty(&self) -> bool {
        self.content.is_empty()
    }

    /// Deletes the text defined by `selection`.
    fn delete_selection(&mut self, selection: &Selection) {
        let mut newline_indices = self.content.match_indices('\n');
        let start_line = selection.start_line();
        if let Some(start_index) = if start_line == 0 {
            Some(0)
        } else {
            newline_indices
                .nth(start_line.saturating_sub(1))
                .map(|index| index.0.saturating_add(1))
        } {
            if let Some((end_index, ..)) = newline_indices.nth(
                selection
                    .end_line()
                    .saturating_sub(start_line.saturating_add(1)),
            ) {
                let _ = self.content.drain(start_index..=end_index);
                self.version = self.version.wrapping_add(1);
            }
        }
    }
}

/// A wrapper around [`u64`].
///
/// Used for storing and modifying within a [`RefCell`].
#[derive(Debug, Default)]
struct Amount(usize);

impl Amount {
    /// Returns the value of `self`.
    const fn value(&self) -> usize {
        self.0
    }

    /// Sets `self` to `amount`.
    fn set(&mut self, amount: usize) {
        self.0 = amount;
    }
}

/// A `SWItch VALue`, which holds a value and if it is enabled.
///
/// Think of it as an [`Option`] where both variants hold a value.
#[derive(Debug, Default)]
struct Swival<T> {
    /// The value.
    value: T,
    /// If the value is enabled.
    is_enabled: bool,
}

impl<T> Swival<T> {
    /// Sets the value of `self` to `value`.
    fn set(&mut self, value: T) {
        self.value = value;
    }
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
    /// Error in the language server.
    #[error("lsp: {0}")]
    Lsp(#[from] lsp::Fault),
    /// Url error.
    #[error("{0}")]
    Url(#[from] UrlError),
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

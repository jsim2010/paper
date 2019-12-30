//! Implements the modality of the application.
use crate::{ui::Config, Failure};
use core::convert::TryFrom;
use displaydoc::Display as DisplayDoc;
use log::{trace, warn};
use lsp_types::{request::{Request, Initialize}, notification::{Initialized, Notification}, ClientCapabilities, InitializeParams, InitializeResult, MessageType, Position, Range, ShowMessageParams, TextEdit, InitializedParams};
use parse_display::Display as ParseDisplay;
use {
    jsonrpc_core::{Call, Id, Params, Version, MethodCall},
    serde::Serialize, 
    std::{
        collections::{HashMap, hash_map::Entry},
        env,
        fmt::Debug,
        fs,
        io::{self, BufRead, BufReader, ErrorKind, Write, Read},
        process::{self, Command, Child, Stdio, ChildStdout},
    },
    serde_json::{self, Value},
};
use url::{ParseError, Url};

/// A [`Range`] specifying the entire document.
const ENTIRE_DOCUMENT: Range = Range{start: Position{line: 0, character: 0}, end: Position{line: u64::max_value(), character: u64::max_value()}};

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

        let file_path = url.clone().to_file_path().map_err(|_| DocumentError::InvalidFilePath())?;
        Ok(Self {
            extension: file_path.extension().map(|ext| ext.to_string_lossy().into_owned()),
            text: fs::read_to_string(file_path)
            .map_err(|error| match error.kind() {
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

/// Represents a language server process.
#[derive(Debug)]
struct LspServer {
    /// The language server process.
    process: Child,
    /// Process the output from the language server.
    reader: BufReader<ChildStdout>,
}

impl LspServer {
    /// Creates a new `LspServer` represented by `process_cmd`.
    fn new(process_cmd: &str) -> Result<Self, Failure> {
        let mut process = Command::new(process_cmd).stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped()).spawn()?;
        Ok(Self {
            reader: BufReader::new(process.stdout.take().ok_or_else(|| Failure::Lsp("Unable to access stdout of language server".to_string()))?),
            process,
        })
    }

    /// Initializes the `LspServer`.
    fn initialize(&mut self) -> Result<(), Failure> {
        self.send_request::<Initialize>(InitializeParams{
            process_id: Some(u64::from(process::id())),
            root_path: None,
            root_uri: Some(Url::from_directory_path(env::current_dir()?.as_path()).map_err(|_| Failure::File(io::Error::new(ErrorKind::Other, "cannot convert current_dir to url")))?),
            initialization_options: None,
            capabilities: ClientCapabilities::default(),
            trace: None,
            workspace_folders: None,
        })?;

        let mut line = String::new();
        let mut blank_line = String::new();

        if self.reader.read_line(&mut line).is_ok() {
            let mut split = line.trim().split(": ");

            if split.next() == Some("Content-Length") && self.reader.read_line(&mut blank_line).is_ok() {
                if let Some(length_str) = split.next() {
                    let mut content = vec![0; length_str.parse()?];

                    if self.reader.read_exact(&mut content).is_ok() {
                        if let Ok(json_string) = String::from_utf8(content) {
                            trace!("received: {}", json_string);
                            if let Ok(message) = serde_json::from_str::<Value>(&json_string) {
                                if let Some(result) = message.get("result") {
                                    if serde_json::from_value::<InitializeResult>(result.to_owned()).is_ok() {
                                        self.send_notification::<Initialized>(InitializedParams {})?;
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        Ok(())
    }

    /// Sends a request with `params` to the language server process.
    fn send_request<T: Request>(&mut self, params: T::Params) -> Result<(), Failure>
    where
        T::Params: Serialize,
    {
        if let Value::Object(params_object) = serde_json::to_value(params)? {
            self.send_message(&Call::MethodCall(MethodCall {
                jsonrpc: Some(Version::V2),
                method: T::METHOD.to_string(),
                params: Params::Map(params_object),
                id: Id::Num(0),
            }))?;
        } else {
            warn!("Request params converted to something other than an object");
        }

        Ok(())
    }

    /// Sends a notification with `params` to the language server process.
    fn send_notification<T: Notification>(&mut self, params: T::Params) -> Result<(), Failure>
    where
        T::Params: Serialize,
    {
        if let Value::Object(params_object) = serde_json::to_value(params)? {
            self.send_message(&Call::Notification(jsonrpc_core::Notification {
                jsonrpc: Some(Version::V2),
                method: T::METHOD.to_string(),
                params: Params::Map(params_object),
            }))?;
        } else {
            warn!("Notification params converted to something other than an object");
        }

        Ok(())
    }

    /// Sends `message` to the language server process.
    fn send_message(&mut self, message: &Call) -> Result<(), Failure>{
        let json_string = serde_json::to_string(message)?;
        trace!("Sending: {}", json_string);

        if let Some(stdin) = self.process.stdin.as_mut() {
            write!(stdin, "Content-Length: {}\r\n\r\n{}", json_string.len(), json_string).unwrap();
        } else {
            warn!("Unable to retrieve stdin of language server processs");
        }

        Ok(())
    }
}

impl Drop for LspServer {
    fn drop(&mut self) {
        if self.process.kill().is_err() {
            warn!("Attempted to kill a language server process that was not running");
        }
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

//! Implements management and use of language servers.
pub(crate) mod utils;

pub(crate) use utils::SendNotificationError;

use {
    crate::io::LanguageId,
    core::{
        cell::{Cell, RefCell},
        convert::{TryFrom, TryInto},
        fmt::{self, Display},
    },
    enum_map::enum_map,
    fehler::{throw, throws},
    jsonrpc_core::Id,
    log::{error, trace, warn},
    lsp_types::{
        notification::{
            DidCloseTextDocument, DidOpenTextDocument, Exit, Initialized, WillSaveTextDocument,
        },
        request::{Initialize, RegisterCapability, Request, Shutdown},
        ClientCapabilities, DidCloseTextDocumentParams, DidOpenTextDocumentParams,
        InitializeParams, InitializeResult, InitializedParams, MessageType, ShowMessageParams,
        SynchronizationCapability, TextDocumentClientCapabilities, TextDocumentIdentifier,
        TextDocumentItem, TextDocumentSaveReason, TextDocumentSyncCapability, TextDocumentSyncKind,
        Url, WillSaveTextDocumentParams,
    },
    market::{
        io::{Reader, Writer},
        ClosedMarketFailure, ConsumeError, Consumer, ProduceError, Producer,
    },
    parse_display::Display as ParseDisplay,
    serde::{de::DeserializeOwned, Serialize},
    serde_json::error::Error as SerdeJsonError,
    std::{
        io,
        process::{self, Child, ChildStderr, ChildStdin, ChildStdout, Command, Stdio},
        rc::Rc,
        sync::{Arc, atomic::{Ordering, AtomicBool}},
        thread::{self, JoinHandle},
    },
    thiserror::Error,
    utils::{LspErrorProcessor, Message, RequestResponseError},
};

/// An error from which the language server was unable to recover.
#[derive(Debug, Error)]
pub enum Fault {
    /// An error from [`utils`].
    ///
    /// [`utils`]: utils/index.html
    #[error("{0}")]
    Util(#[from] utils::Fault),
    /// An error while accessing an IO of the language server process.
    #[error("{0}")]
    Io(#[from] AccessIoError),
    /// An error while spawning a language server process.
    #[error("{0}")]
    Spawn(#[from] SpawnServerError),
    /// An error while waiting for a language server process.
    #[error("unable to wait for language server process exit: {0}")]
    Wait(#[source] io::Error),
    /// An error while killing a language server process.
    #[error("unable to kill language server process: {0}")]
    Kill(#[source] io::Error),
    /// Language server for given language identifier is unknown.
    #[error("language server for `{0}` is unknown")]
    LanguageId(String),
    /// Failed to send notification to language server.
    #[error("failed to send notification message: {0}")]
    SendNotification(#[from] SendNotificationError),
    /// Failed to request response.
    #[error("{0}")]
    Request(#[from] RequestResponseError),
    /// An error while serializing a language server message.
    #[error("unable to serialize language server message: {0}")]
    Serialize(#[from] SerdeJsonError),
    /// Util
    #[error("")]
    Utils(#[from] utils::SendMessageError),
    /// Conversion
    #[error("")]
    Conversion(#[from] TryIntoMessageError),
    /// Normal IO.
    #[error("")]
    NormalIo(#[from] io::Error),
    /// Closed.
    #[error("")]
    Closed(#[from] ClosedMarketFailure),
}

impl From<Fault> for ShowMessageParams {
    #[inline]
    #[must_use]
    fn from(value: Fault) -> Self {
        Self {
            typ: MessageType::Error,
            message: value.to_string(),
        }
    }
}

/// An error creating an LSP client.
#[derive(Debug, Error)]
pub(crate) enum CreateLanguageClientError {
    /// An error spawning the language server.
    #[error(transparent)]
    SpawnServer(#[from] SpawnServerError),
    /// An error accessing an IO of the language server.
    #[error(transparent)]
    Io(#[from] AccessIoError),
    /// An error serializing a language server message.
    #[error("unable to serialize language server message: {0}")]
    Serialize(#[from] SerdeJsonError),
    /// An error initializing the language server.
    #[error("unable to send initialize message to language server: {0}")]
    Initialize(#[from] ProduceError<ClosedMarketFailure>),
}

/// An error while spawning the language server process.
#[derive(Debug, Error)]
#[error("unable to spawn process `{command}`: {error}")]
pub struct SpawnServerError {
    /// The command.
    command: String,
    /// The error.
    #[source]
    error: io::Error,
}

/// An error while accessing the stdio of the language server process.
#[derive(Debug, Error)]
#[error("unable to access {stdio_type} of language server")]
pub struct AccessIoError {
    /// The type of the stdio.
    stdio_type: String,
}

impl From<&str> for AccessIoError {
    #[inline]
    fn from(value: &str) -> Self {
        Self {
            stdio_type: value.to_string(),
        }
    }
}

/// The client interface with a language server.
#[derive(Debug)]
pub(crate) struct LanguageClient {
    /// The language server process.
    pub(crate) server: LangServer,
    /// Transmits messages to the language server process.
    writer: Writer<Message>,
    /// The current request id.
    id: Cell<u64>,
    /// Processes output from the stderr of the language server.
    error_processor: LspErrorProcessor,
    /// Controls settings for the language server.
    settings: Cell<LspSettings>,
    /// Reads messages from the language server process.
    reader: Reader<Message>,
}

impl LanguageClient {
    /// Creates a new `LanguageClient` for `language_id`.
    #[throws(CreateLanguageClientError)]
    pub(crate) fn new(language_id: LanguageId, root: &Url) -> Self {
        let mut server = LangServer::new(language_id)?;
        let writer = Writer::new(server.stdin()?);
        let reader = Reader::new(server.stdout()?);
        let settings = Cell::new(LspSettings::default());

        #[allow(deprecated)] // root_path is a required field.
        writer.produce(Message::request::<Initialize>(
            InitializeParams {
                process_id: Some(u64::from(process::id())),
                root_path: None,
                root_uri: Some(root.clone()),
                initialization_options: None,
                capabilities: ClientCapabilities {
                    workspace: None,
                    text_document: Some(TextDocumentClientCapabilities {
                        synchronization: Some(SynchronizationCapability {
                            dynamic_registration: None,
                            will_save: Some(true),
                            will_save_wait_until: None,
                            did_save: None,
                        }),
                        completion: None,
                        hover: None,
                        signature_help: None,
                        references: None,
                        document_highlight: None,
                        document_symbol: None,
                        formatting: None,
                        range_formatting: None,
                        on_type_formatting: None,
                        declaration: None,
                        definition: None,
                        type_definition: None,
                        implementation: None,
                        code_action: None,
                        code_lens: None,
                        document_link: None,
                        color_provider: None,
                        rename: None,
                        publish_diagnostics: None,
                        folding_range: None,
                    }),
                    window: None,
                    experimental: None,
                },
                trace: None,
                workspace_folders: None,
                client_info: None,
            },
            0,
        )?)?;
        Self {
            // error_processor must be created before server is moved.
            error_processor: LspErrorProcessor::new(server.stderr()?),
            server,
            writer,
            reader,
            settings,
            id: Cell::new(1),
        }
    }

    /// Returns the appropriate request message.
    #[throws(RequestResponseError)]
    fn request<T: Request>(&self, params: T::Params) -> Message
    where
        T::Params: Serialize,
        T::Result: DeserializeOwned + Default,
    {
        let id = self.id.get().wrapping_add(1);
        self.id.set(id);
        Message::request::<T>(params, id)?
    }
}

impl Consumer for LanguageClient {
    type Good = ServerMessage;
    type Failure = ClosedMarketFailure;

    #[throws(ConsumeError<Self::Failure>)]
    fn consume(&self) -> Self::Good {
        let message = self.reader.consume()?;
        trace!("Received LSP message: {}", message);

        match message {
            Message {
                object:
                    utils::Object::Request {
                        id: Some(request_id),
                        ..
                    },
                ..
            } => ServerMessage::Request { id: request_id },
            Message {
                object:
                    utils::Object::Response {
                        outcome: utils::Outcome::Result(value),
                        ..
                    },
                ..
            } => {
                if let Ok(result) = serde_json::from_value::<InitializeResult>(value.clone()) {
                    self.settings.set(LspSettings::from(result));
                    ServerMessage::Initialize
                } else if serde_json::from_value::<()>(value.clone()).is_ok() {
                    ServerMessage::Shutdown
                } else {
                    warn!(
                        "Received unknown response outcome from language client: {}",
                        value
                    );
                    // TODO: Perhaps have a failure thrown here?
                    throw!(ConsumeError::EmptyStock);
                }
            }
            _ => throw!(ConsumeError::EmptyStock),
        }
    }
}

impl Producer for LanguageClient {
    type Good = ClientMessage;
    type Failure = Fault;

    #[throws(ProduceError<Self::Failure>)]
    fn produce(&self, good: Self::Good) {
        if let Some(message) = match &good {
            ClientMessage::Doc(configuration) => {
                match &configuration.message {
                    DocMessage::Open { .. } | DocMessage::Close => {
                        if self.settings.get().notify_open_close {
                            Some(good.clone().try_into().map_err(
                                |error: TryIntoMessageError| ProduceError::Failure(error.into()),
                            )?)
                        } else {
                            None
                        }
                    }
                    DocMessage::Save => {
                        if self.settings.get().notify_save {
                            Some(good.clone().try_into().map_err(
                                |error: TryIntoMessageError| ProduceError::Failure(error.into()),
                            )?)
                        } else {
                            None
                        }
                    }
                }
            }
            ClientMessage::RegisterCapability { .. }
            | ClientMessage::Initialized
            | ClientMessage::Exit => Some(
                good.clone()
                    .try_into()
                    .map_err(|error: TryIntoMessageError| ProduceError::Failure(error.into()))?,
            ),
            ClientMessage::Shutdown => {
                self.error_processor
                    .terminate()
                    .map_err(|error| ProduceError::Failure(error.into()))?;
                Some(
                    self.request::<Shutdown>(())
                        .map_err(|error| ProduceError::Failure(error.into()))?,
                )
            }
        } {
            trace!("Sending LSP message: {}", message);
            self.writer
                .produce(message)
                .map_err(ProduceError::map_into)?
        }
    }
}

/// An error creating a [`LanguageTool`].
#[derive(Debug, Error)]
#[error("unable to create {language_id} language server: {error}")]
pub struct CreateLanguageToolError {
    /// The language identifier of the server.
    language_id: LanguageId,
    /// The error.
    #[source]
    error: CreateLanguageClientError,
}

/// An error editing language client.
#[derive(Debug, Error)]
pub enum EditLanguageToolError {
    /// An error with notification.
    #[error("")]
    Notification(#[from] SendNotificationError),
    /// An error with fault.
    #[error("")]
    Fault(#[from] Fault),
}

impl From<EditLanguageToolError> for ShowMessageParams {
    #[inline]
    #[must_use]
    fn from(value: EditLanguageToolError) -> Self {
        Self {
            typ: MessageType::Error,
            message: value.to_string(),
        }
    }
}

/// Manages the langauge servers.
#[derive(Debug)]
pub(crate) struct LanguageTool {
    drop: Arc<AtomicBool>,
    thread: JoinHandle<()>,
}

impl LanguageTool {
    /// Creates a new [`LanguageTool`].
    #[throws(CreateLanguageToolError)]
    pub(crate) fn new(root_dir: &Url) -> Self {
        let is_dropping = Arc::new(AtomicBool::new(false));
        let dir = root_dir.clone();

        Self {
            drop: Arc::clone(&is_dropping),
            thread: thread::spawn(move || {
                let rust_server = Rc::new(RefCell::new(LanguageClient::new(LanguageId::Rust, &dir).unwrap()));
                let clients = enum_map! {
                    LanguageId::Rust => Rc::clone(&rust_server),
                };
                let mut can_drop = false;

                loop {
                    for (_, client) in &clients {
                        match client.borrow().consume() {
                            Ok(message) => {
                                match message {
                                    ServerMessage::Initialize => client.borrow().produce(ClientMessage::Initialized).unwrap(),
                                    ServerMessage::Request { id } => client.borrow().produce(ClientMessage::RegisterCapability(id)).unwrap(),
                                    ServerMessage::Shutdown => {
                                        // TODO: Update for multiple language clients.
                                        // TODO: Recognize and resolve unexpected shutdown.
                                        can_drop = true;
                                    }
                                }
                            }
                            Err(_) => {}
                        }
                    }

                    if is_dropping.load(Ordering::Relaxed) {
                        for (language_id, client) in &clients {
                            if let Err(error) = client.borrow().produce(ClientMessage::Shutdown) {
                                error!(
                                    "Failed to send shutdown message to {} language server: {}",
                                    language_id, error
                                );
                            }
                        }
                    }

                    if can_drop {
                        for (language_id, client) in &clients {
                            if let Err(error) = client.borrow().produce(ClientMessage::Exit) {
                                error!(
                                    "Failed to send exit message to {} language server: {}",
                                    language_id, error
                                );
                            }

                            if let Err(error) = client.borrow_mut().server.wait() {
                                error!(
                                    "Failed to wait for {} language server process to finish: {}",
                                    language_id, error
                                );
                            }
                        }

                        break;
                    }
                }
            }),
        }
    }
}

impl Consumer for LanguageTool {
    type Good = ToolMessage<ServerMessage>;
    type Failure = Fault;

    #[throws(ConsumeError<Self::Failure>)]
    fn consume(&self) -> Self::Good {
        throw!(ConsumeError::EmptyStock);
    }
}

impl Drop for LanguageTool {
    fn drop(&mut self) {
    }
}

impl Producer for LanguageTool {
    type Good = ToolMessage<ClientMessage>;
    type Failure = ProduceProtocolError;

    #[throws(ProduceError<Self::Failure>)]
    fn produce(&self, _good: Self::Good) {
    }
}

/// A message from the language server.
#[derive(Debug)]
pub(crate) enum ServerMessage {
    /// Initialize.
    Initialize,
    /// Shutdown.
    Shutdown,
    /// Request.
    Request {
        /// Id of the request.
        id: Id,
    },
}

/// Tool message of language server.
#[derive(Clone, Debug, ParseDisplay, PartialEq)]
#[display("{language_id} :: {message}")]
pub(crate) struct ToolMessage<T> {
    /// The URL that generated.
    pub(crate) language_id: LanguageId,
    /// The message.
    pub(crate) message: T,
}

/// Client message to language server.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum ClientMessage {
    /// Shuts down language server.
    Shutdown,
    /// Exits language server.
    Exit,
    /// Initialized.
    Initialized,
    /// Configures a document.
    Doc(DocConfiguration),
    /// Registers a capability.
    RegisterCapability(Id),
}

impl Display for ClientMessage {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}",
            match self {
                Self::Shutdown => "Shutdown".to_string(),
                Self::Exit => "Exit".to_string(),
                Self::Initialized => "Initialized".to_string(),
                Self::Doc(config) => format!("Document configuration {}", config),
                Self::RegisterCapability(id) => format!(
                    "Register capability {}",
                    match id {
                        Id::Null => "null".to_string(),
                        Id::Num(num) => num.to_string(),
                        Id::Str(s) => s.to_string(),
                    }
                ),
            }
        )
    }
}

impl TryFrom<ClientMessage> for Message {
    type Error = TryIntoMessageError;

    #[throws(Self::Error)]
    fn try_from(value: ClientMessage) -> Self {
        match value {
            ClientMessage::Doc(configuration) => match configuration.message {
                DocMessage::Open {
                    language_id,
                    version,
                    text,
                } => Self::notification::<DidOpenTextDocument>(DidOpenTextDocumentParams {
                    text_document: TextDocumentItem::new(
                        configuration.url,
                        language_id.to_string(),
                        version,
                        text,
                    ),
                })?,
                DocMessage::Save => {
                    Self::notification::<WillSaveTextDocument>(WillSaveTextDocumentParams {
                        text_document: TextDocumentIdentifier::new(configuration.url),
                        reason: TextDocumentSaveReason::Manual,
                    })?
                }
                DocMessage::Close => {
                    Self::notification::<DidCloseTextDocument>(DidCloseTextDocumentParams {
                        text_document: TextDocumentIdentifier::new(configuration.url),
                    })?
                }
            },
            ClientMessage::Initialized => Self::notification::<Initialized>(InitializedParams {})?,
            ClientMessage::Exit => Self::notification::<Exit>(())?,
            ClientMessage::RegisterCapability(id) => Self::response::<RegisterCapability>((), id)?,
            ClientMessage::Shutdown => {
                throw!(Self::Error::Null);
            }
        }
    }
}

/// An error converting into message.
#[derive(Debug, Error)]
pub enum TryIntoMessageError {
    /// A null error.
    #[error("")]
    Null,
    /// An error serializing.
    #[error("")]
    Serialize(#[from] SerdeJsonError),
}

/// Configuration of a document.
#[derive(Clone, Debug, ParseDisplay, PartialEq)]
#[display("Configure `{url}` with {message}")]
pub(crate) struct DocConfiguration {
    /// The URL of the doc.
    url: Url,
    /// The message.
    message: DocMessage,
}

impl DocConfiguration {
    /// Creates a new [`DocConfiguration`].
    pub(crate) const fn new(url: Url, message: DocMessage) -> Self {
        Self { url, message }
    }
}

/// A message for interacting with a document.
#[derive(Clone, Debug, ParseDisplay, PartialEq)]
pub(crate) enum DocMessage {
    /// Open a doc.
    #[display("Open document with {language_id} at v{version}")]
    Open {
        /// The language identifier.
        language_id: LanguageId,
        /// The version.
        version: i64,
        /// The text.
        text: String,
    },
    /// Save a doc.
    #[display("Save")]
    Save,
    /// Close a doc.
    #[display("Close")]
    Close,
}

/// An error producing protocol.
#[derive(Debug, Error)]
pub enum ProduceProtocolError {
    /// An error with notification.
    #[error("")]
    Notification(#[from] SendNotificationError),
    /// An error with fault.
    #[error("")]
    Fault(#[from] Fault),
    /// Request.
    #[error("")]
    Request(#[from] RequestResponseError),
    /// Utils.
    #[error("")]
    Utils(#[from] utils::Fault),
}

/// Signifies a language server process.
#[derive(Debug)]
pub(crate) struct LangServer(Child);

impl LangServer {
    /// Creates a new [`LangServer`].
    #[throws(SpawnServerError)]
    fn new(language_id: LanguageId) -> Self {
        Self(
            Command::new(language_id.server_cmd())
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()
                .map_err(|error| SpawnServerError {
                    command: language_id.server_cmd().to_string(),
                    error,
                })?,
        )
    }

    /// Returns the stderr of the process.
    #[throws(AccessIoError)]
    fn stderr(&mut self) -> ChildStderr {
        self.0
            .stderr
            .take()
            .ok_or_else(|| AccessIoError::from("stderr"))?
    }

    /// Returns the stdin of the process.
    #[throws(AccessIoError)]
    fn stdin(&mut self) -> ChildStdin {
        self.0
            .stdin
            .take()
            .ok_or_else(|| AccessIoError::from("stdin"))?
    }

    /// Returns the stdout of the process.
    #[throws(AccessIoError)]
    fn stdout(&mut self) -> ChildStdout {
        self.0
            .stdout
            .take()
            .ok_or_else(|| AccessIoError::from("stdout"))?
    }

    /// Blocks until the proccess ends.
    #[throws(Fault)]
    pub(crate) fn wait(&mut self) {
        self.0.wait().map(|_| ()).map_err(Fault::Wait)?
    }
}

/// Settings of the language server.
#[derive(Clone, Copy, Debug)]
struct LspSettings {
    /// The client should send open and close notifications.
    notify_open_close: bool,
    /// How the client should send change notifications.
    notify_changes_kind: TextDocumentSyncKind,
    /// The client should send save notifications.
    notify_save: bool,
}

impl Default for LspSettings {
    fn default() -> Self {
        Self {
            notify_open_close: false,
            notify_changes_kind: TextDocumentSyncKind::None,
            notify_save: false,
        }
    }
}

impl From<InitializeResult> for LspSettings {
    fn from(value: InitializeResult) -> Self {
        let mut settings = Self::default();

        if let Some(text_document_sync) = value.capabilities.text_document_sync {
            match text_document_sync {
                TextDocumentSyncCapability::Kind(kind) => {
                    if kind != TextDocumentSyncKind::None {
                        settings.notify_open_close = true;
                        settings.notify_changes_kind = kind;
                    }
                }
                TextDocumentSyncCapability::Options(options) => {
                    if let Some(open_close) = options.open_close {
                        settings.notify_open_close = open_close;
                    }

                    if let Some(change) = options.change {
                        settings.notify_changes_kind = change;
                    }

                    if let Some(will_save) = options.will_save {
                        settings.notify_save = will_save;
                    }
                }
            }
        }

        settings
    }
}

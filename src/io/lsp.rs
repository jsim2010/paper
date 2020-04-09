//! Implements management and use of language servers.
pub(crate) mod utils;

pub(crate) use utils::SendNotificationError;

use {
    crate::io::{LanguageId, Purl},
    core::{
        cell::{Cell, RefCell},
        convert::{TryFrom, TryInto},
    },
    enum_map::{enum_map, EnumMap},
    jsonrpc_core::Id,
    log::{trace, warn},
    lsp_types::{
        notification::{
            DidChangeTextDocument, DidCloseTextDocument, DidOpenTextDocument, Exit, Initialized,
            WillSaveTextDocument,
        },
        request::{Initialize, RegisterCapability, Request, Shutdown},
        ClientCapabilities, DidChangeTextDocumentParams, DidCloseTextDocumentParams,
        DidOpenTextDocumentParams, InitializeParams, InitializeResult, InitializedParams,
        MessageType, Range, ShowMessageParams, SynchronizationCapability,
        TextDocumentClientCapabilities, TextDocumentContentChangeEvent, TextDocumentIdentifier,
        TextDocumentItem, TextDocumentSaveReason, TextDocumentSyncCapability, TextDocumentSyncKind,
        Url, VersionedTextDocumentIdentifier, WillSaveTextDocumentParams,
    },
    market::{
        io::{Reader, Writer},
        Consumer, OneShotError, Producer, StripError,
    },
    serde::{de::DeserializeOwned, Serialize},
    serde_json::error::Error as SerdeJsonError,
    std::{
        io,
        process::{self, Child, ChildStderr, ChildStdin, ChildStdout, Command, Stdio},
        rc::Rc,
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
    /// Write.
    #[error("")]
    Write(#[from] StripError<io::Error>),
    /// Normal IO.
    #[error("")]
    NormalIo(#[from] io::Error),
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
pub enum CreateLanguageClientError {
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
    Initialize(#[from] OneShotError<StripError<io::Error>>),
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
    pub(crate) fn new<U>(
        language_id: LanguageId,
        root: U,
    ) -> Result<Self, CreateLanguageClientError>
    where
        U: AsRef<Url>,
    {
        let mut server = LangServer::new(language_id)?;
        let writer = Writer::new(server.stdin()?);
        let reader = Reader::new(server.stdout()?);
        let settings = Cell::new(LspSettings::default());

        #[allow(deprecated)] // root_path is a required field.
        writer.one_shot(Message::request::<Initialize>(
            InitializeParams {
                process_id: Some(u64::from(process::id())),
                root_path: None,
                root_uri: Some(root.as_ref().clone()),
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
        Ok(Self {
            // error_processor must be created before server is moved.
            error_processor: LspErrorProcessor::new(server.stderr()?),
            server,
            writer,
            reader,
            settings,
            id: Cell::new(1),
        })
    }

    /// Returns the appropriate request message.
    fn request<T: Request>(&self, params: T::Params) -> Result<Message, RequestResponseError>
    where
        T::Params: Serialize,
        T::Result: DeserializeOwned + Default,
    {
        let id = self.id.get().wrapping_add(1);
        self.id.set(id);
        Ok(Message::request::<T>(params, id)?)
    }
}

impl Consumer for LanguageClient {
    type Good = ServerMessage;
    type Error = io::Error;

    fn consume(&self) -> Result<Option<Self::Good>, Self::Error> {
        if let Some(message) = self.reader.consume()? {
            trace!("Received LSP message: {}", message);
            match message {
                Message {
                    object:
                        utils::Object::Request {
                            id: Some(request_id),
                            ..
                        },
                    ..
                } => {
                    return Ok(Some(ServerMessage::Request { id: request_id }));
                }
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
                        return Ok(Some(ServerMessage::Initialize));
                    } else if serde_json::from_value::<()>(value.clone()).is_ok() {
                        return Ok(Some(ServerMessage::Shutdown));
                    } else {
                        warn!(
                            "Received unknown response outcome from language client: {}",
                            value
                        );
                    }
                }
                _ => {}
            }
        }

        Ok(None)
    }
}

impl Producer for LanguageClient {
    type Good = ClientMessage;
    type Error = Fault;

    fn produce(&self, good: Self::Good) -> Result<Option<Self::Good>, Self::Error> {
        if let Some(message) = match &good {
            ClientMessage::Doc { url, message } => match message {
                DocMessage::Open { .. } | DocMessage::Close => {
                    if self.settings.get().notify_open_close {
                        Some(good.clone().try_into().map_err(Self::Error::from)?)
                    } else {
                        None
                    }
                }
                DocMessage::Save => {
                    if self.settings.get().notify_save {
                        Some(good.clone().try_into().map_err(Self::Error::from)?)
                    } else {
                        None
                    }
                }
                DocMessage::Change {
                    version,
                    text,
                    range,
                    new_text,
                } => {
                    if let Some(content_changes) = match self.settings.get().notify_changes_kind {
                        TextDocumentSyncKind::None => None,
                        TextDocumentSyncKind::Full => Some(vec![TextDocumentContentChangeEvent {
                            range: None,
                            range_length: None,
                            text: text.to_string(),
                        }]),
                        TextDocumentSyncKind::Incremental => {
                            Some(vec![TextDocumentContentChangeEvent {
                                range: Some(*range),
                                range_length: None,
                                text: new_text.to_string(),
                            }])
                        }
                    } {
                        Some(Message::notification::<DidChangeTextDocument>(
                            DidChangeTextDocumentParams {
                                text_document: VersionedTextDocumentIdentifier::new(
                                    url.clone(),
                                    *version,
                                ),
                                content_changes,
                            },
                        )?)
                    } else {
                        None
                    }
                }
            },
            ClientMessage::RegisterCapability { .. }
            | ClientMessage::Initialized
            | ClientMessage::Exit => Some(good.clone().try_into().map_err(Self::Error::from)?),
            ClientMessage::Shutdown => {
                self.error_processor
                    .terminate()
                    .map_err(Self::Error::from)?;
                Some(self.request::<Shutdown>(()).map_err(Self::Error::from)?)
            }
        } {
            trace!("Sending LSP message: {}", message);
            Ok(self.writer.produce(message)?.map(|_| good))
        } else {
            Ok(None)
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
    /// The clients to servers that have been created by the application.
    // Require Rc due to LanguageClient not impl Copy, see https://gitlab.com/KonradBorowski/enum-map/-/merge_requests/30.
    pub(crate) clients: EnumMap<LanguageId, Rc<RefCell<LanguageClient>>>,
}

impl LanguageTool {
    /// Creates a new [`LanguageTool`].
    pub(crate) fn new(root_dir: &Purl) -> Result<Self, CreateLanguageToolError> {
        let rust_server = Rc::new(RefCell::new(
            LanguageClient::new(LanguageId::Rust, &root_dir).map_err(|error| {
                CreateLanguageToolError {
                    language_id: LanguageId::Rust,
                    error,
                }
            })?,
        ));

        Ok(Self {
            clients: enum_map! {
                LanguageId::Rust => Rc::clone(&rust_server),
            },
        })
    }

    /// Returns the langauge identifiers supported by `self`.
    pub(crate) fn language_ids(&self) -> impl Iterator<Item = LanguageId> + '_ {
        self.clients.iter().map(|(language_id, _)| language_id)
    }
}

impl Consumer for LanguageTool {
    type Good = ToolMessage<ServerMessage>;
    type Error = Fault;

    fn consume(&self) -> Result<Option<Self::Good>, Self::Error> {
        let mut good = None;

        for (language_id, language_client) in &self.clients {
            let client = language_client.borrow();

            if let Some(message) = client.consume()? {
                good = Some(ToolMessage {
                    language_id,
                    message,
                });
                break;
            }
        }

        Ok(good)
    }
}

impl Producer for LanguageTool {
    type Good = ToolMessage<ClientMessage>;
    type Error = ProduceProtocolError;

    fn produce(&self, good: Self::Good) -> Result<Option<Self::Good>, Self::Error> {
        #[allow(clippy::indexing_slicing)] // EnumMap guarantees that index is in bounds.
        Ok(self.clients[good.language_id]
            .borrow()
            .produce(good.message.clone())?
            .map(|_| good))
    }
}

/// A message from the language server.
#[derive(Debug)]
pub enum ServerMessage {
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
#[derive(Clone, Debug, PartialEq)]
pub struct ToolMessage<T> {
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
    Doc {
        /// The URL of the doc.
        url: Url,
        /// The message.
        message: DocMessage,
    },
    /// Registers a capability.
    RegisterCapability {
        /// Id of the request.
        id: Id,
    },
}

impl TryFrom<ClientMessage> for Message {
    type Error = TryIntoMessageError;

    fn try_from(value: ClientMessage) -> Result<Self, Self::Error> {
        Ok(match value {
            ClientMessage::Doc { url, message } => match message {
                DocMessage::Open {
                    language_id,
                    version,
                    text,
                } => Self::notification::<DidOpenTextDocument>(DidOpenTextDocumentParams {
                    text_document: TextDocumentItem::new(
                        url,
                        language_id.to_string(),
                        version,
                        text,
                    ),
                })?,
                DocMessage::Save => {
                    Self::notification::<WillSaveTextDocument>(WillSaveTextDocumentParams {
                        text_document: TextDocumentIdentifier::new(url),
                        reason: TextDocumentSaveReason::Manual,
                    })?
                }
                DocMessage::Close => {
                    Self::notification::<DidCloseTextDocument>(DidCloseTextDocumentParams {
                        text_document: TextDocumentIdentifier::new(url),
                    })?
                }
                DocMessage::Change { .. } => {
                    return Err(Self::Error::Null);
                }
            },
            ClientMessage::Initialized => Self::notification::<Initialized>(InitializedParams {})?,
            ClientMessage::Exit => Self::notification::<Exit>(())?,
            ClientMessage::RegisterCapability { id } => {
                Self::response::<RegisterCapability>((), id)?
            }
            ClientMessage::Shutdown => {
                return Err(Self::Error::Null);
            }
        })
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

/// A message for interacting with a document.
#[allow(dead_code)] // False positive.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum DocMessage {
    /// Open a doc.
    Open {
        /// The language identifier.
        language_id: LanguageId,
        /// The version.
        version: i64,
        /// The text.
        text: String,
    },
    /// Save a doc.
    Save,
    /// Change a doc.
    Change {
        /// The version.
        version: i64,
        /// The text.
        text: String,
        /// The range.
        range: Range,
        /// The new text.
        new_text: String,
    },
    /// Close a doc.
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

impl From<ProduceProtocolError> for ShowMessageParams {
    #[inline]
    #[must_use]
    fn from(value: ProduceProtocolError) -> Self {
        Self {
            typ: MessageType::Error,
            message: value.to_string(),
        }
    }
}

/// Signifies a language server process.
#[derive(Debug)]
pub(crate) struct LangServer(Child);

impl LangServer {
    /// Creates a new [`LangServer`].
    fn new(language_id: LanguageId) -> Result<Self, SpawnServerError> {
        Ok(Self(
            Command::new(language_id.server_cmd())
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()
                .map_err(|error| SpawnServerError {
                    command: language_id.server_cmd().to_string(),
                    error,
                })?,
        ))
    }

    /// Returns the stderr of the process.
    fn stderr(&mut self) -> Result<ChildStderr, AccessIoError> {
        self.0.stderr.take().ok_or_else(|| "stderr".into())
    }

    /// Returns the stdin of the process.
    fn stdin(&mut self) -> Result<ChildStdin, AccessIoError> {
        self.0.stdin.take().ok_or_else(|| "stdin".into())
    }

    /// Returns the stdout of the process.
    fn stdout(&mut self) -> Result<ChildStdout, AccessIoError> {
        self.0.stdout.take().ok_or_else(|| "stdout".into())
    }

    /// Blocks until the proccess ends.
    pub(crate) fn wait(&mut self) -> Result<(), Fault> {
        self.0.wait().map(|_| ()).map_err(Fault::Wait)
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

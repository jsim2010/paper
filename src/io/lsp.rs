//! Implements management and use of language servers.
mod utils;

pub(crate) use utils::SendNotificationError;

use {
    jsonrpc_core::Id,
    crate::io::{LanguageId, PathUrl},
    core::{convert::{TryFrom, TryInto}, cell::{Cell, RefCell}},
    enum_map::{enum_map, EnumMap},
    log::warn,
    lsp_types::{
        notification::{
            DidChangeTextDocument, DidCloseTextDocument, DidOpenTextDocument, Exit, Initialized,
            WillSaveTextDocument,
        },
        request::{RegisterCapability, Request, Initialize, Shutdown},
        ClientCapabilities, DidChangeTextDocumentParams, DidCloseTextDocumentParams,
        DidOpenTextDocumentParams, InitializeParams, InitializeResult, InitializedParams,
        MessageType, Range, ShowMessageParams, SynchronizationCapability,
        TextDocumentClientCapabilities, TextDocumentContentChangeEvent, TextDocumentIdentifier,
        TextDocumentItem, TextDocumentSaveReason, TextDocumentSyncCapability, TextDocumentSyncKind,
        Url, VersionedTextDocumentIdentifier, WillSaveTextDocumentParams,
    },
    market::{Consumer, Writer, Producer},
    std::{
        io,
        process::{self, Child, ChildStderr, ChildStdin, ChildStdout, Command, Stdio},
        rc::Rc,
        sync::mpsc::TryRecvError,
    },
    serde::{de::DeserializeOwned, Serialize},
    serde_json::error::Error as SerdeJsonError,
    thiserror::Error,
    utils::{LspErrorProcessor, Message, LspReceiver, RequestResponseError},
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
    Spawn(#[from] SpawnLangServerError),
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
    /// Write
    #[error("")]
    Write(#[from] market::WriteGoodError<utils::Fault>),
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

/// Failed to create language server client.
#[derive(Debug, Error)]
pub enum CreateLangClientError {
    /// Failed to spawn server.
    #[error("{0}")]
    SpawnServer(#[from] SpawnLangServerError),
    /// Failed to access IO.
    #[error("{0}")]
    Io(#[from] AccessIoError),
    /// Failed to initialize language server.
    #[error("failed to initialize language server: {0}")]
    Init(#[from] RequestResponseError),
    /// Failed to notify language server.
    #[error("failed to notify language server of initialization: {0}")]
    NotifyInit(#[from] SendNotificationError),
    /// An error while serializing a language server message.
    #[error("unable to serialize language server message: {0}")]
    Serialize(#[from] SerdeJsonError),
    /// Write
    #[error("")]
    Write(#[from] market::WriteGoodError<utils::Fault>),
}

/// An error while spawning the language server process.
#[derive(Debug, Error)]
#[error("failed to spawn language server `{command}`: {error}")]
pub struct SpawnLangServerError {
    /// The command.
    command: String,
    /// The error.
    #[source]
    error: io::Error,
}

/// An error while accessing the stdio of the language server process.
#[derive(Debug, Error)]
#[error("failed to access {stdio_type} of language server")]
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
    writer: Writer<Message, ChildStdin>,
    /// The current request id.
    id: Cell<u64>,
    /// Processes output from the stderr of the language server.
    error_processor: LspErrorProcessor,
    /// Controls settings for the language server.
    settings: Cell<LspSettings>,
    /// Receives messages from the language server.
    receiver: LspReceiver,
}

impl LanguageClient {
    /// Creates a new `LanguageClient` for `language_id`.
    pub(crate) fn new<U>(language_id: LanguageId, root: U) -> Result<Self, CreateLangClientError>
    where
        U: AsRef<Url>,
    {
        let mut server = LangServer::new(language_id)?;
        let writer = Writer::new(server.stdin()?);
        let receiver = LspReceiver::new(server.stdout()?);
        let capabilities = ClientCapabilities {
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
        };
        let settings = Cell::new(LspSettings::default());

        #[allow(deprecated)] // root_path is a required field.
        writer.produce(Message::request::<Initialize>(
                InitializeParams {
                    process_id: Some(u64::from(process::id())),
                    root_path: None,
                    root_uri: Some(root.as_ref().clone()),
                    initialization_options: None,
                    capabilities,
                    trace: None,
                    workspace_folders: None,
                    client_info: None,
                },
                0,
            )?)?;
        let client = Self {
            // error_processor must be created before server is moved.
            error_processor: LspErrorProcessor::new(server.stderr()?),
            server,
            writer,
            settings,
            receiver,
            id: Cell::new(1),
        };

        Ok(client)
    }

    /// Returns the appropriate request message.
    fn request<T: Request>(
        &self,
        params: T::Params,
    ) -> Result<Message, RequestResponseError>
    where
        T::Params: Serialize,
        T::Result: DeserializeOwned + Default,
    {
        let id = self.id.get().wrapping_add(1);
        self.id.set(id);
        Ok(Message::request::<T>(
            params,
            id,
        )?)
    }
}

impl Consumer for LanguageClient {
    type Good = ServerMessage;
    type Error = TryRecvError;

    fn consume(&self) -> Option<Result<Self::Good, Self::Error>> {
        match self.receiver.recv() {
            Ok(Message{object: utils::Object::Request{id: Some(Id::Num(id_num)), ..}, ..}) => {
                return Some(Ok(ServerMessage::Request{id: id_num}));
            }
            Ok(Message{object: utils::Object::Response{outcome: utils::Outcome::Result(value), ..}, ..}) => {
                if let Ok(result) = serde_json::from_value::<InitializeResult>(value.clone()) {
                    self.settings.set(LspSettings::from(result));
                    warn!("Settings: {:?}", self.settings);
                    return Some(Ok(ServerMessage::Initialize));
                } else if serde_json::from_value::<()>(value.clone()).is_ok() {
                    warn!("Shutdown result");
                    return Some(Ok(ServerMessage::Shutdown));
                } else {
                    warn!("Received unknown message from language client");
                }
            }
            Err(error) => {
                return Some(Err(error));
            }
            Ok(_) => {}
        }

        None
    }
}

impl Producer<'_> for LanguageClient {
    type Good = ClientMessage;
    type Error = Fault;

    fn produce(&self, good: Self::Good) -> Result<(), Self::Error> {
        if let Some(message) = match &good {
            ClientMessage::Doc{ url, message} => match message.as_ref() {
                DocMessage::Open { .. } | DocMessage::Close => {
                    if self.settings.get().notify_open_close {
                        Some(good.try_into()?)
                    } else {
                        None
                    }
                }
                DocMessage::Save => {
                    if self.settings.get().notify_save {
                        Some(good.try_into()?)
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
                    let uri: &Url = url.as_ref();

                    if let Some(content_changes) = match self.settings.get().notify_changes_kind {
                        TextDocumentSyncKind::None => None,
                        TextDocumentSyncKind::Full => Some(vec![TextDocumentContentChangeEvent {
                            range: None,
                            range_length: None,
                            text: text.to_string(),
                        }]),
                        TextDocumentSyncKind::Incremental => Some(vec![TextDocumentContentChangeEvent {
                            range: Some(*range),
                            range_length: None,
                            text: new_text.to_string(),
                        }]),
                    } {
                        Some(Message::notification::<DidChangeTextDocument>(
                            DidChangeTextDocumentParams {
                                text_document: VersionedTextDocumentIdentifier::new(
                                    uri.clone(),
                                    *version,
                                ),
                                content_changes,
                            })?,
                        )
                    } else {
                        None
                    }
                }
            }
            ClientMessage::RegisterCapability{..} | ClientMessage::Initialized | ClientMessage::Exit => {
                Some(good.try_into()?)
            }
            ClientMessage::Shutdown => {
                self.error_processor.terminate()?;
                Some(self.request::<Shutdown>(())?)
            }
        } {
            Ok(self.writer.produce(message)?)
        } else {
            Ok(())
        }
    }
}

/// An error creating client.
#[derive(Debug, Error)]
pub enum CreateLanguageToolError {
    /// Server.
    #[error("")]
    Server(#[from] CreateLangClientError),
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
    pub(crate) fn new(root_dir: &PathUrl) -> Result<Self, CreateLanguageToolError> {
        let rust_server = Rc::new(RefCell::new(LanguageClient::new(LanguageId::Rust, &root_dir)?));

        Ok(Self {
            clients: enum_map! {
                LanguageId::Rust => Rc::clone(&rust_server),
            },
        })
    }

    /// Returns the langauge identifiers supported by `self`.
    pub(crate) fn language_ids<'a>(&'a self) -> impl Iterator<Item=LanguageId> + 'a {
        self.clients.iter().map(|(language_id, _)| language_id)
    }
}

impl Consumer for LanguageTool {
    type Good = ToolMessage<ServerMessage>;
    type Error = TryRecvError;
    
    fn consume(&self) -> Option<Result<Self::Good, Self::Error>> {
        for (language_id, client) in &self.clients {
            if let Some(consumable) = client.borrow().consume() {
                return Some(consumable.map(|message| ToolMessage{language_id, message}));
            }
        }

        None
    }
}

impl Producer<'_> for LanguageTool {
    type Good = ToolMessage<ClientMessage>;
    type Error = ProduceProtocolError;

    fn produce(&self, good: Self::Good) -> Result<(), Self::Error> {
        #[allow(clippy::indexing_slicing)] // enum_map ensures indexing will not fail.
        self.clients[good.language_id].borrow().produce(good.message)?;
        Ok(())
    }
}

/// A message from the language server.
pub(crate) enum ServerMessage {
    /// Initialize.
    Initialize,
    /// Shutdown.
    Shutdown,
    /// Request.
    Request{
        /// Id of the request.
        id: u64,
    },
}

/// Tool message of language server.
pub(crate) struct ToolMessage<T> {
    /// The URL that generated.
    pub(crate) language_id: LanguageId,
    /// The message.
    pub(crate) message: T,
}

/// Client message to language server.
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
        url: PathUrl,
        /// The message.
        message: Box<DocMessage>,
    },
    /// Registers a capability.
    RegisterCapability {
        /// Id of the request.
        id: u64
    },
}

impl TryFrom<ClientMessage> for Message {
    type Error = TryIntoMessageError;

    fn try_from(value: ClientMessage) -> Result<Self, Self::Error> {
        Ok(match value {
            ClientMessage::Doc{url, message} => match message.as_ref() {
                DocMessage::Open{language_id, version, text} => {
                    let uri: &Url = url.as_ref();
                    Self::notification::<DidOpenTextDocument>(
                        DidOpenTextDocumentParams {
                            text_document: TextDocumentItem::new(
                                uri.clone(),
                                language_id.to_string(),
                                *version,
                                text.to_string(),
                            ),
                        })?
                }
                DocMessage::Save => {
                    let uri: &Url = url.as_ref();
                    Self::notification::<WillSaveTextDocument>(
                        WillSaveTextDocumentParams {
                            text_document: TextDocumentIdentifier::new(uri.clone()),
                            reason: TextDocumentSaveReason::Manual,
                        })?
                }
                DocMessage::Close => {
                    let uri: &Url = url.as_ref();
                    Self::notification::<DidCloseTextDocument>(
                        DidCloseTextDocumentParams {
                            text_document: TextDocumentIdentifier::new(uri.clone()),
                        }
                    )?
                }
                DocMessage::Change { .. } => {
                    return Err(Self::Error::Null);
                }
            }
            ClientMessage::Initialized => {
                Self::notification::<Initialized>(InitializedParams{})?
            }
            ClientMessage::Exit => {
                Self::notification::<Exit>(())?
            }
            ClientMessage::RegisterCapability{id} => {
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
    fn new(language_id: LanguageId) -> Result<Self, SpawnLangServerError> {
        Ok(Self(
            Command::new(language_id.server_cmd())
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()
                .map_err(|error| SpawnLangServerError {
                    command: language_id.to_string(),
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

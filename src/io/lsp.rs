//! Implements management and use of language servers.
mod utils;

pub(crate) use utils::SendNotificationError;

use {
    crate::io::{LanguageId, PathUrl},
    core::{convert::{TryFrom, TryInto}, cell::{Cell, RefCell}},
    enum_map::{enum_map, EnumMap},
    log::warn,
    lsp_types::{
        notification::{
            DidChangeTextDocument, DidCloseTextDocument, DidOpenTextDocument, Exit, Initialized,
            WillSaveTextDocument, Notification,
        },
        request::{RegisterCapability, Initialize, Shutdown},
        ClientCapabilities, DidChangeTextDocumentParams, DidCloseTextDocumentParams,
        DidOpenTextDocumentParams, InitializeParams, InitializeResult, InitializedParams,
        MessageType, Range, ShowMessageParams, SynchronizationCapability,
        TextDocumentClientCapabilities, TextDocumentContentChangeEvent, TextDocumentIdentifier,
        TextDocumentItem, TextDocumentSaveReason, TextDocumentSyncCapability, TextDocumentSyncKind,
        TextEdit, Url, VersionedTextDocumentIdentifier, WillSaveTextDocumentParams,
    },
    market::{Consumer, Producer},
    std::{
        io,
        process::{self, Child, ChildStderr, ChildStdin, ChildStdout, Command, Stdio},
        rc::Rc,
    },
    serde::{de::DeserializeOwned, Serialize},
    serde_json::error::Error as SerdeJsonError,
    thiserror::Error,
    utils::{LspErrorProcessor, Message, LspReceiver, LspTransmitter, RequestResponseError},
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
    transmitter: RefCell<LspTransmitter>,
    id: Cell<u64>,
    /// Processes output from the stderr of the language server.
    error_processor: LspErrorProcessor,
    /// Controls settings for the language server.
    settings: LspSettings,
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
        let transmitter = LspTransmitter::new(server.stdin()?);
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
        let settings = LspSettings::default();
        let client = Self {
            // error_processor must be created before server is moved.
            error_processor: LspErrorProcessor::new(server.stderr()?),
            server,
            transmitter: RefCell::new(transmitter),
            settings,
            receiver,
            id: Cell::new(0),
        };

        #[allow(deprecated)] // root_path is a required field.
        client.request::<Initialize>(
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
        )?;

        Ok(client)
    }

    pub(crate) fn notify<T: Notification>(
        &self,
        params: T::Params,
    ) -> Result<(), SendNotificationError>
    where
        T::Params: Serialize,
    {
        self.transmitter.borrow_mut()
            .send(&utils::Message::Notification {
                method: T::METHOD,
                params: serde_json::to_value(params)?,
            })
            .map_err(|e| e.into())
    }

    /// Sends a response with `id` and `result`.
    pub(crate) fn respond<T: lsp_types::request::Request>(
        &self,
        id: u64,
        result: T::Result,
    ) -> Result<(), Fault>
    where
        T::Result: Serialize,
    {
        self.transmitter.borrow_mut()
            .send(&utils::Message::Response {
                id,
                outcome: utils::Outcome::Success(serde_json::to_value(result)?),
            })
            .map_err(|e| e.into())
    }

    /// Sends `request` to the lsp server.
    pub(crate) fn request<T: lsp_types::request::Request>(
        &self,
        params: T::Params,
    ) -> Result<(), RequestResponseError>
    where
        T::Params: Serialize,
        T::Result: DeserializeOwned + Default,
    {
        let mut id = self.id.get();
        id += 1;
        self.transmitter.borrow_mut().send(&utils::Message::Request(utils::MessageRequest {
            id,
            method: T::METHOD.to_string(),
            params: serde_json::to_value(params)?,
        }))?;
        self.id.set(id);
        Ok(())
    }

    /// Sends the didChange notification, if appropriate.
    pub(crate) fn did_change<U>(
        &self,
        uri: U,
        version: i64,
        text: &str,
        edit: TextEdit,
    ) -> Result<(), Fault>
    where
        U: AsRef<Url>,
    {
        if let Some(content_changes) = match self.settings.notify_changes_kind {
            TextDocumentSyncKind::None => None,
            TextDocumentSyncKind::Full => Some(vec![TextDocumentContentChangeEvent {
                range: None,
                range_length: None,
                text: text.to_string(),
            }]),
            TextDocumentSyncKind::Incremental => Some(vec![TextDocumentContentChangeEvent {
                range: Some(edit.range),
                range_length: None,
                text: edit.new_text,
            }]),
        } {
            self
                .notify::<DidChangeTextDocument>(DidChangeTextDocumentParams {
                    text_document: VersionedTextDocumentIdentifier::new(
                        uri.as_ref().clone(),
                        version,
                    ),
                    content_changes,
                })?;
        }

        Ok(())
    }

    /// Sends the didClose notification, if appropriate.
    pub(crate) fn did_close<U>(&self, uri: U) -> Result<(), Fault>
    where
        U: AsRef<Url>,
    {
        if self.settings.notify_open_close {
            self
                .notify::<DidCloseTextDocument>(DidCloseTextDocumentParams {
                    text_document: TextDocumentIdentifier::new(uri.as_ref().clone()),
                })?;
        }

        Ok(())
    }

    pub(crate) fn register_capability(&self, id: u64) -> Result<(), Fault> {
        Ok(self.respond::<RegisterCapability>(id, ())?)
    }
}

impl Producer<'_> for LanguageClient {
    type Good = ClientMessage;
    type Error = Fault;

    fn produce(&self, good: Self::Good) -> Result<(), Self::Error> {
        match &good {
            ClientMessage::Doc{ url, message} => match message {
                DocMessage::Open { .. } => {
                    if self.settings.notify_open_close {
                        self.transmitter.borrow_mut().send(&good.try_into()?)?;
                    }
                }
                DocMessage::Save => {
                    if self.settings.notify_save {
                        self.transmitter.borrow_mut().send(&good.try_into()?)?;
                    }
                }
                DocMessage::Change {
                    version,
                    text,
                    range,
                    new_text,
                } => {
                    self.did_change(url, *version, &text, TextEdit::new(*range, new_text.to_string()))?;
                }
                DocMessage::Close => {
                    self.did_close(url)?;
                }
            }
            ClientMessage::RegisterCapability{id} => {
                self.register_capability(*id)?;
            }
            ClientMessage::Initialized => {
                self.notify::<Initialized>(InitializedParams {})?;
            }
            ClientMessage::Shutdown => {
                self.request::<Shutdown>(())?;
                self.error_processor.terminate()?;
            }
            ClientMessage::Exit => {
                self.notify::<Exit>(())?;
            }
        }

        Ok(())
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

    pub(crate) fn language_ids<'a>(&'a self) -> impl Iterator<Item=LanguageId> + 'a {
        self.clients.iter().map(|(language_id, _)| language_id)
    }
}

impl Consumer for LanguageTool {
    type Good = ToolMessage<ServerMessage>;
    type Error = ConsumeInputError;
    
    fn consume(&self) -> Option<Result<Self::Good, Self::Error>> {
        for (language_id, server) in &self.clients {
            let mut server = server.borrow_mut();

            match server.receiver.recv() {
                Ok(utils::Message::Request(utils::MessageRequest{id, ..})) => {
                    return Some(Ok(ToolMessage{language_id, message: ServerMessage::Request{id}}));
                }
                Ok(utils::Message::Response{outcome: utils::Outcome::Success(value), ..}) => {
                    if let Ok(result) = serde_json::from_value::<InitializeResult>(value.clone()) {
                        server.settings = LspSettings::from(result);
                        warn!("Settings: {:?}", server.settings);
                        return Some(Ok(ToolMessage{language_id, message: ServerMessage::Initialize}));
                    } else if serde_json::from_value::<()>(value.clone()).is_ok() {
                        warn!("Shutdown result");
                        return Some(Ok(ToolMessage{language_id, message: ServerMessage::Shutdown}));
                    }
                }
                Ok(_) | Err(_) => {}
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

pub(crate) enum ServerMessage {
    Initialize,
    Shutdown,
    Request{id: u64},
}

/// An error consuming lsp input.
#[derive(Clone, Copy, Debug, Error)]
#[error("")]
pub enum ConsumeInputError {
}

/// ToolMessage of language server.
pub(crate) struct ToolMessage<T> {
    /// The URL that generated.
    pub(crate) language_id: LanguageId,
    /// The message.
    pub(crate) message: T,
}

/// ClientMessage to language server.
pub(crate) enum ClientMessage {
    Shutdown,
    Exit,
    Initialized,
    Doc {url: PathUrl, message: DocMessage},
    RegisterCapability {id: u64},
}

impl TryFrom<ClientMessage> for Message {
    type Error = TryIntoMessageError;

    fn try_from(value: ClientMessage) -> Result<Self, Self::Error> {
        Ok(match value {
            ClientMessage::Doc{url, message} => match message {
                DocMessage::Open{language_id, version, text} => {
                    let uri: &Url = url.as_ref();
                    Message::Notification {
                        method: DidOpenTextDocument::METHOD,
                        params: serde_json::to_value(DidOpenTextDocumentParams {
                            text_document: TextDocumentItem::new(
                                uri.clone(),
                                language_id.to_string(),
                                version,
                                text.to_string(),
                            ),
                        })?,
                    }
                }
                DocMessage::Save => {
                    let uri: &Url = url.as_ref();
                    Message::Notification {
                        method: WillSaveTextDocument::METHOD,
                        params: serde_json::to_value(WillSaveTextDocumentParams {
                            text_document: TextDocumentIdentifier::new(uri.clone()),
                            reason: TextDocumentSaveReason::Manual,
                        })?,
                    }
                }
                _ => Err(Self::Error::Null)?,
            }
            _ => Err(Self::Error::Null)?,
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

#[allow(dead_code)] // False positive.
pub(crate) enum DocMessage {
    /// Open a doc.
    Open {
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
#[derive(Debug)]
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

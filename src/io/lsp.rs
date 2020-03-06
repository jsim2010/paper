//! Implements management and use of language servers.
mod utils;

pub(crate) use utils::SendNotificationError;

use {
    crate::io::{LanguageId, PathUrl},
    core::cell::RefCell,
    enum_map::{enum_map, EnumMap},
    log::warn,
    lsp_types::{
        notification::{
            DidChangeTextDocument, DidCloseTextDocument, DidOpenTextDocument, Exit, Initialized,
            WillSaveTextDocument,
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
    thiserror::Error,
    utils::{LspErrorProcessor, LspReceiver, LspTransmitter, RequestResponseError},
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

/// Represents a language server process.
#[derive(Debug)]
pub(crate) struct LspServer {
    /// The language server process.
    pub(crate) server: LangServer,
    /// Transmits messages to the language server process.
    transmitter: LspTransmitter,
    /// Processes output from the stderr of the language server.
    error_processor: LspErrorProcessor,
    /// Controls settings for the language server.
    settings: LspSettings,
    /// Receives messages from the language server.
    receiver: LspReceiver,
}

impl LspServer {
    /// Creates a new `LspServer` for `language_id`.
    pub(crate) fn new<U>(language_id: LanguageId, root: U) -> Result<Self, CreateLangClientError>
    where
        U: AsRef<Url>,
    {
        let mut server = LangServer::new(language_id)?;
        let mut transmitter = LspTransmitter::new(server.stdin()?);
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

        #[allow(deprecated)] // root_path is a required field.
        transmitter.request::<Initialize>(
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

        Ok(Self {
            // error_processor must be created before server is moved.
            error_processor: LspErrorProcessor::new(server.stderr()?),
            server,
            transmitter,
            settings,
            receiver,
        })
    }

    /// Sends the didOpen notification, if appropriate.
    pub(crate) fn did_open<U>(
        &mut self,
        uri: U,
        language_id: &str,
        version: i64,
        text: &str,
    ) -> Result<(), SendNotificationError>
    where
        U: AsRef<Url>,
    {
        if self.settings.notify_open_close {
            self.transmitter
                .notify::<DidOpenTextDocument>(DidOpenTextDocumentParams {
                    text_document: TextDocumentItem::new(
                        uri.as_ref().clone(),
                        language_id.to_string(),
                        version,
                        text.to_string(),
                    ),
                })?;
        }

        Ok(())
    }

    /// Sends the didChange notification, if appropriate.
    pub(crate) fn did_change<U>(
        &mut self,
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
            self.transmitter
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

    /// Sends the willSave notification, if appropriate.
    pub(crate) fn will_save<U>(&mut self, uri: U) -> Result<(), Fault>
    where
        U: AsRef<Url>,
    {
        if self.settings.notify_save {
            self.transmitter
                .notify::<WillSaveTextDocument>(WillSaveTextDocumentParams {
                    text_document: TextDocumentIdentifier::new(uri.as_ref().clone()),
                    reason: TextDocumentSaveReason::Manual,
                })?;
        }

        Ok(())
    }

    /// Sends the didClose notification, if appropriate.
    pub(crate) fn did_close<U>(&mut self, uri: U) -> Result<(), Fault>
    where
        U: AsRef<Url>,
    {
        if self.settings.notify_open_close {
            self.transmitter
                .notify::<DidCloseTextDocument>(DidCloseTextDocumentParams {
                    text_document: TextDocumentIdentifier::new(uri.as_ref().clone()),
                })?;
        }

        Ok(())
    }

    pub(crate) fn register_capability(&mut self, id: u64) -> Result<(), Fault> {
        Ok(self.transmitter.respond::<RegisterCapability>(id, ())?)
    }
}

/// An error creating client.
#[derive(Debug, Error)]
pub enum CreateLanguageClientError {
    /// Server.
    #[error("")]
    Server(#[from] CreateLangClientError),
}

/// An error editing language client.
#[derive(Debug, Error)]
pub enum EditLanguageClientError {
    /// An error with notification.
    #[error("")]
    Notification(#[from] SendNotificationError),
    /// An error with fault.
    #[error("")]
    Fault(#[from] Fault),
}

impl From<EditLanguageClientError> for ShowMessageParams {
    #[inline]
    #[must_use]
    fn from(value: EditLanguageClientError) -> Self {
        Self {
            typ: MessageType::Error,
            message: value.to_string(),
        }
    }
}

/// Manages the langauge servers.
#[derive(Debug)]
pub(crate) struct LanguageClient {
    /// The servers that have been created by the application.
    // Require Rc due to LspServer not impl Copy, see https://gitlab.com/KonradBorowski/enum-map/-/merge_requests/30.
    pub(crate) servers: EnumMap<LanguageId, Rc<RefCell<LspServer>>>,
}

impl LanguageClient {
    /// Creates a new [`LanguageClient`].
    pub(crate) fn new(root_dir: &PathUrl) -> Result<Self, CreateLanguageClientError> {
        let rust_server = Rc::new(RefCell::new(LspServer::new(LanguageId::Rust, &root_dir)?));

        Ok(Self {
            servers: enum_map! {
                LanguageId::Rust => Rc::clone(&rust_server),
            },
        })
    }

    pub(crate) fn language_ids<'a>(&'a self) -> impl Iterator<Item=LanguageId> + 'a {
        self.servers.iter().map(|(language_id, _)| language_id)
    }
}

impl Consumer for LanguageClient {
    type Good = Input;
    type Error = ConsumeInputError;
    
    fn consume(&self) -> Option<Result<Self::Good, Self::Error>> {
        for (language_id, server) in &self.servers {
            let mut server = server.borrow_mut();

            match server.receiver.recv() {
                Ok(utils::Message::Request(utils::MessageRequest{id, ..})) => {
                    return Some(Ok(Input{language_id, reception: Reception::Request{id}}));
                }
                Ok(utils::Message::Response{outcome: utils::Outcome::Success(value), ..}) => {
                    if let Ok(result) = serde_json::from_value::<InitializeResult>(value.clone()) {
                        server.settings = LspSettings::from(result);
                        warn!("Settings: {:?}", server.settings);
                        return Some(Ok(Input{language_id, reception: Reception::Initialize}));
                    } else if serde_json::from_value::<()>(value.clone()).is_ok() {
                        warn!("Shutdown result");
                        return Some(Ok(Input{language_id, reception: Reception::Shutdown}));
                    }
                }
                Ok(_) | Err(_) => {}
            }
        }

        None
    }
}

impl Producer<'_> for LanguageClient {
    type Good = Protocol;
    type Error = ProduceProtocolError;

    fn produce(&self, good: Self::Good) -> Result<(), Self::Error> {
        #[allow(clippy::indexing_slicing)] // enum_map ensures indexing will not fail.
        let mut server = self.servers[good.language_id].borrow_mut();

        match good.message {
            Message::Doc{ url, message} => {
                match message {
                    DocMessage::Open { version, text } => {
                        server.did_open(url, &good.language_id.to_string(), version, &text)?;
                    }
                    DocMessage::Save => {
                        server.will_save(url)?;
                    }
                    DocMessage::Change {
                        version,
                        text,
                        range,
                        new_text,
                    } => {
                        server.did_change(url, version, &text, TextEdit::new(range, new_text))?;
                    }
                    DocMessage::Close => {
                        server.did_close(url)?;
                    }
                }
            }
            Message::RegisterCapability{id} => {
                server.register_capability(id)?;
            }
            Message::Initialized => {
                server.transmitter.notify::<Initialized>(InitializedParams {})?;
            }
            Message::Shutdown => {
                server.transmitter.request::<Shutdown>(())?;
                server.error_processor.terminate()?;
            }
            Message::Exit => {
                server.transmitter.notify::<Exit>(())?;
            }
        }

        Ok(())
    }
}

pub(crate) struct Input {
    pub(crate) language_id: LanguageId,
    pub(crate) reception: Reception,
}

pub(crate) enum Reception {
    Initialize,
    Shutdown,
    Request{id: u64},
}

/// An error consuming lsp input.
#[derive(Clone, Copy, Debug, Error)]
#[error("")]
pub enum ConsumeInputError {
}

/// Protocol of language server.
pub(crate) struct Protocol {
    /// The URL that generated.
    pub(crate) language_id: LanguageId,
    /// The message.
    pub(crate) message: Message,
}

/// Message to language server.
pub(crate) enum Message {
    Shutdown,
    Exit,
    Initialized,
    Doc {url: PathUrl, message: DocMessage},
    RegisterCapability {id: u64},
}

#[allow(dead_code)] // False positive.
pub(crate) enum DocMessage {
    /// Open a doc.
    Open {
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

//! Implements management and use of language servers.
use {
    core::cell::{Cell, RefCell},
    docuglot::{ClientResponse, ClientRequest, ClientMessage, Language, ServerResponse, ServerMessage, CreateClientError, Client},
    enum_map::enum_map,
    fehler::{throw, throws},
    log::error,
    lsp_types::{
        InitializeResult, MessageType, ShowMessageParams,
        TextDocumentSyncCapability, TextDocumentSyncKind,
        Url,
    },
    market::{
        ClosedMarketFailure, ConsumeError, Consumer, ProduceError, Producer,
    },
    parse_display::Display as ParseDisplay,
    serde_json::error::Error as SerdeJsonError,
    std::{
        io,
        rc::Rc,
        sync::{
            atomic::{AtomicBool, Ordering},
            Arc,
        },
        thread::{self, JoinHandle},
    },
    thiserror::Error,
};

/// An error from which the language server was unable to recover.
#[derive(Debug, Error)]
pub enum Fault {
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
    /// Language server for given language is unknown.
    #[error("language server for `{0}` is unknown")]
    Language(String),
    /// An error while serializing a language server message.
    #[error("unable to serialize language server message: {0}")]
    Serialize(#[from] SerdeJsonError),
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
    /// Controls settings for the language server.
    settings: Cell<LspSettings>,
}

impl LanguageClient {
    ///// Creates a new `LanguageClient` for `language_id`.
    //#[throws(CreateLanguageClientError)]
    //pub(crate) fn new(language: Language, root: &Url) -> Self {
    //    let settings = Cell::new(LspSettings::default());
    //}
}

/// An error creating a [`LanguageTool`].
#[derive(Debug, Error)]
#[error("unable to create {language} language server: {error}")]
pub struct CreateLanguageToolError {
    /// The language of the server.
    language: Language,
    /// The error.
    #[source]
    error: CreateLanguageClientError,
}

/// An error editing language client.
#[derive(Debug, Error)]
pub enum EditLanguageToolError {
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

/// An error within the Language Tool thread.
#[derive(Debug, Error)]
enum LanguageToolError {
    /// An error creating the language client.
    #[error(transparent)]
    Create(#[from] CreateClientError),
}

/// Runs the thread of the language tool.
#[throws(LanguageToolError)]
fn thread(root_dir: &Url, is_dropping: &Arc<AtomicBool>) {
    let rust_server = Rc::new(RefCell::new(Client::new(
        Language::Rust,
        root_dir,
    )?));
    let clients = enum_map! {
        Language::Rust => Rc::clone(&rust_server),
    };
    let mut is_shutdown = false;

    while !is_shutdown {
        for (_, client) in &clients {
            let lang_client = client.borrow();

            match lang_client.consume() {
                Ok(message) => {
                    if let Err(error) = match message {
                        ServerMessage::Request { id } => client
                            .borrow()
                            .produce(ClientMessage::Response{id, response: ClientResponse::RegisterCapability}),
                        ServerMessage::Response(response) => match response {
                            ServerResponse::Initialize(_) => {
                                lang_client.produce(ClientMessage::Request(ClientRequest::Initialized))
                            }
                            ServerResponse::Shutdown => {
                                // TODO: Update for multiple language clients.
                                // TODO: Recognize and resolve unexpected shutdown.
                                is_shutdown = true;
                                Ok(())
                            }
                        }
                    } {
                        error!("Failed to process message from language server: {}", error);
                    }
                }
                Err(ConsumeError::EmptyStock) => {}
                Err(ConsumeError::Failure(failure)) => {
                    error!("Failed to read message from language server: {}", failure);
                }
            }

            match client.borrow().stderr().consume() { 
                Ok(message) => error!("lsp stderr: {}", message),
                Err(ConsumeError::Failure(failure)) => error!("error logger: {}", failure),
                Err(_) => {}
            }
        }

        if is_dropping.load(Ordering::Relaxed) {
            for (language_id, client) in &clients {
                if let Err(error) = client.borrow().produce(ClientMessage::Request(ClientRequest::Shutdown)) {
                    error!(
                        "Failed to send shutdown message to {} language server: {}",
                        language_id, error
                    );
                }
            }

            // Reset is_dropping so that Shutdown is only sent once.
            is_dropping.store(false, Ordering::Relaxed);
        }
    }

    for (language_id, client) in &clients {
        if let Err(error) = client.borrow().produce(ClientMessage::Request(ClientRequest::Exit)) {
            error!(
                "Failed to send exit message to {} language server: {}",
                language_id, error
            );
        }

        if let Err(error) = client.borrow_mut().wait() {
            error!(
                "Failed to wait for {} language server process to finish: {}",
                language_id, error
            );
        }
    }
}

/// Manages the langauge servers.
#[derive(Debug)]
pub(crate) struct LanguageTool {
    /// If the language tool is being dropped.
    drop: Arc<AtomicBool>,
    /// The thread handle of the language client thread.
    thread: Option<JoinHandle<()>>,
}

impl LanguageTool {
    /// Creates a new [`LanguageTool`].
    #[throws(CreateLanguageToolError)]
    pub(crate) fn new(root_dir: &Url) -> Self {
        let is_dropping = Arc::new(AtomicBool::new(false));
        let dir = root_dir.clone();

        Self {
            drop: Arc::clone(&is_dropping),
            thread: Some(thread::spawn(move || {
                if let Err(error) = thread(&dir, &is_dropping) {
                    error!("thread error {}", error);
                }
            })),
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
        if let Some(thread) = self.thread.take() {
            self.drop.store(true, Ordering::Relaxed);

            if thread.join().is_err() {
                error!("Failed to join language tool thread");
            }
        }
    }
}

impl Producer for LanguageTool {
    type Good = ToolMessage<ClientMessage>;
    type Failure = ProduceProtocolError;

    #[throws(ProduceError<Self::Failure>)]
    fn produce(&self, _good: Self::Good) {}
}

/// Tool message of language server.
#[derive(Clone, Debug, ParseDisplay, PartialEq)]
#[display("{language} :: {message}")]
pub(crate) struct ToolMessage<T> {
    /// The URL that generated.
    pub(crate) language: Language,
    /// The message.
    pub(crate) message: T,
}

///// Client message to language server.
//#[derive(Clone, Debug, PartialEq)]
//pub(crate) enum ClientMessage {
//    /// Shuts down language server.
//    Shutdown,
//    /// Exits language server.
//    Exit,
//    /// Initialized.
//    Initialized,
//    /// Configures a document.
//    Doc(DocConfiguration),
//    /// Registers a capability.
//    RegisterCapability(Id),
//}

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

/// An error producing protocol.
#[derive(Debug, Error)]
pub enum ProduceProtocolError {
    /// An error with fault.
    #[error("")]
    Fault(#[from] Fault),
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

//! Implements management and use of language servers.
mod utils;

use {
    log::warn,
    lsp_types::{
        notification::{
            DidChangeTextDocument, DidCloseTextDocument, DidOpenTextDocument, Exit, Initialized,
            WillSaveTextDocument,
        },
        request::{Initialize, Shutdown},
        ClientCapabilities, DidChangeTextDocumentParams, DidCloseTextDocumentParams,
        DidOpenTextDocumentParams, InitializeParams, InitializeResult, InitializedParams,
        MessageType, ShowMessageParams, SynchronizationCapability, TextDocumentClientCapabilities,
        TextDocumentContentChangeEvent, TextDocumentIdentifier, TextDocumentItem,
        TextDocumentSaveReason, TextDocumentSyncCapability, TextDocumentSyncKind, TextEdit, Url,
        VersionedTextDocumentIdentifier, WillSaveTextDocumentParams,
    },
    std::{
        io,
        process::{self, Child, ChildStderr, ChildStdin, ChildStdout, Command, Stdio},
    },
    thiserror::Error,
    utils::{LspErrorProcessor, LspReceiver, LspTransmitter},
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
    #[error("unable to access {0} of language server")]
    Io(String),
    /// An error while spawning a language server process.
    #[error("unable to spawn language server process `{0}`: {1}")]
    Spawn(String, #[source] io::Error),
    /// An error while waiting for a language server process.
    #[error("unable to wait for language server process exit: {0}")]
    Wait(#[source] io::Error),
    /// An error while killing a language server process.
    #[error("unable to kill language server process: {0}")]
    Kill(#[source] io::Error),
    /// Language server for given language identifier is unknown.
    #[error("language server for `{0}` is unknown")]
    LanguageId(String),
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

/// Represents a language server process.
#[derive(Debug)]
pub(crate) struct LspServer {
    /// The language server process.
    server: ServerProcess,
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
    pub(crate) fn new<U>(language_id: &str, root: U) -> Result<Option<Self>, Fault>
    where
        U: AsRef<Url>,
    {
        Ok(if let Some(mut server) = ServerProcess::new(language_id)? {
            let mut transmitter = LspTransmitter::new(server.stdin()?);
            let receiver = LspReceiver::new(server.stdout()?, &transmitter);
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

            #[allow(deprecated)] // root_path is a required field.
            let settings = LspSettings::from(transmitter.request::<Initialize>(
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
                &receiver,
            )?);

            transmitter.notify::<Initialized>(InitializedParams {})?;

            Some(Self {
                // error_processor must be created before server is moved.
                error_processor: LspErrorProcessor::new(server.stderr()?),
                server,
                transmitter,
                settings,
                receiver,
            })
        } else {
            None
        })
    }

    /// Sends the didOpen notification, if appropriate.
    pub(crate) fn did_open<U>(
        &mut self,
        uri: U,
        language_id: &str,
        version: i64,
        text: &str,
    ) -> Result<(), Fault>
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

    /// Attempts to cleanly kill the language server process.
    fn shutdown_and_exit(&mut self) -> Result<(), Fault> {
        self.transmitter.request::<Shutdown>((), &self.receiver)?;
        self.error_processor.terminate()?;
        self.transmitter.notify::<Exit>(())?;
        self.server.wait()
    }
}

impl Drop for LspServer {
    fn drop(&mut self) {
        if let Err(e) = self.shutdown_and_exit() {
            warn!("Unable to cleanly shutdown and exit language server: {}", e);

            if let Err(kill_error) = self.server.kill() {
                warn!("{}", kill_error);
            }
        }
    }
}

/// Signifies a language server process.
#[derive(Debug)]
struct ServerProcess(Child);

impl ServerProcess {
    /// Creates a new [`ServerProcess`].
    fn new(language_id: &str) -> Result<Option<Self>, Fault> {
        let command = match language_id {
            "rust" => Some("rls"),
            _ => None,
        };

        Ok(if let Some(cmd) = command {
            Some(Self(
                Command::new(cmd)
                    .stdin(Stdio::piped())
                    .stdout(Stdio::piped())
                    .stderr(Stdio::piped())
                    .spawn()
                    .map_err(|e| Fault::Spawn(cmd.to_string(), e))?,
            ))
        } else {
            None
        })
    }

    /// Returns the stderr of the process.
    fn stderr(&mut self) -> Result<ChildStderr, Fault> {
        self.0
            .stderr
            .take()
            .ok_or_else(|| Fault::Io("stderr".to_string()))
    }

    /// Returns the stdin of the process.
    fn stdin(&mut self) -> Result<ChildStdin, Fault> {
        self.0
            .stdin
            .take()
            .ok_or_else(|| Fault::Io("stdin".to_string()))
    }

    /// Returns the stdout of the process.
    fn stdout(&mut self) -> Result<ChildStdout, Fault> {
        self.0
            .stdout
            .take()
            .ok_or_else(|| Fault::Io("stdout".to_string()))
    }

    /// Kills the process.
    fn kill(&mut self) -> Result<(), Fault> {
        self.0.kill().map_err(Fault::Kill)
    }

    /// Blocks until the proccess ends.
    fn wait(&mut self) -> Result<(), Fault> {
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

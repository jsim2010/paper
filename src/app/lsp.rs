//! Implements management and use of language servers.
mod utils;

use {
    log::warn,
    lsp_types::{
        notification::{
            DidCloseTextDocument, DidOpenTextDocument, Exit, Initialized, DidChangeTextDocument
        },
        request::{Initialize, Shutdown},
        TextEdit, TextDocumentContentChangeEvent, VersionedTextDocumentIdentifier, DidChangeTextDocumentParams,
        ClientCapabilities, DidCloseTextDocumentParams, DidOpenTextDocumentParams,
        InitializeParams, InitializeResult, InitializedParams, TextDocumentIdentifier,
        TextDocumentItem, TextDocumentSyncCapability, TextDocumentSyncKind, Url,
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
    /// An error from which a language server utility was unable to recover.
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
    /// Creates a new `LspServer` represented by `process_cmd`.
    pub(crate) fn new(process_cmd: &str, root: &Url) -> Result<Self, Fault> {
        let mut server = ServerProcess::new(process_cmd)?;
        let mut transmitter = LspTransmitter::new(server.stdin()?);
        let receiver = LspReceiver::new(server.stdout()?, &transmitter);

        #[allow(deprecated)] // root_path is a required field.
        let settings = LspSettings::from(transmitter.request::<Initialize>(
            InitializeParams {
                process_id: Some(u64::from(process::id())),
                root_path: None,
                root_uri: Some(root.clone()),
                initialization_options: None,
                capabilities: ClientCapabilities::default(),
                trace: None,
                workspace_folders: None,
                client_info: None,
            },
            &receiver,
        )?);

        transmitter.notify::<Initialized>(InitializedParams {})?;

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
    pub(crate) fn did_open(&mut self, text_document: &TextDocumentItem) -> Result<(), Fault> {
        if self.settings.notify_open_close {
            self.transmitter
                .notify::<DidOpenTextDocument>(DidOpenTextDocumentParams {
                    text_document: text_document.clone(),
                })?;
        }

        Ok(())
    }

    pub(crate) fn did_change(&mut self, text_document: &TextDocumentItem, edit: TextEdit) -> Result<(), Fault> {
        if let Some(content_changes) = match self.settings.notify_changes_kind {
            TextDocumentSyncKind::None => None,
            TextDocumentSyncKind::Full => Some(vec![TextDocumentContentChangeEvent {
                range: None,
                range_length: None,
                text: text_document.text.clone(),
            }]),
            TextDocumentSyncKind::Incremental => Some(vec![TextDocumentContentChangeEvent {
                range: Some(edit.range),
                range_length: None,
                text: edit.new_text,
            }]),
        } {
            self.transmitter.notify::<DidChangeTextDocument>(DidChangeTextDocumentParams {
                text_document: VersionedTextDocumentIdentifier::new(text_document.uri.clone(), text_document.version),
                content_changes,
            })?;
        }

        Ok(())
    }

    /// Sends the didClose notification, if appropriate.
    pub(crate) fn did_close(&mut self, text_document: &TextDocumentItem) -> Result<(), Fault> {
        if self.settings.notify_open_close {
            self.transmitter
                .notify::<DidCloseTextDocument>(DidCloseTextDocumentParams {
                    text_document: TextDocumentIdentifier::new(text_document.uri.clone()),
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
    fn new(process_cmd: &str) -> Result<Self, Fault> {
        Ok(Self(
            Command::new(process_cmd)
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()
                .map_err(|e| Fault::Spawn(process_cmd.to_string(), e))?,
        ))
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
    notify_changes_kind: TextDocumentSyncKind,
}

impl Default for LspSettings {
    fn default() -> Self {
        Self {
            notify_open_close: false,
            notify_changes_kind: TextDocumentSyncKind::None,
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
                }
            }
        }

        settings
    }
}

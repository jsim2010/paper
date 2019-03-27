//! Implements `Explorer` for local storage.
use super::ProgressParams;
use crate::ptr::Mrc;
use crate::storage::LanguageClient;
use crate::Output;
use lsp_types::{lsp_notification, DidOpenTextDocumentParams, TextDocumentItem, Url};
use std::fs;
use std::path::Path;
use std::sync::{Arc, Mutex, MutexGuard};

/// Signifies an `Explorer` of the local storage.
#[derive(Debug)]
pub struct Explorer {
    /// A local `LanguageClient`.
    language_client: Arc<Mutex<LanguageClient>>,
}

impl Explorer {
    /// Creates a new `Explorer`.
    pub fn new() -> Mrc<Self> {
        mrc!(Self {
            language_client: LanguageClient::new("rls"),
        })
    }

    /// Returns a mutable reference to the `LanguageClient`.
    fn language_client_mut(&mut self) -> MutexGuard<'_, LanguageClient> {
        self.language_client
            .lock()
            .expect("Locking `LanguageClient` of `Explorer`.")
    }
}

impl super::Explorer for Explorer {
    #[inline]
    fn start(&mut self) -> Output<()> {
        self.language_client_mut().initialize()?;
        Ok(())
    }

    #[inline]
    fn read(&mut self, path: &Path) -> Output<String> {
        let text = fs::read_to_string(path).map(|data| data.replace('\r', ""))?;
        self.language_client_mut()
            .send_notification::<lsp_notification!("textDocument/didOpen")>(
                DidOpenTextDocumentParams {
                    text_document: TextDocumentItem::new(
                        Url::from_file_path(path).map_err(|_| {
                            std::io::Error::new(
                                std::io::ErrorKind::InvalidInput,
                                "Not absolute or invalid prefix",
                            )
                        })?,
                        "rust".into(),
                        0,
                        text.clone(),
                    ),
                },
            )?;
        Ok(text)
    }

    #[inline]
    fn write(&self, path: &Path, text: &str) -> Output<()> {
        fs::write(path, text)?;
        Ok(())
    }

    #[inline]
    fn receive_notification(&mut self) -> Option<ProgressParams> {
        self.language_client_mut().receive_notification()
    }
}

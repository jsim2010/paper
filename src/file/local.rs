//! Implements `Explorer` for local storage.
use super::{Effect, ProgressParams};
use crate::{
    lsp::{LanguageClient, Message},
    ptr::Mrc,
};
use lsp_types::{DidOpenTextDocumentParams, TextDocumentItem, Url};
use std::{
    env, fs,
    io::{self, ErrorKind},
    path::{Path, PathBuf},
    sync::{Arc, Mutex, MutexGuard},
};

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
    fn start(&mut self) -> Effect<()> {
        self.language_client_mut()
            .initialize(env::current_dir()?.as_path())?;
        Ok(())
    }

    #[inline]
    fn read(&mut self, path: &PathBuf) -> Effect<String> {
        let absolute_path = if path.is_absolute() {
            path.clone()
        } else {
            let mut new_path = env::current_dir()?;
            new_path.push(path);
            new_path
        };

        let text = fs::read_to_string(absolute_path.clone()).map(|data| data.replace('\r', ""))?;
        self.language_client_mut()
            .send_message(Message::did_open_text_document_notification(
                DidOpenTextDocumentParams {
                    text_document: TextDocumentItem::new(
                        Url::from_file_path(absolute_path).map_err(|_| {
                            io::Error::new(
                                ErrorKind::InvalidInput,
                                "Not absolute or invalid prefix",
                            )
                        })?,
                        "rust".into(),
                        0,
                        text.clone(),
                    ),
                },
            ))?;
        Ok(text)
    }

    #[inline]
    fn write(&self, path: &Path, text: &str) -> Effect<()> {
        fs::write(path, text)?;
        Ok(())
    }

    #[inline]
    fn receive_notification(&mut self) -> Option<ProgressParams> {
        self.language_client_mut().receive_notification()
    }
}

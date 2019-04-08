//! Implements `Explorer` for local storage.
use super::{Effect, ProgressParams};
use crate::{
    lsp::{NotificationMessage, LanguageClient, RequestMethod},
    ptr::Mrc,
};
use lsp_types::{TextDocumentItem, Url};
use std::{
    env, fs,
    io::{self, ErrorKind},
    sync::{Arc, Mutex, MutexGuard},
};

const URL_CONVERSION_ERROR: &str = "Given path is not absolute or, on Windows, the prefix is not a disk prefix (e.g. `C:`) or a UNC prefix (`\\\\`).";

/// Signifies an `Explorer` of the local storage.
#[derive(Debug)]
pub struct Explorer {
    /// A local `LanguageClient`.
    language_client: Arc<Mutex<LanguageClient>>,
    root_url: Url,
}

impl Explorer {
    /// Creates a new `Explorer`.
    pub fn new(root_url: Url) -> Mrc<Self> {
        mrc!(Self {
            language_client: LanguageClient::new("rls"),
            root_url,
        })
    }

    /// Returns the `Url` of the current directory.
    pub fn current_dir_url() -> Effect<Url> {
        Ok(Url::from_directory_path(env::current_dir()?.as_path())
            .map_err(|_| io::Error::new(ErrorKind::InvalidInput, URL_CONVERSION_ERROR))?)
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
        let root_url = self.root_url.clone();
        Ok(self
            .language_client_mut()
            .send_request(RequestMethod::initialize(&root_url))?)
    }

    #[inline]
    fn read(&mut self, path: &String) -> Effect<TextDocumentItem> {
        let url = self.root_url.join(path).unwrap();
        let doc = TextDocumentItem::new(
            url.clone(),
            "rust".to_string(),
            0,
            fs::read_to_string(url.to_file_path().unwrap())?.replace('\r', ""),
        );
        self.language_client_mut().send_notification(NotificationMessage::did_open_text_document(doc.clone()))?;
        Ok(doc)
    }

    #[inline]
    fn write(&self, doc: &TextDocumentItem) -> Effect<()> {
        fs::write(doc.uri.to_file_path().unwrap(), &doc.text)?;
        Ok(())
    }

    #[inline]
    fn receive_notification(&mut self) -> Option<ProgressParams> {
        self.language_client_mut().receive_notification()
    }
}

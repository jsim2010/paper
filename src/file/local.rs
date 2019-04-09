//! Implements `Explorer` for local storage.
use super::{Effect, ProgressParams};
use crate::{
    lsp::{LanguageClient, NotificationMessage, RequestMethod},
    ptr::Mrc,
};
use lsp_types::{Range, TextDocumentItem, Url};
use std::{
    env, fs,
    io::{self, ErrorKind},
    path::PathBuf,
    sync::{Arc, Mutex, MutexGuard},
};

/// Describes the possible errors when converting a `Url` to a `PathBuf`.
const FROM_URL_ERROR: &str = "Host of URL is neither empty nor `localhost` (except on Windows, where `file:` URLs may have non-local host).";
/// Describes the possible errors when converting a `Path` to a `Url`.
const TO_URL_ERROR: &str = "Given path is not absolute or, on Windows, the prefix is not a disk prefix (e.g. `C:`) or a UNC prefix (`\\\\`).";

/// Converts a `Url` to a `PathBuf`.
fn url_path(url: &Url) -> Effect<PathBuf> {
    Ok(url
        .to_file_path()
        .map_err(|_| io::Error::new(ErrorKind::InvalidInput, FROM_URL_ERROR))?)
}

/// Signifies an `Explorer` of the local storage.
#[derive(Debug)]
pub struct Explorer {
    /// A local `LanguageClient`.
    language_client: Arc<Mutex<LanguageClient>>,
    /// Root URL of the `Explorer`.
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
            .map_err(|_| io::Error::new(ErrorKind::InvalidInput, TO_URL_ERROR))?)
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
        self.language_client_mut()
            .send_request(RequestMethod::initialize(&root_url))?;
        Ok(())
    }

    #[inline]
    fn read(&mut self, path: &str) -> Effect<TextDocumentItem> {
        let url = self.root_url.join(path)?;
        let doc = TextDocumentItem::new(
            url.clone(),
            "rust".to_string(),
            0,
            fs::read_to_string(url_path(&url)?)?.replace('\r', ""),
        );
        self.language_client_mut()
            .send_notification(NotificationMessage::did_open_text_document(doc.clone()))?;
        Ok(doc)
    }

    #[inline]
    fn write(&self, doc: &TextDocumentItem) -> Effect<()> {
        fs::write(url_path(&doc.uri)?, &doc.text)?;
        Ok(())
    }

    fn change(&mut self, doc: &mut TextDocumentItem, range: &Range, text: &str) -> Effect<()> {
        self.language_client_mut().send_notification(
            NotificationMessage::did_change_text_document(doc, range, text),
        )?;
        Ok(())
    }

    #[inline]
    fn receive_notification(&mut self) -> Option<ProgressParams> {
        self.language_client_mut().receive_notification()
    }
}

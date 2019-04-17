//! Implements `Explorer` for local storage.
use super::{Effect, ProgressParams};
use crate::{
    lsp::{LanguageClient, NotificationMessage, RequestMethod},
    ptr::Mrc,
};
use std::{
    env, fs,
    path::{Prefix, Component, PathBuf},
    sync::{Arc, Mutex, MutexGuard},
};

use lsp_msg::{Range, TextDocumentItem};

/// Signifies an `Explorer` of the local storage.
#[derive(Debug)]
pub struct Explorer {
    /// A local `LanguageClient`.
    language_client: Arc<Mutex<LanguageClient>>,
    /// Root URL of the `Explorer`.
    root_uri: String,
}

impl Explorer {
    /// Creates a new `Explorer`.
    pub fn new(root_uri: String) -> Mrc<Self> {
        mrc!(Self {
            language_client: LanguageClient::new("rls"),
            root_uri,
        })
    }

    /// Returns the URI of the current directory.
    pub fn current_dir_uri() -> Effect<String> {
        let path = env::current_dir()?;
        let mut uri = String::from("file:");
        
        for component in path.components() {
            if let Component::RootDir = component {
                continue;
            }

            uri.push('/');

            match component {
                Component::Prefix(prefix) => {
                    match prefix.kind() {
                        Prefix::Disk(drive) => {
                            uri.push(drive as char);
                            uri.push(':');
                        }
                        _ => {
                            panic!("Error generating URI from prefix");
                        }
                    }
                }
                Component::Normal(name) => {
                    if let Some(valid_name) = name.to_str() {
                        uri.push_str(valid_name);
                    }
                }
                _ => {
                    panic!("Error generating URI from component `{}`", component.as_os_str().to_string_lossy());
                }
            }
        }

        uri.push('/');
        Ok(uri)
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
        let root_uri = self.root_uri.clone();
        self.language_client_mut()
            .send_request(RequestMethod::initialize(&root_uri))?;
        Ok(())
    }

    #[inline]
    fn read(&mut self, path: &str) -> Effect<TextDocumentItem> {
        let mut uri = self.root_uri.clone();
        let relative_path = PathBuf::from(path);
        let mut is_first = true;

        for component in relative_path.components() {
            if is_first {
                is_first = false;
            } else {
                uri.push('/');
            }

            match component {
                Component::Normal(name) => {
                    if let Some(valid_name) = name.to_str() {
                        uri.push_str(valid_name);
                    }
                }
                _ => continue,
            }
        }

        let absolute_path = uri.get(6..).unwrap().to_string();
        let doc = TextDocumentItem {
            uri: uri.clone(),
            language_id: "rust".to_string(),
            version: 0,
            text: fs::read_to_string(PathBuf::from(absolute_path))?.replace('\r', ""),
        };
        self.language_client_mut()
            .send_notification(NotificationMessage::did_open_text_document(doc.clone()))?;
        Ok(doc)
    }

    #[inline]
    fn write(&self, doc: &TextDocumentItem) -> Effect<()> {
        fs::write(PathBuf::from(&doc.uri), &doc.text)?;
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

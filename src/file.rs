//! Defines the interaction with files.
use crate::{
    lsp::{self, LanguageClient, NotificationMessage, ProgressParams, RequestMethod},
    Flag, Output,
};
use std::{
    env,
    fmt::{self, Debug, Display, Formatter},
    fs, io,
    path::PathBuf,
    sync::{Arc, Mutex, MutexGuard},
};
use url::Url;

use lsp_msg::{Range, TextDocumentItem};

/// Specifies the type returned by `Explorer` functions.
pub type Effect<T> = Result<T, Error>;

/// Defines the interface between the application and documents.
#[derive(Clone, Debug)]
pub struct Explorer {
    /// A `LanguageClient`.
    language_client: Arc<Mutex<LanguageClient>>,
    /// Root URI.
    root_uri: Url,
}

impl Explorer {
    /// Creates a new `Explorer`.
    pub fn new() -> Output<Self> {
        env::current_dir()
            .map_err(|_| Flag::User)
            .and_then(|path| Url::from_directory_path(path).map_err(|_| Flag::User))
            .map(|root_uri| Self {
                language_client: LanguageClient::new("rls"),
                root_uri,
            })
    }

    /// Returns a mutable reference to the `LanguageClient`.
    fn language_client_mut(&mut self) -> MutexGuard<'_, LanguageClient> {
        self.language_client
            .lock()
            .expect("Locking `LanguageClient` of `Explorer`.")
    }

    /// Initializes all functionality needed by the Explorer.
    pub fn start(&mut self) -> Effect<()> {
        let uri = self.root_uri.clone();
        self.language_client_mut()
            .send_request(RequestMethod::initialize(uri.as_str()))?;
        Ok(())
    }

    /// Returns the text from a file.
    pub fn read(&mut self, path: &str) -> Effect<TextDocumentItem> {
        let uri = self.root_uri.join(path)?;

        let doc = TextDocumentItem {
            uri: uri.clone().into_string(),
            language_id: "rust".to_string(),
            version: 0,
            text: fs::read_to_string(PathBuf::from(
                uri.to_file_path().map_err(|_| Error::InvalidUrl)?,
            ))?
            .replace('\r', ""),
        };
        self.language_client_mut()
            .send_notification(NotificationMessage::did_open_text_document(doc.clone()))?;
        Ok(doc)
    }

    /// Writes text to a file.
    pub fn write(&self, doc: &TextDocumentItem) -> Effect<()> {
        fs::write(PathBuf::from(&doc.uri), &doc.text)?;
        Ok(())
    }

    /// Inform server of change to the working copy of a file.
    pub fn change(&mut self, doc: &mut TextDocumentItem, range: &Range, text: &str) -> Effect<()> {
        self.language_client_mut().send_notification(
            NotificationMessage::did_change_text_document(doc, range, text),
        )?;
        Ok(())
    }

    /// Returns the oldest notification from `Explorer`.
    pub fn receive_notification(&mut self) -> Option<ProgressParams> {
        self.language_client_mut().receive_notification()
    }
}

/// Specifies an error within the `Explorer`.
#[derive(Debug)]
pub enum Error {
    /// Error caused by I/O.
    Io(io::Error),
    /// Error caused by LSP.
    Lsp(lsp::Error),
    /// Error caused while parsing a URL.
    Url(url::ParseError),
    /// Error caused by invalid URL.
    InvalidUrl,
}

impl Display for Error {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(e) => write!(f, "I/O Error caused by {}", e),
            Self::Lsp(e) => write!(f, "Lsp Error {}", e),
            Self::Url(e) => write!(f, "URL Parsing error {}", e),
            Self::InvalidUrl => write!(f, "Invalid URL"),
        }
    }
}

impl From<io::Error> for Error {
    fn from(error: io::Error) -> Self {
        Self::Io(error)
    }
}

impl From<lsp::Error> for Error {
    fn from(error: lsp::Error) -> Self {
        Self::Lsp(error)
    }
}

impl From<url::ParseError> for Error {
    fn from(error: url::ParseError) -> Self {
        Self::Url(error)
    }
}

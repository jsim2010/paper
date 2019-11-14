//! Defines the interaction with files.
use crate::{
    lsp::{self, LanguageClient, NotificationMessage, ProgressParams, RequestMethod},
    Alert, Failure,
};
use lsp_types::{Range, TextDocumentItem, Url};
use std::{
    env,
    fmt::{self, Debug, Display, Formatter},
    fs, io,
    path::PathBuf,
    sync::{Arc, Mutex, MutexGuard},
};

/// Specifies the type returned by `Explorer` functions.
pub(crate) type Outcome<T> = Result<T, Error>;

/// Defines the interface between the application and documents.
#[derive(Clone, Debug)]
pub(crate) struct Explorer {
    /// A `LanguageClient`.
    language_client: Arc<Mutex<LanguageClient>>,
    /// Root URI.
    root_uri: Url,
}

impl Explorer {
    /// Creates a new `Explorer`.
    pub(crate) fn new() -> Outcome<Self> {
        env::current_dir()
            .map_err(Error::from)
            .and_then(|path| {
                Url::from_directory_path(path)
                    .map_err(|_| Error::InvalidPath)
            })
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
    pub(crate) fn start(&mut self) -> Outcome<()> {
        let uri = self.root_uri.clone();
        self.language_client_mut()
            .send_request(RequestMethod::initialize(uri))?;
        Ok(())
    }

    /// Returns the text from a file.
    pub(crate) fn read(&mut self, path: &PathBuf) -> Outcome<TextDocumentItem> {
        let uri = self.root_uri.join(path.as_path().to_str().unwrap())?;

        let doc = TextDocumentItem {
            uri: uri.clone(),
            language_id: "rust".to_string(),
            version: 0,
            text: fs::read_to_string(uri.to_file_path().map_err(|_| Error::InvalidUrl)?)?
                .replace('\r', ""),
        };
        self.language_client_mut()
            .send_notification(NotificationMessage::did_open_text_document(doc.clone()))?;
        Ok(doc)
    }

    /// Writes text to a file.
    pub(crate) fn write(&self, doc: &TextDocumentItem) -> Outcome<()> {
        fs::write(PathBuf::from(doc.uri.as_str()), &doc.text)?;
        Ok(())
    }

    /// Inform server of change to the working copy of a file.
    pub(crate) fn change(
        &mut self,
        doc: &mut TextDocumentItem,
        range: &Range,
        text: &str,
    ) -> Outcome<()> {
        self.language_client_mut().send_notification(
            NotificationMessage::did_change_text_document(doc, range, text),
        )?;
        Ok(())
    }

    /// Returns the oldest notification from `Explorer`.
    pub(crate) fn receive_notification(&mut self) -> Option<ProgressParams> {
        self.language_client_mut().receive_notification()
    }
}

impl From<io::Error> for Alert {
    fn from(value: io::Error) -> Self {
        Self::Explorer(Error::from(value))
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
    /// Error caused by invalid path.
    InvalidPath
}

impl Display for Error {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(e) => write!(f, "I/O Error caused by {}", e),
            Self::Lsp(e) => write!(f, "Lsp Error {}", e),
            Self::Url(e) => write!(f, "URL Parsing error {}", e),
            Self::InvalidUrl => write!(f, "Invalid URL"),
            Self::InvalidPath => write!(f, "Invalid Path"),
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

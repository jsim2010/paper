use {
    crate::Failure,
    jsonrpc_core::{Call, MethodCall, Value, Params, Version, Id},
    lsp_types::{InitializeParams, InitializedParams, InitializeResult, Url, ClientCapabilities, notification::{Notification, Initialized}, request::{Initialize, Request}},
    serde::Serialize,
    std::{sync::mpsc::{self, Receiver, RecvError, Sender, SendError}, io::{self, Read, BufRead, BufReader, Write, ErrorKind}, env, process::{self, Stdio, Command, Child, ChildStdout}, thread},
    log::{trace, warn, error},
    displaydoc::Display as DisplayDoc,
};

/// A response to a LSP request.
#[derive(Debug)]
pub enum Response {
    /// Response for `initialize`.
    Initialize(InitializeResult),
}

/// An Lsp Error.
#[derive(Debug, DisplayDoc)]
pub enum LspError {
    /// send error `{0}`
    Send(SendError<Response>),
    /// receive error `{0}`
    Receive(RecvError),
    /// unable to access stdout of language server
    InvalidStdout,
}

impl From<SendError<Response>> for LspError {
    fn from(value: SendError<Response>) -> Self {
        Self::Send(value)
    }
}

impl From<RecvError> for LspError {
    fn from(value: RecvError) -> Self {
        Self::Receive(value)
    }
}

struct LspProcessor {
    reader: BufReader<ChildStdout>,
    tx: Sender<Response>,
}

impl LspProcessor {
    fn new(process: &mut Child, tx: Sender<Response>) -> Result<Self, Failure> {
        process.stdout.take().ok_or_else(|| {
                LspError::InvalidStdout.into()
        }).map(|stdout| Self { reader: BufReader::new(stdout), tx})
    }

    fn process(&mut self) -> Result<(), LspError> {
        let mut line = String::new();
        let mut blank_line = String::new();

        if self.reader.read_line(&mut line).is_ok() {
            let mut split = line.trim().split(": ");

            if split.next() == Some("Content-Length")
                && self.reader.read_line(&mut blank_line).is_ok()
            {
                if let Some(length_str) = split.next() {
                    if let Ok(length) = length_str.parse() {
                        let mut content = vec![0; length];

                        if self.reader.read_exact(&mut content).is_ok() {
                            if let Ok(json_string) = String::from_utf8(content) {
                                trace!("received: {}", json_string);
                                if let Ok(message) = serde_json::from_str::<Value>(&json_string) {
                                    if let Some(result) = message.get("result") {
                                        if let Ok(response) = serde_json::from_value(result.clone()) {
                                            #[allow(clippy::result_expect_used)]
                                            self.tx.send(Response::Initialize(response))?;
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        Ok(())
    }
}

/// Represents a language server process.
#[derive(Debug)]
pub(crate) struct LspServer {
    /// The language server process.
    process: Child,
    /// Receives responses from the language server process.
    rx: Receiver<Response>,
}

impl LspServer {
    /// Creates a new `LspServer` represented by `process_cmd`.
    pub(crate) fn new(process_cmd: &str) -> Result<Self, Failure> {
        let mut process = Command::new(process_cmd)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?;
        let (tx, rx) = mpsc::channel();
        let mut processor = LspProcessor::new(&mut process, tx)?;

        let _ = thread::spawn(move || {
            if let Err(error) = processor.process() {
                error!("Error in LspProcessor: {}", error);
            }
        });

        Ok(Self {
            process,
            rx,
        })
    }

    /// Initializes the `LspServer`.
    pub(crate) fn initialize(&mut self) -> Result<(), Failure> {
        #[allow(deprecated)] // root_path is a required field.
        self.send_request::<Initialize>(InitializeParams {
            process_id: Some(u64::from(process::id())),
            root_path: None,
            root_uri: Some(
                Url::from_directory_path(env::current_dir()?.as_path()).map_err(|_| {
                    Failure::File(io::Error::new(
                        ErrorKind::Other,
                        "cannot convert current_dir to url",
                    ))
                })?,
            ),
            initialization_options: None,
            capabilities: ClientCapabilities::default(),
            trace: None,
            workspace_folders: None,
            client_info: None,
        })?;

        let _response = self.rx.recv().map_err(LspError::from)?;
        self.send_notification::<Initialized>(
            InitializedParams {},
        )?;

        Ok(())
    }

    /// Sends a request with `params` to the language server process.
    fn send_request<T: Request>(&mut self, params: T::Params) -> Result<(), Failure>
    where
        T::Params: Serialize,
    {
        if let Value::Object(params_object) = serde_json::to_value(params)? {
            self.send_message(&Call::MethodCall(MethodCall {
                jsonrpc: Some(Version::V2),
                method: T::METHOD.to_string(),
                params: Params::Map(params_object),
                id: Id::Num(0),
            }))?;
        } else {
            warn!("Request params converted to something other than an object");
        }

        Ok(())
    }

    /// Sends a notification with `params` to the language server process.
    fn send_notification<T: Notification>(&mut self, params: T::Params) -> Result<(), Failure>
    where
        T::Params: Serialize,
    {
        if let Value::Object(params_object) = serde_json::to_value(params)? {
            self.send_message(&Call::Notification(jsonrpc_core::Notification {
                jsonrpc: Some(Version::V2),
                method: T::METHOD.to_string(),
                params: Params::Map(params_object),
            }))?;
        } else {
            warn!("Notification params converted to something other than an object");
        }

        Ok(())
    }

    /// Sends `message` to the language server process.
    fn send_message(&mut self, message: &Call) -> Result<(), Failure> {
        let json_string = serde_json::to_string(message)?;
        trace!("Sending: {}", json_string);

        if let Some(stdin) = self.process.stdin.as_mut() {
            write!(
                stdin,
                "Content-Length: {}\r\n\r\n{}",
                json_string.len(),
                json_string
            )
            .unwrap();
        } else {
            warn!("Unable to retrieve stdin of language server processs");
        }

        Ok(())
    }
}

impl Drop for LspServer {
    fn drop(&mut self) {
        if self.process.kill().is_err() {
            warn!("Attempted to kill a language server process that was not running");
        }
    }
}

use {
    crate::Failure,
    jsonrpc_core::{Call, MethodCall, Value, Params, Output, Success, Response, Version, Id},
    lsp_types::{lsp_request, lsp_notification, InitializeParams, InitializedParams, Url, ClientCapabilities, notification::Notification, request::Request},
    serde::Serialize,
    std::{sync::{Mutex, Arc, mpsc::{self, Receiver, RecvError, Sender, SendError}}, io::{self, Read, BufRead, BufReader, Write, ErrorKind}, env, process::{self, Stdio, Command, Child, ChildStdout, ChildStdin}, thread},
    log::{trace, error, warn},
    displaydoc::Display as DisplayDoc,
};

/// An Lsp Error.
#[derive(Clone, Copy, Debug, DisplayDoc)]
pub enum LspError {
    /// send error `{0}`
    Send(SendError<u64>),
    /// receive error `{0}`
    Receive(RecvError),
    /// unable to access stdout of language server
    InvalidStdout,
    /// unable to process request params
    InvalidRequestParams,
}

impl From<SendError<u64>> for LspError {
    fn from(value: SendError<u64>) -> Self {
        Self::Send(value)
    }
}

impl From<RecvError> for LspError {
    fn from(value: RecvError) -> Self {
        Self::Receive(value)
    }
}

struct LspProcessor {
    transmitter: LspTransmitter,
    reader: BufReader<ChildStdout>,
    response_tx: Sender<u64>,
    is_quitting: bool,
}

impl LspProcessor {
    fn new(process: &mut Child, response_tx: Sender<u64>, transmitter: LspTransmitter) -> Result<Self, Failure> {
        process.stdout.take().ok_or_else(|| {
                LspError::InvalidStdout.into()
        }).map(|stdout| Self { reader: BufReader::new(stdout), response_tx, is_quitting: false, transmitter})
    }

    fn process(&mut self) -> Result<(), LspError> {
        let mut line = String::new();
        let mut blank_line = String::new();

        while !self.is_quitting {
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
                                    trace!("Received: {}", json_string);
                                    if let Ok(message) = serde_json::from_str::<Value>(&json_string) {
                                        if let Some(_result) = message.get("result") {
                                            if let Some(id) = message.get("id") {
                                                if let Ok(response_id) = serde_json::from_value(id.clone()) {
                                                    self.response_tx.send(response_id)?;
                                                }
                                            }
                                        } else if let Some(id) = message.get("id") {
                                            if let Ok(message_id) = serde_json::from_value::<u64>(id.clone()) {
                                                self.transmitter.send_response(&Response::Single(Output::Success(Success {
                                                    jsonrpc: Some(Version::V2),
                                                    result: Value::Null,
                                                    id: Id::Num(message_id),
                                                })))?;
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
                
                line.clear();
            }
        }
        
        Ok(())
    }
}

impl Drop for LspProcessor {
    fn drop(&mut self) {
        self.is_quitting = true;
    }
}

/// Represents a language server process.
#[derive(Debug)]
pub(crate) struct LspServer {
    /// The language server process.
    process: Child,
    /// Receives responses from the language server process.
    response_rx: Receiver<u64>,
    transmitter: LspTransmitter,
    stderr_tx: Sender<()>,
}

impl LspServer {
    /// Creates a new `LspServer` represented by `process_cmd`.
    pub(crate) fn new(process_cmd: &str) -> Result<Self, Failure> {
        let mut process = Command::new(process_cmd)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?;
        let (response_tx, response_rx) = mpsc::channel();
        let transmitter = LspTransmitter::new(process.stdin.take().unwrap());
        let mut processor = LspProcessor::new(&mut process, response_tx, transmitter.clone())?;

        let _ = thread::spawn(move || {
            if let Err(error) = processor.process() {
                error!("Error in LspProcessor: {}", error);
            }
        });

        let stderr = process.stderr.take().unwrap();
        let (stderr_tx, stderr_rx) = mpsc::channel();
        let _ = thread::spawn(move || {
            let mut reader = BufReader::new(stderr);
            let mut line = String::new();

            while stderr_rx.try_recv().is_err() {
                // Rust's language server (rls) seems to send empty lines over stderr after shutdown request so skip those.
                if reader.read_line(&mut line).is_ok() && !line.is_empty() {
                    error!("{}", line);
                    line.clear();
                }
            }
        });

        Ok(Self {
            process,
            response_rx,
            transmitter,
            stderr_tx,
        })
    }

    /// Initializes the `LspServer`.
    pub(crate) fn initialize(&mut self) -> Result<(), Failure> {
        #[allow(deprecated)] // root_path is a required field.
        self.send_request::<lsp_request!("initialize")>(InitializeParams {
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

        self.transmitter.send_notification::<lsp_notification!("initialized")>(
            InitializedParams {},
        )?;

        Ok(())
    }

    fn shutdown_and_exit(&mut self) -> Result<(), Failure> {
        self.send_request::<lsp_request!("shutdown")>(())?;
        self.terminate_stderr_thread();
        self.transmitter.send_notification::<lsp_notification!("exit")>(())?;
        
        if let Err(e) = self.process.wait() {
            warn!("Unable to wait on language server process exit: {}", e);
        }

        Ok(())
    }

    fn terminate_stderr_thread(&self) {
        self.stderr_tx.send(()).unwrap();
    }

    /// Sends a request with `params` to the language server process and waits for a response.
    fn send_request<T: Request>(&mut self, params: T::Params) -> Result<(), Failure>
    where
        T::Params: Serialize,
    {
        self.transmitter.send_request::<T>(params)?;

        while !self.transmitter.confirm_id(self.response_rx.recv().map_err(LspError::from)?) {}

        Ok(())
    }
}

impl Drop for LspServer {
    fn drop(&mut self) {
        if let Err(e) = self.shutdown_and_exit() {
            warn!("Unable to cleanly shutdown and exit language server: {}", e);

            if let Err(kill_error) = self.process.kill() {
                warn!("Unable to kill language server process: {}", kill_error);
            }
        }
    }
}

#[derive(Clone, Debug)]
struct LspTransmitter {
    stdin: Arc<Mutex<ChildStdin>>,
    /// Current request id.
    id: u64,
}

impl LspTransmitter {
    fn new(stdin: ChildStdin) -> Self {
        Self {id: 0, stdin: Arc::new(Mutex::new(stdin))}
    }

    fn confirm_id(&mut self, id: u64) -> bool {
        let result = id == self.id;
        
        if result {
            self.id += 1;
        }

        result
    }

    fn get_params<T: Serialize>(params: T) -> Result<Params, Failure>
    {
        Ok(match serde_json::to_value(params)? {
            Value::Object(params_object) => Ok(Params::Map(params_object)),
            Value::Null => Ok(Params::None),
            _ => Err(LspError::InvalidRequestParams),
        }?)
    }

    /// Sends a request with `params` to the language server process and waits for a response.
    fn send_request<T: Request>(&mut self, params: T::Params) -> Result<(), Failure>
    where
        T::Params: Serialize,
    {
        self.send_call(&Call::MethodCall(MethodCall {
            jsonrpc: Some(Version::V2),
            method: T::METHOD.to_string(),
            params: Self::get_params(params)?,
            id: Id::Num(self.id),
        }))
    }

    /// Sends a notification with `params` to the language server process.
    fn send_notification<T: Notification>(&mut self, params: T::Params) -> Result<(), Failure>
    where
        T::Params: Serialize,
    {
        self.send_call(&Call::Notification(jsonrpc_core::Notification {
            jsonrpc: Some(Version::V2),
            method: T::METHOD.to_string(),
            params: Self::get_params(params)?,
        }))?;

        Ok(())
    }

    /// Sends `call` to the language server process.
    fn send_call(&mut self, call: &Call) -> Result<(), Failure> {
        self.send_string(serde_json::to_string(call)?);

        Ok(())
    }

    fn send_response(&mut self, response: &Response) -> Result<(), LspError> {
        self.send_string(serde_json::to_string(response).unwrap());

        Ok(())
    }

    fn send_string(&mut self, s: String) {
        trace!("Sending: {}", s);

        write!(
            self.stdin.lock().unwrap(),
            "Content-Length: {}\r\n\r\n{}",
            s.len(),
            s
        ).unwrap();
    }
}

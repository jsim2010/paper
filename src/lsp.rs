use std::sync::{Arc, Mutex};
use lsp_types::{InitializedParams, InitializeParams, Registration, ServerCapabilities, InitializeResult, Url};
use std::thread::{self, JoinHandle};
use std::process::{Child, ChildStdout, ChildStdin, Command, Stdio};
use serde_json::Value;
use std::io::{BufReader, Write, BufRead, Read};
use std::sync::mpsc::{self, Receiver, Sender};
use crate::file::ProgressParams;
use crate::storage::LspError;
use jsonrpc_core;
use lsp_types::request::{Initialize, Request};
use serde::Serialize;
use lsp_types::notification::{Initialized, Notification};

/// The interface with the language server.
#[derive(Debug)]
pub(crate) struct LanguageClient {
    /// The thread running the language server.
    server: Child,
    /// The id for the next request to be sent by `LanguageClient`.
    request_id: u64,
    /// The capabilities of the language server.
    server_capabilities: ServerCapabilities,
    /// Registrations received from language server.
    registrations: Vec<Registration>,
    /// Handle of the receiver thread.
    receiver_handle: Option<JoinHandle<()>>,
    /// Receives notifications.
    notification_rx: Receiver<ProgressParams>,
}

impl LanguageClient {
    /// Creates a new `LanguageClient`.
    pub(crate) fn new(command: &str) -> Arc<Mutex<Self>> {
        let server = Self::spawn_server(command);
        let (notification_tx, notification_rx) = mpsc::channel::<ProgressParams>();
        let their_client = Arc::new(Mutex::new(Self {
            server,
            notification_rx,
            request_id: u64::default(),
            server_capabilities: ServerCapabilities::default(),
            registrations: Vec::new(),
            receiver_handle: None,
        }));
        let my_client = Arc::clone(&their_client);
        their_client.lock().expect("Locking language client").receiver_handle = Some(thread::spawn(move ||
            Self::process(my_client, notification_tx)
        ));
        their_client
    }

    fn spawn_server(command: &str) -> Child {
        Command::new(command)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("Spawning language server process")
    }

    fn process(mut client: Arc<Mutex<Self>>, notification_tx: Sender<ProgressParams>) {
        let mut messages = MessageReader::new(&mut client);

        loop {
            if let Ok(message) = messages.get_message() {
                if let Some(id) = message.get("id") {
                    if let Ok(id) = serde_json::from_value::<u64>(id.to_owned()) {
                        if let Some(_method) = message.get("method") {
                            if let Ok(params) =
                                serde_json::from_value::<lsp_types::RegistrationParams>(
                                    message.get("params").unwrap().to_owned(),
                                )
                            {
                                let mut client = client
                                    .lock()
                                    .expect("Accessing language client from receiver");
                                client.registrations = params.registrations;
                                client
                                    .send_response::<lsp_types::request::RegisterCapability>((), id)
                                    .expect("Sending RegisterCapability to language server");
                            } else {
                                dbg!(message);
                            }
                        } else if let Some(result) = message.get("result") {
                            if let Ok(initialize_result) =
                                serde_json::from_value::<InitializeResult>(
                                    result.to_owned(),
                                )
                            {
                                let mut client = client
                                    .lock()
                                    .expect("Accessing language client from receiver");
                                client.server_capabilities = initialize_result.capabilities;
                                client.send_notification::<Initialized>(InitializedParams {}).unwrap();
                            } else {
                                dbg!(result);
                            }
                        } else {
                            dbg!(message);
                        }
                    } else {
                        dbg!(message);
                    }
                } else if let Some(_method) = message.get("method") {
                    if let Ok(params) = serde_json::from_value::<ProgressParams>(
                        message.get("params").unwrap().to_owned(),
                    ) {
                        notification_tx
                            .send(params)
                            .expect("Transferring notification")
                    } else {
                        dbg!(message);
                    }
                } else {
                    dbg!(message);
                }
            } else {
                dbg!("Unable to read message");
            }
        }
    }

    pub(crate) fn initialize(&mut self) -> Result<(), LspError> {
        Ok(self.send_request::<Initialize>(InitializeParams {
            process_id: Some(u64::from(std::process::id())),
            root_path: None,
            root_uri: Some(
                Url::from_file_path(std::env::current_dir()?.as_path()).map_err(|_| LspError::Io)?,
            ),
            initialization_options: None,
            capabilities: lsp_types::ClientCapabilities::default(),
            trace: None,
            workspace_folders: None,
        })?)
    }

    /// Sends a response to the language server.
    fn send_response<T: Request>(&mut self, result: T::Result, id: u64) -> Result<(), LspError>
    where
        T::Result: Serialize,
    {
        let response = jsonrpc_core::Output::Success(jsonrpc_core::Success {
            jsonrpc: Some(jsonrpc_core::Version::V2),
            result: serde_json::to_value(result)?,
            id: jsonrpc_core::Id::Num(id),
        });
        self.send_message(&response)
    }

    /// Sends a message to the language server.
    fn send_message<T: Serialize>(&mut self, message: &T) -> Result<(), LspError> {
        let json_string = serde_json::to_string(message)?;
        write!(
            self.stdin_mut(),
            "Content-Length: {}\r\n\r\n{}",
            json_string.len(),
            json_string
        )?;
        Ok(())
    }

    /// Sends a notification to the language server.
    pub(crate) fn send_notification<T: Notification>(
        &mut self,
        params: T::Params,
    ) -> Result<(), LspError>
    where
        T::Params: Serialize,
    {
        if let serde_json::value::Value::Object(params) = serde_json::to_value(params)? {
            let notification = jsonrpc_core::Call::Notification(jsonrpc_core::Notification {
                jsonrpc: Some(jsonrpc_core::Version::V2),
                method: T::METHOD.to_string(),
                params: jsonrpc_core::Params::Map(params),
            });
            self.send_message(&notification)
        } else {
            Ok(())
        }
    }

    /// Sends a request to the language server.
    fn send_request<T: Request>(&mut self, params: T::Params) -> Result<(), LspError>
    where
        T::Params: Serialize,
    {
        if let serde_json::value::Value::Object(params) = serde_json::to_value(params)? {
            let request = jsonrpc_core::Call::MethodCall(jsonrpc_core::MethodCall {
                jsonrpc: Some(jsonrpc_core::Version::V2),
                method: T::METHOD.to_string(),
                params: jsonrpc_core::Params::Map(params),
                id: jsonrpc_core::Id::Num(self.request_id),
            });
            self.request_id += 1;
            self.send_message(&request)
        } else {
            Ok(())
        }
    }

    /// Returns a mutable reference to the stdin of the language server.
    fn stdin_mut(&mut self) -> &mut ChildStdin {
        self.server
            .stdin
            .as_mut()
            .expect("Accessing stdin of language server process.")
    }

    /// Return the notification from the `Explorer`.
    pub(crate) fn receive_notification(&self) -> Option<ProgressParams> {
        self.notification_rx.try_recv().ok()
    }

    fn stdout(&mut self) -> ChildStdout {
        self.server.stdout.take().expect("Taking stdout of language server")
    }
}

struct MessageReader {
    reader: BufReader<ChildStdout>,
}

impl MessageReader {
    fn new(client: &mut Arc<Mutex<LanguageClient>>) -> Self {
        Self {
            reader: BufReader::new(client.lock().expect("Locking language client").stdout()),
        }
    }

    fn get_message(&mut self) -> Result<Value, LspError> {
        let content_length = self.get_content_length()?;
        let mut content = vec![0; content_length];

        self.reader.read_exact(&mut content)?;
        let json_string = String::from_utf8(content).map_err(|_| LspError::Parse)?;
        Ok(serde_json::from_str(&json_string)?)
    }

    fn get_content_length(&mut self) -> Result<usize, LspError> {
        let mut line = String::new();
        let mut blank_line = String::new();

        let mut _bytes_read = self.reader.read_line(&mut line)?;
        let mut split = line.trim().split(": ");

        if split.next() == Some("Content-Length") {
            _bytes_read = self.reader.read_line(&mut blank_line)?;
            Ok(split
               .next()
               .ok_or(LspError::Protocol)
               .and_then(|value_string| value_string.parse().map_err(|_| LspError::Parse))?)
        } else {
            Err(LspError::Protocol)
        }
    }
}

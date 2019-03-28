//! Defines the interaction with files.
pub(crate) mod local;

use crate::Output;
use serde::{Deserialize, Serialize};
use std::fmt::Debug;
use std::path::Path;

/// Defines the interface between the application and documents.
pub trait Explorer: Debug {
    /// Initializes all functionality needed by the Explorer.
    fn start(&mut self) -> Output<()>;
    /// Returns the text from a file.
    fn read(&mut self, path: &Path) -> Output<String>;
    /// Writes text to a file.
    fn write(&self, path: &Path, text: &str) -> Output<()>;
    /// Returns the oldest notification from `Explorer`.
    fn receive_notification(&mut self) -> Option<ProgressParams>;
}

#[derive(Deserialize, Debug, Serialize)]
/// `ProgressParams` defined by `VSCode`.
pub struct ProgressParams {
    /// The id of the notification.
    id: String,
    /// The title of the notification.
    title: String,
    /// The message of the notification.
    pub message: Option<String>,
    /// Indicates if no more notifications will be sent.
    done: Option<bool>,
}

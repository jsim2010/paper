//! Defines the interaction with files.
pub(crate) mod local;

use crate::lsp::ProgressParams;
use crate::Output;
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

//! Implements the logging functionality of the application.
use {
    log::{trace, LevelFilter, Log, Metadata, Record, SetLoggerError},
    std::{
        fs::File,
        io::{self, Write},
        sync::{Arc, RwLock, RwLockWriteGuard},
    },
    thiserror::Error,
    time::PrimitiveDateTime,
};

/// Provides a handle to dynamically configure the [`Logger`].
#[derive(Clone, Debug)]
pub struct LogConfig {
    /// A pointer to the [`Writer`] used by the [`Logger`].
    writer: Arc<RwLock<Writer>>,
}

impl LogConfig {
    /// Creates a new [`LogConfig`].
    pub(crate) fn new() -> Result<Self, Fault> {
        let logger = Logger::new()?;
        let writer = Arc::clone(logger.writer());

        log::set_boxed_logger(Box::new(logger))?;
        log::set_max_level(LevelFilter::Trace);
        trace!("logger initialized");

        Ok(Self { writer })
    }

    /// Returns the writer.
    pub(crate) fn writer(&self) -> Result<RwLockWriteGuard<'_, Writer>, Fault> {
        self.writer.write().map_err(|_| Fault::WriterLock)
    }
}

/// Implements writing logs to a file.
#[derive(Debug)]
pub(crate) struct Writer {
    /// Defines the file that stores logs.
    file: File,
    /// Defines the level at which logs from starship are allowed.
    pub(crate) starship_level: LevelFilter,
}

impl Writer {
    /// Creates a new [`Writer`].
    fn new() -> Result<Self, Fault> {
        let log_filename = "paper.log".to_string();

        Ok(Self {
            file: File::create(&log_filename).map_err(|e| Fault::CreateFile(log_filename, e))?,
            starship_level: LevelFilter::Off,
        })
    }

    /// Writes `record` to the file of `self`.
    fn write(&mut self, record: &Record<'_>) {
        let _ = writeln!(
            self.file,
            "{} [{}] {}: {}",
            PrimitiveDateTime::now().format("%F %T"),
            record.level(),
            record.target(),
            record.args()
        );
    }

    /// Flushes the buffer of the writer.
    fn flush(&mut self) {
        let _ = self.file.flush();
    }
}

/// Implements the logger of the application.
pub(crate) struct Logger {
    /// The [`Writer`] of the logger.
    writer: Arc<RwLock<Writer>>,
}

impl Logger {
    /// Creates a new [`Logger`].
    pub(crate) fn new() -> Result<Self, Fault> {
        Ok(Self {
            writer: Arc::new(RwLock::new(Writer::new()?)),
        })
    }

    /// Returns the writer.
    pub(crate) const fn writer(&self) -> &Arc<RwLock<Writer>> {
        &self.writer
    }
}

impl Log for Logger {
    fn enabled(&self, metadata: &Metadata<'_>) -> bool {
        if let Ok(writer) = self.writer.read() {
            if metadata.target().starts_with("starship") {
                metadata.level() <= writer.starship_level
            } else {
                true
            }
        } else {
            false
        }
    }

    fn log(&self, record: &Record<'_>) {
        if self.enabled(record.metadata()) {
            if let Ok(mut writer) = self.writer.write() {
                writer.write(record);
            }
        }
    }

    fn flush(&self) {
        if let Ok(mut writer) = self.writer.write() {
            writer.flush();
        }
    }
}

/// An error that occurs within the logger.
#[derive(Debug, Error)]
pub enum Fault {
    /// A failure to initialize the logger.
    #[error("failed to initialize logger: {0}")]
    Init(#[from] SetLoggerError),
    /// A failure to create the log file.
    #[error("failed to create log file `{0}`: {1}")]
    CreateFile(String, #[source] io::Error),
    /// Failed to lock the [`Writer`] of the [`Logger`].
    #[error("log writer lock is poisoned")]
    WriterLock,
}

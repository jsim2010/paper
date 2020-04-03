//! Implements the logging functionality of `paper`.
use {
    crate::io::Arguments,
    log::{trace, LevelFilter, Log, Metadata, Record, SetLoggerError},
    std::{
        fs::File,
        io::{self, Write},
        sync::{Arc, RwLock},
    },
    thiserror::Error,
    time::OffsetDateTime,
};

/// An error from which the logging functionality was unable to recover.
#[derive(Debug, Error)]
pub enum Fault {
    /// A failure to initialize the logger.
    #[error("while initializing logger: {0}")]
    Init(#[from] SetLoggerError),
    /// An error while creating the log file.
    #[error("while creating log file `{0}`: {1}")]
    CreateFile(String, #[source] io::Error),
    /// Failed to lock the logger.
    #[error("unable to lock logger")]
    Lock,
}

/// Configures logging for `paper` during runtime.
#[derive(Clone, Debug)]
pub(crate) struct LogManager {
    /// Implements the logging for `paper`.
    config: Arc<RwLock<Config>>,
}

impl LogManager {
    /// Creates a new [`LogManager`].
    pub(crate) fn new(arguments: &Arguments<'_>) -> Result<Self, Fault> {
        let logger = Logger::new(arguments)?;
        let config = Arc::clone(logger.config());

        log::set_boxed_logger(Box::new(logger))?;
        log::set_max_level(LevelFilter::Trace);
        trace!("Logger initialized");

        Ok(Self { config })
    }
}

/// Implements the logger of the application.
struct Logger {
    /// Defines the file that stores logs.
    file: Arc<RwLock<File>>,
    /// The [`Config`] of the logger.
    config: Arc<RwLock<Config>>,
}

impl Logger {
    /// Creates a new [`Logger`].
    fn new(arguments: &Arguments<'_>) -> Result<Self, Fault> {
        let log_filename = "paper.log".to_string();

        Ok(Self {
            file: Arc::new(RwLock::new(
                File::create(&log_filename).map_err(|e| Fault::CreateFile(log_filename, e))?,
            )),
            config: Arc::new(RwLock::new(Config::new(arguments))),
        })
    }

    /// Returns the config.
    const fn config(&self) -> &Arc<RwLock<Config>> {
        &self.config
    }
}

impl Log for Logger {
    fn enabled(&self, metadata: &Metadata<'_>) -> bool {
        if let Ok(config) = self.config.read() {
            if metadata.level() <= config.level {
                if metadata.target().starts_with("starship") {
                    config.is_starship_enabled
                } else {
                    true
                }
            } else {
                false
            }
        } else {
            false
        }
    }

    fn log(&self, record: &Record<'_>) {
        if self.enabled(record.metadata()) {
            if let Ok(mut file) = self.file.write() {
                #[allow(unused_must_use)]
                {
                    // log() definition does not allow propagating error.
                    writeln!(
                        file,
                        "{} [{}]: {}",
                        OffsetDateTime::now_local().format("%F %T"),
                        record.level(),
                        record.args()
                    );
                }
            }
        }
    }

    fn flush(&self) {
        if let Ok(mut file) = self.file.write() {
            #[allow(unused_must_use)]
            {
                // flush() definition does not allow propagating error.
                file.flush();
            }
        }
    }
}

/// Implements writing logs to a file.
#[derive(Debug)]
struct Config {
    /// Defines if logs from starship are written.
    is_starship_enabled: bool,
    /// The minimum level of logs to be written.
    level: LevelFilter,
}

impl Config {
    /// Creates a new [`Config`].
    const fn new(arguments: &Arguments<'_>) -> Self {
        Self {
            is_starship_enabled: arguments.is_starship_enabled,
            level: arguments.verbosity,
        }
    }
}

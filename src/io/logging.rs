//! Implements the logging functionality of `paper`.
use {
    log::{trace, LevelFilter, Log, Metadata, Record, SetLoggerError},
    market::Producer,
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
    pub(crate) fn new() -> Result<Self, Fault> {
        let logger = Logger::new()?;
        let config = Arc::clone(logger.config());

        log::set_boxed_logger(Box::new(logger))?;
        log::set_max_level(LevelFilter::Trace);
        trace!("Logger initialized");

        Ok(Self { config })
    }
}

impl Producer for LogManager {
    type Good = Output;
    type Error = Fault;

    fn produce(&self, good: Self::Good) -> Result<Option<Self::Good>, Self::Error> {
        match good {
            Output::StarshipLevel(level) => {
                if let Ok(mut config) = self.config.write() {
                    config.starship_level = level;
                }
            }
        }

        Ok(None)
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
    fn new() -> Result<Self, Fault> {
        let log_filename = "paper.log".to_string();

        Ok(Self {
            file: Arc::new(RwLock::new(
                File::create(&log_filename).map_err(|e| Fault::CreateFile(log_filename, e))?,
            )),
            config: Arc::new(RwLock::new(Config::new())),
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
            if metadata.target().starts_with("starship") {
                metadata.level() <= config.starship_level
            } else {
                true
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
    /// Defines the level at which logs from starship are allowed.
    starship_level: LevelFilter,
}

impl Config {
    /// Creates a new [`Config`].
    const fn new() -> Self {
        Self {
            starship_level: LevelFilter::Off,
        }
    }
}

/// A logging output.
pub(crate) enum Output {
    /// The level of the starship module.
    StarshipLevel(LevelFilter),
}

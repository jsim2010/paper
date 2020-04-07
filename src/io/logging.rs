//! Implements the logging functionality of `paper`.
use {
    clap::ArgMatches,
    log::{info, LevelFilter, Log, Metadata, Record, SetLoggerError},
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
}

/// Creates a new logger.
pub(crate) fn init(config: Config) -> Result<(), Fault> {
    let logger = Logger::new(config.is_starship_enabled)?;

    log::set_boxed_logger(Box::new(logger))?;
    log::set_max_level(config.level);
    info!("Logger initialized");
    Ok(())
}

/// Implements the logger of the application.
struct Logger {
    /// Defines the file that stores logs.
    file: Arc<RwLock<File>>,
    /// If logs from starship should be written.
    is_starship_enabled: bool,
}

impl Logger {
    /// Creates a new [`Logger`].
    fn new(is_starship_enabled: bool) -> Result<Self, Fault> {
        let log_filename = "paper.log".to_string();

        Ok(Self {
            file: Arc::new(RwLock::new(
                File::create(&log_filename).map_err(|e| Fault::CreateFile(log_filename, e))?,
            )),
            is_starship_enabled,
        })
    }
}

impl Log for Logger {
    fn enabled(&self, metadata: &Metadata<'_>) -> bool {
        if metadata.target().starts_with("starship") {
            self.is_starship_enabled
        } else {
            true
        }
    }

    fn log(&self, record: &Record<'_>) {
        if self.enabled(record.metadata()) {
            if let Ok(mut file) = self.file.write() {
                #[allow(unused_must_use)] // Log::log() does not propagate error.
                {
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
            #[allow(unused_must_use)] // Log::flush() does not propagate error.
            {
                file.flush();
            }
        }
    }
}

/// Implements writing logs to a file.
#[derive(Clone, Copy, Debug)]
pub struct Config {
    /// Defines if logs from starship are written.
    is_starship_enabled: bool,
    /// The minimum level of logs to be written.
    level: LevelFilter,
}

impl Default for Config {
    #[inline]
    fn default() -> Self {
        Self {
            level: LevelFilter::Warn,
            is_starship_enabled: false,
        }
    }
}

impl From<&ArgMatches<'_>> for Config {
    #[inline]
    fn from(value: &ArgMatches<'_>) -> Self {
        Self {
            level: match value.occurrences_of("verbose") {
                0 => LevelFilter::Warn,
                1 => LevelFilter::Info,
                2 => LevelFilter::Debug,
                _ => LevelFilter::Trace,
            },
            is_starship_enabled: value.value_of("log") == Some("starship"),
        }
    }
}

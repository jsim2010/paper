//! Implements [`Consumer`] for configs.
use {
    core::{cell::Cell, fmt, time::Duration},
    log::LevelFilter,
    market::{GoodFinisher, Consumer, MpscConsumer, IntermediateConsumer, UnlimitedQueue},
    notify::{DebouncedEvent, RecommendedWatcher, RecursiveMode, Watcher},
    serde::Deserialize,
    std::{fs, io, path::PathBuf, sync::mpsc},
    thiserror::Error,
};

/// An error while creating a [`SettingConsumer`].
#[derive(Debug, Error)]
pub enum CreateSettingConsumerError {
    /// An error creating a [`Watcher`].
    #[error("")]
    CreateWatcher(#[source] notify::Error),
    /// An error beginning to watch a file.
    #[error("")]
    WatchFile(#[source] notify::Error),
    /// An error building the configuration.
    #[error("")]
    CreateConfiguration(#[from] CreateConfigurationError),
}

/// An error creating the [`Configuration`].
#[derive(Debug, Error)]
pub enum CreateConfigurationError {
    /// An error reading the config file.
    #[error("")]
    ReadFile(#[from] io::Error),
    /// An error deserializing the config file text.
    #[error("")]
    Deserialize(#[from] toml::de::Error),
}

/// The Change Filter.
pub(crate) struct SettingConsumer {
    /// Watches for events on the config file.
    #[allow(dead_code)] // Must keep ownership of watcher.
    watcher: RecommendedWatcher,
    /// The consumer of config file events.
    consumer: IntermediateConsumer<DebouncedEvent, <MpscConsumer<DebouncedEvent> as Consumer>::Error, SettingFinisher, Setting>,
}

impl SettingConsumer {
    /// Creates a new [`SettingConsumer`].
    pub(crate) fn new(path: &PathBuf) -> Result<Self, CreateSettingConsumerError> {
        let (event_tx, event_rx) = mpsc::channel();
        let mut watcher = notify::watcher(event_tx, Duration::from_secs(0)).map_err(CreateSettingConsumerError::CreateWatcher)?;
        let finisher = SettingFinisher::new(path)?;

        if path.is_file() {
            watcher.watch(path, RecursiveMode::NonRecursive).map_err(CreateSettingConsumerError::WatchFile)?;
        }

        Ok(Self {
            watcher,
            consumer: IntermediateConsumer::new(MpscConsumer::from(event_rx), finisher),
        })
    }
}

impl fmt::Debug for SettingConsumer {
    #[inline]
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "SettingConsumer {{consumer: {:?}}}",
            self.consumer
        )
    }
}

impl Consumer for SettingConsumer {
    type Good = Setting;
    type Error = <UnlimitedQueue<Setting> as Consumer>::Error;

    fn can_consume(&self) -> bool {
        self.consumer.can_consume()
    }

    fn consume(&self) -> Result<Self::Good, Self::Error> {
        self.consumer.consume()
    }
}

/// Manages the finishing of [`Setting`]s.
#[derive(Debug)]
struct SettingFinisher {
    /// The deserialization of the config file.
    config: Cell<Configuration>,
}

impl SettingFinisher {
    /// Creates a new [`SettingFinisher`].
    fn new(path: &PathBuf) -> Result<Self, CreateSettingConsumerError> {
        Ok(Self {
            config: Cell::new(Configuration::new(path).unwrap_or_default()),
        })
    }
}

impl GoodFinisher for SettingFinisher {
    type Intermediate = DebouncedEvent;
    type Final = Setting;

    fn finish(&self, intermediate_good: Self::Intermediate) -> Vec<Self::Final> {
        let mut finished_goods = Vec::new();

        if let DebouncedEvent::Write(file) = intermediate_good {
            let new_config = Configuration::new(&file).unwrap_or_default();
            let config = self.config.get();

            if config.wrap != new_config.wrap {
                finished_goods.push(Setting::Wrap(new_config.wrap.0));
            }

            if config.starship_log != new_config.starship_log {
                finished_goods.push(Setting::StarshipLog(new_config.starship_log.0));
            }

            self.config.set(new_config);
        }

        finished_goods
    }
}

/// The configuration of the application.
#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq)]
struct Configuration {
    /// If documents shall wrap.
    wrap: Wrap,
    /// The level filter of starship logs.
    starship_log: StarshipLog,
}

impl Configuration {
    /// Creates a new [`Configuration`].
    fn new(file: &PathBuf) -> Result<Self, CreateConfigurationError> {
        Ok(toml::from_str(&fs::read_to_string(file)?)?)
    }
}

/// If all documents shall wrap long text.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq)]
struct Wrap(bool);

impl Default for Wrap {
    fn default() -> Self {
        Self(false)
    }
}

/// The minimum level of logging the starship module.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq)]
struct StarshipLog(LevelFilter);

impl Default for StarshipLog {
    fn default() -> Self {
        Self(LevelFilter::Off)
    }
}

/// Signifies a configuration.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Setting {
    /// If the document shall wrap long text.
    Wrap(bool),
    /// The level at which starship records shall be logged.
    StarshipLog(LevelFilter),
}

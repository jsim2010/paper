//! Implements [`Consumer`] for configs.
use {
    core::{cell::Cell, fmt, time::Duration},
    market::{
        channel::MpscConsumer, ClosedMarketError, Consumer, Inspector, StripFrom,
        StrippingConsumer, VigilantConsumer,
    },
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

/// An error consuming [`Setting`]s.
#[derive(Copy, Clone, Debug, Error)]
pub enum ConsumeSettingError {
    /// Consume.
    #[error("")]
    Consume(
        #[source]
        <VigilantConsumer<
            StrippingConsumer<MpscConsumer<DebouncedEvent>, Setting>,
            SettingDeduplicator,
        > as Consumer>::Error,
    ),
}

/// The Change Filter.
pub(crate) struct SettingConsumer {
    /// Watches for events on the config file.
    #[allow(dead_code)] // Must keep ownership of watcher.
    watcher: RecommendedWatcher,
    /// The consumer of settings.
    consumer: VigilantConsumer<
        StrippingConsumer<MpscConsumer<DebouncedEvent>, Setting>,
        SettingDeduplicator,
    >,
}

impl SettingConsumer {
    /// Creates a new [`SettingConsumer`].
    pub(crate) fn new(path: &PathBuf) -> Result<Self, CreateSettingConsumerError> {
        let (event_tx, event_rx) = mpsc::channel();
        let mut watcher = notify::watcher(event_tx, Duration::from_secs(0))
            .map_err(CreateSettingConsumerError::CreateWatcher)?;

        if path.is_file() {
            watcher
                .watch(path, RecursiveMode::NonRecursive)
                .map_err(CreateSettingConsumerError::WatchFile)?;
        }

        Ok(Self {
            watcher,
            consumer: VigilantConsumer::new(
                StrippingConsumer::new(MpscConsumer::from(event_rx)),
                SettingDeduplicator::new(path),
            ),
        })
    }
}

impl fmt::Debug for SettingConsumer {
    #[inline]
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "SettingConsumer {{ .. }}")
    }
}

impl Consumer for SettingConsumer {
    type Good = Setting;
    type Error = ClosedMarketError;

    fn consume(&self) -> Result<Option<Self::Good>, Self::Error> {
        self.consumer.consume()
    }
}

impl StripFrom<DebouncedEvent> for Setting {
    #[inline]
    fn strip_from(good: &DebouncedEvent) -> Vec<Self> {
        let mut finished_goods = Vec::new();

        if let DebouncedEvent::Write(file) = good {
            if let Ok(config) = Configuration::new(file) {
                finished_goods.push(Self::Wrap(config.wrap.0));
            }
        }

        finished_goods
    }
}

/// Filters settings that already match the current configuration.
#[derive(Debug)]
pub struct SettingDeduplicator {
    /// The current configuration.
    config: Cell<Configuration>,
}

impl SettingDeduplicator {
    /// Creates a new [`SettingDeduplicator`].
    fn new(path: &PathBuf) -> Self {
        Self {
            config: Cell::new(Configuration::new(path).unwrap_or_default()),
        }
    }
}

impl Inspector for SettingDeduplicator {
    type Good = Setting;

    #[inline]
    fn allows(&self, good: &Self::Good) -> bool {
        let config = self.config.get();
        let mut new_config = config;
        let result;

        match good {
            Self::Good::Wrap(wrap) => {
                result = *wrap == config.wrap.0;
                new_config.wrap.0 = *wrap;
            }
        }

        self.config.set(new_config);
        result
    }
}

/// The configuration of the application.
#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq)]
struct Configuration {
    /// If documents shall wrap.
    wrap: Wrap,
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

/// Signifies a configuration.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Setting {
    /// If the document shall wrap long text.
    Wrap(bool),
}

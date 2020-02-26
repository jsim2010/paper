//! Implements [`Consumer`] for configs.
use {
    crate::{
        io::Input,
        market::{ConsumeError, Consumer, StdConsumer},
    },
    core::{cell::Cell, fmt, time::Duration},
    log::{warn, LevelFilter},
    notify::{DebouncedEvent, RecommendedWatcher, RecursiveMode, Watcher},
    serde::Deserialize,
    std::{fs, path::PathBuf, sync::mpsc},
};

pub(crate) struct ChangeFilter {
    /// The deserialization of the config file.
    config: Cell<Config>,
    /// Watches for events on the config file.
    #[allow(dead_code)] // Must keep ownership of watcher.
    watcher: Option<RecommendedWatcher>,
    /// Receives events generated by `watcher`.
    file_event_drain: StdConsumer<DebouncedEvent>,
    setting_tx: crossbeam_channel::Sender<Setting>,
    setting_rx: crossbeam_channel::Receiver<Setting>,
}

impl ChangeFilter {
    pub(crate) fn new(path: &PathBuf) -> Self {
        let (event_tx, event_rx) = mpsc::channel();
        let (setting_tx, setting_rx) = crossbeam_channel::unbounded();

        let (watcher, config) = if path.is_file() {
            let watcher = match notify::watcher(event_tx, Duration::from_secs(0)) {
                Ok(mut w) => {
                    if let Err(error) = w.watch(path, RecursiveMode::NonRecursive) {
                        warn!("unable to watch config file: {}", error);
                    }

                    Some(w)
                }
                Err(error) => {
                    warn!("unable to create config file watcher: {}", error);
                    None
                }
            };

            (watcher, Config::read(path))
        } else {
            (None, Config::default())
        };

        Self {
            config: Cell::new(config),
            watcher,
            file_event_drain: event_rx.into(),
            setting_tx,
            setting_rx,
        }
    }

    fn process(&self) {
        while self.file_event_drain.can_consume() {
            if let Some(DebouncedEvent::Write(config_file)) =
                self.file_event_drain.optional_consume().unwrap()
            {
                let new_config = Config::read(&config_file);

                if new_config.wrap != self.config.get().wrap {
                    self.setting_tx
                        .send(Setting::Wrap(new_config.wrap))
                        .unwrap();
                }

                if new_config.starship_log != self.config.get().starship_log {
                    self.setting_tx
                        .send(Setting::StarshipLog(new_config.starship_log))
                        .unwrap();
                }

                self.config.set(new_config);
            }
        }
    }
}

impl fmt::Debug for ChangeFilter {
    #[inline]
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "ChangeFilter {{config: {:?}, file_event_drain: {:?}, setting_tx: {:?}, setting_rx: {:?}}}", self.config, self.file_event_drain, self.setting_tx, self.setting_rx)
    }
}

impl Consumer for ChangeFilter {
    type Record = Input;

    fn can_consume(&self) -> bool {
        self.process();
        !self.setting_rx.is_empty()
    }

    fn consume(&self) -> Result<Self::Record, ConsumeError> {
        self.process();
        self.setting_rx
            .recv()
            .map(|setting| setting.into())
            .map_err(|_| ConsumeError)
    }
}

#[derive(Clone, Copy, Debug, Deserialize)]
struct Config {
    wrap: bool,
    starship_log: LevelFilter,
}

impl Config {
    fn read(config_file: &PathBuf) -> Self {
        toml::from_str(&fs::read_to_string(config_file).unwrap()).unwrap()
    }
}

impl Default for Config {
    fn default() -> Self {
        Self {
            wrap: false,
            starship_log: LevelFilter::Off,
        }
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

impl From<Setting> for Input {
    fn from(value: Setting) -> Self {
        Self::Config(value)
    }
}

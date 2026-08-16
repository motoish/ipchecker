use std::{
    fs::{self, OpenOptions},
    io::Write,
    net::Ipv4Addr,
    path::PathBuf,
    str::FromStr,
};

use serde::{Deserialize, Deserializer, Serialize};
use thiserror::Error;

pub const ALLOWED_INTERVAL_MINUTES: [u64; 5] = [1, 5, 15, 30, 60];

#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("configuration I/O failed: {0}")]
    Io(#[from] std::io::Error),
    #[error("configuration serialization failed: {0}")]
    Serialization(#[from] toml::ser::Error),
    #[error("interval {0} minutes is not allowed")]
    InvalidInterval(u64),
    #[error("application data directory is unavailable")]
    DataDirectoryUnavailable,
}

#[derive(Deserialize, Default)]
struct RawConfig {
    #[serde(default, deserialize_with = "recover_string")]
    expected_ip: Option<String>,
    #[serde(default, deserialize_with = "recover_u64")]
    interval_minutes: Option<u64>,
    #[serde(default, deserialize_with = "recover_bool")]
    show_network_speed: Option<bool>,
}

fn recover_string<'de, D>(deserializer: D) -> Result<Option<String>, D::Error>
where
    D: Deserializer<'de>,
{
    Ok(Option::<String>::deserialize(deserializer).ok().flatten())
}

fn recover_u64<'de, D>(deserializer: D) -> Result<Option<u64>, D::Error>
where
    D: Deserializer<'de>,
{
    Ok(Option::<u64>::deserialize(deserializer).ok().flatten())
}

fn recover_bool<'de, D>(deserializer: D) -> Result<Option<bool>, D::Error>
where
    D: Deserializer<'de>,
{
    Ok(Option::<bool>::deserialize(deserializer).ok().flatten())
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Config {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expected_ip: Option<Ipv4Addr>,
    pub interval_minutes: u64,
    #[serde(rename = "show_network_speed")]
    pub is_show_network_speed: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            expected_ip: None,
            interval_minutes: 5,
            is_show_network_speed: true,
        }
    }
}

impl Config {
    pub fn from_toml(input: &str) -> Self {
        let Ok(raw) = toml::from_str::<RawConfig>(input) else {
            return Self::default();
        };

        Self {
            expected_ip: raw
                .expected_ip
                .and_then(|value| Ipv4Addr::from_str(&value).ok()),
            interval_minutes: raw
                .interval_minutes
                .filter(|value| ALLOWED_INTERVAL_MINUTES.contains(value))
                .unwrap_or(5),
            is_show_network_speed: raw.show_network_speed.unwrap_or(true),
        }
    }

    pub fn to_toml(&self) -> Result<String, ConfigError> {
        toml::to_string(self).map_err(ConfigError::from)
    }

    pub fn set_interval(&mut self, interval_minutes: u64) -> Result<(), ConfigError> {
        if !ALLOWED_INTERVAL_MINUTES.contains(&interval_minutes) {
            return Err(ConfigError::InvalidInterval(interval_minutes));
        }

        self.interval_minutes = interval_minutes;
        Ok(())
    }
}

pub struct ConfigStore {
    path: PathBuf,
}

impl ConfigStore {
    pub fn new(path: PathBuf) -> Self {
        Self { path }
    }

    pub fn load_or_create(&self) -> Result<Config, ConfigError> {
        match fs::read_to_string(&self.path) {
            Ok(contents) => Ok(Config::from_toml(&contents)),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                let config = Config::default();
                self.save(&config)?;
                Ok(config)
            }
            Err(error) => Err(error.into()),
        }
    }

    pub fn save(&self, config: &Config) -> Result<(), ConfigError> {
        let contents = config.to_toml()?;
        if let Some(parent) = self.path.parent()
            && !parent.as_os_str().is_empty()
        {
            fs::create_dir_all(parent)?;
        }

        let temporary_path = PathBuf::from(format!("{}.tmp", self.path.display()));
        let mut temporary_file = OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open(&temporary_path)?;
        temporary_file.write_all(contents.as_bytes())?;
        temporary_file.flush()?;
        fs::rename(temporary_path, &self.path)?;
        Ok(())
    }

    pub fn default_path() -> Result<PathBuf, ConfigError> {
        dirs::data_dir()
            .map(|data_dir| data_dir.join("ipchecker").join("config.toml"))
            .ok_or(ConfigError::DataDirectoryUnavailable)
    }
}

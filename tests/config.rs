use ipchecker::config::{ALLOWED_INTERVAL_MINUTES, Config, ConfigStore};
use std::{net::Ipv4Addr, str::FromStr};
use tempfile::tempdir;

#[test]
fn defaults_to_five_minutes_without_expected_ip() {
    assert_eq!(Config::default().interval_minutes, 5);
    assert_eq!(Config::default().expected_ip, None);
    assert!(Config::default().is_show_network_speed);
    assert!(Config::default().is_show_network_latency);
}

#[test]
fn missing_show_network_speed_defaults_to_enabled() {
    let config = Config::from_toml("interval_minutes = 15\n");
    assert!(config.is_show_network_speed);
    assert_eq!(config.interval_minutes, 15);
}

#[test]
fn invalid_show_network_speed_recovers_to_enabled() {
    let config = Config::from_toml("show_network_speed = \"nope\"\ninterval_minutes = 30\n");
    assert!(config.is_show_network_speed);
    assert_eq!(config.interval_minutes, 30);
}

#[test]
fn missing_show_network_latency_defaults_to_enabled() {
    let config = Config::from_toml("interval_minutes = 15\n");
    assert!(config.is_show_network_latency);
}

#[test]
fn invalid_show_network_latency_recovers_to_enabled() {
    let config = Config::from_toml("show_network_latency = \"nope\"\ninterval_minutes = 30\n");
    assert!(config.is_show_network_latency);
    assert_eq!(config.interval_minutes, 30);
}

#[test]
fn accepts_only_documented_intervals() {
    for value in ALLOWED_INTERVAL_MINUTES {
        let mut config = Config::default();
        assert!(config.set_interval(value).is_ok());
    }
    let mut config = Config::default();
    assert!(config.set_interval(10).is_err());
    assert_eq!(config.interval_minutes, 5);
}

#[test]
fn invalid_fields_recover_independently() {
    let config = Config::from_toml("expected_ip = \"2001:db8::1\"\ninterval_minutes = 10\n");
    assert_eq!(config.expected_ip, None);
    assert_eq!(config.interval_minutes, 5);
}

#[test]
fn wrong_typed_fields_recover_independently() {
    let config = Config::from_toml("expected_ip = 42\ninterval_minutes = 15\n");
    assert_eq!(config.expected_ip, None);
    assert_eq!(config.interval_minutes, 15);
}

#[test]
fn valid_toml_round_trips() {
    let expected = Config {
        expected_ip: Some(Ipv4Addr::from_str("203.0.113.10").unwrap()),
        interval_minutes: 15,
        is_show_network_speed: false,
        is_show_network_latency: true,
    };
    assert_eq!(Config::from_toml(&expected.to_toml().unwrap()), expected);
}

#[test]
fn missing_file_is_created_with_defaults() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("nested/config.toml");
    let store = ConfigStore::new(path.clone());
    assert_eq!(store.load_or_create().unwrap(), Config::default());
    assert!(path.exists());
}

#[test]
fn default_path_ends_with_ipchecker_config_file() {
    let path = ConfigStore::default_path().unwrap();
    assert!(path.to_string_lossy().ends_with("ipchecker/config.toml"));
}

#[test]
fn save_overwrites_existing_configuration() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("config.toml");
    let store = ConfigStore::new(path.clone());
    let initial = Config {
        expected_ip: Some(Ipv4Addr::from_str("198.51.100.7").unwrap()),
        interval_minutes: 1,
        is_show_network_speed: true,
        is_show_network_latency: true,
    };
    let replacement = Config {
        expected_ip: Some(Ipv4Addr::from_str("203.0.113.8").unwrap()),
        interval_minutes: 60,
        is_show_network_speed: false,
        is_show_network_latency: false,
    };

    store.save(&initial).unwrap();
    store.save(&replacement).unwrap();

    let contents = std::fs::read_to_string(path).unwrap();
    assert_eq!(Config::from_toml(&contents), replacement);
}

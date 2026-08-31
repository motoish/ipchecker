#[macro_use]
extern crate rust_i18n;

rust_i18n::i18n!("locales", fallback = "en");

pub mod about;
pub mod app;
pub mod config;
pub mod i18n;
pub mod ip_input;
pub mod ip_source;
pub mod monitor;
pub mod net_latency;
pub mod net_metrics;
pub mod net_speed;
pub mod notification;
pub mod session;
pub mod ui;
pub mod update;
pub mod update_coordinator;
pub mod update_dialog;

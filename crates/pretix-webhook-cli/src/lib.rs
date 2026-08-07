#![doc = include_str!("../README.md")]

mod config;

pub use config::{Config, ConfigError, EffectiveConfig, EffectiveEndpoint};

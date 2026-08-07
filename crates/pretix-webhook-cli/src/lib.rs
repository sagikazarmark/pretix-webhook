//! Configuration for the `pretix-webhook` command.

mod config;

pub use config::{Config, ConfigError, EffectiveConfig, EffectiveEndpoint};

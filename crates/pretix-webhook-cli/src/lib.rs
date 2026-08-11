//! Configuration for the `pretix-webhook` command-line receiver.
//!
//! [`Config`] collects command-line and environment inputs;
//! [`into_effective`](Config::into_effective) resolves and validates them into
//! an [`EffectiveConfig`] before the listener binds. Two operating modes are
//! resolved here and cannot be mixed: simple mode exposes one endpoint from
//! flags and environment variables, while multi mode is selected only by an
//! explicit `--config` TOML file and exposes several endpoints beneath one
//! prefix.
//!
//! See the crate's README for the operator-facing guide to flags, environment
//! variables, and the TOML file.

mod config;

pub use config::{Config, ConfigError, EffectiveConfig, EffectiveEndpoint};

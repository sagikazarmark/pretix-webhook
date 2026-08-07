use std::{
    collections::HashSet,
    fmt::{Debug, Display, Formatter},
    net::SocketAddr,
    path::{Path, PathBuf},
    str::FromStr,
};

use clap::Parser;
use pretix_webhook::{
    BasicAuthCredential, WebhookConfig, resolve_webhook_path, validate_absolute_webhook_path,
    validate_webhook_prefix,
};
use serde::Deserialize;
use thiserror::Error;

#[derive(Clone)]
struct Credential(BasicAuthCredential);

impl FromStr for Credential {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        let (username, password) = value
            .split_once(':')
            .ok_or_else(|| "expected USERNAME:PASSWORD".to_owned())?;
        if username.is_empty() || password.is_empty() {
            return Err("username and password must be non-empty".to_owned());
        }
        Ok(Self(BasicAuthCredential::new(username, password)))
    }
}

impl Debug for Credential {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("Credential(REDACTED)")
    }
}

/// Command-line and environment configuration.
#[derive(Debug, Parser)]
#[command(name = "pretix-webhook", version, about)]
pub struct Config {
    /// Address on which to accept HTTP connections.
    #[arg(long, env = "PRETIX_WEBHOOK_BIND", default_value = "127.0.0.1:3000")]
    bind: SocketAddr,

    /// Load multiple webhook routes from this TOML file.
    #[arg(long = "config")]
    file: Option<PathBuf>,

    /// Override the global prefix used with --config.
    #[arg(
        long,
        env = "PRETIX_WEBHOOK_PREFIX",
        value_parser = parse_prefix
    )]
    prefix: Option<String>,

    /// Exact URL path at which the webhook is exposed.
    #[arg(
        long,
        env = "PRETIX_WEBHOOK_PATH",
        value_parser = parse_path
    )]
    path: Option<String>,

    /// Allowed organizer slug. May be supplied more than once.
    #[arg(
        long = "allow-organizer",
        env = "PRETIX_WEBHOOK_ALLOW_ORGANIZERS",
        value_delimiter = ';'
    )]
    allowed_organizers: Vec<String>,

    /// Allowed event slug. May be supplied more than once.
    #[arg(
        long = "allow-event",
        env = "PRETIX_WEBHOOK_ALLOW_EVENTS",
        value_delimiter = ';'
    )]
    allowed_events: Vec<String>,

    /// Accepted USERNAME:PASSWORD pair. May be supplied more than once.
    #[arg(
        long = "credential",
        env = "PRETIX_WEBHOOK_CREDENTIALS",
        value_delimiter = ';'
    )]
    credentials: Vec<Credential>,
}

impl Config {
    /// Returns the path supplied by a command-line flag or environment value.
    ///
    /// `None` means that loading will apply the simple-mode `/webhook` default.
    #[must_use]
    pub fn path_input(&self) -> Option<&str> {
        self.path.as_deref()
    }

    /// Returns the configuration file supplied explicitly on the command line.
    #[must_use]
    pub fn config_path_input(&self) -> Option<&Path> {
        self.file.as_deref()
    }

    /// Resolves and validates the complete effective process configuration.
    ///
    /// # Errors
    ///
    /// Returns [`ConfigError`] when operating-mode inputs are mixed, the TOML
    /// source cannot be loaded, or an endpoint is invalid.
    pub fn into_effective(self) -> Result<EffectiveConfig, ConfigError> {
        if let Some(config_path) = self.file.clone() {
            return self.into_multi_effective(&config_path);
        }
        self.into_simple_effective()
    }

    fn into_simple_effective(self) -> Result<EffectiveConfig, ConfigError> {
        if self.prefix.is_some() {
            return Err(ConfigError::MixedMode(
                "--prefix and PRETIX_WEBHOOK_PREFIX require --config".to_owned(),
            ));
        }

        let unrestricted = self.allowed_organizers.is_empty() && self.allowed_events.is_empty();
        let unauthenticated = self.credentials.is_empty();
        let mut webhook_config = WebhookConfig::new();
        for organizer in self.allowed_organizers {
            webhook_config = webhook_config
                .allow_organizer(organizer)
                .map_err(|error| ConfigError::InvalidSimple(error.to_string()))?;
        }
        for event in self.allowed_events {
            webhook_config = webhook_config
                .allow_event(event)
                .map_err(|error| ConfigError::InvalidSimple(error.to_string()))?;
        }
        if !unauthenticated {
            webhook_config = webhook_config
                .require_basic_auth(self.credentials.into_iter().map(|credential| credential.0));
        }

        Ok(EffectiveConfig {
            bind: self.bind,
            endpoints: vec![EffectiveEndpoint {
                path: self.path.unwrap_or_else(|| "/webhook".to_owned()),
                webhook_config,
                unrestricted,
                unauthenticated,
            }],
        })
    }

    fn into_multi_effective(self, config_path: &Path) -> Result<EffectiveConfig, ConfigError> {
        if self.path.is_some()
            || !self.allowed_organizers.is_empty()
            || !self.allowed_events.is_empty()
            || !self.credentials.is_empty()
        {
            return Err(ConfigError::MixedMode(
                "--config cannot be combined with simple endpoint path, filter, or credential inputs"
                    .to_owned(),
            ));
        }

        let source = std::fs::read_to_string(config_path).map_err(|source| ConfigError::Read {
            path: config_path.to_owned(),
            source,
        })?;
        let document: TomlConfig = toml::from_str(&source).map_err(|source| ConfigError::Toml {
            path: config_path.to_owned(),
            source,
        })?;
        if document.webhooks.is_empty() {
            return Err(invalid_toml_config(
                config_path,
                &"at least one [[webhooks]] entry is required",
            ));
        }

        let prefix = self
            .prefix
            .or(document.prefix)
            .unwrap_or_else(|| "/webhook".to_owned());
        validate_webhook_prefix(&prefix)
            .map_err(|error| invalid_toml_config(config_path, &error))?;

        let mut resolved_paths = HashSet::new();
        let mut endpoints = Vec::with_capacity(document.webhooks.len());
        for (index, webhook) in document.webhooks.into_iter().enumerate() {
            let route_number = index + 1;
            let path = resolve_webhook_path(&prefix, &webhook.path).map_err(|error| {
                invalid_toml_config(
                    config_path,
                    &format!("webhooks entry {route_number}: {error}"),
                )
            })?;
            if !resolved_paths.insert(path.clone()) {
                return Err(invalid_toml_config(
                    config_path,
                    &format!("webhooks entry {route_number}: duplicate webhook path {path:?}"),
                ));
            }
            let unrestricted =
                webhook.allow_organizers.is_empty() && webhook.allow_events.is_empty();
            let unauthenticated = webhook.credential_env.is_empty();
            let mut credentials = Vec::with_capacity(webhook.credential_env.len());
            for variable in webhook.credential_env {
                let value = std::env::var(&variable).map_err(|_| {
                    invalid_toml_config(
                        config_path,
                        &format!(
                            "webhooks entry {route_number} ({path:?}): credential environment variable {variable:?} is missing or not valid Unicode"
                        ),
                    )
                })?;
                let credential = value.parse::<Credential>().map_err(|error| {
                    invalid_toml_config(
                        config_path,
                        &format!(
                            "webhooks entry {route_number} ({path:?}): credential environment variable {variable:?} is invalid: {error}"
                        ),
                    )
                })?;
                credentials.push(credential.0);
            }

            let mut webhook_config = WebhookConfig::new();
            for organizer in webhook.allow_organizers {
                webhook_config = webhook_config.allow_organizer(organizer).map_err(|error| {
                    invalid_toml_config(
                        config_path,
                        &format!("webhooks entry {route_number} ({path:?}): {error}"),
                    )
                })?;
            }
            for event in webhook.allow_events {
                webhook_config = webhook_config.allow_event(event).map_err(|error| {
                    invalid_toml_config(
                        config_path,
                        &format!("webhooks entry {route_number} ({path:?}): {error}"),
                    )
                })?;
            }
            if !unauthenticated {
                webhook_config = webhook_config.require_basic_auth(credentials);
            }

            endpoints.push(EffectiveEndpoint {
                path,
                webhook_config,
                unrestricted,
                unauthenticated,
            });
        }

        Ok(EffectiveConfig {
            bind: self.bind,
            endpoints,
        })
    }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct TomlConfig {
    prefix: Option<String>,
    webhooks: Vec<TomlWebhook>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct TomlWebhook {
    path: String,
    #[serde(default)]
    allow_organizers: Vec<String>,
    #[serde(default)]
    allow_events: Vec<String>,
    #[serde(default)]
    credential_env: Vec<String>,
}

/// An invalid command-line, environment, or TOML configuration.
#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("simple and multi webhook settings cannot be mixed: {0}")]
    MixedMode(String),
    #[error("invalid simple webhook configuration: {0}")]
    InvalidSimple(String),
    #[error("could not read TOML config {path:?}: {source}")]
    Read {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("could not parse TOML config {path:?}: {source}")]
    Toml {
        path: PathBuf,
        #[source]
        source: toml::de::Error,
    },
    #[error("invalid TOML config {path:?}: {message}")]
    InvalidToml { path: PathBuf, message: String },
}

fn invalid_toml_config(path: &Path, message: &dyn Display) -> ConfigError {
    ConfigError::InvalidToml {
        path: path.to_owned(),
        message: message.to_string(),
    }
}

/// Fully resolved and validated process configuration.
#[derive(Clone, Debug)]
pub struct EffectiveConfig {
    bind: SocketAddr,
    endpoints: Vec<EffectiveEndpoint>,
}

impl EffectiveConfig {
    #[must_use]
    pub fn bind(&self) -> SocketAddr {
        self.bind
    }

    #[must_use]
    pub fn endpoints(&self) -> &[EffectiveEndpoint] {
        &self.endpoints
    }

    #[must_use]
    pub fn into_parts(self) -> (SocketAddr, Vec<EffectiveEndpoint>) {
        (self.bind, self.endpoints)
    }
}

/// One fully resolved and validated webhook endpoint.
#[derive(Clone, Debug)]
pub struct EffectiveEndpoint {
    path: String,
    webhook_config: WebhookConfig,
    unrestricted: bool,
    unauthenticated: bool,
}

impl EffectiveEndpoint {
    #[must_use]
    pub fn path(&self) -> &str {
        &self.path
    }

    #[must_use]
    pub fn is_unrestricted(&self) -> bool {
        self.unrestricted
    }

    #[must_use]
    pub fn is_unauthenticated(&self) -> bool {
        self.unauthenticated
    }

    #[must_use]
    pub fn into_parts(self) -> (String, WebhookConfig) {
        (self.path, self.webhook_config)
    }
}

fn parse_path(value: &str) -> Result<String, String> {
    validate_absolute_webhook_path(value).map_err(|error| error.to_string())?;
    Ok(value.to_owned())
}

fn parse_prefix(value: &str) -> Result<String, String> {
    validate_webhook_prefix(value).map_err(|error| error.to_string())?;
    Ok(value.to_owned())
}

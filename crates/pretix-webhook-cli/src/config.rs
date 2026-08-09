use std::{
    collections::{HashMap, HashSet},
    fmt::{Debug, Display, Formatter},
    net::SocketAddr,
    path::{Path, PathBuf},
    str::FromStr,
};

use clap::Parser;
use pretix_webhook::{
    BasicAuthCredential, WebhookConfig, resolve_webhook_path, validate_absolute_webhook_path,
    validate_relative_webhook_path, validate_webhook_prefix,
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
    #[arg(long, env = "PRETIX_WEBHOOK_PREFIX")]
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
        long_help = "Allowed organizer slug. May be supplied more than once.\n\nWhen the flag is absent, PRETIX_WEBHOOK_ALLOW_ORGANIZERS supplies a semicolon-separated list."
    )]
    allowed_organizers: Vec<String>,

    /// Allowed event slug. May be supplied more than once.
    #[arg(
        long = "allow-event",
        long_help = "Allowed event slug. May be supplied more than once.\n\nWhen the flag is absent, PRETIX_WEBHOOK_ALLOW_EVENTS supplies a semicolon-separated list."
    )]
    allowed_events: Vec<String>,

    /// Accepted USERNAME:PASSWORD pair. May be supplied more than once.
    #[arg(
        long = "credential",
        long_help = "Accepted USERNAME:PASSWORD pair. May be supplied more than once.\n\nWhen the flag is absent, PRETIX_WEBHOOK_CREDENTIALS supplies a semicolon-separated list."
    )]
    credentials: Vec<Credential>,
}

const ORGANIZERS_ENVIRONMENT: &str = "PRETIX_WEBHOOK_ALLOW_ORGANIZERS";
const EVENTS_ENVIRONMENT: &str = "PRETIX_WEBHOOK_ALLOW_EVENTS";
const CREDENTIALS_ENVIRONMENT: &str = "PRETIX_WEBHOOK_CREDENTIALS";

/// Reads one semicolon-separated list-valued environment variable.
///
/// Splitting happens here rather than through a Clap value delimiter so that
/// only the environment convention is semicolon-separated; a flag value is
/// always exactly one entry. An unset variable is an omitted list, while an
/// empty variable is a one-entry list so that an empty value stays a
/// configuration error instead of silently meaning "unrestricted".
fn semicolon_list(variable: &str) -> Result<Vec<String>, ConfigError> {
    match std::env::var(variable) {
        Ok(value) => Ok(value.split(';').map(str::to_owned).collect()),
        Err(std::env::VarError::NotPresent) => Ok(Vec::new()),
        Err(std::env::VarError::NotUnicode(_)) => Err(ConfigError::InvalidSimple(format!(
            "environment variable {variable:?} is not valid Unicode"
        ))),
    }
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
    pub fn into_effective(mut self) -> Result<EffectiveConfig, ConfigError> {
        if self.allowed_organizers.is_empty() {
            self.allowed_organizers = semicolon_list(ORGANIZERS_ENVIRONMENT)?;
        }
        if self.allowed_events.is_empty() {
            self.allowed_events = semicolon_list(EVENTS_ENVIRONMENT)?;
        }
        // Kept unparsed so that multi mode reports the mode conflict rather
        // than a credential error it would never have used.
        let environment_credentials = if self.credentials.is_empty() {
            semicolon_list(CREDENTIALS_ENVIRONMENT)?
        } else {
            Vec::new()
        };

        if let Some(config_path) = self.file.clone() {
            return self.into_multi_effective(&config_path, &environment_credentials);
        }
        self.into_simple_effective(environment_credentials)
    }

    fn into_simple_effective(
        mut self,
        environment_credentials: Vec<String>,
    ) -> Result<EffectiveConfig, ConfigError> {
        if self.prefix.is_some() {
            return Err(ConfigError::MixedMode(
                "--prefix and PRETIX_WEBHOOK_PREFIX require --config".to_owned(),
            ));
        }
        if contains_duplicates(&self.allowed_organizers) {
            return Err(ConfigError::InvalidSimple(
                "duplicate organizer slug".to_owned(),
            ));
        }
        if contains_duplicates(&self.allowed_events) {
            return Err(ConfigError::InvalidSimple(
                "duplicate event slug".to_owned(),
            ));
        }
        for value in environment_credentials {
            let credential = value.parse::<Credential>().map_err(|error| {
                ConfigError::InvalidSimple(format!(
                    "credential in {CREDENTIALS_ENVIRONMENT} is invalid: {error}"
                ))
            })?;
            self.credentials.push(credential);
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

    fn into_multi_effective(
        self,
        config_path: &Path,
        environment_credentials: &[String],
    ) -> Result<EffectiveConfig, ConfigError> {
        let document = read_toml_config(config_path)?;

        let mut errors = self.multi_mode_conflict_errors(environment_credentials);
        if document.webhooks.is_empty() {
            errors.push("at least one [[webhooks]] entry is required".to_owned());
        }

        let prefix = resolve_prefix(self.prefix, document.prefix.as_deref(), &mut errors);
        let prefix_is_valid = match validate_webhook_prefix(&prefix) {
            Ok(()) => true,
            Err(error) => {
                errors.push(error.to_string());
                false
            }
        };

        let mut route_paths = HashMap::new();
        let mut validated_webhooks = Vec::with_capacity(document.webhooks.len());
        for (index, webhook) in document.webhooks.into_iter().enumerate() {
            let route_number = index + 1;
            let relative_path_is_valid = match validate_relative_webhook_path(&webhook.path) {
                Ok(()) => true,
                Err(error) => {
                    errors.push(format!("webhooks entry {route_number}: {error}"));
                    false
                }
            };
            let path = (prefix_is_valid && relative_path_is_valid)
                .then(|| resolve_webhook_path(&prefix, &webhook.path).expect("validated paths"));
            if relative_path_is_valid {
                if let Some(error) = duplicate_route_error(
                    &mut route_paths,
                    &webhook.path,
                    path.as_deref(),
                    route_number,
                ) {
                    errors.push(error);
                }
            }

            if contains_duplicates(&webhook.allow_organizers) {
                errors.push(format!(
                    "webhooks entry {route_number}: duplicate organizer slug"
                ));
            }
            if contains_duplicates(&webhook.allow_events) {
                errors.push(format!(
                    "webhooks entry {route_number}: duplicate event slug"
                ));
            }
            if contains_duplicates(&webhook.credential_env) {
                errors.push(format!(
                    "webhooks entry {route_number}: duplicate credential environment-variable name"
                ));
            }

            let route = route_context(route_number, path.as_deref());
            for organizer in &webhook.allow_organizers {
                if let Err(error) = WebhookConfig::new().allow_organizer(organizer.as_str()) {
                    errors.push(format!("{route}: {error}"));
                }
            }
            for event in &webhook.allow_events {
                if let Err(error) = WebhookConfig::new().allow_event(event.as_str()) {
                    errors.push(format!("{route}: {error}"));
                }
            }

            let credentials = resolve_credentials(&webhook.credential_env, &route, &mut errors);

            validated_webhooks.push(ValidatedWebhook {
                webhook,
                path,
                credentials,
            });
        }

        if !errors.is_empty() {
            return Err(invalid_toml_config(
                config_path,
                &format_validation_errors(&errors),
            ));
        }

        let endpoints = build_effective_endpoints(validated_webhooks);

        Ok(EffectiveConfig {
            bind: self.bind,
            endpoints,
        })
    }

    fn multi_mode_conflict_errors(&self, environment_credentials: &[String]) -> Vec<String> {
        let mut errors = Vec::new();
        if self.path.is_some() {
            errors.push("simple webhook path input cannot be combined with --config".to_owned());
        }
        if !self.allowed_organizers.is_empty() {
            errors
                .push("simple organizer filter inputs cannot be combined with --config".to_owned());
        }
        if !self.allowed_events.is_empty() {
            errors.push("simple event filter inputs cannot be combined with --config".to_owned());
        }
        if !self.credentials.is_empty() || !environment_credentials.is_empty() {
            errors.push("simple credential inputs cannot be combined with --config".to_owned());
        }
        errors
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

fn read_toml_config(path: &Path) -> Result<TomlConfig, ConfigError> {
    let source = std::fs::read_to_string(path).map_err(|source| ConfigError::Read {
        path: path.to_owned(),
        source,
    })?;
    toml::from_str(&source).map_err(|source| ConfigError::Toml {
        path: path.to_owned(),
        source,
    })
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

/// Resolves the effective global prefix for multi mode.
///
/// The file's own prefix is validated even when an override wins, so that a
/// reusable configuration cannot rot unnoticed behind a deployment-specific
/// flag or variable.
fn resolve_prefix(
    overriding_prefix: Option<String>,
    file_prefix: Option<&str>,
    errors: &mut Vec<String>,
) -> String {
    let Some(overriding_prefix) = overriding_prefix else {
        return file_prefix.unwrap_or("/webhook").to_owned();
    };
    if let Some(file_prefix) = file_prefix {
        if let Err(error) = validate_webhook_prefix(file_prefix) {
            errors.push(format!("overridden TOML prefix is invalid: {error}"));
        }
    }
    overriding_prefix
}

/// Resolves one route's referenced credentials from the environment.
///
/// Diagnostics name the route and the variable but never its value.
fn resolve_credentials(
    variables: &[String],
    route: &str,
    errors: &mut Vec<String>,
) -> Vec<BasicAuthCredential> {
    let mut credentials = Vec::with_capacity(variables.len());
    for variable in variables {
        match std::env::var(variable) {
            Ok(value) => match value.parse::<Credential>() {
                Ok(credential) => credentials.push(credential.0),
                Err(error) => errors.push(format!(
                    "{route}: credential environment variable {variable:?} is invalid: {error}"
                )),
            },
            Err(_) => errors.push(format!(
                "{route}: credential environment variable {variable:?} is missing or not valid Unicode"
            )),
        }
    }
    credentials
}

fn contains_duplicates(values: &[String]) -> bool {
    let mut unique = HashSet::new();
    values.iter().any(|value| !unique.insert(value))
}

fn duplicate_route_error(
    routes: &mut HashMap<String, usize>,
    relative_path: &str,
    resolved_path: Option<&str>,
    route_number: usize,
) -> Option<String> {
    let Some(first_route) = routes.get(relative_path) else {
        routes.insert(relative_path.to_owned(), route_number);
        return None;
    };
    Some(resolved_path.map_or_else(
        || {
            format!(
                "webhooks entry {route_number}: duplicate webhook route (first used by entry {first_route}; resolved path unavailable because the prefix is invalid)"
            )
        },
        |path| {
            format!(
                "webhooks entry {route_number}: duplicate resolved webhook path {path:?} (first used by entry {first_route})"
            )
        },
    ))
}

fn route_context(route_number: usize, path: Option<&str>) -> String {
    path.map_or_else(
        || format!("webhooks entry {route_number}"),
        |path| format!("webhooks entry {route_number} ({path:?})"),
    )
}

fn format_validation_errors(errors: &[String]) -> String {
    format!(
        "configuration has semantic errors:\n- {}",
        errors.join("\n- ")
    )
}

struct ValidatedWebhook {
    webhook: TomlWebhook,
    path: Option<String>,
    credentials: Vec<BasicAuthCredential>,
}

fn build_effective_endpoints(webhooks: Vec<ValidatedWebhook>) -> Vec<EffectiveEndpoint> {
    webhooks
        .into_iter()
        .map(|validated| {
            let webhook = validated.webhook;
            let unrestricted =
                webhook.allow_organizers.is_empty() && webhook.allow_events.is_empty();
            let unauthenticated = webhook.credential_env.is_empty();
            let mut webhook_config = WebhookConfig::new();
            for organizer in webhook.allow_organizers {
                webhook_config = webhook_config
                    .allow_organizer(organizer)
                    .expect("organizer filters were validated");
            }
            for event in webhook.allow_events {
                webhook_config = webhook_config
                    .allow_event(event)
                    .expect("event filters were validated");
            }
            if !unauthenticated {
                webhook_config = webhook_config.require_basic_auth(validated.credentials);
            }

            EffectiveEndpoint {
                path: validated.path.expect("all routes were validated"),
                webhook_config,
                unrestricted,
                unauthenticated,
            }
        })
        .collect()
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

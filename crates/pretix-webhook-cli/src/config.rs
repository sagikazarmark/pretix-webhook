use std::{
    fmt::{Debug, Formatter},
    net::SocketAddr,
    str::FromStr,
};

use clap::Parser;
use pretix_webhook::{
    BasicAuthCredential, WebhookConfig, WebhookFilterError, validate_absolute_webhook_path,
};

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

    /// Resolves defaults and validates one effective simple-mode endpoint.
    ///
    /// # Errors
    ///
    /// Returns [`WebhookFilterError`] when an organizer or event value is empty
    /// or whitespace-padded.
    pub fn into_effective(self) -> Result<EffectiveConfig, WebhookFilterError> {
        let unrestricted = self.allowed_organizers.is_empty() && self.allowed_events.is_empty();
        let unauthenticated = self.credentials.is_empty();
        let mut webhook_config = WebhookConfig::new();
        for organizer in self.allowed_organizers {
            webhook_config = webhook_config.allow_organizer(organizer)?;
        }
        for event in self.allowed_events {
            webhook_config = webhook_config.allow_event(event)?;
        }
        if !unauthenticated {
            webhook_config = webhook_config
                .require_basic_auth(self.credentials.into_iter().map(|credential| credential.0));
        }

        Ok(EffectiveConfig {
            bind: self.bind,
            endpoint: EffectiveEndpoint {
                path: self.path.unwrap_or_else(|| "/webhook".to_owned()),
                webhook_config,
                unrestricted,
                unauthenticated,
            },
        })
    }
}

/// Fully resolved and validated process configuration.
#[derive(Clone, Debug)]
pub struct EffectiveConfig {
    bind: SocketAddr,
    endpoint: EffectiveEndpoint,
}

impl EffectiveConfig {
    #[must_use]
    pub fn bind(&self) -> SocketAddr {
        self.bind
    }

    #[must_use]
    pub fn endpoint(&self) -> &EffectiveEndpoint {
        &self.endpoint
    }

    #[must_use]
    pub fn into_parts(self) -> (SocketAddr, EffectiveEndpoint) {
        (self.bind, self.endpoint)
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

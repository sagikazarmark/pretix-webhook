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
        default_value = "/webhook",
        value_parser = parse_path
    )]
    path: String,

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
    #[must_use]
    pub fn bind(&self) -> SocketAddr {
        self.bind
    }

    #[must_use]
    pub fn path(&self) -> &str {
        &self.path
    }

    #[must_use]
    pub fn allowed_organizers(&self) -> &[String] {
        &self.allowed_organizers
    }

    #[must_use]
    pub fn allowed_events(&self) -> &[String] {
        &self.allowed_events
    }

    #[must_use]
    pub fn is_unrestricted(&self) -> bool {
        self.allowed_organizers.is_empty() && self.allowed_events.is_empty()
    }

    /// Builds the receiver policy represented by this configuration.
    ///
    /// # Errors
    ///
    /// Returns [`WebhookFilterError`] when an organizer or event value is empty
    /// or whitespace-padded.
    pub fn webhook_config(&self) -> Result<WebhookConfig, WebhookFilterError> {
        let mut config = WebhookConfig::new();
        for organizer in &self.allowed_organizers {
            config = config.allow_organizer(organizer)?;
        }
        for event in &self.allowed_events {
            config = config.allow_event(event)?;
        }

        if self.credentials.is_empty() {
            Ok(config)
        } else {
            Ok(config.require_basic_auth(
                self.credentials
                    .iter()
                    .map(|credential| credential.0.clone()),
            ))
        }
    }
}

fn parse_path(value: &str) -> Result<String, String> {
    validate_absolute_webhook_path(value).map_err(|error| error.to_string())?;
    Ok(value.to_owned())
}

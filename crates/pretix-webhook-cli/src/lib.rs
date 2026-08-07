//! Configuration for the `pretix-webhook` command.

use std::{
    fmt::{Debug, Formatter},
    net::SocketAddr,
    str::FromStr,
};

use clap::Parser;
use pretix_webhook::{BasicAuthCredential, WebhookConfig};

/// One organizer/event policy entry.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum AllowedTarget {
    Event { organizer: String, event: String },
    AllEvents { organizer: String },
}

impl FromStr for AllowedTarget {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        let (organizer, event) = value
            .split_once('/')
            .ok_or_else(|| "expected ORGANIZER/EVENT or ORGANIZER/*".to_owned())?;
        if organizer.is_empty() || event.is_empty() || event.contains('/') {
            return Err(
                "organizer and event slugs must be non-empty and contain no '/'".to_owned(),
            );
        }

        if event == "*" {
            Ok(Self::AllEvents {
                organizer: organizer.to_owned(),
            })
        } else {
            Ok(Self::Event {
                organizer: organizer.to_owned(),
                event: event.to_owned(),
            })
        }
    }
}

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

    /// Allowed ORGANIZER/EVENT pair; use ORGANIZER/* for all events.
    #[arg(long = "allow", env = "PRETIX_WEBHOOK_ALLOW", value_delimiter = ';')]
    allowed_targets: Vec<AllowedTarget>,

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
    pub fn allowed_targets(&self) -> &[AllowedTarget] {
        &self.allowed_targets
    }

    #[must_use]
    pub fn is_unrestricted(&self) -> bool {
        self.allowed_targets.is_empty()
    }

    #[must_use]
    pub fn webhook_config(&self) -> WebhookConfig {
        let mut config = if self.is_unrestricted() {
            WebhookConfig::new().allow_everything()
        } else {
            WebhookConfig::new()
        };
        for target in &self.allowed_targets {
            config = match target {
                AllowedTarget::Event { organizer, event } => config.allow_event(organizer, event),
                AllowedTarget::AllEvents { organizer } => config.allow_all_events(organizer),
            };
        }

        if self.credentials.is_empty() {
            config
        } else {
            config.require_basic_auth(
                self.credentials
                    .iter()
                    .map(|credential| credential.0.clone()),
            )
        }
    }
}

fn parse_path(value: &str) -> Result<String, String> {
    if !value.starts_with('/') || value.contains(['?', '#', '{', '}']) {
        return Err("path must be an absolute static URL path".to_owned());
    }
    Ok(value.to_owned())
}

use std::{
    collections::{BTreeMap, BTreeSet},
    fmt::{Debug, Formatter},
};

use axum::http::{HeaderMap, header};
use base64::{Engine as _, engine::general_purpose::STANDARD};
use pretix_webhook_events::WebhookEvent;
use sha2::{Digest, Sha256};
use subtle::{Choice, ConstantTimeEq};

/// A username/password pair accepted by HTTP Basic authentication.
#[derive(Clone)]
pub struct BasicAuthCredential {
    digest: [u8; 32],
}

impl BasicAuthCredential {
    #[must_use]
    pub fn new(username: impl AsRef<str>, password: impl AsRef<str>) -> Self {
        let mut hasher = Sha256::new();
        hasher.update(username.as_ref().as_bytes());
        hasher.update(b":");
        hasher.update(password.as_ref().as_bytes());
        Self {
            digest: hasher.finalize().into(),
        }
    }
}

impl Debug for BasicAuthCredential {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("BasicAuthCredential(REDACTED)")
    }
}

/// Authentication and organizer/event policy for a webhook endpoint.
#[derive(Clone, Debug, Default)]
pub struct WebhookConfig {
    organizers: BTreeMap<String, AllowedEvents>,
    unrestricted: bool,
    credentials: Vec<BasicAuthCredential>,
}

#[derive(Clone, Debug)]
enum AllowedEvents {
    All,
    Only(BTreeSet<String>),
}

impl Default for AllowedEvents {
    fn default() -> Self {
        Self::Only(BTreeSet::new())
    }
}

impl WebhookConfig {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Clears configured restrictions and allows every organizer and event.
    #[must_use]
    pub fn allow_everything(mut self) -> Self {
        self.organizers.clear();
        self.unrestricted = true;
        self
    }

    /// Allows organizer-level payloads and one event for an organizer.
    #[must_use]
    pub fn allow_event(mut self, organizer: impl Into<String>, event: impl Into<String>) -> Self {
        self.unrestricted = false;
        let events = self.organizers.entry(organizer.into()).or_default();
        if let AllowedEvents::Only(events) = events {
            events.insert(event.into());
        }
        self
    }

    /// Allows organizer-level payloads and every event for an organizer.
    #[must_use]
    pub fn allow_all_events(mut self, organizer: impl Into<String>) -> Self {
        self.unrestricted = false;
        self.organizers.insert(organizer.into(), AllowedEvents::All);
        self
    }

    /// Requires any one of the supplied credentials.
    #[must_use]
    pub fn require_basic_auth(
        mut self,
        credentials: impl IntoIterator<Item = BasicAuthCredential>,
    ) -> Self {
        self.credentials = credentials.into_iter().collect();
        self
    }

    pub(super) fn allows(&self, event: &WebhookEvent) -> bool {
        if self.unrestricted {
            return true;
        }
        let Some(organizer) = event.organizer_slug() else {
            return false;
        };
        let Some(events) = self.organizers.get(organizer) else {
            return false;
        };

        match (events, event.event_slug()) {
            (_, None) | (AllowedEvents::All, Some(_)) => true,
            (AllowedEvents::Only(allowed), Some(event)) => allowed.contains(event),
        }
    }

    pub(super) fn authenticates(&self, headers: &HeaderMap) -> bool {
        if self.credentials.is_empty() {
            return true;
        }

        let Some(encoded) = headers
            .get(header::AUTHORIZATION)
            .and_then(|value| value.to_str().ok())
            .and_then(|value| value.split_once(' '))
            .filter(|(scheme, _)| scheme.eq_ignore_ascii_case("basic"))
            .map(|(_, encoded)| encoded)
        else {
            return false;
        };
        let Ok(presented) = STANDARD.decode(encoded) else {
            return false;
        };
        let digest: [u8; 32] = Sha256::digest(presented).into();

        bool::from(
            self.credentials
                .iter()
                .fold(Choice::from(0), |matched, credential| {
                    matched | credential.digest.ct_eq(&digest)
                }),
        )
    }
}

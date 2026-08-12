use std::{
    collections::BTreeSet,
    fmt::{Debug, Display, Formatter},
};

use base64::{Engine as _, engine::general_purpose::STANDARD};
use http::{HeaderMap, header};
use pretix_webhook_events::WebhookEvent;
use sha2::{Digest, Sha256};
use subtle::{Choice, ConstantTimeEq};

use crate::service::{DEFAULT_BODY_LIMIT, WebhookService};

/// A username/password pair accepted by HTTP Basic authentication.
#[derive(Clone)]
pub struct BasicAuthCredential {
    digest: [u8; 32],
}

impl BasicAuthCredential {
    /// Creates a credential from the exact username and password bytes.
    ///
    /// HTTP Basic authentication uses the first colon as the username/password
    /// separator, so usernames should not contain `:`. Passwords may contain
    /// colons. Serve authenticated endpoints only through HTTPS or trusted TLS
    /// termination because HTTP Basic credentials are not encrypted.
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

/// Configures a [`WebhookService`].
#[derive(Clone)]
pub struct WebhookServiceBuilder {
    organizers: BTreeSet<String>,
    events: BTreeSet<String>,
    credentials: Vec<BasicAuthCredential>,
    body_limit: usize,
}

/// Reports how much policy is configured without disclosing any of it.
///
/// Configured slugs are policy, not payload data, so they are redacted for the
/// same reason [`BasicAuthCredential`] and [`WebhookFilterError`] are: a
/// derived `Debug` would place them in any diagnostic that renders a
/// configuration.
impl Debug for WebhookServiceBuilder {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("WebhookServiceBuilder")
            .field("organizers", &Redacted(self.organizers.len()))
            .field("events", &Redacted(self.events.len()))
            .field("credentials", &Redacted(self.credentials.len()))
            .field("body_limit", &self.body_limit)
            .finish()
    }
}

struct Redacted(usize);

impl Debug for Redacted {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "<{} REDACTED>", self.0)
    }
}

/// An invalid organizer or event filter value.
///
/// The rejected value is never included in the message so that diagnostics can
/// be reported without disclosing configured policy.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WebhookFilterError {
    message: String,
}

impl Display for WebhookFilterError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for WebhookFilterError {}

impl WebhookServiceBuilder {
    /// Creates a builder with no filters or authentication requirement.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Allows payloads from one organizer slug.
    ///
    /// # Errors
    ///
    /// Returns [`WebhookFilterError`] when `organizer` is empty or has leading
    /// or trailing whitespace.
    pub fn allow_organizer(
        mut self,
        organizer: impl Into<String>,
    ) -> Result<Self, WebhookFilterError> {
        self.organizers
            .insert(validate_filter("organizer", organizer.into())?);
        Ok(self)
    }

    /// Allows payloads for one event slug, independently of organizer filters.
    ///
    /// # Errors
    ///
    /// Returns [`WebhookFilterError`] when `event` is empty or has leading or
    /// trailing whitespace.
    pub fn allow_event(mut self, event: impl Into<String>) -> Result<Self, WebhookFilterError> {
        self.events.insert(validate_filter("event", event.into())?);
        Ok(self)
    }

    /// Requires any one of the supplied credentials.
    ///
    /// Passing an empty iterator disables authentication.
    #[must_use]
    pub fn require_basic_auth(
        mut self,
        credentials: impl IntoIterator<Item = BasicAuthCredential>,
    ) -> Self {
        self.credentials = credentials.into_iter().collect();
        self
    }

    /// Sets the maximum request body size in bytes.
    ///
    /// The default is [`DEFAULT_BODY_LIMIT`]. A request that exceeds the limit
    /// receives `413 Payload Too Large` without reaching the handler.
    #[must_use]
    pub fn body_limit(mut self, body_limit: usize) -> Self {
        self.body_limit = body_limit;
        self
    }

    /// Builds an HTTP webhook service around an event handler.
    pub fn build<H>(self, handler: H) -> WebhookService<H> {
        WebhookService::new(handler, self)
    }

    pub(super) fn allows(&self, event: &WebhookEvent) -> bool {
        (self.organizers.is_empty()
            || event
                .organizer_slug()
                .is_some_and(|organizer| self.organizers.contains(organizer)))
            && (self.events.is_empty()
                || !event.is_event_level()
                || event
                    .event_slug()
                    .is_some_and(|event| self.events.contains(event)))
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

    pub(super) fn body_limit_bytes(&self) -> usize {
        self.body_limit
    }
}

impl Default for WebhookServiceBuilder {
    fn default() -> Self {
        Self {
            organizers: BTreeSet::new(),
            events: BTreeSet::new(),
            credentials: Vec::new(),
            body_limit: DEFAULT_BODY_LIMIT,
        }
    }
}

fn validate_filter(kind: &str, value: String) -> Result<String, WebhookFilterError> {
    if value.is_empty() {
        return Err(WebhookFilterError {
            message: format!("invalid {kind} slug: it must not be empty"),
        });
    }

    if value.trim() != value {
        return Err(WebhookFilterError {
            message: format!(
                "invalid {kind} slug: leading and trailing whitespace are not allowed"
            ),
        });
    }

    Ok(value)
}

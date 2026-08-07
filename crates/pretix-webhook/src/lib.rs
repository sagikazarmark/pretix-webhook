//! Axum support for receiving pretix webhooks.

use std::{
    collections::{BTreeMap, BTreeSet},
    fmt::{Debug, Display, Formatter},
    future::Future,
};

use axum::{
    Router,
    body::Bytes,
    extract::State,
    http::{HeaderMap, StatusCode, header},
    response::{IntoResponse, Response},
    routing::post,
};
use base64::{Engine as _, engine::general_purpose::STANDARD};
use pretix_webhook_events::WebhookEvent;
use sha2::{Digest, Sha256};
use subtle::{Choice, ConstantTimeEq};

/// Processes accepted webhook events.
pub trait WebhookHandler: Clone + Send + Sync + 'static {
    type Error: Display + Send + Sync + 'static;

    fn handle(&self, event: WebhookEvent) -> impl Future<Output = Result<(), Self::Error>> + Send;
}

/// A handler that acknowledges and discards every event.
#[derive(Clone, Copy, Debug, Default)]
pub struct NoopHandler;

impl WebhookHandler for NoopHandler {
    type Error = std::convert::Infallible;

    async fn handle(&self, _event: WebhookEvent) -> Result<(), Self::Error> {
        Ok(())
    }
}

/// Adapts an async function or closure into a [`WebhookHandler`].
#[derive(Clone)]
pub struct FnHandler<F>(F);

/// Creates a handler from an async function or closure.
pub fn handler_fn<F>(function: F) -> FnHandler<F> {
    FnHandler(function)
}

impl<F, Fut, E> WebhookHandler for FnHandler<F>
where
    F: Fn(WebhookEvent) -> Fut + Clone + Send + Sync + 'static,
    Fut: Future<Output = Result<(), E>> + Send,
    E: Display + Send + Sync + 'static,
{
    type Error = E;

    fn handle(&self, event: WebhookEvent) -> impl Future<Output = Result<(), Self::Error>> + Send {
        (self.0)(event)
    }
}

/// A handler that logs complete events through the `log` facade.
#[cfg(feature = "log")]
#[derive(Clone, Copy, Debug, Default)]
pub struct LogHandler;

#[cfg(feature = "log")]
impl WebhookHandler for LogHandler {
    type Error = std::convert::Infallible;

    async fn handle(&self, event: WebhookEvent) -> Result<(), Self::Error> {
        log::info!("received pretix webhook: {event:?}");
        Ok(())
    }
}

/// A handler that emits a structured semantic tracing event.
#[cfg(feature = "tracing")]
#[derive(Clone, Copy, Debug, Default)]
pub struct TracingHandler;

#[cfg(feature = "tracing")]
impl WebhookHandler for TracingHandler {
    type Error = std::convert::Infallible;

    async fn handle(&self, event: WebhookEvent) -> Result<(), Self::Error> {
        tracing::info!(
            notification_id = event.notification_id(),
            action = event.action(),
            organizer = event.organizer_slug(),
            pretix_event = event.event_slug(),
            kind = ?event.kind(),
            "received pretix webhook"
        );
        Ok(())
    }
}

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

/// Organizer and event policy for a webhook endpoint.
#[derive(Clone, Debug, Default)]
pub struct WebhookConfig {
    organizers: BTreeMap<String, AllowedEvents>,
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

    /// Allows organizer-level payloads and one event for an organizer.
    #[must_use]
    pub fn allow_event(mut self, organizer: impl Into<String>, event: impl Into<String>) -> Self {
        let events = self.organizers.entry(organizer.into()).or_default();
        if let AllowedEvents::Only(events) = events {
            events.insert(event.into());
        }
        self
    }

    /// Allows organizer-level payloads and every event for an organizer.
    #[must_use]
    pub fn allow_all_events(mut self, organizer: impl Into<String>) -> Self {
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

    fn allows(&self, event: &WebhookEvent) -> bool {
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

    fn authenticates(&self, headers: &HeaderMap) -> bool {
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

#[derive(Clone)]
struct AppState<H> {
    handler: H,
    config: WebhookConfig,
}

/// Builds a router with a `POST /` webhook endpoint.
///
/// Nest the returned router to expose it at a different path.
pub fn webhook_router<H>(handler: H, config: WebhookConfig) -> Router
where
    H: WebhookHandler,
{
    webhook_router_at("/", handler, config)
}

/// Builds a router with a webhook endpoint at an exact path.
///
/// The path must be a valid static Axum route beginning with `/`.
pub fn webhook_router_at<H>(path: &str, handler: H, config: WebhookConfig) -> Router
where
    H: WebhookHandler,
{
    Router::new()
        .route(path, post(receive::<H>))
        .with_state(AppState { handler, config })
}

async fn receive<H>(State(state): State<AppState<H>>, headers: HeaderMap, body: Bytes) -> Response
where
    H: WebhookHandler,
{
    if !state.config.authenticates(&headers) {
        return (
            StatusCode::UNAUTHORIZED,
            [(header::WWW_AUTHENTICATE, "Basic realm=\"pretix-webhook\"")],
        )
            .into_response();
    }

    let Ok(event) = serde_json::from_slice::<WebhookEvent>(&body) else {
        return StatusCode::BAD_REQUEST.into_response();
    };

    if !state.config.allows(&event) {
        return StatusCode::NOT_FOUND.into_response();
    }

    match state.handler.handle(event).await {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(error) => {
            #[cfg(feature = "tracing")]
            tracing::error!(%error, "pretix webhook handler failed");
            #[cfg(all(feature = "log", not(feature = "tracing")))]
            log::error!("pretix webhook handler failed: {error}");
            #[cfg(not(any(feature = "log", feature = "tracing")))]
            let _ = error;
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}

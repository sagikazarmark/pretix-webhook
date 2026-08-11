use std::collections::HashSet;

use axum::{
    Router,
    body::Bytes,
    extract::State,
    http::{HeaderMap, StatusCode, header},
    response::{IntoResponse, Response},
    routing::post,
};
use pretix_webhook_events::WebhookEvent;

use crate::{
    config::WebhookConfig,
    handler::WebhookHandler,
    path::{
        WebhookPathError, resolve_webhook_path, validate_absolute_webhook_path,
        validate_webhook_prefix,
    },
};

#[derive(Clone)]
struct AppState<H> {
    handler: H,
    config: WebhookConfig,
    #[cfg(feature = "tracing")]
    route: Option<String>,
}

/// Builds multiple exact webhook endpoints at absolute paths.
///
/// Registering an already registered path returns an error, where merging two
/// Axum routers that share a route would panic. Use [`MultiWebhookRouter`]
/// instead when every path is relative to one shared prefix.
pub struct WebhookRouterBuilder {
    resolved_paths: HashSet<String>,
    router: Router,
}

impl WebhookRouterBuilder {
    /// Creates an empty builder.
    #[must_use]
    pub fn new() -> Self {
        Self {
            resolved_paths: HashSet::new(),
            router: Router::new(),
        }
    }

    /// Registers one independently configured webhook at an exact absolute path.
    ///
    /// Each call may use a different concrete handler and handler error type.
    ///
    /// # Errors
    ///
    /// Returns [`WebhookPathError`] when `path` is invalid or already
    /// registered.
    pub fn register_at<H>(
        self,
        path: &str,
        handler: H,
        config: WebhookConfig,
    ) -> Result<Self, WebhookPathError>
    where
        H: WebhookHandler,
    {
        validate_absolute_webhook_path(path)?;
        self.install(path, handler, config)
    }

    /// Finishes registration and returns an ordinary Axum router.
    pub fn finish(self) -> Router {
        self.router
    }

    fn install<H>(
        mut self,
        path: &str,
        handler: H,
        config: WebhookConfig,
    ) -> Result<Self, WebhookPathError>
    where
        H: WebhookHandler,
    {
        if !self.resolved_paths.insert(path.to_owned()) {
            return Err(WebhookPathError::duplicate(path));
        }

        self.router = self
            .router
            .merge(build_webhook_router(path, Some(path), handler, config));

        Ok(self)
    }
}

impl Default for WebhookRouterBuilder {
    fn default() -> Self {
        Self::new()
    }
}

/// Builds multiple exact webhook endpoints beneath one global URL prefix.
pub struct MultiWebhookRouter {
    prefix: String,
    builder: WebhookRouterBuilder,
}

impl MultiWebhookRouter {
    /// Creates an empty multi-webhook router with a validated global prefix.
    ///
    /// # Errors
    ///
    /// Returns [`WebhookPathError`] when `prefix` is invalid.
    pub fn new(prefix: impl Into<String>) -> Result<Self, WebhookPathError> {
        let prefix = prefix.into();
        validate_webhook_prefix(&prefix)?;

        Ok(Self {
            prefix,
            builder: WebhookRouterBuilder::new(),
        })
    }

    /// Registers one independently configured webhook at a relative path.
    ///
    /// Each call may use a different concrete handler and handler error type.
    ///
    /// # Errors
    ///
    /// Returns [`WebhookPathError`] when `relative_path` is invalid or resolves
    /// to an already registered path.
    pub fn register<H>(
        self,
        relative_path: &str,
        handler: H,
        config: WebhookConfig,
    ) -> Result<Self, WebhookPathError>
    where
        H: WebhookHandler,
    {
        let path = resolve_webhook_path(&self.prefix, relative_path)?;
        let Self { prefix, builder } = self;

        Ok(Self {
            prefix,
            builder: builder.install(&path, handler, config)?,
        })
    }

    /// Finishes registration and returns an ordinary Axum router.
    pub fn finish(self) -> Router {
        self.builder.finish()
    }
}

/// Builds a router with a `POST /` webhook endpoint.
///
/// Nest the returned router to expose it at a different path.
pub fn webhook_router<H>(handler: H, config: WebhookConfig) -> Router
where
    H: WebhookHandler,
{
    build_webhook_router("/", None, handler, config)
}

/// Builds a router with a webhook endpoint at an exact path.
///
/// Returns an error unless `path` is an absolute path containing only static,
/// URL-unreserved ASCII segments.
///
/// # Errors
///
/// Returns [`WebhookPathError`] when `path` is invalid.
pub fn webhook_router_at<H>(
    path: &str,
    handler: H,
    config: WebhookConfig,
) -> Result<Router, WebhookPathError>
where
    H: WebhookHandler,
{
    validate_absolute_webhook_path(path)?;

    Ok(build_webhook_router(path, Some(path), handler, config))
}

fn build_webhook_router<H>(
    path: &str,
    route: Option<&str>,
    handler: H,
    config: WebhookConfig,
) -> Router
where
    H: WebhookHandler,
{
    #[cfg(not(feature = "tracing"))]
    let _ = route;

    Router::new()
        .route(path, post(receive::<H>))
        .with_state(AppState {
            handler,
            config,
            #[cfg(feature = "tracing")]
            route: route.map(str::to_owned),
        })
}

async fn receive<H>(State(state): State<AppState<H>>, headers: HeaderMap, body: Bytes) -> Response
where
    H: WebhookHandler,
{
    // The span is created before the request is consumed so that everything the
    // handler emits inherits the route and the event's identity.
    #[cfg(feature = "tracing")]
    let span = request_span(state.route.as_deref());
    let response = respond(state, headers, body);
    #[cfg(feature = "tracing")]
    let response = tracing::Instrument::instrument(response, span);

    response.await
}

/// Opens the request span with the event's identity left empty until the
/// payload parses.
///
/// A `route` of `None` leaves the field unrecorded: [`webhook_router`] is meant
/// to be nested, so the path it is finally served at is not known here.
#[cfg(feature = "tracing")]
fn request_span(route: Option<&str>) -> tracing::Span {
    use tracing::field::Empty;

    tracing::info_span!(
        "pretix_webhook",
        route,
        notification_id = Empty,
        action = Empty,
        organizer = Empty,
        pretix_event = Empty,
        kind = Empty,
    )
}

#[cfg(feature = "tracing")]
fn record_identity(event: &WebhookEvent) {
    let span = tracing::Span::current();
    span.record("notification_id", event.notification_id());
    span.record("action", event.action());
    span.record("organizer", event.organizer_slug());
    span.record("pretix_event", event.event_slug());
    span.record("kind", tracing::field::debug(event.kind()));
}

async fn respond<H>(state: AppState<H>, headers: HeaderMap, body: Bytes) -> Response
where
    H: WebhookHandler,
{
    if !state.config.authenticates(&headers) {
        #[cfg(feature = "tracing")]
        tracing::warn!("rejected unauthenticated pretix webhook request");
        return (
            StatusCode::UNAUTHORIZED,
            [(header::WWW_AUTHENTICATE, "Basic realm=\"pretix-webhook\"")],
        )
            .into_response();
    }

    let event = match serde_json::from_slice::<WebhookEvent>(&body) {
        Ok(event) => event,
        Err(error) => {
            #[cfg(feature = "tracing")]
            tracing::warn!(%error, "rejected malformed pretix webhook payload");
            #[cfg(not(feature = "tracing"))]
            let _ = error;
            return StatusCode::BAD_REQUEST.into_response();
        }
    };

    #[cfg(feature = "tracing")]
    record_identity(&event);

    if !state.config.allows(&event) {
        #[cfg(feature = "tracing")]
        tracing::debug!("rejected filtered pretix webhook event");
        return StatusCode::NOT_FOUND.into_response();
    }

    // Emitted before dispatch so a handler that fails, panics, or hangs still
    // leaves a record that the event was accepted.
    #[cfg(feature = "tracing")]
    tracing::info!("received pretix webhook");

    match state.handler.handle(event).await {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(error) => {
            #[cfg(feature = "tracing")]
            tracing::error!(%error, "pretix webhook handler failed");
            #[cfg(not(feature = "tracing"))]
            let _ = error;
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}

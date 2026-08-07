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
    #[cfg(any(feature = "log", feature = "tracing"))]
    route: Option<String>,
}

/// Builds multiple exact webhook endpoints beneath one global URL prefix.
pub struct MultiWebhookRouter {
    prefix: String,
    resolved_paths: HashSet<String>,
    router: Router,
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
            resolved_paths: HashSet::new(),
            router: Router::new(),
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
        mut self,
        relative_path: &str,
        handler: H,
        config: WebhookConfig,
    ) -> Result<Self, WebhookPathError>
    where
        H: WebhookHandler,
    {
        let path = resolve_webhook_path(&self.prefix, relative_path)?;
        if !self.resolved_paths.insert(path.clone()) {
            return Err(WebhookPathError::duplicate(&path));
        }
        self.router = self
            .router
            .merge(build_webhook_router(&path, Some(&path), handler, config));
        Ok(self)
    }

    /// Finishes registration and returns an ordinary Axum router.
    pub fn finish(self) -> Router {
        self.router
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
    #[cfg(not(any(feature = "log", feature = "tracing")))]
    let _ = route;

    Router::new()
        .route(path, post(receive::<H>))
        .with_state(AppState {
            handler,
            config,
            #[cfg(any(feature = "log", feature = "tracing"))]
            route: route.map(str::to_owned),
        })
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
            if let Some(route) = state.route.as_deref() {
                tracing::error!(%error, %route, "pretix webhook handler failed");
            } else {
                tracing::error!(%error, "pretix webhook handler failed");
            }
            #[cfg(all(feature = "log", not(feature = "tracing")))]
            if let Some(route) = state.route.as_deref() {
                log::error!(route; "pretix webhook handler failed: {error}");
            } else {
                log::error!("pretix webhook handler failed: {error}");
            }
            #[cfg(not(any(feature = "log", feature = "tracing")))]
            let _ = error;
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}

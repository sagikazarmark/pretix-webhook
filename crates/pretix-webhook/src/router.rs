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
    path::{WebhookPathError, validate_absolute_webhook_path},
};

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
    build_webhook_router("/", handler, config)
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
    Ok(build_webhook_router(path, handler, config))
}

fn build_webhook_router<H>(path: &str, handler: H, config: WebhookConfig) -> Router
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

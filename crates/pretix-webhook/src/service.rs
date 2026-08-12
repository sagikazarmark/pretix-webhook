use std::{
    convert::Infallible,
    fmt::Display,
    future::Future,
    pin::Pin,
    sync::Arc,
    task::{Context, Poll},
};

use bytes::Bytes;
use http::{HeaderMap, Request, Response, StatusCode, header};
use http_body::Body;
use http_body_util::{BodyExt, Empty, LengthLimitError, Limited};
use pretix_webhook_events::WebhookEvent;
use tower::{BoxError, Service};

use crate::{builder::WebhookServiceBuilder, handler::WebhookHandler};

/// The maximum request body size used by [`WebhookServiceBuilder::new`].
pub const DEFAULT_BODY_LIMIT: usize = 2 * 1024 * 1024;

/// The empty HTTP response returned by [`WebhookService`].
pub type WebhookResponse = Response<Empty<Bytes>>;

type ResponseFuture =
    Pin<Box<dyn Future<Output = Result<WebhookResponse, Infallible>> + Send + 'static>>;

/// An HTTP service that authenticates and processes pretix webhooks.
///
/// Routing and HTTP method selection belong to the caller. The handler
/// receives only authenticated, parsed events that pass the configured filters.
/// Apply an outer Tower concurrency or load-shedding layer when accepted work
/// must be bounded.
pub struct WebhookService<H> {
    handler: Arc<H>,
    policy: WebhookServiceBuilder,
}

impl<H> WebhookService<H> {
    pub(crate) fn new(handler: H, policy: WebhookServiceBuilder) -> Self {
        Self {
            handler: Arc::new(handler),
            policy,
        }
    }
}

impl<H> Clone for WebhookService<H> {
    fn clone(&self) -> Self {
        Self {
            handler: Arc::clone(&self.handler),
            policy: self.policy.clone(),
        }
    }
}

impl<H, B> Service<Request<B>> for WebhookService<H>
where
    B: Body<Data = Bytes> + Send + 'static,
    B::Error: Into<BoxError>,
    H: WebhookHandler,
{
    type Response = WebhookResponse;
    type Error = Infallible;
    type Future = ResponseFuture;

    fn poll_ready(&mut self, _context: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, request: Request<B>) -> Self::Future {
        let policy = self.policy.clone();
        let handler = Arc::clone(&self.handler);
        let (parts, body) = request.into_parts();
        #[cfg(feature = "tracing")]
        let route = parts.uri.path().to_owned();

        Box::pin(async move {
            let body = match Limited::new(body, policy.body_limit_bytes())
                .collect()
                .await
            {
                Ok(body) => body.to_bytes(),
                Err(error) if error.is::<LengthLimitError>() => {
                    return Ok(empty_response(StatusCode::PAYLOAD_TOO_LARGE));
                }
                Err(_) => return Ok(empty_response(StatusCode::BAD_REQUEST)),
            };

            #[cfg(feature = "tracing")]
            let response = tracing::Instrument::instrument(
                respond(policy, handler, parts.headers, body),
                request_span(&route),
            )
            .await;
            #[cfg(not(feature = "tracing"))]
            let response = respond(policy, handler, parts.headers, body).await;

            Ok(response)
        })
    }
}

async fn respond<H>(
    policy: WebhookServiceBuilder,
    handler: Arc<H>,
    headers: HeaderMap,
    body: Bytes,
) -> WebhookResponse
where
    H: WebhookHandler,
{
    if !policy.authenticates(&headers) {
        #[cfg(feature = "tracing")]
        tracing::warn!("rejected unauthenticated pretix webhook request");
        let mut response = empty_response(StatusCode::UNAUTHORIZED);
        response.headers_mut().insert(
            header::WWW_AUTHENTICATE,
            http::HeaderValue::from_static("Basic realm=\"pretix-webhook\""),
        );
        return response;
    }

    let event = match serde_json::from_slice::<WebhookEvent>(&body) {
        Ok(event) => event,
        Err(error) => {
            #[cfg(feature = "tracing")]
            tracing::warn!(%error, "rejected malformed pretix webhook payload");
            #[cfg(not(feature = "tracing"))]
            let _ = error;
            return empty_response(StatusCode::BAD_REQUEST);
        }
    };

    #[cfg(feature = "tracing")]
    record_identity(&event);

    if !policy.allows(&event) {
        #[cfg(feature = "tracing")]
        tracing::debug!("rejected filtered pretix webhook event");
        return empty_response(StatusCode::NOT_FOUND);
    }

    #[cfg(feature = "tracing")]
    tracing::info!("received pretix webhook");

    match handler.handle(event).await {
        Ok(()) => empty_response(StatusCode::NO_CONTENT),
        Err(error) => failed_response(error),
    }
}

fn failed_response(error: impl Display) -> WebhookResponse {
    #[cfg(feature = "tracing")]
    tracing::error!(%error, "pretix webhook handler failed");
    #[cfg(not(feature = "tracing"))]
    let _ = error;
    empty_response(StatusCode::INTERNAL_SERVER_ERROR)
}

fn empty_response(status: StatusCode) -> WebhookResponse {
    let mut response = Response::new(Empty::new());
    *response.status_mut() = status;
    response
}

#[cfg(feature = "tracing")]
fn request_span(route: &str) -> tracing::Span {
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

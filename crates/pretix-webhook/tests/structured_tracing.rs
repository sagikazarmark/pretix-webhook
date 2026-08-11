#![cfg(feature = "tracing")]

use std::{
    future::Future,
    io::Write,
    sync::{Arc, Mutex},
};

use axum::{
    Router,
    body::Body,
    http::{Request, StatusCode},
};
use pretix_webhook::{
    BasicAuthCredential, NoopHandler, WebhookConfig, handler_fn, webhook_router, webhook_router_at,
};
use serde_json::{Value, json};
use tower::ServiceExt;
use tracing::Level;
use tracing_subscriber::fmt::MakeWriter;

#[derive(Clone, Default)]
struct CapturedOutput(Arc<Mutex<Vec<u8>>>);

impl Write for CapturedOutput {
    fn write(&mut self, buffer: &[u8]) -> std::io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(buffer);
        Ok(buffer.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

impl<'writer> MakeWriter<'writer> for CapturedOutput {
    type Writer = Self;

    fn make_writer(&'writer self) -> Self::Writer {
        self.clone()
    }
}

/// Runs `work` under a JSON subscriber and returns one record per emitted event.
async fn capture<F, Fut>(work: F) -> Vec<Value>
where
    F: FnOnce() -> Fut,
    Fut: Future<Output = ()>,
{
    let output = CapturedOutput::default();
    let subscriber = tracing_subscriber::fmt()
        .json()
        .without_time()
        .with_target(false)
        .with_max_level(Level::DEBUG)
        .with_writer(output.clone())
        .finish();
    let _subscriber = tracing::subscriber::set_default(subscriber);

    work().await;

    let bytes = output.0.lock().unwrap().clone();
    String::from_utf8(bytes)
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect()
}

fn webhook_request(path: &str) -> Request<Body> {
    Request::post(path)
        .body(Body::from(
            r#"{
                "notification_id": 42,
                "organizer": "acmecorp",
                "event": "democon",
                "action": "pretix.event.changed"
            }"#,
        ))
        .unwrap()
}

async fn post(app: Router, request: Request<Body>) -> StatusCode {
    app.oneshot(request).await.unwrap().status()
}

fn identified_span() -> Value {
    json!({
        "name": "pretix_webhook",
        "route": "/hooks/pretix",
        "notification_id": 42,
        "action": "pretix.event.changed",
        "organizer": "acmecorp",
        "pretix_event": "democon",
        "kind": "Event",
    })
}

#[tokio::test]
async fn accepted_events_are_recorded_on_the_request_span() {
    let records = capture(|| async {
        let app = webhook_router_at("/hooks/pretix", NoopHandler, WebhookConfig::new()).unwrap();
        assert_eq!(
            post(app, webhook_request("/hooks/pretix")).await,
            StatusCode::NO_CONTENT
        );
    })
    .await;

    assert_eq!(records.len(), 1);
    assert_eq!(records[0]["level"], "INFO");
    assert_eq!(
        records[0]["fields"],
        json!({"message": "received pretix webhook"})
    );
    assert_eq!(records[0]["span"], identified_span());
}

#[tokio::test]
async fn handler_output_inherits_the_request_span() {
    let records = capture(|| async {
        let handler = handler_fn(|_event| async {
            tracing::info!("dispatched to fulfilment");
            Ok::<_, std::convert::Infallible>(())
        });
        let app = webhook_router_at("/hooks/pretix", handler, WebhookConfig::new()).unwrap();
        assert_eq!(
            post(app, webhook_request("/hooks/pretix")).await,
            StatusCode::NO_CONTENT
        );
    })
    .await;

    assert_eq!(records.len(), 2);
    assert_eq!(
        records[1]["fields"],
        json!({"message": "dispatched to fulfilment"})
    );
    // The point of instrumenting natively: a handler's own output carries the
    // route and the event's identity without the handler knowing about either.
    assert_eq!(records[1]["span"], identified_span());
}

#[tokio::test]
async fn a_nested_router_records_no_route() {
    let records = capture(|| async {
        let app = webhook_router(NoopHandler, WebhookConfig::new());
        assert_eq!(
            post(app, webhook_request("/")).await,
            StatusCode::NO_CONTENT
        );
    })
    .await;

    assert_eq!(records.len(), 1);
    assert_eq!(
        records[0]["span"],
        json!({
            "name": "pretix_webhook",
            "notification_id": 42,
            "action": "pretix.event.changed",
            "organizer": "acmecorp",
            "pretix_event": "democon",
            "kind": "Event",
        })
    );
}

#[tokio::test]
async fn handler_failures_are_recorded_with_the_event_identity() {
    let records = capture(|| async {
        let handler = handler_fn(|_event| async { Err::<(), _>("downstream unavailable") });
        let app = webhook_router_at("/hooks/pretix", handler, WebhookConfig::new()).unwrap();
        assert_eq!(
            post(app, webhook_request("/hooks/pretix")).await,
            StatusCode::INTERNAL_SERVER_ERROR
        );
    })
    .await;

    assert_eq!(records.len(), 2);
    assert_eq!(records[1]["level"], "ERROR");
    assert_eq!(
        records[1]["fields"],
        json!({
            "message": "pretix webhook handler failed",
            "error": "downstream unavailable",
        })
    );
    assert_eq!(records[1]["span"], identified_span());
}

#[tokio::test]
async fn rejected_requests_are_recorded_before_the_identity_is_known() {
    let unauthenticated = capture(|| async {
        let config = WebhookConfig::new()
            .require_basic_auth([BasicAuthCredential::new("webhook", "secret")]);
        let app = webhook_router_at("/hooks/pretix", NoopHandler, config).unwrap();
        assert_eq!(
            post(app, webhook_request("/hooks/pretix")).await,
            StatusCode::UNAUTHORIZED
        );
    })
    .await;

    assert_eq!(unauthenticated.len(), 1);
    assert_eq!(unauthenticated[0]["level"], "WARN");
    assert_eq!(
        unauthenticated[0]["fields"],
        json!({"message": "rejected unauthenticated pretix webhook request"})
    );
    assert_eq!(
        unauthenticated[0]["span"],
        json!({"name": "pretix_webhook", "route": "/hooks/pretix"})
    );

    let malformed = capture(|| async {
        let app = webhook_router_at("/hooks/pretix", NoopHandler, WebhookConfig::new()).unwrap();
        let request = Request::post("/hooks/pretix")
            .body(Body::from("{}"))
            .unwrap();
        assert_eq!(post(app, request).await, StatusCode::BAD_REQUEST);
    })
    .await;

    assert_eq!(malformed.len(), 1);
    assert_eq!(malformed[0]["level"], "WARN");
    assert_eq!(
        malformed[0]["fields"]["message"],
        "rejected malformed pretix webhook payload"
    );
    assert!(malformed[0]["fields"]["error"].is_string());
    assert_eq!(
        malformed[0]["span"],
        json!({"name": "pretix_webhook", "route": "/hooks/pretix"})
    );
}

#[tokio::test]
async fn routing_and_extraction_rejections_emit_no_request_records() {
    let records = capture(|| async {
        let app = webhook_router_at("/hooks/pretix", NoopHandler, WebhookConfig::new()).unwrap();
        let unsupported_method = Request::get("/hooks/pretix").body(Body::empty()).unwrap();
        assert_eq!(
            post(app.clone(), unsupported_method).await,
            StatusCode::METHOD_NOT_ALLOWED
        );

        let oversized = Request::post("/hooks/pretix")
            .body(Body::from(vec![b' '; 2 * 1024 * 1024 + 1]))
            .unwrap();
        assert_eq!(post(app, oversized).await, StatusCode::PAYLOAD_TOO_LARGE);
    })
    .await;

    assert!(records.is_empty());
}

#[tokio::test]
async fn filtered_events_are_recorded_at_debug() {
    let records = capture(|| async {
        let config = WebhookConfig::new().allow_organizer("othercorp").unwrap();
        let app = webhook_router_at("/hooks/pretix", NoopHandler, config).unwrap();
        assert_eq!(
            post(app, webhook_request("/hooks/pretix")).await,
            StatusCode::NOT_FOUND
        );
    })
    .await;

    assert_eq!(records.len(), 1);
    assert_eq!(records[0]["level"], "DEBUG");
    assert_eq!(
        records[0]["fields"],
        json!({"message": "rejected filtered pretix webhook event"})
    );
    // Filtering happens after the payload parses, so the identity is known.
    assert_eq!(records[0]["span"], identified_span());
}

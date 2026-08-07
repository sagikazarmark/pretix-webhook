#![cfg(feature = "tracing")]

use std::{
    io::Write,
    sync::{Arc, Mutex},
};

use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use pretix_webhook::{
    TracingHandler, WebhookConfig, WebhookHandler, handler_fn, webhook_router, webhook_router_at,
};
use pretix_webhook_events::WebhookEvent;
use serde_json::Value;
use tower::ServiceExt;
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

#[tokio::test]
async fn tracing_handler_emits_semantic_fields_with_optional_route_identity() {
    let output = CapturedOutput::default();
    let subscriber = tracing_subscriber::fmt()
        .json()
        .without_time()
        .with_target(false)
        .with_level(false)
        .with_writer(output.clone())
        .finish();
    let _subscriber = tracing::subscriber::set_default(subscriber);
    let event: WebhookEvent = serde_json::from_str(
        r#"{
            "notification_id": 42,
            "organizer": "acmecorp",
            "event": "democon",
            "action": "pretix.event.changed"
        }"#,
    )
    .unwrap();

    TracingHandler::with_route("/hooks/pretix")
        .unwrap()
        .handle(event.clone())
        .await
        .unwrap();
    TracingHandler.handle(event).await.unwrap();

    let failing_handler = handler_fn(|_event| async { Err::<(), _>("downstream unavailable") });
    let routed_app = webhook_router_at(
        "/hooks/pretix",
        failing_handler.clone(),
        WebhookConfig::new(),
    )
    .unwrap();
    let route_less_app = webhook_router(failing_handler, WebhookConfig::new());
    let response = routed_app
        .oneshot(failing_request("/hooks/pretix"))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
    let response = route_less_app.oneshot(failing_request("/")).await.unwrap();
    assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);

    let bytes = output.0.lock().unwrap().clone();
    let records: Vec<Value> = String::from_utf8(bytes)
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    assert_eq!(records.len(), 4);
    assert_eq!(
        records[0]["fields"],
        serde_json::json!({
            "message": "received pretix webhook",
            "notification_id": 42,
            "action": "pretix.event.changed",
            "organizer": "acmecorp",
            "pretix_event": "democon",
            "kind": "Event",
            "route": "/hooks/pretix",
        })
    );
    assert_eq!(
        records[1]["fields"],
        serde_json::json!({
            "message": "received pretix webhook",
            "notification_id": 42,
            "action": "pretix.event.changed",
            "organizer": "acmecorp",
            "pretix_event": "democon",
            "kind": "Event",
        })
    );
    assert_eq!(
        records[2]["fields"],
        serde_json::json!({
            "message": "pretix webhook handler failed",
            "error": "downstream unavailable",
            "route": "/hooks/pretix",
        })
    );
    assert_eq!(
        records[3]["fields"],
        serde_json::json!({
            "message": "pretix webhook handler failed",
            "error": "downstream unavailable",
        })
    );
}

fn failing_request(path: &str) -> Request<Body> {
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

use std::{
    convert::Infallible,
    sync::{Arc, Mutex},
};

use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use pretix_webhook::{
    BasicAuthCredential, WebhookConfig, WebhookHandler, webhook_router, webhook_router_at,
};
use pretix_webhook_events::WebhookEvent;
use tower::ServiceExt;

#[derive(Clone, Default)]
struct RecordingHandler {
    events: Arc<Mutex<Vec<WebhookEvent>>>,
}

impl WebhookHandler for RecordingHandler {
    type Error = Infallible;

    async fn handle(&self, event: WebhookEvent) -> Result<(), Self::Error> {
        self.events.lock().unwrap().push(event);
        Ok(())
    }
}

#[tokio::test]
async fn accepted_webhook_is_delivered_to_the_handler() {
    let handler = RecordingHandler::default();
    let recorded = Arc::clone(&handler.events);
    let app = webhook_router(
        handler,
        WebhookConfig::new().allow_event("acmecorp", "democon"),
    );
    let request = Request::post("/")
        .header("content-type", "application/json")
        .body(Body::from(
            r#"{
                "notification_id": 123455,
                "organizer": "acmecorp",
                "event": "democon",
                "code": "ABC23",
                "action": "pretix.event.order.placed"
            }"#,
        ))
        .unwrap();

    let response = app.oneshot(request).await.unwrap();

    assert_eq!(response.status(), StatusCode::NO_CONTENT);
    let events = recorded.lock().unwrap();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].notification_id(), 123_455);
}

#[tokio::test]
async fn unsupported_organizers_and_events_are_hidden_with_not_found() {
    let app = webhook_router(
        RecordingHandler::default(),
        WebhookConfig::new().allow_event("acmecorp", "democon"),
    );

    for (organizer, event) in [("other", "democon"), ("acmecorp", "other")] {
        let payload = format!(
            r#"{{
                "notification_id": 1,
                "organizer": "{organizer}",
                "event": "{event}",
                "code": "ABC23",
                "action": "pretix.event.order.paid"
            }}"#
        );
        let request = Request::post("/")
            .header("content-type", "application/json")
            .body(Body::from(payload))
            .unwrap();

        let response = app.clone().oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }
}

#[tokio::test]
async fn explicit_unrestricted_policy_accepts_every_organizer_and_event() {
    let app = webhook_router(
        RecordingHandler::default(),
        WebhookConfig::new().allow_everything(),
    );
    let request = Request::post("/")
        .header("content-type", "application/json")
        .body(Body::from(
            r#"{
                "notification_id": 1,
                "organizer": "previously-unknown",
                "event": "future-event",
                "action": "pretix.event.changed"
            }"#,
        ))
        .unwrap();

    let response = app.oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::NO_CONTENT);
}

#[tokio::test]
async fn restrictions_added_after_allow_everything_replace_prior_restrictions() {
    let app = webhook_router(
        RecordingHandler::default(),
        WebhookConfig::new()
            .allow_event("old", "old")
            .allow_everything()
            .allow_event("new", "new"),
    );

    for (organizer, event, expected) in [
        ("old", "old", StatusCode::NOT_FOUND),
        ("new", "new", StatusCode::NO_CONTENT),
    ] {
        let payload = format!(
            r#"{{
                "notification_id": 1,
                "organizer": "{organizer}",
                "event": "{event}",
                "action": "pretix.event.changed"
            }}"#
        );
        let request = Request::post("/").body(Body::from(payload)).unwrap();
        let response = app.clone().oneshot(request).await.unwrap();
        assert_eq!(response.status(), expected);
    }
}

#[tokio::test]
async fn organizer_can_allow_all_current_and_future_events() {
    let app = webhook_router(
        RecordingHandler::default(),
        WebhookConfig::new().allow_all_events("acmecorp"),
    );
    let request = Request::post("/")
        .header("content-type", "application/json")
        .body(Body::from(
            r#"{
                "notification_id": 1,
                "organizer": "acmecorp",
                "event": "future-event",
                "action": "pretix.event.changed"
            }"#,
        ))
        .unwrap();

    let response = app.oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::NO_CONTENT);
}

#[tokio::test]
async fn any_configured_basic_auth_credential_is_accepted() {
    let app = webhook_router(
        RecordingHandler::default(),
        WebhookConfig::new()
            .allow_event("acmecorp", "democon")
            .require_basic_auth([
                BasicAuthCredential::new("old", "secret"),
                BasicAuthCredential::new("current", "new-secret"),
            ]),
    );
    let payload = r#"{
        "notification_id": 1,
        "organizer": "acmecorp",
        "event": "democon",
        "action": "pretix.event.changed"
    }"#;

    for authorization in [None, Some("Basic d3Jvbmc6d3Jvbmc=")] {
        let mut request = Request::post("/")
            .header("content-type", "application/json")
            .body(Body::from(payload))
            .unwrap();
        if let Some(authorization) = authorization {
            request
                .headers_mut()
                .insert("authorization", authorization.parse().unwrap());
        }

        let response = app.clone().oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
        assert_eq!(
            response.headers().get("www-authenticate").unwrap(),
            "Basic realm=\"pretix-webhook\""
        );
    }

    for authorization in ["Basic b2xkOnNlY3JldA==", "Basic Y3VycmVudDpuZXctc2VjcmV0"] {
        let request = Request::post("/")
            .header("content-type", "application/json")
            .header("authorization", authorization)
            .body(Body::from(payload))
            .unwrap();

        let response = app.clone().oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::NO_CONTENT);
    }
}

#[tokio::test]
async fn malformed_payload_returns_bad_request() {
    let app = webhook_router(
        RecordingHandler::default(),
        WebhookConfig::new().allow_all_events("acmecorp"),
    );
    let request = Request::post("/")
        .header("content-type", "application/json")
        .body(Body::from("not json"))
        .unwrap();

    let response = app.oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[derive(Clone)]
struct FailingHandler;

impl WebhookHandler for FailingHandler {
    type Error = &'static str;

    async fn handle(&self, _event: WebhookEvent) -> Result<(), Self::Error> {
        Err("downstream unavailable")
    }
}

#[tokio::test]
async fn handler_failure_returns_retryable_server_error() {
    let app = webhook_router(
        FailingHandler,
        WebhookConfig::new().allow_event("acmecorp", "democon"),
    );
    let request = Request::post("/")
        .header("content-type", "application/json")
        .body(Body::from(
            r#"{
                "notification_id": 1,
                "organizer": "acmecorp",
                "event": "democon",
                "action": "pretix.event.changed"
            }"#,
        ))
        .unwrap();

    let response = app.oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
}

#[tokio::test]
async fn router_can_mount_the_endpoint_at_an_exact_path() {
    let app = webhook_router_at(
        "/hooks/pretix",
        RecordingHandler::default(),
        WebhookConfig::new().allow_event("acmecorp", "democon"),
    );
    let request = Request::post("/hooks/pretix")
        .header("content-type", "application/json")
        .body(Body::from(
            r#"{
                "notification_id": 1,
                "organizer": "acmecorp",
                "event": "democon",
                "action": "pretix.event.changed"
            }"#,
        ))
        .unwrap();

    let response = app.oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::NO_CONTENT);
}

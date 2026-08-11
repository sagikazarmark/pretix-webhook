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

fn event_policy(organizer: &str, event: &str) -> WebhookConfig {
    WebhookConfig::new()
        .allow_organizer(organizer)
        .unwrap()
        .allow_event(event)
        .unwrap()
}

#[tokio::test]
async fn accepted_webhook_is_delivered_to_the_handler() {
    let handler = RecordingHandler::default();
    let recorded = Arc::clone(&handler.events);
    let app = webhook_router(handler, event_policy("acmecorp", "democon"));
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
        WebhookConfig::new()
            .allow_organizer("acmecorp")
            .unwrap()
            .allow_event("democon")
            .unwrap(),
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
async fn new_config_accepts_every_organizer_and_event() {
    let app = webhook_router(RecordingHandler::default(), WebhookConfig::new());
    for payload in [
        r#"{
            "notification_id": 1,
            "organizer": "previously-unknown",
            "event": "future-event",
            "action": "pretix.event.changed"
        }"#,
        r#"{
            "notification_id": 2,
            "action": "pretix.plugin.unknown"
        }"#,
    ] {
        let request = Request::post("/")
            .header("content-type", "application/json")
            .body(Body::from(payload))
            .unwrap();
        let response = app.clone().oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::NO_CONTENT);
    }
}

#[tokio::test]
async fn organizer_and_event_filters_are_independent() {
    let cases = [
        (
            WebhookConfig::new().allow_organizer("acmecorp").unwrap(),
            "acmecorp",
            "any-event",
            StatusCode::NO_CONTENT,
        ),
        (
            WebhookConfig::new().allow_organizer("acmecorp").unwrap(),
            "other",
            "any-event",
            StatusCode::NOT_FOUND,
        ),
        (
            WebhookConfig::new().allow_event("democon").unwrap(),
            "any-organizer",
            "democon",
            StatusCode::NO_CONTENT,
        ),
        (
            WebhookConfig::new().allow_event("democon").unwrap(),
            "any-organizer",
            "other",
            StatusCode::NOT_FOUND,
        ),
    ];

    for (config, organizer, event, expected) in cases {
        let app = webhook_router(RecordingHandler::default(), config);
        let payload = format!(
            r#"{{
                "notification_id": 1,
                "organizer": "{organizer}",
                "event": "{event}",
                "action": "pretix.event.changed"
            }}"#
        );
        let request = Request::post("/").body(Body::from(payload)).unwrap();
        let response = app.oneshot(request).await.unwrap();
        assert_eq!(response.status(), expected);
    }
}

#[tokio::test]
async fn organizer_level_payloads_enforce_only_the_organizer_filter() {
    let config = event_policy("acmecorp", "event-filter-is-not-applicable");

    for (organizer, expected) in [
        ("acmecorp", StatusCode::NO_CONTENT),
        ("other", StatusCode::NOT_FOUND),
    ] {
        let app = webhook_router(RecordingHandler::default(), config.clone());
        let payload = format!(
            r#"{{
                "notification_id": 1,
                "organizer": "{organizer}",
                "customer": "customer-1",
                "action": "pretix.customer.created"
            }}"#
        );
        let request = Request::post("/").body(Body::from(payload)).unwrap();
        let response = app.oneshot(request).await.unwrap();
        assert_eq!(response.status(), expected);
    }
}

#[tokio::test]
async fn unknown_payloads_cannot_bypass_applicable_filters_with_missing_fields() {
    let organizer_filtered = webhook_router(
        RecordingHandler::default(),
        WebhookConfig::new().allow_organizer("acmecorp").unwrap(),
    );
    let missing_organizer = Request::post("/")
        .body(Body::from(
            r#"{
                "notification_id": 1,
                "event": "democon",
                "action": "pretix.plugin.unknown"
            }"#,
        ))
        .unwrap();
    assert_eq!(
        organizer_filtered
            .oneshot(missing_organizer)
            .await
            .unwrap()
            .status(),
        StatusCode::NOT_FOUND
    );

    let event_filtered = webhook_router(
        RecordingHandler::default(),
        WebhookConfig::new().allow_event("democon").unwrap(),
    );
    let organizer_level = Request::post("/")
        .body(Body::from(
            r#"{
                "notification_id": 1,
                "organizer": "acmecorp",
                "action": "pretix.plugin.organizer-level"
            }"#,
        ))
        .unwrap();
    assert_eq!(
        event_filtered
            .clone()
            .oneshot(organizer_level)
            .await
            .unwrap()
            .status(),
        StatusCode::NO_CONTENT
    );

    for unreadable_event in ["12345", "null", "[\"democon\"]"] {
        let payload = format!(
            r#"{{
                "notification_id": 1,
                "organizer": "acmecorp",
                "event": {unreadable_event},
                "action": "pretix.plugin.unknown"
            }}"#
        );
        let request = Request::post("/").body(Body::from(payload)).unwrap();
        assert_eq!(
            event_filtered
                .clone()
                .oneshot(request)
                .await
                .unwrap()
                .status(),
            StatusCode::NOT_FOUND,
            "event field {unreadable_event} bypassed the event filter"
        );
    }
}

#[tokio::test]
async fn filter_matching_is_exact_and_case_sensitive() {
    let app = webhook_router(
        RecordingHandler::default(),
        event_policy("AcmeCorp", "DemoCon"),
    );

    for (organizer, event, expected) in [
        ("AcmeCorp", "DemoCon", StatusCode::NO_CONTENT),
        ("acmecorp", "DemoCon", StatusCode::NOT_FOUND),
        ("AcmeCorp", "democon", StatusCode::NOT_FOUND),
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
        assert_eq!(
            app.clone().oneshot(request).await.unwrap().status(),
            expected
        );
    }
}

#[test]
fn filter_values_reject_only_empty_or_padded_slugs() {
    for value in ["", " padded", "padded ", "\tpadded"] {
        assert!(WebhookConfig::new().allow_organizer(value).is_err());
        assert!(WebhookConfig::new().allow_event(value).is_err());
    }

    let non_pretix_slug = format!("legacy:value-{}", "x".repeat(300));
    assert!(
        WebhookConfig::new()
            .allow_organizer(&non_pretix_slug)
            .is_ok()
    );
    assert!(WebhookConfig::new().allow_event(non_pretix_slug).is_ok());
}

#[test]
fn debug_output_reports_policy_size_without_disclosing_it() {
    let config = event_policy("private-organizer", "private-event")
        .require_basic_auth([BasicAuthCredential::new("private-user", "private-secret")]);

    let debug = format!("{config:?}");

    for value in [
        "private-organizer",
        "private-event",
        "private-user",
        "private-secret",
    ] {
        assert!(!debug.contains(value), "value leaked in {debug:?}");
    }
    assert_eq!(
        debug,
        "WebhookConfig { organizers: <1 REDACTED>, events: <1 REDACTED>, credentials: <1 REDACTED> }"
    );
}

#[tokio::test]
async fn any_configured_basic_auth_credential_is_accepted() {
    let app = webhook_router(
        RecordingHandler::default(),
        WebhookConfig::new().require_basic_auth([
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
async fn empty_basic_auth_credentials_disable_authentication() {
    let app = webhook_router(
        RecordingHandler::default(),
        WebhookConfig::new().require_basic_auth([]),
    );
    let request = Request::post("/")
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

#[tokio::test]
async fn malformed_payload_returns_bad_request() {
    let app = webhook_router(RecordingHandler::default(), WebhookConfig::new());
    let request = Request::post("/")
        .header("content-type", "application/json")
        .body(Body::from("not json"))
        .unwrap();

    let response = app.oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn default_body_limit_rejects_oversized_payloads() {
    let app = webhook_router(RecordingHandler::default(), WebhookConfig::new());
    let request = Request::post("/")
        .body(Body::from(vec![b' '; 2 * 1024 * 1024 + 1]))
        .unwrap();

    let response = app.oneshot(request).await.unwrap();

    assert_eq!(response.status(), StatusCode::PAYLOAD_TOO_LARGE);
}

#[tokio::test]
async fn unsupported_methods_return_method_not_allowed() {
    let app = webhook_router(RecordingHandler::default(), WebhookConfig::new());
    let request = Request::get("/").body(Body::empty()).unwrap();

    let response = app.oneshot(request).await.unwrap();

    assert_eq!(response.status(), StatusCode::METHOD_NOT_ALLOWED);
}

#[tokio::test]
async fn authentication_is_checked_before_payload_parsing() {
    let app = webhook_router(
        RecordingHandler::default(),
        WebhookConfig::new().require_basic_auth([BasicAuthCredential::new("user", "secret")]),
    );
    let unauthenticated = Request::post("/").body(Body::from("not json")).unwrap();
    assert_eq!(
        app.clone().oneshot(unauthenticated).await.unwrap().status(),
        StatusCode::UNAUTHORIZED
    );

    let authenticated = Request::post("/")
        .header("authorization", "Basic dXNlcjpzZWNyZXQ=")
        .body(Body::from("not json"))
        .unwrap();
    assert_eq!(
        app.oneshot(authenticated).await.unwrap().status(),
        StatusCode::BAD_REQUEST
    );
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
    let app = webhook_router(FailingHandler, WebhookConfig::new());
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
        WebhookConfig::new(),
    )
    .unwrap();
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

    let response = app.clone().oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::NO_CONTENT);

    for path in ["/hooks/pretix/", "/hooks/pretix/more", "/hooks"] {
        let request = Request::post(path).body(Body::empty()).unwrap();
        let response = app.clone().oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }
}

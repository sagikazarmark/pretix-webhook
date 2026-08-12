use std::convert::Infallible;

use axum::{Router, routing::post_service};
use bytes::Bytes;
use http::{Request, StatusCode};
use http_body_util::Full;
use pretix_webhook::{BasicAuthCredential, WebhookHandler, WebhookServiceBuilder};
use pretix_webhook_events::WebhookEvent;
use tower::ServiceExt;

const PAYLOAD: &str = r#"{
    "notification_id": 1,
    "organizer": "acmecorp",
    "event": "democon",
    "action": "pretix.event.changed"
}"#;

#[derive(Default)]
struct RecordingHandler {
    events: std::sync::Arc<std::sync::Mutex<Vec<WebhookEvent>>>,
}

impl WebhookHandler for RecordingHandler {
    type Error = Infallible;

    async fn handle(&self, event: WebhookEvent) -> Result<(), Self::Error> {
        self.events.lock().unwrap().push(event);
        Ok(())
    }
}

fn request(path: &str, body: impl Into<Bytes>) -> Request<Full<Bytes>> {
    Request::post(path).body(Full::new(body.into())).unwrap()
}

#[tokio::test]
async fn service_dispatches_an_accepted_event() {
    let handler = RecordingHandler::default();
    let events = std::sync::Arc::clone(&handler.events);
    let service = WebhookServiceBuilder::new()
        .allow_organizer("acmecorp")
        .unwrap()
        .allow_event("democon")
        .unwrap()
        .build(handler);

    let response = service
        .oneshot(request("/caller/route", PAYLOAD))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::NO_CONTENT);
    let events = events.lock().unwrap();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].notification_id(), 1);
}

#[tokio::test]
async fn builder_accepts_an_async_handler_closure() {
    let actions = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let handler_actions = std::sync::Arc::clone(&actions);
    let service = WebhookServiceBuilder::new().build(move |event: WebhookEvent| {
        let handler_actions = std::sync::Arc::clone(&handler_actions);
        async move {
            assert_eq!(event.notification_id(), 1);
            handler_actions
                .lock()
                .unwrap()
                .push(event.action().to_owned());
            Ok::<(), Infallible>(())
        }
    });

    let response = service.oneshot(request("/webhook", PAYLOAD)).await.unwrap();

    assert_eq!(response.status(), StatusCode::NO_CONTENT);
    assert_eq!(actions.lock().unwrap().as_slice(), ["pretix.event.changed"]);
}

#[tokio::test]
async fn service_owns_authentication_and_body_limits() {
    let service = WebhookServiceBuilder::new()
        .require_basic_auth([BasicAuthCredential::new("user", "secret")])
        .build(RecordingHandler::default());

    let unauthenticated = service
        .clone()
        .oneshot(request("/webhook", "not json"))
        .await
        .unwrap();
    assert_eq!(unauthenticated.status(), StatusCode::UNAUTHORIZED);

    let oversized = service
        .oneshot(request("/webhook", vec![b' '; 2 * 1024 * 1024 + 1]))
        .await
        .unwrap();
    assert_eq!(oversized.status(), StatusCode::PAYLOAD_TOO_LARGE);

    let lower_limit = WebhookServiceBuilder::new()
        .body_limit(PAYLOAD.len() - 1)
        .build(RecordingHandler::default())
        .oneshot(request("/webhook", PAYLOAD))
        .await
        .unwrap();
    assert_eq!(lower_limit.status(), StatusCode::PAYLOAD_TOO_LARGE);
}

#[tokio::test]
async fn handler_does_not_need_to_be_cloneable() {
    let handler = RecordingHandler::default();
    let events = std::sync::Arc::clone(&handler.events);
    let service = WebhookServiceBuilder::new().build(handler);

    assert_eq!(
        service
            .clone()
            .oneshot(request("/webhook", PAYLOAD))
            .await
            .unwrap()
            .status(),
        StatusCode::NO_CONTENT
    );
    assert_eq!(
        service
            .oneshot(request("/webhook", PAYLOAD))
            .await
            .unwrap()
            .status(),
        StatusCode::NO_CONTENT
    );
    assert_eq!(events.lock().unwrap().len(), 2);
}

#[tokio::test]
async fn caller_owned_axum_routes_can_mount_independent_services() {
    let first = RecordingHandler::default();
    let first_events = std::sync::Arc::clone(&first.events);
    let second = RecordingHandler::default();
    let second_events = std::sync::Arc::clone(&second.events);
    let app = Router::new()
        .route(
            "/first",
            post_service(WebhookServiceBuilder::new().build(first)),
        )
        .route(
            "/second",
            post_service(WebhookServiceBuilder::new().build(second)),
        );

    assert_eq!(
        app.clone()
            .oneshot(request("/first", PAYLOAD))
            .await
            .unwrap()
            .status(),
        StatusCode::NO_CONTENT
    );
    assert_eq!(first_events.lock().unwrap().len(), 1);
    assert!(second_events.lock().unwrap().is_empty());

    assert_eq!(
        app.clone()
            .oneshot(request("/second", PAYLOAD))
            .await
            .unwrap()
            .status(),
        StatusCode::NO_CONTENT
    );
    assert_eq!(second_events.lock().unwrap().len(), 1);

    let get = Request::get("/first")
        .body(Full::new(Bytes::new()))
        .unwrap();
    assert_eq!(
        app.clone().oneshot(get).await.unwrap().status(),
        StatusCode::METHOD_NOT_ALLOWED
    );
    assert_eq!(
        app.oneshot(request("/unknown", PAYLOAD))
            .await
            .unwrap()
            .status(),
        StatusCode::NOT_FOUND
    );
}

use std::{
    convert::Infallible,
    sync::{Arc, Mutex},
};

use axum::{
    Router,
    body::Body,
    http::{Request, StatusCode},
    routing::get,
};
use pretix_webhook::{
    BasicAuthCredential, MultiWebhookRouter, WebhookConfig, WebhookHandler, WebhookRouterBuilder,
};
use pretix_webhook_events::WebhookEvent;
use tower::ServiceExt;

const PAYLOAD: &str = r#"{
    "notification_id": 1,
    "organizer": "acmecorp",
    "event": "democon",
    "action": "pretix.event.changed"
}"#;

#[derive(Clone, Default)]
struct CountingHandler {
    deliveries: Arc<Mutex<usize>>,
}

impl WebhookHandler for CountingHandler {
    type Error = Infallible;

    async fn handle(&self, _event: WebhookEvent) -> Result<(), Self::Error> {
        *self.deliveries.lock().unwrap() += 1;
        Ok(())
    }
}

#[derive(Clone)]
struct ActionHandler {
    actions: Arc<Mutex<Vec<String>>>,
}

#[derive(Debug)]
struct ActionHandlerError;

impl std::fmt::Display for ActionHandlerError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("action handler failed")
    }
}

impl WebhookHandler for ActionHandler {
    type Error = ActionHandlerError;

    async fn handle(&self, event: WebhookEvent) -> Result<(), Self::Error> {
        self.actions.lock().unwrap().push(event.action().to_owned());
        Ok(())
    }
}

fn request(path: &str) -> Request<Body> {
    Request::post(path).body(Body::from(PAYLOAD)).unwrap()
}

fn event_request(path: &str, organizer: &str, event: &str, authorization: &str) -> Request<Body> {
    let payload = format!(
        r#"{{
            "notification_id": 1,
            "organizer": "{organizer}",
            "event": "{event}",
            "action": "pretix.event.changed"
        }}"#
    );
    Request::post(path)
        .header("authorization", authorization)
        .body(Body::from(payload))
        .unwrap()
}

#[tokio::test]
async fn exact_paths_dispatch_to_heterogeneous_handlers() {
    let counting_handler = CountingHandler::default();
    let deliveries = Arc::clone(&counting_handler.deliveries);
    let actions = Arc::new(Mutex::new(Vec::new()));
    let action_handler = ActionHandler {
        actions: Arc::clone(&actions),
    };

    let app = MultiWebhookRouter::new("/hooks")
        .unwrap()
        .register("sales/primary", counting_handler, WebhookConfig::new())
        .unwrap()
        .register("audit/pretix", action_handler, WebhookConfig::new())
        .unwrap()
        .finish();

    assert_eq!(
        app.clone()
            .oneshot(request("/hooks/sales/primary"))
            .await
            .unwrap()
            .status(),
        StatusCode::NO_CONTENT
    );
    assert_eq!(*deliveries.lock().unwrap(), 1);
    assert!(actions.lock().unwrap().is_empty());

    assert_eq!(
        app.clone()
            .oneshot(request("/hooks/audit/pretix"))
            .await
            .unwrap()
            .status(),
        StatusCode::NO_CONTENT
    );
    assert_eq!(*deliveries.lock().unwrap(), 1);
    assert_eq!(actions.lock().unwrap().as_slice(), ["pretix.event.changed"]);

    for path in [
        "/hooks/sales",
        "/hooks/sales/primary/",
        "/hooks/sales/primary/more",
        "/hooks/unregistered",
    ] {
        assert_eq!(
            app.clone().oneshot(request(path)).await.unwrap().status(),
            StatusCode::NOT_FOUND
        );
    }
}

#[test]
fn duplicate_resolved_paths_are_rejected_before_router_merge() {
    let builder = MultiWebhookRouter::new("/hooks")
        .unwrap()
        .register(
            "pretix/events",
            CountingHandler::default(),
            WebhookConfig::new(),
        )
        .unwrap();

    let result = builder.register(
        "pretix/events",
        CountingHandler::default(),
        WebhookConfig::new(),
    );

    let Err(error) = result else {
        panic!("duplicate route was accepted");
    };
    assert!(error.to_string().contains("/hooks/pretix/events"));
}

#[tokio::test]
async fn root_prefixed_webhooks_compose_with_unrelated_routes() {
    let webhooks = MultiWebhookRouter::new("/")
        .unwrap()
        .register(
            "pretix/events",
            CountingHandler::default(),
            WebhookConfig::new(),
        )
        .unwrap()
        .finish();
    let app = Router::new()
        .route("/health", get(|| async { StatusCode::OK }))
        .merge(webhooks);

    assert_eq!(
        app.clone()
            .oneshot(request("/pretix/events"))
            .await
            .unwrap()
            .status(),
        StatusCode::NO_CONTENT
    );
    assert_eq!(
        app.oneshot(Request::get("/health").body(Body::empty()).unwrap())
            .await
            .unwrap()
            .status(),
        StatusCode::OK
    );
}

#[tokio::test]
async fn authentication_and_filters_are_isolated_per_route() {
    let first = CountingHandler::default();
    let first_deliveries = Arc::clone(&first.deliveries);
    let second = CountingHandler::default();
    let second_deliveries = Arc::clone(&second.deliveries);
    let first_config = WebhookConfig::new()
        .allow_organizer("acmecorp")
        .unwrap()
        .allow_event("democon")
        .unwrap()
        .require_basic_auth([BasicAuthCredential::new("alpha", "one")]);
    let second_config = WebhookConfig::new()
        .allow_organizer("other")
        .unwrap()
        .allow_event("conference")
        .unwrap()
        .require_basic_auth([BasicAuthCredential::new("beta", "two")]);
    let app = MultiWebhookRouter::new("/hooks")
        .unwrap()
        .register("first", first, first_config)
        .unwrap()
        .register("second", second, second_config)
        .unwrap()
        .finish();

    let wrong_route_credential =
        event_request("/hooks/second", "other", "conference", "Basic YWxwaGE6b25l");
    assert_eq!(
        app.clone()
            .oneshot(wrong_route_credential)
            .await
            .unwrap()
            .status(),
        StatusCode::UNAUTHORIZED
    );

    let wrong_route_filter =
        event_request("/hooks/first", "other", "conference", "Basic YWxwaGE6b25l");
    assert_eq!(
        app.clone()
            .oneshot(wrong_route_filter)
            .await
            .unwrap()
            .status(),
        StatusCode::NOT_FOUND
    );
    assert_eq!(*first_deliveries.lock().unwrap(), 0);
    assert_eq!(*second_deliveries.lock().unwrap(), 0);

    let first_request = event_request("/hooks/first", "acmecorp", "democon", "Basic YWxwaGE6b25l");
    let second_request =
        event_request("/hooks/second", "other", "conference", "Basic YmV0YTp0d28=");
    assert_eq!(
        app.clone().oneshot(first_request).await.unwrap().status(),
        StatusCode::NO_CONTENT
    );
    assert_eq!(
        app.oneshot(second_request).await.unwrap().status(),
        StatusCode::NO_CONTENT
    );
    assert_eq!(*first_deliveries.lock().unwrap(), 1);
    assert_eq!(*second_deliveries.lock().unwrap(), 1);
}

#[tokio::test]
async fn exact_path_registration_dispatches_and_rejects_collisions() {
    let counting_handler = CountingHandler::default();
    let deliveries = Arc::clone(&counting_handler.deliveries);
    let actions = Arc::new(Mutex::new(Vec::new()));
    let action_handler = ActionHandler {
        actions: Arc::clone(&actions),
    };

    let app = WebhookRouterBuilder::new()
        .register_at(
            "/hooks/sales/primary",
            counting_handler,
            WebhookConfig::new(),
        )
        .unwrap()
        .register_at("/audit", action_handler, WebhookConfig::new())
        .unwrap()
        .finish();

    assert_eq!(
        app.clone()
            .oneshot(request("/hooks/sales/primary"))
            .await
            .unwrap()
            .status(),
        StatusCode::NO_CONTENT
    );
    assert_eq!(
        app.oneshot(request("/audit")).await.unwrap().status(),
        StatusCode::NO_CONTENT
    );
    assert_eq!(*deliveries.lock().unwrap(), 1);
    assert_eq!(actions.lock().unwrap().as_slice(), ["pretix.event.changed"]);

    let registered = WebhookRouterBuilder::new()
        .register_at("/hooks", CountingHandler::default(), WebhookConfig::new())
        .unwrap();
    let Err(error) =
        registered.register_at("/hooks", CountingHandler::default(), WebhookConfig::new())
    else {
        panic!("duplicate exact path was accepted");
    };
    assert!(error.to_string().contains("/hooks"));

    let Err(error) = WebhookRouterBuilder::new().register_at(
        "hooks/{organizer}",
        CountingHandler::default(),
        WebhookConfig::new(),
    ) else {
        panic!("invalid exact path was accepted");
    };
    assert!(error.to_string().contains("hooks/{organizer}"));
}

#[test]
fn construction_and_registration_enforce_the_shared_path_grammar() {
    for prefix in ["hooks", "/hooks/", "/hooks/{organizer}"] {
        let result = MultiWebhookRouter::new(prefix);
        let Err(error) = result else {
            panic!("invalid prefix {prefix:?} was accepted");
        };
        assert!(error.to_string().contains(prefix));
    }

    for relative_path in ["", "/pretix", "pretix/", "pretix/{event}"] {
        let result = MultiWebhookRouter::new("/hooks").unwrap().register(
            relative_path,
            CountingHandler::default(),
            WebhookConfig::new(),
        );
        let Err(error) = result else {
            panic!("invalid relative path {relative_path:?} was accepted");
        };
        assert!(error.to_string().contains(relative_path));
    }
}

#![cfg(feature = "log")]

use std::{collections::BTreeMap, sync::Mutex};

#[cfg(not(feature = "tracing"))]
use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use log::{
    Log, Metadata, Record,
    kv::{Key, Value, VisitSource},
};
use pretix_webhook::{LogHandler, WebhookHandler};
#[cfg(not(feature = "tracing"))]
use pretix_webhook::{WebhookConfig, handler_fn, webhook_router, webhook_router_at};
use pretix_webhook_events::WebhookEvent;
#[cfg(not(feature = "tracing"))]
use tower::ServiceExt;

static LOGGER: CapturingLogger = CapturingLogger {
    records: Mutex::new(Vec::new()),
};

struct CapturingLogger {
    records: Mutex<Vec<CapturedRecord>>,
}

struct CapturedRecord {
    message: String,
    fields: BTreeMap<String, String>,
}

impl Log for CapturingLogger {
    fn enabled(&self, _metadata: &Metadata<'_>) -> bool {
        true
    }

    fn log(&self, record: &Record<'_>) {
        let mut visitor = FieldVisitor::default();
        record.key_values().visit(&mut visitor).unwrap();
        self.records.lock().unwrap().push(CapturedRecord {
            message: record.args().to_string(),
            fields: visitor.fields,
        });
    }

    fn flush(&self) {}
}

#[derive(Default)]
struct FieldVisitor {
    fields: BTreeMap<String, String>,
}

impl<'kvs> VisitSource<'kvs> for FieldVisitor {
    fn visit_pair(&mut self, key: Key<'kvs>, value: Value<'kvs>) -> Result<(), log::kv::Error> {
        self.fields.insert(key.to_string(), value.to_string());
        Ok(())
    }
}

#[tokio::test]
async fn log_handler_emits_semantic_fields_with_optional_route_identity() {
    log::set_logger(&LOGGER).unwrap();
    log::set_max_level(log::LevelFilter::Info);
    let event: WebhookEvent = serde_json::from_str(
        r#"{
            "notification_id": 42,
            "organizer": "acmecorp",
            "event": "democon",
            "action": "pretix.event.changed"
        }"#,
    )
    .unwrap();

    LogHandler::with_route("/hooks/pretix")
        .unwrap()
        .handle(event.clone())
        .await
        .unwrap();
    LogHandler.handle(event).await.unwrap();

    {
        let records = LOGGER.records.lock().unwrap();
        assert_eq!(records.len(), 2);
        assert_eq!(records[0].message, "received pretix webhook");
        assert_eq!(
            records[0].fields,
            BTreeMap::from([
                ("action".into(), "pretix.event.changed".into()),
                ("kind".into(), "Event".into()),
                ("notification_id".into(), "42".into()),
                ("organizer".into(), "acmecorp".into()),
                ("pretix_event".into(), "democon".into()),
                ("route".into(), "/hooks/pretix".into()),
            ])
        );
        assert_eq!(records[1].message, "received pretix webhook");
        assert_eq!(
            records[1].fields,
            BTreeMap::from([
                ("action".into(), "pretix.event.changed".into()),
                ("kind".into(), "Event".into()),
                ("notification_id".into(), "42".into()),
                ("organizer".into(), "acmecorp".into()),
                ("pretix_event".into(), "democon".into()),
            ])
        );
    }

    let error = LogHandler::with_route("hooks/pretix").unwrap_err();
    assert!(error.to_string().contains("it must start with '/'"));

    #[cfg(not(feature = "tracing"))]
    {
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

        let records = LOGGER.records.lock().unwrap();
        assert_eq!(
            records[2].message,
            "pretix webhook handler failed: downstream unavailable"
        );
        assert_eq!(
            records[2].fields,
            BTreeMap::from([("route".into(), "/hooks/pretix".into())])
        );
        assert_eq!(
            records[3].message,
            "pretix webhook handler failed: downstream unavailable"
        );
        assert_eq!(records[3].fields, BTreeMap::new());
    }
}

#[cfg(not(feature = "tracing"))]
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

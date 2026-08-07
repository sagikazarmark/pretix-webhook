#![cfg(feature = "log")]

use std::{collections::BTreeMap, sync::Mutex};

use log::{
    Log, Metadata, Record,
    kv::{Key, Value, VisitSource},
};
use pretix_webhook::{LogHandler, WebhookHandler};
use pretix_webhook_events::WebhookEvent;

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
async fn log_handler_emits_semantic_fields() {
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

    LogHandler.handle(event).await.unwrap();

    let records = LOGGER.records.lock().unwrap();
    assert_eq!(records.len(), 1);
    assert_eq!(records[0].message, "received pretix webhook");
    assert_eq!(
        records[0].fields,
        BTreeMap::from([
            ("action".into(), "pretix.event.changed".into()),
            ("kind".into(), "Event".into()),
            ("notification_id".into(), "42".into()),
            ("organizer".into(), "acmecorp".into()),
            ("pretix_event".into(), "democon".into()),
        ])
    );
}

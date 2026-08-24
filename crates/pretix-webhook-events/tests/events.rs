use std::collections::BTreeMap;

use pretix_webhook_events::{
    Notification, OrderEvent, UnknownEvent, WebhookEvent, WebhookEventKind,
};

#[test]
fn deserializes_an_order_event() {
    let event: WebhookEvent = serde_json::from_str(
        r#"{
            "notification_id": 123455,
            "organizer": "acmecorp",
            "event": "democon",
            "code": "ABC23",
            "action": "pretix.event.order.placed"
        }"#,
    )
    .expect("documented Pretix payload should deserialize");

    assert_eq!(event.notification_id(), 123_455);
    assert_eq!(event.action(), "pretix.event.order.placed");
    assert_eq!(event.organizer_slug(), Some("acmecorp"));
    assert_eq!(event.event_slug(), Some("democon"));

    let order = event
        .as_order()
        .expect("an order action should produce an order event");
    assert_eq!(order.code, "ABC23");
}

#[test]
fn deserializes_a_checkin_event_with_its_extra_fields() {
    let event: WebhookEvent = serde_json::from_str(
        r#"{
            "notification_id": 42,
            "organizer": "acmecorp",
            "event": "democon",
            "code": "ABC23",
            "action": "pretix.event.checkin",
            "orderposition_id": 91,
            "orderposition_positionid": 2,
            "checkin_list": 3,
            "type": "entry",
            "first_checkin": true
        }"#,
    )
    .expect("Pretix check-in payload should deserialize");

    let checkin = event
        .as_checkin()
        .expect("a check-in action should produce a check-in event");
    assert_eq!(checkin.orderposition_id, Some(91));
    assert_eq!(checkin.orderposition_positionid, Some(2));
    assert_eq!(checkin.checkin_list, Some(3));
    assert_eq!(checkin.kind.as_deref(), Some("entry"));
    assert_eq!(checkin.first_checkin, Some(true));
}

#[test]
fn dispatches_every_core_payload_shape() {
    let cases = [
        (
            r#"{"notification_id":1,"organizer":"acme","event":"demo","action":"pretix.event.changed"}"#,
            WebhookEventKind::Event,
        ),
        (
            r#"{"notification_id":2,"organizer":"acme","event":"demo","voucher":17,"action":"pretix.voucher.changed"}"#,
            WebhookEventKind::Voucher,
        ),
        (
            r#"{"notification_id":3,"organizer":"acme","event":"demo","subevent":"18","action":"pretix.subevent.added"}"#,
            WebhookEventKind::Subevent,
        ),
        (
            r#"{"notification_id":4,"organizer":"acme","event":"demo","item":19,"action":"pretix.event.item.changed"}"#,
            WebhookEventKind::Item,
        ),
        (
            r#"{"notification_id":5,"organizer":"acme","event":"demo","waitinglistentry":20,"action":"pretix.event.orders.waitinglist.added"}"#,
            WebhookEventKind::WaitingList,
        ),
        (
            r#"{"notification_id":6,"organizer":"acme","customer":"cust-1","action":"pretix.customer.created"}"#,
            WebhookEventKind::Customer,
        ),
        (
            r#"{"notification_id":7,"issuer_id":4,"issuer_slug":"acme","giftcard":21,"action":"pretix.giftcards.created"}"#,
            WebhookEventKind::GiftCard,
        ),
        (
            r#"{"notification_id":8,"issuer_id":4,"issuer_slug":"acme","acceptor_id":5,"acceptor_slug":"other","giftcard":21,"action":"pretix.giftcards.transaction.redeemed"}"#,
            WebhookEventKind::GiftCardTransaction,
        ),
    ];

    for (json, expected_kind) in cases {
        let event: WebhookEvent = serde_json::from_str(json).expect("core payload should parse");
        assert_eq!(event.kind(), expected_kind);
    }
}

#[test]
fn preserves_unknown_plugin_events_for_forwarding() {
    let input = serde_json::json!({
        "notification_id": 99,
        "organizer": "acme",
        "event": "demo",
        "action": "pretix.plugin.badge.printed",
        "badge_id": 123,
        "nested": { "value": true }
    });

    let event: WebhookEvent =
        serde_json::from_value(input.clone()).expect("plugin payload should parse");

    assert_eq!(event.kind(), WebhookEventKind::Unknown);
    assert_eq!(event.organizer_slug(), Some("acme"));
    assert_eq!(event.event_slug(), Some("demo"));
    assert_eq!(serde_json::to_value(event).unwrap(), input);
}

#[test]
fn rejects_invalid_common_envelopes_and_malformed_known_payloads() {
    for input in [
        serde_json::json!({"notification_id": 1}),
        serde_json::json!({"notification_id": 1, "action": 42}),
        serde_json::json!({
            "notification_id": -1,
            "action": "pretix.plugin.badge.printed"
        }),
        serde_json::json!({"action": "pretix.plugin.badge.printed"}),
        serde_json::json!({
            "notification_id": 1,
            "action": "pretix.event.order.placed"
        }),
    ] {
        assert!(
            serde_json::from_value::<WebhookEvent>(input.clone()).is_err(),
            "invalid payload unexpectedly parsed: {input}"
        );
    }
}

#[test]
fn wraps_payloads_into_their_webhook_event_variant() {
    let order = OrderEvent {
        notification: Notification {
            notification_id: 7,
            action: "pretix.event.order.paid".to_owned(),
        },
        organizer: "acme".to_owned(),
        event: "demo".to_owned(),
        code: "ABC23".to_owned(),
    };
    let unknown = UnknownEvent {
        notification: Notification {
            notification_id: 8,
            action: "pretix.plugin.badge.printed".to_owned(),
        },
        fields: BTreeMap::from([("badge_id".to_owned(), serde_json::json!(123))]),
    };

    let event = WebhookEvent::from(order.clone());
    assert_eq!(event, WebhookEvent::Order(order.clone()));
    assert_eq!(event.kind(), WebhookEventKind::Order);
    assert_eq!(event.as_order(), Some(&order));

    let event: WebhookEvent = unknown.clone().into();
    assert_eq!(event, WebhookEvent::Unknown(unknown));
    assert_eq!(event.kind(), WebhookEventKind::Unknown);

    // Constructing through `From` produces the same value as deserializing.
    let parsed: WebhookEvent = serde_json::from_value(serde_json::to_value(&order).unwrap())
        .expect("a serialized order payload should deserialize");
    assert_eq!(parsed, WebhookEvent::from(order));
}

# pretix-webhook-events

[![crates.io](https://img.shields.io/crates/v/pretix-webhook-events?style=flat-square)](https://crates.io/crates/pretix-webhook-events)
[![docs.rs](https://img.shields.io/docsrs/pretix-webhook-events?style=flat-square)](https://docs.rs/pretix-webhook-events)

**Typed payloads sent by [pretix webhooks](https://docs.pretix.eu/dev/api/webhooks.html), with no HTTP dependency.**

Use this crate directly to parse, inspect, or forward payloads;
`pretix-webhook` builds a framework-independent Tower receiver on top of it.

Pretix only guarantees `notification_id` and `action` across all core payloads.
`WebhookEvent` dispatches known core actions to their actual payload shapes and
preserves plugin-defined actions in `UnknownEvent`.

## Quick Start

`WebhookEvent` implements `Deserialize`, so a delivery body parses in one step
and the action selects the payload family:

```rust
use pretix_webhook_events::WebhookEvent;

fn main() -> Result<(), serde_json::Error> {
    let event: WebhookEvent = serde_json::from_str(
        r#"{
            "notification_id": 123455,
            "organizer": "acmecorp",
            "event": "democon",
            "code": "ABC23",
            "action": "pretix.event.order.placed"
        }"#,
    )?;

    assert_eq!(event.notification_id(), 123_455);
    assert_eq!(event.action(), "pretix.event.order.placed");

    let order = event.as_order().expect("an order action carries an order payload");
    assert_eq!(order.code, "ABC23");

    Ok(())
}
```

Payloads are only triggers: fetch trusted state from the authenticated pretix
API before acting on it. Pretix documents that notifications can be duplicated,
so consumers should be idempotent.

## Actions and payload types

| Action | Type | `kind()` |
| --- | --- | --- |
| `pretix.event.order.*` | `OrderEvent` | `Order` |
| `pretix.event.checkin`, `pretix.event.checkin.reverted` | `CheckinEvent` | `Checkin` |
| `pretix.event.added`, `.changed`, `.deleted`, `.live.*`, `.testmode.*` | `EventEvent` | `Event` |
| `pretix.voucher.*` | `VoucherEvent` | `Voucher` |
| `pretix.subevent.*` | `SubeventEvent` | `Subevent` |
| `pretix.event.item.*`, `pretix.event.quota.*` | `ItemEvent` | `Item` |
| `pretix.event.orders.waitinglist.*` | `WaitingListEvent` | `WaitingList` |
| `pretix.customer.*` | `CustomerEvent` | `Customer` |
| `pretix.giftcards.transaction.*` | `GiftCardTransactionEvent` | `GiftCardTransaction` |
| `pretix.giftcards.*` (other) | `GiftCardEvent` | `GiftCard` |
| anything else | `UnknownEvent` | `Unknown` |

Payload types nest rather than repeat themselves: every payload flattens a
`Notification` (`notification_id` and `action`), `CheckinEvent` extends
`OrderEvent`, and the voucher, sub-event, item, and waiting-list payloads
extend `EventEvent`. `GiftCardTransactionEvent` extends `GiftCardEvent`. Fields
that pretix documents as generic object relations are `ResourceId`, which
accepts either an integer or a string.

`kind()` reports the family without matching on the enum, and `as_order` and
`as_checkin` borrow the two payloads that carry more than identity fields. Both
`WebhookEvent` and the individual payload types are `Serialize`, so events can
be re-emitted onto a queue or written to storage.

## Routing without matching on the payload

The accessors that matter for filtering and dispatch work across every family,
so a receiver does not need to know which one it holds:

```rust
use pretix_webhook_events::WebhookEvent;

fn accepts(event: &WebhookEvent, organizer: &str, allowed_event: &str) -> bool {
    if event.organizer_slug() != Some(organizer) {
        return false;
    }

    // Organizer-level payloads (customers, gift cards) carry no event field,
    // so an event filter does not apply to them at all.
    !event.is_event_level() || event.event_slug() == Some(allowed_event)
}

fn main() -> Result<(), serde_json::Error> {
    let checkin: WebhookEvent = serde_json::from_str(
        r#"{
            "notification_id": 42,
            "organizer": "acmecorp",
            "event": "democon",
            "code": "ABC23",
            "action": "pretix.event.checkin",
            "first_checkin": true
        }"#,
    )?;
    assert!(accepts(&checkin, "acmecorp", "democon"));

    let gift_card: WebhookEvent = serde_json::from_str(
        r#"{
            "notification_id": 43,
            "issuer_id": 4,
            "issuer_slug": "acmecorp",
            "giftcard": 21,
            "action": "pretix.giftcards.created"
        }"#,
    )?;
    assert!(accepts(&gift_card, "acmecorp", "democon"));

    Ok(())
}
```

`organizer_slug` reads `issuer_slug` for gift-card payloads, so gift cards
filter on their issuing organizer like everything else. `is_event_level`
answers whether an event filter applies at all: a payload carrying an event
field is event-level even when the field cannot be read as a slug, so an
unreadable value fails an applicable filter instead of being treated as
organizer-level.

## Unknown and plugin actions

Actions from plugins, or added by a future pretix release, deserialize into
`UnknownEvent` when the common envelope contains a non-negative integer
`notification_id` representable as `u64` and a string `action`. All other JSON
fields are kept in `fields`, and
`organizer_slug`, `event_slug`, and `is_event_level` still work by reading the
conventional field names, so valid unknown events can be filtered and forwarded
without discarding fields:

```rust
use pretix_webhook_events::{WebhookEvent, WebhookEventKind};

fn main() -> Result<(), serde_json::Error> {
    let input = serde_json::json!({
        "notification_id": 99,
        "organizer": "acmecorp",
        "event": "democon",
        "action": "pretix.plugin.badge.printed",
        "badge_id": 123,
        "nested": { "value": true }
    });

    let event: WebhookEvent = serde_json::from_value(input.clone())?;

    assert_eq!(event.kind(), WebhookEventKind::Unknown);
    assert_eq!(event.organizer_slug(), Some("acmecorp"));
    assert_eq!(event.event_slug(), Some("democon"));
    assert_eq!(serde_json::to_value(&event)?, input);

    Ok(())
}
```

Payloads with a missing or invalid common envelope are rejected. Known actions
are also validated against their typed payload and are rejected when required
fields are missing or have the wrong type.

## References

- [Pretix webhook receiving and retry behavior](https://docs.pretix.eu/dev/api/webhooks.html)
- [Pretix core webhook action types](https://docs.pretix.eu/dev/api/resources/webhooks.html)
- [Pretix core payload builders](https://github.com/pretix/pretix/blob/master/src/pretix/api/webhooks.py)

## License

Licensed under either of

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE) or <https://www.apache.org/licenses/LICENSE-2.0>)
- MIT license ([LICENSE-MIT](LICENSE-MIT) or <https://opensource.org/licenses/MIT>)

at your option.

### Contribution

Unless you explicitly state otherwise, any contribution intentionally submitted
for inclusion in the work by you, as defined in the Apache-2.0 license, shall be
dual licensed as above, without any additional terms or conditions.

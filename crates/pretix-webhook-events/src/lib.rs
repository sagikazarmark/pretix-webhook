//! Typed payloads sent by pretix webhooks, with no HTTP dependency.
//!
//! Pretix only guarantees `notification_id` and `action` across all core
//! payloads. [`WebhookEvent`] dispatches known core actions to their actual
//! payload shapes and preserves plugin-defined actions in [`UnknownEvent`].
//!
//! # Parsing a payload
//!
//! [`WebhookEvent`] implements [`Deserialize`](serde::Deserialize), so a
//! delivery body parses in one step and the action selects the payload family:
//!
//! ```
//! use pretix_webhook_events::WebhookEvent;
//!
//! let event: WebhookEvent = serde_json::from_str(
//!     r#"{
//!         "notification_id": 123455,
//!         "organizer": "acmecorp",
//!         "event": "democon",
//!         "code": "ABC23",
//!         "action": "pretix.event.order.placed"
//!     }"#,
//! )?;
//!
//! assert_eq!(event.notification_id(), 123_455);
//! assert_eq!(event.action(), "pretix.event.order.placed");
//!
//! let order = event.as_order().expect("an order action carries an order payload");
//! assert_eq!(order.code, "ABC23");
//! # Ok::<(), serde_json::Error>(())
//! ```
//!
//! # Actions and payload types
//!
//! | Action | Type | [`kind()`](WebhookEvent::kind) |
//! | --- | --- | --- |
//! | `pretix.event.order.*` | [`OrderEvent`] | [`Order`](WebhookEventKind::Order) |
//! | `pretix.event.checkin`, `pretix.event.checkin.reverted` | [`CheckinEvent`] | [`Checkin`](WebhookEventKind::Checkin) |
//! | `pretix.event.added`, `.changed`, `.deleted`, `.live.*`, `.testmode.*` | [`EventEvent`] | [`Event`](WebhookEventKind::Event) |
//! | `pretix.voucher.*` | [`VoucherEvent`] | [`Voucher`](WebhookEventKind::Voucher) |
//! | `pretix.subevent.*` | [`SubeventEvent`] | [`Subevent`](WebhookEventKind::Subevent) |
//! | `pretix.event.item.*`, `pretix.event.quota.*` | [`ItemEvent`] | [`Item`](WebhookEventKind::Item) |
//! | `pretix.event.orders.waitinglist.*` | [`WaitingListEvent`] | [`WaitingList`](WebhookEventKind::WaitingList) |
//! | `pretix.customer.*` | [`CustomerEvent`] | [`Customer`](WebhookEventKind::Customer) |
//! | `pretix.giftcards.transaction.*` | [`GiftCardTransactionEvent`] | [`GiftCardTransaction`](WebhookEventKind::GiftCardTransaction) |
//! | `pretix.giftcards.*` (other) | [`GiftCardEvent`] | [`GiftCard`](WebhookEventKind::GiftCard) |
//! | anything else | [`UnknownEvent`] | [`Unknown`](WebhookEventKind::Unknown) |
//!
//! Payload types nest rather than repeat themselves: every payload flattens a
//! [`Notification`], [`CheckinEvent`] extends [`OrderEvent`], the voucher,
//! sub-event, item, and waiting-list payloads extend [`EventEvent`], and
//! [`GiftCardTransactionEvent`] extends [`GiftCardEvent`]. Fields that pretix
//! documents as generic object relations are [`ResourceId`], which accepts
//! either an integer or a string.
//!
//! Both [`WebhookEvent`] and the individual payload types are
//! [`Serialize`](serde::Serialize), so events can be re-emitted onto a queue or
//! written to storage.
//!
//! # Routing without matching on the payload
//!
//! The accessors that matter for filtering and dispatch work across every
//! family, so a receiver does not need to know which one it holds:
//!
//! ```
//! use pretix_webhook_events::WebhookEvent;
//!
//! fn accepts(event: &WebhookEvent, organizer: &str, allowed_event: &str) -> bool {
//!     if event.organizer_slug() != Some(organizer) {
//!         return false;
//!     }
//!
//!     // Organizer-level payloads (customers, gift cards) carry no event
//!     // field, so an event filter does not apply to them at all.
//!     !event.is_event_level() || event.event_slug() == Some(allowed_event)
//! }
//!
//! let checkin: WebhookEvent = serde_json::from_str(
//!     r#"{
//!         "notification_id": 42,
//!         "organizer": "acmecorp",
//!         "event": "democon",
//!         "code": "ABC23",
//!         "action": "pretix.event.checkin",
//!         "first_checkin": true
//!     }"#,
//! )?;
//! assert!(accepts(&checkin, "acmecorp", "democon"));
//!
//! let gift_card: WebhookEvent = serde_json::from_str(
//!     r#"{
//!         "notification_id": 43,
//!         "issuer_id": 4,
//!         "issuer_slug": "acmecorp",
//!         "giftcard": 21,
//!         "action": "pretix.giftcards.created"
//!     }"#,
//! )?;
//! assert!(accepts(&gift_card, "acmecorp", "democon"));
//! # Ok::<(), serde_json::Error>(())
//! ```
//!
//! [`WebhookEvent::organizer_slug`] reads `issuer_slug` for gift-card payloads,
//! so gift cards filter on their issuing organizer like everything else.
//! [`WebhookEvent::is_event_level`] answers whether an event filter applies at
//! all.
//!
//! # Unknown and plugin actions
//!
//! Actions from plugins, or added by a future pretix release, deserialize into
//! [`UnknownEvent`] when the common envelope contains a non-negative integer
//! `notification_id` representable as `u64` and a string `action`. All other
//! JSON fields are kept in [`fields`](UnknownEvent::fields), and the accessors
//! above still work by reading the conventional field names, so valid unknown
//! events can be filtered and forwarded without discarding fields:
//!
//! ```
//! use pretix_webhook_events::{WebhookEvent, WebhookEventKind};
//!
//! let input = serde_json::json!({
//!     "notification_id": 99,
//!     "organizer": "acmecorp",
//!     "event": "democon",
//!     "action": "pretix.plugin.badge.printed",
//!     "badge_id": 123,
//!     "nested": { "value": true }
//! });
//!
//! let event: WebhookEvent = serde_json::from_value(input.clone())?;
//!
//! assert_eq!(event.kind(), WebhookEventKind::Unknown);
//! assert_eq!(event.organizer_slug(), Some("acmecorp"));
//! assert_eq!(event.event_slug(), Some("democon"));
//! assert_eq!(serde_json::to_value(&event)?, input);
//! # Ok::<(), serde_json::Error>(())
//! ```
//!
//! Payloads with a missing or invalid common envelope are rejected. Known
//! actions are also validated against their typed payload and are rejected when
//! required fields are missing or have the wrong type.

mod event;
mod payload;

pub use event::{WebhookEvent, WebhookEventKind};
pub use payload::{
    CheckinEvent, CustomerEvent, EventEvent, GiftCardEvent, GiftCardTransactionEvent, ItemEvent,
    Notification, OrderEvent, ResourceId, SubeventEvent, UnknownEvent, VoucherEvent,
    WaitingListEvent,
};

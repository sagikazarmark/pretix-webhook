//! Typed payloads sent by pretix webhooks.
//!
//! Pretix only guarantees `notification_id` and `action` across all core
//! payloads. This crate dispatches known core actions to their actual payload
//! shapes and preserves plugin-defined actions in [`UnknownEvent`].

mod event;
mod payload;

pub use event::{WebhookEvent, WebhookEventKind};
pub use payload::{
    CheckinEvent, CustomerEvent, EventEvent, GiftCardEvent, GiftCardTransactionEvent, ItemEvent,
    Notification, OrderEvent, ResourceId, SubeventEvent, UnknownEvent, VoucherEvent,
    WaitingListEvent,
};

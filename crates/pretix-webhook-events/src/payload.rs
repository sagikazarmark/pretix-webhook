use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Fields shared by every webhook payload emitted by pretix core.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Notification {
    /// Pretix's identifier for this webhook delivery.
    pub notification_id: u64,
    /// The exact action string that selects the payload family.
    pub action: String,
}

/// An identifier from a generic Django object relation.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ResourceId {
    /// A numeric object identifier.
    Integer(u64),
    /// A string object identifier used by integrations with non-numeric keys.
    String(String),
}

/// A webhook concerning an order.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct OrderEvent {
    #[serde(flatten)]
    /// Fields shared by every webhook payload.
    pub notification: Notification,
    /// The organizer that owns the order.
    pub organizer: String,
    /// The event that owns the order.
    pub event: String,
    /// The human-facing order code.
    pub code: String,
}

/// A ticket check-in or reverted check-in.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct CheckinEvent {
    #[serde(flatten)]
    /// The order and common notification fields for this check-in.
    pub order: OrderEvent,
    /// The database identifier of the checked-in order position, when supplied.
    pub orderposition_id: Option<u64>,
    /// The position number within the order, when supplied.
    pub orderposition_positionid: Option<u64>,
    /// The check-in list identifier, when supplied.
    pub checkin_list: Option<u64>,
    #[serde(rename = "type")]
    /// The check-in type from the wire `type` field, when supplied.
    pub kind: Option<String>,
    /// Whether this was the position's first check-in, when supplied.
    pub first_checkin: Option<bool>,
}

/// A webhook concerning the event itself.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct EventEvent {
    #[serde(flatten)]
    /// Fields shared by every webhook payload.
    pub notification: Notification,
    /// The organizer that owns the event.
    pub organizer: String,
    /// The affected event slug.
    pub event: String,
}

/// A webhook concerning a voucher.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct VoucherEvent {
    #[serde(flatten)]
    /// The affected event and common notification fields.
    pub event: EventEvent,
    /// The affected voucher's object identifier.
    pub voucher: ResourceId,
}

/// A webhook concerning a date in an event series.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct SubeventEvent {
    #[serde(flatten)]
    /// The affected event and common notification fields.
    pub event: EventEvent,
    /// The affected event-series date's object identifier.
    pub subevent: ResourceId,
}

/// A webhook concerning an item or quota.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ItemEvent {
    #[serde(flatten)]
    /// The affected event and common notification fields.
    pub event: EventEvent,
    /// The affected item or quota object identifier.
    pub item: ResourceId,
}

/// A webhook concerning a waiting-list entry.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct WaitingListEvent {
    #[serde(flatten)]
    /// The affected event and common notification fields.
    pub event: EventEvent,
    /// The affected waiting-list entry's object identifier.
    pub waitinglistentry: ResourceId,
}

/// A webhook concerning an organizer customer.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct CustomerEvent {
    #[serde(flatten)]
    /// Fields shared by every webhook payload.
    pub notification: Notification,
    /// The organizer that owns the customer.
    pub organizer: String,
    /// The affected customer identifier.
    pub customer: String,
}

/// A webhook concerning a gift card.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct GiftCardEvent {
    #[serde(flatten)]
    /// Fields shared by every webhook payload.
    pub notification: Notification,
    /// The numeric identifier of the issuing organizer.
    pub issuer_id: u64,
    /// The slug of the issuing organizer.
    pub issuer_slug: String,
    /// The affected gift-card identifier.
    pub giftcard: u64,
}

/// A webhook concerning a gift-card transaction.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct GiftCardTransactionEvent {
    #[serde(flatten)]
    /// The affected gift card and common notification fields.
    pub gift_card: GiftCardEvent,
    /// The accepting organizer's numeric identifier, when applicable.
    pub acceptor_id: Option<u64>,
    /// The accepting organizer's slug, when applicable.
    pub acceptor_slug: Option<String>,
}

/// A plugin-defined or future pretix payload.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct UnknownEvent {
    #[serde(flatten)]
    /// The required common notification envelope.
    pub notification: Notification,
    #[serde(flatten)]
    /// Every payload field outside the common notification envelope.
    ///
    /// Keys named `notification_id` or `action` are reserved by
    /// [`notification`](Self::notification).
    pub fields: BTreeMap<String, Value>,
}

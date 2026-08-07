use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Fields shared by every webhook payload emitted by pretix core.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Notification {
    pub notification_id: u64,
    pub action: String,
}

/// An identifier from a generic Django object relation.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ResourceId {
    Integer(u64),
    String(String),
}

/// A webhook concerning an order.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct OrderEvent {
    #[serde(flatten)]
    pub notification: Notification,
    pub organizer: String,
    pub event: String,
    pub code: String,
}

/// A ticket check-in or reverted check-in.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct CheckinEvent {
    #[serde(flatten)]
    pub order: OrderEvent,
    pub orderposition_id: Option<u64>,
    pub orderposition_positionid: Option<u64>,
    pub checkin_list: Option<u64>,
    #[serde(rename = "type")]
    pub kind: Option<String>,
    pub first_checkin: Option<bool>,
}

/// A webhook concerning the event itself.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct EventEvent {
    #[serde(flatten)]
    pub notification: Notification,
    pub organizer: String,
    pub event: String,
}

/// A webhook concerning a voucher.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct VoucherEvent {
    #[serde(flatten)]
    pub event: EventEvent,
    pub voucher: ResourceId,
}

/// A webhook concerning a date in an event series.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct SubeventEvent {
    #[serde(flatten)]
    pub event: EventEvent,
    pub subevent: ResourceId,
}

/// A webhook concerning an item or quota.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ItemEvent {
    #[serde(flatten)]
    pub event: EventEvent,
    pub item: ResourceId,
}

/// A webhook concerning a waiting-list entry.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct WaitingListEvent {
    #[serde(flatten)]
    pub event: EventEvent,
    pub waitinglistentry: ResourceId,
}

/// A webhook concerning an organizer customer.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct CustomerEvent {
    #[serde(flatten)]
    pub notification: Notification,
    pub organizer: String,
    pub customer: String,
}

/// A webhook concerning a gift card.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct GiftCardEvent {
    #[serde(flatten)]
    pub notification: Notification,
    pub issuer_id: u64,
    pub issuer_slug: String,
    pub giftcard: u64,
}

/// A webhook concerning a gift-card transaction.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct GiftCardTransactionEvent {
    #[serde(flatten)]
    pub gift_card: GiftCardEvent,
    pub acceptor_id: Option<u64>,
    pub acceptor_slug: Option<String>,
}

/// A plugin-defined or future pretix payload.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct UnknownEvent {
    #[serde(flatten)]
    pub notification: Notification,
    #[serde(flatten)]
    pub fields: BTreeMap<String, Value>,
}

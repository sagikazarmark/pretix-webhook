//! Typed payloads sent by pretix webhooks.
//!
//! Pretix only guarantees `notification_id` and `action` across all core
//! payloads. This crate dispatches known core actions to their actual payload
//! shapes and preserves plugin-defined actions in [`UnknownEvent`].

use std::collections::BTreeMap;

use serde::{Deserialize, Deserializer, Serialize, Serializer, de::DeserializeOwned};
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

/// The payload family selected from a webhook action.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WebhookEventKind {
    Order,
    Checkin,
    Event,
    Voucher,
    Subevent,
    Item,
    WaitingList,
    Customer,
    GiftCard,
    GiftCardTransaction,
    Unknown,
}

/// A typed pretix webhook payload.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum WebhookEvent {
    Order(OrderEvent),
    Checkin(CheckinEvent),
    Event(EventEvent),
    Voucher(VoucherEvent),
    Subevent(SubeventEvent),
    Item(ItemEvent),
    WaitingList(WaitingListEvent),
    Customer(CustomerEvent),
    GiftCard(GiftCardEvent),
    GiftCardTransaction(GiftCardTransactionEvent),
    Unknown(UnknownEvent),
}

impl WebhookEvent {
    #[must_use]
    pub fn kind(&self) -> WebhookEventKind {
        match self {
            Self::Order(_) => WebhookEventKind::Order,
            Self::Checkin(_) => WebhookEventKind::Checkin,
            Self::Event(_) => WebhookEventKind::Event,
            Self::Voucher(_) => WebhookEventKind::Voucher,
            Self::Subevent(_) => WebhookEventKind::Subevent,
            Self::Item(_) => WebhookEventKind::Item,
            Self::WaitingList(_) => WebhookEventKind::WaitingList,
            Self::Customer(_) => WebhookEventKind::Customer,
            Self::GiftCard(_) => WebhookEventKind::GiftCard,
            Self::GiftCardTransaction(_) => WebhookEventKind::GiftCardTransaction,
            Self::Unknown(_) => WebhookEventKind::Unknown,
        }
    }

    #[must_use]
    pub fn notification_id(&self) -> u64 {
        self.notification().notification_id
    }

    #[must_use]
    pub fn action(&self) -> &str {
        &self.notification().action
    }

    #[must_use]
    pub fn organizer_slug(&self) -> Option<&str> {
        match self {
            Self::Order(event) => Some(&event.organizer),
            Self::Checkin(event) => Some(&event.order.organizer),
            Self::Event(event) => Some(&event.organizer),
            Self::Voucher(event) => Some(&event.event.organizer),
            Self::Subevent(event) => Some(&event.event.organizer),
            Self::Item(event) => Some(&event.event.organizer),
            Self::WaitingList(event) => Some(&event.event.organizer),
            Self::Customer(event) => Some(&event.organizer),
            Self::GiftCard(event) => Some(&event.issuer_slug),
            Self::GiftCardTransaction(event) => Some(&event.gift_card.issuer_slug),
            Self::Unknown(event) => event
                .fields
                .get("organizer")
                .or_else(|| event.fields.get("issuer_slug"))
                .and_then(Value::as_str),
        }
    }

    #[must_use]
    pub fn event_slug(&self) -> Option<&str> {
        match self {
            Self::Order(event) => Some(&event.event),
            Self::Checkin(event) => Some(&event.order.event),
            Self::Event(event) => Some(&event.event),
            Self::Voucher(event) => Some(&event.event.event),
            Self::Subevent(event) => Some(&event.event.event),
            Self::Item(event) => Some(&event.event.event),
            Self::WaitingList(event) => Some(&event.event.event),
            Self::Customer(_) | Self::GiftCard(_) | Self::GiftCardTransaction(_) => None,
            Self::Unknown(event) => event.fields.get("event").and_then(Value::as_str),
        }
    }

    #[must_use]
    pub fn as_order(&self) -> Option<&OrderEvent> {
        if let Self::Order(event) = self {
            Some(event)
        } else {
            None
        }
    }

    #[must_use]
    pub fn as_checkin(&self) -> Option<&CheckinEvent> {
        if let Self::Checkin(event) = self {
            Some(event)
        } else {
            None
        }
    }

    fn notification(&self) -> &Notification {
        match self {
            Self::Order(event) => &event.notification,
            Self::Checkin(event) => &event.order.notification,
            Self::Event(event) => &event.notification,
            Self::Voucher(event) => &event.event.notification,
            Self::Subevent(event) => &event.event.notification,
            Self::Item(event) => &event.event.notification,
            Self::WaitingList(event) => &event.event.notification,
            Self::Customer(event) => &event.notification,
            Self::GiftCard(event) => &event.notification,
            Self::GiftCardTransaction(event) => &event.gift_card.notification,
            Self::Unknown(event) => &event.notification,
        }
    }
}

impl Serialize for WebhookEvent {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match self {
            Self::Order(event) => event.serialize(serializer),
            Self::Checkin(event) => event.serialize(serializer),
            Self::Event(event) => event.serialize(serializer),
            Self::Voucher(event) => event.serialize(serializer),
            Self::Subevent(event) => event.serialize(serializer),
            Self::Item(event) => event.serialize(serializer),
            Self::WaitingList(event) => event.serialize(serializer),
            Self::Customer(event) => event.serialize(serializer),
            Self::GiftCard(event) => event.serialize(serializer),
            Self::GiftCardTransaction(event) => event.serialize(serializer),
            Self::Unknown(event) => event.serialize(serializer),
        }
    }
}

impl<'de> Deserialize<'de> for WebhookEvent {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = Value::deserialize(deserializer)?;
        let action = value
            .get("action")
            .and_then(Value::as_str)
            .ok_or_else(|| serde::de::Error::missing_field("action"))?
            .to_owned();

        match action.as_str() {
            "pretix.event.checkin" | "pretix.event.checkin.reverted" => parse(value, Self::Checkin),
            action if action.starts_with("pretix.event.order.") => parse(value, Self::Order),
            "pretix.event.added"
            | "pretix.event.changed"
            | "pretix.event.deleted"
            | "pretix.event.live.activated"
            | "pretix.event.live.deactivated"
            | "pretix.event.testmode.activated"
            | "pretix.event.testmode.deactivated" => parse(value, Self::Event),
            action if action.starts_with("pretix.voucher.") => parse(value, Self::Voucher),
            action if action.starts_with("pretix.subevent.") => parse(value, Self::Subevent),
            action
                if action.starts_with("pretix.event.item.")
                    || action.starts_with("pretix.event.quota.") =>
            {
                parse(value, Self::Item)
            }
            action if action.starts_with("pretix.event.orders.waitinglist.") => {
                parse(value, Self::WaitingList)
            }
            action if action.starts_with("pretix.customer.") => parse(value, Self::Customer),
            action if action.starts_with("pretix.giftcards.transaction.") => {
                parse(value, Self::GiftCardTransaction)
            }
            action if action.starts_with("pretix.giftcards.") => parse(value, Self::GiftCard),
            _ => parse(value, Self::Unknown),
        }
        .map_err(serde::de::Error::custom)
    }
}

fn parse<T, F>(value: Value, wrap: F) -> serde_json::Result<WebhookEvent>
where
    T: DeserializeOwned,
    F: FnOnce(T) -> WebhookEvent,
{
    serde_json::from_value(value).map(wrap)
}

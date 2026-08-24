use serde::{Deserialize, Deserializer, Serialize, Serializer, de::DeserializeOwned};
use serde_json::Value;

use crate::payload::{
    CheckinEvent, CustomerEvent, EventEvent, GiftCardEvent, GiftCardTransactionEvent, ItemEvent,
    Notification, OrderEvent, SubeventEvent, UnknownEvent, VoucherEvent, WaitingListEvent,
};

/// The payload family selected from a webhook action.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WebhookEventKind {
    /// An order lifecycle event.
    Order,
    /// A ticket check-in or reverted check-in event.
    Checkin,
    /// An event configuration or lifecycle event.
    Event,
    /// A voucher event.
    Voucher,
    /// An event-series date event.
    Subevent,
    /// An item or quota event.
    Item,
    /// A waiting-list event.
    WaitingList,
    /// An organizer customer event.
    Customer,
    /// A gift-card event other than a transaction.
    GiftCard,
    /// A gift-card transaction event.
    GiftCardTransaction,
    /// A plugin-defined or otherwise unrecognized action.
    Unknown,
}

/// A typed pretix webhook payload.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum WebhookEvent {
    /// An order lifecycle payload.
    Order(OrderEvent),
    /// A ticket check-in or reverted check-in payload.
    Checkin(CheckinEvent),
    /// An event configuration or lifecycle payload.
    Event(EventEvent),
    /// A voucher payload.
    Voucher(VoucherEvent),
    /// An event-series date payload.
    Subevent(SubeventEvent),
    /// An item or quota payload.
    Item(ItemEvent),
    /// A waiting-list payload.
    WaitingList(WaitingListEvent),
    /// An organizer customer payload.
    Customer(CustomerEvent),
    /// A gift-card payload other than a transaction.
    GiftCard(GiftCardEvent),
    /// A gift-card transaction payload.
    GiftCardTransaction(GiftCardTransactionEvent),
    /// A valid payload whose action is not recognized by this crate.
    Unknown(UnknownEvent),
}

impl WebhookEvent {
    /// Returns the payload family selected from the action.
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

    /// Returns the delivery identifier supplied by pretix.
    #[must_use]
    pub fn notification_id(&self) -> u64 {
        self.notification().notification_id
    }

    /// Returns the exact action string supplied by pretix.
    #[must_use]
    pub fn action(&self) -> &str {
        &self.notification().action
    }

    /// Returns the organizer slug used for filtering, when present and valid.
    ///
    /// Gift-card payloads use their `issuer_slug`. Unknown payloads inspect the
    /// conventional `organizer` and `issuer_slug` fields.
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

    /// Reports whether the payload concerns one specific event.
    ///
    /// A payload that carries an event field is event-level even when the field
    /// cannot be read as a slug, so unreadable values fail an applicable event
    /// filter instead of being treated as organizer-level.
    #[must_use]
    pub fn is_event_level(&self) -> bool {
        match self {
            Self::Order(_)
            | Self::Checkin(_)
            | Self::Event(_)
            | Self::Voucher(_)
            | Self::Subevent(_)
            | Self::Item(_)
            | Self::WaitingList(_) => true,
            Self::Customer(_) | Self::GiftCard(_) | Self::GiftCardTransaction(_) => false,
            Self::Unknown(event) => event.fields.contains_key("event"),
        }
    }

    /// Returns the event slug used for filtering, when present and valid.
    ///
    /// Organizer-level payloads return `None`. Unknown payloads inspect the
    /// conventional `event` field.
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

    /// Borrows the payload when this is an order event.
    #[must_use]
    pub fn as_order(&self) -> Option<&OrderEvent> {
        if let Self::Order(event) = self {
            Some(event)
        } else {
            None
        }
    }

    /// Borrows the payload when this is a check-in event.
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

/// Wraps each payload type in its [`WebhookEvent`] variant.
macro_rules! from_payload {
    ($($payload:ident => $variant:ident),* $(,)?) => {
        $(
            impl From<$payload> for WebhookEvent {
                fn from(event: $payload) -> Self {
                    Self::$variant(event)
                }
            }
        )*
    };
}

from_payload! {
    OrderEvent => Order,
    CheckinEvent => Checkin,
    EventEvent => Event,
    VoucherEvent => Voucher,
    SubeventEvent => Subevent,
    ItemEvent => Item,
    WaitingListEvent => WaitingList,
    CustomerEvent => Customer,
    GiftCardEvent => GiftCard,
    GiftCardTransactionEvent => GiftCardTransaction,
    UnknownEvent => Unknown,
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
            "pretix.event.checkin" | "pretix.event.checkin.reverted" => {
                parse::<CheckinEvent>(value)
            }
            action if action.starts_with("pretix.event.order.") => parse::<OrderEvent>(value),
            "pretix.event.added"
            | "pretix.event.changed"
            | "pretix.event.deleted"
            | "pretix.event.live.activated"
            | "pretix.event.live.deactivated"
            | "pretix.event.testmode.activated"
            | "pretix.event.testmode.deactivated" => parse::<EventEvent>(value),
            action if action.starts_with("pretix.voucher.") => parse::<VoucherEvent>(value),
            action if action.starts_with("pretix.subevent.") => parse::<SubeventEvent>(value),
            action
                if action.starts_with("pretix.event.item.")
                    || action.starts_with("pretix.event.quota.") =>
            {
                parse::<ItemEvent>(value)
            }
            action if action.starts_with("pretix.event.orders.waitinglist.") => {
                parse::<WaitingListEvent>(value)
            }
            action if action.starts_with("pretix.customer.") => parse::<CustomerEvent>(value),
            action if action.starts_with("pretix.giftcards.transaction.") => {
                parse::<GiftCardTransactionEvent>(value)
            }
            action if action.starts_with("pretix.giftcards.") => parse::<GiftCardEvent>(value),
            _ => parse::<UnknownEvent>(value),
        }
        .map_err(serde::de::Error::custom)
    }
}

fn parse<T>(value: Value) -> serde_json::Result<WebhookEvent>
where
    T: DeserializeOwned + Into<WebhookEvent>,
{
    serde_json::from_value::<T>(value).map(Into::into)
}

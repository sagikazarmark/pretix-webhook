use std::{fmt::Display, future::Future};

use pretix_webhook_events::WebhookEvent;

/// Processes authenticated, parsed webhook events that passed their filters.
pub trait WebhookHandler: Send + Sync + 'static {
    /// The failure returned when an accepted event cannot be processed.
    type Error: Display + Send + Sync + 'static;

    /// Processes one accepted event.
    ///
    /// # Errors
    ///
    /// Returning an error produces a `500 Internal Server Error` response so
    /// pretix can retry the delivery.
    fn handle(&self, event: WebhookEvent) -> impl Future<Output = Result<(), Self::Error>> + Send;
}

impl<F, Fut, E> WebhookHandler for F
where
    F: Fn(WebhookEvent) -> Fut + Send + Sync + 'static,
    Fut: Future<Output = Result<(), E>> + Send,
    E: Display + Send + Sync + 'static,
{
    type Error = E;

    fn handle(&self, event: WebhookEvent) -> impl Future<Output = Result<(), Self::Error>> + Send {
        (self)(event)
    }
}

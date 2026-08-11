use std::{fmt::Display, future::Future};

use pretix_webhook_events::WebhookEvent;

/// Processes accepted webhook events.
pub trait WebhookHandler: Clone + Send + Sync + 'static {
    type Error: Display + Send + Sync + 'static;

    fn handle(&self, event: WebhookEvent) -> impl Future<Output = Result<(), Self::Error>> + Send;
}

/// A handler that acknowledges and discards every event.
///
/// With the `tracing` feature enabled, the router still records every accepted
/// event, so this is the handler to use for a receiver that only observes.
#[derive(Clone, Copy, Debug, Default)]
pub struct NoopHandler;

impl WebhookHandler for NoopHandler {
    type Error = std::convert::Infallible;

    async fn handle(&self, _event: WebhookEvent) -> Result<(), Self::Error> {
        Ok(())
    }
}

/// Adapts an async function or closure into a [`WebhookHandler`].
#[derive(Clone)]
pub struct FnHandler<F>(F);

/// Creates a handler from an async function or closure.
pub fn handler_fn<F>(function: F) -> FnHandler<F> {
    FnHandler(function)
}

impl<F, Fut, E> WebhookHandler for FnHandler<F>
where
    F: Fn(WebhookEvent) -> Fut + Clone + Send + Sync + 'static,
    Fut: Future<Output = Result<(), E>> + Send,
    E: Display + Send + Sync + 'static,
{
    type Error = E;

    fn handle(&self, event: WebhookEvent) -> impl Future<Output = Result<(), Self::Error>> + Send {
        (self.0)(event)
    }
}

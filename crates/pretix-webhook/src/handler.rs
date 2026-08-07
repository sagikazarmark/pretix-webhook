use std::{fmt::Display, future::Future};

use pretix_webhook_events::WebhookEvent;

#[cfg(any(feature = "log", feature = "tracing"))]
use crate::path::{WebhookPathError, validate_absolute_webhook_path};

/// Processes accepted webhook events.
pub trait WebhookHandler: Clone + Send + Sync + 'static {
    type Error: Display + Send + Sync + 'static;

    fn handle(&self, event: WebhookEvent) -> impl Future<Output = Result<(), Self::Error>> + Send;
}

/// A handler that acknowledges and discards every event.
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

/// A handler that logs semantic event fields through the `log` facade.
#[cfg(feature = "log")]
#[derive(Clone, Copy, Debug, Default)]
pub struct LogHandler;

#[cfg(feature = "log")]
impl LogHandler {
    /// Creates a log handler that identifies events with an exact webhook route.
    ///
    /// # Errors
    ///
    /// Returns [`WebhookPathError`] when `route` is not a valid absolute
    /// webhook path.
    pub fn with_route(
        route: impl Into<String>,
    ) -> Result<
        impl WebhookHandler<Error = std::convert::Infallible> + std::fmt::Debug,
        WebhookPathError,
    > {
        let route = route.into();
        validate_absolute_webhook_path(&route)?;
        Ok(RoutedLogHandler { route })
    }
}

#[cfg(feature = "log")]
impl WebhookHandler for LogHandler {
    type Error = std::convert::Infallible;

    async fn handle(&self, event: WebhookEvent) -> Result<(), Self::Error> {
        log::info!(
            notification_id = event.notification_id(),
            action = event.action(),
            organizer = event.organizer_slug(),
            pretix_event = event.event_slug(),
            kind:? = event.kind();
            "received pretix webhook"
        );
        Ok(())
    }
}

#[cfg(feature = "log")]
#[derive(Clone, Debug)]
struct RoutedLogHandler {
    route: String,
}

#[cfg(feature = "log")]
impl WebhookHandler for RoutedLogHandler {
    type Error = std::convert::Infallible;

    async fn handle(&self, event: WebhookEvent) -> Result<(), Self::Error> {
        log::info!(
            notification_id = event.notification_id(),
            action = event.action(),
            organizer = event.organizer_slug(),
            pretix_event = event.event_slug(),
            kind:? = event.kind(),
            route = self.route.as_str();
            "received pretix webhook"
        );
        Ok(())
    }
}

/// A handler that emits a structured semantic tracing event.
#[cfg(feature = "tracing")]
#[derive(Clone, Copy, Debug, Default)]
pub struct TracingHandler;

#[cfg(feature = "tracing")]
impl TracingHandler {
    /// Creates a tracing handler that identifies events with an exact webhook route.
    ///
    /// # Errors
    ///
    /// Returns [`WebhookPathError`] when `route` is not a valid absolute
    /// webhook path.
    pub fn with_route(
        route: impl Into<String>,
    ) -> Result<
        impl WebhookHandler<Error = std::convert::Infallible> + std::fmt::Debug,
        WebhookPathError,
    > {
        let route = route.into();
        validate_absolute_webhook_path(&route)?;
        Ok(RoutedTracingHandler { route })
    }
}

#[cfg(feature = "tracing")]
impl WebhookHandler for TracingHandler {
    type Error = std::convert::Infallible;

    async fn handle(&self, event: WebhookEvent) -> Result<(), Self::Error> {
        tracing::info!(
            notification_id = event.notification_id(),
            action = event.action(),
            organizer = event.organizer_slug(),
            pretix_event = event.event_slug(),
            kind = ?event.kind(),
            "received pretix webhook"
        );
        Ok(())
    }
}

#[cfg(feature = "tracing")]
#[derive(Clone, Debug)]
struct RoutedTracingHandler {
    route: String,
}

#[cfg(feature = "tracing")]
impl WebhookHandler for RoutedTracingHandler {
    type Error = std::convert::Infallible;

    async fn handle(&self, event: WebhookEvent) -> Result<(), Self::Error> {
        tracing::info!(
            notification_id = event.notification_id(),
            action = event.action(),
            organizer = event.organizer_slug(),
            pretix_event = event.event_slug(),
            kind = ?event.kind(),
            route = self.route.as_str(),
            "received pretix webhook"
        );
        Ok(())
    }
}

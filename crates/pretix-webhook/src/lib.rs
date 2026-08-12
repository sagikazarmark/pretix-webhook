//! Framework-independent Tower service for receiving pretix webhooks.
//!
//! [`WebhookServiceBuilder`] configures authentication, organizer and event
//! filters, and a request body limit. [`build`](WebhookServiceBuilder::build)
//! wraps a [`WebhookHandler`]. Functions and closures that accept a
//! [`WebhookEvent`](pretix_webhook_events::WebhookEvent) and return a `Send`
//! future implement that trait automatically:
//!
//! ```
//! use std::convert::Infallible;
//!
//! use pretix_webhook::WebhookServiceBuilder;
//! use pretix_webhook_events::WebhookEvent;
//!
//! let handler = |event: WebhookEvent| async move {
//!     println!("{}: {}", event.notification_id(), event.action());
//!     Ok::<_, Infallible>(())
//! };
//! let service = WebhookServiceBuilder::new()
//!     .allow_organizer("acmecorp")?
//!     .allow_event("democon")?
//!     .build(handler);
//! # let _ = service;
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```
//!
//! The resulting [`WebhookService`] implements
//! `tower::Service<http::Request<B>>` for request bodies whose data is
//! [`bytes::Bytes`]. It returns ordinary [`http::Response`] values and does not
//! depend on Axum or a runtime. Callers own URL and HTTP method routing; for
//! example, an Axum application can mount it with `post_service`:
//!
//! ```
//! use std::convert::Infallible;
//!
//! use axum::{Router, routing::post_service};
//! use pretix_webhook::WebhookServiceBuilder;
//! use pretix_webhook_events::WebhookEvent;
//!
//! let service = WebhookServiceBuilder::new()
//!     .build(|_event: WebhookEvent| async { Ok::<_, Infallible>(()) });
//! let app = Router::<()>::new().route("/webhook", post_service(service));
//! # let _: Router = app;
//! ```
//!
//! Authentication is checked before JSON parsing. Organizer and event filters
//! are independent and exact. Organizer-level payloads consult only the
//! organizer filter; an event-level payload with an unreadable event slug fails
//! a configured event filter.
//!
//! The service returns `204` on success, `400` for malformed payloads, `401`
//! for failed authentication, `404` for filtered events, `413` when the request
//! exceeds the configured body limit, and `500` when the handler returns an
//! error. Routing failures and unsupported methods are handled by the caller's
//! router.
//!
//! # Feature flags
//!
//! The default feature set is empty. The `tracing` feature opens a
//! `pretix_webhook` span containing the request URI path and, after parsing,
//! the event identity. Records emitted by the handler inherit that span.

mod builder;
mod handler;
mod service;

pub use builder::{BasicAuthCredential, WebhookFilterError, WebhookServiceBuilder};
pub use handler::WebhookHandler;
pub use service::{DEFAULT_BODY_LIMIT, WebhookResponse, WebhookService};

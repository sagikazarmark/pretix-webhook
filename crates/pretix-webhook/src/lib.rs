#![doc = include_str!("../README.md")]

mod config;
mod handler;
mod path;
mod router;

pub use config::{BasicAuthCredential, WebhookConfig, WebhookFilterError};
#[cfg(feature = "log")]
pub use handler::LogHandler;
#[cfg(feature = "tracing")]
pub use handler::TracingHandler;
pub use handler::{FnHandler, NoopHandler, WebhookHandler, handler_fn};
pub use path::{
    WebhookPathError, resolve_webhook_path, validate_absolute_webhook_path,
    validate_relative_webhook_path, validate_webhook_prefix,
};
pub use router::{MultiWebhookRouter, webhook_router, webhook_router_at};

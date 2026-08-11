# pretix-webhook

[![crates.io](https://img.shields.io/crates/v/pretix-webhook?style=flat-square)](https://crates.io/crates/pretix-webhook)
[![docs.rs](https://img.shields.io/docsrs/pretix-webhook?style=flat-square)](https://docs.rs/pretix-webhook)

**Tokio-free Axum routing, policy, HTTP Basic authentication, and handlers for
receiving [pretix webhooks](https://docs.pretix.eu/dev/api/webhooks.html).**

Add `pretix-webhook-events` when using event types directly.

## Quick Start

Implement `WebhookHandler` directly, or adapt an async closure with
`handler_fn`:

```rust
use pretix_webhook::{WebhookConfig, handler_fn, webhook_router_at};
use pretix_webhook_events::WebhookEvent;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let handler = handler_fn(|event: WebhookEvent| async move {
        println!("{}: {}", event.notification_id(), event.action());
        Ok::<_, std::convert::Infallible>(())
    });

    let config = WebhookConfig::new()
        .allow_organizer("acmecorp")?
        .allow_event("democon")?;

    let _router = webhook_router_at("/webhook", handler, config)?;

    Ok(())
}
```

Organizer and event filters are independent and exact. Every non-empty
applicable filter is enforced. Organizer-level payloads carry no event field, so
they consult only the organizer filter; a payload that carries an event field
whose value cannot be read as a slug is still event-level and fails a non-empty
event filter. An empty filter leaves that dimension unrestricted.

Add rotating credentials in the HTTP layer. Load passwords from your
deployment's secret source rather than hard-coding them:

```rust
use pretix_webhook::{BasicAuthCredential, WebhookConfig};

fn main() -> Result<(), std::env::VarError> {
    let old_password = std::env::var("PRETIX_WEBHOOK_OLD_PASSWORD")?;
    let current_password = std::env::var("PRETIX_WEBHOOK_CURRENT_PASSWORD")?;
    let _config = WebhookConfig::new().require_basic_auth([
        BasicAuthCredential::new("old-user", old_password),
        BasicAuthCredential::new("current-user", current_password),
    ]);

    Ok(())
}
```

HTTP Basic credentials are encoded rather than encrypted. Serve the endpoint
through HTTPS or terminate TLS at a trusted reverse proxy.

Passing an empty credential collection disables authentication.

## Feature Flags

- `tracing`: instruments webhook requests that reach the endpoint handler and
  emits structured acceptance and rejection records. The default feature set is
  empty. Enable it with
  `pretix-webhook = { version = "0.1", features = ["tracing"] }`; see the
  [crate-level feature documentation](https://docs.rs/pretix-webhook/latest/pretix_webhook/#feature-flags)
  for its behavior.

## Multiple webhooks

`MultiWebhookRouter` registers exact relative paths beneath one absolute
prefix. Every registration has independent filters, credentials, and a
concrete handler type. `finish` returns an ordinary Axum router that can be
merged into a larger application. See the compile-checked
[`multiple_webhooks.rs`](examples/multiple_webhooks.rs) example for a complete
configuration with independent routes, credentials, filters, and handlers.

The example configures `/hooks/sales/orders` and
`/hooks/operations/checkins`. A request is dispatched only to the handler at
its exact path; filters do not fan out requests between registrations.

`WebhookRouterBuilder` is the same builder for callers that already hold exact
absolute paths: `register_at` validates each path and returns an error on a
collision, where merging two Axum routers that share a route would panic.

The endpoint returns `204` on success, `400` for malformed payloads, `401` for
failed authentication, `404` for unsupported organizers/events, and `500` when
the handler fails so pretix retries delivery. Axum returns `405` for unsupported
methods and applies its default 2 MiB request-body limit before the handler,
returning `413` when the limit is exceeded. Applications can replace that limit
with Axum's `DefaultBodyLimit` layer.

The crate's normal dependency graph does not include Tokio; choose a runtime
when serving or testing the completed Axum router.

## Observability

The optional `tracing` feature instruments the endpoint itself, so enabling it
is all that is required; there is no handler to install and nothing to call.
Each POST request that reaches the webhook handler after routing and body
extraction opens a `pretix_webhook` span, which means a handler's own output
carries the route and the event's identity without the handler knowing about
either. Routing and extraction rejections such as `405` and `413` occur before
this span is created:

```rust
use pretix_webhook::{WebhookConfig, handler_fn, webhook_router_at};
use pretix_webhook_events::WebhookEvent;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let handler = handler_fn(|_event: WebhookEvent| async move {
        // Carries route, notification_id, action, organizer, pretix_event, and
        // kind from the enclosing span.
        tracing::info!("dispatching to fulfilment");
        Ok::<_, std::convert::Infallible>(())
    });

    let _router = webhook_router_at("/webhook", handler, WebhookConfig::new())?;

    Ok(())
}
```

The span carries `route` plus the event's `notification_id`, `action`,
`organizer`, `pretix_event`, and `kind`. `route` is recorded only for routers
built at an exact path; `webhook_router` is meant to be nested, so the path it
is finally served at is not known here. The identity fields are recorded once
the payload parses, so they are absent from records emitted before that point.

Within the span the endpoint emits:

| Level | Message                                           | When                          |
| ----- | ------------------------------------------------- | ----------------------------- |
| INFO  | `received pretix webhook`                         | accepted, before dispatch     |
| ERROR | `pretix webhook handler failed`                   | the handler returned an error |
| WARN  | `rejected unauthenticated pretix webhook request` | authentication failed         |
| WARN  | `rejected malformed pretix webhook payload`       | the body did not parse        |
| DEBUG | `rejected filtered pretix webhook event`          | filters excluded the event    |

Records are emitted whenever the feature is compiled in; silence them per
deployment through the subscriber's filter rather than at compile time, for
example `RUST_LOG=pretix_webhook=off`. Use `NoopHandler` for a receiver that
only observes.

## License

Licensed under either of

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE) or <https://www.apache.org/licenses/LICENSE-2.0>)
- MIT license ([LICENSE-MIT](LICENSE-MIT) or <https://opensource.org/licenses/MIT>)

at your option.

### Contribution

Unless you explicitly state otherwise, any contribution intentionally submitted
for inclusion in the work by you, as defined in the Apache-2.0 license, shall be
dual licensed as above, without any additional terms or conditions.

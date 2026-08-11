# pretix-webhook

Tokio-free Axum routing, policy, HTTP Basic authentication, and handlers for
receiving [pretix webhooks]. Add `pretix-webhook-events` when using event types
directly.

## Single webhook

Implement `WebhookHandler` directly, or adapt an async closure with
`handler_fn`:

```rust
use pretix_webhook::{WebhookConfig, handler_fn, webhook_router_at};
use pretix_webhook_events::WebhookEvent;

let handler = handler_fn(|event: WebhookEvent| async move {
    println!("{}: {}", event.notification_id(), event.action());
    Ok::<_, std::convert::Infallible>(())
});

let config = WebhookConfig::new()
    .allow_organizer("acmecorp")?
    .allow_event("democon")?;

let router = webhook_router_at("/webhook", handler, config)?;
# Ok::<(), Box<dyn std::error::Error>>(())
```

Organizer and event filters are independent and exact. Every non-empty
applicable filter is enforced. Organizer-level payloads carry no event field, so
they consult only the organizer filter; a payload that carries an event field
whose value cannot be read as a slug is still event-level and fails a non-empty
event filter. An empty filter leaves that dimension unrestricted.

Add rotating credentials in the HTTP layer. Load passwords from your
deployment's secret source rather than hard-coding them:

```rust,no_run
use pretix_webhook::{BasicAuthCredential, WebhookConfig};

let old_password = std::env::var("PRETIX_WEBHOOK_OLD_PASSWORD")?;
let current_password = std::env::var("PRETIX_WEBHOOK_CURRENT_PASSWORD")?;
let config = WebhookConfig::new().require_basic_auth([
    BasicAuthCredential::new("old-user", old_password),
    BasicAuthCredential::new("current-user", current_password),
]);
# let _ = config;
# Ok::<(), std::env::VarError>(())
```

## Multiple webhooks

`MultiWebhookRouter` registers exact relative paths beneath one absolute
prefix. Every registration has independent filters, credentials, and a
concrete handler type. `finish` returns an ordinary Axum router that can be
merged into a larger application:

```rust
use axum::{Router, routing::get};
use pretix_webhook::{
    BasicAuthCredential, MultiWebhookRouter, NoopHandler, WebhookConfig,
    handler_fn,
};
use pretix_webhook_events::WebhookEvent;

fn application(
    sales_password: &str,
    operations_password: &str,
) -> Result<Router, Box<dyn std::error::Error>> {
    let sales = WebhookConfig::new()
        .allow_organizer("acmecorp")?
        .allow_event("democon")?
        .require_basic_auth([BasicAuthCredential::new(
            "sales-webhook",
            sales_password,
        )]);
    let operations = WebhookConfig::new()
        .allow_organizer("acmecorp")?
        .require_basic_auth([BasicAuthCredential::new(
            "operations-webhook",
            operations_password,
        )]);

    let webhooks = MultiWebhookRouter::new("/hooks")?
        .register(
            "sales/orders",
            handler_fn(|event: WebhookEvent| async move {
                println!("sales event: {}", event.action());
                Ok::<_, std::convert::Infallible>(())
            }),
            sales,
        )?
        .register("operations/checkins", NoopHandler, operations)?
        .finish();

    Ok(Router::new()
        .route("/health", get(|| async { "ok" }))
        .merge(webhooks))
}
# let _ = application;
```

The example exposes `/hooks/sales/orders` and
`/hooks/operations/checkins`. A request is dispatched only to the handler at
its exact path; filters do not fan out requests between registrations.

`WebhookRouterBuilder` is the same builder for callers that already hold exact
absolute paths: `register_at` validates each path and returns an error on a
collision, where merging two Axum routers that share a route would panic.

The endpoint returns `204` on success, `400` for malformed payloads, `401` for
failed authentication, `404` for unsupported organizers/events, and `500` when
the handler fails so pretix retries delivery.

The crate's normal dependency graph does not include Tokio; choose a runtime
when serving or testing the completed Axum router.

## Observability

The optional `tracing` feature instruments the endpoint itself, so enabling it
is all that is required — there is no handler to install and nothing to call.
Every request opens a `pretix_webhook` span, which means a handler's own output
carries the route and the event's identity without the handler knowing about
either:

```rust
use pretix_webhook::{WebhookConfig, handler_fn, webhook_router_at};
use pretix_webhook_events::WebhookEvent;

let handler = handler_fn(|event: WebhookEvent| async move {
    // Carries route, notification_id, action, organizer, pretix_event, and
    // kind from the enclosing span.
    tracing::info!("dispatching to fulfilment");
    Ok::<_, std::convert::Infallible>(())
});

let router = webhook_router_at("/webhook", handler, WebhookConfig::new())?;
# Ok::<(), Box<dyn std::error::Error>>(())
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

[pretix webhooks]: https://docs.pretix.eu/dev/api/webhooks.html

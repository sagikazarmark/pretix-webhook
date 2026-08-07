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
applicable filter is enforced. Organizer-level payloads have no event slug, so
they consult only the organizer filter. An empty filter leaves that dimension
unrestricted.

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

The endpoint returns `204` on success, `400` for malformed payloads, `401` for
failed authentication, `404` for unsupported organizers/events, and `500` when
the handler fails so pretix retries delivery.

Optional `log` and `tracing` features provide `LogHandler` and
`TracingHandler`. `NoopHandler` and `handler_fn` are always available. The
crate's normal dependency graph does not include Tokio; choose a runtime when
serving or testing the completed Axum router.

[pretix webhooks]: https://docs.pretix.eu/dev/api/webhooks.html

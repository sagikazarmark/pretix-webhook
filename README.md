# pretix-webhook

[![OpenSSF Scorecard](https://api.securityscorecards.dev/projects/github.com/sagikazarmark/pretix-webhook/badge?style=flat-square)](https://securityscorecards.dev/viewer/?uri=github.com/sagikazarmark/pretix-webhook)
[![crates.io](https://img.shields.io/crates/v/pretix-webhook?style=flat-square)](https://crates.io/crates/pretix-webhook)
[![docs.rs](https://img.shields.io/docsrs/pretix-webhook?style=flat-square)](https://docs.rs/pretix-webhook)

**Receive and process [pretix webhooks] in Rust.**

## Features

- **Typed event payloads** for orders, check-ins, events, vouchers, sub-events,
  items/quotas, waiting-list entries, customers, and gift cards; unknown plugin
  actions preserve all JSON fields
- **Tokio-free Axum receiver** — applications choose the runtime that serves
  the router
- **Per-route policy** with independent organizer/event filters and HTTP Basic
  authentication (with credential rotation)
- **Multi-webhook builder** for registering several handlers beneath a shared
  prefix
- **Ready-to-run CLI server** with flag, environment variable, and TOML
  configuration

The workspace contains three crates:

- [`pretix-webhook-events`](crates/pretix-webhook-events): serializable typed
  payloads with no HTTP dependency.
- [`pretix-webhook`](crates/pretix-webhook): Axum router, policy, Basic
  authentication, and handler trait.
- [`pretix-webhook-cli`](crates/pretix-webhook-cli): a native HTTP server that
  logs accepted webhooks.

## Quickstart

Add the receiver crates and a runtime to a binary crate:

```toml
[dependencies]
pretix-webhook = "0.1"
pretix-webhook-events = "0.1"
axum = "0.8"
tokio = { version = "1", features = ["macros", "net", "rt-multi-thread"] }
```

Build a router from a handler and a config, then serve it with the runtime of
your choice:

```rust
use pretix_webhook::{WebhookConfig, handler_fn, webhook_router_at};
use pretix_webhook_events::WebhookEvent;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let handler = handler_fn(|event: WebhookEvent| async move {
        println!("{}: {}", event.notification_id(), event.action());
        Ok::<_, std::convert::Infallible>(())
    });

    let config = WebhookConfig::new()
        .allow_organizer("acmecorp")?
        .allow_event("democon")?;

    let app = webhook_router_at("/webhook", handler, config)?;

    let listener = tokio::net::TcpListener::bind("127.0.0.1:3000").await?;
    axum::serve(listener, app).await?;

    Ok(())
}
```

Point a pretix webhook at `http://your-host:3000/webhook` and it will accept
deliveries for the `acmecorp` organizer's `democon` event. Filters are
independent and exact; an omitted filter leaves that dimension unrestricted.

Add HTTP Basic authentication to the config, loading passwords from your
deployment's secret source rather than hard-coding them:

```rust
use pretix_webhook::{BasicAuthCredential, WebhookConfig};

let config = WebhookConfig::new().require_basic_auth([
    BasicAuthCredential::new("old-user", old_password),
    BasicAuthCredential::new("current-user", current_password),
]);
```

Listing multiple credentials keeps the old one valid while pretix is switched
over to the new one.

## Writing a handler

`handler_fn` adapts any async function or closure, as shown above. For handlers
with state (an API client, a database pool), implement `WebhookHandler` on your
own type:

```rust
use pretix_webhook::WebhookHandler;
use pretix_webhook_events::WebhookEvent;

#[derive(Clone)]
struct OrderHandler {
    pretix: PretixClient,
}

impl WebhookHandler for OrderHandler {
    type Error = Box<dyn std::error::Error + Send + Sync>;

    async fn handle(&self, event: WebhookEvent) -> Result<(), Self::Error> {
        let Some(order) = event.as_order() else {
            return Ok(());
        };

        // Webhook payloads are only triggers: fetch trusted state from the
        // authenticated pretix API before acting on it.
        let details = self
            .pretix
            .order(&order.organizer, &order.event, &order.code)
            .await?;

        process(details).await
    }
}
```

Returning an error produces a `500` response, so pretix retries the delivery;
returning `Ok(())` acknowledges it with `204`. Pretix documents that
notifications can be duplicated, so handlers should be idempotent.

`NoopHandler` is always available, and the optional `log` and `tracing`
features provide `LogHandler` and `TracingHandler`.

## Multiple webhooks

`MultiWebhookRouter` registers exact relative paths beneath one absolute
prefix. Every registration has its own filters, credentials, and handler, and
path collisions are reported as errors instead of panics. `finish` returns an
ordinary Axum router that can be merged into a larger application:

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
```

The example exposes `/hooks/sales/orders` and `/hooks/operations/checkins`.
A request is dispatched only to the handler at its exact path; filters do not
fan out requests between registrations.

See the [receiver library guide] for checked examples covering the endpoint's
response codes, filter semantics, `WebhookRouterBuilder` for absolute paths,
and Axum composition.

## References

- [Pretix webhook receiving and retry behavior][pretix webhooks]
- [Pretix core webhook action types]
- [Pretix core payload builders]

[pretix webhooks]: https://docs.pretix.eu/dev/api/webhooks.html
[Pretix core webhook action types]: https://docs.pretix.eu/dev/api/resources/webhooks.html
[Pretix core payload builders]: https://github.com/pretix/pretix/blob/master/src/pretix/api/webhooks.py
[receiver library guide]: crates/pretix-webhook/README.md
[CLI operator guide]: crates/pretix-webhook-cli/README.md

## License

Licensed under either of

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE) or <http://www.apache.org/licenses/LICENSE-2.0>)
- MIT license ([LICENSE-MIT](LICENSE-MIT) or <http://opensource.org/licenses/MIT>)

at your option.

### Contribution

Unless you explicitly state otherwise, any contribution intentionally submitted
for inclusion in the work by you, as defined in the Apache-2.0 license, shall be
dual licensed as above, without any additional terms or conditions.

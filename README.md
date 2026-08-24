# pretix-webhook

[![ci](https://img.shields.io/github/actions/workflow/status/sagikazarmark/pretix-webhook/dagger.yaml?style=flat-square&label=ci)](https://github.com/sagikazarmark/pretix-webhook/actions/workflows/dagger.yaml)
[![openssf scorecard](https://api.securityscorecards.dev/projects/github.com/sagikazarmark/pretix-webhook/badge?style=flat-square)](https://securityscorecards.dev/viewer/?uri=github.com/sagikazarmark/pretix-webhook)
[![crates.io](https://img.shields.io/crates/v/pretix-webhook?style=flat-square)](https://crates.io/crates/pretix-webhook)
[![docs.rs](https://img.shields.io/docsrs/pretix-webhook?style=flat-square)](https://docs.rs/pretix-webhook)

**Receive and process [pretix webhooks](https://docs.pretix.eu/dev/api/webhooks.html) in Rust.**

## Features

- Typed event payloads for core pretix webhook actions, with unknown plugin
  actions preserving all JSON fields
- Framework-independent Tower receiver with ordinary HTTP request and response
  types
- Per-service organizer/event filters and rotating HTTP Basic credentials
- Caller-owned routing for straightforward integration with Axum and other
  Tower-compatible applications
- Built-in request body limits and retry-aware HTTP response mapping
- Optional structured `tracing` instrumentation
- Ready-to-run CLI server with flag, environment variable, and TOML configuration

The workspace contains three crates:

- [`pretix-webhook-events`](crates/pretix-webhook-events): serializable typed
  payloads with no HTTP dependency.
- [`pretix-webhook`](crates/pretix-webhook): framework-independent Tower service,
  policy, and HTTP Basic authentication.
- [`pretix-webhook-cli`](crates/pretix-webhook-cli): an Axum-based native HTTP
  server that logs accepted webhooks.

## Quickstart

Add the receiver crates, Axum, and a runtime:

```toml
[dependencies]
pretix-webhook = "0.1"
pretix-webhook-events = "0.1"
axum = "0.8"
tokio = { version = "1", features = ["macros", "net", "rt-multi-thread"] }
```

Build a webhook service around an event handler, then mount it at a caller-owned
route:

```rust
use std::convert::Infallible;

use axum::{Router, routing::post_service};
use pretix_webhook::WebhookServiceBuilder;
use pretix_webhook_events::WebhookEvent;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let handler = |event: WebhookEvent| async move {
        println!("{}: {}", event.notification_id(), event.action());
        Ok::<_, Infallible>(())
    };
    let webhook = WebhookServiceBuilder::new()
        .allow_organizer("acmecorp")?
        .allow_event("democon")?
        .build(handler);
    let app = Router::<()>::new().route("/webhook", post_service(webhook));

    let listener = tokio::net::TcpListener::bind("127.0.0.1:3000").await?;
    axum::serve(listener, app).await?;
    Ok(())
}
```

The router owns paths, methods, and composition. `WebhookService` owns Basic
authentication, bounded body reading, payload parsing, filtering, event
dispatch, and response mapping.

## Authentication

Load credentials from your deployment's secret source and add all credentials
that should be valid during rotation:

```rust
use pretix_webhook::{BasicAuthCredential, WebhookServiceBuilder};

# let (old_password, current_password) = ("old", "current");
let builder = WebhookServiceBuilder::new().require_basic_auth([
    BasicAuthCredential::new("old-user", old_password),
    BasicAuthCredential::new("current-user", current_password),
]);
# let _ = builder;
```

Serve authenticated endpoints through HTTPS or trusted TLS termination.

## Multiple Webhooks

Create one service per endpoint and mount each one with the caller's router.
Every service can use independent filters, credentials, body limits, and event
handlers. See the compile-checked
[`multiple_webhooks.rs`](crates/pretix-webhook/examples/multiple_webhooks.rs)
example.

## Response Contract

The receiver returns `204` on success, `400` for malformed payloads, `401` for
failed authentication, `404` for filtered events, `413` for a body over the
configured limit, and `500` when the event handler fails so pretix retries
the delivery. The caller's router determines unmatched-path and unsupported
method behavior.

Pretix notifications can be duplicated, so event processing should be
idempotent.

## References

- [Pretix webhook receiving and retry behavior](https://docs.pretix.eu/dev/api/webhooks.html)
- [Pretix core webhook action types](https://docs.pretix.eu/dev/api/resources/webhooks.html)
- [Pretix core payload builders](https://github.com/pretix/pretix/blob/master/src/pretix/api/webhooks.py)

## Development

The workspace requires Rust 1.85 or newer:

```console
cargo fmt --all --check
cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
cargo test --workspace --all-targets --locked
cargo test --workspace --doc --locked
cargo doc --workspace --no-deps --all-features --locked
```

## License

Licensed under either of

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE) or <https://www.apache.org/licenses/LICENSE-2.0>)
- MIT license ([LICENSE-MIT](LICENSE-MIT) or <https://opensource.org/licenses/MIT>)

at your option.

### Contribution

Unless you explicitly state otherwise, any contribution intentionally submitted
for inclusion in the work by you, as defined in the Apache-2.0 license, shall be
dual licensed as above, without any additional terms or conditions.

# pretix-webhook

[![crates.io](https://img.shields.io/crates/v/pretix-webhook?style=flat-square)](https://crates.io/crates/pretix-webhook)
[![docs.rs](https://img.shields.io/docsrs/pretix-webhook?style=flat-square)](https://docs.rs/pretix-webhook)

**A framework-independent Tower service for receiving
[pretix webhooks](https://docs.pretix.eu/dev/api/webhooks.html).**

Add `pretix-webhook-events` when using event types directly.

## Quick Start

Build a webhook service around an async handler that accepts a `WebhookEvent`:

```rust
use std::convert::Infallible;

use pretix_webhook::WebhookServiceBuilder;
use pretix_webhook_events::WebhookEvent;

let handler = |event: WebhookEvent| async move {
    println!("{}: {}", event.notification_id(), event.action());
    Ok::<_, Infallible>(())
};

let webhook = WebhookServiceBuilder::new()
    .allow_organizer("acmecorp")?
    .allow_event("democon")?
    .build(handler);
# let _ = webhook;
# Ok::<(), Box<dyn std::error::Error>>(())
```

`WebhookService` implements `tower::Service<http::Request<B>>` and returns an
ordinary `http::Response`. It does not depend on Axum or Tokio.

Functions and closures returning a `Send` future implement `WebhookHandler`
automatically. Stateful handlers can implement the trait directly;
`WebhookService` stores the handler in an `Arc`, so it does not need to
implement `Clone`.

Routing belongs to the caller. Axum applications can mount the service directly:

```rust
use axum::{Router, routing::post_service};
# use std::convert::Infallible;
# use pretix_webhook::WebhookServiceBuilder;
# use pretix_webhook_events::WebhookEvent;
# let webhook = WebhookServiceBuilder::new()
#     .build(|_event: WebhookEvent| async { Ok::<_, Infallible>(()) });

let app = Router::<()>::new().route("/webhook", post_service(webhook));
# let _: Router = app;
```

Applications select the path, HTTP method, and routing behavior. Independent
services can be mounted at as many routes as required, each with its own policy
and event handler.

## Authentication And Filtering

Add rotating HTTP Basic credentials through the builder. Load passwords from a
secret source rather than hard-coding them:

```rust
use pretix_webhook::{BasicAuthCredential, WebhookServiceBuilder};

# let (old_password, current_password) = ("old", "current");
let builder = WebhookServiceBuilder::new().require_basic_auth([
    BasicAuthCredential::new("old-user", old_password),
    BasicAuthCredential::new("current-user", current_password),
]);
# let _ = builder;
```

HTTP Basic credentials are encoded rather than encrypted. Serve authenticated
endpoints through HTTPS or trusted TLS termination. An empty credential list
disables authentication.

Organizer and event filters are independent, exact, and case-sensitive. Empty
filters leave that dimension unrestricted. Organizer-level payloads consult
only the organizer filter.

## HTTP Contract

The service returns:

| Status | Meaning |
| --- | --- |
| `204 No Content` | The handler succeeded |
| `400 Bad Request` | The body failed or was not a valid webhook payload |
| `401 Unauthorized` | HTTP Basic authentication failed |
| `404 Not Found` | The event did not pass configured filters |
| `413 Payload Too Large` | The body exceeded the configured limit |
| `500 Internal Server Error` | The handler returned an error |

The default body limit is 2 MiB and can be changed with
`WebhookServiceBuilder::body_limit`. Unsupported methods and unmatched paths
are caller-router concerns.

## Observability

Enable the `tracing` feature to instrument requests that reach webhook
processing. The `pretix_webhook` span contains the request URI path and the
event's notification ID, action, organizer, event, and kind once parsing
succeeds. Records emitted by the handler inherit this span.

## License

Licensed under either of

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE) or <https://opensource.org/licenses/Apache-2.0>)
- MIT license ([LICENSE-MIT](LICENSE-MIT) or <https://opensource.org/licenses/MIT>)

at your option.

### Contribution

Unless you explicitly state otherwise, any contribution intentionally submitted
for inclusion in the work by you, as defined in the Apache-2.0 license, shall be
dual licensed as above, without any additional terms or conditions.

# pretix-webhook

Rust crates for receiving and processing [pretix webhooks]. The workspace
contains:

- `pretix-webhook-events`: serializable typed payloads with no HTTP dependency.
- `pretix-webhook`: a Tokio-free Axum router, policy, Basic authentication, and
  handler trait.
- `pretix-webhook-cli`: a native HTTP server that logs accepted webhooks.

Cloudflare Worker runtime integration is intentionally deferred. The event and
Axum crates do not select Axum's Tokio or HTTP-server features; a future Worker
crate can provide the runtime adapter without changing handlers or event types.

## Event model

Pretix payloads are not uniform. `notification_id` and `action` are common to
the core payloads, but organizer/event routing and resource identifiers differ:
gift-card events use `issuer_slug`, customer events have no event slug, and
check-ins add position and check-in-list fields.

`pretix_webhook_events::WebhookEvent` therefore dispatches known core actions
to typed variants for orders, check-ins, events, vouchers, sub-events,
items/quotas, waiting-list entries, customers, and gift cards. Unknown plugin
actions use `WebhookEvent::Unknown`, preserving all JSON fields so events can be
forwarded through a queue without depending on Axum.

Pretix documents that notifications can be duplicated and that webhook data
must only be used as a trigger to fetch trusted data from its authenticated API.
Handlers should be idempotent.

## Library

Implement `WebhookHandler` directly, or adapt an async closure with
`handler_fn`:

```rust
use pretix_webhook::{WebhookConfig, handler_fn, webhook_router_at};

let handler = handler_fn(|event| async move {
    println!("{}: {}", event.notification_id(), event.action());
    Ok::<_, std::convert::Infallible>(())
});

let config = WebhookConfig::new()
    .allow_event("acmecorp", "democon")
    .allow_all_events("another-organizer");

let router = webhook_router_at("/webhook", handler, config);
# let _ = router;
```

Add rotating Basic authentication credentials in the HTTP layer:

```rust
# use pretix_webhook::{BasicAuthCredential, WebhookConfig};
let config = WebhookConfig::new()
    .allow_event("acmecorp", "democon")
    .require_basic_auth([
        BasicAuthCredential::new("old-user", "old-password"),
        BasicAuthCredential::new("current-user", "current-password"),
    ]);
# let _ = config;
```

The endpoint returns `204` on success, `400` for malformed payloads, `401` for
failed authentication, `404` for unsupported organizers/events, and `500` when
the handler fails so pretix retries delivery.

Optional library features provide terminal handlers:

- `log`: `LogHandler` emits semantic key-values through the `log` facade.
- `tracing`: `TracingHandler` emits structured semantic fields through
  `tracing`.

`NoopHandler` and `handler_fn` are always available.

## CLI

The CLI enables the `log` feature by default:

```console
cargo run -p pretix-webhook-cli --bin pretix-webhook -- \
  --allow acmecorp/democon \
  --allow another-organizer/* \
  --credential webhook-user:change-me
```

Every option has an environment equivalent:

| Option | Environment variable | Default |
| --- | --- | --- |
| `--bind` | `PRETIX_WEBHOOK_BIND` | `127.0.0.1:3000` |
| `--path` | `PRETIX_WEBHOOK_PATH` | `/webhook` |
| `--allow` | `PRETIX_WEBHOOK_ALLOW` | all organizers and events |
| `--credential` | `PRETIX_WEBHOOK_CREDENTIALS` | authentication disabled |

Use semicolons between multiple values in environment variables:

```console
PRETIX_WEBHOOK_ALLOW='acmecorp/democon;another-organizer/*' \
PRETIX_WEBHOOK_CREDENTIALS='old:secret;current:new-secret' \
cargo run -p pretix-webhook-cli --bin pretix-webhook
```

If no allowlist is supplied, the CLI prints a warning and accepts webhooks for
all organizers and events.

Build the CLI with structured tracing instead of default logging:

```console
cargo run -p pretix-webhook-cli --bin pretix-webhook \
  --no-default-features --features tracing -- --allow acmecorp/democon
```

## References

- [Pretix webhook receiving and retry behavior][pretix webhooks]
- [Pretix core webhook action types]
- [Pretix core payload builders]

[pretix webhooks]: https://docs.pretix.eu/dev/api/webhooks.html
[Pretix core webhook action types]: https://docs.pretix.eu/dev/api/resources/webhooks.html
[Pretix core payload builders]: https://github.com/pretix/pretix/blob/master/src/pretix/api/webhooks.py

## License

Licensed under either Apache-2.0 or MIT, at your option.

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

The receiver provides convenience routers for one webhook and a
`MultiWebhookRouter` builder for multiple exact paths beneath a shared prefix.
Each registration has independent organizer and event filters, credentials,
and a concrete handler. The completed Axum router can be served directly or
merged into a larger application.

See the [receiver library guide] for checked examples covering single and
multi-webhook builders, per-route handlers and credentials, independent
filters, and Axum composition. The receiver's normal dependency graph is
Tokio-free; applications choose the runtime used to serve the router.

## CLI

The server supports a simple single-webhook mode through flags or environment
variables and an explicit `--config` TOML mode for multiple routes. TOML routes
reference credential environment variables rather than containing secrets.
Configuration is validated before binding, and startup warns for each
unrestricted or unauthenticated route.

See the [CLI operator guide] for all options, prefix precedence, the reusable
TOML format, credential handling, validation behavior, security guidance, and
supported observability builds.

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

Licensed under either Apache-2.0 or MIT, at your option.

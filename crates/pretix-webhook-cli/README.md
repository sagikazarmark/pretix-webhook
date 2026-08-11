# pretix-webhook-cli

Native HTTP server for receiving
[pretix webhooks](https://docs.pretix.eu/dev/api/webhooks.html). It supports a
simple single-webhook mode and an explicit TOML multi-webhook mode.

## Simple mode

Options may be supplied as repeatable flags:

```console
pretix-webhook \
  --allow-organizer acmecorp \
  --allow-organizer another-organizer \
  --allow-event democon
```

Or through environment variables, using semicolons between multiple values:

```console
PRETIX_WEBHOOK_ALLOW_ORGANIZERS='acmecorp;another-organizer' \
PRETIX_WEBHOOK_ALLOW_EVENTS='democon;conference' \
PRETIX_WEBHOOK_CREDENTIALS="${OLD_WEBHOOK_CREDENTIAL};${CURRENT_WEBHOOK_CREDENTIAL}" \
pretix-webhook
```

Inject `OLD_WEBHOOK_CREDENTIAL` and `CURRENT_WEBHOOK_CREDENTIAL` through your
deployment's secret mechanism as exact `USERNAME:PASSWORD` values.

| Option | Environment variable | Default |
| --- | --- | --- |
| `--bind` | `PRETIX_WEBHOOK_BIND` | `127.0.0.1:3000` |
| `--path` | `PRETIX_WEBHOOK_PATH` | `/webhook` |
| `--allow-organizer` | `PRETIX_WEBHOOK_ALLOW_ORGANIZERS` | all organizers |
| `--allow-event` | `PRETIX_WEBHOOK_ALLOW_EVENTS` | all events |
| `--credential` | `PRETIX_WEBHOOK_CREDENTIALS` | authentication disabled |

Organizer and event filters are independent and exact. Every non-empty
applicable filter is enforced. Organizer-level payloads consult only the
organizer filter. An omitted filter leaves that dimension unrestricted.

Filter values must be exact, so an empty value is rejected rather than ignored.
To leave a dimension unrestricted, leave its variable unset; setting it to an
empty string is a configuration error, not an empty list. Repeating the same
slug is also rejected, so copy/paste mistakes stay visible.

Semicolons separate values only in the environment variables. A flag value is
always one exact slug or credential, so a flag is repeated rather than
delimited. When a flag is supplied, its environment variable is ignored rather
than merged.

## TOML multi-webhook mode

Multi-webhook mode is selected only by the explicit, flag-only `--config`
option. The file is loaded once at startup, must contain at least one webhook,
and rejects unknown fields. Entry paths are relative to the global prefix:

```toml
prefix = "/incoming"

[[webhooks]]
path = "sales/orders"
allow_organizers = ["acmecorp"]
allow_events = ["democon"]
credential_env = ["PRETIX_SALES_WEBHOOK_CURRENT", "PRETIX_SALES_WEBHOOK_NEXT"]

[[webhooks]]
path = "operations/checkins"
allow_organizers = ["acmecorp"]
credential_env = ["PRETIX_OPERATIONS_WEBHOOK"]
```

Each name in `credential_env` identifies an environment variable containing
exactly one `USERNAME:PASSWORD` value. Inject those variables through your
deployment's secret mechanism before starting the receiver:

```console
pretix-webhook --config webhooks.toml
```

`--prefix` or `PRETIX_WEBHOOK_PREFIX` can override the file's prefix. Prefix
precedence is `--prefix`, then `PRETIX_WEBHOOK_PREFIX`, then TOML `prefix`, then
the `/webhook` default. An overridden TOML `prefix` is still validated, so a
reusable file cannot rot unnoticed behind a deployment-specific override.
`--bind` and `PRETIX_WEBHOOK_BIND` work in both modes.

Simple endpoint settings (`--path`, filters, and literal credential inputs,
including their environment equivalents) cannot be combined with `--config`.
Prefix inputs require `--config`; the receiver never discovers a configuration
file from the environment.

All routes, filters, credential references, and collisions are validated
before the listener binds. Independently detectable semantic errors are
reported together. Missing, empty, or malformed credential variables fail
startup without printing their values.

An omitted or empty filter list leaves that dimension unrestricted. An omitted
or empty `credential_env` list disables authentication for that route. Startup
emits a route-specific warning when both filter dimensions are unrestricted or
authentication is disabled. HTTP Basic credentials are only encoded, not
encrypted: terminate TLS in front of the receiver and keep credential values
out of TOML, command-line arguments, logs, and source control.

## Observability

Each POST request that reaches a configured webhook handler after routing and
body extraction is recorded through `tracing`, along with startup diagnostics.
Routing and extraction rejections such as unsupported methods or oversized
bodies occur before the request span is created. `RUST_LOG` selects what is
emitted, defaulting to `pretix_webhook=info,pretix_webhook_cli=info`. Records
for accepted events carry the route and the event's identity; see the
`pretix-webhook` crate for the full field and message set.

## License

Licensed under either of

- Apache License, Version 2.0 ([LICENSE-APACHE](../../LICENSE-APACHE) or <https://www.apache.org/licenses/LICENSE-2.0>)
- MIT license ([LICENSE-MIT](../../LICENSE-MIT) or <https://opensource.org/licenses/MIT>)

at your option.

### Contribution

Unless you explicitly state otherwise, any contribution intentionally submitted
for inclusion in the work by you, as defined in the Apache-2.0 license, shall be
dual licensed as above, without any additional terms or conditions.

# Multiple Independently Configured Webhooks

Status: ready-for-agent

## Problem Statement

The receiver currently serves one webhook endpoint with one organizer/event policy, one optional HTTP Basic credential list, and one handler. This is sufficient for a small deployment, but it forces users who receive webhooks for independent Pretix organizers or events to run multiple server instances even when one process could safely host them.

Users need one receiver instance to expose multiple exact URL paths under a shared prefix. Each path must have an independent organizer filter, event filter, credential list, and Rust handler. At the same time, the common single-webhook deployment must remain simple to configure through CLI flags or environment variables.

The CLI currently has no structured configuration-file mode, and its combined `ORGANIZER/EVENT` allowlist cannot express the desired independent organizer and event dimensions. Multiple endpoints also introduce startup validation, route identity, diagnostics, and secret-handling requirements that the current flat configuration does not address.

## Solution

Provide a first-class multi-webhook router in the receiver library. It owns a global absolute URL prefix and accepts independently configured registrations at validated relative paths. Each registration installs its own configuration and concrete handler immediately, allowing registrations to use different handler types without introducing a type-erased handler registry. The completed router remains an ordinary Axum router that callers can serve or compose with other application routes.

Replace the combined organizer/event allowlist with independent organizer-slug and event-slug filters. Every non-empty filter is enforced when applicable; an empty filter leaves that dimension unrestricted. Organizer-level payloads consult only the organizer filter, while event-level payloads consult both filters. Matching follows Pretix's exact, case-sensitive slug behavior.

Retain a simple CLI mode for one absolute path configured by flags and environment variables. Add a mutually exclusive multi-webhook mode selected by an explicit TOML configuration-file flag. The file defines one or more webhooks at relative paths and may define the global prefix. Each webhook has independent filter arrays and a list of environment-variable names from which HTTP Basic credentials are resolved. The CLI uses its existing compile-time-selected built-in handler for all configured routes and includes each route's resolved path in logs.

Validate the full effective configuration before binding the listener. Reject ambiguous paths, duplicate routes, unknown fields, unresolved credentials, mixed simple/multi endpoint settings, and all other detectable semantic errors. Warn per route when filtering is unrestricted or authentication is disabled.

## User Stories

1. As a library user, I want to host multiple Pretix webhook endpoints in one Axum router, so that I do not need one process per integration.
2. As a library user, I want all registered endpoints to share a configurable URL prefix, so that I can group Pretix webhook routes under one namespace.
3. As a library user, I want to register each webhook at a relative path, so that route definitions remain concise and consistent with the shared prefix.
4. As a library user, I want relative paths to support multiple static segments, so that I can organize routes by integration, organizer, or event.
5. As a library user, I want each registration to have its own handler, so that unrelated integrations can perform different work.
6. As a library user, I want registrations to accept different concrete handler types, so that multi-webhook support does not force all handlers into one error or dispatch type.
7. As a library user, I want the finished multi-webhook router to remain an ordinary Axum router, so that I can merge or nest it in a larger application.
8. As a library user, I want the single-webhook router helpers to remain available, so that a simple receiver does not require the multi-webhook builder.
9. As a library user, I want handler input to remain a parsed `WebhookEvent`, so that multi-route support does not force an unrelated handler API migration.
10. As a library user, I want a new webhook configuration to permit all organizers and events by default, so that empty filters consistently mean unchecked dimensions.
11. As a library user, I want to allow organizer slugs independently, so that organizer-level events have an unambiguous policy.
12. As a library user, I want to allow event slugs independently, so that the same event filter can apply across any permitted organizer.
13. As a library user, I want every non-empty applicable filter to be enforced, so that organizer and event restrictions compose as an intersection.
14. As a library user, I want an empty organizer filter to permit any organizer, so that I can filter only by event slug.
15. As a library user, I want an empty event filter to permit any event, so that I can filter only by organizer slug.
16. As a library user, I want organizer-level payloads to ignore event filters, so that payloads without an event slug are not ambiguously classified against event names.
17. As a library user, I want organizer-level payloads to honor non-empty organizer filters, so that customer and gift-card events stay within the intended organizer scope.
18. As a library user, I want event-level payloads to honor both non-empty filters, so that a route can be constrained to a particular organizer and event.
19. As a security-conscious user, I want missing organizer or event data to fail an applicable non-empty filter, so that unknown payloads do not bypass configured restrictions.
20. As a Pretix administrator, I want slug matching to be exact and case-sensitive, so that receiver behavior matches Pretix lookups.
21. As a Pretix administrator, I want the receiver not to reimplement Pretix's current slug validator, so that legacy or version-specific identifiers remain usable.
22. As a CLI user, I want to configure one webhook with flags, so that the common deployment remains quick to start.
23. As a CLI user, I want to configure one webhook with environment variables, so that the common deployment works naturally in containers.
24. As a CLI user, I want separate organizer and event flags, so that simple mode uses the same filter model as the library.
25. As a CLI user, I want organizer and event flags to be repeatable, so that I can permit more than one slug in either dimension.
26. As a CLI user, I want list-valued environment variables to remain semicolon-separated, so that the existing deployment convention stays consistent.
27. As a CLI user, I want the obsolete combined allowlist option removed, so that there is only one clear filter model.
28. As a CLI user, I want simple mode to retain one configurable absolute path, so that existing single-endpoint deployments remain straightforward.
29. As an operator, I want to select multi-webhook mode with an explicit configuration-file flag, so that the server never discovers or activates a file unexpectedly.
30. As an operator, I want the configuration-file path to be flag-only, so that an inherited environment variable cannot silently change the operating mode.
31. As an operator, I want simple and multi endpoint settings to be mutually exclusive, so that source precedence cannot create partially merged webhook definitions.
32. As an operator, I want server-global bind configuration to remain available in both modes, so that routing configuration does not control listener placement.
33. As an operator, I want multi-mode prefix overrides through a flag or environment variable, so that deployment-specific URL mounting does not require editing the file.
34. As an operator, I want prefix precedence to be deterministic, so that CLI overrides environment, environment overrides TOML, TOML overrides the default.
35. As an operator, I want the default multi-webhook prefix to be `/webhook`, so that the default namespace remains familiar.
36. As an operator, I want `/` to be a valid prefix, so that I can expose configured webhooks at top-level paths.
37. As an operator, I want the prefix option to be invalid without a multi-webhook config, so that simple mode has one unambiguous route option.
38. As an operator, I want the simple absolute path option to be invalid with a multi-webhook config, so that multi mode uses only prefix-plus-relative-path routing.
39. As an operator, I want a TOML file with repeated webhook entries, so that independent routes are readable and reviewable.
40. As an operator, I want each webhook entry to define a relative path, so that its final URL is derived predictably from the global prefix.
41. As an operator, I want each webhook entry to define independent organizer and event arrays, so that route authorization policies do not leak across endpoints.
42. As an operator, I want omitted or empty filter arrays to mean unrestricted dimensions, so that concise configuration has the same semantics as library configuration.
43. As an operator, I want at least one webhook entry to be required, so that an accidentally empty server fails startup.
44. As an operator, I want unknown TOML fields rejected, so that misspelled security settings cannot silently become unrestricted behavior.
45. As an operator, I want one self-contained configuration file without includes, profiles, or inheritance, so that effective routing is easy to understand.
46. As an operator, I want configuration loaded once at startup, so that all routes and credentials change atomically through a process restart.
47. As a security-conscious operator, I want TOML to reference credential environment variables rather than contain literal credentials, so that reusable files do not encourage stored secrets.
48. As a security-conscious operator, I want each referenced variable to hold exactly one `USERNAME:PASSWORD` credential, so that secret parsing is deterministic.
49. As a security-conscious operator, I want multiple credential references per route, so that I can rotate credentials without downtime.
50. As a security-conscious operator, I want a missing credential reference to fail startup, so that authentication is never weakened by an environment mistake.
51. As a security-conscious operator, I want empty or malformed credential values to fail startup, so that invalid secrets cannot disable or corrupt authentication.
52. As a security-conscious operator, I want credential errors to identify the route and variable name without printing the value, so that I can fix configuration without leaking secrets.
53. As an operator, I want omitted credentials to disable authentication for that route, so that HTTP Basic authentication remains optional.
54. As an operator, I want an unauthenticated route to emit a startup warning, so that the exposure is visible.
55. As an operator, I want an unrestricted route to emit a startup warning, so that broad delivery is visible.
56. As an operator, I want the same security warnings in simple and multi mode, so that neither mode hides risky defaults.
57. As an operator, I want route paths restricted to stable URL-unreserved ASCII segments, so that encoded or normalized equivalents cannot create ambiguous routing.
58. As an operator, I want leading slashes rejected in relative paths, so that webhook entries cannot escape the shared-prefix model.
59. As an operator, I want trailing slashes and empty segments rejected, so that every configured route has one exact canonical form.
60. As an operator, I want `.` and `..` path segments rejected, so that routes cannot imply path traversal or normalization.
61. As an operator, I want dynamic route parameters, queries, and fragments rejected, so that every webhook is an exact static POST endpoint.
62. As an operator, I want duplicate resolved paths rejected, so that registration order never decides which webhook handles a request.
63. As an operator, I want duplicate filter entries and credential variable names rejected, so that copy/paste errors are visible.
64. As an operator, I want empty and whitespace-padded filter values rejected instead of normalized, so that configured values remain exact and auditable.
65. As an operator, I want all independent semantic configuration errors reported together, so that I can repair a file in one pass.
66. As an operator, I want syntax and type errors to identify their TOML source, so that malformed files are actionable.
67. As an operator, I want startup logs to show the bind address and route count, so that I can verify the effective server shape.
68. As an operator, I want one startup line per resolved route, so that I can confirm every expected endpoint is installed.
69. As an operator, I want event logs to include the resolved route path, so that otherwise similar Pretix events can be attributed to the receiving webhook.
70. As a security-conscious operator, I want logs to omit filter values, credential variable names, usernames, and secrets, so that diagnostics do not disclose configuration or authentication data.
71. As a CLI maintainer, I want all configured routes to use the existing compile-time-selected built-in handler, so that this feature does not introduce configurable plugins or commands.
72. As a CLI maintainer, I want log, tracing, and noop build variants to remain supported, so that multi-webhook support does not remove current feature combinations.
73. As an integration author, I want the URL path to select exactly one handler, so that overlapping filters never cause unintended fan-out.
74. As an integration author, I want authentication checked before payload parsing and filtering, so that protected routes retain their current information-disclosure behavior.
75. As an integration author, I want malformed payloads to return `400`, so that Pretix can distinguish invalid delivery from route filtering.
76. As an integration author, I want filter mismatches to return `404`, so that rejected targets retain current behavior.
77. As an integration author, I want failed authentication to return `401` with a Basic challenge, so that credentials retain current HTTP semantics.
78. As an integration author, I want successful handlers to return `204`, so that Pretix treats delivery as complete without a response body.
79. As an integration author, I want handler failures to return `500`, so that Pretix retains its retry behavior.
80. As a maintainer, I want path validation shared between simple and multi modes, so that the modes do not accept subtly different static-route syntax.
81. As a maintainer, I want multi-route behavior tested through public HTTP behavior, so that tests remain independent of router internals.
82. As a maintainer, I want CLI behavior tested at its parsing and process boundary, so that source precedence and diagnostics are verified as users experience them.

## Implementation Decisions

- The receiver library will add a first-class multi-webhook router builder. Construction validates one global absolute prefix. Registration validates a relative path, computes the resolved exact path, rejects collisions, and immediately installs the supplied handler and webhook configuration into the underlying Axum router.
- Registration will be generic over each concrete handler. The builder will not store heterogeneous handlers in a centralized registry and will not change the `WebhookHandler` trait's event-only input.
- The builder will produce an ordinary Axum router for serving or composition.
- Existing single-webhook router helpers remain as convenience APIs.
- `WebhookConfig` will model organizer and event filters as independent sets.
- A newly constructed `WebhookConfig` is unrestricted because both sets are empty.
- The configuration API exposes methods to add an organizer slug and an event slug independently. The combined organizer/event method, explicit allow-everything method, and organizer-wide event method are removed.
- Organizer-level payloads are payloads from which no event slug can be extracted. They consult only a non-empty organizer filter.
- Event-level payloads consult each non-empty organizer and event filter independently.
- A payload missing a field required by a non-empty applicable filter does not match.
- Slug comparisons are exact and case-sensitive, consistent with Pretix's ordinary exact model lookups. Configured values are not lowercased or otherwise normalized.
- Organizer and event filter validation rejects empty and whitespace-padded values but does not duplicate Pretix's current regex or length restrictions.
- The request pipeline and response contract remain authentication first, then parsing, filtering, and synchronous handler execution. Existing response codes and Basic challenge behavior remain unchanged.
- The URL path selects one webhook and one handler. Filtering does not dispatch or fan out requests to other registered webhooks.
- The CLI has two mutually exclusive modes: simple mode when no configuration-file flag is present and multi mode when an explicit configuration-file flag is present.
- The configuration-file path has no environment-variable equivalent and no automatic discovery.
- Simple mode retains an absolute path with `/webhook` as its default and uses separate repeatable organizer and event options.
- Simple-mode organizer, event, and credential environment variables retain semicolon-separated list values.
- The old combined allowlist CLI option and environment variable are removed.
- Multi mode accepts one TOML file. It contains an optional global `prefix` field and a required non-empty `webhooks` list. Each webhook contains `path`, optional `allow_organizers`, optional `allow_events`, and optional `credential_env` fields.
- Unknown fields are rejected at the TOML root and webhook-entry levels.
- Omitted or empty filter arrays leave that filter dimension unrestricted.
- Omitted or empty credential references disable HTTP Basic authentication for that webhook.
- Literal credentials are not accepted in TOML. Every credential reference names an environment variable containing exactly one `USERNAME:PASSWORD` value. Existing password support for additional colon characters is retained by splitting at the first colon.
- Credential references are resolved and validated at startup. A missing, empty, or malformed referenced value is a startup error. Diagnostics may identify the webhook and environment-variable name but never the variable value.
- The global bind address remains process-level CLI/environment configuration and does not move into TOML.
- Multi-mode prefix precedence is explicit CLI option, then prefix environment variable, then TOML, then `/webhook`.
- Simple-mode endpoint options, including path, organizer filters, event filters, and credentials, are rejected when multi mode is selected. Multi-mode prefix options are rejected in simple mode. Mixed settings are not treated as defaults or overrides.
- The config file is loaded once. Runtime reload, includes, inheritance, and profiles are not supported.
- Prefixes are `/` or absolute static paths without a trailing slash. Relative paths contain one or more segments and have no leading or trailing slash.
- Prefix and route segments allow only ASCII letters, digits, hyphen, dot, underscore, and tilde. Empty, `.` and `..` segments are invalid. Dynamic parameters, queries, fragments, percent-encoded forms, whitespace, and other characters are invalid.
- Simple-mode absolute paths use the same segment rules as multi-mode resolved paths, with `/` allowed.
- Duplicate resolved routes are startup or builder-registration errors. Duplicate organizer slugs, event slugs, and credential environment-variable names within one webhook entry are configuration errors.
- TOML syntax and deserialization errors may stop parsing immediately. After successful deserialization, all independently detectable semantic errors are accumulated and reported together with webhook indexes and resolved paths where available.
- The CLI uses the same compile-time-selected log, tracing, or noop handler for every configured route. TOML has no handler-selection field.
- Built-in event logging gains optional resolved-route identity without changing the handler trait input. The CLI supplies that identity when constructing each route's handler.
- Startup logging emits the bind address and route count followed by one informational entry per resolved route. It emits route-specific warnings for every unrestricted route and every route without authentication.
- Logs must not contain filter values, credential environment-variable names, usernames, passwords, or raw credentials.
- The CLI's public configuration model may change to represent the two operating modes and multiple effective endpoints; preserving the existing combined-allowlist API is not required at version `0.1.0`.
- Configuration is validated completely before listener binding or server startup.

## Testing Decisions

- Good tests assert external behavior through public APIs and process-visible diagnostics. They do not inspect private configuration sets, Axum state, hashing details, or internal builder storage.
- Two seams are required because the receiver library and CLI expose separate contracts. The highest receiver seam is an HTTP request sent to a completed public Axum router through Tower's in-process service interface. The highest practical CLI seams are its public configuration-loading boundary and subprocess startup behavior.
- Receiver HTTP tests will extend the existing router integration-test style that sends requests with representative Pretix payloads and records handler delivery.
- Receiver tests will verify that two different relative paths under one prefix dispatch only to their respective handlers, including different concrete handler types where practical.
- Receiver tests will verify exact route matching, root-prefix behavior, multi-segment relative paths, duplicate-path rejection, and all invalid path classes.
- Receiver tests will verify independent organizer and event filtering for event-level payloads: both constrained, organizer-only, event-only, and both unrestricted.
- Receiver tests will verify organizer-level payload filtering separately, including the rule that event filters are not applied.
- Receiver tests will verify missing-field behavior for unknown payloads against non-empty and empty filters.
- Receiver tests will verify exact case-sensitive slug matching.
- Receiver tests will preserve coverage for authentication precedence, malformed payloads, filter rejection, handler success, and handler failure status codes.
- Receiver tests will verify independent credential lists on different routes and ensure a credential accepted on one route is not accepted on another.
- Single-route tests will be updated to establish the new unrestricted default and independent builder methods.
- CLI configuration tests will extend the existing direct parser/environment-test style for simple-mode repeatable flags, semicolon-separated values, empty filters, unrestricted defaults, and removed combined allowlist behavior.
- CLI configuration tests will cover explicit config-file selection, the absence of config discovery, and rejection of every simple/multi source combination, including conflicting environment variables.
- CLI configuration tests will cover prefix precedence across CLI, environment, TOML, and the built-in default.
- TOML tests will cover a valid multi-webhook file, omitted optional fields, root prefix, multiple and multi-segment paths, an empty webhook list, unknown root fields, unknown entry fields, malformed types, duplicate paths, duplicate list entries, and aggregated semantic errors.
- Credential-resolution tests will use isolated environment setup and cover rotation, missing variables, empty values, malformed values, multiple colons in passwords, and diagnostics that do not contain secret values.
- CLI subprocess tests will extend the existing startup-diagnostic style. They will verify route count and resolved-route logs, unrestricted and unauthenticated warnings in both modes, and the absence of sensitive configuration values from output.
- Built-in handler logging tests will extend existing structured-log assertions to include resolved route identity while preserving current event fields.
- Tests will not require opening a real listening socket for normal routing coverage because the in-process public router seam exercises the receiver contract more directly and deterministically.

## Out of Scope

- Configurable CLI handler types, dynamic plugins, shell-command handlers, or per-route output destinations.
- Changing `WebhookHandler` to receive headers, paths, endpoint names, raw bodies, or request context.
- Runtime configuration or credential reload.
- Multiple listeners or per-webhook bind addresses.
- TLS termination or certificate management.
- Queueing, background acknowledgement, handler timeouts, retries performed by this server, deduplication, or idempotency storage.
- Request fan-out across matching webhook policies.
- Filtering by webhook action, typed event kind, notification ID, resource ID, gift-card acceptor, or other payload attributes.
- TOML includes, profiles, inheritance, interpolation, or alternate file formats.
- Literal credentials in configuration files, secret-manager integrations, or file-based secret references.
- Automatic configuration-file discovery or a configuration-file environment variable.
- Health, readiness, metrics, index, or administrative routes.
- Changing existing request-body, content-type, or unmatched-method behavior.
- Reimplementing or remotely validating Pretix organizer/event slug rules.
- Backward compatibility for the removed combined allowlist APIs and options before a stable release.

## Further Notes

- Pretix organizer and event model validators permit uppercase characters even though their help text recommends lowercase, and API detail lookup uses exact slug fields. Exact case-sensitive comparison is therefore safer than normalization.
- Empty filters deliberately mean unrestricted behavior in both the library and CLI. Startup warnings, strict unknown-field rejection, and fail-closed credential references make this broad default operationally visible without changing the chosen semantics.
- The relative path is the webhook's stable identity. A separate configured name is intentionally omitted until a concrete use requires another identifier.
- Merging independently state-bound Axum routers is the existing architectural seam that makes heterogeneous handlers practical. The new builder formalizes this seam while centralizing prefix joining, path validation, and duplicate detection.
- Direct TOML parsing is preferred over Figment because endpoint configuration sources are intentionally exclusive rather than generally merged. Clap remains responsible for flags and simple-mode environment variables.

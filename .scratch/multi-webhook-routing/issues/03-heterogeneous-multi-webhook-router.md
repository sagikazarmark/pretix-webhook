# 03 — Heterogeneous multi-webhook router

**What to build:** Let library users register multiple exact webhook endpoints beneath one global prefix, each with its own configuration and concrete handler, and finish with an ordinary Axum router that can be served or composed with unrelated application routes.

**Blocked by:** 01 — Shared exact webhook path grammar; 02 — Independent organizer and event filters.

**Status:** ready-for-agent

- [ ] Construction validates one absolute global prefix, and each registration validates and resolves a multi-segment relative path through the shared path grammar.
- [ ] Registrations may use different concrete handler and handler-error types without changing the event-only handler interface or introducing a type-erased handler registry.
- [ ] Each request dispatches to exactly the handler selected by its exact resolved URL path; filters never fan out delivery to another registration.
- [ ] Duplicate resolved paths are rejected deterministically before they can make registration order significant.
- [ ] Authentication credentials and filtering policies remain isolated per route, including credentials that are valid on only one of two routes.
- [ ] The completed router supports root prefixes, exact matching, merging with unrelated Axum routes, and all existing response semantics through public in-process HTTP tests.

# 05 — Route-aware built-in handlers

**What to build:** Let the built-in log and tracing handlers identify the resolved webhook route in event and failure records while preserving route-less construction for library users and leaving the handler trait's parsed-event input unchanged.

**Blocked by:** 01 — Shared exact webhook path grammar.

**Status:** ready-for-agent

- [ ] Log and tracing handlers can be constructed with a validated resolved route and emit that route as structured identity on handled events.
- [ ] Handler-failure records include the same optional route identity while preserving their current error information and HTTP behavior.
- [ ] Existing route-less construction remains supported and does not invent a route value.
- [ ] Structured logging tests preserve existing event fields and verify route identity without exposing filter values, credentials, or secrets.
- [ ] Log-only, tracing-only, and both-feature builds preserve the established observability selection behavior.

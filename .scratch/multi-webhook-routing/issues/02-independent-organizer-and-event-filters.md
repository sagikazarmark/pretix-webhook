# 02 — Independent organizer and event filters

**What to build:** Let library and simple CLI users constrain organizer slugs and event slugs independently, with exact Pretix-compatible matching and unrestricted dimensions represented by empty filters. Remove the obsolete combined organizer/event allowlist so both interfaces expose one coherent policy model.

**Blocked by:** None — can start immediately.

**Status:** ready-for-agent

- [ ] A new webhook configuration permits all organizers and events until a non-empty organizer or event filter is added.
- [ ] Organizer-level payloads enforce a configured organizer filter but ignore the event filter; event-level payloads enforce every non-empty applicable filter as an intersection.
- [ ] Missing organizer or event data fails the corresponding applicable non-empty filter, while empty filters leave their dimensions unrestricted.
- [ ] Filter matching is exact and case-sensitive; empty or whitespace-padded configured values are rejected without imposing Pretix's current slug regex or length limits.
- [ ] Simple CLI mode accepts repeatable organizer and event flags and semicolon-separated environment values, and no longer accepts the combined allowlist flag or environment variable.
- [ ] Public HTTP tests preserve authentication-first processing and the existing `400`, `401`, `404`, `204`, and `500` response contract while covering the independent filter matrix.

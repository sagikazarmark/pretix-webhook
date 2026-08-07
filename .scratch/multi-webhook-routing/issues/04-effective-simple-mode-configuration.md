# 04 — Effective simple-mode configuration

**What to build:** Preserve the quick single-webhook deployment while separating raw CLI and environment inputs from one fully validated effective endpoint, so defaults and source provenance are resolved before the server binds.

**Blocked by:** 01 — Shared exact webhook path grammar; 02 — Independent organizer and event filters.

**Status:** ready-for-agent

- [ ] With no configuration-file flag, the CLI produces one effective endpoint at `/webhook` unless an explicit absolute path is supplied.
- [ ] Bind configuration remains process-global and available from its existing flag and environment sources.
- [ ] Repeatable flags and semicolon-separated environment values produce the same independent organizer, event, and credential policies.
- [ ] Empty organizer and event lists produce an unrestricted policy, and omitted credentials produce an unauthenticated endpoint.
- [ ] Invalid paths, filters, and credentials are rejected at the public configuration-loading boundary before any listener bind is attempted.
- [ ] Direct parser tests and process-boundary tests distinguish explicitly supplied values from defaults without depending on private parser state.

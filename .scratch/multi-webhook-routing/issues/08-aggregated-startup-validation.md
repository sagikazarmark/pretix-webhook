# 08 — Aggregated startup validation

**What to build:** Give operators one actionable startup report containing every independently detectable semantic configuration error, so a malformed multi-webhook deployment can be repaired in one pass before the process touches the listener.

**Blocked by:** 06 — Unauthenticated TOML multi mode; 07 — Per-route credential references.

**Status:** ready-for-agent

- [ ] An empty webhook list is rejected rather than starting a server with no webhook routes.
- [ ] Duplicate resolved routes, organizer slugs, event slugs, and credential environment-variable names are reported as configuration errors with route indexes or resolved paths where available.
- [ ] Empty and whitespace-padded filter values and every invalid prefix or relative-path form are included in semantic validation.
- [ ] All independent semantic errors are accumulated into one deterministic report after successful TOML deserialization; syntax and type errors may fail immediately.
- [ ] Every simple/multi source conflict, including conflicting environment inputs, is rejected rather than merged or treated as a default.
- [ ] A subprocess test proves configuration validation is reported before an otherwise inevitable listener-bind failure.
- [ ] Aggregated diagnostics identify actionable fields and routes without disclosing filter values, credential variable names except where needed for credential resolution errors, usernames, or secrets.

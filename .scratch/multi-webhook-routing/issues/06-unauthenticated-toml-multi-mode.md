# 06 — Unauthenticated TOML multi mode

**What to build:** Let operators explicitly select a strict TOML configuration that starts multiple independently filtered, unauthenticated webhook routes beneath one effective global prefix, without automatic discovery or accidental mixing with simple endpoint settings.

**Blocked by:** 03 — Heterogeneous multi-webhook router; 04 — Effective simple-mode configuration.

**Status:** ready-for-agent

- [ ] Multi mode is selected only by an explicit flag-only configuration-file path; no environment variable or automatic file discovery can activate it.
- [ ] A TOML document accepts an optional global prefix and one or more webhook entries with relative path and optional organizer and event filter arrays.
- [ ] Unknown root and webhook-entry fields and malformed TOML types are rejected with diagnostics that identify the TOML source.
- [ ] Effective prefix precedence is CLI override, environment override, TOML value, then `/webhook`, with `/` supported as the root prefix.
- [ ] Simple endpoint inputs are rejected in multi mode, multi-prefix inputs are rejected in simple mode, and bind configuration remains available in both modes.
- [ ] A valid file with multiple or multi-segment routes produces independently filtered public HTTP endpoints, while omitted or empty credential references leave a route unauthenticated.
- [ ] The complete supported effective configuration is loaded and validated before listener binding, with no runtime reload, includes, profiles, or inheritance.

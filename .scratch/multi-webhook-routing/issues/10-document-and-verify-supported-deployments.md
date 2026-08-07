# 10 — Document and verify supported deployments

**What to build:** Give library users and operators accurate examples for single and multi-webhook deployments, and verify that every supported observability combination and crate boundary still builds and behaves as documented.

**Blocked by:** 02 — Independent organizer and event filters; 03 — Heterogeneous multi-webhook router; 04 — Effective simple-mode configuration; 05 — Route-aware built-in handlers; 06 — Unauthenticated TOML multi mode; 07 — Per-route credential references; 08 — Aggregated startup validation; 09 — Route-specific startup diagnostics.

**Status:** ready-for-agent

- [ ] Public documentation demonstrates the multi-webhook library builder, independent filters, per-route handlers and credentials, and composition of the completed Axum router.
- [ ] CLI documentation covers simple flags and environment variables, explicit TOML mode, prefix precedence, credential environment references, validation behavior, and security warnings.
- [ ] Documentation no longer advertises the combined organizer/event allowlist and does not place literal secrets in reusable TOML examples.
- [ ] Receiver tests pass with default features, log only, tracing only, and all features, while the receiver remains usable without a Tokio runtime dependency.
- [ ] CLI tests pass for default log, no-observability, tracing-only, and all-feature builds with the established tracing precedence.
- [ ] Workspace formatting, linting, tests, dependency-policy checks, and documentation checks pass for the completed feature.

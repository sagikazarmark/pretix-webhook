# 09 — Route-specific startup diagnostics

**What to build:** Make the effective server shape and each route's security posture visible at startup, and attribute handled events to their resolved route, without exposing policy details or credentials.

**Blocked by:** 05 — Route-aware built-in handlers; 08 — Aggregated startup validation.

**Status:** ready-for-agent

- [ ] Startup output reports the bind address and total route count, followed by one informational record for every resolved route.
- [ ] Every unrestricted route and every unauthenticated route emits its own startup warning in both simple and multi modes.
- [ ] Event and handler-failure records emitted by CLI-selected built-in handlers include the resolved route path.
- [ ] Startup and event output omit organizer and event filter values, credential environment-variable names, usernames, passwords, and raw credentials.
- [ ] Process-boundary tests cover successful startup diagnostics and warnings without relying on a permanent listening socket.
- [ ] Log, tracing, and noop builds all provide appropriate process-visible startup information while preserving compile-time handler selection.

# 07 — Per-route credential references

**What to build:** Let each TOML webhook route resolve its own optional HTTP Basic credential list from named environment variables, supporting rotation while ensuring configuration mistakes fail closed and diagnostics never disclose secret values.

**Blocked by:** 06 — Unauthenticated TOML multi mode.

**Status:** ready-for-agent

- [ ] Each credential reference resolves exactly one environment variable containing one `USERNAME:PASSWORD` value, splitting at the first colon so passwords may contain additional colons.
- [ ] Multiple references on one route accept any configured credential for zero-downtime rotation, and credentials accepted by one route are not accepted by another.
- [ ] Omitted or empty reference lists leave authentication disabled for that route without affecting other routes.
- [ ] Missing variables and empty or malformed values fail startup and identify the affected route and variable name.
- [ ] Configuration errors, debug output, and process diagnostics never print environment values, usernames, passwords, or raw credentials.
- [ ] Authentication remains the first request-pipeline check and failed authentication returns `401` with the existing Basic challenge.

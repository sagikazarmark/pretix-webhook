# 01 — Shared exact webhook path grammar

**What to build:** Give library callers and the CLI one shared, deterministic way to validate absolute webhook paths, global prefixes, and relative registration paths, and to resolve a prefix plus relative path without ambiguous URL forms. Existing single-webhook router helpers must remain available.

**Blocked by:** None — can start immediately.

**Status:** ready-for-agent

- [ ] Absolute webhook paths accept `/` and static URL-unreserved ASCII segments, while rejecting relative paths, trailing slashes, empty segments, `.` or `..`, dynamic parameters, queries, fragments, percent encoding, whitespace, and all other characters.
- [ ] Global prefixes follow the same absolute grammar, including support for `/`, and relative registration paths require one or more valid segments with no leading or trailing slash.
- [ ] Joining a valid prefix and relative path produces exactly one canonical absolute path, including when the prefix is `/`.
- [ ] Invalid inputs return actionable validation errors rather than reaching Axum route construction or panicking.
- [ ] Existing single-webhook router helpers continue to expose exact static routes through their public API.

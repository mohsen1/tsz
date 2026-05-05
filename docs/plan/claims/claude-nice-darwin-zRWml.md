# checker: JSDoc @import aliases accept any whitespace around `as`

- **Date**: 2026-05-05
- **Branch**: `claude/nice-darwin-zRWml`
- **PR**: TBD
- **Status**: claim
- **Workstream**: conformance / JSDoc parsing

## Intent

Issue #3178: `parse_jsdoc_import_tag` splits named JSDoc `@import` aliases
with the literal string `" as "`, so any non-space whitespace (e.g. tab)
between the imported name and the local alias loses the alias and
produces a spurious TS2304. Replace the literal split with a token-aware
helper that recognizes the `as` keyword surrounded by any JS whitespace,
matching tsc behavior. Apply the same fix to the namespace `* as <name>`
form so the rule generalizes structurally.

## Files Touched

- `crates/tsz-checker/src/jsdoc/parsing.rs`

## Verification

- `cargo nextest run -p tsz-checker --lib`
- New unit tests in `parsing.rs` covering tab/multi-space variants of
  `@import { Foo as Local }` and `@import * as NS`.

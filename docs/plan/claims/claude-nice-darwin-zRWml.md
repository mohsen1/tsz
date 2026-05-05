# checker: JSDoc @import aliases accept any whitespace around `as`

- **Date**: 2026-05-05
- **Branch**: `claude/nice-darwin-zRWml`
- **PR**: TBD
- **Status**: ready
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

- `cargo test -p tsz-checker --lib jsdoc::parsing::tests` — 9 new tests pass.
- `cargo test -p tsz-checker --lib` — only two pre-existing failures
  (`ts2300_tests::duplicate_identifier_with_default_lib_symbol_reports_lib_locations`,
  `ts2353_tests::recursive_array_union_excess_property_uses_outer_alias_display`)
  reproduce on the unmodified branch and are unrelated.
- New unit tests in `parsing.rs` covering tab and mixed-whitespace
  variants of `@import { Foo as Local }` and `@import * as NS`, plus
  guards against false matches in identifiers that contain the
  substring `as`.

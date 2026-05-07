# config: emit TS5025 spelling suggestions for typoed compiler options

- **Date**: 2026-05-07
- **Branch**: `claude/nice-darwin-uWWZZ`
- **PR**: TBD
- **Status**: claim
- **Workstream**: config diagnostics parity

## Intent

`unknown_compiler_option_suggestion` only handles a hardcoded `disableSolution*`
pair, so common typos like `stric`, `noEmti`, and `moduleResoluton` are
reported as bare `TS5023` instead of `TS5025` with a `Did you mean ...`
suggestion (issue #3831). Generalize the suggestion path to consult the
canonical compiler-option list via the existing
`tsz_parser::parser::spelling::get_spelling_suggestion` helper, which mirrors
TypeScript's `getSpellingSuggestion` algorithm.

## Files Touched

- `crates/tsz-core/src/config/mod.rs` (~80 LOC: spelling fallback + canonical
  option list + tests)

## Verification

- `cargo nextest run -p tsz-core --lib config::`
- targeted reruns of existing TS5023/TS5025 unit tests

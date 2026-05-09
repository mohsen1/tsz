# config: emit TS5025 spelling suggestions for typoed compiler options

- **Date**: 2026-05-07
- **Branch**: `claude/nice-darwin-uWWZZ`
- **PR**: #4373
- **Status**: ready
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

- `cargo test -p tsz-core --lib config::` — 148 passed
- `cargo test -p tsz-core --lib` — 3118 passed
- New tests: `test_typo_suggestions_emit_ts5025_for_close_compiler_option_names`,
  `test_unrelated_unknown_compiler_option_still_falls_back_to_ts5023`

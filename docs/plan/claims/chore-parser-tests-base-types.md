# chore(parser/tests): unit tests for parser/base.rs base types

- **Date**: 2026-04-26
- **Branch**: `chore/parser-tests-base-types`
- **PR**: #1320
- **Status**: ready
- **Workstream**: 8.x (DRY/test-coverage backfill)

## Intent

`crates/tsz-parser/src/parser/base.rs` defines the foundational `TextRange`,
`NodeIndex`, and `NodeList` types that the entire thin pipeline depends on,
but it currently has no unit tests. The only existing coverage is two
`NodeIndex::is_some/is_none` assertions inside `tests/tests.rs`.

This PR adds a pure-additive test file that locks the public API surface:
- `NodeIndex::NONE` sentinel value, `is_none`/`is_some`, `into_option`
- `NodeList::new`, `with_capacity`, `push`, `len`, `is_empty`, `Default`
- `TextRange::new`, default field values, serde round-trip

Pure additive — no behavior changes.

## Files Touched

- `crates/tsz-parser/tests/base_tests.rs` (new file, ~150 LOC)
- `crates/tsz-parser/src/parser/mod.rs` (one `#[cfg(test)] #[path]` line)
- `docs/plan/claims/chore-parser-tests-base-types.md` (this file)

## Verification

- `cargo nextest run -p tsz-parser --lib` (638 tests pass; 31 new in `base_tests`)
- `cargo clippy -p tsz-parser --lib --tests --all-features -- -D warnings` (clean)
- `cargo fmt -p tsz-parser --check` (clean)

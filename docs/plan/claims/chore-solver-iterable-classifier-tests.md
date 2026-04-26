# chore(solver/tests): unit tests for type_queries::iterable classifier helpers

- **Date**: 2026-04-26
- **Branch**: `chore/solver-iterable-classifier-tests`
- **PR**: #1309
- **Status**: ready
- **Workstream**: 8 (test-fixture coverage; lock untested public solver helpers)

## Intent

`crates/tsz-solver/src/type_queries/iterable.rs` (187 LOC) currently has zero
unit tests and exposes three publicly used classifier helpers
(`classify_full_iterable_type`, `classify_async_iterable_type`,
`classify_for_of_element_type`) consumed from `tsz-checker`'s
`iterable_checker.rs` (10+ call sites) and from the solver's own iterable
boundary helper. Each function is a `TypeData` -> kind mapper with a wide
match arm, so silent drift (a new `TypeData` variant slipping into the
wrong arm) would only show up downstream as flaky `for-of` / spread / async
iterable diagnostics.

This PR adds 30+ focused unit tests pinning each enum-variant mapping for
all three classifiers, plus the explicit `NotIterable` /
`NotAsyncIterable` / `Other` fallback rows. Pure additive — no production
code changes.

## Files Touched

- `crates/tsz-solver/src/type_queries/iterable.rs` — append the standard
  `#[cfg(test)] #[path = "../../tests/iterable_classifier_tests.rs"] mod tests;`
  block.
- `crates/tsz-solver/tests/iterable_classifier_tests.rs` — new test file.

## Verification

- `cargo nextest run -p tsz-solver --lib` — 5387 tests pass, including 45
  new `type_queries::iterable::tests::*` cases. No regressions.
- `cargo clippy -p tsz-solver --tests -- -D warnings` — clean.
- `cargo fmt -p tsz-solver --check` — clean.

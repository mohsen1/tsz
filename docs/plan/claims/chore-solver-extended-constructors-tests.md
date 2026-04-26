# chore(solver/tests): unit tests for type_queries::extended_constructors classifier helpers

- **Date**: 2026-04-26
- **Branch**: `chore/solver-extended-constructors-tests`
- **PR**: #1327
- **Status**: ready
- **Workstream**: 8 (test-fixture coverage; lock untested public solver helpers)

## Intent

`crates/tsz-solver/src/type_queries/extended_constructors.rs` (366 LOC)
currently has zero unit tests despite exposing seven publicly used classifier
helpers consumed by the checker for constructor / class / abstract-class /
instance-type / interface merging:

- `classify_for_abstract_check`
- `classify_for_class_decl`
- `classify_for_constructor_check`
- `classify_for_instance_type`
- `classify_for_constructor_return_merge`
- `classify_for_base_instance_merge`
- `resolve_abstract_constructor_anchor` (public, peels Application bases)

Each function is a `TypeData` → kind mapper with a wide match arm. A new
`TypeData` variant silently slipping into the wrong arm would only surface
downstream as flaky `new` / abstract-class / interface-merge diagnostics.

This PR adds focused unit tests pinning each enum-variant mapping for all
seven helpers, plus the explicit `NotAbstract` / `NotObject` / `Other`
fallback rows. Pure additive — no production code changes.

## Files Touched

- `crates/tsz-solver/src/type_queries/extended_constructors.rs` — append the
  standard `#[cfg(test)] #[path = "../../tests/extended_constructors_tests.rs"] mod tests;`
  block.
- `crates/tsz-solver/tests/extended_constructors_tests.rs` — new test file.

## Verification

- `cargo nextest run -p tsz-solver --lib` — 5449 tests pass, including 48
  new `type_queries::extended_constructors::tests::*` cases. No regressions.
- `cargo clippy -p tsz-solver --tests -- -D warnings` — clean.
- `cargo fmt -p tsz-solver --check` — clean.

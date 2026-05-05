# fix(checker): drop printer-output Factory&lt; heuristic in JSX LMA

- **Date**: 2026-05-05
- **Branch**: `claude/nice-darwin-AtYlr`
- **PR**: TBD
- **Status**: claim
- **Workstream**: 1 (Conformance / checker)

## Intent

Issue #3227. `apply_jsx_library_managed_attributes` in
`crates/tsz-checker/src/checkers/jsx/extraction.rs:226` discards the
`JSX.LibraryManagedAttributes` evaluation whenever the formatted display of
the evaluated type contains the substring `Factory<`. That violates the
anti-hardcoding directive (§25): the decision is driven by the printer's
output for a user-chosen identifier, so any user type spelled `Factory<…>`
silently breaks LMA and produces a false `TS2741` for the optional prop.
The check is also a duplicate of the structural
`should_preserve_contextual_application_shape` guard immediately below it.

The fix removes the printer-output check, adds a regression unit test that
declares a user `Factory<T>` interface, and verifies that no JSX checker /
solver tests regress.

## Files Touched

- `crates/tsz-checker/src/checkers/jsx/extraction.rs` (-3 LOC)
- `crates/tsz-checker/tests/conformance_issues/features/namespace_construct_signature.rs` (regression test)

## Verification

- `cargo nextest run -p tsz-checker` (clean)
- `cargo nextest run -p tsz-solver` (clean)

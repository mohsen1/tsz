# fix(checker): preserve type alias spelling in TS2322 mismatch messages

- **Date**: 2026-05-09
- **Branch**: `claude/nice-darwin-NlwJr`
- **PR**: TBD
- **Status**: claim
- **Workstream**: conformance fingerprint parity

## Intent

`render_type_mismatch` was leaking the TS2739/TS2741 alias-unfold rewrite
into the generic TS2322 ("Type 'X' is not assignable to type 'Y'") path,
so a non-generic alias whose body resolves to a generic Application
displayed as the unfolded application form. tsc preserves the alias name
in TS2322; the unfold is scoped to the missing-properties messages.

This PR drops the unfold call from the TypeMismatch render path, restoring
parity with tsc on `compiler/typeVariableConstraintedToAliasNotAssignableToUnion.ts`.

## Files Touched

- `crates/tsz-checker/src/error_reporter/render_failure/type_mismatch.rs`
  (remove the misplaced `ts2739_alias_of_application_source_display` /
  `ts2739_alias_target_display` calls in the TS2322 path)
- `crates/tsz-checker/src/tests/ts2739_alias_unfold_display_tests.rs`
  (add two TS2322-side regression tests that lock the alias-preservation
  contract; one renamed-identifier cover for anti-hardcoding)

## Verification

- `cargo test -p tsz-checker --lib` — 3785 passed
- `cargo test -p tsz-solver --lib` — 5713 passed
- `scripts/conformance/conformance.sh run --filter "typeVariableConstraintedToAliasNotAssignableToUnion"` — 1/1 passed
- `scripts/conformance/conformance.sh run` — pending full-suite re-run

# fix(checker): align tupleTypes assignment fingerprints

- **Date**: 2026-05-06
- **Branch**: `fix/checker-tupletypes-fingerprint`
- **PR**: #3535
- **Status**: ready
- **Workstream**: 1 (Diagnostic conformance)
- **Claimed**: 2026-05-06

## Intent

Fix the current conformance failure
`TypeScript/tests/cases/compiler/tupleTypes.ts`. The diagnostic codes already
match TypeScript (`TS2322`, `TS2403`, `TS2454`, `TS2493`, `TS2540`), but the
fingerprints drift for tuple assignment display: `tsz` reports `Type 'B' is
not assignable to type '[number, string]'` at line 15 instead of TypeScript's
`Type '[number]' is not assignable to type '[number, string]'`, and emits an
extra optional tuple length assignment fingerprint at line 65.

## Files Touched

- `docs/plan/claims/fix-checker-tupletypes-fingerprint.md`
- `crates/tsz-checker/src/error_reporter/assignability.rs`
- `crates/tsz-checker/src/error_reporter/core/diagnostic_source.rs`
- `crates/tsz-checker/src/error_reporter/core/diagnostic_source/assignment_formatting.rs`
- `crates/tsz-checker/src/assignability/assignment_checker_tests.rs`
- `crates/tsz-solver/src/operations/property_helpers.rs`
- `crates/tsz-solver/src/evaluation/evaluate_rules/index_access.rs`
- `crates/tsz-solver/tests/evaluate_tests.rs`

## Verification

- `CARGO_BUILD_JOBS=1 cargo nextest run --target-dir /var/tmp/tsz-check-3535 -p tsz-checker array_literal_tuple_assignment_ignores_later_tuple_length_alias_display`
- `CARGO_BUILD_JOBS=1 cargo nextest run --target-dir /var/tmp/tsz-check-3535 -p tsz-checker optional_tuple_length_assignment_accepts_minimum_length_literal`
- `CARGO_BUILD_JOBS=1 cargo nextest run --target-dir /var/tmp/tsz-check-3535 -p tsz-solver test_index_access_optional_tuple_string_literal_length`
- `CARGO_BUILD_JOBS=1 cargo build --target-dir /var/tmp/tsz-check-3535 -p tsz-cli -p tsz-conformance`
- `/var/tmp/tsz-check-3535/debug/tsz-conformance --test-dir /Users/mohsen/code/tsz-main-worktree/TypeScript/tests/cases --cache-file scripts/conformance/tsc-cache-full.json --tsz-binary /var/tmp/tsz-check-3535/debug/tsz --server-binary /var/tmp/tsz-check-3535/debug/tsz-server --workers 1 --filter tupleTypes --print-test --verbose --print-fingerprints --print-test-files` (`FINAL RESULTS: 1/1 passed (100.0%)`)

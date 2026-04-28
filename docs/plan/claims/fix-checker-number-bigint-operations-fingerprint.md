# fix(checker): align number and bigint operation diagnostics

- **Date**: 2026-04-28
- **Branch**: `fix/checker-number-bigint-operations-fingerprint`
- **PR**: TBD
- **Status**: ready
- **Workstream**: 1 (Diagnostic conformance fingerprints)

## Intent

Fix the fingerprint-only conformance mismatch in `numberVsBigIntOperations.ts`.
The target already emits the expected diagnostic codes, so this slice focuses on
the root cause for message, count, or anchor drift in number/bigint operation
diagnostics rather than adding a narrow suppression.

## Files Touched

- `crates/tsz-solver/src/operations/binary_ops.rs`
- `crates/tsz-checker/src/error_reporter/operator_errors.rs`
- `crates/tsz-checker/src/types/computation/binary.rs`
- `crates/tsz-checker/src/types/computation/helpers.rs`
- `crates/tsz-checker/src/assignability/compound_assignment.rs`
- `crates/tsz-checker/src/assignability/assignment_checker/arithmetic_ops.rs`
- `crates/tsz-checker/tests/value_usage_tests.rs`

## Verification

- `cargo fmt --check --package tsz-checker --package tsz-solver`
- `CARGO_TARGET_DIR=.target cargo check --package tsz-checker --package tsz-solver`
- `CARGO_TARGET_DIR=.target cargo nextest run --package tsz-checker test_bigint_unsigned_shift_reports_pair_error test_invalid_non_bigint_bitwise_result_stays_number test_number_bigint_union_operator_display_expands_alias_but_preserves_type_parameter`
- `CARGO_TARGET_DIR=.target CARGO_INCREMENTAL=0 cargo build --target-dir .target --profile dist-fast -p tsz-cli -p tsz-conformance`
- `.target/dist-fast/tsz-conformance --test-dir /Users/mohsen/code/tsz/TypeScript/tests/cases --cache-file scripts/conformance/tsc-cache-full.json --tsz-binary .target/dist-fast/tsz --filter numberVsBigIntOperations --print-fingerprints --verbose --workers 1 --timeout 60`
- `.target/dist-fast/tsz-conformance --test-dir /Users/mohsen/code/tsz/TypeScript/tests/cases --cache-file scripts/conformance/tsc-cache-full.json --tsz-binary .target/dist-fast/tsz --max 200 --workers 4 --timeout 60`

Notes:

- The direct focused conformance run passed `numberVsBigIntOperations.ts`.
- The 200-test conformance smoke passed 199/200; the only mismatch was existing
  `aliasOnMergedModuleInterface.ts` TS2708 missing.
- `./scripts/conformance/conformance.sh ...` could not complete in this
  sandbox because npm dependency installation failed with DNS `ENOTFOUND`.

# fix(checker): align excessively large tuple spread diagnostics

- **Date**: 2026-05-05
- **Branch**: `fix/checker-excessively-large-tuple-spread-diagnostics`
- **PR**: https://github.com/mohsen1/tsz/pull/3065
- **Status**: ready
- **Workstream**: 1 (Diagnostic Conformance And Fingerprints)

## Intent

Fix the current wrong-code divergence in
`TypeScript/tests/cases/compiler/excessivelyLargeTupleSpread.ts`.
The picker reports expected diagnostics `TS2799` and `TS2800`, while tsz
currently emits `TS2589`, so this PR will root-cause tuple spread size handling
and align the emitted diagnostics.

## Files Touched

- `crates/tsz-checker/src/state/type_resolution/core.rs`
- `crates/tsz-checker/src/types/computation/array_literal.rs`
- `crates/tsz-checker/src/types/computation/large_tuple.rs`
- `crates/tsz-checker/src/types/computation/mod.rs`
- `crates/tsz-checker/src/types/type_checking/type_alias_checking.rs`
- `crates/tsz-checker/tests/ts2589_tests.rs`
- `docs/plan/claims/fix-checker-excessively-large-tuple-spread-diagnostics.md`

## Verification

- PASS `cargo test -p tsz-checker excessively_large_tuple_spreads_report_tuple_size_diagnostics -- --nocapture`
- PASS `./scripts/conformance/conformance.sh run --filter "excessivelyLargeTupleSpread" --verbose`
- PASS `./scripts/conformance/conformance.sh run --max 200`
- PASS `scripts/githooks/pre-commit`

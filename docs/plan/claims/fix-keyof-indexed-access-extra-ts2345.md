# [WIP] fix(checker): suppress extra TS2345 in keyof indexed access

- **Date**: 2026-05-05
- **Branch**: `fix/keyof-indexed-access-extra-ts2345`
- **PR**: https://github.com/mohsen1/tsz/pull/3238
- **Status**: ready
- **Workstream**: 1 (Diagnostic Conformance And Fingerprints)

## Intent

Fix the random conformance pick
`TypeScript/tests/cases/conformance/types/keyof/keyofAndIndexedAccess.ts`,
where `tsc` expects only `TS2322` but `tsz` currently emits an extra `TS2345`.

## Files Touched

- `docs/plan/claims/fix-keyof-indexed-access-extra-ts2345.md`
- `crates/tsz-checker/src/state/type_environment/lazy.rs`
- `crates/tsz-checker/src/assignability/assignability_checker.rs`
- `crates/tsz-checker/tests/this_type_tests.rs`

## Verification Plan

- `./scripts/conformance/conformance.sh run --filter "keyofAndIndexedAccess" --verbose`
- Focused Rust regression test in the owning crate
- `./scripts/conformance/conformance.sh run --max 200`
- `scripts/githooks/pre-commit`

## Result

Suppresses the extra `TS2345` diagnostics by keeping `ThisType`-dependent
environment evaluation results out of the shared evaluation cache, and by
resolving direct `this["property"]` indexed-access targets to the current
class property type for call-argument assignability. Wrapped polymorphic-this
targets such as `Unwrap<this["prop"]>` remain deferred and diagnostic-producing.

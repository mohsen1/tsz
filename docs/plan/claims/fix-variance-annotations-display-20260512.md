# fix(checker): align varianceAnnotations display fingerprints

- **Date**: 2026-05-12
- **Branch**: `fix/variance-annotations-display-20260512`
- **PR**: https://github.com/mohsen1/tsz/pull/5699
- **Status**: ready
- **Workstream**: conformance

## Intent

Reduce the remaining `varianceAnnotations.ts` conformance drift after prior work made the diagnostic code set match. This slice will focus on the smallest display or position mismatch that can be fixed without broadening variance semantics or touching unrelated conformance failures.

## Files Touched

- `docs/plan/claims/fix-variance-annotations-display-20260512.md`
- `crates/tsz-checker/src/error_reporter/call_errors/error_emission.rs`
- `crates/tsz-checker/src/error_reporter/call_errors_tests.rs`

## Verification

- Baseline: `.target/dist-fast/tsz-conformance --test-dir /Users/mohsen/code/tsz/TypeScript/tests/cases --cache-file scripts/conformance/tsc-cache-full.json --tsz-binary .target/dist-fast/tsz --filter varianceAnnotations --print-fingerprints --verbose` (1/2 passed; fingerprint-only drift included TS2345 `ActionObject<...>>` extra close, missing matching single-close TS2345)
- `cargo test -p tsz-checker --lib generic_call_parameter_display_trims_unmatched_trailing_type_arg_close -- --nocapture` (passed)
- `cargo build --profile dist-fast -p tsz-cli -p tsz-conformance` (passed)
- `.target/dist-fast/tsz-conformance --test-dir /Users/mohsen/code/tsz/TypeScript/tests/cases --cache-file scripts/conformance/tsc-cache-full.json --tsz-binary .target/dist-fast/tsz --filter varianceAnnotations --print-fingerprints --verbose` (1/2 passed; TS2345 `ActionObject` mismatch removed; remaining fingerprint-only drift is missing `Baz<string>` TS2322 and two extra anonymous-class TS2322 diagnostics)

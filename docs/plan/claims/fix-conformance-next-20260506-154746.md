# fix(checker): preserve merge receiver display prefix

- **Date**: 2026-05-06
- **Branch**: `fix/conformance-next-20260506-154746`
- **PR**: #4112
- **Status**: ready
- **Workstream**: 1 (Diagnostic Conformance)

## Intent

Target the quick-pick conformance failure:
`TypeScript/tests/cases/compiler/longObjectInstantiationChain2.ts`.

`tsc` and tsz already agree on diagnostic code `TS2339`, but the
fingerprints differ. The drift was in long property receiver display for
nested `merge<...>` applications: tsz pre-elided the innermost object
arguments before final truncation, while tsc preserves the initial concrete
object arguments and elides later ones. This slice aligns that display path
without changing the diagnostic set.

## Files Touched

- `crates/tsz-checker/src/error_reporter/core_formatting.rs`
- `crates/tsz-checker/src/error_reporter/core/diagnostic_source.rs`
- `crates/tsz-checker/src/error_reporter/property_receiver_formatting.rs`
- `crates/tsz-checker/src/tests/property_alias_display_tests.rs`

## Verification

- `cargo fmt --check` passed.
- `CARGO_BUILD_JOBS=2 cargo check -p tsz-checker --lib` passed.
- `CARGO_BUILD_JOBS=2 cargo check -p tsz-solver --lib` passed.
- `CARGO_BUILD_JOBS=2 cargo nextest run -p tsz-checker --lib -E 'test(ts2339_long_merge_receiver_keeps_initial_object_args_before_truncation)'` passed.
- `CARGO_BUILD_JOBS=2 cargo nextest run -p tsz-checker --lib` passed: 3662 passed, 10 skipped.
- `CARGO_BUILD_JOBS=2 ./scripts/conformance/conformance.sh run --filter "longObjectInstantiationChain2" --verbose --test-dir /Users/mohsen/code/tsz/.worktrees/fix-large-ts-repo-signature-param-reserve-20260506/TypeScript/tests/cases` passed: 1/1.
- `CARGO_BUILD_JOBS=2 ./scripts/conformance/conformance.sh run --max 200 --test-dir /Users/mohsen/code/tsz/.worktrees/fix-large-ts-repo-signature-param-reserve-20260506/TypeScript/tests/cases` completed with the existing fingerprint-only `TS2538` column drift in `anyIndexedAccessArrayNoException.ts`: 199/200 passed, 1 fingerprint-only.

# fix(checker): align union type reduction2 arity display

- **Date**: 2026-05-06
- **Branch**: `fix/checker-union-type-reduction2-callable-arity`
- **PR**: https://github.com/mohsen1/tsz/pull/3608
- **Status**: ready
- **Workstream**: 1 (Diagnostic conformance)

## Intent

This PR targets the quick-picked fingerprint-only conformance mismatch in
`TypeScript/tests/cases/conformance/types/union/unionTypeReduction2.ts`.
The diagnostic code set already matches TypeScript (`TS2554`), but the
fingerprint text or anchor differs.

## Context

Selected with `scripts/session/quick-pick.sh --seed 3519`. The normal
`--run` path could not initialize `TypeScript` because this checkout has
`.gitmodules` metadata but no tracked `TypeScript` gitlink, so the pick used
the existing `scripts/conformance/conformance-detail.json` after the failed
submodule attempt.

## Files Touched

- `crates/tsz-checker/src/types/computation/helpers.rs`
- `crates/tsz-checker/src/types/computation/binary.rs`
- `crates/tsz-checker/src/flow/control_flow/assignment.rs`
- `crates/tsz-checker/src/flow/control_flow/core.rs`
- `crates/tsz-checker/src/error_reporter/call_errors_tests.rs`
- `crates/tsz-solver/src/operations/core/call_resolution.rs`

## Verification

- `cargo fmt --check`
- `CARGO_TARGET_DIR=.target-pr3553 CARGO_INCREMENTAL=0 cargo build --target-dir .target-pr3553 --profile dist-fast -p tsz-cli --bin tsz -p tsz-conformance --bin tsz-conformance`
- `./.target-pr3553/dist-fast/tsz-conformance --test-dir TypeScript/tests/cases --cache-file scripts/conformance/tsc-cache-full.json --tsz-binary ./.target-pr3553/dist-fast/tsz --filter 'unionTypeReduction2' --verbose --print-fingerprints --write-diff-artifacts --diff-artifacts-dir artifacts/conformance/union-type-reduction2 --workers 2 --max-worker-rss-mb 1024 --max-compilations-per-worker 10`
  - Result: `FINAL RESULTS: 1/1 passed (100.0%)`
- `CARGO_TARGET_DIR=.target-pr3553 CARGO_INCREMENTAL=0 cargo nextest run --target-dir .target-pr3553 -p tsz-checker union_type_reduction2_preserves_tsc_callable_arity`
  - Result: 1 test passed

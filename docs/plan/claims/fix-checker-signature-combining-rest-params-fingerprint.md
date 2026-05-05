# [WIP] fix(checker): align signature-combining rest parameter fingerprint

- **Date**: 2026-05-05
- **Branch**: `fix/checker-signature-combining-rest-params-fingerprint`
- **PR**: #3372
- **Status**: ready
- **Workstream**: 1 (Conformance / diagnostic fingerprints)

## Intent

The canonical conformance picker selected
`TypeScript/tests/cases/compiler/signatureCombiningRestParameters4.ts`, a
fingerprint-only failure where tsz and tsc agree on diagnostic code `TS2345`
but disagree on diagnostic fingerprint details.

This PR will root-cause the rest-parameter signature-combining display or
anchor mismatch, add owning Rust regression coverage, and rerun the targeted
conformance test.

## Files Touched

- `crates/tsz-solver/src/intern/core/constructors.rs`
- `crates/tsz-checker/tests/union_index_access_function_application_param_tests.rs`

## Verification

- `CARGO_BUILD_JOBS=1 cargo nextest run -p tsz-checker --test union_index_access_function_application_param_tests signature_combining_rest_parameters_4_preserves_intersection_display_order`
- `CARGO_BUILD_JOBS=1 cargo build -p tsz-cli --bin tsz`
- `/Users/mohsen/code/tsz-worktrees/conformance-quick-pick-20260505-next25/.target-run/debug/tsz-conformance --filter signatureCombiningRestParameters4 --workers 1 --verbose --print-fingerprints --test-dir /Users/mohsen/code/tsz-worktrees/conformance-quick-pick-20260505-next20/TypeScript/tests/cases --cache-file scripts/conformance/tsc-cache-full.json --tsz-binary .target/debug/tsz`
- Adjacent guard:
  `/Users/mohsen/code/tsz-worktrees/conformance-quick-pick-20260505-next25/.target-run/debug/tsz-conformance --filter signatureCombiningRestParameters3 --workers 1 --verbose --print-fingerprints --test-dir /Users/mohsen/code/tsz-worktrees/conformance-quick-pick-20260505-next20/TypeScript/tests/cases --cache-file scripts/conformance/tsc-cache-full.json --tsz-binary .target/debug/tsz`

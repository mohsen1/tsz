# [WIP] fix(checker): align common type intersection fingerprints

- **Date**: 2026-05-06
- **Branch**: `fix/common-type-intersection-fingerprint`
- **PR**: #3817
- **Status**: ready
- **Workstream**: 1 (Diagnostic Conformance)
- **Claimed**: 2026-05-06 07:06:14 UTC

## Intent

Fix the picked fingerprint-only conformance mismatch in
`TypeScript/tests/cases/conformance/types/intersection/commonTypeIntersection.ts`.
The snapshot reports matching diagnostic code `TS2322`; this slice will
root-cause the remaining fingerprint divergence in message rendering, source
type display, or diagnostic anchoring and fix it in the owning checker/solver
layer.

## Files Touched

- `docs/plan/claims/fix-common-type-intersection-fingerprint.md`
- `crates/tsz-checker/src/error_reporter/core/mod.rs`
- `crates/tsz-checker/src/error_reporter/core/annotation_literal_display.rs`
- `crates/tsz-checker/src/error_reporter/core/declared_intersection_display.rs`
- `crates/tsz-checker/src/error_reporter/core/diagnostic_source.rs`
- `crates/tsz-checker/src/error_reporter/core/diagnostic_source/assignment_formatting.rs`
- `crates/tsz-checker/src/error_reporter/core/type_display.rs`
- `crates/tsz-checker/src/error_reporter/assignability.rs`
- `crates/tsz-checker/src/error_reporter/render_failure/type_mismatch.rs`
- `crates/tsz-checker/tests/intersection_primitive_member_assignability_tests.rs`

## Verification

- `cargo fmt --check`
- `git diff --check`
- `CARGO_INCREMENTAL=0 CARGO_BUILD_JOBS=1 cargo check -p tsz-checker --target-dir /var/tmp/tsz-target-common-intersection`
- `TSZ_TYPESCRIPT_LIB_DIR=/Users/mohsen/code/tsz-main-worktree/TypeScript/lib CARGO_INCREMENTAL=0 CARGO_BUILD_JOBS=1 cargo test --target-dir /var/tmp/tsz-target-common-intersection -p tsz-checker --test intersection_primitive_member_assignability_tests conformance_common_type_intersection_emits_two_ts2322 -- --nocapture`
- `CARGO_INCREMENTAL=0 CARGO_BUILD_JOBS=1 cargo test --target-dir /var/tmp/tsz-target-common-intersection -p tsz-checker --test ts2322_tests generic_object_assign_helper_keeps_outer_ts2322 -- --nocapture`
- `CARGO_INCREMENTAL=0 CARGO_BUILD_JOBS=1 cargo test --target-dir /var/tmp/tsz-target-common-intersection -p tsz-checker --test conformance_issues test_intersection_index_signature_diagnostics_preserve_declared_identifier_annotations -- --nocapture`
- `CARGO_INCREMENTAL=0 CARGO_BUILD_JOBS=1 cargo test --target-dir /var/tmp/tsz-target-common-intersection -p tsz-core test_any_in_intersection_types -- --nocapture`
- `CARGO_INCREMENTAL=0 CARGO_BUILD_JOBS=1 cargo build --profile dist-fast -p tsz-cli -p tsz-conformance --target-dir /var/tmp/tsz-target-common-intersection`
- `/var/tmp/tsz-target-common-intersection/dist-fast/tsz-conformance --test-dir /Users/mohsen/code/tsz-main-worktree/TypeScript/tests/cases --cache-file scripts/conformance/tsc-cache-full.json --tsz-binary /var/tmp/tsz-target-common-intersection/dist-fast/tsz --server-binary /var/tmp/tsz-target-common-intersection/dist-fast/tsz-server --workers 1 --filter commonTypeIntersection --print-test --verbose --print-fingerprints --print-test-files` (1/1 passed, known failures 0)

# fix(checker): suppress extra TS2304 for JSDoc satisfies tag typedef lookup

- **Date**: 2026-05-06
- **Branch**: `fix/jsdoc-satisfies-tag14-diagnostics`
- **PR**: #3629
- **Status**: ready
- **Workstream**: 1 (Diagnostic conformance)
- **Claimed**: 2026-05-06 05:01:00 UTC

## Intent

Fix the current conformance false positive in
`TypeScript/tests/cases/conformance/jsdoc/checkJsdocSatisfiesTag14.ts`.
TypeScript reports the expected parse diagnostic, while `tsz` also reports an
extra `TS2304` for the JSDoc `@satisfies T1` type references.

## Files Touched

- `docs/plan/claims/fix-jsdoc-satisfies-tag14-ts2304.md`
- `crates/tsz-checker/src/jsdoc/diagnostics.rs`
- `crates/tsz-checker/tests/conformance_issues/features/namespace_construct_signature.rs`

## Verification

- live conformance run saved at `/tmp/tsz-current-conformance-b7b6871374.txt`
  (`12455/12582 passed`, selected live failure `checkJsdocSatisfiesTag14.ts`)
- `CARGO_INCREMENTAL=0 CARGO_BUILD_JOBS=1 cargo nextest run --target-dir /var/tmp/tsz-jsdoc-satisfies14-check -p tsz-checker --test conformance_issues test_malformed_jsdoc_satisfies_does_not_emit_duplicate_tag_error`
  (`1 passed`)
- `CARGO_INCREMENTAL=0 CARGO_BUILD_JOBS=1 cargo nextest run --target-dir /var/tmp/tsz-jsdoc-satisfies14-check -p tsz-cli compile_jsdoc_satisfies_malformed_tag_does_not_apply_later_braced_type`
  (`1 passed`)
- `CARGO_INCREMENTAL=0 CARGO_BUILD_JOBS=1 cargo build --profile dist-fast -p tsz-cli -p tsz-conformance --target-dir /Users/mohsen/code/tsz-main-worktree/.target`
- `/Users/mohsen/code/tsz-main-worktree/.target/dist-fast/tsz-conformance --test-dir /Users/mohsen/code/tsz-main-worktree/TypeScript/tests/cases --cache-file scripts/conformance/tsc-cache-full.json --tsz-binary /Users/mohsen/code/tsz-main-worktree/.target/dist-fast/tsz --server-binary /Users/mohsen/code/tsz-main-worktree/.target/dist-fast/tsz-server --workers 1 --filter checkJsdocSatisfiesTag14 --print-test --verbose --print-fingerprints --print-test-files`
  (`1/1 passed`)
- `/Users/mohsen/code/tsz-main-worktree/.target/dist-fast/tsz-conformance --test-dir /Users/mohsen/code/tsz-main-worktree/TypeScript/tests/cases --cache-file scripts/conformance/tsc-cache-full.json --tsz-binary /Users/mohsen/code/tsz-main-worktree/.target/dist-fast/tsz --server-binary /Users/mohsen/code/tsz-main-worktree/.target/dist-fast/tsz-server --workers 8 --max 200 --print-test`
  (`200/200 passed`)

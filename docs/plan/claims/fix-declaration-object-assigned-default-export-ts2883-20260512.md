# fix(conformance): restore TS2883 object-assigned default export

- **Date**: 2026-05-12
- **Branch**: `fix/declaration-object-assigned-default-export-ts2883-20260512`
- **PR**: #5836
- **Status**: ready
- **Workstream**: 1 (Diagnostic conformance)

## Intent

Restore the missing TS2883 diagnostic in `TypeScript/tests/cases/compiler/declarationEmitObjectAssignedDefaultExport.ts`, which full conformance reports as a PASS -> FAIL regression on latest `main` at `e401ed706f`.

## Files Touched

- `crates/conformance/src/tsz_wrapper.rs`
- `crates/conformance/tests/tsz_wrapper.rs`

## Verification

- Full conformance on latest main/mixin branch reported `12580/12582`, with this test as the only real regression and `mixinAccessModifiers.ts` as the remaining known XFAIL.
- `.target/dist-fast/tsz-conformance --test-dir TypeScript/tests/cases --cache-file scripts/conformance/tsc-cache-full.json --tsz-binary /Users/mohsen/code/tsz/.target/dist-fast/tsz --workers 1 --print-test --print-test-files --print-fingerprints --verbose --no-batch --filter declarationEmitObjectAssignedDefaultExport` -> 1/1 passed.
- `./scripts/conformance/conformance.sh run --filter "declarationEmitObjectAssignedDefaultExport" --verbose` -> 1/1 passed.
- `cargo test -p tsz-conformance test_prepare_test_dir_implicit_include_matches_tsc_harness -- --nocapture` -> passed.
- `cargo test -p tsz-conformance test_prepare_test_dir_ts2883_keeps_node_modules_declarations_resolution_only -- --nocapture` -> passed.
- `.target/dist-fast/tsz-conformance ... --no-batch --filter augmentExportEquals7` -> 1/1 passed, guarding node_modules declaration root-file behavior outside TS2883 portability cases.

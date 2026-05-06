# fix(checker): suppress TS2694 for node import attributes type mode

- **Date**: 2026-05-06
- **Branch**: `fix/node-import-attributes-type-mode-ts2694`
- **PR**: #3598
- **Status**: ready
- **Workstream**: 1 (Diagnostic conformance)
- **Claimed**: 2026-05-06 02:17:15 UTC

## Intent

Fix the current conformance false positive in
`TypeScript/tests/cases/conformance/node/nodeModulesImportAttributesTypeModeDeclarationEmit.ts`.
The live conformance run reports that TypeScript emits no diagnostics, while
`tsz` emits an extra `TS2694`.

## Files Touched

- `docs/plan/claims/fix-node-import-attributes-type-mode-ts2694.md`
- `crates/tsz-checker/src/context/resolver.rs`
- `crates/tsz-cli/tests/driver_tests.rs`

## Verification

- live conformance run saved at `/tmp/tsz-current-conformance-claim2.txt`
  (`12454/12582 passed`, selected live failure
  `nodeModulesImportAttributesTypeModeDeclarationEmit.ts`)
- `CARGO_BUILD_JOBS=1 cargo nextest run --target-dir /var/tmp/tsz-claim2-check -p tsz-cli import_type_resolution_mode_declaration_emit_uses_exact_package_condition`
  (`1 passed`)
- `CARGO_BUILD_JOBS=1 cargo check --target-dir /var/tmp/tsz-claim2-check -p tsz-checker`
- `CARGO_BUILD_JOBS=1 cargo check --target-dir /var/tmp/tsz-claim2-check -p tsz-cli`
- `CARGO_BUILD_JOBS=1 cargo build --profile dist-fast -p tsz-cli -p tsz-conformance`
- `./.target/dist-fast/tsz-conformance --test-dir /Users/mohsen/code/tsz-main-worktree/TypeScript/tests/cases --cache-file scripts/conformance/tsc-cache-full.json --tsz-binary ./.target/dist-fast/tsz --server-binary ./.target/dist-fast/tsz-server --workers 1 --filter nodeModulesImportAttributesTypeModeDeclarationEmit --print-test --verbose --print-fingerprints --print-test-files`
  (`2/2 passed`)
- `./.target/dist-fast/tsz-conformance --test-dir /Users/mohsen/code/tsz-main-worktree/TypeScript/tests/cases --cache-file scripts/conformance/tsc-cache-full.json --tsz-binary ./.target/dist-fast/tsz --server-binary ./.target/dist-fast/tsz-server --workers 8 --max 200 --print-test`
  (`200/200 passed`)

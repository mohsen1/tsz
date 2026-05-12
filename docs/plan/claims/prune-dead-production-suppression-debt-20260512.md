# chore(conformance): prune dead production suppression debt

- **Date**: 2026-05-12
- **Branch**: `fix/promisetry-conformance-20260512`
- **PR**: #5772
- **Status**: implemented
- **Workstream**: 1 (Conformance - suppression debt cleanup)

## Intent

After PR #5755 merged, several entries in
`PRODUCTION_SUPPRESSION_DEBT_PATTERNS` no longer matched failing conformance
cases. Remove those dead suppressions so future regressions in those tests are
reported directly instead of being eligible for known-failure accounting.

The entries removed in this pass are:

- `moduleAugmentationDoesNamespaceEnumMergeOfReexport`
- `jsxNamespaceImplicitImportJSXNamespaceFromConfigPickedOverGlobalOne`
- `jsxNamespaceImplicitImportJSXNamespaceFromPragmaPickedOverGlobalOne`
- `instantiationExpressionErrorNoCrash`
- `styledComponentsInstantiaionLimitNotReached`
- `isolatedModulesReExportType`
- `typeFromPropertyAssignment39`
- `promiseTry`

## Files Touched

- `crates/conformance/src/runner.rs`

## Verification

- `cargo fmt --all --check`
- `git diff --check`
- `CARGO_TARGET_DIR=/Users/mohsen/code/tsz/.target cargo build --profile dist-fast -p tsz-conformance`
- Focused conformance for each removed pattern with the rebuilt runner:
  `FINAL RESULTS: 1/1 passed (100.0%)`, `Known failures: 0`,
  `Fingerprint-only: 0`.

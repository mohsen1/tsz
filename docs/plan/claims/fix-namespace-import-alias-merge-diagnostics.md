# fix(checker): diagnose namespace merge with import alias

- **Date**: 2026-04-27
- **Branch**: `fix/namespace-import-alias-merge-diagnostics`
- **PR**: TBD
- **Status**: ready
- **Workstream**: 1 (Diagnostic conformance)

## Intent

Fix `namespaceMergedWithImportAliasNoCrash.ts` by aligning namespace/type diagnostics around uninstantiated namespaces and import-alias namespace merges. Pure namespace value member access now reports TS2708 before polluted cross-file receiver types can produce TS2339, and conflicted import-alias/local-namespace type lookup now reports TS2694 from the local namespace export surface instead of falling through to the imported module.

## Files Touched

- `crates/tsz-checker/src/types/computation/identifier/core.rs`
- `crates/tsz-checker/src/types/property_access_type/helpers.rs`
- `crates/tsz-checker/src/types/property_access_type/resolve.rs`
- `crates/tsz-checker/src/symbols/symbol_resolver_qualified.rs`
- `crates/tsz-checker/src/symbols/symbol_resolver_utils.rs`
- `crates/tsz-checker/src/state/type_analysis/core.rs`
- `crates/tsz-checker/src/state/type_resolution/core.rs`
- `crates/tsz-checker/tests/name_resolution_boundary_tests.rs`

## Verification

- `cargo test -p tsz-checker --test name_resolution_boundary_tests -- --nocapture`
- `cargo fmt --check`
- `scripts/conformance/conformance.sh run --filter "namespaceMergedWithImportAliasNoCrash" --verbose` (1/1 pass)

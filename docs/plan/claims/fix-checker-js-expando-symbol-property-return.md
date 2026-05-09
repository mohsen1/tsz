# fix(checker): accept JS expando symbol property returns

- **Date**: 2026-05-05
- **Branch**: `fix/checker-js-expando-symbol-property-return`
- **PR**: #3259
- **Status**: ready
- **Workstream**: 1 (Diagnostic Conformance And Fingerprints)

## Intent

Fix the false-positive conformance failure in
`TypeScript/tests/cases/compiler/expandoFunctionSymbolPropertyJs.ts`. TypeScript
accepts returning a JS function whose computed `Symbol()` expando property
satisfies a callable interface with a readonly computed symbol member, but tsz
currently emits extra `TS2322` and `TS2741` diagnostics.

## Files Touched

- `docs/plan/claims/fix-checker-js-expando-symbol-property-return.md`
- `crates/tsz-checker/src/state/type_environment/type_node_resolution.rs`
- `crates/tsz-checker/src/types/computation/access.rs`
- `crates/tsz-checker/src/types/function_type.rs`
- `crates/tsz-checker/src/types/property_access_helpers/access_semantics.rs`
- `crates/tsz-checker/src/types/property_access_helpers/expando.rs`

## Verification

- `cargo check --target-dir .target -p tsz-checker`
- `cargo nextest run --target-dir .target -p tsz-checker --test js_container_merge_ts2339_tests --test commonjs_constructor_diagnostics_tests`
- `./scripts/conformance/conformance.sh run --filter "expandoFunctionSymbolPropertyJs" --verbose`

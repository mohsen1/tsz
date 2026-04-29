# [WIP] fix(checker): suppress TS2749 for namespace type-only export merge

- **Date**: 2026-04-29
- **Branch**: `fix/checker-namespace-type-only-export-ts2749`
- **PR**: #1819
- **Status**: ready
- **Workstream**: 1 (Diagnostic conformance)

## Intent

The quick-pick target is `TypeScript/tests/cases/compiler/namespacesWithTypeAliasOnlyExportsMerge.ts`, a false-positive `TS2749` where tsz reports a value-vs-type diagnostic that tsc does not. The fix keeps a value merged with a namespace valid as an intermediate qualified-type anchor (`Q2.Q.A`) while preserving `TS2749` when the same value-side symbol is used as the final type name (`Q2.Q`).

## Files Touched

- `crates/tsz-checker/src/symbols/symbol_resolver_qualified.rs`
- `crates/tsz-checker/tests/name_resolution_boundary_tests.rs`
- `docs/plan/claims/fix-checker-namespace-type-only-export-ts2749.md`

## Verification

- `cargo check --package tsz-checker --package tsz-solver`
- `cargo build --profile dist-fast --bin tsz`
- `cargo nextest run -p tsz-checker --test name_resolution_boundary_tests merged_value_namespace_reexport`
- `cargo nextest run --package tsz-checker --lib`
- `cargo nextest run --package tsz-solver --lib`
- `./scripts/conformance/conformance.sh run --filter "namespacesWithTypeAliasOnlyExportsMerge" --verbose`
- `./scripts/conformance/conformance.sh run --max 200`
- `scripts/safe-run.sh ./scripts/conformance/conformance.sh run 2>&1 | grep FINAL` (`12251/12582 passed`, `97.4%`)

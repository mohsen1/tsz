# fix(checker): suppress JSX pragma namespace circular alias

- **Date**: 2026-05-05
- **Branch**: `fix/checker-jsx-pragma-namespace-cycle`
- **PR**: https://github.com/mohsen1/tsz/pull/3398
- **Status**: draft
- **Workstream**: 1 (Diagnostic conformance)

## Intent

This PR targets the conformance mismatch in
`TypeScript/tests/cases/compiler/jsxNamespaceImplicitImportJSXNamespaceFromPragmaPickedOverGlobalOne.tsx`.
The current fingerprint has the expected duplicate identifier diagnostic but
also reports an extra `TS2456` circular type alias diagnostic. The fix will
identify why the JSX pragma namespace path is treated as an alias cycle and
suppress only the false circularity report.

## Files Touched

- `crates/tsz-checker/src/state/type_analysis/computed/type_alias_variable_alias.rs`
  - Continues using JSX runtime bridge suppression during inline alias
    circularity checks.
- `crates/tsz-checker/src/state/type_analysis/computed/jsx_runtime_bridge.rs`
  - Makes JSX runtime bridge alias detection program-aware so a pragma in the
    entry file suppresses false circularity in the imported runtime `.d.ts`.
- `crates/tsz-checker/src/state/type_analysis/computed/mod.rs`
  - Registers the focused runtime bridge helper module.
- `crates/tsz-checker/src/state/type_analysis/computed_helpers.rs`
  - Reuses the runtime bridge suppression in cross-file circular alias
    post-processing.
- `crates/tsz-checker/tests/jsx_import_source_namespace_tests.rs`
  - Adds a pragma-based regression for the `@emotion/react/jsx-runtime`
    namespace bridge.

## Verification

- `cargo fmt --check`
- `CARGO_TARGET_DIR=target-codex CARGO_INCREMENTAL=0 cargo nextest run --target-dir target-codex -p tsz-checker --test jsx_import_source_namespace_tests jsx_import_source_pragma_suppresses_runtime_bridge_alias_circularity jsx_import_source_namespace_overrides_global_jsx_intrinsic_elements`
- `CARGO_TARGET_DIR=target-codex CARGO_INCREMENTAL=0 cargo build --target-dir target-codex --profile dist-fast -j 4 -p tsz-cli -p tsz-conformance`
- `./target-codex/dist-fast/tsz-conformance --test-dir /Users/mohsen/code/tsz/TypeScript/tests/cases --cache-file /Users/mohsen/code/tsz/scripts/conformance/tsc-cache-full.json --tsz-binary ./target-codex/dist-fast/tsz --filter 'jsxNamespaceImplicitImportJSXNamespaceFromPragmaPickedOverGlobalOne' --verbose --print-fingerprints --write-diff-artifacts --diff-artifacts-dir artifacts/conformance/jsx-pragma-namespace-cycle --workers 2 --max-worker-rss-mb 1024 --max-compilations-per-worker 10`

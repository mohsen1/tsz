# fix(node-modules): resolve_via_index detection and ESM extension error classification

- **Date**: 2026-04-30
- **Branch**: `fix/node-modules-ts2307-ts2834-v2`
- **PR**: TBD
- **Status**: ready
- **Workstream**: 1 (Diagnostic Conformance)

## Intent

Fixes `resolved_via_index` detection in `resolve_relative` to correctly distinguish
direct index file resolution (`./subfolder/index` → `subfolder/index.mts`) from
directory index resolution (`./subfolder` → `subfolder/index.mts`). Adds bare
directory specifier check so `./` and `../` emit TS2307 instead of TS2834 (no
filename to suggest). Updates `fallback_needs_esm_extension_error` to respect
`importing_module_kind` rather than relying solely on file extension.

These changes improve `nodeModules1.ts` from wrong-code (code mismatch) to
fingerprint-only (error codes match, only position/text differences remain).

## Files Touched

- `crates/tsz-core/src/module_resolver/relative_resolution.rs` — `resolved_via_index`
  reconstruction with candidate filename check; bare directory specifier guard
- `crates/tsz-core/src/module_resolver/mod.rs` — `fallback_needs_esm_extension_error`
  uses `importing_module_kind` parameter
- `crates/tsz-core/src/module_resolver/tests.rs` — 2 new tests for bare directory
  TS2307 and direct index TS2835 suggestion

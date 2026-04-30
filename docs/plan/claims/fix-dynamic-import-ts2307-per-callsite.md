# fix(checker): dynamic import() emits TS2307 per call-site (no cross-site dedup)

- **Date**: 2026-04-30
- **Branch**: `claude/exciting-keller-XklDc`
- **PR**: TBD
- **Status**: claim
- **Workstream**: 1 (Diagnostic Conformance)

## Intent

`check_dynamic_import_module_specifier` used `modules_with_ts2307_emitted` to
suppress TS2307 on dynamic `import()` calls when a static import (or another
dynamic import) had already reported the same module as unresolved.  tsc emits
TS2307 independently per call-site for dynamic imports; there is no
cross-site deduplication.  Removing the dedup makes tsz match tsc for tests
like `rewriteRelativeImportExtensions/emit.ts` (4 missing TS2307 fingerprints)
and `rewriteRelativeImportExtensions/emitModuleCommonJS.ts` (1 missing).

## Files Touched

- `crates/tsz-checker/src/declarations/dynamic_import_checker.rs` — remove
  `modules_with_ts2307_emitted` contains/insert checks in both the
  resolution-error and fallback not-found paths.
- `crates/tsz-checker/tests/dynamic_import_ts2307_per_callsite_tests.rs` — 5
  new unit tests locking per-call-site emission and no-dedup invariant.
- `crates/tsz-checker/src/lib.rs` — register new test module.

## Verification Plan

- 5 new unit tests in `dynamic_import_ts2307_per_callsite_tests.rs` pass.
- Targeted conformance: `rewriteRelativeImportExtensions/emit.ts` and
  `rewriteRelativeImportExtensions/emitModuleCommonJS.ts` flip to PASS.
- Full conformance suite: net positive, zero regressions.

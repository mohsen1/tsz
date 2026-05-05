# fix(checker): align Array interface implementation TS2416 fingerprint

- **Date**: 2026-05-05
- **Branch**: `fix/checker-implement-array-interface-ts2416-fingerprint`
- **PR**: #3403
- **Status**: ready
- **Workstream**: 1 (Diagnostic Conformance And Fingerprints)

## Intent

Fix the fingerprint-only conformance failure in
`TypeScript/tests/cases/compiler/implementArrayInterface.ts`.
Both tsc and tsz emit TS2416, but the diagnostic tuple differs. The planned
scope is to root-cause the mismatch in interface implementation diagnostics,
most likely around method/property compatibility display or anchoring for a
class implementing `Array<T>`.

Root cause: the implements checker already had a global `Array<T>` display and
member path, but it only recognized symbols from the exact loaded lib arena.
Conformance/driver paths can compare against cloned standard-library symbols,
so the checker fell back to formatting the base as `Array<T>` instead of tsc's
`T[]`.

## Files Touched

- `crates/tsz-checker/src/classes/class_implements_checker/core.rs`
- `crates/tsz-checker/tests/conformance_issues/modules/declaration_module_emit.rs`
- `docs/plan/claims/fix-checker-implement-array-interface-ts2416-fingerprint.md`

## Verification

- `cargo fmt --check`
- `CARGO_BUILD_JOBS=2 CARGO_INCREMENTAL=0 cargo nextest run -j 1 -p tsz-checker --test conformance_issues test_implement_array_interface_ts2416_not_ts2420 test_module_local_array_interface_missing_member_uses_local_display test_module_local_array_interface_in_implements_shadows_global_array --no-tests=fail` (3/3 PASS)
- `./scripts/conformance/conformance.sh run --filter "implementArrayInterface" --verbose` (1/1 PASS)
- `./scripts/conformance/conformance.sh run --max 200` (200/200 PASS)

## Conformance Impact

- Flips `TypeScript/tests/cases/compiler/implementArrayInterface.ts` from
  fingerprint-only failure to pass by displaying the global `Array<T>` base as
  `T[]` in TS2416.

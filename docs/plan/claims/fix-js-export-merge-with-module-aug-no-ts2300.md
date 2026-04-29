# fix(checker): allow class+interface/namespace merge through CJS object exports

- **Date**: 2026-04-29
- **Branch**: `fix/js-export-merge-with-module-aug-no-ts2300`
- **PR**: TBD
- **Status**: claim
- **Workstream**: 1 (Conformance fixes)

## Intent

`module.exports = { Cls }` re-exports a JS class through a CJS object literal,
and a downstream module-augmentation `declare module "./test" { interface Cls
{ … } }` is a valid declaration merge (interface augments the class instance
type). The CJS-object-export duplicate-identifier guard in
`commonjs_object_exports.rs` only short-circuited on `FUNCTION` flags, so any
other mergeable conflict (interface, namespace) surfaced as a spurious TS2300
at the class declaration site.

This widens the skip mask to also cover INTERFACE, NAMESPACE_MODULE, and
VALUE_MODULE — matching tsc's declaration-merging rules for class
exports.

## Files Touched

- `crates/tsz-checker/src/types/type_checking/commonjs_object_exports.rs`:
  expand the augmentation-merge skip mask.
- `crates/tsz-checker/tests/cjs_object_export_module_augmentation_merge_tests.rs`:
  regression test asserting no TS2300 for class+interface merge through CJS
  object export.
- `crates/tsz-checker/Cargo.toml`: register the new test.

## Verification

- `cargo nextest run -p tsz-checker --lib` — 2964/2964 pass.
- `cargo nextest run -p tsz-checker --test cjs_object_export_module_augmentation_merge_tests`
  — passes.
- `./scripts/conformance/conformance.sh run --filter
  "jsExportMemberMergedWithModuleAugmentation"` — 3/3 pass (was 0/3).
- Quick regression `--max 200`: +1 bonus improvement
  (`aliasOnMergedModuleInterface.ts` flips), 0 regressions.
- Full conformance run pending before flipping `Status: ready`.

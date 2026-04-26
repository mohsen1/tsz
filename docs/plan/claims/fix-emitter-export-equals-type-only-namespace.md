# fix(emitter): erase `export = X` when X is a type-only namespace

- **Date**: 2026-04-26
- **Branch**: `fix/emitter-export-equals-type-only-namespace`
- **PR**: #1346
- **Status**: ready
- **Workstream**: 2 (JS Emit pass rate)

## Intent

`export_assignment_identifier_is_type_only` in
`crates/tsz-emitter/src/emitter/module_emission/core/mod.rs` only set
`matched_runtime` for *instantiated* `MODULE_DECLARATION`s and did
nothing in the type-only or `declare` cases. Combined with the
`matched_type && !matched_runtime` return clause, the function returned
`false` for `export = X` where `X` is a non-instantiated namespace,
so the source-file emit loop treated `export = X;` as a runtime export
and emitted `module.exports = X;` even though `X` had no JS binding.

After the fix, the namespace-name match flips into `matched_type` when
the module is `declare` or non-instantiated, mirroring the existing
treatment of interfaces, type aliases, and ambient classes/functions.
The CommonJS emit now correctly produces just the `__esModule` marker
for these files (matches tsc baseline for
`exportNamespaceDeclarationRetainsVisibility`).

This unblocks the conformance test
`tests/cases/compiler/exportNamespaceDeclarationRetainsVisibility.ts`
and any future test that exports a type-only namespace via
`export = X;`.

## Files Touched

- `crates/tsz-emitter/src/emitter/module_emission/core/mod.rs`
  (~12 LOC: split single MODULE_DECLARATION arm into runtime/type-only
  branches).
- `crates/tsz-emitter/tests/export_equals_type_only_namespace.rs`
  (new file, 4 regression tests).
- `crates/tsz-emitter/Cargo.toml` (+1 `[[test]]` entry).

## Verification

- `cargo nextest run -p tsz-emitter` — 1640 tests pass, 2 skipped.
- `scripts/emit/run.sh --filter='exportNamespaceDeclarationRetainsVisibility' --js-only`
  — was failing (+1/-1), now passes.
- `scripts/emit/run.sh --filter='export' --max=200 --js-only` — 191/200
  pass; the 9 failing tests are unrelated (module-system / decorator
  emission issues).
- `scripts/emit/run.sh --filter='namespace' --max=200 --js-only` — 186/200
  pass; the 14 failing tests are unrelated (AMD/UMD wrapper issues, JSX
  namespace handling).

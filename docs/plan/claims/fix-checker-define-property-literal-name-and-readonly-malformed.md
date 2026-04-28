# fix(checker): only literal-named defineProperty exports, malformed descriptors are readonly

- **Date**: 2026-04-28
- **Branch**: `fix/checker-define-property-literal-name-and-readonly-malformed`
- **PR**: TBD
- **Status**: claim
- **Workstream**: 1 (Diagnostic conformance — JS/CommonJS export synthesis)

## Intent

Two adjacent fixes for `Object.defineProperty(exports, ...)` synthesis in JS/`checkJs` files, surfaced by the all-missing target `conformance/jsdoc/checkOtherObjectAssignProperty.ts`:

1. `constant_define_property_name_in_file` previously followed `Identifier → const initializer` to recover a literal name. tsc's binder only treats syntactic literals as the property-name argument at bind time — references to `const`/`let` bindings (even ones initialized to a string literal) are NOT propagated, and the corresponding property is never added to the synthesized exports type. Drop the Identifier-following branch.
2. `define_property_info_from_descriptor` previously returned a permissive non-readonly any-typed property for malformed descriptors (empty `{}`, mixed `get`+`value`, lone `writable: true`). tsc actually treats those as readonly any-typed: only a paired `value` + `writable: true` data descriptor or an explicit `set` accessor makes the property writable. Flip `readonly: false` → `readonly: true` for malformed branches.

A third related plumbing fix: when a JS file has only non-literal-named `Object.defineProperty(exports, ...)` calls, the file's named-exports list is empty, but the file IS still a CommonJS module. Add `file_has_define_property_export_call` and OR it into `JsExportSurface.has_commonjs_exports` so the synthesized `typeof import(...)` type is created (with whatever literal-named exports exist) rather than collapsing to ANY.

## Files Touched

- `crates/tsz-checker/src/state/type_analysis/computed_commonjs/exports_resolution.rs` (drop Identifier branch in `constant_define_property_name_in_file`; flip `readonly: false → true` in malformed-descriptor return; add `file_has_define_property_export_call` helper).
- `crates/tsz-checker/src/query_boundaries/js_exports.rs` (use the new helper in `compute_js_export_surface` so non-literal-named defineProperty files still register as CommonJS modules).
- `crates/tsz-checker/tests/js_export_surface_tests.rs` (replace two tests that locked the old wrong-permissive behavior with two that lock the new tsc-accurate behavior).

## Verification

- `cargo nextest run -p tsz-checker --test js_export_surface_tests` — 67/67 pass (2 skipped).
- `./scripts/conformance/conformance.sh run --filter "checkOtherObjectAssignProperty"` — moves from all-missing (`actual: []`) to wrong-code (`actual: [TS2540]`), emitting 3 of 7 expected diagnostics. Remaining 4 missing TS2339 are blocked by an unrelated expando-on-`require()` augmentation bug (`mod.X = Y` writes in importer.js silently augment `typeof import("mod1")` with `X`, suppressing TS2339 on the read), out of scope for this PR.
- `./scripts/conformance/conformance.sh run --filter "checkExportsObjectAssignProperty|jsDeclarationsExportDefinePropertyEmit|lateBoundAssignmentDeclarationSupport3|checkOtherObjectAssignProperty"` — 3/4 still PASS (the same target is the one we partially improved).

## Out of scope

- The expando-on-`require()` augmentation that suppresses TS2339 in importer files. Investigated but deferred: in JS/`checkJs` mode, `mod = require("./other")` plus `mod.X = Y` augments `mod`'s type with `X`, so subsequent `mod.X` reads silently succeed. tsc emits TS2339 in this scenario. Likely lives in `crates/tsz-checker/src/types/property_access_helpers/expando.rs` and needs a guard that excludes `require()`-bound identifiers from JS-expando augmentation.

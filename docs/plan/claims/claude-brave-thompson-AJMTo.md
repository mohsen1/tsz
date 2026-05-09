# fix(checker): treat JSDoc @typedef in JS modules as type-only exports

- **Date**: 2026-05-04
- **Branch**: `claude/brave-thompson-AJMTo`
- **PR**: TBD
- **Status**: ready
- **Workstream**: 1 (Diagnostic Conformance)

## Intent

`compiler/reuseTypeAnnotationImportTypeInGlobalThisTypeArgument.ts` was
wrong-code: tsc emits two `TS2304` diagnostics inside a JSDoc `@typedef` body
(`Cannot find name 'Keyword'.` / `Cannot find name 'ParamValueTyped'.`). tsz
emitted neither and instead raised two spurious diagnostics — `TS2305`
(import) and `TS2694` (`import('./types.js').ParamStateRecord`) — because it
did not recognise that a `@typedef` declared in a `.js` module is a
type-only exported member of that module, and because the typedef body
validator skipped generic type arguments when the base name resolved.

## Root Cause

Two stacked gaps:

1. **JS module export surface ignored JSDoc typedefs.** The named-import
   check (`crates/tsz-checker/src/declarations/import/core/import_members.rs`)
   and the `import('./mod').Member` resolver
   (`crates/tsz-checker/src/state/type_resolution/import_type.rs`) both
   considered only the binder-built exports table for `.js` modules.
   `import { ParamStateRecord }` therefore failed with `TS2305`, and
   `import('./types.js').ParamStateRecord` failed with `TS2694`, even
   though the JS file declared `@typedef ParamStateRecord`.
2. **Typedef body validator stopped at the base name.** In
   `crates/tsz-checker/src/jsdoc/diagnostics.rs::check_jsdoc_typedef_base_types`,
   when a `@typedef` base type was a generic application like
   `Record<Keyword, ParamValueTyped>`, the existing code skipped further
   validation as soon as the base name (`Record`) resolved. The body's
   type arguments were never checked, so the unresolvable identifiers
   inside the brackets did not surface as `TS2304`.

## Fix

- **Helper** (`crates/tsz-checker/src/jsdoc/lookup.rs`): added
  `module_has_jsdoc_typedef_export(module_name, typedef_name, source_file_idx)`
  — resolves the import target file index for a module specifier, returns
  `true` iff the target is a `.js`/`.mjs`/`.cjs` file containing a
  top-level JSDoc `@typedef` matching the requested name. Wraps the
  pre-existing `source_file_has_jsdoc_typedef_named` (now `pub(crate)`).
- **TS2305 suppression**
  (`crates/tsz-checker/src/declarations/import/core/import_members.rs`):
  added a single new fall-through check immediately after the existing
  `js_export_surface_has_export` short-circuit at both named-import call
  sites. When the helper returns `true`, the named-import diagnostic
  pipeline `continue`s — TS2305/TS2459/TS2614/TS2724 are all skipped.
- **TS2694 / member resolution**
  (`crates/tsz-checker/src/state/type_resolution/import_type.rs`):
  `resolve_import_type_jsdoc_typedef` now (a) gates the per-source-file
  check on `source_file_has_jsdoc_typedef_named` to skip work for
  unrelated files and (b) falls back to `TypeId::ANY` when a typedef
  with the requested name exists but its body resolves to
  `TypeId::ERROR`/`UNKNOWN`. tsc treats unresolvable identifiers in a
  typedef body as `any`; mirroring that here lets `import('./mod').Name`
  resolve cleanly (no `TS2694`) while body errors still surface as
  `TS2304` from the typedef itself.
- **TS2304 typedef body recursion**
  (`crates/tsz-checker/src/jsdoc/diagnostics.rs`): when the typedef base
  type is `BaseName<arg1, arg2, …>` and `BaseName` resolves, recurse into
  each top-level type argument and emit `TS2304` for unresolvable simple
  identifiers. `@template` parameters declared on the same typedef are
  excluded.

## Files Touched

- `crates/tsz-checker/src/jsdoc/lookup.rs` (~50 LOC: new helper +
  visibility bump on existing helper)
- `crates/tsz-checker/src/declarations/import/core/import_members.rs`
  (~22 LOC: 2 short suppression checks)
- `crates/tsz-checker/src/state/type_resolution/import_type.rs`
  (~22 LOC: existence-gated body resolution + ANY fallback)
- `crates/tsz-checker/src/jsdoc/diagnostics.rs` (~23 LOC: type-arg
  recursion in `check_jsdoc_typedef_base_types`)
- `crates/tsz-checker/tests/jsdoc_typedef_module_export_tests.rs`
  (new file, 6 tests covering all three invariants — name-choice
  independent)
- `crates/tsz-checker/src/lib.rs` (test-module registration)

## Verification

- `./scripts/conformance/conformance.sh run --filter "reuseTypeAnnotationImportTypeInGlobalThisTypeArgument"` — `1/1 passed`
- `cargo nextest run --package tsz-checker --lib jsdoc_typedef_module_export` — 6/6 pass
- `cargo test --package tsz-checker --lib` — 3289 + 6 = 3295 pass, 0 failed
- `cargo fmt --all --check` — clean
- `scripts/safe-run.sh cargo clippy --workspace --all-targets --all-features -- -D warnings` — clean
- JSDoc filter conformance: `374/377` (3 pre-existing failures, unchanged)
- Import filter conformance: `280/281` (1 pre-existing failure, unchanged)
- Module filter conformance: `411/414` (3 pre-existing failures, unchanged)

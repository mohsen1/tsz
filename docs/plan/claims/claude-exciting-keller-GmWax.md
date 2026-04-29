# Claim: Fix TS2614 false-negative for `export default <identifier>`

- **Date**: 2026-04-29
- **Branch**: `claude/exciting-keller-GmWax`
- **PR**: TBD
- **Status**: ready
- **Workstream**: 1 (Diagnostic Conformance)

## Intent

`import { a } from './mod'` should emit TS2614 when `mod` only has
`export default a` (i.e., `a` is not a named export). TSZ was silently
resolving the import instead. This fix corrects the binder so that a bare
`export default <identifier>` does not pollute the module's named-exports
table.

## Problem

In `crates/tsz-binder/src/modules/import_export.rs`, processing of
`export default <identifier>` unconditionally set `sym.is_exported = true`
on the referenced local symbol (e.g., `a` in `var a = 10; export default a`).

The `populate_module_exports_from_file_symbols` loop in
`crates/tsz-binder/src/state/core.rs` then added every `is_exported` symbol
to `file_exports` under its local name. This made `exports_table.has("a")`
return `true` in the checker, suppressing the TS2614 path.

## Root Cause

In `import_export.rs`, only set `sym.is_exported = true` (and `EXPORT_VALUE`)
when the `export default` clause **is itself a declaration** (class, function,
etc.). For bare identifier references (`export default a`), do not set
`is_exported` — the checker already adds the identifier to `referenced_symbols`
when it calls `check_statement` on the clause, which suppresses TS6133 without
polluting the named-exports table.

## Files Touched

- `crates/tsz-binder/src/modules/import_export.rs` (~20 LOC change)
- `crates/tsz-binder/src/state/tests.rs` (regression test added)
- `docs/plan/claims/claude-exciting-keller-GmWax.md` (this file)

## Verification

- Targeted conformance: `es6ImportDefaultBindingFollowedWithNamedImport1` -> 3/3 pass
- Binder unit tests: all pass (`cargo nextest run -p tsz-binder`)

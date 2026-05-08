---
status: WIP
issue: 3767
agent: claude (auto-loop)
started: 2026-05-07 23:56:15 UTC
---

# Circular JSDoc `@enum` stack overflow (#3767)

## Problem
`/** @enum {E} */ const E = { x: 0 };` aborted the tsz process with
`fatal runtime error: stack overflow` because
`jsdoc_enum_annotation_type_for_symbol_decl` re-entered itself via the
JSDoc name resolver:

1. `@enum {E}` extracts `"E"` and calls `resolve_jsdoc_reference("E")`.
2. `resolve_jsdoc_type_name("E")` finds `E` as a file-local var symbol.
3. `resolve_jsdoc_symbol_type(E)` walks `symbol.declarations` and calls
   `jsdoc_enum_annotation_type_for_symbol_decl(E, decl)` again — same
   symbol, same decl, infinite recursion.

## Fix
Add an `FxHashSet<SymbolId> jsdoc_enum_resolution_set` to
`CheckerContext` and gate the public entry point
`jsdoc_enum_annotation_type_for_symbol_decl` on it. Re-entry returns
`None` and the resolver falls through to the variable's intrinsic value
type, ending the recursion.

The crash fix is the high-priority part. Matching tsc's TS2456 emission
for self-referential JSDoc enums (currently tsz emits TS2322 for the
mismatched enum body type) is a separate enhancement and tracked as
out-of-scope for this PR.

## Files
- `crates/tsz-checker/src/context/mod.rs` — new resolution-set field.
- `crates/tsz-checker/src/context/constructors.rs` — initialiser.
- `crates/tsz-checker/src/jsdoc/resolution/type_construction.rs` —
  outer guard with `_inner` body fn.
- `crates/tsz-checker/tests/jsdoc_enum_circular_tests.rs` — three
  regression tests (direct self-ref, mutual A↔B cycle, non-circular
  baseline).
- `crates/tsz-checker/src/lib.rs` — register the new test module.

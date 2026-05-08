# fix(emitter): preserve local alias used by inferred call-expression export (#3755)

- **Date**: 2026-05-08
- **Branch**: `fix/declaration-emit-local-alias-3755`
- **PR**: TBD
- **Status**: claim
- **Workstream**: declaration emit

## Intent

Declaration emit was dropping a local type alias that an `export const`
only references through the inferred type of a call-expression
initializer. tsc emits the alias declaration alongside the export; tsz
left the .d.ts referencing the undeclared alias and the output failed
to type-check (`TS2304: Cannot find name 'Box'`).

The retention pre-pass walks exported variable statements without type
annotations and, for call-expression initializers, calls
`call_expression_declared_return_type_text(initializer)` — which
returns the *source-text* of the callee's declared return type
annotation, preserving local alias names verbatim. Identifiers from
that text are added to `used_symbols` via the existing
`retain_local_type_names_for_public_api` helper.

The path is intentionally narrow:
- Only fires for VARIABLE_STATEMENT (or `EXPORT_DECLARATION` wrapping
  one) with effective export.
- Skips declarations that already have a type annotation.
- Only applies the call-expression annotation-text fallback —
  structural type-printer text was tried first but caused regressions
  in tests that depend on the existing public-API filter pruning
  unrelated symbols, so the structural path was dropped.

## Files Touched

- `crates/tsz-emitter/src/declaration_emitter/helpers/synthetic_dependencies.rs`
  — new `retain_synthetic_variable_declaration_dependencies_*` walker.
- `crates/tsz-emitter/src/declaration_emitter/core/emit_declarations.rs`
  — wire the walker into the `.ts` retention pass alongside the
  function/class/property walkers.
- `crates/tsz-emitter/src/declaration_emitter/tests/misc_features.rs`
  — `test_inferred_const_initializer_call_preserves_local_alias` locks
  the issue's repro.

## Verification

- `cargo nextest run -p tsz-emitter` — 2235/2235 pass on rebased branch.
- `./scripts/emit/run.sh --dts-only` — DTS pass rate unchanged at
  1457/1669 (no regressions).
- Manual verification: the issue's repro now emits the `type Box =`
  declaration alongside `export declare const item: Box;`.

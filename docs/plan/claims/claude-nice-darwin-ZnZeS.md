# fix(binder): stop polluting shadow's declarations vec when preserving lib meaning (#4687)

- **Date**: 2026-05-08
- **Branch**: `claude/nice-darwin-ZnZeS`
- **PR**: TBD
- **Status**: claim
- **Workstream**: Conformance / lib-shadow correctness

## Intent

PR #4634 ("preserve lib's other-namespace meaning when shadowing") fixed
false TS2749/TS2339 by attaching the lib symbol's other-namespace
declarations onto the new module-local shadow symbol's `declarations` vec
and `declaration_arenas` map. That pollution entangled type-alias
evaluation memoization for unrelated user types whose computed property
keys reference the shadow symbol's value (e.g.
`export declare const Readonly: unique symbol` shadowing lib's
`type Readonly<T>`) — see issue #4687, where `deeplyNestedMappedTypes`
collapses `Input` and `Output` to the same shape.

This change keeps the `flags` and `value_declaration` preservation
(plus a single `declaration_arenas` entry for the preserved
`value_declaration`) but stops adding the lib's TYPE/INTERFACE
declarations onto the user shadow's `declarations` vec. A new
`Symbol::lib_shadow_origin` field records the original lib `SymbolId`
for future checker-side fallbacks.

## Files Touched

- `crates/tsz-binder/src/symbols.rs` — add `lib_shadow_origin: SymbolId` field
- `crates/tsz-binder/src/nodes/binding.rs` — drop declarations-vec pollution; keep flags + value_declaration; mirror only the value-decl arena entry
- `crates/tsz-checker/tests/lib_global_namespace_shadowing_tests.rs` — add regression tests covering #4687 plus the original #3502 invariants

## Verification

- `cargo test -p tsz-checker --test lib_global_namespace_shadowing_tests` — 7 pass (5 original + 2 new)
- `cargo test -p tsz-binder --lib` — 339 pass
- `cargo test -p tsz-checker --lib` — 3766 pass
- `./scripts/conformance/conformance.sh run --filter deeplyNestedMappedTypes` — 1/1 pass (was 0/1 fingerprint-only before fix)

## Conformance

Conformance delta vs baseline tracked in PR body once the full snapshot
finishes; the fix flips `deeplyNestedMappedTypes` from FAIL→PASS and
resolves the source-side shape conflation reported in #4687.

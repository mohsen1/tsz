# fix(solver): import-qualify namespace-qualified types from foreign modules in TS2741 messages

- **Date**: 2026-04-26
- **Branch**: `claude/exciting-keller-8enNl`
- **PR**: TBD
- **Status**: ready
- **Workstream**: 1 (Diagnostic Conformance — fingerprint-only fixes)

## Intent

`disambiguate_union_member_names` resolves name collisions in diagnostic messages
by first namespace-qualifying duplicates (Pass 1), then import-qualifying any
remaining collisions (Pass 2). The bug: Pass 2 only ran when names still collided
after Pass 1. For `JSX.Element` vs `predom.JSX.Element`, Pass 1 produces unique
names and Pass 2 never fires, so tsc's expected
`import("renderer2").predom.JSX.Element` display was never emitted.

Fix: track which slots were namespace-qualified in Pass 1. In Pass 2, apply
import-qualification to those slots unconditionally (not only on collision) —
but skip `declare global { }` augmentation types since they're globally
accessible and tsc never import-qualifies them.

Fixes `inlineJsxFactoryLocalTypeGlobalFallback.tsx` (0% → 100%).

## Files Touched

- `crates/tsz-solver/src/diagnostics/format/compound.rs` (~40 LOC change)
  - `disambiguate_union_member_names`: track `was_ns_qualified` per slot;
    trigger Pass 2 on `was_ns_qualified[i]` in addition to collision
  - `import_qualified_name_for_type`: skip global augmentation types;
    visibility widened to `pub(crate)` for test access
- `crates/tsz-solver/src/diagnostics/format/tests.rs` (~130 LOC added)
  - 3 new unit tests locking the new disambiguation behavior

## Verification

- `cargo test --package tsz-solver --lib -- disambiguate` → 2 tests pass
- `cargo test --package tsz-solver --lib -- global_augmentation` → passes
- `./scripts/conformance/conformance.sh run --filter "inlineJsxFactoryLocalTypeGlobalFallback"` → 1/1 (100%)
- `./scripts/conformance/conformance.sh run --max 300` → no regressions vs baseline

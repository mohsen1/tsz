# fix(checker): cross-file fallback for class cycle detection

- **Date**: 2026-04-30
- **Branch**: `fix/checker-nonnull-contextual-overload-document` (reused)
- **PR**: TBD
- **Status**: ready
- **Workstream**: 1 (Conformance)

## Intent

`classExtendsItselfIndirectly3.ts` declares a 3-cycle of classes split across 6 files (`C extends E` in file1, `D extends C` in file2, `E extends D` in file3, plus a generic mirror). tsc emits TS2506 for every class in the cycle; tsz emitted **zero** because the cycle DFS, when walking from a class's declaration in another file, called `binder.resolve_identifier` against that file's binder — which in script-mode (non-module) projects can't see classes declared elsewhere.

The DFS already uses `get_arena_for_file` / `get_binder_for_file` in `resolve_declared_parent_symbols` to locate the right arena+binder for a declaration's home file. The fix adds a *cross-file fallback* in `resolve_heritage_symbol_with`: when the per-file binder's `resolve_identifier` returns `None`, walk `ctx.all_binders` and look for a matching `file_locals` entry in any other non-module binder. This restores tsc parity for cross-file inheritance cycle detection without affecting module-shaped projects.

## Files Touched

- `crates/tsz-checker/src/classes/class_inheritance.rs` — new `all_binders`-walking branch in `resolve_heritage_symbol_with` (≈25 LOC including rationale comment).
- `crates/tsz-checker/tests/ts2506_cross_file_cycle_tests.rs` — two integration tests exercising the bare and generic 3-cycle shapes; the conformance suite is the authoritative no-regression guard for the negative direction (counter-test omitted from this unit suite — see in-file note explaining why).
- `crates/tsz-checker/Cargo.toml` — register the new test target.

## Verification

- `cargo nextest run -p tsz-checker --test ts2506_cross_file_cycle_tests` — 2/2 pass.
- `cargo nextest run -p tsz-checker` — 5738 tests pass.
- `bash scripts/conformance/conformance.sh run` — net **+13** (12262 → 12275): `classExtendsItselfIndirectly3.ts` flips FAIL → PASS alongside the 14 BCT-pipeline siblings from the earlier `subtypeRelationForNever` fix. The two remaining `PASS → FAIL` entries (`circularInlineMappedGenericTupleTypeNoCrash.ts`, `typeGuardConstructorClassAndNumber.ts`) are stale-baseline artifacts: both fail on `origin/main` without this PR as well.
- CLI smoke: `tsz file1.ts file2.ts file3.ts` (the cycle) emits TS2506 for all three; `tsz base.ts derived.ts` (no cycle) emits zero TS2506.

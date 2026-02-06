# Perf Path Forward (2026-02-06)

## Stopping Point

Optimization work paused by request after landing and validating one focused change.

## Proven Change

Added memoization for `ensure_application_symbols_resolved` traversal so repeated assignability checks avoid re-walking the same type graphs.

Files:
- `src/checker/context.rs`
- `src/checker/state_type_environment.rs`
- `src/checker/state_checking.rs`

Key mechanics:
- `CheckerContext` now tracks:
  - `application_symbols_resolved: FxHashSet<TypeId>`
  - `application_symbols_resolution_set: FxHashSet<TypeId>`
- `ensure_application_symbols_resolved` now:
  - fast-returns for already resolved roots
  - guards recursion on in-progress roots
  - records fully-resolved visited nodes in memo set
- Memoization is cleared per source file before rebuilding `type_env`.

## Validation

Build validation:
- `cargo check -q` passed.

Benchmark validation (`./scripts/bench-vs-tsgo.sh --quick --filter '100 classes'`):
- Before this change in current session: `tsz 277.07ms` vs `tsgo 209.16ms` (`tsgo 1.32x faster`).
- After change with rebuild: `tsz 82.82ms` vs `tsgo 149.81ms` (`tsz 1.81x faster`).
- Follow-up rerun: `tsz 107.36ms` vs `tsgo 136.21ms` (`tsz 1.27x faster`).

Note: variance remains high on this machine; direction is consistently positive for this benchmark after the patch.

## Path Forward (Next Issues, One at a Time)

1. `100 classes` to `>2x`
- Re-profile with symbols on `synthetic_100_classes.ts` after this memoization patch.
- Target next hotspot only (likely class member/type-parameter resolution churn still in method checking).
- Keep change localized and remeasure with `--filter '100 classes'`.

2. `enumLiteralsSubtypeReduction.ts` to `>2x`
- Add a specific microbench in `benches/` that isolates enum literal subtype reduction behavior.
- Profile subtype/union reduction path and optimize one hotspot at a time.

3. Stabilize perf signal
- Keep using `--filter` for fast iteration.
- For acceptance, run at least 2-3 consecutive filtered runs and use medians before deciding to keep/revert a perf patch.

## Useful Commands

- Targeted class benchmark:
  - `./scripts/bench-vs-tsgo.sh --quick --filter '100 classes'`
- Rebuild + targeted benchmark:
  - `./scripts/bench-vs-tsgo.sh --quick --rebuild --filter '100 classes'`
- Full quick sweep:
  - `./scripts/bench-vs-tsgo.sh --quick`
- Profile one case:
  - `sample <pid> 5 -mayDie -file /tmp/tsz.sample.txt`

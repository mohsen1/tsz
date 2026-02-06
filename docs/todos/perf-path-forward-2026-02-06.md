# Perf Path Forward (2026-02-06)

## Stopping Point

Resumed optimization work and landed one additional focused fix with isolated benchmark evidence.

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

## Proven Change (Follow-up Session)

Fixed unbounded recursion in private brand assignability override for non-progressing Lazy resolution.

Files:
- `src/solver/compat.rs`
- `src/solver/tests/compat_tests.rs`

Key mechanics:
- `private_brand_assignability_override` now fast-returns for identical `source`/`target` TypeIds.
- Added non-progress guards for Lazy resolution:
  - if `source` Lazy resolves back to the same `source`, return `None` (fall through to normal structural path)
  - if `target` Lazy resolves back to the same `target`, return `None`
- Added regression test:
  - `test_private_brand_lazy_self_resolution_does_not_recurse`
  - uses a resolver that maps `DefId -> same Lazy(TypeId)` and asserts the override returns `None` without recursion.

Root-cause evidence:
- `manyConstExports.ts` previously hit stack overflow in:
  - `CompatChecker::private_brand_assignability_override` (recursive frame repetition at `compat.rs:1195` from lldb backtrace)

Validation:
- Targeted tests:
  - `cargo test -q test_private_brand_lazy_self_resolution_does_not_recurse` passed.
  - `cargo test -q private_brand_` passed.
- Direct run check:
  - `.target-bench/dist/tsz --noEmit TypeScript/tests/cases/compiler/manyConstExports.ts` now exits 0 (no stack overflow).
- Benchmarks (`./scripts/bench-vs-tsgo.sh --quick --filter 'manyConstExports'`):
  - Run 1: `tsz 86.37ms` vs `tsgo 216.34ms` (`tsz 2.50x faster`)
  - Run 2: `tsz 86.85ms` vs `tsgo 216.49ms` (`tsz 2.49x faster`)
  - Run 3: `tsz 87.36ms` vs `tsgo 217.80ms` (`tsz 2.49x faster`)
- Regression spot-check:
  - `./scripts/bench-vs-tsgo.sh --quick --filter '100 classes'` -> `tsz 95.80ms` vs `tsgo 227.74ms` (`tsz 2.38x faster`)

## Proven Change (BCT Perf Follow-up)

Reduced assignability hot-path overhead by memoizing `contains_infer_types` decisions in checker state and reducing repeated subtree work in `contains_type_matching`.

Files:
- `src/checker/context.rs`
- `src/checker/state_checking.rs`
- `src/checker/assignability_checker.rs`
- `src/checker/state_type_environment.rs`
- `src/solver/visitor.rs`

Key mechanics:
- Added per-file memo sets in `CheckerContext`:
  - `contains_infer_types_true: FxHashSet<TypeId>`
  - `contains_infer_types_false: FxHashSet<TypeId>`
- Added `contains_infer_types_cached` helper on `CheckerState` and switched assignability/subtype cacheability checks to use it.
- Reused the same memoization in `evaluate_application_type` cacheability gate.
- Cleared per-file infer-memo sets at file-check start alongside other per-file memo state.
- Optimized `ContainsTypeChecker` internals with per-traversal memoization:
  - `memo: FxHashMap<TypeId, bool>`
  - avoids re-traversing repeated subgraphs during one `contains_*` query
  - preserves existing depth/cycle behavior for early exits.

Profiling signal:
- Debug sampling on scaled BCT stress showed dominant stack:
  - `check_return_statement -> is_assignable_to -> contains_infer_types -> ContainsTypeChecker`.

Validation:
- `cargo check -q` passed.
- `cargo test -q best_common_type` passed.
- Target benchmark (`./scripts/bench-vs-tsgo.sh --filter 'BCT candidates=200'`):
  - Before this follow-up: `tsz 366.98ms` vs `tsgo 249.39ms` (`tsgo 1.47x faster`)
  - After checker-side memoization: `tsz 333.53ms` vs `tsgo 240.89ms` (`tsgo 1.38x faster`)
  - After `ContainsTypeChecker` memoization + rebuild: `tsz 319.74ms` vs `tsgo 240.97ms` (`tsgo 1.33x faster`)
  - Follow-up runs: `320.95ms` and `318.76ms` (stable direction)
- Spot-check regressions:
  - `./scripts/bench-vs-tsgo.sh --quick --filter '100 classes|BCT candidates=50'`:
    - `100 classes`: `tsz 90.96ms` vs `tsgo 217.15ms` (`tsz 2.39x faster`)
    - `BCT candidates=50`: `tsz 86.40ms` vs `tsgo 216.41ms` (`tsz 2.50x faster`)

## Proven Change (Flow Analysis Fast Path for Non-Variables)

Eliminated unnecessary control-flow narrowing work for identifier symbols that are not variable-like bindings.

Files:
- `src/checker/flow_analysis.rs`

Key mechanics:
- Added a fast-path guard in `check_flow_usage`:
  - only symbols with `symbol_flags::VARIABLE` participate in definite-assignment and flow narrowing.
  - class/function/namespace/type-value merged symbols now return their declared type directly.
- This avoids expensive `FlowAnalyzer::get_flow_type` traversals for identifiers like class constructors in `new DerivedN()`.

Profiling signal:
- Symbolized sampling on scaled BCT stress (`/tmp/bct_1000.ts`) showed dominant stack:
  - `get_type_of_identifier -> check_flow_usage -> apply_flow_narrowing -> FlowAnalyzer::get_flow_type`.
- The hottest per-use pattern came from constructor identifiers in `new DerivedN()` across array/return/call stress shapes.

Validation:
- `cargo check -q` passed.
- `cargo test -q test_ts2454_` passed.
- `cargo test -q test_closure_capture_` passed.
- Target benchmark (`./scripts/bench-vs-tsgo.sh --rebuild --filter 'BCT candidates=200'`):
  - Before this change (after BCT follow-up): `tsz 319.74ms` vs `tsgo 240.97ms` (`tsgo 1.33x faster`)
  - After this change: `tsz 121.90ms` vs `tsgo 241.34ms` (`tsz 1.98x faster`)
- Follow-up filtered set from prior “worse” list:
  - `BCT candidates=200`: `121.09ms` vs `240.82ms` (`tsz 1.99x faster`)
  - `BCT candidates=100`: `88.83ms` vs `219.88ms` (`tsz 2.48x faster`)
  - `200 classes`: `104.48ms` vs `219.49ms` (`tsz 2.10x faster`)
  - `privacyVar.ts`: `74.97ms` vs `214.20ms` (`tsz 2.86x faster`)
  - `Constraint conflicts N=100`: `100.55ms` vs `218.89ms` (`tsz 2.18x faster`)
  - `Constraint conflicts N=200`: `206.08ms` vs `236.32ms` (`tsz 1.15x faster`)
  - `resolvingClassDeclarationWhenInBaseTypeResolution.ts`: `156.66ms` vs `226.82ms` (`tsz 1.45x faster`)

## Path Forward (Next Issues, One at a Time)

1. `enumLiteralsSubtypeReduction.ts` to `>2x`
- Add a specific microbench in `benches/` that isolates enum literal subtype reduction behavior.
- Profile subtype/union reduction path and optimize one hotspot at a time.

2. Stabilize perf signal
- Keep using `--filter` for fast iteration.
- For acceptance, run at least 2-3 consecutive filtered runs and use medians before deciding to keep/revert a perf patch.

## Unproven Stash Candidates

The stash `stash@{0}` (`temp-commit-isolation-2026-02-06`) contains additional WIP that was **not** validated with isolated benchmark evidence and was intentionally not committed as proven perf work.

Candidate changes in that stash:
- `src/checker/state_checking_members.rs`:
  - adds memoization of class instance `this` type in constructor checking via `cached_instance_this_type`.
- `src/checker/type_checking_queries.rs`:
  - reuses cached instance `this` type in `class_member_this_type`.
- `src/checker/class_checker.rs`:
  - adds tracing counters/timing around `check_implements_clauses`.
- `src/checker/class_type.rs`:
  - adds tracing instrumentation on class type construction methods.

How to inspect later:
- `git stash show --name-status stash@{0}`
- `git stash show -p stash@{0}`

Recommendation for future session:
- Apply one candidate at a time.
- Re-run `./scripts/bench-vs-tsgo.sh --quick --filter '100 classes'`.
- Keep only candidates that repeatedly improve median runtime (2-3 runs), then commit separately.

## Useful Commands

- Targeted class benchmark:
  - `./scripts/bench-vs-tsgo.sh --quick --filter '100 classes'`
- Rebuild + targeted benchmark:
  - `./scripts/bench-vs-tsgo.sh --quick --rebuild --filter '100 classes'`
- Full quick sweep:
  - `./scripts/bench-vs-tsgo.sh --quick`
- Profile one case:
  - `sample <pid> 5 -mayDie -file /tmp/tsz.sample.txt`

# Claim — Arena-direct lowering for `TypeEnvironmentCore` type-param extraction

**Owner:** claude (this session)
**Branch:** `perf/typeenv-core-arena-direct-2026-05-13`
**Draft PR:** to be opened with this claim
**Sequences after:** #6111 (cache source-file symbol arena delegation)
**Decision record reference:** [`perf-runs/2026-05-13-attribution-post-6111.md`](../perf-runs/2026-05-13-attribution-post-6111.md)

## Goal

Reduce `with_parent_cache_by_reason[TypeEnvironmentCore]` on the scale-cliff
fixtures by extending the existing arena-direct fast path
(`extract_simple_type_params_from_decl_in_arena`) so that it covers cases
that currently fall through to a `CheckerState::with_parent_cache_attributed`
construction without doing meaningful type-checking work.

Per the decision record refreshed on 2026-05-13, `TypeEnvironmentCore`
constructs **5.7× more child checkers than `DelegateCrossArenaSymbol`** on
`monorepo-006` (5,259 vs 924) and **13.3×** on `monorepo-003` (5,108 vs 384).
The construction site is `crates/tsz-checker/src/state/type_environment/core.rs`
at lines ~1841 and ~1959 — both inside `get_type_params_for_symbol` and
keyed by the `cross_file_type_params_cache` env-var-gated memo.

## Why now / why this and not the env-var cache

The same 2026-05-13 attribution run showed that
`cross_file_type_params_cache` has **0 hits** and ~5,000 misses on each
cliff fixture when forced on, because each child-checker construction
queries a unique `(file_idx, decl_idx)` key once. Default-enabling the
cache is therefore not the lever — the lever is **never constructing the
child checker** for cases where the result is structural and arena-only.

The `cross_file_cache_miss_causes` table from #5863 is all-zero here too,
because the hot path bypasses the canonical `cached_cross_file_*` readers
on its way to construction.

## Audit of fall-through reasons

In `extract_simple_type_params_from_decl_in_arena`
(`core.rs` ~line 1539) the arena-direct path bails out — returning `None`
— in three distinct cases that then trigger `with_parent_cache_attributed`:

| Bail-out | Slow-path semantics |
| --- | --- |
| Class with no `type_parameters` AST list | Slow path returns `Some(Vec::new())` (after a `JSDoc @template` check). |
| Interface with no `type_parameters` AST list | Slow path returns `Some(Vec::new())`. |
| Any type parameter has `constraint != NONE` or `default != NONE` | Slow path lowers the constraint/default type expression via `push_type_parameters`. |

The first two cases are the **easy and likely dominant** subset: the
work the slow path does is purely arena reads (no type interning, no
relation, no diagnostic emission). `jsdoc_template_type_params_for_decl`
already only reads `arena.source_files[0].text`, `comments`,
`arena.get_extended(decl_idx)`, and calls `try_leading_jsdoc` (which is
arena-only itself — its `&self` receiver is unused). Refactoring this
into an arena-keyed function lets the fast path cover both no-typeparams
cases without touching the constraint/default case.

The third case is real semantic work and stays on the slow path for
this slice.

## What this slice does

1. Extract a free function `jsdoc_template_type_params_for_decl_in_arena(
   arena, decl_idx, atom_interner) -> Option<Vec<TypeParamInfo>>` that
   does the same thing as today's `jsdoc_template_type_params_for_decl`
   but takes the arena directly.
2. Change `extract_simple_type_params_from_decl_in_arena` so that when a
   class or interface has no `type_parameters`, it:
   - returns `Some(jsdoc_params)` if JSDoc `@template` is present;
   - returns `Some(Vec::new())` otherwise.
   (Both match what the slow path does today.)
3. Keep the existing constraint/default bail-out for now — that's the
   next slice. No semantic change to that path.

## What this slice does NOT do

- No change to the constraint/default lowering path. Type parameters
  with constraint or default still go through the slow path.
- No removal of `cross_file_type_params_cache`. It remains env-var-gated.
  A separate decision can default-enable or remove it once the data
  supports a call.
- No changes to `DelegateCrossArenaSymbol` (that's #6111's lane) or any
  other `CheckerCreationReason`.
- No new perf-counter buckets. The existing
  `with_parent_cache_by_reason[TypeEnvironmentCore]` counter is the
  before/after signal.

## Correctness plan

- Add a unit test that runs the arena-direct path against:
  - a plain class `class Foo {}`,
  - a plain interface `interface Bar {}`,
  - both with a `JSDoc @template` comment producing one and two params,
  - both with `is_const` modifier on the JSDoc param,
  - both placed inside an `EXPORT_DECLARATION` wrapper (the JSDoc
    leading-comment search-pos adjustment the slow path performs).
- Conformance must remain at 100%. Diagnostics must be byte-identical.

## Expected attribution signal

After this slice, on the same scale-cliff fixtures and same
`--features perf-tools` build, `with_parent_cache_by_reason[
TypeEnvironmentCore]` should drop. The exact magnitude depends on the
class/interface vs. type-alias mix in each fixture; the decision record
recorded 5,259 constructions on monorepo-006 and the slow path's
"work" is exactly the JSDoc-or-empty branch for the no-typeparams cases.
The follow-up attribution run will quote the actual delta.

## Coordination

- No file overlap with in-flight PRs (#6111 touches
  `cross_file_query.rs`; this touches `type_environment/core.rs` and a
  small piece of `jsdoc/params.rs`).
- No conformance-baseline churn expected.
- Will not touch `cross_file_type_params_cache` wiring — leaving it for
  a later decision.

## Exit criteria for the PR

1. Arena-direct path covers no-typeparams class and interface
   (including JSDoc `@template`).
2. Unit tests added in `crates/tsz-checker` for the no-typeparams +
   JSDoc cases (covering both name choices, per the anti-hardcoding
   directive).
3. Full conformance unchanged (100%).
4. Lint clean.
5. Decision-record follow-up under `docs/plan/perf-runs/` with the
   measured `with_parent_cache_by_reason[TypeEnvironmentCore]` delta on
   `monorepo-001..006` once CI is green.

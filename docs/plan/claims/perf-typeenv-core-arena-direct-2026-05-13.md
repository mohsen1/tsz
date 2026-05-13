# Claim — Arena-direct lowering for `TypeEnvironmentCore` type-param extraction

**Owner:** Codex session
**Branch:** `perf/typeenv-core-arena-direct-2026-05-13`
**Draft PR:** #6144
**Sequences after:** #6111 (cache source-file symbol arena delegation)
**Input decision record:** [`perf-runs/2026-05-13-post-6111-attribution.md`](../perf-runs/2026-05-13-post-6111-attribution.md)
**Follow-up decision record:** [`perf-runs/2026-05-13-typeenv-arena-direct-attribution.md`](../perf-runs/2026-05-13-typeenv-arena-direct-attribution.md)

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

1. Adds `jsdoc_template_params_in_arena` in
   `crates/tsz-checker/src/state/type_environment/core.rs`, preserving the
   existing leading-comment and `export` wrapper behavior while avoiding a
   child checker for class declarations that have no AST type-parameter list.
2. Changes `extract_simple_type_params_from_decl_in_arena` so a class with no
   AST type parameters returns the arena-derived JSDoc `@template` params, or
   `Some(Vec::new())` when no template is present.
3. Changes the interface branch so a non-interface declaration candidate for a
   merged interface/value symbol returns `Some(Vec::new())` instead of bailing
   out. The candidate cannot contribute type params, and this lets later real
   interface declarations provide the params without first constructing a child
   checker.
4. Registers and extends the focused
   `jsdoc_class_template_arena_direct_tests` test target.
5. Keeps the existing constraint/default bail-out for now. Type parameters with
   constraints or defaults still use the slow path.

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

## Verification

- `cargo test -p tsz-checker --test jsdoc_class_template_arena_direct_tests -- --nocapture`
- `cargo test -p tsz-checker --lib type_environment -- --nocapture`
- `CARGO_TARGET_DIR=.target cargo build -p tsz-cli --bin tsz --features perf-tools --release`
- `TSZ_PERF_COUNTERS=1 .target/release/tsz --extendedDiagnostics --noEmit -p scripts/bench/scale-cliff/fixtures/monorepo-{001..006}/tsconfig.json --diagnostics-json ... --perf-counters-json ...`

## Measured attribution signal

On the same scale-cliff fixtures and the same `--features perf-tools` build,
`with_parent_cache_by_reason[TypeEnvironmentCore]` drops from thousands of
child-checker constructions to one per fixture:

| Fixture | before | after | delta |
| --- | ---: | ---: | ---: |
| monorepo-001 | 110 | 1 | -109 |
| monorepo-002 | 1,019 | 1 | -1,018 |
| monorepo-003 | 5,108 | 1 | -5,107 |
| monorepo-004 | 5,160 | 1 | -5,159 |
| monorepo-005 | 5,210 | 1 | -5,209 |
| monorepo-006 | 5,259 | 1 | -5,258 |

The follow-up record keeps `DelegateCrossArenaSymbol` unchanged, which matches
the slice boundary.

## Coordination

- No file overlap with in-flight PRs (#6111 touches
  `cross_file_query.rs`; this touches `type_environment/core.rs` and a
  small piece of `jsdoc/params.rs`).
- No conformance-baseline churn expected.
- Will not touch `cross_file_type_params_cache` wiring — leaving it for
  a later decision.

## Exit criteria for the PR

1. Arena-direct path covers no-typeparams class and interface cases that do
   not require constraint/default lowering.
2. Focused `crates/tsz-checker` tests cover plain class/interface,
   JSDoc-template classes including `@template const`, exported
   JSDoc-template classes, exported plain interfaces, and merged
   interface/value symbols.
3. Targeted type-environment tests pass locally.
4. Lint and formatting are clean.
5. Decision-record follow-up under `docs/plan/perf-runs/` records the measured
   `with_parent_cache_by_reason[TypeEnvironmentCore]` delta on
   `monorepo-001..006`.

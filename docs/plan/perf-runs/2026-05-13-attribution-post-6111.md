# 2026-05-13 (afternoon) — Attribution Refresh, `TypeEnvironmentCore` Bigger Lever

Companion record to [`2026-05-13-post-5863-attribution.md`](2026-05-13-post-5863-attribution.md)
(#6071, morning). #6071 already ran post-#5863 attribution and
correctly identified that `cross_file_cache_miss_causes` is zero
because the dominant `DelegateCrossArenaSymbol` path bypasses the
gateway (PR #6111 fixes that bypass).

This afternoon record re-runs the same fixtures with #6111 in flight
on the same `origin/main` tip (`56d8c32594`) and zooms in on the
per-reason breakdown that #6071's table includes but does not lean on
for its decision:

> `TypeEnvironmentCore` is **5.7 × the size of `DelegateCrossArenaSymbol`
> on monorepo-006 and 13.3 × on monorepo-003**.

#6071 concludes "the next T2.2 PR should target `DelegateCrossArenaSymbol`
first" — that is correct for the *instrumented-gateway* fix (the
path #6111 is migrating onto `CrossFileQueryKind::SymbolType`). This
record adds the **second next-priority claim**: after #6111, the
single biggest remaining lever in the post-#5863 data is the
`TypeEnvironmentCore` path, and the right fix for it is *not* a
gateway migration but an arena-direct type-parameter lowering that
drops the child-checker entirely.

## Reproducer

| Item | Value |
| --- | --- |
| `tsz` commit | `56d8c32594` (`origin/main` tip, pre-#6111) |
| `tsz` build | `cargo build -p tsz-cli --bin tsz --features perf-tools --release` |
| Fixture path | `scripts/bench/scale-cliff/fixtures/monorepo-{001..006}` |
| Fixture generator | `scripts/bench/scale-cliff/generate-fixtures.sh` (default sizes) |
| Bench harness | direct `tsz` invocation per fixture (`TSZ_PERF_COUNTERS=1`) |
| Timing feature | `tsz-common/perf-counters-timing` ON (transitively via `perf-tools`) |
| Machine | macOS Darwin 25.2.0 (single host, no warmup) |
| `large-ts-repo` | not measured — same OOM/stack-overflow blockers as 2026-05-{10,11} |

Raw JSON: `docs/plan/perf-runs/raw/2026-05-13-monorepo-{001..006}-{pc,diag}.json`.

> **Wall-time disclaimer**: `perf-counters-timing` is ON. Per
> [`PERFORMANCE_PLAN.md`](../PERFORMANCE_PLAN.md) §3 lines 84–85, every
> interner write lock pays an `Instant::now()` bracket. Wall times in
> this run are not comparable to timing-mode `tsz` or to `tsgo`. Use the
> [2026-05-10 baseline](2026-05-10-scale-cliff-summary.md) for any
> wall-time comparison.

## Lock-Wait Histogram

| Fixture | Total waits | <100ns | <1µs | <10µs | <100µs | <1ms | <10ms | <100ms | overflow |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| monorepo-001 | 5,890 | 5,800 | 90 | 0 | 0 | 0 | 0 | 0 | 0 |
| monorepo-002 | 37,390 | 36,745 | 637 | 7 | 1 | 0 | 0 | 0 | 0 |
| monorepo-003 | 177,537 | 176,555 | 944 | 29 | 9 | 0 | 0 | 0 | 0 |
| monorepo-004 | 177,548 | 175,761 | 1,745 | 21 | 21 | 0 | 0 | 0 | 0 |
| monorepo-005 | 177,548 | 176,573 | 942 | 26 | 7 | 0 | 0 | 0 | 0 |
| monorepo-006 | 179,635 | 177,895 | 1,706 | 25 | 9 | 0 | 0 | 0 | 0 |

The May-11 conclusion stands: **the interner is not contention-bound**
at the cliff. 99.5 %+ of waits land in `<100ns`, no observation crosses
`1 ms`. T2.4 stays de-prioritised.

## Child-Checker Construction

| Fixture | files (diag) | state_constructed | with_parent_cache | ratio |
| --- | ---: | ---: | ---: | ---: |
| monorepo-001 | 183 | 102 | 165 | 1.62 |
| monorepo-002 | 1,092 | 1,011 | 1,074 | 1.06 |
| monorepo-003 | 5,181 | 5,100 | 5,506 | 1.08 |
| monorepo-004 | 5,233 | 5,152 | 5,608 | 1.09 |
| monorepo-005 | 5,283 | 5,202 | 5,658 | 1.09 |
| monorepo-006 | 5,332 | 5,251 | 6,197 | 1.18 |

The monorepo-006 ratio dropped from **1.22 → 1.18** between
2026-05-11 and 2026-05-13 (~3 % drop). The interesting movement is the
*size* of the per-reason count, broken out below.

### Child Checkers By `CheckerCreationReason`

Only non-zero reasons shown.

| Fixture | DelegateCrossArenaSymbol | TypeEnvironmentCore | CallHelpers | total nonzero |
| --- | ---: | ---: | ---: | ---: |
| monorepo-001 | 41 | 110 | 14 | 165 |
| monorepo-002 | 41 | 1,019 | 14 | 1,074 |
| monorepo-003 | 384 | **5,108** | 14 | 5,506 |
| monorepo-004 | 434 | **5,160** | 14 | 5,608 |
| monorepo-005 | 434 | **5,210** | 14 | 5,658 |
| monorepo-006 | 924 | **5,259** | 14 | 6,197 |

**Headline finding**: `TypeEnvironmentCore` is **5.7 ×** the size of
`DelegateCrossArenaSymbol` on monorepo-006 (5,259 vs 924), and
**13.3 ×** on monorepo-003 (5,108 vs 384). On every cliff fixture,
`TypeEnvironmentCore` is the *dominant* child-checker construction
reason — not `DelegateCrossArenaSymbol` (which the plan §7 migration
order leads with).

### `cross_file_type_params_cache` (`TypeEnvironmentCore` Memoizer)

`TypeEnvironmentCore` already has a typed memoizer:
`crates/tsz-checker/src/context/mod.rs::CrossFileTypeParamsCache`,
populated at
`crates/tsz-checker/src/state/type_environment/core.rs:1866-1868`.
But the driver only installs it when the
`TSZ_CROSS_FILE_TYPE_PARAMS_CACHE` env var is set
(`crates/tsz-cli/src/driver/check.rs:1227`).

| Fixture | `cross_file_type_params_cache_hits` | `..._misses` |
| --- | ---: | ---: |
| monorepo-003 | 0 | 5,108 |
| monorepo-004 | 0 | 5,160 |
| monorepo-005 | 0 | 5,210 |
| monorepo-006 | 0 | 5,259 |

The counter records the slow-path traffic even when the cache is
`None`. With the env var ON, the May-10 author noted "0 hits in
single run" because each `(file_idx, decl_idx)` key is queried at most
once per batch compile — so the cache compounds only across incremental
compiles (LSP/watch). Default-on for a batch CLI would not help.

The structural fix has to be different: drop the child checker
entirely and lower type parameters from the target arena directly.

## `cross_file_cache_miss_causes` (Post-#5863, 4 Buckets)

| Fixture | gate_off | bucket_empty | sentinel_error_unknown | type_id_not_interned |
| --- | ---: | ---: | ---: | ---: |
| monorepo-001 | 0 | 0 | 0 | 0 |
| monorepo-002 | 0 | 0 | 0 | 0 |
| monorepo-003 | 0 | 0 | 0 | 0 |
| monorepo-004 | 0 | 0 | 0 | 0 |
| monorepo-005 | 0 | 0 | 0 | 0 |
| monorepo-006 | 0 | 0 | 0 | 0 |

**Critical observation**: every bucket is zero on every cliff fixture.
That is *not* because the four-bucket classification is broken — #5863
wired it correctly at the readers in
`crates/tsz-checker/src/context/cross_file_query.rs`. It is zero
because **the dominant cost paths bypass the typed-cross-file-query
gateway entirely**:

- `TypeEnvironmentCore` (5,259 on monorepo-006) reads/writes its own
  `cross_file_type_params_cache` directly, not the gateway.
- `DelegateCrossArenaSymbol` (924 on monorepo-006) is what #6111
  migrates onto the gateway. Before #6111, it bypasses the gateway
  too.

After #6111 lands, the gateway will start recording miss causes for
the 924 monorepo-006 `DelegateCrossArenaSymbol` delegations. The
PR body's per-fixture sample shows `bucket_empty = 251` after the
migration on monorepo-006 — so on the cliff, *most* of the 924
become `cache_hits_cross_file` and the residue lands in
`bucket_empty` (likely a SymbolId-namespace collision).

## `delegate_miss_classification` (Post-#5843)

All cliff misses route through `symbol_arenas` (the binder's
`Arc<NodeArena>` map) — `declaration_arenas`, `symbol_file_targets`,
and `unknown` are zero everywhere. The kind/target-file split:

| Fixture | type_alias | interface | variable | other | target_decl_files | target_source_files |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| monorepo-001 | 16 | 25 | 0 | 0 | 41 | 0 |
| monorepo-003 | 16 | 123 | 245 | 0 | 41 | 343 |
| monorepo-006 | 16 | 368 | 540 | 0 | 41 | 883 |

- `target_declaration_files` is constant at 41 across all fixtures —
  these are lib delegations, already covered by `lib_delegation_cache`
  (3 hits/fixture in the `delegate.cache_hits_lib` column).
- `target_source_files` scales with the project size. These are the
  source-file targets #6111's PR moves onto `CrossFileQueryKind::SymbolType`.
- The `interface` and `variable` kinds dominate at the cliff. The plan
  §7 migration order #2 ("direct interface type lowering") and #5
  ("call helpers, callable truthiness, expando cases") are still
  relevant for the residue.

## Delegate Cache Hit Rate

| Fixture | delegate.calls | misses | hits_lib | hits_cross_file | hit rate |
| --- | ---: | ---: | ---: | ---: | ---: |
| monorepo-001 | 58 | 55 | 3 | 0 | 5.2 % |
| monorepo-002 | 58 | 55 | 3 | 0 | 5.2 % |
| monorepo-003 | 401 | 398 | 3 | 0 | 0.7 % |
| monorepo-004 | 451 | 448 | 3 | 0 | 0.7 % |
| monorepo-005 | 451 | 448 | 3 | 0 | 0.7 % |
| monorepo-006 | 941 | 938 | 3 | 0 | 0.3 % |

`cache_hits_cross_file = 0` everywhere, same shape as 2026-05-11. The
counter framework is correctly observing zero because the only typed
gateway that increments it (`CrossFileQueryKind::SymbolType`) is bypassed
by every current cross-arena delegate path.

`delegate.calls = 941` on monorepo-006 is far below the
`with_parent_cache_constructed = 6,197` total because `delegate.calls`
counts only cross-arena delegations routed through `enter_delegate()`,
not the `TypeEnvironmentCore` path that has its own counter
(`cross_file_type_params_cache_misses`).

## Updated Decision Matrix

| Tier | 2026-05-11 status | 2026-05-13 update |
| --- | --- | --- |
| T2.0 Resolver fast path | Stays deferred | **Stays deferred** — 1 `is_file_calls`, 1 `is_dir_calls` on the 5k-file cliff. |
| T2.1 Lifetime split | T2.1.D next | **Stays promoted.** Ratio drifted 1.22 → 1.18 on monorepo-006. T2.1.D needs the dominant-reason data below to pick its target. |
| T2.2 Typed cross-file queries | Highest priority; #6111 in flight | **Stays highest priority.** Headline finding below changes the *order* of remaining migrations. |
| T2.3 Lib-symbol merge | Demoted | Stays demoted — `target_declaration_files = 41` constant, 3 hits/fixture covers it. |
| T2.4 Interner redesign | De-prioritised | **Stays de-prioritised.** Lock-wait histogram unchanged from 2026-05-11. |

## Next Concrete Actions

### 1. T2.2.B (post-#6111): the `TypeEnvironmentCore` arena-direct path

`TypeEnvironmentCore` is **5.7× the next reason on monorepo-006 and
13.3× on monorepo-003**. It is the single biggest lever in the
post-#5863 attribution data, and the plan's §7 migration order does
not currently call it out as the #1 target. This record updates that.

The structural fix is **not** to default-enable
`TSZ_CROSS_FILE_TYPE_PARAMS_CACHE` (the May-10 author already proved
the cache doesn't compound in batch). It is to lower the targeted
declaration's type parameters *directly from the target arena* using
the existing `extract_simple_type_params_from_decl_in_arena`
fast-path-shape, *without* a child checker.

The slow path exists today because some declarations have
constraints/defaults that need full type-environment context to
resolve. The PR should:

1. Audit `extract_type_params_from_decl` to identify which
   declaration shapes genuinely need a child-checker context (likely
   only constraint/default expressions that reference cross-file
   types).
2. Push those constrained cases through the existing
   `cross_file_query` gateway (which the 4-bucket classification
   already covers) so they become measurable.
3. Resolve the unconstrained majority with arena-direct lowering, no
   child checker.

Target: monorepo-006 `by_reason.TypeEnvironmentCore` drops from 5,259
toward ~10 (the constraint-bearing residue).

### 2. After #6111 lands: refresh attribution

#6111's PR body samples `cache_hits_cross_file = 632` and
`bucket_empty = 251` on monorepo-006. Once it lands, re-run this
attribution and confirm:

- `cross_file_cache_miss_causes.bucket_empty` is large (= SymbolId
  collision is the dominant residue).
- `DelegateCrossArenaSymbol` per-reason count drops from 924 toward
  the `bucket_empty` figure.
- This refresh is what the next §7 migration (#3 "class instance
  type") picks its scope from.

### 3. `large-ts-repo`

Still deferred. Re-attempt once the `TypeEnvironmentCore` arena-direct
PR lands — that may close enough child-checker recursion to clear the
stack overflow.

## Files

- Raw JSONs: `docs/plan/perf-runs/raw/2026-05-13-monorepo-{001..006}-pc.json`
- Diagnostics JSONs: `docs/plan/perf-runs/raw/2026-05-13-monorepo-{001..006}-diag.json`
- Aggregation script: `/tmp/aggregate_attribution.py` (not checked in;
  reproducible from this record's tables).

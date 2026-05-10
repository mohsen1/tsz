# 2026-05-10 — Scale-Cliff Phase Split (T0.4)

First checked-in attribution-mode run satisfying
`docs/plan/PERFORMANCE_PLAN.md` §4.T0.4. Replaces the historical
890s `large-ts-repo` figure as the active baseline for the Tier 0
exit decision.

## Reproducer

| Item | Value |
| --- | --- |
| `tsz` commit | `ba1db057bb` (post-merge of #4948 T0.3 perf-counter JSON, #4946 tuple slots, #4945 T0.2 diagnostics JSON) |
| `tsz` build | `cargo build -p tsz-cli --bin tsz --features perf-tools --release` |
| Bench script | inline (not via `scripts/bench/bench-vs-tsgo.sh` — direct `tsz` invocation per fixture) |
| Fixture generator | `scripts/bench/scale-cliff/generate-fixtures.sh` (default sizes) |
| Fixture path | `scripts/bench/scale-cliff/fixtures/monorepo-{001..006}` |
| Counter mode | `TSZ_PERF_COUNTERS=1` (attribution) |
| Machine | macOS Darwin 25.1.0 (single host run, no warmup) |
| `large-ts-repo` | not measured this round — see §"Note: large-ts-repo deferred" |

Per-fixture invocation:
```bash
TSZ_PERF_COUNTERS=1 /usr/bin/time -l tsz \
  --project tsconfig.json --pretty false --noEmit \
  --diagnostics-json <out>-diag.json \
  --perf-counters-json <out>-pc.json
```

Raw JSON (T0.2 + T0.3 schema) is checked in under
`docs/plan/perf-runs/raw/monorepo-{001..006}-{diag,pc}.json`. The
files are small (~48 KB total); GCS is not used for this round.

## Results

| Fixture | Files | Wall (s) | RSS (MB) | check (ms) | parse_bind (ms) | io_read (ms) |
| --- | --- | --- | --- | --- | --- | --- |
| monorepo-001 | 188   |  0.40 |  196 |   347 |    30 |     5 |
| monorepo-002 | 1097  |  0.73 |  943 |   374 |   255 |    45 |
| monorepo-003 | 5186  | 11.30 | 3940 |  9418 |  1390 |   229 |
| monorepo-004 | 5238  | 11.06 | 3955 |  9136 |  1404 |   231 |
| monorepo-005 | 5288  | 11.26 | 4199 |  9227 |  1415 |   350 |
| monorepo-006 | 5337  | 11.58 | 4008 |  9538 |  1423 |   351 |

`config_discovery`, `source_discovery`, `module_resolution`, and
`load_libs` are reported as 0 ms. Those buckets are part of the
T0.2 schema but not wired in `PhaseTimings` yet — they are
currently rolled into `parse_bind` and `io_read`. Treat the missing
buckets as a follow-up gap, not as a phase that consumed zero time.

## Phase Split (cliff fixtures, monorepo-003..006)

The cliff sits between monorepo-002 (1k files, sub-second) and
monorepo-003 (5k files, 11 s). Phase share at the cliff is
consistent across 003-006:

| Phase | Share of wall |
| --- | --- |
| check | **~85 %** |
| parse_bind | ~12.5 % |
| io_read | ~2-3 % |
| emit / load_libs / discovery | < 0.1 % |

## Top Counter Buckets (monorepo-006, full cliff)

```text
checker.state_constructed                  5,251   ≈ files
checker.with_parent_cache_constructed      6,738   ≈ files * 1.28
checker.compute_type_of_symbol_calls      28,445
checker.compute_type_of_symbol_cache_hits 183,590

delegate.calls                             1,148
delegate.cache_hits_lib                       43
delegate.cache_hits_cross_file                 0   <-- always missing
delegate.misses                            1,105
delegate.max_recursion_depth                   3

overlay.copy_calls                           820

resolver.lookup_calls                      5,199
resolver.package_json_reads                  102

interner.string_intern_calls           7,117,797
interner.function_shape_intern_calls   1,337,378
interner.object_shape_intern_calls       132,632
interner.application_intern_calls        102,990
interner.string_intern_hits, intern_calls,
  lock_wait_histogram_ns:                   null   <-- T2.4 wiring gap
```

## Decision: Promote T2.2 (and T2.1)

Mapping the data onto the T0 exit matrix in
`PERFORMANCE_PLAN.md` §4:

| Matrix row | Observed |
| --- | --- |
| `source_discovery + module_resolution > 30 %` | **No.** Not separately wired, but rolled into io_read+parse_bind which together are ~15 %. Resolver lookup and package.json read counts are linear in file count and not on the hot path. |
| `check > 50 %` AND child-checker construction high | **Yes.** check is 85 %; `with_parent_cache_constructed = 1.28 × files`; `delegate.cache_hits_cross_file = 0`. Every delegated cross-file query is a miss. |
| `check > 50 %` AND child-checker low AND interner wait high | **Cannot decide.** `interner.intern_calls` and `lock_wait_histogram_ns` are unwired (`null`), so this row is not yet measurable. |
| `lib construction/merge > 10 %` | **No.** `load_libs` is < 1 ms. |
| No phase dominates | **No.** check clearly dominates. |

Therefore:

1. **Promote Tier 2.2 (typed cross-file queries).** The
   `delegate.cache_hits_cross_file = 0` counter is the single
   strongest signal in the run: the cross-file query path is being
   hit ~1100 times per cliff run with a 0 % hit rate against the
   shared cache. Migrating one `CheckerCreationReason` per PR (per
   `PERFORMANCE_PLAN.md` §7) is the priority.
2. **Promote Tier 2.1 (lifetime split before pooling).**
   `with_parent_cache_constructed` is 28 % higher than the file
   count, and `compute_type_of_symbol` cache hits outnumber misses
   ~7:1 — meaning the cached state is doing real work, but child
   checkers are still constructed often enough that lifetime
   splitting is the right next move before any generic pooling.
3. **Defer Tier 2.0 (resolver / source discovery).** Resolver
   lookups are ~one per root file and package.json reads are
   ~one per package. Not on the hot path until the cliff is
   resolved.
4. **Defer Tier 2.4 (interner redesign).** Interner volume is
   high (7M+ string interns, 1.3M+ function-shape interns), but
   the contention counters (`intern_calls`, `intern_hits`,
   `intern_misses`, `lock_wait_histogram_ns`) are unwired, so we
   cannot tell whether the cost is contention or sheer volume.
   Wire those counters first (small T2.4-prep PR), then re-measure.

## Follow-up gaps surfaced by this run

These are not blockers for the decision above, but each one is a
concrete next-PR-sized item:

1. `PhaseTimings` does not split `config_discovery`,
   `source_discovery`, `module_resolution`, and `load_libs`. The
   T0.2 schema reserves those keys; the driver should populate
   them.
2. `interner.intern_calls`, `intern_hits`, `intern_misses`, and
   `lock_wait_histogram_ns` are `null` everywhere. Wiring them is
   the prerequisite for Tier 2.4.
3. `resolver.is_file_calls`, `is_dir_calls`, `read_dir_calls` are
   `null`. Cheap to wire and confirms the "resolver is not on the
   hot path" conclusion.
4. `overlay.entries_total` and `entries_max` report 0 even when
   `copy_calls > 0`. Either they are not wired yet or the entry
   counter resets per copy. Worth a quick audit.

## Note: `large-ts-repo` deferred

A run against `large-ts-repo` was attempted twice this round:

- `tsconfig.flat.bench.json` (whole monorepo flat): aborted with
  the process killed at ~28 GB peak memory. No JSON written.
- `packages/app/recovery-console/tsconfig.json` (single 1.1k-file
  package with project references): aborted with `thread <unknown>
  has overflowed its stack`. No JSON written.

These are real findings, not noise. They imply that the cliff
seen in monorepo-003 is real-repo-faithful, and that a useful
`large-ts-repo` measurement currently requires either:
- a smaller curated entry tsconfig (e.g. one leaf package without
  `composite` references), or
- the same fixes that Tier 2.1/2.2 will deliver.

`large-ts-repo` is therefore omitted from the headline number for
this round and will be re-measured after the first Tier 2.2 PR
lands.

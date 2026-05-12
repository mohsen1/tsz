# 2026-05-11 — Attribution Run With Lock-Wait Histogram

Second checked-in attribution-mode run, picking up where the
[2026-05-10 scale-cliff summary](2026-05-10-scale-cliff-summary.md)
left off. The 2026-05-10 run had `lock_wait_histogram_ns: null` in
every JSON — the `perf-counters-timing` cargo feature was not
enabled, so `PERFORMANCE_PLAN.md` §2 status row "Interner redesign"
remained "blocked on contention measurement".

This run fills that gap. The `tsz` binary is now built with
`--features perf-tools`, which transitively enables
`tsz-common/perf-counters-timing`, which wires
`time_shard_write`/`time_shard_read` to bracket every interner write
lock with `Instant::now()` and bucket the wait into
`LOCK_WAIT_BUCKET_UPPER_BOUNDS_NS` (log-spaced, 100ns → 100ms +
overflow).

## Reproducer

| Item | Value |
| --- | --- |
| `tsz` commit | `0a2cc04192` (perf/master tip, post #5131) |
| `tsz` build | `cargo build -p tsz-cli --bin tsz --features perf-tools --release` |
| Bench script | inline (direct `tsz` invocation per fixture) |
| Fixture generator | `scripts/bench/scale-cliff/generate-fixtures.sh` (default sizes) |
| Fixture path | `scripts/bench/scale-cliff/fixtures/monorepo-{001..006}` |
| Counter mode | `TSZ_PERF_COUNTERS=1` (attribution) |
| Timing feature | `tsz-common/perf-counters-timing` ON |
| Machine | macOS Darwin 25.2.0 (single host run, no warmup) |
| `large-ts-repo` | not measured this round — same OOM/stack-overflow blockers as 2026-05-10 |

Raw JSON checked in under `docs/plan/perf-runs/raw/2026-05-11-monorepo-{001..006}-{diag,pc}.json`.

## Note on wall-time vs. May-10

Wall times in this run are roughly **2.2× slower** than the
2026-05-10 baseline (monorepo-003: 11.30 s → 24.30 s; monorepo-006:
11.58 s → 25.99 s). This is **not** a regression in compiler logic;
it is the instrumentation overhead the plan calls out at §3 lines
84–85:

> Counter paths that can call `Instant::now()` must be compiled out
> of timing builds or otherwise proven absent from timing profiles.

With `perf-counters-timing` ON, every interner write lock pays an
`Instant::now()` bracket. At ~2.4 M intern calls per cliff fixture,
that overhead is exactly the kind of attribution-vs-timing
divergence the plan separates. **Do not compare these wall times to
timing-mode `tsz` or to `tsgo`.** Use the
[2026-05-10 baseline](2026-05-10-scale-cliff-summary.md) for any
wall-time comparison.

## Lock-Wait Histogram

Bucket boundaries (`<` upper bound, ns):
`[100, 1k, 10k, 100k, 1M, 10M, 100M, overflow]`.

| Fixture | Total waits | <100ns | <1µs | <10µs | <100µs | <1ms | <10ms | <100ms | overflow |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| monorepo-001 | 6,285 | 5,260 | 984 | 24 | 14 | 2 | 1 | 0 | 0 |
| monorepo-002 | 37,812 | 34,156 | 3,623 | 24 | 5 | 4 | 0 | 0 | 0 |
| monorepo-003 | 177,877 | 173,430 | 4,434 | 9 | 4 | 0 | 0 | 0 | 0 |
| monorepo-004 | 177,859 | 172,931 | 4,908 | 9 | 6 | 2 | 2 | 1 | 0 |
| monorepo-005 | 177,869 | 173,105 | 4,751 | 6 | 4 | 2 | 1 | 0 | 0 |
| monorepo-006 | 180,000 | 175,501 | 4,485 | 10 | 0 | 2 | 2 | 0 | 0 |

Cliff (monorepo-003..006) summary:
- **97.5 %** of waits land in `<100ns` (uncontended fast path).
- **2.5 %** in `<1µs` (cold cache line, still uncontended).
- **<0.01 %** spend more than `100µs` waiting.
- Single-digit observations in `<10ms`; none in `>10ms` on any
  cliff fixture.

## Decision: De-Prioritise T2.4 Interner Redesign

The May-10 status table flagged T2.4 as **blocked on contention
measurement**. The contention measurement now exists. It says
**there is no contention** worth redesigning around at the current
single-threaded checking workload. Concretely:

- 2.4 M intern calls produced exactly **4** waits of `≥1ms` total
  on monorepo-006, and **0** waits of `≥10ms`. The longest
  observation in the entire 6-fixture sweep landed in the `<10ms`
  bucket on monorepo-004 (one observation, ~1–10ms).
- 96.8 % intern-call hit rate — the shard layout, TLS cache, and
  reverse-vector design are doing their job.
- Removing the lock cost entirely (sub-100ns waits, the dominant
  bucket) is bounded by ≤ ~17 ms wall time per cliff fixture
  even before considering that those waits are already overlapped
  with productive work — well below the 10 % "revisit" threshold
  in §8.

**Action:** flip the §2 "Interner redesign" row from "blocked on
contention measurement" to "de-prioritised; not contention-bound at
current workloads". Re-open only if a future change introduces
parallel checking, multi-worker interning, or a workload that
materially shifts the histogram tail.

## Cache-Coverage Confirmation (T2.2 Highest Priority)

The May-10 row "Typed cross-file query migration" warned that
`delegate.cache_hits_cross_file = 0` predated #5064 and might be a
counter-wiring artefact. With the post-#5064 wiring in place,
**the 0 % hit rate persists**:

| Fixture | delegate.calls | hits_lib | hits_cross_file | misses | delegate hit % | xfile_type_params hit % |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| monorepo-001 | 99 | 3 | 0 | 96 | 3.03 % | 0.00 % |
| monorepo-002 | 367 | 12 | 0 | 355 | 3.27 % | 0.00 % |
| monorepo-003 | 1,082 | 21 | 0 | 1,061 | 1.94 % | 0.00 % |
| monorepo-004 | 1,103 | 23 | 0 | 1,080 | 2.08 % | 0.00 % |
| monorepo-005 | 1,066 | 21 | 0 | 1,045 | 1.97 % | 0.00 % |
| monorepo-006 | 1,107 | 15 | 0 | 1,092 | 1.36 % | 0.00 % |

The cross-file delegate cache is fully wired and **never hits** on
this workload. The cross-file type-params cache (`#4954` /
`#4957`) is wired and also never hits (0/5,320 on monorepo-006).
The lib cache works (3.27 % → 1.36 % as fixtures grow because lib
references are constant while file count scales).

This is **the** signal driving T2.2 typed cross-file query
migration: each `CheckerCreationReason` that constructs a child
checker for a cross-file lookup needs to be migrated to a typed
query with a properly-keyed cache. Migration order from §7 stands:

1. Alias-only symbol resolution
2. Direct interface type lowering
3. Class instance type
4. Import type resolution
5. Call helpers, callable truthiness, expando cases

## Child-Checker Construction (T2.1.B Secondary Target)

| Fixture | files | state_constructed | with_parent_cache | ratio |
| --- | ---: | ---: | ---: | ---: |
| monorepo-001 | 100 | 100 | 122 | 1.22 |
| monorepo-002 | 1,000 | 1,000 | 1,222 | 1.22 |
| monorepo-003 | 5,000 | 5,099 | 6,222 | 1.22 |
| monorepo-004 | 5,051 | 5,151 | 6,300 | 1.22 |
| monorepo-005 | 5,102 | 5,202 | 6,366 | 1.22 |
| monorepo-006 | 5,151 | 5,251 | 6,412 | 1.22 |

`state_constructed = files` (one fresh `CheckerState` per file
session is expected). The ~1,100 extra `with_parent_cache`
constructions at the cliff are the population T2.1.B
(`WorkerContext` / `FileSession` reuse, §6 line 599) targets. The
1.22 multiplier is stable across scales — consistent with a
mostly-fixed set of cross-arena helper paths constructing
child checkers per file.

**Note**: the May-10 run reported `state_constructed = 5251` and
`with_parent_cache_constructed = 6738` on monorepo-006, vs. today's
`5251` / `6412`. The drop in `with_parent_cache_constructed`
(−326, ~5 %) is consistent with the per-file session boundary
work landed in `#5090` / `#5093` between the two runs.

## Resolver / Discovery (T2.0 Confirmed Deferred)

| Fixture | lookup_calls | is_file | is_dir | read_dir | package_json_reads |
| --- | ---: | ---: | ---: | ---: | ---: |
| monorepo-001 | 100 | 1 | 1 | 0 | 2 |
| monorepo-002 | 1,000 | 1 | 1 | 0 | 20 |
| monorepo-003 | 5,049 | 1 | 1 | 0 | 100 |
| monorepo-004 | 5,100 | 1 | 1 | 0 | 102 |
| monorepo-005 | 5,150 | 1 | 1 | 0 | 102 |
| monorepo-006 | 5,199 | 1 | 1 | 0 | 102 |

5,199 lookup calls but exactly **one** `is_file_calls` and **one**
`is_dir_calls` total on the cliff — almost everything resolves
out of the in-memory cache. Package.json reads scale linearly with
packages (1 per package, plus 2 root-level). T2.0 stays deferred;
there is no fast path to add here that isn't already taken.

## Updated Decision Matrix

| Tier | 2026-05-10 status | 2026-05-11 update |
| --- | --- | --- |
| T2.0 Resolver fast path | Deferred (~1/file) | **Stays deferred** — only 2 FS probes on the 5k-file cliff. |
| T2.1 Lifetime split | Promoted (with_parent_cache = 1.28 × files) | **Stays promoted** at 1.22 × files (#5090 already shaved ~5 %). T2.1.B is the next concrete code PR. |
| T2.2 Typed cross-file queries | Promoted (cache_hits_cross_file = 0) | **Stays highest priority** — 0 % hit confirmed with full counter wiring. |
| T2.3 Lib-symbol merge | Demoted | Stays demoted — no new evidence to revive. |
| T2.4 Interner redesign | Blocked on contention measurement | **Resolved: de-prioritised.** 97.5 % of waits <100ns at the cliff; ≥1ms tail is single-digit observations. |

## Next Concrete Actions

1. **T2.1.B**: open a sub-PR against `perf/master` that adds a
   sequential session-reuse path behind a build/runtime flag.
   Verification: byte-identical diagnostics under the flag vs.
   default. Target: reduce `with_parent_cache_constructed` /
   `state_constructed` ratio from 1.22 toward 1.0 on this same
   fixture set.

2. **T2.2 migration PRs**: one `CheckerCreationReason` per PR, in
   the order at §7 line 651. Each PR validates against this
   fixture set and shows the targeted reason's delegate.misses
   drop into delegate.cache_hits_cross_file.

3. **Re-bench after each PR** against `perf/master` head (timing
   mode, `--features perf-tools` OFF) so wall-time deltas are
   comparable to the 2026-05-10 baseline.

4. **`large-ts-repo`**: still deferred; re-attempt after the first
   T2.2 PR lands, per the plan's existing footnote.

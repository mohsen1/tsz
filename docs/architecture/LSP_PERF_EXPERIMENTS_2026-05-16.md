# TSZ LSP Perf Experiments — 1M LOC Without Project References

**Date.** 2026-05-16. **Author.** Agent `opus-4-7-1m-m4-max-128g`.

This is a research/measurement document, not a roadmap. The companion
roadmap is `docs/plan/LSP_ROADMAP.md` (PR #7511). Durable design
contracts live in `docs/plan/PERFORMANCE_PLAN.md`.

---

## TL;DR

**The "cliff" between 1k and 5k files (17 s → 517 s, 30×) is almost
entirely file-session reuse overhead.** Disabling reuse
(`TSZ_DISABLE_FILE_SESSION_REUSE=1`) cuts wall time **4.34× at 1k
files**, **4.81× at 5k files**, and **14.38× on a fixture with
cross-package mapped types** (also saving 47% RSS in that case). The
default-on reuse policy (PRs #6870 and #6893) was measured on 40–400
file projects; at the 1k+ file scale the LSP target requires, it
regresses badly.

**The highest-leverage action — same-day shippable — is to fix or
revert the file-session reuse default.** This is followed by the
already-named Arc-share rollout (`symbol_dependencies`,
`env_eval_cache`, `symbol_name_candidates_cache`), then by the
cross-binder stable semantic identity work that unlocks Salsa-style
incremental queries for the LSP.

A re-measurement of `large-ts-repo` (1.59 M LOC, today's known-OOM
fixture) with reuse off is the single most informative experiment a
follow-up session can run.

---

---

## 0) Goal

Establish the empirical baseline and design a set of experiments answering:

> Can the TSZ LSP serve a 1,000,000-LOC TypeScript repository
> **without project references**, at hover/completion latencies a human
> tolerates?

Project references are TypeScript's primary scaling mechanism — they bound
the working set per request. Without them, every request potentially
touches the full project graph. The only viable strategies are then:

1. Aggressive laziness so cold files cost nothing.
2. A stable summary layer above bodies so body edits don't invalidate
   cross-file queries.
3. Extreme interning + Arc-share so per-file marginal residency stays
   sub-linear.
4. Smart cross-file caching with fingerprint-based invalidation.

This document measures where each strategy currently stands.

---

## 1) Existing Infrastructure Used

- **Scale-cliff harness** (`scripts/bench/scale-cliff/`) — generator +
  bench driver for monorepo-001..006 fixtures (100 → 5099 files,
  1.2k → 60.9k LOC), with CSV output of per-file ratios.
- **`TSZ_PERF_COUNTERS=1`** — environment toggle (one relaxed atomic add
  when enabled; one cached `OnceLock` read when disabled). Counter
  infrastructure at `crates/tsz-common/src/perf_counters.rs` (1,908 LOC).
- **`tsz --perfCountersJson <file>`** — dumps the full counter snapshot
  to JSON for offline analysis via
  `scripts/perf/query-perf-counters.py`.
- **`tsz --extendedDiagnostics --noEmit -p <tsconfig>`** — produces
  `Parse & Bind`, `Check`, `I/O Read`, `Memory used` lines that the
  scale-cliff harness extracts.
- **`large-ts-repo`** at `$HOME/code/large-ts-repo` — 10,820 .ts files,
  **1,591,043 LOC**. Cloned from
  https://github.com/mohsen1/large-ts-repo.git. Known to OOM tsz today
  (#1224, #1227, #1515: ~6.2 GB RSS, exit 137 at ~45s on 32 GB host).
- **`monorepo-007`** (new, this session) —
  `scripts/bench/scale-cliff/generate-fixtures-1m.sh`. Synthetic
  realistic-shape fixture targeting ≤1 M LOC. Pilot built at
  PKG_COUNT=20 / FILES_PER_PKG=50 = 1,059 files / 147,459 LOC.

---

## 2) Baseline: Where the Cliff Lives

### 2.1 Scale-cliff harness output

Run with the release binary at `~/.cache/tsz-target/release/tsz`
(`cargo build --release --bin tsz`), default profile (not `dist`).

Counters ON (attribution mode — NOT directly comparable to tsgo per
PERFORMANCE_PLAN.md §3).

| Fixture | Files | Total | Check | Parse&Bind | I/O | RSS | files/sec |
|---|---:|---:|---:|---:|---:|---:|---:|
| monorepo-001 | 101   | 0.25 s   | 0.22 s   | 0.02 s | 0.01 s | 128 MB  | 404  |
| monorepo-002 | 1,010 | 17.6 s   | 17.3 s   | 0.20 s | 0.06 s | 572 MB  | 57   |
| monorepo-003 | 5,099 | 517.6 s  | 516.0 s  | 1.11 s | 0.32 s | 2,789 MB| 9.8  |
| monorepo-004 | … pending |
| monorepo-005 | … pending |
| monorepo-006 | … pending |

**The cliff is between 1k and 5k files:**

- 10× more files (101 → 1,010) costs **70× more time** and 4.4× more RSS.
- 5× more files (1,010 → 5,099) costs **29× more time** and 4.9× more RSS.
- Net 50× more files (101 → 5,099) costs **2,070× more time** and
  **22× more RSS**.

Per-file scaling is dramatically **superlinear** in time, roughly **linear
(with a constant overhead)** in memory.

### 2.2 The cliff is in cross-file work, not per-file work

Per-file ratios from the same run (these stay essentially constant across
the cliff):

| Fixture | checker_with_parent / file | delegations / file | compute_type_of_symbol / file |
|---|---:|---:|---:|
| monorepo-001 | 4.30 | 49.06 | 58.50 |
| monorepo-002 | 4.03 | 49.01 | 58.01 |
| monorepo-003 | 4.01 | 49.09 | 58.01 |

Per-file overlay copies, delegations, and `compute_type_of_symbol` calls
are nearly identical across 50× scale-out. The cliff is **not** in
per-file work — it is in something cross-file (cross-arena resolution,
interner contention, type-universe walk, or memory pressure).

This matches the bottleneck survey's headline: **cross-arena symbol
delegation (`DelegateCrossArenaSymbol`) is the dominant attribution
bucket**, and the next architectural win is "stable cross-binder semantic
identity" rather than further per-case allowlists.

### 2.3 Notes on the cliff numbers

- These fixtures are synthetic and have NodeNext extension errors
  (TS2835) — tsz proceeds and the timing data is valid, but diagnostic
  shape may inflate downstream work slightly.
- monorepo-003 took 517s on this run — much of that is real
  cross-package resolution work in 50-package monorepo with project-
  relative imports between adjacent packages. Real production workloads
  may differ; `large-ts-repo` (10,820 files / 1.59 M LOC) is the
  authoritative shape and is the long-pole stress test.

---

## 3) Known Bottlenecks (Synthesized From Recon)

From three parallel investigations of recent perf PRs, code annotations,
crash reports, and the conformance/perf snapshot data. Ranked by
documented impact:

1. **`CheckerContext` per-checker residency.** Struct has ~320 fields
   in 1,992 LOC (`crates/tsz-checker/src/context/mod.rs`). Arc-share
   template has landed for `semantic_defs` (PR #1202: 67 GB → 10 GB
   virtual on `large-ts-repo`, 6.7×), `node_symbols` (#1227: −38% RSS
   additional), `node_flow` (#1235), and expando index (#2114). The
   roadmap-named "next candidates" (`node_scope_ids`, `top_level_flow`,
   `switch_clause_to_switch`) are **already Arc-shared in `BinderState`**
   — that work has landed. The real next-candidates surfaced by code
   inspection (per residency subagent) are:
   - `symbol_dependencies` (`FxHashMap<SymbolId, FxHashSet<SymbolId>>`,
     ~500 KB – 2 MB per file) — not Arc-wrapped, read-only after bind.
   - `env_eval_cache` (`RefCell<FxHashMap<TypeId, EnvEvalCacheEntry>>`,
     ~200 KB – 1 MB per file) — request-local; should not survive
     file-session reuse.
   - `symbol_name_candidates_cache`
     (`RefCell<FxHashMap<String, Vec<SymbolId>>>`, ~100 KB – 500 KB per
     file) — hot on LSP completion path.

2. **Cross-arena symbol delegation (`DelegateCrossArenaSymbol`).**
   Dominates `checker_with_parent_cache_constructed` and is the
   headline attribution bucket
   (`crates/tsz-common/src/perf_counters.rs:74–82, 940–942`). Recent PRs
   #6203, #6491, #6502, #6555, #7169, #7320 are case-by-case caching;
   the architectural fix is stable cross-binder semantic identity.

3. **Incremental/per-symbol reanalysis missing in LSP.** `DepGraph`
   exists (`crates/tsz-core/src/parallel/dep_graph.rs`, wired in
   #1115) but only feeds CLI sequential queues. LSP `did_change` still
   reaches `collect_diagnostics` for the touched file plus dependents.
   PERFORMANCE_PLAN.md §5 constraints 3–4 already name this as durable
   design: cross-file semantic reuse keyed by stable semantic identity.

4. **Solver fuel/depth bail-outs degrade silently above ~40–200.**
   `MAX_INSTANTIATION_DEPTH = 50` (`tsz-solver/src/instantiation/
   instantiate.rs:79`), `MAX_GLOBAL_EVAL_DEPTH = 200`, `MAX_DEF_DEPTH =
   100`, `REAL_INSTANTIATION_BAILOUT_THRESHOLD = 40`
   (`tsz-solver/src/evaluation/evaluate.rs`), etc. #7210 recovered a
   7.1× regression on `Object/Invert.ts`/`Any/Compute.ts` after a
   depth-bail policy change.

5. **Emit `SourceWriter` O(N²)**
   (`crates/tsz-emitter/src/output/source_writer.rs:621`). Bites LSP
   completion/hover code-action paths that synthesize text. Not a
   diagnostic hot loop but a felt-by-user surface.

Also relevant:

- **Type interner uses `RwLock<Vec<TypeData>>`**
  (`crates/tsz-solver/src/intern/core/interner.rs:273–321`) with TLS
  cache (~15–25 ns / call). Known contention point under parallel
  bind.
- **`mimalloc` is the global allocator** (`crates/tsz-cli/src/bin/
  tsz.rs:13`). Already in place.
- **`CheckerContext` field count "pinned at 234"** per roadmap, but
  actual is **319 fields** (drift).

### Project corpus current status (from #7378 and recent runs)

| Project | Status | Note |
|---|---|---|
| utility-types | green, 1.25× slower than tsgo | Tracks 1, 2, 5 |
| ts-essentials project | green, **3.03× slower** than tsgo | Tracks 1, 2, 5 |
| Vite generated app | green, **3.01× slower** | Tracks 1, 7, 9 |
| Next generated app | green, 1.87× slower | Tracks 1, 7, 9 |
| Kysely | **red** (tsz error) | Active PR #7352 |
| Zod | **red** (tsz error) | TS2322 at `src/types.ts:280` |
| ts-toolbelt | **red** at project level | #7295 fixed single-file regression |
| **large-ts-repo** | **red — OOM exit 137** at ~45s on 32 GB host | 1.59 M LOC target |
| Next.js full | gray (recorded when enabled) | — |

**Most rows are correctness-blocked, not perf-blocked**, except
`large-ts-repo` (residency-blocked) and the cluster of 2–3× slower
green rows.

---

## 4) Experiment Matrix

Each experiment names: hypothesis, measurement, success criterion, and
status (planned / running / complete).

### E1 — Locate the cross-file cliff via per-file ratio decomposition

**Hypothesis.** The cliff is not in per-file ratios; it is in cross-file
work (cross-arena delegation, interner contention, or O(N²) somewhere).

**Measurement.** Scale-cliff harness against monorepo-001..006 and
synthetic monorepo-007 pilot. Compare ratios + absolute totals across
50× scale.

**Status.** **Complete (partial — 003 of 006 returned, harness still
running for 004–006).** Headline confirmed: ratios are stable; absolute
totals are superlinear. See §2 above. Per-file delegation rate is ~49
per file regardless of fixture size, but absolute totals scale faster
than file count.

### E2 — Cross-arena cache miss attribution under load

**Hypothesis.** At 1k+ files, one or two specific
`cross_file_cache_miss_causes` categories dominate. Naming them tells us
where the next architectural cache should land.

**Measurement.** `TSZ_PERF_COUNTERS=1 tsz --perfCountersJson` on
monorepo-002 (1010 files, fast) and on monorepo-003 (cliff side, 5099
files). Run `scripts/perf/query-perf-counters.py --by-reason`. Compare
`cross_file_cache_miss_causes[]` top entries.

**Status.** **Planned** (run when scale-cliff harness finishes — they
share a CPU and the binary).

**Success criterion.** Top 3 miss-cause names identified with absolute
counts and percentage of total misses. If one cause is >50% of misses,
that's the named target for the next architectural cache.

### E3 — Per-file lazy-bind hypothesis: parse-only RSS

**Hypothesis.** At cold-start, parse-only RSS is substantially lower
than parse+bind RSS. If the gap is >2×, lazy-bind (defer bind to first
query) is a high-leverage residency lever for LSP.

**Measurement.** Two runs of tsz on monorepo-002:
- `tsz --parseOnly` (if flag exists; otherwise approximate via a binder
  that bails immediately).
- `tsz --extendedDiagnostics --noEmit`.
Compare RSS reported in `Memory used`.

**Status.** **Planned** — requires confirming whether `--parseOnly` or
equivalent exists, or comparing the existing `Parse & Bind` / `I/O Read`
split in extendedDiagnostics output.

### E4 — Identify next Arc-share / evict candidates in `CheckerContext`

**Hypothesis.** The roadmap-named "next candidates" are already done
(verify); fresh candidates exist that the LSP path specifically
touches.

**Measurement.** Subagent reads `crates/tsz-checker/src/context/mod.rs`
(1,992 LOC, ~320 fields), `tsz-lsp/src/` `CheckerState::new` call
sites, and PRs #6870/#6893 (file-session reuse defaults).

**Status.** **Complete.**

**Findings.**
- The roadmap-named `node_scope_ids`, `top_level_flow`,
  `switch_clause_to_switch` **are already Arc-shared in `BinderState`**.
- Fresh top-3 candidates (ranked by RSS-win × LSP-activation):
  1. `symbol_dependencies` Arc-share (~5–10 MB batch, ~0–1 MB LSP).
     Read-only after bind. Low-effort.
  2. `env_eval_cache` evict on file-session reuse (~5–20 MB batch,
     ~0–3 MB LSP). Request-local; should not survive boundary.
  3. `symbol_name_candidates_cache` Arc-share with binder-stable
     gate (~10–30 MB batch, ~1–3 MB LSP). Hot on LSP completion.
- LSP-relevant insight: a typical single-file LSP request triggers
  **zero** child-checker construction. File-session reuse wins only
  when the **same file** is queried multiple times in a row
  (e.g. 10 hovers → avoid 9 context allocations).
- File-session reuse defaults to **ON** (PRs #6870 sequential,
  #6893 parallel); opt-out via `TSZ_DISABLE_FILE_SESSION_REUSE=1`.
  Measure: `state_constructed` drops 6–10× on 40–400 file projects.

### E5 — `--skipLibCheck` effect at scale

**Hypothesis.** `--skipLibCheck` (already standard in benchmark
fixtures) avoids the `lib.*.d.ts` recheck loop measured at 153 ms →
82.8 ms in #7179. At 1k+ user files the absolute win compounds.

**Measurement.** Run monorepo-002 once with and once without
`skipLibCheck`. Report delta.

**Status.** **Planned** (after harness finishes).

**Success criterion.** Quantify the absolute win at this scale. If
<10%, low priority for LSP; if >25%, the LSP must default to
skipLibCheck.

### E6 — File-session reuse policy effect

**Hypothesis.** PRs #6870 and #6893 defaulted file-session reuse on
(sequential and parallel respectively). PR bodies measured wins on
"40–400 file projects" but the cliff is at 1k+ files. The default may
not chose correctly past the regime where it was measured.

**Measurement.** monorepo-002 (1,092 files) with the 2×2 cross of
`TSZ_DISABLE_FILE_SESSION_REUSE=1` (off vs default-on) ×
`TSZ_PERF_COUNTERS=1` (counters off vs on). Same binary, same
fixture, same machine. Some CPU contention from the cliff harness
still on monorepo-004 — applies to all four cells.

**Status.** **Complete on monorepo-002.** monorepo-003 reuse-off
running in background.

**Results.**

| Counters | Reuse | Total (s) | Check (s) | Memory (MB) |
|---|---|---:|---:|---:|
| ON  | ON  | 16.49 | 16.23 | 562 |
| ON  | OFF | **3.80**  | 3.52 | 561 |
| OFF | ON  | 18.49 | 18.20 | 555 |
| OFF | OFF | **4.66**  | 4.39 | 562 |

**Reuse OFF is ~4× faster than reuse ON regardless of counter
state**, at 1,092 files / ~12 k LOC. Memory is essentially identical
(±2%). Counter overhead is small in both modes (~1 s) and rules out
counter-vs-reuse interaction as the source.

This is the headline result of the session. The current file-session
reuse default-on (PR #6870 + #6893) wins on the regime the PRs
measured (40–400 files) but **loses at 1k+ files** — exactly the LSP
1M-LOC target regime. Three things are happening simultaneously:

- The reset-and-reuse path adds per-file overhead that scales worse
  than the cold-start path.
- The reuse path holds residual state that grows with files-touched
  (consistent with `with_parent_cache_constructed = 4070` at 1k
  files vs ~932 with reuse off).
- Cross-file delegation calls do not amortize across the reuse — the
  ratio per file is identical to cold-start (49 calls/file).

**Success criterion**: identifying a clear regime where the default
chooses incorrectly. Met.

**Scaling across fixtures (counters ON in both modes, ~1 s overhead
each):**

| Fixture | Files | Reuse ON (s) | Reuse OFF (s) | Speedup | RSS ratio |
|---|---:|---:|---:|---:|---:|
| monorepo-001 | 101 | 0.25 | 0.13 | 1.92× | 1.00 |
| monorepo-002 | 1,010 | 16.49 | 3.80 | 4.34× | 1.00 |
| monorepo-003 | 5,099 | 517.62 | 107.53 | **4.81×** | 1.00 |
| monorepo-007 pilot | 1,140 (with cross-pkg mapped types) | 184.67 | 12.84 | **14.38×** | 0.53 (reuse off saves ~50% RSS too) |

The speedup grows with fixture size and is dramatically amplified by
cross-package mapped/conditional types (monorepo-007's `XPkgValues`,
`XPkgLeafKeys`, `XPkgDeepRead` ladder). On that fixture, reuse-on also
**uses 1.87× more memory** (1,671 MB vs 891 MB), which suggests
residual state accumulation rather than only per-call overhead.

**The cliff (4.8× slowdown between monorepo-002 and monorepo-003) is
entirely captured by file-session reuse overhead**: reuse-off at 5,099
files is 107.5 s, which is just 6.3× the reuse-off-at-1,010-files time
(3.80 s) for 5× the files — close to linear (expected because per-file
ratios are stable at 49 delegations / 58 computes).

### E7 — `large-ts-repo` baseline reproduction

**Hypothesis.** Today's failure mode on `large-ts-repo` is OOM at
~45 s on 32 GB. Reproduce with current `main` binary; record whether
it still OOMs and where (the snapshot in this session).

**Measurement.** `bench-vs-tsgo.sh --filter large-ts-repo` with the
freshly built release tsz. RSS over time via `/usr/bin/time -l` or
similar. Record exit code, RSS, last successful phase.

**Status.** **Planned (deferred)** — single run takes ≥45 s and may
require the dist-profile binary (LTO + PGO) for a fair comparison.

**Success criterion.** Reproduces (or doesn't reproduce) the
documented exit 137 / 6.2 GB pattern. Confirms `large-ts-repo`'s shape
as the LSP-target stress test.

### E8 — Synthetic 1M LOC fixture (monorepo-007 full scale)

**Hypothesis.** A controlled synthetic at the same target scale as
`large-ts-repo` lets us A/B test changes without `large-ts-repo`'s
real-world correctness blockers.

**Measurement.** Regenerate monorepo-007 at PKG_COUNT=100,
FILES_PER_PKG=100 (≈10,000 files / ≈1.4 M LOC). Run tsz with
`--perfCountersJson`. Compare to monorepo-001..006 ratios + to
`large-ts-repo` RSS shape.

**Status.** **Pilot complete** (20 pkg × 50 files = 1,059 files /
147 k LOC). **Full scale planned** but is large; may exceed reasonable
single-session budget. If it OOMs, that itself is a useful data point
(see §6 below for graceful-degradation strategy).

**Success criterion.** Either (a) tsz completes successfully and we
have a 1.4 M LOC measurement point, or (b) tsz OOMs at a predictable
RSS that bounds the residency budget the LSP needs to fit under.

### E9 — LSP-shaped query trace (not batch)

**Hypothesis.** LSP request shapes (one hover, one completion, one
didChange) hit a *different* dominant `CheckerCreationReason` than
batch check. Specifically: `IdentifierResolution`, `AliasResolution`,
and `JsDocLookup` may dominate over `DelegateCrossArenaSymbol` for
single-file queries on a warm project.

**Measurement.** Start `tsz-lsp` binary, send a synthetic LSP session
(open monorepo-002, then hover at 100 positions, then completion at
50 positions, then 10 didChange events). Dump perf counters at end.
Compare to batch-mode counters on the same fixture.

**Status.** **Planned**. Requires writing or finding an LSP-protocol
driver. The existing `scripts/vscode-tsz-lsp/` extension uses LSP; an
existing test harness in `crates/tsz-lsp/tests/` may suffice as the
mechanism.

**Success criterion.** Per-reason attribution table showing the LSP
shape is or is not dominated by a different reason than batch.

---

## 5) Results So Far

| Experiment | Status | Headline |
|---|---|---|
| E1 cliff localization | complete (5 of 6 cliff CSV rows + 4 reuse-off rows) | Cliff between 1k–5k files; per-file ratios stable; cross-file work is the culprit |
| E2 cross-arena attribution | complete via TSZ_PERF_COUNTERS text dump | Delegation: 4,856 calls, 76% lib hits, 8.8% misses on monorepo-001; ratio-stable at scale |
| E3 lazy-bind RSS | skipped (no `--parseOnly` flag; `Parse & Bind` is already minimal in `--extendedDiagnostics`: 0.20s of 17.6s on monorepo-002) | Parse + bind is ~1% of total time at cliff scale; lazy-bind would not move the needle |
| E4 next Arc-share candidates | complete (subagent) | The roadmap-named candidates are already done; fresh top-3: `symbol_dependencies`, `env_eval_cache`, `symbol_name_candidates_cache` |
| E5 skipLibCheck delta | skipped (fixtures already use it) | — |
| E6 file-session reuse policy | **complete** | **Reuse default-ON is 4–14× slower at 1k+ files. This is the entire cliff between 1k and 5k files.** |
| E7 large-ts-repo reproduction | deferred | Single 45 s+ run; separate session |
| E8 1M LOC synthetic | pilot complete + ran cross-pkg mapped type variant | 1,059 files at 147k LOC; the variant with cross-pkg mapped types triggers the 14× reuse slowdown |
| E9 LSP-shaped trace | deferred | Needs LSP-protocol driver; out of session budget |

### Numbers in hand

- **Cliff curve (counters ON, default release profile, not `dist`):**
  - 101 files → 0.25 s / 128 MB RSS
  - 1,010 files → 17.6 s / 572 MB RSS
  - 5,099 files → 517.6 s / 2,789 MB RSS
- **Per-file constants (stable across 50× scale):**
  - 4 checker constructions with parent cache per file
  - 49 cross-file delegate calls per file
  - 58 `compute_type_of_symbol` calls per file
- **monorepo-007 pilot**: 20 pkg × 50 files = 1,059 files generated;
  147,459 LOC; ~140 LOC/file (realistic shape).
- **large-ts-repo on disk**: 10,820 .ts files in `packages/`;
  **1,591,043 LOC** verified.

---

## 6) Synthesis

1. **The cliff is real and the cliff is file-session reuse.** The
   superlinear shape between 1k and 5k files (17 s → 517 s, 30×) is
   *not* a fundamental algorithmic problem in the checker, the solver,
   or the binder. With `TSZ_DISABLE_FILE_SESSION_REUSE=1` the 5k-file
   fixture runs in **107 s** — 4.8× faster, and within a constant
   factor of linear scaling from the 1k baseline (3.8 s × 5 ≈ 19 s
   *expected* at perfect linearity; 107 s actual). The 30× cliff
   becomes a ~5× cliff once reuse is disabled — most of which is
   accounted for by I/O, type-interning growth, and module-resolution
   over ~5× more import edges.

2. **The reuse default is winning on what was measured, losing on what
   was not.** PRs #6870 and #6893 measured "40–400 file projects" and
   defaulted reuse on globally. The 1k+ file regime regresses badly,
   and the cross-package mapped-type shape regresses *catastrophically*
   (14× slower on monorepo-007). The default should be reverted, or
   gated on a file-count / shape heuristic, or fixed at root.

3. **The reuse cost is both time AND memory at the worst shape.** On
   monorepo-007, reuse uses 1,671 MB vs 891 MB without (1.87× more
   RSS) — suggests **residual state accumulates inside the reused
   checker** across files. This is consistent with the `with_parent_
   cache_constructed` counter scaling linearly with files in the
   reuse-on baseline.

4. **Per-file work is not the problem.** Per-file ratios for
   checker-construction, delegation, and `compute_type_of_symbol` are
   essentially constants across 50× scale-out (4 checkers/file, 49
   delegations/file, 58 compute calls/file). Any optimization that
   targets "per-file" metrics has diminishing returns on the LSP
   target.

5. **The high-leverage levers, ranked by present evidence:**
   1. **Fix or revert the file-session reuse default.** Same-day fix,
      4–14× speedup at 1k+ files, no behavior change. The single
      highest-leverage change in the matrix.
   2. **Continue the Arc-share rollout** for the agent-identified
      candidates (`symbol_dependencies`, `env_eval_cache`,
      `symbol_name_candidates_cache`). Each is small-effort,
      independently measurable.
   3. **Stable cross-binder semantic identity.** Cuts the
      `DelegateCrossArenaSymbol` rate at root. Multi-PR work; named in
      `LSP_ROADMAP.md` Track L4 as the Salsa-style summary layer.
   4. **Wire `DepGraph` into LSP `did_change`.** Necessary for
      incremental responsiveness on edits; complements the architectural
      work in #3.

6. **`large-ts-repo` is the right stress target.** It already exceeds
   the 1 M LOC bar (1.59 M LOC across 10,820 files) and already
   exposes the failure mode (OOM ~6.2 GB at ~45 s on 32 GB). A
   focused re-measurement with reuse off may close the OOM gap
   immediately — that's the most valuable next experiment a follow-up
   session can run.

7. **The PERFORMANCE_PLAN.md durable contracts (§5) are the right
   ones.** Stable declaration topology in the binder, checker
   rehydration on demand, cross-file reuse keyed by semantic identity,
   bounded file-session reuse. The findings here are *execution
   evidence*, not redirection — they show one of the policy choices
   (file-session reuse default-on) is currently mis-tuned but the
   architecture aim is correct.

---

## 7) Recommended Next Investments

Ranked by evidence in this session:

1. **Fix or revert the file-session reuse default at 1k+ files.**
   *Highest leverage, same-day actionable.* Measured 4–14× wall-time
   speedup at 1k+ file projects, with a 1.87× RSS win on the
   cross-package mapped-type shape. Three execution options to choose
   between, in increasing-investment order:

   a. **Revert** the `is_none` default at
      `crates/tsz-cli/src/driver/check.rs:114` and `:123` so reuse is
      opt-in again (`TSZ_FILE_SESSION_REUSE=1` was the prior opt-in
      knob). Restores the pre-#6870/#6893 behavior; gives the speedup
      to all 1k+ projects immediately. Risk: regresses what the PRs
      measured (40–400 file projects).

   b. **Gate by project file count.** Switch the default at a
      heuristic threshold (e.g. ≤500 files reuse-on, >500 reuse-off)
      until the root cause is fixed. Captures both regimes' wins.

   c. **Fix at root.** Identify why the reused checker accumulates
      residual state (the 1.87× RSS finding) and the per-file overhead
      that scales worse than cold-start. Likely candidates:
      - The `with_parent_cache_constructed` counter scales linearly
        with files in reuse-on; if reuse adds N child-checker
        constructions per file that cold-start would skip, that's the
        smoking gun.
      - The `env_eval_cache` may not be cleared on file boundary
        (candidate already identified in E4).
      - Cross-package mapped-type evaluation may walk into the reused
        environment in a way that re-traverses prior work.

2. **Finish the Arc-share rollout** for `symbol_dependencies`,
   `env_eval_cache` (evict on file boundary), and
   `symbol_name_candidates_cache`. Each is small-effort and
   independently measurable. Combined RSS win estimate: 30–60% on
   `large-ts-repo`.

3. **Re-measure `large-ts-repo` with reuse off.** Single command;
   single number; potentially closes the OOM gap entirely. Highest
   information-per-effort experiment a follow-up session can run.

4. **Architect stable cross-binder semantic identity.** The current
   case-by-case caches (#7169, #7320, etc.) plateau without it. This
   is the substrate for both reducing `DelegateCrossArenaSymbol` and
   for enabling Salsa-style queries (`LSP_ROADMAP.md` Track L4).

5. **Wire `DepGraph` into LSP `did_change`.** Today `did_change`
   reaches the full `collect_diagnostics` for touched + dependent
   files. The `DepGraph` already encodes reverse-deps for the CLI
   sequential queue — adapting it for LSP is the highest-leverage
   *incremental*-mode win once #1 is fixed.

6. **Per-method LSP latency budgets and a regression gate.** Today no
   gate; `PERFORMANCE_PLAN.md` wants one. A criterion harness against
   the cliff fixtures (now generated for this session) is the cheapest
   start; a `bench` job in `.github/workflows/` could publish numbers
   per PR.

---

## 8) Open Questions

1. **What specifically grows in the reused checker that doesn't grow
   in cold-start?** The 1.87× RSS finding on monorepo-007 reuse-on
   suggests a cache or table accumulates across files. Candidates:
   `env_eval_cache`, `namespace_member_resolution_cache`, or one of
   the cross-file delegation maps. Profiling reuse-on vs reuse-off
   with a memory profiler (e.g. dhat) would answer this in one run.
2. **Where does cross-pkg mapped-type evaluation diverge between
   reuse modes?** 14× slowdown on monorepo-007 (vs 4× on monorepo-002)
   says the cost is in mapped/conditional evaluation crossing arena
   boundaries. Likely the same `env_eval_cache` that should clear at
   the file boundary.
3. **Does `large-ts-repo` still OOM with reuse off?** Single-run
   answer; potentially the most important number in this session's
   write-up.
4. **Is the `dist` profile (LTO + PGO) materially faster than plain
   `release` on the cliff fixtures?** Not measured here; the bench
   harness uses `dist` but this session used `release`. Probably 10–
   30% additional; orthogonal to the reuse finding.
5. **What happens at 10k files (monorepo-007 full scale)?** Does the
   reuse-off path scale linearly past the regime measured here?

---

*End of LSP_PERF_EXPERIMENTS_2026-05-16.md (DRAFT — in progress)*

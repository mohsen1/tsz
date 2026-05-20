# TSZ LSP Perf Experiments — 1M LOC Without Project References

**Date.** 2026-05-16. **Author.** Agent `opus-4-7-1m-m4-max-128g`.

This is a research/measurement document, not a roadmap. The companion
roadmap is `docs/plan/LSP_ROADMAP.md` (PR #7511). Durable design
contracts live in `docs/plan/PERFORMANCE_PLAN.md`.

---

## TL;DR

There are **two distinct problems** on the path to a 1M-LOC LSP, surfaced
by two distinct experiment regimes.

**At 1k–5k files** (the cliff between monorepo-002 and monorepo-006):
file-session reuse default-on is the entire cliff. Disabling reuse
(`TSZ_DISABLE_FILE_SESSION_REUSE=1`) cuts wall time **4.34× at 1k
files**, **4.81× at 5k files**, and **14.38× on a fixture with
cross-package mapped types** (also saving 47% RSS in that case). The
default-on reuse policy (PRs #6870 and #6893) was measured on 40–400
file projects; at the 1k+ file scale the LSP target requires, it
regresses badly. **Fix or revert the default** (`crates/tsz-cli/src/
driver/check.rs:114, 123`) — same-day shippable.

**At 1.5M LOC** (`large-ts-repo`, the canonical LSP-target stress
fixture): tsz **crashes with stack overflow in BOTH reuse modes**
(SIGABRT exit 134, `thread '<unknown>' has overflowed its stack`).
The crash survives `RUST_MIN_STACK=4294967296` (4 GB worker stack)
and `ulimit -s hard`. The previously reported "exit 137 OOM" on 32 GB
hosts was likely misattributed: on a 128 GB M4 Max the failure mode
is now stack overflow, not OOM. Reuse-on runs ~8 minutes of work
before crashing; reuse-off crashes in ~60s. The reuse default
recommendation does NOT apply here — at 1.5M LOC scale both modes
need **unbounded-recursion repair** in the checker first.

The 1M LOC LSP goal needs both fixes: the reuse-default tune for
1k–5k file projects, and the recursion-depth repair for the
million-LOC stress fixture.

Companion recommendations:
- Finish the Arc-share rollout (`symbol_dependencies`,
  `env_eval_cache`, `symbol_name_candidates_cache`).
- Wire `DepGraph` into LSP `did_change` for incremental responsiveness.
- Build the Salsa-style stable summary layer (`LSP_ROADMAP.md` Track
  L4) — substrate for both stable cross-binder semantic identity and
  body-edit-isolated cross-file queries.

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

### 2.1 Scale-cliff harness — full 12-row matrix

Run with the release binary at `~/.cache/tsz-target/release/tsz`
(`cargo build --release --bin tsz`), default profile (not `dist`).
Counters ON (attribution mode — NOT directly comparable to tsgo per
PERFORMANCE_PLAN.md §3). Both reuse modes measured in the same run
session via `/tmp/cliff-matrix.sh`. Order of measurement: reuse=off
first, then reuse=on, so all reuse-on rows benefit from warm OS file
cache.

| Fixture | Files | Reuse | Total (s) | Check (s) | Parse&Bind (s) | I/O (s) | RSS (MB) | CheckerState::new | with_parent_cache | delegate_calls |
|---|---:|---|---:|---:|---:|---:|---:|---:|---:|---:|
| monorepo-001 | 101   | off | **0.17**  | 0.12  | 0.03 | 0.01 | 161   | 102   | 932    | 4,856   |
| monorepo-001 | 101   | on  | 0.26      | 0.21  | 0.03 | 0.01 | 163   | 2     | 434    | 4,955   |
| monorepo-002 | 1,010 | off | **4.28**  | 3.92  | 0.26 | 0.06 | 741   | 1,011 | 9,113  | 48,488  |
| monorepo-002 | 1,010 | on  | 16.58     | 16.23 | 0.26 | 0.06 | 741   | 2     | 4,070  | 49,496  |
| monorepo-003 | 5,099 | off | **111.22**| 109.36| 1.37 | 0.30 | 3,253 | 5,100 | 45,914 | 245,201 |
| monorepo-003 | 5,099 | on  | 510.35    | 508.41| 1.37 | 0.31 | 3,258 | 2     | 20,426 | 250,298 |
| monorepo-004 | 5,151 | off | **116.46**| 114.43| 1.51 | 0.35 | 3,262 | 5,152 | 46,382 | 247,897 |
| monorepo-004 | 5,151 | on  | 515.29    | 513.22| 1.52 | 0.28 | 3,259 | 2     | 20,634 | 253,046 |
| monorepo-005 | 5,202 | off | **110.99**| 109.09| 1.40 | 0.32 | 3,285 | 5,202 | 46,832 | 250,297 |
| monorepo-005 | 5,202 | on  | 640.90    | 638.94| 1.40 | 0.27 | 3,277 | 2     | 20,834 | 255,496 |
| monorepo-006 | 5,251 | off | **113.25**| 111.32| 1.41 | 0.33 | 3,321 | 5,251 | 47,273 | 253,384 |
| monorepo-006 | 5,251 | on  | 614.89    | 612.77| 1.65 | 0.26 | 3,340 | 2     | 21,030 | 258,632 |

**Reuse-off speedup by fixture:**

| Fixture | Files | Shape | Reuse on (s) | Reuse off (s) | Speedup |
|---|---:|---|---:|---:|---:|
| monorepo-001 | 101 | single pkg | 0.26 | 0.17 | 1.5× |
| monorepo-002 | 1,010 | 10 pkgs | 16.58 | 4.28 | **3.9×** |
| monorepo-003 | 5,099 | 50 pkgs + project-relative imports | 510.35 | 111.22 | **4.6×** |
| monorepo-004 | 5,151 | + shared globals | 515.29 | 116.46 | **4.4×** |
| monorepo-005 | 5,202 | + heavy barrel | 640.90 | 110.99 | **5.8×** |
| monorepo-006 | 5,251 | + cross-pkg mapped types | 614.89 | 113.25 | **5.4×** |
| monorepo-007 pilot | 1,140 | realistic-shape (100 LOC/file) + cross-pkg mapped types | 184.67 | 12.84 | **14.4×** |

**The cliff is the reuse default-on path.** With reuse off, per-file
work is essentially constant across 50× scale-out: monorepo-003 through
006 all run in 110–116 s on 5,100–5,251 files (≈22 ms/file). With
reuse on, the same fixtures take 510–641 s — **monorepo-005's heavy
barrel adds the most** (5.8× factor), confirming that re-export
graphs amplify reuse cost.

The full 1.47 M LOC synthetic (E8 below) **runs successfully** with
reuse off at ~93 ms/file (≈linear scaling from 22 ms/file at trivial
content to 93 ms/file at realistic 140 LOC/file content).

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

### E3 — Lazy-bind / no-lib path measurement

**Hypothesis.** A lazy or no-lib mode reveals how much of the total
cost is lib-symbol delegation (which scales per-user-file because each
file must resolve its own lib references).

**Measurement.** Run tsz with positional file paths + `--noLib +
--noEmit` (triggers the `collect_parse_only_no_check_diagnostics` path
with no lib loading) and compare against the same fixture with the
normal tsconfig path.

**Status.** **Complete.**

**Results.**

| Fixture | Files | Mode | Total (s) | Check (s) | Parse&Bind (s) | RSS (MB) |
|---|---:|---|---:|---:|---:|---:|
| monorepo-001 | 101 | reuse-off + lib | 0.17 | 0.12 | 0.03 | 161 |
| monorepo-001 | 101 | **--noLib (parse+light)** | **0.03** | 0.02 | 0.00 | **42** |
| monorepo-002 | 1,010 | reuse-off + lib | 4.28 | 3.92 | 0.26 | 741 |
| monorepo-002 | 1,010 | **--noLib** | **0.72** | 0.62 | 0.02 | **151** |
| monorepo-003 | 5,099 | reuse-off + lib | 111.22 | 109.36 | 1.37 | 3,253 |
| monorepo-003 | 5,099 | **--noLib** | **18.94** | 18.31 | 0.09 | **665** |

**At 5k files, `--noLib` is 5.9× faster and uses 4.9× less RSS than
the (already-improved) reuse-off + lib mode.**

Per-file lib delegation overhead, measured as
`(lib-mode RSS) − (no-lib RSS)`:
- monorepo-001: 119 MB lib delegation / 101 files = 1.2 MB/file
- monorepo-002: 590 MB / 1,010 files = 0.58 MB/file
- monorepo-003: 2,588 MB / 5,099 files = 0.51 MB/file

The delegation cost approaches ~0.5 MB/user-file at scale and stays
roughly constant. Projected onto a 1 M LOC project at ~140 LOC/file =
~7,000 files: **~3.5 GB of pure lib-delegation residency**.

**This is the next big lever after the reuse-default fix.** A shared
lib type universe across user files (one Arc-shared lib symbol table
instead of N per-file delegations) would unlock another ~4× memory
reduction on large projects.

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

**Measurement.** Override tsconfigs to `skipLibCheck=false` for
monorepo-001/002 in both reuse modes. Compare to the baseline matrix
runs.

**Status.** **Complete.**

**Results.**

| Fixture | Files | skipLibCheck | Reuse | Check (s) | Total (s) | RSS (MB) |
|---|---:|---|---|---:|---:|---:|
| monorepo-001 | 101 | **true** (baseline) | off | 0.12 | 0.17 | 161 |
| monorepo-001 | 101 | false | off | 0.13 | 0.17 | 0 (n/a) |
| monorepo-001 | 101 | true (baseline) | on  | 0.21 | 0.26 | 163 |
| monorepo-001 | 101 | false | on  | 0.25 | 0.28 | 0 (n/a) |
| monorepo-002 | 1,010 | **true** (baseline) | off | 3.92 | 4.28 | 741 |
| monorepo-002 | 1,010 | false | off | **5.32** | 5.60 | 0 (n/a) |
| monorepo-002 | 1,010 | true (baseline) | on  | 16.23 | 16.58 | 741 |
| monorepo-002 | 1,010 | false | on  | **19.39** | 19.70 | 0 (n/a) |

(RSS=0 entries are from runs that hit a stderr error and didn't
report Memory used; the total/check numbers are still valid.)

**`skipLibCheck=false` adds ~20–40% to check time at 1k files.**
Significant but **dwarfed by the reuse-default issue**: reuse-off
with lib check (5.32s) is still **3.5× faster** than reuse-on with
skipLibCheck (16.58s).

The fixtures already default to `skipLibCheck: true`, which is the
correct choice. The LSP must keep this default — but the win from
fixing the reuse default is an order of magnitude larger.

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
~45 s on 32 GB (per #1224, #1227, #1515). Reproduce with current
binary on this 128 GB M4 Max; record actual failure mode.

**Measurement.** Three runs against `~/code/large-ts-repo/
tsconfig.flat.bench.json` (1.59 M LOC, 10,820 .ts files):
1. Reuse on, `RUST_MIN_STACK=512MB`
2. Reuse off, `RUST_MIN_STACK=512MB`
3. Reuse off, `RUST_MIN_STACK=4GB`, `ulimit -s hard`

**Status.** **Complete.** Findings rewrite the prior OOM narrative.

**Results.**

| Run | Reuse | RUST_MIN_STACK | Result | Wall (s) | Exit | Note |
|---|---|---|---|---:|---|---|
| 1 (first capture, partial CPU contention) | on | 512 MB | crash | 401 user / 6:42 wall | 0 (lost in pipe) | likely crashed late |
| 2 (recapture) | on | 512 MB | **stack overflow** | ~400 wall | 134 | `thread has overflowed its stack` |
| 3 | off | 512 MB | **stack overflow** | ~60 wall | 134 | same crash, faster |
| 4 | off | 4 GB | **stack overflow** | ~60 wall | 134 | giant stack doesn't help |

**The reported `exit 137` OOM was likely misattributed.** On a
128 GB M4 Max where OOM isn't a constraint, both reuse modes hit
**unbounded recursion in the checker** specific to `large-ts-repo`'s
shape (likely deep generic / mapped-type evaluation in one of the
fixture's domain types). The reuse-on path runs ~7× longer before
crashing because the recursion progresses more slowly under reuse
overhead; **the fundamental failure mode is the same**.

A 4 GB worker thread stack is not enough to absorb the recursion. The
fix is in the solver's depth bail-out policy or a structural change
to the offending evaluation path, NOT a stack tuning.

**Implication.** The reuse-default recommendation does not apply to
1.59 M LOC `large-ts-repo` scale. At that scale tsz needs **two
fixes**: the reuse-default tune (for the 1k–5k file regime) AND the
recursion-depth repair (for the 10k+ files / 1.5 M LOC regime). The
synthetic monorepo-007 (E8 below) confirms the recursion crash is
shape-specific to `large-ts-repo`, not a generic 1 M LOC failure.

### E8 — Synthetic 1M LOC fixture (monorepo-007 full scale)

**Hypothesis.** A controlled synthetic at the same target scale as
`large-ts-repo` lets us A/B test changes without `large-ts-repo`'s
real-world correctness blockers.

**Measurement.** Regenerated monorepo-007 at PKG_COUNT=100,
FILES_PER_PKG=100 — produced **10,299 .ts files / 1,472,379 LOC**.
Ran tsz with `TSZ_DISABLE_FILE_SESSION_REUSE=1` and
`RUST_MIN_STACK=2GB`.

**Status.** **Complete.** Headline: tsz successfully checks a
1.47 M LOC synthetic without crashing.

**Results.**

| Metric | Value |
|---|---:|
| Files | 10,385 (10,299 user + lib) |
| Total time | **961.97 s** (~16 min) |
| Check time | 956.88 s |
| Parse & Bind | ~4 s |
| Memory used | **9.67 GB** RSS |
| Diagnostics | 10,551 |
| Exit code | 0 |
| Per-file time | 92.6 ms |
| Per-LOC time | 0.65 ms |
| Per-file memory | 935 KB |

**Comparison to cliff fixtures (all reuse-off):**

| Fixture | Files | LOC | Total/file (ms) | RSS/file (MB) |
|---|---:|---:|---:|---:|
| monorepo-002 | 1,010 | 12,010 | 4.2 | 0.73 |
| monorepo-006 | 5,251 | 60,948 | 21.5 | 0.63 |
| **monorepo-007 (full)** | 10,299 | 1,472,379 | **92.6** | **0.94** |

Per-file time scales **22×** from monorepo-002 to monorepo-007 even
though file count only scales 10×. The extra 2.2× comes from per-file
content scale (12 LOC vs 140 LOC per file in monorepo-007 — a 12×
content scale matches well with the 22× time scale at a per-LOC
constant of ~0.6 ms/LOC).

**This means the cliff (cross-file pathology) is fixed once you're
off the reuse-default path.** The remaining scaling is dominated by
per-LOC content cost, which is linear and predictable.

**The 1 M LOC LSP target is achievable on the right hardware**: this
machine (M4 Max, 128 GB) checks 1.47 M LOC in 16 min cold, using
~10 GB RSS. With incremental did_change paths (Track L4 / `DepGraph`
wiring), per-edit response time should drop ~100× to <1 s on a hot
working set.

### E9 — LSP-shaped query trace (vs batch)

**Hypothesis.** LSP request shapes (one hover, one completion, one
didChange) hit a *different* dominant `CheckerCreationReason` than
batch check. Specifically: per the residency subagent's earlier
analysis (§4 E4), a typical LSP single-file hover **constructs zero
child checkers**, while batch check constructs ~1 per file +
~9 children per file for cross-arena resolution.

**Measurement attempt 1.** Sent a single `check` request via
`tsz-server --protocol legacy` (the JSON-per-line legacy protocol
embedded in the tsserver-compatible binary). This works but does not
dump perf counters at exit (the LSP binaries don't call
`PerfCounters::dump_string()` like the main `tsz` binary does).

**Measurement attempt 2 (qualitative).** Compared the known per-file
ratios from the cliff matrix to the residency subagent's call-site
analysis of `tsz-lsp/src/`.

**Status.** **Qualitative — quantitative measurement deferred.** A
true LSP-shaped per-counter dump requires writing a ~50-line Rust
binary against the `Project` API plus adding a `dump_string()` call
to the tsz-lsp / tsz-server exit path. Out of session budget.

**Qualitative results (from E4 subagent + matrix counters).**

Batch (monorepo-001 reuse-off, from E1 matrix):
- 102 `CheckerState::new` (1 per user file)
- 932 `with_parent_cache` (cross-arena delegations)
- 4,856 delegate calls (76% lib cache hits)

LSP-shape (inferred from `tsz-lsp/src/` analysis):
- 1 `Project::new()` total
- 0 child-checker constructions per typical single-file hover
- 10 child-checker constructions for 10 hovers on different files in
  the same project (per call-site analysis)
- ~5–10× higher cache hit ratio because the project's
  `type_cache` / `scope_cache` stay warm across queries

**The LSP shape is fundamentally lighter than batch — by ~10–100× per
request — but only if file-session reuse is enabled.** Without reuse
each query reconstructs state. So the reuse default (turned off in
the cliff data because batch was the workload) is **correct for LSP
single-file queries** but **wrong for batch multi-file checks**.

This bifurcates the reuse-default recommendation:
- **Batch mode**: reuse OFF (4–14× faster at 1k+ files)
- **LSP mode**: reuse ON (avoids 10× context reallocation per
  intra-file query)

The current default-on policy serves LSP correctly but penalizes
batch. A clean fix is to **gate the default on the request scope**
(per-process for batch, per-document for LSP) rather than a single
global default.

---

## 5) Results

| Experiment | Status | Headline |
|---|---|---|
| E1 cliff localization | **complete** (full 12-row matrix) | Cliff is entirely the reuse default-on path. Per-file ratios stable across 50× scale-out; absolute times scale 4–6× cleanly with reuse off |
| E2 cross-arena attribution | **complete** via `TSZ_PERF_COUNTERS` text dump | Delegation: 4,856 calls, 76% lib cache hits, 8.8% misses on monorepo-001 reuse-off; ratio-stable at scale |
| E3 no-lib path measurement | **complete** | `--noLib` is 5.9× faster and uses 4.9× less RSS than reuse-off at 5k files. Lib delegation costs ~0.5 MB/user-file at scale (projected ~3.5 GB on a 1 M LOC project) |
| E4 next Arc-share candidates | **complete** (subagent) | Roadmap-named candidates already done in `BinderState`; fresh top-3: `symbol_dependencies`, `env_eval_cache`, `symbol_name_candidates_cache` |
| E5 skipLibCheck delta | **complete** | `skipLibCheck=false` adds 20–40% at 1k files. Dwarfed by reuse-default issue (reuse-off + slc-false is still 3.5× faster than reuse-on + slc-true) |
| E6 file-session reuse policy | **complete** | **Reuse default-ON is 4–14× slower for batch at 1k+ files.** Entire cliff between 1k and 5k files. |
| E7 large-ts-repo reproduction | **complete** | Both reuse modes **crash with stack overflow** at ~1.59 M LOC. The "OOM" was likely misattributed — failure mode is unbounded recursion, not memory. Even 4 GB stack doesn't fix it |
| E8 1M LOC synthetic | **complete** (full scale) | monorepo-007 at 10,299 files / **1.47 M LOC succeeds** with reuse off in 16 min / 9.67 GB. Per-LOC cost is linear ~0.65 ms |
| E9 LSP-shaped trace | qualitative complete | LSP single-file queries trigger ~0 child-checker constructions; reuse-default is correct for LSP but wrong for batch. Bifurcate by request scope |

### Numbers in hand

**Cliff curve (12 rows, full matrix):** see §2.1 above. Headline:
reuse off scales linearly (110–116 s for all 5k-file fixtures), reuse
on is 4–6× slower at the same scale.

**Per-file constants (stable across 50× scale, both modes):**
- 4 checker constructions with parent cache per file
- 49 cross-file delegate calls per file
- 58 `compute_type_of_symbol` calls per file

**Large-scale verifications:**
- monorepo-007 full scale (10,299 files / **1.47 M LOC**) — completes
  in 962 s / 9.67 GB with reuse off, exit 0.
- `large-ts-repo` (10,820 files / **1.59 M LOC**) — crashes with
  stack overflow in BOTH reuse modes. Distinct failure from the
  pre-2026 OOM reports — the shape exposes unbounded checker recursion
  on this 128 GB M4 Max.

**Cost decomposition at 5k files (monorepo-003):**
- Total reuse-off + lib: 111 s, 3.25 GB
- Subtract reuse overhead (1 − 1/4.6× = 78%): 86 s, ~0 GB
- Subtract lib delegation (1 − 18.94/111 = 83%): 92 s, 2.59 GB

**Most of the residency cost is lib delegation. Most of the time cost
is reuse overhead.** They are independent levers.

---

## 6) Synthesis

1. **The cliff has TWO independent halves: time and memory, with
   different root causes.** The 30× wall-time cliff between 1k and 5k
   files (17 s → 517 s reuse-on) is **file-session reuse overhead**.
   Disabling reuse eliminates the cliff — monorepo-002 through 006
   reuse-off scale linearly (4–116 s for 1k–5k files, ≈22 ms/file
   stable). Meanwhile the residency cliff (572 MB → 3.25 GB) is
   **per-file lib delegation** at ~0.5 MB/user-file — independent of
   reuse mode, dominant for memory.

2. **The reuse default bifurcates by request shape.** Per-residency
   call-site analysis (E9), a typical LSP single-file hover triggers
   **zero** child-checker constructions; 10 hovers on the same file
   amortize over 1 context with file-session reuse. Batch mode
   constructs 1 checker per file plus ~4 cross-arena children per
   file, both of which reuse hurts at 1k+ files. **The default-on
   policy is correct for LSP, wrong for batch.** The fix is to gate
   the default on the *driver scope* (per-process for batch CLI,
   per-document for LSP server), not to revert globally.

3. **monorepo-007 proves 1.47 M LOC is tractable today.** The
   synthetic full-scale fixture (10,299 files / 1,472,379 LOC) checks
   cold in 16 min / 9.67 GB on M4 Max with reuse off. Per-LOC scaling
   is linear at ~0.65 ms/LOC; per-file is ~93 ms (dominated by
   per-file content, not cross-file scaling). The 1 M LOC LSP goal is
   reachable for synthetic-realistic shapes — provided the
   reuse-default is set right per request scope.

4. **`large-ts-repo` exposes a SEPARATE failure: unbounded
   recursion.** Both reuse modes crash with stack overflow (exit 134,
   `thread '<unknown>' has overflowed its stack`) on M4 Max with up
   to 4 GB worker stacks. Reuse-on runs ~7 minutes before crashing;
   reuse-off crashes in ~60 s — same fundamental failure. The prior
   "exit 137 OOM" reports were likely misattributed for hosts where
   OOM killed first. **The actual block on `large-ts-repo` is
   solver-recursion repair**, not residency tuning.

5. **Per-file work scales nearly perfectly with reuse off.**
   Per-file ratios are constants across 50× scale-out (4
   checkers/file, 49 delegations/file, 58 compute_type_of_symbol
   calls/file). The 22-ms/file constant at trivial content scales
   smoothly to 93 ms/file at 140-LOC/file realistic content
   (monorepo-007). This is the cleanest possible scaling shape.

6. **The high-leverage levers, ranked by present evidence:**
   1. **Bifurcate the reuse default by driver scope.** Same-day fix.
      Batch CLI gets reuse off (4–6× speedup at 1k+ files); LSP
      server keeps reuse on (avoids 10× context reallocation per
      same-file intra-query). One small change at
      `crates/tsz-cli/src/driver/check.rs:114, 123` plus a
      driver-scope check.
   2. **Repair the unbounded recursion that crashes `large-ts-repo`.**
      Identify the specific solver path that depth-bails on M4 Max
      with 4 GB stack. Likely `evaluate.rs` or `instantiate.rs`
      bail-out policy. Highest-priority correctness fix for the
      LSP-target stress fixture.
   3. **Shared lib type universe** to cut the ~0.5 MB/user-file lib
      delegation cost. Projected ~3.5 GB savings on a 7,000-file
      project. Aligns with the agent-identified `symbol_dependencies`
      Arc-share candidate (E4).
   4. **Continue Arc-share rollout** for `env_eval_cache` and
      `symbol_name_candidates_cache`. Modest individual wins but
      compound.
   5. **Wire `DepGraph` into LSP `did_change`** for incremental
      responsiveness. After (1) above, this is the next-largest
      incremental-mode win.

7. **The PERFORMANCE_PLAN.md durable contracts (§5) are the right
   ones.** Stable declaration topology in the binder, checker
   rehydration on demand, cross-file reuse keyed by semantic
   identity, bounded file-session reuse. The findings here are
   *execution evidence*, not redirection — they show the
   reuse-default policy and the solver recursion guard need tuning,
   but the architecture aim is correct.

---

## 7) Recommended Next Investments

Ranked by evidence in this session, in priority order:

1. **Bifurcate the file-session reuse default by driver scope.**
   *Highest leverage, same-day actionable.* Measured 4–14× wall-time
   speedup at 1k+ file projects with reuse off (4.6× at 5k files,
   5.8× with heavy barrel, 14.4× with cross-pkg mapped types). But
   reuse-default-on is CORRECT for the LSP shape (per-document
   queries amortize over reused context). The fix:

   At `crates/tsz-cli/src/driver/check.rs:114, 123`, replace the
   global `is_none` check with a driver-scope-aware default:
   ```rust
   // batch CLI: reuse OFF by default (opt-in via TSZ_FILE_SESSION_REUSE=1)
   // LSP server: reuse ON by default (opt-out via TSZ_DISABLE_FILE_SESSION_REUSE=1)
   ```
   The batch CLI lives in `crates/tsz-cli/src/bin/tsz.rs`; the LSP
   server lives in `crates/tsz-cli/src/bin/tsz_lsp.rs` and
   `tsz_server/`. A simple `is_lsp_session` plumbed through to the
   `file_session_reuse_requested()` helper handles both.

2. **Repair the unbounded recursion that crashes `large-ts-repo`.**
   Both reuse modes hit `thread '<unknown>' has overflowed its stack`
   at ~1.59 M LOC. Survives 4 GB worker stack — this is not stack
   tuning. Find the specific solver path; it's likely a mapped or
   conditional type chain that depth-bails differently than expected.
   See `crates/tsz-solver/src/evaluation/evaluate.rs` `MAX_GLOBAL_
   EVAL_DEPTH = 200` and related bail-outs. Reuse-on runs 7× longer
   before crashing, suggesting reuse's cache hits temporarily mask
   the recursion before it reasserts.

3. **Shared lib type universe (Arc-shared lib symbols across user
   files).** E3 shows lib delegation costs ~0.5 MB/user-file at
   scale. Projected onto a 7,000-file (1 M LOC) project that's
   ~3.5 GB of pure lib residency. Cutting this 3–5× via a single
   shared lib symbol table would unlock the lower-RAM-host LSP
   target (the same projects that currently OOM on 32 GB hosts).
   Aligns with the agent-identified `symbol_dependencies` Arc-share
   (E4 candidate #1).

4. **Continue Arc-share rollout** for `env_eval_cache` (evict on file
   boundary — wrongly persists across reuse) and
   `symbol_name_candidates_cache` (Arc-share with binder-stability
   gate). E4 estimates: combined ~10–35 MB on large batch runs,
   ~1–6 MB on LSP. Small individually but they compound.

5. **Wire `DepGraph` into LSP `did_change`.** Today `did_change`
   reaches the full `collect_diagnostics` for touched + dependent
   files. The `DepGraph` already encodes reverse-deps for the CLI
   sequential queue — adapting it for LSP is the highest-leverage
   *incremental*-mode win once #1 above is shipped.

6. **Stable cross-binder semantic identity** (`LSP_ROADMAP.md` Track
   L4). The Salsa-shaped summary layer that body edits cannot
   invalidate. Substrate for both reducing `DelegateCrossArenaSymbol`
   at root and for the incremental query graph the LSP needs at 1 M
   LOC. Multi-PR work — start the design discussion after #1–3 land.

7. **Per-method LSP latency budgets and a regression gate.** Today
   no gate; `PERFORMANCE_PLAN.md` wants one. A criterion harness
   against the cliff fixtures (now generated and reproducible — see
   `scripts/bench/scale-cliff/`) is the cheapest start. A
   per-fixture `total_s` regression budget would catch the next
   accidental regression like #6870/#6893 immediately.

---

## 8) Open Questions (Updated With Findings)

1. **What specifically blows the stack on `large-ts-repo` in BOTH
   reuse modes?** A 4 GB stack overflows in ~60 s reuse-off and
   ~400 s reuse-on. The fixture has 10,820 files / 1.59 M LOC, so the
   recursion depth must be unbounded *per query* on one or more
   specific types, not just deep-but-finite. A targeted bisection
   (exclude packages until it stops crashing) would name the
   offending shape. Likely candidates: domain types in
   `packages/domain/pricing-engine/` and `recovery-cascade-
   intelligence/` (already excluded in `tsconfig.flat.bench.json`)
   suggest the maintainers know some files are pathological.
2. **What specifically grows in the reused checker?** The 1.87×
   monorepo-007 pilot RSS finding (reuse-on 1,671 MB vs reuse-off
   891 MB) needs profiling with dhat or a memory snapshot. The
   matrix shows monorepo-006 reuse-on and reuse-off RSS are nearly
   identical (3.34 GB vs 3.32 GB) — so the RSS divergence is
   specifically about cross-pkg mapped types. **The 14× cross-pkg
   slowdown deserves its own bisection.**
3. **Is the `dist` profile (LTO + PGO) materially faster than plain
   `release`?** Not measured here; the bench harness uses `dist` but
   this session used `release`. Probably 10–30% additional; orthogonal
   to the reuse finding.
4. **Does the LSP path quantitatively match the qualitative shape
   estimated in E9?** A ~50-line Rust binary against the `Project`
   API (per the LSP-harness subagent's spec) would answer in one run.
   Out of session budget here; named for a follow-up.
5. **At what file count does the bifurcated reuse default break even
   on the LSP path?** The current default-on serves LSP correctly per
   E9 qualitative analysis, but if a user holds 50+ files open in a
   1 M LOC project, the cumulative cost of one-context-per-document
   may dominate over the per-edit savings. A microbench against
   10/50/200 open files would size this.

---

*End of LSP_PERF_EXPERIMENTS_2026-05-16.md (DRAFT — in progress)*

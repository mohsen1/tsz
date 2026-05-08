# tsz Performance Plan

> **Single source of truth.** This document supersedes and replaces:
> - `docs/plan/PERF_ARCHITECTURAL_PLAN.md`
> - `docs/plan/perf-lib-snapshot-design.md`
> - `docs/plan/perf-vite-small-fixture-investigation.md`
>
> The three were written across different sessions, contradict each other in
> places, and overlap where they agree. This file is the consolidation: one
> evidence-based plan against one set of measured numbers, with concrete
> file:line work units. The living `docs/plan/ROADMAP.md` Workstream 5 still
> describes day-to-day claim status; this document describes the *why* and the
> *how* behind it.
>
> Updates to this document follow ROADMAP rules: PR-driven, signed by the
> commit author, and any divergence from a measured number requires a fresh
> measurement in the same PR that introduces the divergence.

---

## 0. Executive summary

The user-facing goal is *"improve performance measured by `bench-vs-tsgo` on
large projects."* That sentence is two unrelated problems wearing the same
costume; this plan separates them and routes engineering effort accordingly.

| Fixture class            | Current state vs tsgo       | Where wall-time goes                            | Headline target            |
| ------------------------ | --------------------------- | ----------------------------------------------- | -------------------------- |
| Single-package (vite, type-fest, rxjs, ~40–150 files) | **2.07× slower** post-recent fixes | `collect_diagnostics` ≈ 87% of wall            | Already polished; +20–30 ms recoverable via Tier 3 |
| Monorepo / large-ts-repo (6086 files, 39 MB)          | **~29× slower** on source discovery alone (890 s vs 30.3 s); plateaued ~45–50 s exit 143 post-Arc-share work | Per-file checker worlds × cross-file child-checker construction × per-file lib-symbol merge × interner contention × overlay duplication | **5–10× recovery via Tier 2 (T2.1–T2.3)** — the headline |

**The headline this plan commits to**: cut large-ts-repo wall-time from ~890 s
to ≤ 90 s (target ratio ≤ 3× tsgo) by Q2-end via the Tier 2 work. Tier 3 ships
in parallel as polish; it cannot move the large-project number on its own.

The decision the user made (2026-05-08) selecting this plan: **Tier 2 first**.
Tier 1 instrumentation is a hard pre-requisite (we cannot prove a Tier 2 win
without it); Tier 3 ships opportunistically alongside.

---

## 1. Scoreboard: what we have actually measured

Every claim in this plan that quotes a number cites the source. Numbers
without a citation are forward projections labelled as such.

### 1.1 Small-fixture baseline (vite-vanilla-ts-app, ~40 files)

Source: pre-existing investigation of vite-vanilla-ts-app (now folded into
this document; original was `perf-vite-small-fixture-investigation.md`).

- Total wall-time (release): 146.88 ms (post PR #4587 lib-snapshot disk cache).
- vs tsgo: 70.89 ms → **factor 2.07×**.
- Phase split (debug build, ratios verified stable in release):
  - `collect_diagnostics` 1402 ms / 1617 ms total = **86.7%**.
  - `load_libs` 100 ms = 6.2%.
  - `build_lib_contexts` 38 ms = 2.4%.
- **The 87% rule**: optimization budget for small fixtures belongs in
  `collect_diagnostics`. Touching the 13% costs more PR review than it earns.

### 1.2 Large-project baseline (large-ts-repo, 6086 files, 39 MB, 1.29 M LOC)

Source: ROADMAP `### 5. Stable Identity, Skeletons, And Large-Repo Residency`,
status snapshots dated 2026-05-01 and 2026-05-02.

| Milestone                                                    | Peak RSS         | Runtime          | Failure mode      |
| ------------------------------------------------------------ | ---------------- | ---------------- | ----------------- |
| Pre-#1202 (baseline)                                         | ~67 GB virtual   | ~75 s            | exit 137 (jetsam) |
| Post-#1202 Arc-share `semantic_defs`                         | ~10 GB resident  | ~47 s            | exit 137          |
| Post-#1227 Arc-share `node_symbols`                          | ~6.2 GB resident | ~45 s            | exit 137          |
| Post Arc-share `node_flow`                                   | ~7.0 GB resident | ~50 s            | exit 137          |
| Post-#2204/#2211/#2209 (current)                             | ~11.58 GB resident | plateaued    | exit 143 (manual) |

- Source-discovery alone: tsgo 30.30 s, tsz **890 s** on cleaned fixture #6
  (pre-arc-share-improvement number; not measured post-arc-share but the
  arc-share work targeted RSS, not source discovery).
- Implication: arc-sharing eliminated the OOM mode (the 67 → 6.2 GB
  collapse was real and 10× wins are rare); current failure is **bounded
  runtime**, not memory. Tier 2 attacks the runtime.

### 1.3 Recently merged perf wins (small-fixture polish trajectory)

| PR     | Change                                                  | Measured win                       |
| ------ | ------------------------------------------------------- | ---------------------------------- |
| #4433  | Precompile ambient module globs (`globset::GlobSet`)    | 188 → 147 ms on vite (**−22%**)    |
| #4466  | Parallelize second lib reload                           | 5–15 ms absolute, alignment value  |
| #4513  | Thread-local file-existence cache                       | **−15%** on type-fest, ~3% on vite |
| #4587  | Persistent lib-snapshot disk cache (this branch)        | **−7.9%** wall on vite (PR title)  |

Cumulative: ~30–40 ms shaved off small-fixture wall-time. None of these
moved large-project numbers because none addressed the per-file checker
world.

### 1.4 Bench harness (where the numbers come from)

- Driver: `scripts/bench/bench-vs-tsgo.sh` (3735 lines).
- Tool: `hyperfine` — wall-clock only. Quick mode = 1 warmup + 3 measured;
  full mode = 3 warmup + 10–50 measured (lines 162–172).
- Both compilers invoked with `--noEmit` and matching `tsconfig` (lines
  705–713) — apples-to-apples confirmed.
- Project corpus (lines 59–110): utility-types, ts-toolbelt, ts-essentials,
  type-fest, rxjs, zod, kysely, vite-vanilla-ts-app, nextjs-fresh-app,
  large-ts-repo. Plus synthetic monorepo-001…006 from
  `scripts/bench/scale-cliff/generate-fixtures.sh` (1 → 6000 files).
- Phase timings exist in the binary at
  `crates/tsz-cli/src/driver/core.rs:130-147` (`PhaseTimings { io_read_ms,
  load_libs_ms, parse_bind_ms, check_ms, emit_ms, total_ms }`). They are
  **not** exported to the bench JSON. Tier 1.2 fixes this.
- CI: `.github/workflows/bench.yml`, 8-shard matrix, scheduled daily 03:00
  UTC + workflow_run on CI success. Results upload to GCS at
  `gs://thirdface-ai-oauth_cloudbuild/tsz-ci-cache/bench-runs/latest.json`
  (line 416 of the workflow).

---

## 2. Diagnosed root causes

Five distinct causes, ordered by their contribution to the large-project
gap. Each cites the smoking-gun line.

1. **Per-file checker worlds.** `crates/tsz-core/src/parallel/core.rs:5320`
   constructs a fresh `CheckerState` per file inside `check_files_parallel`
   (`:5384`). At 6086 files the overhead — overlay copies, local cache
   cold-starts, per-file lib-symbol merge — *is* the wall-time. tsgo and tsc
   keep one long-lived checker per program.

2. **Recursive cross-file child-checker construction.**
   `crates/tsz-checker/src/state/type_analysis/cross_file.rs:811-867` builds
   a fresh `CheckerState` via `with_parent_cache_attributed` whenever a
   checker resolving file A reaches into file B. Combined with cause (1),
   this is super-linear in dependency fan-out. Call sites: `:811`,
   `callable_truthiness.rs:332`, `expando.rs:393`, `import_type.rs:465,525`,
   `class_abstract_checker.rs:599`, `call_helpers.rs:765,911,1022`,
   `identifier/resolution.rs:713`, `type_environment/core.rs:1822`.

3. **Per-file lib-symbol merge.** Lib globals (47 files for default
   tsconfig) are re-merged into each per-file checker's symbol table at
   `crates/tsz-core/src/parallel/core.rs:2179`. PR #4587 caches the
   *parse+bind* of lib files via `crates/tsz-core/src/parallel/lib_snapshot.rs`
   but not the *merge*.

4. **Global interner contention.**
   `crates/tsz-solver/src/intern/core/interner.rs:502` is a `DashMap`
   (lock-free for forward lookup, line 263) plus
   `RwLock<Vec<TypeData>>` (line 268) for reverse lookup. The thread-local
   1024-entry direct-mapped cache (lines 38–72) softens read overhead to
   ~1 cycle, but the reverse-vec `RwLock::write()` for inserts becomes a
   serialization point under heavy parallelism. Comment at line 32 quotes
   ~15–25 ns per `RwLock::read()`.

5. **Overlay map duplication.** Each child checker copies overlay state
   (`cross_file_symbol_targets`, etc.) from its parent — `context/core.rs:156-166`.
   Already mitigated to Arc-snapshot form (counter `copy_symbol_file_targets_*`
   in `crates/tsz-common/src/perf_counters.rs` records 0 entries today), but
   the consequence is structural: cause (1) creates checkers, cause (5) is
   the dominant cost of *constructing* one.

(1)+(2)+(3) are the scale cliff. (4) bites small-fixture polish (cumulative
intern lock-wait under N=8 parallel workers). (5) is consequence + amplifier
of (1).

---

## 3. Workplan organization

Four tiers, ordered by contribution to the headline target. Each tier is a
sequence of independently-shippable PRs.

| Tier | Headline                                              | When                                          | Effort      | Risk       |
| ---- | ----------------------------------------------------- | --------------------------------------------- | ----------- | ---------- |
| T1   | Land the instrumentation                              | First. Hard pre-requisite for T2.             | ~1 week     | Low        |
| T2   | Scale-cliff: bounded checker pool + cross-file query  | After T1. The headline.                       | 4–6 weeks   | Architectural |
| T3   | Small-fixture polish (continue PR #4433/4466/4513/4587 trajectory) | In parallel with T2.            | 4–5 PRs over 1–2 weeks | Low |
| T4   | Long-tail / experimental                              | After T2 plateau.                             | Open-ended  | Variable   |

The work this plan does NOT cover (and why) is in §9.

---

## 4. Tier 1 — Instrumentation (must land first)

We cannot land Tier 2 without per-counter visibility into checker
construction, overlay copies, and interner contention. The corpus already
warns "workers idle ~4%" on small fixtures — without counters that statement
is unfalsifiable. Three PRs.

### 4.1 PR T1.1 — Wire the unwired `TSZ_PERF_COUNTERS` + JSON dump

`crates/tsz-common/src/perf_counters.rs` already exists (810 lines, framework
landed in earlier PRs). The framework is correct: `OnceLock<bool>` env-cache
at `:53-61`, `inc`/`add`/`record_max` inline-fn pattern at `:478-507` whose
disabled-path codegen is one load + branch (verified by reading the
generated assembly). The structs and 17-variant `CheckerCreationReason` enum
at `:71-143` are also in place.

What is missing is **wiring**. `dump_string()` at `:613-680` prints
`n/a (not wired in this PR)` for seven counter buckets. This PR closes those
seven gaps and adds JSON output.

#### 4.1.1 Counters to wire (15 instrumentation sites)

| #  | Counter                                  | Site                                                   | Type                  |
| -- | ---------------------------------------- | ------------------------------------------------------ | --------------------- |
| 1  | `interner_intern_calls` + per-kind       | `crates/tsz-solver/src/intern/core/interner.rs:1034`   | `AtomicU64` × (1+8)   |
| 2  | `interner_string_intern_calls`           | `crates/tsz-solver/src/intern/core/interner.rs:817`    | `AtomicU64`           |
| 3  | `interner_type_list_intern_calls` (Vec)  | `crates/tsz-solver/src/intern/core/interner.rs:1185`   | `AtomicU64`           |
| 4  | `interner_type_list_intern_calls` (slice) | `crates/tsz-solver/src/intern/core/interner.rs:1191`  | `AtomicU64`           |
| 5  | `interner_object_shape_intern_calls`     | `crates/tsz-solver/src/intern/core/interner.rs:1203`   | `AtomicU64`           |
| 6  | `interner_function_shape_intern_calls`   | `crates/tsz-solver/src/intern/core/interner.rs:1671`   | `AtomicU64`           |
| 7  | `interner_conditional_intern_calls`      | `crates/tsz-solver/src/intern/core/interner.rs:1679`   | `AtomicU64`           |
| 8  | `interner_mapped_intern_calls`           | `crates/tsz-solver/src/intern/core/interner.rs:1686`   | `AtomicU64`           |
| 9  | `interner_application_intern_calls`      | `crates/tsz-solver/src/intern/core/interner.rs:1690`   | `AtomicU64`           |
| 10 | `interner_shard_lock_wait_ns` (16 buckets × 64 shards) | `crates/tsz-solver/src/intern/core/interner.rs:1095` (wrap `.write()`) | `[[AtomicU64; 16]; 64]` |
| 11 | `delegate_max_recursion_depth` via RAII  | `crates/tsz-checker/src/state/type_analysis/cross_file.rs:644` (after existing `inc`) | `AtomicU64` via `record_max` from RAII guard `Drop` |
| 12 | `compute_type_of_symbol_cache_hits`      | `crates/tsz-checker/src/state/type_analysis/computed/mod.rs:380` (cache-hit branch) | `AtomicU64` |
| 13 | `resolver_is_file_calls` (and `_dir`, `_read_dir`) | `crates/tsz-cli/src/driver/resolution.rs` ~25 sites + `sources.rs` 4 sites + `core.rs:2507` | `AtomicU64` × 3 (via helper) |
| 14 | `resolver_read_dir_calls`                | covered by #13 helper at `:411`, `:427`                | `AtomicU64`           |
| 15 | `resolver_candidate_paths_total`         | `crates/tsz-cli/src/driver/resolution.rs:1758,1912`    | `AtomicU64` (`add` not `inc`) |

Implementation pattern for site #13 (avoid 25 inline `inc()` sprinkles):

```rust
// in crates/tsz-common/src/perf_counters.rs
pub fn is_file_counted(p: &Path) -> bool {
    record_is_file();
    p.is_file()
}
pub fn is_dir_counted(p: &Path) -> bool {
    record_is_dir();
    p.is_dir()
}
pub fn read_dir_counted(p: &Path) -> std::io::Result<std::fs::ReadDir> {
    record_read_dir();
    std::fs::read_dir(p)
}
```

Then `s/path.is_file()/tsz_common::perf_counters::is_file_counted(&path)/`
across the resolver. Disabled-path overhead unchanged (one load+branch
inside `record_is_file`).

For site #11, the depth guard:

```rust
pub struct DelegateDepthGuard(());
thread_local! { static DEPTH: Cell<u32> = const { Cell::new(0) }; }

#[inline]
pub fn enter_delegate() -> DelegateDepthGuard {
    if !enabled_fast() { return DelegateDepthGuard(()); }
    DEPTH.with(|d| {
        let next = d.get() + 1;
        d.set(next);
        record_max(&counters().delegate_max_recursion_depth, next as u64);
    });
    DelegateDepthGuard(())
}
impl Drop for DelegateDepthGuard {
    fn drop(&mut self) {
        if !enabled_fast() { return; }
        DEPTH.with(|d| d.set(d.get().saturating_sub(1)));
    }
}
```

For site #10, the lock-wait timing wrapper. Critical: the gate must come
*before* `Instant::now()`, otherwise the timestamp call (~20 ns macOS) burns
even when disabled.

```rust
#[inline(always)]
pub fn time_shard_write<R>(shard_idx: u32, f: impl FnOnce() -> R) -> R {
    if !enabled_fast() { return f(); }
    let start = std::time::Instant::now();
    let r = f();
    let ns = start.elapsed().as_nanos() as u64;
    record_shard_lock_wait_ns(shard_idx, ns);
    r
}

#[inline(always)]
fn bucket_of_ns(ns: u64) -> usize {
    if ns == 0 { return 0; }
    let log = 64 - ns.leading_zeros() as usize;
    log.min(15)  // 16 buckets, capped at ~32 µs
}
```

Storage cost: `16 buckets × 64 shards × 8 bytes = 8 KiB` total — fits in L1.

#### 4.1.2 JSON output

Trigger: `TSZ_PERF_COUNTERS_OUT=<path>` (or `=1` for default
`./tsz-perf-counters.json`). Wire-up in `crates/tsz-cli/src/bin/tsz.rs:1795`:

```rust
let counter_dump = tsz_common::perf_counters::PerfCounters::dump_string();
if !counter_dump.is_empty() { print!("{counter_dump}"); }
if let Some(path) = tsz_common::perf_counters::PerfCounters::json_output_path() {
    let _ = tsz_common::perf_counters::PerfCounters::write_json_to(&path);
}
```

JSON shape (stable; consumed by `scripts/bench/scale-cliff/run-cliff.sh`):

```json
{
  "schema_version": 1,
  "delegate": { "calls": 0, "cache_hits_lib": 0, "cache_hits_cross_file": 0,
                "misses": 0, "max_recursion_depth": 0 },
  "checker":  { "state_constructed": 0, "with_parent_cache_constructed": 0,
                "by_reason": { "DelegateCrossArenaSymbol": 0, "...": 0 } },
  "overlay":  { "calls": 0, "entries_total": 0, "entries_max": 0,
                "len_ge_1k": 0, "len_ge_10k": 0, "len_ge_100k": 0, "len_ge_1m": 0 },
  "interner": { "calls": 0,
                "by_kind": { "Object": 0, "Application": 0, "...": 0 },
                "string_calls": 0, "type_list_calls": 0, "object_shape_calls": 0,
                "function_shape_calls": 0, "application_calls": 0,
                "conditional_calls": 0, "mapped_calls": 0,
                "shard_lock_wait_ns_buckets": [[0, 0, "...16"], "...64 shards"] },
  "compute_type_of_symbol": { "calls": 0, "cache_hits": 0 },
  "resolver": { "lookup_calls": 0, "is_file_calls": 0, "is_dir_calls": 0,
                "read_dir_calls": 0, "read_package_json_calls": 0,
                "candidate_paths_total": 0 }
}
```

Use `serde_json::json!` macro — no `Serialize` derive on `PerfCounters`
(stateful atomic loads aren't reflectable). `serde_json` is already a
`tsz-common` dep.

#### 4.1.3 Histogram choice — manual 16-bucket power-of-two

Reject `hdrhistogram` (heavyweight, not currently in `Cargo.toml`). 16
power-of-two buckets handle the four decision thresholds the architectural
plan cares about (1k/10k/100k/1M for overlay, 1µs–32µs for lock-wait); more
fidelity is wasted bytes.

#### 4.1.4 Test plan

Three test files in `crates/tsz-common/tests/`. Critical: **the
"enabled" test must live in its own integration target** because
`ENABLED_FAST: OnceLock<bool>` is set on first observation. Cargo runs each
`tests/*.rs` file as its own process binary, so a per-file split gives clean
state.

```rust
// tests/perf_counters_disabled.rs
#[test]
fn disabled_when_env_unset() {
    assert!(std::env::var_os("TSZ_PERF_COUNTERS").is_none());
    let before = counters().delegate_cross_arena_calls.load(Ordering::Relaxed);
    inc(&counters().delegate_cross_arena_calls);
    assert_eq!(counters().delegate_cross_arena_calls.load(Ordering::Relaxed), before);
}

// tests/perf_counters_enabled.rs
#[test]
fn enabled_when_env_set() {
    unsafe { std::env::set_var("TSZ_PERF_COUNTERS", "1"); }
    let before = counters().delegate_cross_arena_calls.load(Ordering::Relaxed);
    inc(&counters().delegate_cross_arena_calls);
    assert_eq!(counters().delegate_cross_arena_calls.load(Ordering::Relaxed), before + 1);
}

// tests/perf_counters_json.rs
#[test]
fn json_dump_shape_round_trips() {
    let v = PerfCounters::dump_json();
    assert_eq!(v["schema_version"], 1);
    let s = serde_json::to_string(&v).unwrap();
    let v2: serde_json::Value = serde_json::from_str(&s).unwrap();
    assert_eq!(v, v2);
}
```

#### 4.1.5 Estimated effort

~300 LOC. One PR. One day's coding + half a day for the resolver helper
sweep + half a day for tests = two days end-to-end.

### 4.2 PR T1.2 — Wire `PhaseTimings` into bench JSON

`PhaseTimings` is captured in
`crates/tsz-cli/src/driver/core.rs:172` (`CompilationResult.phase_timings`)
but never exported. `bench-vs-tsgo.sh` extracts only hyperfine wall-time via
`hyperfine_mean_for()` at line 238.

Change: add a sidecar invocation per fixture (1 run, no warmup) that
captures `--extendedDiagnostics` JSON, merges into the matrix-shard JSON,
and surfaces in `docs/site/benchmarks.md`.

**Why this matters for T2**: a Tier 2 PR that cuts wall-time 30% must show
*which* phase moved. Without this we will land an architectural change and
not be able to attribute the win.

~150 LOC. One PR. Independent of T1.1 and T1.3; can land in parallel.

### 4.3 PR T1.3 — Make scale-cliff a CI gate

Fixtures already exist (`scripts/bench/scale-cliff/generate-fixtures.sh`
produces monorepo-001 through monorepo-006, 1 → 6000 files × 1 → 50
packages). Runner exists (`scripts/bench/scale-cliff/run-cliff.sh`). Neither
runs on CI.

Change: add a daily workflow run that emits the per-file-ratio CSV and
posts a regression alert if ratios on monorepo-004+ move more than ±10%
between adjacent runs.

The "cliff" concept (per the original architectural plan): plot wall-time /
file-count as N grows. tsgo's curve is roughly flat (~5 ms/file). tsz's
curve today is roughly flat through monorepo-003 (~100 files) then
inflects — that inflection is the cliff. The CI gate flags regressions
before they ship.

Depends on T1.1 (uses the JSON output for counter columns). One PR,
~100 LOC of workflow + script change.

### 4.4 Tier 1 exit criteria

- `TSZ_PERF_COUNTERS=1` produces a JSON dump consumable by `jq`.
- `bench-vs-tsgo.sh --json` includes per-fixture phase timings.
- Daily CI run on scale-cliff fixtures publishes a CSV; regression bot
  comments on PRs that move ratios > ±10%.

---

## 5. Tier 2 — Scale cliff (the headline)

The destination architecture, restated from first principles:

```
Global (immutable, lifetime = program):
  - intrinsic TypeIds (TypeId::NONE..=STRICT_ANY, reservations 0..99)
  - canonical symbols, declarations, files
  - string atoms (Atom)
  - lib metadata (parse+bind state, now disk-cached via lib_snapshot.rs)

Per checker (lifetime = program; one per worker, N = num_cpus):
  - type arena (shared via interner; checker holds queries)
  - structural interner caches (eval, subtype, assignability — promoted
    from per-file to per-program)
  - per-checker query caches (RefCell, no atomic overhead)

Cross-file:
  - checker ↔ checker via stable SymbolId/DefId queries
  - never pass TypeId between separate interners (today: program-global
    interner means this is satisfied trivially)
  - only diagnostics/displayable outputs escape per-file scope

No periodic merge phase; no recursive child-checker construction.
```

We are roughly at "Global immutable + N per-file checkers". Target is
"N per-program checkers + cross-checker DefId protocol". That is a 4-PR
multi-week effort.

### 5.1 PR T2.1 — Bounded checker pool, not per-file checkers

#### 5.1.1 Architecture choice — thread-pinned pool

Three viable shapes:

**(a) Thread-pinned pool (Rayon `start_handler` + thread-local).** N=num_cpus
checkers, each pinned to a worker. Files dispatched by `rayon::scope`, each
worker pulls "its" checker from a thread-local. Zero coordination overhead.
Limitation: imbalanced work cannot redistribute.

**(b) Pull-from-pool with work-stealing (`crossbeam_deque`).** Lock-free
queue; workers pop a checker, run a file, push it back. Adds ~1 µs per file.
Better load balance.

**(c) Hybrid.** Pinned by default with a loaner path. High complexity.

**Pick (a).** Justification:

1. `CheckerContext` holds `Rc<EvaluationSession>` at `:913`, several
   `RefCell` caches at `:367,377,381,396,461-464,474,478-487,492,501,588-617`,
   and a `flow_worklist` at `:461`. None are `Send`/`Sync`. With pinning
   they never leave the worker — no `unsafe impl Send`, no soundness
   footnote.
2. Existing rayon use at `tsz-core/src/parallel/core.rs:428` (parsing) and
   `:5307` (function-body checking) is naturally pinned-compatible.
3. (b) becomes a follow-up if T1.1 counters show worker imbalance after (a)
   ships.

Sketch:

```rust
// crates/tsz-core/src/parallel/checker_pool.rs (new)
thread_local! {
    static WORKER_CHECKER: RefCell<Option<PooledChecker>> = const { RefCell::new(None) };
}

let pool = rayon::ThreadPoolBuilder::new()
    .num_threads(num_cpus::get())
    .stack_size(THREAD_STACK_SIZE_BYTES)
    .start_handler(move |_idx| {
        WORKER_CHECKER.with(|slot| {
            *slot.borrow_mut() = Some(PooledChecker::new(/* shared program refs */));
        });
    })
    .build()
    .unwrap();

pool.scope(|s| {
    for (file_idx, file) in program.files.iter().enumerate() {
        s.spawn(move |_| {
            WORKER_CHECKER.with(|slot| {
                let checker = slot.borrow_mut().as_mut().unwrap();
                checker.check_file(file_idx, file);
            });
        });
    }
});
```

#### 5.1.2 Per-file state hazard list (audit results)

`CheckerState<'a>` borrows `&'a NodeArena` and `&'a BinderState` from the
*current* file (`crates/tsz-checker/src/state/state.rs:163-164`). Making it
program-lifetime requires replacing `'a` with a swap-on-each-file
mechanism. Below are every place the audit found per-file state assumed
implicitly. **Missing any of the 13 🔴 hazards produces a silent
wrong-answer bug.**

| #  | File:line                                                 | Field / state                                  | Severity | Action                          |
| -- | --------------------------------------------------------- | ---------------------------------------------- | -------- | ------------------------------- |
| 1  | `crates/tsz-checker/src/types/utilities/cycle_guard.rs:40-51` | `CONST_ENUM_VISITED`, `NON_CONST_ENUM_VISITED` thread-locals (FxHashSet<NodeIndex>) | 🔴 | Call `clear_visited_sets()` per file |
| 2  | `crates/tsz-checker/src/types/utilities/enum_utils.rs:21-24`, `const_enum_eval.rs:23-25` | `EVAL_MEMO`, `CONST_EVAL_MEMO` thread-locals | 🔴 | Call existing `clear_*_memo()` per file |
| 3  | `crates/tsz-checker/src/context/mod.rs:917`               | `recursion_depth: RefCell<DepthCounter>`       | 🔴 | Reset per file (rebuild or `.set_zero()`) |
| 4  | `crates/tsz-checker/src/context/mod.rs:720,732,739,743`   | All `Vec<Diagnostic>` accumulators             | 🔴 | `clear()` per file (already drained at `parallel/core.rs:5354` via `mem::take`; must re-clear in pool) |
| 5  | `crates/tsz-checker/src/context/mod.rs:722`               | `emitted_diagnostics: FxHashSet<(u32, u32)>`   | 🔴 | `clear()` per file (otherwise file-2 diagnostics suppressed) |
| 6  | `crates/tsz-checker/src/context/mod.rs:419`               | `request_node_types: FxHashMap<(u32, RequestCacheKey), TypeId>` (u32 = NodeIndex) | 🔴 | `clear()` per file or rekey to include `FileId` |
| 7  | `crates/tsz-checker/src/context/mod.rs:803`               | `node_resolution_stack: Vec<NodeIndex>`        | 🔴 | `clear()` per file |
| 8  | `crates/tsz-checker/src/context/mod.rs:809-825`           | `implicit_any_checked_closures`, `implicit_any_contextual_closures`, `deferred_implicit_any_closures`, `speculative_implicit_any_closures` (all NodeIndex-keyed) | 🔴 | `clear()` per file |
| 9  | `crates/tsz-checker/src/context/mod.rs:579,583`           | `class_instance_type_cache`, `class_constructor_type_cache: FxHashMap<NodeIndex, TypeId>` | 🔴 | `clear()` per file |
| 10 | `crates/tsz-checker/src/context/mod.rs:835,840`           | `checking_classes`, `checked_classes: FxHashSet<NodeIndex>` | 🔴 | `clear()` per file |
| 11 | `crates/tsz-checker/src/context/mod.rs:788`               | `pending_circular_return_sites: FxHashMap<SymbolId, Vec<NodeIndex>>` | 🔴 | `clear()` per file (NodeIndexes inside leak) |
| 12 | `crates/tsz-checker/src/context/mod.rs:209-213`           | `call_depth`, `circ_ref_depth`, `overlap_depth: RefCell<DepthCounter>` | 🔴 | Reset per file |
| 13 | `crates/tsz-checker/src/context/mod.rs:895`               | `instantiation_depth: Cell<u32>`               | 🔴 | `.set(0)` per file |
| 14 | `crates/tsz-checker/src/context/mod.rs:727`               | `no_overload_call_nodes: FxHashSet<u32>` (NodeIndex) | 🔴 | `clear()` per file |
| 15 | `crates/tsz-checker/src/context/mod.rs:367-396`           | Six cross-file lookup caches keyed by string/SymbolId | ⚠️ | Audit keying; clear conservatively if any encode per-file scope |

Fields explicitly **safe** to keep across files (do not clear):

- `symbol_types: SymbolTypeCache` (`mod.rs:339`) — keyed by SymbolId, post-merge stable.
- `symbol_instance_types: SymbolTypeCache` (`mod.rs:343`) — same.
- `lib_delegation_cache: FxHashMap<SymbolId, TypeId>` (`mod.rs:361`) — same.
- `shared_lib_type_cache: DashMap<String, Option<TypeId>>` (`mod.rs:400`) — string-keyed, intended to be program-scoped.
- `current_file_idx: usize` (`mod.rs:1151`) — already set explicitly per dispatch (`context/core.rs:826`).

Required scaffolding:

```rust
// crates/tsz-checker/src/context/core.rs (new method)
impl CheckerContext<'_> {
    /// Reset all per-file state. Call before checking each new file.
    /// Must keep program-scoped caches (symbol_types, lib_delegation_cache, etc.).
    /// Must clear all NodeIndex-keyed caches and all diagnostic accumulators.
    pub fn reset_for_next_file(&mut self) {
        // §5.1.2 hazards 1-14. See audit table.
        self.diagnostics.clear();
        self.callback_return_type_errors.clear();
        self.deferred_truthiness_diagnostics.clear();
        self.deferred_excess_property_implicit_any_diagnostics.clear();
        self.emitted_diagnostics.clear();
        self.request_node_types.clear();
        self.node_resolution_stack.clear();
        self.implicit_any_checked_closures.clear();
        self.implicit_any_contextual_closures.clear();
        self.deferred_implicit_any_closures.clear();
        self.speculative_implicit_any_closures.clear();
        self.class_instance_type_cache.clear();
        self.class_constructor_type_cache.clear();
        self.checking_classes.clear();
        self.checked_classes.clear();
        self.pending_circular_return_sites.clear();
        self.no_overload_call_nodes.clear();
        self.recursion_depth.borrow_mut().reset();
        self.call_depth.borrow_mut().reset();
        self.circ_ref_depth.borrow_mut().reset();
        self.overlap_depth.borrow_mut().reset();
        self.instantiation_depth.set(0);
        // Thread-locals — must be called from the same thread:
        crate::types::utilities::cycle_guard::clear_visited_sets();
        crate::types::utilities::enum_utils::clear_enum_eval_memo();
        crate::types::utilities::const_enum_eval::clear_const_eval_memo();
        // Debug-mode assert all recursion stacks are empty (catch leaks).
        debug_assert!(self.symbol_resolution_stack.is_empty());
        debug_assert!(self.symbol_resolution_set.is_empty());
        debug_assert!(self.import_resolution_stack.is_empty());
    }
}
```

#### 5.1.3 Cache-lifetime audit (`QueryCache`)

`crates/tsz-solver/src/caches/query_cache.rs:329` has 11 RefCell-backed
local caches plus an optional `&SharedQueryCache` (`:81-85`). Today the
per-file `QueryCache` is built at `crates/tsz-core/src/parallel/core.rs:5501-5505`
and dropped per file. After T2.1 it lives for the worker's lifetime.

All 11 are type-keyed (`TypeId`, `RelationCacheKey`, `DefId`, `Atom`), not
NodeIndex-keyed. **All 11 are program-lifetime safe** because every checker
in T2.1 shares `program.type_interner` (`tsz-core/src/parallel/core.rs:5316`)
— the `TypeId` universe is global; the architectural rule "do not pass a
`TypeId` from one checker's interner to another" is trivially satisfied.

Caveat: `variance_cache: DefId → Arc<[Variance]>` at `:343` — variance
depends on `def_type_params` which is checker-context state (`mod.rs:1008`).
If two files register different params for the same `DefId`, variance would
be stale. Audit in PR A: search for `def_type_params.borrow_mut().insert`
and confirm no `DefId` ever gets two different parameter lists. If clean,
shipping T2.1 lets us drop the `SharedQueryCache` `DashMap` layer entirely
in T2.D (one less DashMap write per query).

#### 5.1.4 Migration sequence — 4 PRs

**PR T2.1.A — `PooledChecker` with N=1.** Introduce `PooledChecker` struct
that wraps `CheckerState` plus the `reset_for_next_file()` method from
§5.1.2. New entry point `check_files_with_pool` runs sequentially, reusing
one checker. Existing `check_files_parallel` unchanged. Behind env flag
`TSZ_CHECKER_POOL=1`. **Verification gate**: full conformance run with the
flag set produces byte-identical diagnostic output to without. Bench:
should be slightly slower on small fixtures (per-call save/restore
overhead) but neutral on large because there's only ever one file in
flight. *Risk unit*: every cache miss in §5.1.2 is a silent bug here.
Required: a debug-assertion mode that re-runs each file with a fresh
checker and diffs all outputs (gated behind `TSZ_CHECKER_POOL_PARANOID=1`).

**PR T2.1.B — Switch production parallel path to thread-pinned pool of
N=num_cpus.** Replace `maybe_parallel_iter!(program.files).enumerate().map(check_one_file)`
at `parallel/core.rs:5725` with the Rayon `start_handler` thread-local
pattern in §5.1.1. Lib-file checking (`check_one_lib`,
`check_one_lib_baseline` at `:5572,5659`) stays the same for now. **Risk
unit**: thread-local lifetime issues; `'a` lifetime erasure on
`arena`/`binder` pointers (the swap-files API converts `&'a NodeArena` into
a per-call parameter or a `Cell<*const NodeArena>` with a scope guard).
**Verification**: T1.3 scale-cliff CSV must show monotonic improvement on
monorepo-001…006.

**PR T2.1.C — Route `delegate_cross_arena_symbol_resolution` through
swap-files API instead of constructing child `CheckerState`.** Cross-file
queries today (cause #2 in §2) build a child checker via
`with_parent_cache_attributed` (`cross_file.rs:811-867`). After T2.1.C they
swap the current checker's `arena`/`binder`/file-local-state to point at
the target file, run the query, swap back. New helper module
`crates/tsz-checker/src/query_boundaries/cross_file.rs`:

```rust
pub enum CrossFileQuery {
    SymbolType(SymbolId),
    InterfaceType(SymbolId),
    ClassInstanceType(SymbolId),
    InterfaceMemberSimpleTypes(SymbolId),
}

pub struct CrossFileQueryResult {
    pub type_id: TypeId,
    pub type_params: Vec<TypeParamInfo>,
}

impl PooledChecker {
    fn resolve_cross_file(&mut self, target_file_idx: usize, q: CrossFileQuery)
        -> Option<CrossFileQueryResult>
    {
        let saved = self.snapshot_file_local_state();
        self.swap_to_file(target_file_idx);
        let result = match q {
            CrossFileQuery::SymbolType(s) => self.checker.get_type_of_symbol(s),
            CrossFileQuery::InterfaceType(s) => self.checker.get_interface_type(s),
            CrossFileQuery::ClassInstanceType(s) => self.checker.get_class_instance_type(s),
            CrossFileQuery::InterfaceMemberSimpleTypes(s) => self.checker.get_interface_member_simple_types(s),
        };
        self.restore_file_local_state(saved);
        result
    }
}
```

Save/restore cost: `node_types` is `Arc<FxHashMap<u32, TypeId>>` at
`context/caches.rs:14` (already snapshot-cheap via `Arc::clone`).
`cross_file_symbol_targets` is already designed for parent/child snapshot
(`context/core.rs:161`). Other NodeIndex caches need cloning, but only on
cross-file queries, not per-file. **Verification gate**: PR T2.1.C is the
keystone; if conformance regresses, the swap-files protocol has a hole.

Architectural compliance: this fits CLAUDE.md §3, §4, §11, §12, §22 — type
computation stays in `compute_type_of_symbol` (solver-orchestrated), checker
stays thin orchestration. The query protocol is a `query_boundaries/`
helper, not ad-hoc checker logic.

**PR T2.1.D — Drop `SharedQueryCache` `DashMap` layer (optional).** Once N
workers each have program-lifetime local caches, the cross-thread `DashMap`
write at `query_cache.rs:81-85` is pure overhead. Drop the layer; verify
each `QueryCache` is constructed without `new_with_shared` at the two sites
in `parallel/core.rs:5501,5582`. **Risk unit**: low. Ship A/B/C first,
measure, only land D if measured win.

### 5.2 PR T2.2 — Eliminate recursive child-checker construction

Subsumed by T2.1.C above — the swap-files protocol *is* the elimination.
T2.2 is a separate PR-tracking name only if T2.1.C is deferred for any
reason. Otherwise treat as part of T2.1.

### 5.3 PR T2.3 — One lib-symbol merge per program

Today: lib globals are merged into each per-file checker's symbol table at
`crates/tsz-core/src/parallel/core.rs:2179` and reconstructed inside each
`check_one_lib` call. PR #4587 cached the lib *parse+bind*; the lib *merge*
still runs N times.

Change: compute a `MergedLibSymbols` once during
`parse_and_bind_parallel_with_libs` (`tsz-core/src/parallel/core.rs:1347`)
and `Arc::share` it into each pooled checker at construction.

Expected delta: 10–20% on monorepos; lower on small fixtures (already only
47 lib files × small N).

Pre-requisite: probably depends on T2.1 because the symbol-table layout
assumes per-file ownership; with a checker pool the merged data is
naturally shared.

~400 LOC. Independent PR after T2.1 lands.

### 5.4 PR T2.4 — Type-interner reverse-Vec lock sharding

`crates/tsz-solver/src/intern/core/interner.rs:268` holds
`RwLock<Vec<TypeData>>`. Inserts take the write lock. Today's contention is
hidden behind the thread-local lookup cache (1024 entries, lines 38–72) but
once T2.1 is in place and fan-out goes up, this becomes the next
bottleneck.

Change: shard into `[RwLock<Vec<TypeData>>; 32]` indexed by hash; `TypeId`
encodes shard in low bits.

Defer until T1.1 counters show contention (the `interner_shard_lock_wait_ns`
buckets from §4.1.1 site #10). Premature without T2.1 in place.

**Risk**: `TypeId(u32)` packing is structural per CLAUDE.md §16 (the 0..99
reservation policy); need stable bit layout to avoid breaking solver
query-cache keys. Audit before coding.

~200 LOC. Optional.

### 5.5 What Tier 2 does not solve

- **Resolver syscall topology.** Source discovery / `Path::is_file` /
  `package.json` reads (per the 2026-04-29 update in the original
  architectural plan) are pre-checker work and untouched by T2. Tier 4.1
  promotes the existing thread-local file-existence cache (PR #4513) to a
  per-worker resolver bundle.
- **`merge_bind_results_ref` hotspot.** Pre-check work.
- **Cross-checker `TypeId` mixing.** T2.1 keeps the global
  `program.type_interner` and global `DefinitionStore`, so the architectural
  plan's stronger rule ("no `TypeId` crosses a checker boundary") is *not*
  enforced. Every worker's checker shares the same `TypeId` universe via
  `&dyn QueryDatabase`. That is a deliberate choice for T2.1: enforcing the
  stronger isolation requires checker-local interners (a separate
  ~3-week effort), and doing both at once would conflate two independent
  risks. T2.1 first; checker-local interner only if measured contention
  remains after T2.4.
- **Out-of-order or work-stealing scheduling.** Files still complete in
  arrival order on each worker. Shapes (b) and (c) from §5.1.1 are
  deferred.
- **Speculative call-resolution rollback semantics.** The deferred-diagnostic
  queues in §5.1.2 hazard #4 are correctly cleared per file by T2.1, but
  T2.1 does not change *how* speculation works
  (`overload_resolution.rs:243`'s `mem::take` of `node_types`).

### 5.6 Tier 2 exit criteria

- `large-ts-repo` (cleaned fixture #6) wall-time ≤ 90 s (target ratio
  ≤ 3× tsgo).
- Scale-cliff CSV from T1.3 shows roughly flat per-file ratio across
  monorepo-001…006 (matching tsgo's curve shape).
- Conformance pass rate within ±0.05 pp of pre-Tier 2 baseline (we accept no
  conformance regressions for perf wins).
- Per-file `CheckerState` constructions (counter from T1.1) drop from
  ~6086 to ~num_cpus on the large-repo fixture.

---

## 6. Tier 3 — Small-fixture polish (continue current trajectory)

Five PRs, ordered by impact per the existing investigation. Each is
independently shippable; total budget ~1–2 weeks.

### 6.1 PR T3.1 — Persistent type-interner snapshot (Phases 2 + 3 of lib cache)

Phase 1 (PR #4587, just merged) caches parse+bind state for stdlib lib files
via `crates/tsz-core/src/parallel/lib_snapshot.rs`. Phases 2+3 cache the
populated `TypeInterner` so we skip the type-construction work too.

The earlier Phase 1.4 prototype used JSON: 27% regression (231 ms vs
181 ms baseline on vite-vanilla-ts-app), 29 MB cache for 49 lib files.
Lesson: **binary format mandatory**.

#### 6.1.1 Format choice — `postcard`

| Candidate     | Size (49 lib files) | Deserialize cost           | Dependency footprint                      | Risks                                                             |
| ------------- | ------------------- | -------------------------- | ----------------------------------------- | ----------------------------------------------------------------- |
| **postcard**  | ~3–5 MB (varint)    | ~2–4 ms / interner         | One serde-compatible dep                  | None beyond bincode; no `unsafe`; deterministic                   |
| rkyv          | ~7–10 MB (alignment) | ~0 ms (zero-copy via mmap) | Two deps (`rkyv`, `bytecheck`); custom derives diverge from existing serde derives | Alignment crashes if file mmapped at unaligned offset; on-disk format changes between minor versions; requires `bytecheck` for safety; `unsafe` in fast path |
| bincode 1.x   | ~6–8 MB (fixint)    | ~5–8 ms                    | Already in tree                           | The `skip_serializing_if`-desync bug class already cost a half-day on Phase 1 (`lib_snapshot.rs:18-23`) |

**Pick postcard.** Three concrete justifications:

1. The interner is a sea of `u32` (`TypeId`, `ObjectShapeId`, `DefId`,
   `Atom`). `TypeId(0)..TypeId(99)` are reserved
   (`crates/tsz-solver/src/types.rs:85-154`); every reference inside
   `TypeData` is one of these `u32` IDs (`types.rs:738-923`). Postcard's
   varint codec encodes IDs ≤ 127 in 1 byte and ≤ 16383 in 2 bytes —
   nearly every ID in a lib snapshot fits in 2 bytes. Bincode 1 fixint
   always uses 4 bytes. Just from this: ~2× size win.
2. Postcard is field-tag-driven for enums, resilient to optional fields —
   the bincode 1 desync hazard (`lib_snapshot.rs:18-23`) doesn't recur.
   `TypeData` is a 26-variant enum (`types.rs:738-923`) and Phases 2+3
   will add variants over time.
3. No `unsafe`, no alignment, no mmap. Phase 1 ships under 200 ms per cold
   start; saving 5 ms by going zero-copy isn't worth the rkyv operational
   risk (alignment-fault crashes, ABI freeze on the on-disk schema).

Reject **rkyv**: zero-copy only pays off if downstream code can read
directly from the mmapped buffer. `TypeInterner` is a `DashMap` of
`TypeData → TypeId` (`interner.rs:262-272`); the live interner allocates
new entries unconditionally, so the mmapped data must be *copied into* the
DashMap on load anyway. Zero-copy win evaporates.

Reject **bincode 2**: viable backup if postcard turns out slower than
projected, but its varint mode requires opting into a different codec path
and we'd reuse none of the existing bincode-1 magic-header machinery.

#### 6.1.2 What gets serialized

Read `crates/tsz-solver/src/intern/core/interner.rs:502-616`. Persist:

- `shards: Vec<TypeShard>` — the core mapping (sharded
  `(DashMap<TypeData, u32>, RwLock<Vec<TypeData>>)` × 64).
- `string_interner: ShardedInterner` — atoms appear inside
  `TypeData::Literal::String(Atom)`, `PropertyInfo::name`, etc.
- `type_lists`, `tuple_lists`, `template_lists` — referenced by ID inside
  `TypeData::Union/Tuple/TemplateLiteral`.
- `object_shapes`, `function_shapes`, `callable_shapes`, `conditional_types`,
  `mapped_types`, `applications` — all referenced by ID.
- `boxed_types`, `boxed_def_ids`, `this_type_marker_def_ids`,
  `array_base_type`, `array_display_base_type`, `array_base_type_params` —
  lib-derived globals populated only during the lib pass.

Skip (re-derive lazily on first access — matches the BinderState
`#[serde(skip)]` pattern from PR #3, `lib_snapshot.rs:34-37`):

- `identity_comparable_cache`, `contains_this_cache`, `display_properties`,
  `display_alias`, `display_union_origin`, `object_property_maps`.

Reset on load:

- `alloc_counter` to high-water mark; `instance_id` fresh.
- `poisoned`, `union_too_complex`, `evaluation_fuel`,
  `no_unchecked_indexed_access`, `exact_optional_property_types` — runtime
  flags reset to default or compiler-option-derived value.
- Thread-local lookup/intern caches — already cleared per
  `clear_thread_local_cache` (`interner.rs:183-198`).

Critical structural fact: `TypeData` is `Copy`-shaped and all
self-references go through interned IDs (`types.rs:738-923`). **No `Arc`
cycles**. The `Recursive(u32)`/`BoundParameter(u32)` De Bruijn variants
encode pure structure and round-trip trivially.

#### 6.1.3 Atom remapping (the subtle bit)

`Atom` embeds shard index in low `SHARD_BITS=6` of `u32`
(`crates/tsz-common/src/interner/mod.rs:50-52,406-411`). On load, shard
insertion order can differ from the recording run (different thread
scheduling), so raw `Atom` values are not reproducible across runs.

Strategy: serialize a string list, on load call `intern_string` for each
in deterministic order, build a per-snapshot `OldAtom → NewAtom` remap
table, walk all `TypeData`/shape values during deserialize and rewrite
embedded `Atom`s through the table. O(n), cheap. Mirrors the strategy
`BinderState` already uses for symbol references (PR #3 lazy-rebuild
invariant).

`TypeId` itself is *not* shard-encoded — `interner.rs:1067-1071` derives
`shard_idx` from `hash(TypeData)`, not from id bits. So `TypeId`
round-trips byte-stable provided we re-intern into the same shards in the
same order, which we control on the deserialize side.

#### 6.1.4 Versioning — manual `SNAPSHOT_VERSION`

Auto-invalidating on every git commit costs a full ~10–15 ms cache miss on
every developer's first run after `git pull`. Manual is the right call —
matches rustc incremental cache (uses `rustc -V`, not git SHA).

- Bump `SNAPSHOT_MAGIC` from `b"TSZSNAP\x03"` to `b"TSZSNAP\x04"` on the
  postcard switchover (`lib_snapshot.rs:59`).
- Add `SNAPSHOT_SCHEMA_VERSION: u32 = 1` constant in a new file
  (`crates/tsz-solver/src/intern/core/snapshot.rs`). Bump on any field
  add/remove/rename inside `FrozenInternerSnapshot`, `TypeData`, or any
  shape struct.
- Document the bump policy in a comment block on `TypeData`
  (`types.rs:738`) — the same pattern protects `LiteralData`/`FunctionTypeData`
  field changes today.
- Add `BUILTIN_TYPEID_LAYOUT_VER: u32 = 1` next to `TypeId::FIRST_USER`
  (`types.rs:154`). Bump when reordering any `pub const TypeId` reservation
  in 0..99 range.

#### 6.1.5 Cache key — three-layer

```
cache_key = blake3(
    b"tsz-libsnap-v1"               // namespace (rotate to flush all caches)
    || u32_le(SNAPSHOT_MAGIC_VER)   // current "\x04" -> 4
    || u32_le(SNAPSHOT_SCHEMA_VER)  // §6.1.4
    || u32_le(BUILTIN_TYPEID_LAYOUT_VER)  // §6.1.4
    || u32_le(libfile_count)
    || for each lib_file (sorted by name):
         len_le(name) || name || len_le(src) || src
    || u32_le(compile_options_hash)  // {target, lib, module}
)
```

Replace the current `FxHasher((file_name, source_text))` key
(`lib_snapshot.rs:87-93`). FxHasher is non-cryptographic and per-process;
blake3 gives cross-platform stability and excludes trivial collisions.

#### 6.1.6 Size budget

Phase 1+2+3 estimate: ~10 MB in `~/.cache/tsz/lib-cache/`. Comparison:
rustc incremental ~100 MB+ per workspace, sccache 5–10 GB by default,
Cargo registry index 200 MB+. 10 MB per `(target, lib, module)` triple is
well within norms.

Safety net: emit `tracing::warn!` if cache dir exceeds 1 GB; add a
`tsz cache clear` subcommand (separate small PR).

#### 6.1.7 Phase split — 3 PRs

| #         | Scope                                                                              | LOC   | Bench claim                                          | Deps |
| --------- | ---------------------------------------------------------------------------------- | ----- | ---------------------------------------------------- | ---- |
| **T3.1.A** | Phase 2-prep + Phase 2-capture: serde derives on type structs + `FrozenInternerSnapshot` capture/install round-trip + unit tests | ~1500 | "structural prep — no bench delta"                   | none |
| **T3.1.B** | Phase 3 wiring: postcard format swap, magic `\x04`, `SNAPSHOT_SCHEMA_VERSION`, layered cache key, frozen-snapshot read/write, panic-safe deserialize | ~800 | "+20–30 ms on `vite-vanilla-ts-app` with `TSZ_LIB_CACHE=1`" | T3.1.A |
| **T3.1.C** | Default-on + ops polish: flip `TSZ_LIB_CACHE` default to on, add `tsz cache clear`, full conformance run, size-cap warning at 1 GB | ~250 | "matches T3.1.B on enabled path, no regression on disabled path" | T3.1.B |

The `Serialize`/`Deserialize` work in T3.1.A is significant: today only
`TypeId` has `Serialize` (none have `Deserialize`) per `types.rs:7,80`. PR
T3.1.A adds derives on `TypeData`, `ObjectShape`, `FunctionShape`,
`CallableShape`, `ConditionalType`, `MappedType`, `TypeApplication`,
`PropertyInfo`, `TupleElement`, `TemplateSpan`, `TypeParamInfo`,
`LiteralValue` (custom impl via `OrderedFloat`), `IntrinsicKind`, `DefId`,
`TypeListId`. Mechanical; existing `tests/intern_tests.rs` round-trip
checks anchor it.

#### 6.1.8 Risks

- **Stale cache producing wrong types.** Mitigation: layered cache key
  (§6.1.5) + paranoid post-load assertions: walk every reserved
  `TypeId::NONE..STRICT_ANY`, confirm it resolves to the expected variant,
  panic-with-fallback to `TypeInterner::new()` on mismatch. Include a
  content-hash of the ten reserved intrinsic `TypeData::Intrinsic(...)`
  entries in the snapshot header.
- **Cross-platform compatibility.** Postcard varint is endianness-neutral
  and pointer-width-neutral; schema includes no `usize`. Atom remapping
  (§6.1.3) handles thread-count-dependent shard layout.
- **User cache dir corruption.** Phase 1 already handles magic header
  (`lib_snapshot.rs:202-207`) and content hash (`lib_snapshot.rs:142-144`).
  New: wrap deserialize in `std::panic::catch_unwind` (postcard malformed
  input *can* panic on bad varints); convert panic to `None` and fall
  through to fresh parse+bind. Verify `BUILTIN_TYPEID_LAYOUT_VER` matches
  before trusting any TypeId.
- **Disk-cache disabled paths.** Phase 1 already gates on `TSZ_LIB_CACHE=1`.
  Add the inverse: `TSZ_LIB_CACHE=0` forces off even when default flips on
  (T3.1.C).

#### 6.1.9 Test plan

- **Round-trip property test** in
  `crates/tsz-solver/src/tests/intern_tests.rs`: any sequence of
  `intern(TypeData)` operations followed by `capture_frozen()` →
  `install_frozen()` yields an interner where `lookup(id) ==
  lookup_in_original(id)` for every interned ID. `proptest` strategy
  generates arbitrary `TypeData` including De Bruijn forms.
- **Atom remap correctness**: encode `TypeData::Literal(LiteralValue::String(atom_X))`,
  decode in a process where the atom interner has different prior contents,
  assert `resolve(atom_X')` returns the original string.
- **Schema-version regression test**: hand-construct a snapshot with
  `schema_version = 99`, assert `try_load` returns `None`.
- **E2E test extension** in `lib_snapshot.rs:271-340`: extend
  `disk_round_trip_resolves_identifier_text_and_symbols` to also assert
  that `Promise<T>` resolves to the *same* `TypeId` after a cache hit as on
  the first run.
- **Conformance corpus diff**: snapshot=on vs snapshot=off byte-identical
  diagnostic output across the conformance suite.
- **Bench gate**: `cargo bench --bench vite_vanilla_ts_app` must show ≥ 5%
  wall-time win vs Phase 1 baseline. PR body must include the numbers.

### 6.2 PR T3.2 — `ObjectShape::hash` lazy caching (~5 ms)

Profile shows ~100 samples in `ObjectShape::hash` (per-property `Vec` hash).
Add `OnceCell<u64>` on `ObjectShape`; invalidate on existing mutator paths.
File: `crates/tsz-solver/src/relations/judge.rs` shows the hot subtype path
that hashes shapes. ~150 LOC.

### 6.3 PR T3.3 — `walk_referenced_types` allocator reuse (~5 ms)

Profile shows ~35 samples in `walk_referenced_types` from allocation churn.
Replace fresh `FxHashSet`/`Vec` per call with a thread-local pool that
lends + reclaims. ~80 LOC.

### 6.4 PR T3.4 — `collect_comment_at` cache (~2–3 ms)

`(node, pos) → Option<&str>` cache; avoids repeated comment-text scans for
JSDoc-heavy files. ~50 LOC.

### 6.5 PR T3.5 — `judge.rs` shape clone elimination (~5–10 ms)

Shape clones in subtype dispatch at
`crates/tsz-solver/src/relations/judge.rs:578,580,584,586,593-594,959-999`.
Convert to borrowed slices where the lifetime allows. **Risk**: some clones
are load-bearing for cache-key storage; check call sites individually
before refactoring. ~200 LOC.

### 6.6 Tier 3 exit criteria

- vite-vanilla-ts-app wall-time ≤ 100 ms (current 147 ms, target 1.4× tsgo
  vs current 2.07×).
- type-fest, rxjs, kysely fixture wall-times within ±10% of tsgo (no fixture
  >2.5×).

---

## 7. Tier 4 — Long-tail / experimental

### 7.1 T4.1 — Per-worker resolver state

Promote PR #4513's thread-local file-existence cache to a per-worker
resolver bundle that also caches `read_dir`, `package_json`, `tsconfig`
resolution. Aligns with the destination architecture's "per-worker resolver
state" callout. Defer until T2 + T1.1 counters identify resolver as a
non-trivial fraction of remaining wall-time.

### 7.2 T4.2 — Skip-empty-lib-interface-pass

Honor the existing claim
`docs/plan/claims/perf-skip-empty-lib-interface-pass.md`; small win but
free.

### 7.3 T4.3 — Definition store population in parallel

`tsz-core/src/parallel/core.rs:2473` is single-threaded. After T2.1 it
shouldn't be on the critical path; verify with T1.1 counters before
optimizing.

---

## 8. Measurement protocol (mandatory for every perf PR)

Lessons from the Phase 1.4 JSON regression and the run-to-run noise on
small fixtures (±9% on `--quick` mode per the original investigation):

1. **A/B against the same worktree.** Different worktrees mean different
   `target/` content, different sccache state, different file-system state.
   Use one worktree, switch branches, re-build with `cargo build --release`,
   run both. The vite-fast 31 ms run that turned out not to be a regression
   was a multi-worktree artifact.
2. **`--extendedDiagnostics` first, profiler second.** Phase breakdown
   answers "which phase moved" instantly. Reach for `samply` only when
   `--extendedDiagnostics` doesn't explain the delta.
3. **Quick-mode noise: ±14 ms / 9% on small fixtures.** Use full bench mode
   (3 warmup + 10 measured runs) for PR-quality numbers. Quick mode is for
   "is this in the right zip code".
4. **Quote both peak RSS and wall-time** for any large-repo PR. RSS regressions
   that don't move wall-time still cost users headroom.
5. **`scripts/safe-run.sh` wraps any heavy run.** Default 75% physical
   footprint guard. CLAUDE.md §20.75 is non-negotiable.
6. **`scripts/bench/perf-hotspots.sh --quick` before/after** for every
   roadmap-relevant change. ROADMAP §1 makes this a top-priority gate.
7. **Update the metric in the same PR.** When a PR moves a number quoted in
   this document, the PR must update the number. No "we'll update the doc
   later".

---

## 9. What this plan deliberately does NOT include

- **A new picker/script.** CLAUDE.md §20.25 is emphatic about not creating
  new session scripts. Use `scripts/session/quick-pick.sh` or extend
  `pick.py`. This plan adds zero new top-level scripts.
- **Full incremental compilation / `.tsbuildinfo` parity.** Out of scope
  for this perf series; would dominate the engineering budget. Revisit
  after the scale cliff is flattened.
- **WASM-targeted perf.** Different constraints (single thread, no `Mutex`
  if WASM-strict). Out of scope unless the user explicitly redirects to the
  LSP/WASM lane.
- **Allocator swap (mimalloc → jemalloc/snmalloc).** Recon shows
  `mimalloc overhead ~60 samples`. Real but small; revisit after T2 lands.
- **Architectural rewrite of the binder.** Binder isn't on the critical
  path per the recon. Hands off until counters say otherwise.
- **A fully checker-local interner** (the architectural plan's stronger
  isolation rule). Conflated with T2.1 it would double the migration risk
  for an unmeasured marginal win. Defer until T2.4 contention data
  justifies it.

---

## 10. File:line index (for reviewers)

The most-cited locations in this plan, grouped by area.

### Bench infrastructure

- `scripts/bench/bench-vs-tsgo.sh:162-172` — quick/full mode constants
- `scripts/bench/bench-vs-tsgo.sh:238` — `hyperfine_mean_for()` extraction
- `scripts/bench/bench-vs-tsgo.sh:705-713` — tsz/tsgo invocation (apples-to-apples)
- `scripts/bench/scale-cliff/generate-fixtures.sh` — monorepo-001…006 generator
- `scripts/bench/scale-cliff/run-cliff.sh:67-105` — current text-dump parser (T1.1 swaps to JSON)
- `.github/workflows/bench.yml:257-279` — 8-shard matrix
- `.github/workflows/bench.yml:363-431` — GCS publish path

### Pipeline orchestration

- `crates/tsz-core/src/parallel/core.rs:428` — parallel parse
- `crates/tsz-core/src/parallel/core.rs:817` — `parse_and_bind_parallel`
- `crates/tsz-core/src/parallel/core.rs:974` — `load_lib_files_for_binding`
- `crates/tsz-core/src/parallel/core.rs:1148` — `parse_and_bind_lib_file` (consults lib snapshot)
- `crates/tsz-core/src/parallel/core.rs:1347` — `parse_and_bind_parallel_with_libs`
- `crates/tsz-core/src/parallel/core.rs:2179` — file-locals reconstruction
- `crates/tsz-core/src/parallel/core.rs:2473` — `pre_populate_definition_store`
- `crates/tsz-core/src/parallel/core.rs:2591` — `merge_bind_results`
- `crates/tsz-core/src/parallel/core.rs:5316` — shared `program.type_interner`
- `crates/tsz-core/src/parallel/core.rs:5320` — per-file `CheckerState::new` (T2.1 target)
- `crates/tsz-core/src/parallel/core.rs:5354` — per-file diagnostic drain
- `crates/tsz-core/src/parallel/core.rs:5384` — `check_files_parallel`
- `crates/tsz-core/src/parallel/core.rs:5418` — `SharedBinderData::from_program()`
- `crates/tsz-core/src/parallel/core.rs:5461` — global symbol→file index
- `crates/tsz-core/src/parallel/core.rs:5501` — per-file `QueryCache` construction
- `crates/tsz-core/src/parallel/core.rs:5572` — `check_one_lib`
- `crates/tsz-core/src/parallel/core.rs:5725` — `maybe_parallel_iter!(program.files)` (T2.1.B target)

### Lib snapshot cache (Phase 1, just merged)

- `crates/tsz-core/src/parallel/lib_snapshot.rs:18-23` — bincode `skip_serializing_if` desync warning
- `crates/tsz-core/src/parallel/lib_snapshot.rs:34-37` — `#[serde(skip)]` lazy-rebuild fields
- `crates/tsz-core/src/parallel/lib_snapshot.rs:59` — `SNAPSHOT_MAGIC = b"TSZSNAP\x03"`
- `crates/tsz-core/src/parallel/lib_snapshot.rs:62` — `TSZ_LIB_CACHE` env-var gate
- `crates/tsz-core/src/parallel/lib_snapshot.rs:87-93` — content-hash cache key (T3.1.B replaces with blake3)
- `crates/tsz-core/src/parallel/lib_snapshot.rs:142-144` — content-hash verification
- `crates/tsz-core/src/parallel/lib_snapshot.rs:202-207` — magic-header verification
- `crates/tsz-core/src/parallel/lib_snapshot.rs:271-340` — disk round-trip E2E test (T3.1.B extends)

### Type interner

- `crates/tsz-solver/src/intern/core/interner.rs:32` — RwLock::read ~15-25 ns comment
- `crates/tsz-solver/src/intern/core/interner.rs:38-90` — thread-local lookup/intern caches
- `crates/tsz-solver/src/intern/core/interner.rs:262-272` — DashMap forward / RwLock<Vec> reverse
- `crates/tsz-solver/src/intern/core/interner.rs:502` — `pub struct TypeInterner`
- `crates/tsz-solver/src/intern/core/interner.rs:817` — `intern_string`
- `crates/tsz-solver/src/intern/core/interner.rs:1034` — `intern` (top-level)
- `crates/tsz-solver/src/intern/core/interner.rs:1067-1071` — `shard_idx = hash(TypeData)`
- `crates/tsz-solver/src/intern/core/interner.rs:1086` — `Entry::Vacant` insert
- `crates/tsz-solver/src/intern/core/interner.rs:1095-1100` — reverse-vec write lock (T1.1 site #10)
- `crates/tsz-solver/src/intern/core/interner.rs:1185,1191` — `intern_type_list` (Vec/slice)
- `crates/tsz-solver/src/intern/core/interner.rs:1203,1671,1679,1686,1690` — shape interns
- `crates/tsz-solver/src/types.rs:85-154` — `TypeId` reservations 0..99
- `crates/tsz-solver/src/types.rs:154` — `TypeId::FIRST_USER = 100`
- `crates/tsz-solver/src/types.rs:738-923` — `TypeData` 26-variant enum

### Checker

- `crates/tsz-checker/src/state/state.rs:34-38` — `CROSS_ARENA_DEPTH` thread-local
- `crates/tsz-checker/src/state/state.rs:52` — `pub struct CheckerState<'a>`
- `crates/tsz-checker/src/state/state.rs:149` — `checker_state_constructed` counter
- `crates/tsz-checker/src/state/state.rs:163-164` — `&'a NodeArena`, `&'a BinderState` borrows
- `crates/tsz-checker/src/state/state.rs:276` — `with_parent_cache_attributed` (T2.1.C target)
- `crates/tsz-checker/src/state/state.rs:302` — `enter_cross_arena_delegation`
- `crates/tsz-checker/src/state/type_analysis/cross_file.rs:393` — `delegate_cross_arena_symbol_resolution`
- `crates/tsz-checker/src/state/type_analysis/cross_file.rs:644` — `delegate_cross_arena_calls` counter
- `crates/tsz-checker/src/state/type_analysis/cross_file.rs:649-727` — early-return paths (`cached_cross_file_symbol_type`, `direct_cross_file_interface_lowering`)
- `crates/tsz-checker/src/state/type_analysis/cross_file.rs:731` — `delegate_cross_arena_misses`
- `crates/tsz-checker/src/state/type_analysis/cross_file.rs:811-867` — child checker construction (T2.1.C eliminates)
- `crates/tsz-checker/src/state/type_analysis/computed/mod.rs:380` — `compute_type_of_symbol_calls` counter site
- `crates/tsz-checker/src/context/mod.rs:209-213` — depth counters (T2.1 hazard #12)
- `crates/tsz-checker/src/context/mod.rs:339,343` — `symbol_types`, `symbol_instance_types` (program-safe)
- `crates/tsz-checker/src/context/mod.rs:361` — `lib_delegation_cache` (program-safe)
- `crates/tsz-checker/src/context/mod.rs:367-396` — six cross-file lookup caches (audit per T2.1 hazard #15)
- `crates/tsz-checker/src/context/mod.rs:400` — `shared_lib_type_cache` (program-safe)
- `crates/tsz-checker/src/context/mod.rs:419` — `request_node_types` (T2.1 hazard #6)
- `crates/tsz-checker/src/context/mod.rs:579,583` — class type caches (T2.1 hazard #9)
- `crates/tsz-checker/src/context/mod.rs:720` — `diagnostics: Vec<Diagnostic>` (T2.1 hazard #4)
- `crates/tsz-checker/src/context/mod.rs:722` — `emitted_diagnostics` (T2.1 hazard #5)
- `crates/tsz-checker/src/context/mod.rs:727` — `no_overload_call_nodes` (T2.1 hazard #14)
- `crates/tsz-checker/src/context/mod.rs:732,739,743` — deferred diagnostic vecs (T2.1 hazard #4)
- `crates/tsz-checker/src/context/mod.rs:747-810,835-840,1188` — recursion guards
- `crates/tsz-checker/src/context/mod.rs:788` — `pending_circular_return_sites` (T2.1 hazard #11)
- `crates/tsz-checker/src/context/mod.rs:803` — `node_resolution_stack` (T2.1 hazard #7)
- `crates/tsz-checker/src/context/mod.rs:809-825` — implicit-any closure tracking (T2.1 hazard #8)
- `crates/tsz-checker/src/context/mod.rs:835,840` — class checking sets (T2.1 hazard #10)
- `crates/tsz-checker/src/context/mod.rs:895` — `instantiation_depth` (T2.1 hazard #13)
- `crates/tsz-checker/src/context/mod.rs:913` — `Rc<EvaluationSession>` (single-thread invariant)
- `crates/tsz-checker/src/context/mod.rs:917` — `recursion_depth` (T2.1 hazard #3)
- `crates/tsz-checker/src/context/mod.rs:1041` — `cross_file_symbol_targets`
- `crates/tsz-checker/src/context/mod.rs:1151` — `current_file_idx`
- `crates/tsz-checker/src/context/core.rs:156-166` — overlay snapshot to children
- `crates/tsz-checker/src/context/core.rs:494` — `set_all_binders`
- `crates/tsz-checker/src/context/core.rs:826` — `set_current_file_idx`
- `crates/tsz-checker/src/context/caches.rs:14` — `node_types: Arc<FxHashMap<u32, TypeId>>`
- `crates/tsz-checker/src/context/constructors.rs:581` — `with_parent_cache`
- `crates/tsz-checker/src/context/constructors.rs:612-615` — "after merge, all binders use global SymbolIds"
- `crates/tsz-checker/src/types/utilities/cycle_guard.rs:40-51` — `CONST_ENUM_VISITED`/`NON_CONST_ENUM_VISITED` thread-locals (T2.1 hazard #1)
- `crates/tsz-checker/src/types/utilities/enum_utils.rs:21-24` — `EVAL_MEMO` thread-local (T2.1 hazard #2)
- `crates/tsz-checker/src/types/utilities/const_enum_eval.rs:23-25` — `CONST_EVAL_MEMO` thread-local (T2.1 hazard #2)

### Solver query caches

- `crates/tsz-solver/src/caches/query_cache.rs:33` — `EvalCacheKey`
- `crates/tsz-solver/src/caches/query_cache.rs:81-85` — `SharedQueryCache` `DashMap` layer (T2.1.D removes)
- `crates/tsz-solver/src/caches/query_cache.rs:329` — local `QueryCache` (RefCell)
- `crates/tsz-solver/src/caches/query_cache.rs:331-365` — 11 local caches (all type-keyed; program-lifetime safe)

### Atom interner

- `crates/tsz-common/src/interner/mod.rs:50-52,406-411` — atom shard-bits encoding (T3.1 atom-remap subtlety)

### CLI driver

- `crates/tsz-cli/src/driver/core.rs:130-147` — `PhaseTimings` struct (T1.2 wires to bench JSON)
- `crates/tsz-cli/src/driver/core.rs:172` — `CompilationResult.phase_timings`
- `crates/tsz-cli/src/driver/core.rs:690` — `pub fn compile()` entry
- `crates/tsz-cli/src/driver/core.rs:930` — `compile_inner()`
- `crates/tsz-cli/src/bin/tsz.rs:1599-1615` — `--extendedDiagnostics` output
- `crates/tsz-cli/src/bin/tsz.rs:1795` — `dump_string()` call site (T1.1 wires JSON)
- `crates/tsz-cli/src/driver/resolution.rs:411,427,1758,1912` — resolver sites (T1.1 hazard #13/#14/#15)

### Perf counters (existing framework)

- `crates/tsz-common/src/perf_counters.rs:53-61` — `enabled_fast()` `OnceLock<bool>`
- `crates/tsz-common/src/perf_counters.rs:71-143` — `CheckerCreationReason` enum
- `crates/tsz-common/src/perf_counters.rs:318-400` — `PerfCounters` struct
- `crates/tsz-common/src/perf_counters.rs:478-507` — `inc`/`add`/`record_max` inline-fn pattern
- `crates/tsz-common/src/perf_counters.rs:613-680` — `dump_string` (T1.1 fills the `n/a` rows)

---

## 11. Glossary

- **Bench-vs-tsgo**: `scripts/bench/bench-vs-tsgo.sh`, the hyperfine-driven
  comparison harness vs `@typescript/native-preview`.
- **Cleaned fixture #6**: `large-ts-repo` with the synthetic `tsgo`-rejected
  cases removed (per the 2026-04-29 bench-integrity note in the original
  architectural plan).
- **CheckerState**: per-file checker world. Today one is constructed at
  `parallel/core.rs:5320`. T2.1 makes it program-lifetime.
- **NodeIndex**: AST-arena-local coordinate. Per CLAUDE.md §6, never use as
  cross-file semantic identity. The 13 🔴 T2.1 hazards exist because some
  caches use `NodeIndex` as a key — those keys collide across files when a
  checker is reused.
- **Overlay**: `cross_file_symbol_targets` snapshot copied into each child
  checker. The `copy_symbol_file_targets_*` counter family in the existing
  `perf_counters.rs` measures this; today recorded at 0 because the data
  has been Arc-snapshotted.
- **Scale cliff**: the inflection point in tsz's wall-time / file-count
  curve, today between ~100 files and ~1000 files.
- **Skeleton**: a stable, post-merge view of declarations and topology that
  doesn't require full binder/arena residency.
- **TSZ_PERF_COUNTERS**: env var that gates perf-counter recording.
  Disabled-path overhead is one load + branch (verified via codegen).
- **Tier 1/2/3/4**: this plan's PR groupings, by leverage on the headline
  large-project number.

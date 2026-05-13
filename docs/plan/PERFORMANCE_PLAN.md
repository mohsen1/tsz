# tsz Performance Plan

---

## 0. Executive Summary

The current large-project baseline is **not established**. The historical
`large-ts-repo` number of about 890 s came from an older tree and must not be
used for target setting until Tier 0 reproduces or replaces it. The previous
headline target of `<= 90 s` is suspended.

The immediate goal is not a wall-time number. The immediate goal is a
reproducible phase split and counter snapshot for `large-ts-repo` and the
scale-cliff fixtures, emitted by a perf-specific build or benchmark harness as
JSON. Architecture work starts only after that decision record lands.

Current guidance:

1. Freeze architecture work until Tier 0 produces fresh data.
2. Complete first-class diagnostics JSON and perf-counter JSON.
3. Use the phase split to choose resolver/source-discovery work or
   checker/cross-file-query work.
4. If checker work dominates, split checker state by lifetime before generic
   pooling, and migrate child-checker cases into typed cross-file queries one
   reason at a time.
5. Keep lib snapshot Phase 2/3 and interner redesign counter-gated.
6. Every project benchmark fixture that currently fails to run (OOM, stack
   overflow, panic, hang, or any non-zero exit before the runner records a
   timing) must eventually pass. A failing fixture is a correctness/scaling
   bug masquerading as a missing data point: it withholds the very baseline
   the rest of this plan depends on, so "we can't measure it" is never an
   acceptable end state. Each currently-failing fixture (e.g. `large-ts-repo`
   OOM/stack-overflow, monorepo-006 cliff failures) must have either an open
   issue with a root-cause hypothesis or a tier-2 task that is expected to
   resolve it; once resolved, the fixture rejoins the standard bench matrix
   and its result is required in PR descriptions that quote large-project
   numbers.

### Parallel PR Coordination (as of 2026-05-13)

- #6260 and #6286 have merged. Before further T2.2 declaration-file edits,
  rebase on `main` and preserve the current conservative `41 -> 40` actual-lib
  baseline unless the PR carries fresh conformance and counter evidence.

---

## 1. Current Main Baseline

This plan is rebased against current `main` as of 2026-05-13 (`b745f6aa40`).
The important fact is that the codebase has moved since the original plan.
Treat the items below as the starting point.

| Area | Current state | Planning consequence |
| --- | --- | --- |
| Large fixture fallback | `scripts/bench/bench-vs-tsgo.sh` already gates the local `~/code/large-ts-repo` fallback behind `TSZ_BENCH_ALLOW_LOCAL_FIXTURE=1`. | T0.1 is verify/audit work, not a fresh implementation task. |
| Diagnostics timing | `PhaseTimings` exists in `crates/tsz-cli/src/driver/core.rs`, and machine-readable diagnostics JSON is wired (#4945 / #4970) with sub-bucket phase splits (`config_discovery_ms`, `source_discovery_ms`, `module_resolution_ms`, `load_libs_ms`). | T0.2 done. Treat T0.2 follow-ups as bench-harness consumption (jq, not text scraping). |
| Perf counters | `TSZ_PERF_COUNTERS` exists in `crates/tsz-common/src/perf_counters.rs`. All `WiredCounters` flags read `true` on main: `delegate_cross_arena`, `checker_construction`, `overlay_copy`, `interner_intern_calls`/`interner_per_kind`, `resolver_lookup`/`resolver_fs_probes`, `compute_type_of_symbol`. The `delegate.calls`/`misses`/`cache_hits_cross_file` trio is wired at all 11 cross-arena construction sites (#5061/#5064/#5069/#5072). `interner_lock_wait` is feature-gated (`perf-counters-timing`); histogram data fills only under that build. | Treat default-release builds as counter-inert; expose JSON output only via perf-specific binaries / `cfg(feature)` gates. Next material wiring requires an attribution-mode bench run, not new code. |
| Program sharing | `ProgramContext` (renamed from `ProjectEnv` in PR 5B) exists in `crates/tsz-checker/src/context/mod.rs` and is built by `crates/tsz-cli/src/driver/check.rs`. It shares arenas, binders, lib contexts, resolved-module maps, skeleton-derived indices, and the shared `DefinitionStore`. | Continue refining `ProgramContext` rather than building a parallel program-level abstraction. |
| Overlay inheritance | `CheckerContext::copy_symbol_file_targets_to_attributed` uses parent snapshots rather than deep overlay copies. | Rewrite the old "overlay duplication" root cause as child-checker construction and local cache cold starts. |
| Cross-file queries | `crates/tsz-checker/src/state/type_analysis/cross_file.rs` already has direct/typed fast paths and per-reason counters while retaining child-checker fallback. | Continue this migration one reason per PR. |

### Historical Large-Project Number

The 890 s `large-ts-repo` source-discovery figure is historical evidence, not a
current baseline. It predates substantial sharing work, including `ProgramContext` (formerly `ProjectEnv`),
skeleton-derived program indices, shared `DefinitionStore` installation, and
Arc-based overlay snapshots.

Do not quote the 890 s number in PR bodies as a current measurement. A PR may
mention it only as historical context and must pair it with the current Tier 0
measurement once available.

---

## 2. Status Table

| Work item | Status | Next action |
| --- | --- | --- |
| Gate local `large-ts-repo` fallback | Done | Audit completed: `LARGE_TS_LOCAL_DIR="${HOME}/code/large-ts-repo"` at `scripts/bench/bench-vs-tsgo.sh:105` is the only local fixture fallback, and it is already gated behind `TSZ_BENCH_ALLOW_LOCAL_FIXTURE=1` (lines 112-114). No other implicit local-fixture paths found. Fixture provenance in diagnostics JSON is the T0.2 piece (#4970). |
| Perf-only diagnostics JSON | Done (#4945, #4970) | T0.2 shipped. `PhaseTimings` sub-buckets (`config_discovery_ms`, `source_discovery_ms`, `module_resolution_ms`, `load_libs_ms`) split in #4970. |
| Perf-only counter JSON | Done (#4948, follow-ups in #4960/#4993/#5009/#5015/#5060/#5061/#5064/#5069/#5072/#5843/#5863) | T0.3 shipped. `interner.intern_calls`/`hits`/`misses` and `resolver.is_file_calls`/`is_dir_calls`/`read_dir_calls` are now wired and exposed in JSON. `lock_wait_histogram_ns` is now wrapped at all interner write paths (#5060) and all cross-arena delegate paths (#5061), still gated on the `perf-counters-timing` cargo feature per §3. The `delegate.calls` / `delegate.misses` / `delegate.cache_hits_cross_file` trio is now wired at all 11 cross-arena child-checker construction sites: 4 in `cross_file.rs` (#5061 + #5064), 7 in non-cross_file paths (ExpandoProperty / CallableTruthiness / CallHelpers / ImportType — #5069 added `calls`, #5072 added `misses`). At every construction site `calls = misses + cache_hits_cross_file + cache_hits_lib` holds, making attribution-mode bench output self-consistent. **#5843 added** the four classification arrays the text dump prints — `delegate_miss_classification` (by_source, by_kind, declaration-file/source-file totals), `alias_shortcut_outcomes`, `direct_interface_lowering_outcomes` — to `PerfCounterSnapshot` JSON. **#5863 added** `cross_file_cache_miss_causes` (4 buckets: `gate_off` / `bucket_empty` / `sentinel_error_unknown` / `type_id_not_interned`) wired into the four reader helpers in `crates/tsz-checker/src/context/cross_file_query.rs`. **#6208 refines #6203** as `source_file_symbol_arena_cache_eligibility_outcomes`, splitting stable-key availability from concrete structural rejections for source-file symbol-arena delegations. **The declaration-file residue naming slice adds** `delegate_declaration_file_miss_residues`, a bounded `(name, kind, source, target_file, count)` table for the remaining declaration-file child-checker tail. **The alias-outcome instrumentation slice adds** `direct_actual_lib_alias_body_outcomes`, splitting the actual-lib alias-body helper into success, conservative name-gate rejection, resolver/definition-store proof failures, and generic-alias rejection. |
| Fresh phase split | Done (refreshed 2026-05-13) | See `docs/plan/perf-runs/2026-05-13-typeenv-arena-direct-attribution.md`. After the #6144 TypeEnvironmentCore arena-direct slice, the cliff remains checker-dominated (monorepo-003..006: check ≈ 97-98 % in attribution mode). `large-ts-repo` remains deferred (previous OOM / stack-overflow blocker); re-measure after one more measured child-checker path is removed or stack behavior is re-audited. |
| Resolver/source-discovery fast path | **Deferred** | Resolver lookups ~1/file, package.json reads ~1/package on cliff. Not on the hot path. Revisit only after T2.2 lands. |
| Checker lifetime split | **Promoted** | T0.4 measured `with_parent_cache_constructed = 1.28 × files` on monorepo-006. The 2026-05-11 attribution run, post-#5090 (`reset_for_next_file` boundary), measures 1.22 × files — a ~5 % drop from the same fixture. **T2.1.A** scaffolding (inventory + shells + reset boundary) is on `perf/master`. **T2.1.B** sequential session-reuse path behind `TSZ_FILE_SESSION_REUSE` shipped at `32d1c20bfe`; the `CheckerContext::switch_to_file` boundary in `crates/tsz-checker/src/context/file_session_reset.rs` clears file-local state while preserving the shared `QueryCache` and program-stable caches. **T2.1.C** parallel session reuse (#5842, merged `ee20f50f0e`) extends the same boundary to the rayon-chunked parallel driver path. **T2.1.D** ("replace the hottest child-checker path with an explicit session lease or typed query") is the next concrete code PR; the data driving the target choice should come from the refreshed attribution run that consumes the #5843/#5863 classification + miss-cause buckets. Decision record: [`perf-runs/2026-05-11-attribution-lock-wait.md`](perf-runs/2026-05-11-attribution-lock-wait.md). |
| Typed cross-file query migration | **Promoted — highest Tier 2 priority** | #6111 landed the first `DelegateCrossArenaSymbol` source-file symbol-arena gateway path. #6144 then removes the dominant `TypeEnvironmentCore` arena-only type-param child-checker path. #6191 converts 96 stable source-file symbol-arena bucket-empty misses into cross-file cache hits on monorepo-006, dropping `DelegateCrossArenaSymbol` from 924 to 828. #6203 classifies the residue: 247 stable source-file keys are cold first reads, 540 are source-file variable symbols outside the current stability proof, and 41 are declaration-file targets. #6212 proves and admits the annotated single-declaration variable slice, dropping the variable-driven `not_class_or_interface` outcome from 540 to 0 and `DelegateCrossArenaSymbol` from 828 to 539 on monorepo-006. #6231 adds a direct source-file interface query for scope-independent stable interfaces, dropping `DelegateCrossArenaSymbol` from 539 to 292 on monorepo-006. #6243 adds a direct source-file variable annotation query for scope-independent annotations and same-file direct interfaces, dropping `DelegateCrossArenaSymbol` from 292 to 41 on monorepo-006. #6260/#6286 route a conservative actual bundled-lib option/registry interface slice through the existing lib resolver, dropping `DelegateCrossArenaSymbol` from 41 to 40 and `checker.with_parent_cache_constructed` from 56 to 55 on monorepo-006. #6314 broadens the proven non-DOM/non-webworker interface slice, dropping `DelegateCrossArenaSymbol` from 40 to 31 and `checker.with_parent_cache_constructed` from 55 to 40 while keeping aliases and value-merged symbols on fallback paths. #6302 adds the first namespace-qualified actual-lib slice for `Intl.CollatorOptions`; its isolated pre-#6314 run dropped `DelegateCrossArenaSymbol` from 40 to 39 and `checker.with_parent_cache_constructed` from 55 to 54. The post-#6314/#6302 refresh measures `DelegateCrossArenaSymbol = 30` and `checker.with_parent_cache_constructed = 39` on monorepo-006. This allowlist-expansion follow-up keeps those main safety gates and the `Intl.CollatorOptions` namespace path, while routing additional iterator/regexp/disposable lib interfaces through `resolve_lib_type_with_params`; its isolated measured branch dropped `DelegateCrossArenaSymbol` from 40 to 30, `checker.with_parent_cache_constructed` from 55 to 33, and `delegate.misses` from 54 to 32. The latest attribution slice names the remaining declaration-file misses as concrete rows; the repeated utility-alias shortcut failed full conformance, so the next alias work should first make the direct/lib delegation cache preserve generic type parameters and then introduce a typed actual-lib alias-body query or canonical `DefinitionStore` entry rather than expanding a name allowlist. The proof/admission stack admits only `Readonly<T>` as the first generic alias slice, removing one measured miss (`checker.with_parent_cache_constructed` 29 -> 28, `delegate.misses` 28 -> 27 on the regenerated monorepo-006 fixture) while leaving other utility aliases on fallback. Decision records: [`perf-runs/2026-05-13-delegate-bucket-empty-attribution.md`](perf-runs/2026-05-13-delegate-bucket-empty-attribution.md), [`perf-runs/2026-05-13-delegate-residue-classification.md`](perf-runs/2026-05-13-delegate-residue-classification.md), [`perf-runs/2026-05-13-delegate-variable-symbol-cache.md`](perf-runs/2026-05-13-delegate-variable-symbol-cache.md), [`perf-runs/2026-05-13-delegate-source-file-direct-interface.md`](perf-runs/2026-05-13-delegate-source-file-direct-interface.md), [`perf-runs/2026-05-13-delegate-source-file-variable-direct.md`](perf-runs/2026-05-13-delegate-source-file-variable-direct.md), [`perf-runs/2026-05-13-delegate-actual-lib-direct.md`](perf-runs/2026-05-13-delegate-actual-lib-direct.md), [`perf-runs/2026-05-13-delegate-intl-lib-direct.md`](perf-runs/2026-05-13-delegate-intl-lib-direct.md), [`perf-runs/2026-05-13-delegate-actual-lib-allowlist-expansion.md`](perf-runs/2026-05-13-delegate-actual-lib-allowlist-expansion.md), [`perf-runs/2026-05-13-delegate-post-lib-residue.md`](perf-runs/2026-05-13-delegate-post-lib-residue.md), [`perf-runs/2026-05-13-delegate-decl-residue-names.md`](perf-runs/2026-05-13-delegate-decl-residue-names.md), [`perf-runs/2026-05-13-actual-lib-readonly-alias-admission-attribution.md`](perf-runs/2026-05-13-actual-lib-readonly-alias-admission-attribution.md). |
| Lib snapshot Phase 2/3 | Demoted | Revive only if lib construction/merge is measured as non-trivial. |
| Interner redesign | **De-prioritised — not contention-bound** | 2026-05-11 attribution run with `--features perf-tools` (transitively enabling `tsz-common/perf-counters-timing`) measured the lock-wait histogram across monorepo-001..006. At the cliff (monorepo-006, 2.4 M intern calls): 97.5 % of waits land in `<100ns`, only 4 observations exceeded `100µs`, and zero exceeded `10ms`. The interner is not contention-bound on the current single-threaded checking workload. Revisit only if a future change introduces parallel checking, multi-worker interning, or a workload that materially shifts the histogram tail. Decision record: [`perf-runs/2026-05-11-attribution-lock-wait.md`](perf-runs/2026-05-11-attribution-lock-wait.md). |

---

## 3. Measurement Model

All performance PRs use two modes.

| Mode | Purpose | Counters | Hyperfine | Comparable to tsgo timing? |
| --- | --- | --- | --- | --- |
| `timing` | Wall time and RSS claims | Off | Warmups plus repeated runs | Yes |
| `attribution` | Explain where time goes | On | One or a few runs | No |

Never compare attribution-mode `tsz` directly against timing-mode `tsgo`.
Counter paths that can call `Instant::now()` must be compiled out of timing
builds or otherwise proven absent from timing profiles.

The JSON interfaces in this section are **not end-user CLI surface**. Default
release builds of `tsz` must not expose `--diagnostics-json`,
`--perf-counters-json`, `TSZ_PERF_COUNTERS` behavior, or equivalent
user-facing/debug surfaces, and must not carry counter timing overhead. Expose
these outputs only through one of:

- a perf-specific binary such as `tsz-perf`
- a benchmark-harness wrapper that links a perf build
- `cfg(feature = "perf-tools")` / similar build-gated flags unavailable in
  normal release artifacts

### Diagnostics JSON

In perf-specific builds, add a harness-only diagnostics JSON output. The exact
spelling can be `tsz-perf --diagnostics-json <path>` or a `cfg`-gated flag on
`tsz`; it must not appear in default release help or normal user builds.

```text
tsz-perf --diagnostics-json <path>
```

Minimum schema:

```json
{
  "schema_version": 1,
  "mode": "timing",
  "tsz": {
    "version": "...",
    "commit": "...",
    "profile": "release"
  },
  "fixture": {
    "name": "large-ts-repo",
    "repo": "mohsen1/large-ts-repo",
    "ref": "e1b22bda18664a507ed0da19c155e0365d585b18",
    "actual_commit": "...",
    "path": "...",
    "local_override": false
  },
  "command_line": ["tsz", "--noEmit", "--project", "tsconfig.json"],
  "phases_ms": {
    "config_discovery": 0,
    "source_discovery": 0,
    "module_resolution": 0,
    "io_read": 0,
    "load_libs": 0,
    "parse_bind": 0,
    "check": 0,
    "emit": 0,
    "total": 0
  },
  "counts": {
    "files": 0,
    "root_files": 0,
    "lib_files": 0,
    "source_bytes": 0,
    "diagnostics": 0
  },
  "rss_peak_bytes": null
}
```

### Perf-Counter JSON

In perf-specific attribution builds, add a harness-only perf-counter JSON
output. This must not ship as default end-user CLI surface.

```text
tsz-perf --perf-counters-json <path>
```

`PerfCounters::snapshot()` should load atomics once into a value object. Text
dumping and JSON dumping should format the same snapshot so they cannot drift.

Counter JSON must distinguish `0` from "not wired." Use `null` for unwired
values and a `wired` map for reviewer clarity.

```json
{
  "schema_version": 1,
  "enabled": true,
  "mode": "attribution",
  "wired": {
    "checker_child_by_reason": true,
    "overlay_by_reason": true,
    "resolver_fs_probes": false,
    "interner_lock_wait": false
  },
  "checker": {
    "state_constructed": 0,
    "with_parent_cache_constructed": 0,
    "with_parent_cache_by_reason": {}
  },
  "delegate": {
    "calls": 0,
    "misses": 0,
    "cache_hits_lib": 0,
    "cache_hits_cross_file": 0,
    "max_recursion_depth": 0
  },
  "resolver": {
    "lookup_calls": 0,
    "is_file_calls": null,
    "is_dir_calls": null,
    "read_dir_calls": null,
    "package_json_reads": null,
    "candidate_paths_total": 0
  },
  "interner": {
    "intern_calls": null,
    "intern_hits": null,
    "intern_misses": null,
    "lock_wait_histogram_ns": null
  }
}
```

---

## 4. Tier 0: Hard Gate

No Tier 2 architecture PR starts until Tier 0 exits.

### T0.1 Fixture Provenance

Status: **done** for the local fallback gate.

- Audit complete: `LARGE_TS_LOCAL_DIR="${HOME}/code/large-ts-repo"`
  (`scripts/bench/bench-vs-tsgo.sh:105`) is the only local fixture
  fallback, already gated behind `TSZ_BENCH_ALLOW_LOCAL_FIXTURE=1`
  (lines 112-114). No other implicit local-fixture paths exist in the
  bench script.
- Fixture provenance is emitted into diagnostics JSON per #4970
  (configured ref, actual checkout SHA, path, repo URL, local override
  flag).
- PR descriptions may use local fixtures only when they explicitly say
  `TSZ_BENCH_ALLOW_LOCAL_FIXTURE=1` was used and do not present that as
  canonical benchmark evidence.

### T0.2 Diagnostics JSON

Implement diagnostics JSON in a perf-specific binary/build path and consume it
from the bench harness with `jq`, not shell text scraping.

Done when:

- One small fixture and one large fixture emit valid schema-versioned JSON.
- The JSON includes phase timings, run metadata, fixture provenance, counts,
  and RSS when available.
- Timing mode does not enable perf counters.
- Default end-user `tsz` release builds expose no diagnostics JSON flag.

### T0.3 Perf-Counter JSON

Implement `PerfCounters::snapshot()`, `PerfCounters::write_json_to()`, and a
perf-build-only way for the harness to request counter JSON.

Done when:

- Attribution mode emits checker, delegate, overlay, resolver, and interner
  sections.
- Unwired buckets are encoded as `null` plus `wired: false`.
- Counter code with `Instant::now()` is compile-time gated or unreachable in
  timing mode.
- Default end-user `tsz` release builds expose no perf-counter JSON flag and
  contain no perf-counter timing path.
- Default release builds either compile out perf-counter hooks or make
  `TSZ_PERF_COUNTERS` inert.

#### Counter Wiring Details

The existing counter framework is useful but incomplete. The next PR should
preserve its cheap disabled path and add a stable snapshot object before adding
JSON formatting.

```rust
pub struct PerfCounterSnapshot {
    pub schema_version: u32,
    pub enabled: bool,
    pub wired: WiredCounters,
    pub delegate: DelegateCounters,
    pub checker: CheckerCounters,
    pub overlay: OverlayCounters,
    pub resolver: ResolverCounters,
    pub interner: InternerCounters,
}

impl PerfCounters {
    pub fn snapshot() -> PerfCounterSnapshot { /* load atomics once */ }
    pub fn write_json_to(path: &Path) -> std::io::Result<()> { /* serde_json */ }
}
```

`dump_string()` should format `PerfCounterSnapshot` rather than loading
atomics directly. That keeps text and JSON output aligned.

Priority counter buckets:

| Bucket | Signal needed | Implementation note |
| --- | --- | --- |
| Checker construction | Total `CheckerState` creation and creation by `CheckerCreationReason`. | Already partially present; expose in JSON with reason names. |
| Delegate recursion | Calls, misses, cache hits, and max recursion depth. | Use an RAII depth guard so early returns and panics unwind correctly. |
| Overlay inheritance | Calls, total inherited entries, max entries, and size buckets. | Keep the current Arc snapshot model; this is to prove copy cost is gone. |
| Resolver filesystem | `is_file`, `is_dir`, `read_dir`, package-json reads, candidate paths. | Prefer a counting filesystem wrapper instead of many inline `inc()` calls. |
| Interner activity | Intern calls, hits, misses, kind breakdown, shard write waits. | Gate any timing calls before `Instant::now()`. |
| `compute_type_of_symbol` | Calls and cache hits. | Needed to tell cache cold-starts from semantic work. |

Counting filesystem wrapper shape:

```rust
pub trait FsProbe {
    fn is_file(&self, path: &Path) -> bool;
    fn is_dir(&self, path: &Path) -> bool;
    fn read_dir(&self, path: &Path) -> std::io::Result<std::fs::ReadDir>;
    fn read_to_string(&self, path: &Path) -> std::io::Result<String>;
}
```

Start with a thin `CountingFs` around the real filesystem. This avoids
sprinkling instrumentation throughout resolver code and gives resolver-cache
work a natural home later.

Delegate recursion guard shape:

```rust
pub struct DelegateDepthGuard(());
thread_local! { static DEPTH: Cell<u32> = const { Cell::new(0) }; }

#[inline]
pub fn enter_delegate() -> DelegateDepthGuard {
    if !enabled_fast() {
        return DelegateDepthGuard(());
    }
    DEPTH.with(|d| {
        let next = d.get() + 1;
        d.set(next);
        record_max(&counters().delegate_max_recursion_depth, next as u64);
    });
    DelegateDepthGuard(())
}

impl Drop for DelegateDepthGuard {
    fn drop(&mut self) {
        if !enabled_fast() {
            return;
        }
        DEPTH.with(|d| d.set(d.get().saturating_sub(1)));
    }
}
```

Lock-wait timing shape:

```rust
#[cfg(feature = "perf-counters-timing")]
#[inline(always)]
pub fn time_shard_write<R>(shard_idx: u32, f: impl FnOnce() -> R) -> R {
    let start = std::time::Instant::now();
    let result = f();
    record_shard_lock_wait_ns(shard_idx, start.elapsed().as_nanos() as u64);
    result
}

#[cfg(not(feature = "perf-counters-timing"))]
#[inline(always)]
pub fn time_shard_write<R>(_shard_idx: u32, f: impl FnOnce() -> R) -> R {
    f()
}
```

The compile-time gate is deliberate. A runtime branch is acceptable for cheap
integer counters, but benchmark timing builds must not pay timestamp costs.

Perf-counter tests should live in separate integration-test binaries because
the enabled flag is cached on first observation:

- `perf_counters_disabled.rs`: env var unset, increments are no-ops.
- `perf_counters_enabled.rs`: env var set before first observation, increments
  are visible.
- `perf_counters_json.rs`: snapshot serializes, deserializes, and preserves
  schema shape.

### T0.4 Phase Split And Decision Record

Run attribution mode against:

- `large-ts-repo`
- monorepo-001 through monorepo-006

Check in a short decision record under:

```text
docs/plan/perf-runs/YYYY-MM-DD-scale-cliff-summary.md
```

The raw JSON may live in GCS, but the checked-in summary must include:

- exact `tsz` commit
- benchmark script commit
- fixture commit and path
- phase split
- top counter buckets
- chosen next tier and why

### T0 Exit Decision Matrix

| Fresh T0 result | Next work |
| --- | --- |
| `source_discovery + module_resolution > 30%` of wall time | Promote T2.0 resolver/source-discovery fast path. |
| `check > 50%` and child-checker construction/miss counters are high | Promote T2.1/T2.2 checker lifetime and typed-query work. |
| `check > 50%`, child-checker counters are low, and interner wait is high | Promote T2.4 interner mitigation. |
| `lib construction/merge > 10%` | Promote only the measured lib merge/snapshot subproblem. |
| No phase dominates | Stop architecture work and capture a sampling profile before changing structure. |

---

## 5. Tier 2.0: Resolver And Source Discovery

This tier is conditional. Start it only if Tier 0 shows source discovery or
module resolution is a dominant fraction of wall time.

### Measurements Needed First

Counters must answer:

- filesystem probes per source file
- repeated negative probes
- package.json reads and unique package.json paths
- directory scans and repeated scans
- module-resolution cache hits and misses by request kind
- time spent in config/source discovery versus module resolution during check

### Likely Safe Work

1. Add a counting filesystem wrapper.
2. Cache positive and negative `is_file` / `is_dir` results for one
   compilation.
3. Cache parsed package metadata by canonical directory.
4. Cache canonical path results when normalization repeats.
5. Add a request-keyed module-resolution cache with a complete key:
   containing file, specifier, import kind, resolution mode, compiler options,
   path mapping options, package mode, and relevant feature flags.

Parallelize resolver work only after repeated probes are removed. Parallel
filesystem pressure can make the problem worse if repeated work remains.

### Exit Criteria

- File list and module answers are unchanged.
- Resolution snapshot tests cover NodeNext, package exports/imports, path
  mapping, JSON imports, `.d.ts` preference, and duplicate package redirects.
- Resolver/source-discovery phase improves on measured fixtures.

---

## 6. Tier 2.1: Lifetime Split Before Pooling

This tier is conditional. Start it only if Tier 0 shows checking and
child-checker construction dominate.

The migration should refine existing `ProgramContext` (formerly `ProjectEnv`),
not bypass it:

```text
ProgramContext (formerly ProjectEnv) — already exists; refine it, do not duplicate.
CheckerContext mixed fields -> ProgramContext + WorkerContext + FileSession + SpeculationScope + LspPersistentCache
CheckerState -> thin owner/borrower of FileSession and query APIs
```

### Lifetime Classes

Use these exact classes in the generated inventory:

| Class | Meaning |
| --- | --- |
| `ProgramStable` | Immutable or logically immutable for one compilation/program version. |
| `WorkerReusable` | Owned by one worker and reusable across file sessions. |
| `FileLocalReset` | Initialized for one file check and reset or dropped before the next file. |
| `SpeculationScoped` | Must roll back when overload/generic/speculative checking aborts. |
| `DiagnosticsOnly` | Affects reporting or suppression but not type answers. |
| `LspPersistent` | Survives requests and is invalidated by document/project version. |
| `Unknown` | CI failure. |

### Generated Field Inventory

Add a manifest next to the checker context, for example:

```toml
# crates/tsz-checker/src/context/checker_context_lifetimes.toml
[all_arenas]
lifetime = "ProgramStable"
reason = "shared immutable program arenas"

[request_node_types]
lifetime = "FileLocalReset"
reason = "keyed by NodeIndex for the current file"
```

Add a guard script that parses `CheckerContext` fields and fails if any field
is missing from the manifest. The script should also generate a markdown table
for PR review.

### Detailed Reset Hazards

The old plan carried a useful hand audit. Keep it as review context, but do
not treat it as the source of truth; the generated inventory is the source of
truth. Every item below should be represented in the manifest or explicitly
superseded by a better classification.

| Area | State | Risk | Required handling |
| --- | --- | --- | --- |
| Const enum cycle guards | `CONST_ENUM_VISITED`, `NON_CONST_ENUM_VISITED` thread-locals keyed by `NodeIndex`. | A reused worker can suppress or mis-detect cycles in the next file. | Clear per file on the same thread. |
| Enum evaluation memo | `EVAL_MEMO` and `CONST_EVAL_MEMO` thread-locals. | Values can be keyed by file-local nodes. | Clear per file unless proven fully program-keyed. |
| Diagnostic buffers | `diagnostics`, callback return errors, truthiness diagnostics, excess-property implicit-any diagnostics. | Diagnostics can leak into or suppress later files. | Drain and clear at every `FileSession` boundary. |
| Emitted diagnostic set | `emitted_diagnostics` keyed by positions. | File 2 diagnostics can be suppressed by file 1 positions. | Clear per file. |
| Request node types | `request_node_types` keyed by `(u32, RequestCacheKey)`. | `u32` is a node index and collides across files. | Clear per file or rekey by `(FileId, NodeIndex, RequestCacheKey)`. |
| Resolution stacks | `node_resolution_stack`, import/symbol resolution stacks and sets. | Reuse can create false recursion. | Clear per file; debug assert empty after each file. |
| Implicit-any tracking | checked/contextual/deferred/speculative closure sets keyed by nodes. | Suppresses or replays errors in the wrong file. | Clear per file and rollback speculation separately. |
| Class caches | class instance/constructor caches keyed by `NodeIndex`. | Returns a class type for a node in another file. | Clear per file or rekey with file identity. |
| Class checking sets | `checking_classes`, `checked_classes`. | False recursion or skipped class checks. | Clear per file. |
| Circular return sites | `pending_circular_return_sites` containing `NodeIndex` values. | Stores file-local nodes inside symbol-keyed state. | Clear per file or replace payload with stable declaration IDs. |
| Depth counters | call/circular/overlap/recursion/instantiation depths. | Bad depth values can suppress work or trigger false TS2589-like behavior. | Reset at session boundaries and after speculation rollback. |
| No-overload call nodes | `no_overload_call_nodes` keyed by node id. | Wrong call gets no-overload suppression. | Clear per file. |
| Cross-file lookup caches | string/SymbolId keyed lookup caches. | Some are safe, some may encode current-file context indirectly. | Audit keys before moving to `ProgramStable` or `WorkerReusable`. |

Fields expected to be safe across files, assuming stable symbol identity:

- symbol type caches keyed by `SymbolId`
- lib delegation caches keyed by `SymbolId`
- shared lib type caches keyed by stable strings
- global/module indices installed by `ProgramContext`
- current file index, if assigned explicitly for every session

Reset helper shape:

```rust
impl CheckerContext<'_> {
    pub fn reset_for_next_file(&mut self) {
        self.diagnostics.clear();
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
        self.call_depth.borrow_mut().reset();
        self.circ_ref_depth.borrow_mut().reset();
        self.overlap_depth.borrow_mut().reset();
        self.recursion_depth.borrow_mut().reset();
        self.instantiation_depth.set(0);

        crate::types::utilities::cycle_guard::clear_visited_sets();
        crate::types::utilities::enum_utils::clear_enum_eval_memo();
        crate::types::utilities::const_enum_eval::clear_const_eval_memo();

        debug_assert!(self.symbol_resolution_stack.is_empty());
        debug_assert!(self.symbol_resolution_set.is_empty());
        debug_assert!(self.import_resolution_stack.is_empty());
    }
}
```

The helper above is illustrative. The real implementation should come after
the generated inventory, so newly added fields cannot escape classification.

### QueryCache Lifetime Audit

The solver `QueryCache` contains local caches keyed by stable solver values
such as `TypeId`, `RelationCacheKey`, `DefId`, and `Atom`. These are candidates
for `WorkerReusable` only because current project checking uses one program
type interner. If a future design moves to checker-local interners, this
classification must be revisited.

Audit before moving query caches:

- every cache key must be independent of `NodeIndex`
- `DefId` entries must be stable across files
- variance cache entries must not depend on a per-file `def_type_params` view
- relation/evaluation caches must include all compiler-option bits that affect
  answers
- shared `DashMap` cache layers should be removed only after local
  worker-lifetime caches prove sufficient

### Migration Order

1. Field-lifetime inventory and CI guard.
2. ~~`ProjectEnv` -> `ProgramContext` no-behavior refactor.~~ Done in PR 5B.
3. Add accessors so call sites can move gradually from direct field reads to
   lifetime-owned state.
4. Introduce `WorkerContext`, initially with only obvious reusable scratch.
5. Introduce `FileSession` reset boundaries after fields are classified.
6. Consider generic checker pooling only if counters still show construction
   or reset costs after typed-query migration.

Do not add `unsafe impl Send` or `unsafe impl Sync` for checker state as part
of this migration. Prefer scoped worker ownership and explicit borrows.

### Staged Checker PRs

Keep the risk units small:

| PR | Scope | Verification |
| --- | --- | --- |
| T2.1.A | Add field inventory, manifest, `ProgramContext`/`WorkerContext`/`FileSession` shells. Move only obvious `ProgramStable` fields. | CI fails on unknown fields; no behavior change. |
| T2.1.B | Add a sequential session-reuse path behind a flag. | Full conformance with flag produces byte-identical diagnostics to default path. |
| T2.1.C | Introduce scoped worker ownership, each worker owning a `WorkerContext`. | Attribution JSON shows construction/reset counters move in the expected direction. |
| T2.1.D | Replace the hottest child-checker path with an explicit session lease or typed query. | Target `CheckerCreationReason` count drops; fallback remains; conformance is unchanged. |

The lease protocol must save and restore caller state, run the target-file
query, and return only stable program values such as `TypeId`, `SymbolId`,
`DefId`, or copied diagnostics. Borrowed AST nodes and raw `NodeIndex` values
must not cross the lease boundary.

---

## 7. Tier 2.2: Typed Cross-File Queries

This is the preferred checker-side way to reduce recursive child-checker
construction. Current `cross_file.rs` already has the beginning of this shape:
direct alias/interface fast paths, global cache checks, per-reason counters,
and child-checker fallback.

### Principle

A cross-file query is a pure request for a typed answer from another file. It
is not "construct a new checker world and inherit side effects."

Suggested API shape:

```rust
#[derive(Clone, Copy, Hash, PartialEq, Eq)]
pub enum CrossFileQueryKind {
    SymbolType,
    ClassInstanceType,
    InterfaceType,
    InterfaceMemberSimpleType,
}

#[derive(Clone, Hash, PartialEq, Eq)]
pub struct CrossFileQueryKey {
    pub kind: CrossFileQueryKind,
    pub target_file_idx: u32,
    pub symbol_id: SymbolId,
    pub request_key: Option<RequestCacheKey>,
    pub options_fingerprint: u64,
}

pub enum CrossFileQueryAnswer {
    Type(TypeId),
    TypeWithParams(TypeId, Vec<TypeParamInfo>),
    MemberType { member: Atom, ty: TypeId },
    Unknown,
    Error,
}
```

### Migration Order

Use counter data to choose the actual order. The likely safe order is:

1. Alias-only symbol resolution.
2. Direct interface type lowering.
3. Class instance type.
4. Import type resolution.
5. Call helpers, callable truthiness, and expando cases.

Each PR:

- targets one `CheckerCreationReason`
- records rejection/fallback reasons
- keeps child-checker fallback
- proves diagnostics are unchanged
- shows the target reason's construction count drops

### Cache Key Requirements

Typed-query cache keys must include every input that changes the answer:

- target file index
- symbol ID or stable declaration location
- query kind
- request cache key / contextual origin when applicable
- import resolution mode when applicable
- relevant compiler-options fingerprint
- lib/program version fingerprint

Too-small keys are high-risk correctness bugs.

---

## 8. Tier 2.3: One Lib-Symbol Merge Per Program

This tier is conditional. Do not assume lib merge is still dominant.

Measure first:

- lib contexts built
- lib binder clones
- lib symbol remaps/merges per source file
- time spent in lib merge/build
- size of lib symbol tables and declaration arenas

If still non-trivial, the target architecture is:

```text
build lib/program global symbol surface once
share immutable surface from ProgramContext
per-file sessions borrow it
local/global augmentations overlay it
```

Gate this work on fresh attribution showing lib construction or merge is at
least 10% of wall time, or remains visible after T2.1/T2.2 improvements.

---

## 9. Tier 2.4: Interner Instrumentation Before Redesign

The interner concern is plausible but unproven. Current code uses sharded
storage, `DashMap`, reverse arrays guarded by locks, thread-local lookup and
intern caches, and a 64-shard layout. That may or may not be the active
bottleneck.

Measure:

- total intern calls
- hits and misses
- misses by `TypeData` kind
- shard distribution
- write-lock wait histogram by shard
- reverse-vector write counts
- TLS cache hit rate
- `TypeId`s allocated per file and per phase

Prefer low-risk mitigations first:

- pre-size shards if reallocation is visible
- improve hit paths before write paths
- reduce duplicate type construction upstream
- benchmark lock alternatives only after contention is measured

Avoid per-worker local interning unless global interning is conclusively
dominant and simpler fixes fail. `TypeId` identity flows through too many
caches and diagnostics to make local merging a first-line option.

### Interner Redesign Guardrails

Do not start by changing `TypeId` packing. If contention is measured, try
lower-risk changes first:

1. Pre-size hot shards from measured type counts.
2. Improve lookup/intern TLS hit rates.
3. Reduce upstream duplicate type construction.
4. Compare lock implementations under attribution builds.
5. Only then consider storage layout changes.

Any layout change must preserve:

- reserved built-in `TypeId` range
- stable reverse lookup
- relation/query-cache key correctness
- diagnostic display stability
- conformance output

### What Tier 2 Does Not Solve

Tier 2 must not silently absorb unrelated work:

- Resolver syscall topology is handled by T2.0 only when measurement promotes
  it. Checker lifetime work should not also rewrite module resolution.
- Pre-check bind/merge hotspots need their own attribution before changes.
- Checker-local interners are out of scope unless global interner contention
  remains dominant after lower-risk fixes.
- Out-of-order scheduling and work stealing are separate scheduling projects.
- Speculative overload/generic rollback semantics are correctness-critical;
  lifetime splitting must preserve them before trying to optimize them.

---

## 10. Tier 3: Small-Fixture Polish And Lib Snapshots

Tier 3 is demoted. Ship it only if Tier 0 shows lib construction, lib merge,
or small-fixture overhead is worth the review cost.

### T3.1 Lib Snapshot Phase 2/3

Phase 1 caches parse and bind state for standard library files. Phase 2/3
would cache more of the populated type-interner state so small projects can
skip repeated lib type construction. This is not part of the current headline
until measurement says lib work still matters.

If revived, do not serialize live internal structs with derived serde and
assume compatibility. Postcard is not self-describing; it tags enum variants
by index. Sender and receiver must share a schema. Use explicit versioned
snapshot structs and manual encode/decode:

```rust
#[derive(Serialize, Deserialize)]
pub struct LibSnapshotV1 {
    pub schema_version: u32,
    pub tsz_semantic_layout_version: u32,
    pub typescript_lib_version: String,
    pub compiler_target: String,
    pub lib_file_set_hash: String,
    pub payload: LibSnapshotPayloadV1,
}
```

#### Format Choice

Postcard remains a reasonable binary candidate because the interner is mostly
small integer IDs (`TypeId`, shape IDs, `DefId`, `Atom`) and varint encoding
should be compact. The correction is that postcard does not solve schema
evolution by itself.

Format guidance:

| Candidate | Why consider it | Rejection / caution |
| --- | --- | --- |
| postcard | Compact varints, no mmap or unsafe requirement, serde-compatible. | Not self-describing; must use explicit versioned structs and golden tests. |
| rkyv | Potential zero-copy reads. | The live interner still needs allocated maps, so zero-copy likely evaporates; adds alignment and format-stability risk. |
| bincode 1.x | Already familiar in the tree. | Fixed-width integer size and known field-skip desync hazards. |
| bincode 2 | Viable backup if postcard underperforms. | Still needs explicit schema/version discipline. |

#### Snapshot Payload

Persist only data that is expensive and stable enough to justify versioning:

- type interner shards and reverse lookup data
- string interner contents needed by atoms embedded in type data
- type lists, tuple lists, and template lists
- object, function, callable, conditional, mapped, and application shapes
- lib-derived global handles such as boxed types, `ThisType` marker IDs, and
  array base/display types

Skip or rebuild lazily:

- identity-comparable caches
- `contains_this` caches
- display-only properties and alias display maps
- display union origins
- object-property maps
- thread-local lookup/intern caches

Reset on load:

- allocation counters to the high-water mark
- fresh interner instance ID
- runtime flags derived from compiler options
- poison/fuel flags
- any profile/debug-only counters

#### Atom Remapping

`Atom` values encode shard information. A snapshot cannot assume raw atom IDs
are stable across process runs or thread schedules. Serialize strings in a
deterministic order, intern them on load, and build an `OldAtom -> NewAtom`
remap. Walk all snapshot payloads and rewrite embedded atoms through the table
before installing them.

`TypeId` needs a separate invariant: if IDs are expected to round-trip, the
loader must install type data in a deterministic order and verify reserved
built-in IDs before trusting the snapshot.

#### Versioning And Cache Key

Snapshot invalidation must include:

- snapshot schema version
- tsz semantic data-layout version
- TypeScript/lib asset version
- compiler target/lib list
- DOM replacement package identity if relevant
- flags that affect lib parse/bind/check surface

Avoid storing fields that are cheap to rebuild and fragile to version.

Cache key shape:

```text
cache_key = hash(
    "tsz-libsnap-v1",
    snapshot_magic_version,
    snapshot_schema_version,
    builtin_typeid_layout_version,
    lib_file_count,
    sorted(lib_file_name, lib_file_content_hash),
    compiler_options_hash
)
```

Use a stable content hash rather than a process-seeded hasher. Include enough
layout information to reject stale cache files before reading IDs as trusted.

#### Load Safety

The loader should:

- validate magic and schema before decoding payloads
- validate built-in `TypeId` layout before installing payloads
- treat malformed or panicking deserialize paths as a cache miss
- fall back to fresh lib parse/bind/check on any mismatch
- support an explicit `TSZ_LIB_CACHE=0` override if the default ever flips on
- warn when the cache directory exceeds a documented size budget

#### Snapshot Tests

Required before default-on:

- round-trip arbitrary generated interner data through capture/install
- atom remap test with different prior atom-interner contents
- schema-version mismatch returns cache miss
- corrupt payload returns cache miss rather than panic
- snapshot-on and snapshot-off conformance output are byte-identical
- small-fixture benchmark shows a real wall-time win in timing mode

### T3.2 Other Small-Fixture Work

Keep these behind attribution data:

| Work | Expected benefit | Risk |
| --- | --- | --- |
| Lazy `ObjectShape` hash caching | Avoid repeated per-property shape hashing. | Must invalidate or make shapes immutable. |
| `walk_referenced_types` allocator reuse | Reduce temporary `Vec`/set churn. | Thread-local pools must not leak state across checks. |
| `collect_comment_at` cache | Avoid repeated JSDoc/comment scans. | Needs stable source-position keys. |
| Shape clone elimination in subtype dispatch | Reduce clone-heavy hot paths. | Some clones may be load-bearing for cache keys. |

---

## 11. Recommended PR Sequence

### PR 1: Diagnostics JSON

Goal: stable machine-readable phase timings and run metadata.

Changes:

- Add diagnostics JSON to a perf-specific build or benchmark harness path.
- Do not expose diagnostics JSON as normal end-user `tsz` CLI surface.
- Emit phase timings and run metadata from the perf build.
- Include fixture provenance.
- Teach the bench script to consume the JSON.

Done when:

- JSON schema has a version.
- One small fixture and one large fixture emit valid JSON.
- Timing mode does not enable perf counters.
- Default release `tsz --help` does not show diagnostics JSON options.

### PR 2: Perf-Counter JSON

Goal: expose existing counter data reliably.

Changes:

- Add `PerfCounters::snapshot()` and `write_json_to()`.
- Add a perf-build-only counter JSON output for the benchmark harness.
- Do not expose counter JSON as normal end-user `tsz` CLI surface.
- Add `wired` metadata.
- Encode unwired buckets as `null`.
- Separate attribution mode from timing mode.

Done when:

- Attribution mode emits checker/delegate/overlay/resolver/interner JSON.
- Timing mode does not call expensive counter code.
- Default release `tsz --help` does not show perf-counter JSON options.
- Default release builds do not honor `TSZ_PERF_COUNTERS`.

### PR 3: Attribution Run And Decision Record

Goal: choose the next architecture path from data.

Changes:

- Run `large-ts-repo` and monorepo-001..006 in attribution mode.
- Publish JSON artifacts.
- Check in a short summary under `docs/plan/perf-runs/`.

Done when:

- The plan states whether source discovery, checking, lib work, or interner
  contention is dominant.
- T2.0/T2.1/T2.2 priority is selected from measured data.

### PR 4A: Resolver Fast Path

Run only if T0 says discovery/resolution dominates.

Done when:

- File list and module answers are unchanged.
- Resolver/source-discovery phase improves on measured fixtures.

### PR 4B: Checker Field Inventory

Run only if T0 says checking/child-checkers dominate.

Done when:

- Every `CheckerContext` field is classified.
- CI fails on unclassified fields.
- Reviewers get a generated markdown inventory.

### PR 5B: ~~`ProjectEnv` -> `ProgramContext`~~ — done

No-behavior rename shipped: the program-stable layer is now spelled
`ProgramContext` everywhere. Conformance unchanged; no new unsafe
thread-safety implementations introduced.

### PR 5C: ~~Counter JSON classification + miss-cause~~ — done

Two follow-ups on top of #4948 (T0.3) that turn flat counters into
structurally classified data so the bench harness can pick the next
architecture target programmatically:

- **#5843** added `delegate_miss_classification` (by_source / by_kind /
  declaration-file / source-file totals), `alias_shortcut_outcomes`,
  and `direct_interface_lowering_outcomes` to `PerfCounterSnapshot`
  JSON. These were already in the text dump; the JSON parity closes
  the bench-consumer gap.
- **#5863** added `cross_file_cache_miss_causes` (4 buckets:
  `gate_off` / `bucket_empty` / `sentinel_error_unknown` /
  `type_id_not_interned`) wired into all four reader helpers in
  `crates/tsz-checker/src/context/cross_file_query.rs`. Splits the
  load-bearing `cache_hits_cross_file = 0` figure into its
  structural root causes.
- The declaration-file residue naming slice adds
  `delegate_declaration_file_miss_residues`, a bounded table of
  `(name, kind, source, target_file, count)` rows for declaration-file
  `DelegateCrossArenaSymbol` misses after all current direct paths decline.
- The actual-lib alias-body outcome slice adds
  `direct_actual_lib_alias_body_outcomes`, splitting the typed alias-body
  helper into success, conservative name-gate rejection, resolver /
  `DefinitionStore` proof failures, and generic-alias rejection before the
  next canonical alias PR widens behavior.

Schema version stays at `1` — pure additive extensions.

### PR 6B+: Typed Cross-File Query PRs

Goal: reduce child-checker construction without generic pooling.

Done when:

- The target reason's child-checker count drops.
- Diagnostics stay stable.
- Fallback remains for unsupported cases.

**Sequencing note:** the post-#5863 attribution run showed the
`cross_file_cache_miss_causes` table is present but all-zero on the
scale-cliff fixtures because the hot `DelegateCrossArenaSymbol` path
bypasses the canonical `cached_cross_file_*` readers. The next 6B PR
should target symbol-arena-sourced source-file delegations first:
route them through the canonical cross-file query bucket, or add one
smaller counter if review needs a preparatory slice. Only use the
miss-cause table to choose between `gate_off`, `bucket_empty`, and
`type_id_not_interned` after the hot path actually reaches those
reader helpers.

**2026-05-13 post-#6111 update:** the symbol-arena source-file slice landed in
#6111 and routes proven single-declaration, non-generic class/interface
declarations through the `SymbolType` bucket when the program has no module
augmentations. Source-file symbol-arena entries use requester-file and
program-local scope key slots because answers can depend on caller context and
bare `(file_idx, SymbolId)` values can be reused by unrelated virtual programs
in the same process. The post-merge attribution refresh shows the final
requester-scoped key makes the path observable (`bucket_empty = 343` on
monorepo-006) but does not produce reusable batch `delegate.cache_hits_cross_file`
hits. `TypeEnvironmentCore` remains the largest child-checker reason, so #6144
is the next active typed-query/lifetime slice.

**2026-05-13 post-#6144 update:** the TypeEnvironmentCore arena-direct slice
drops monorepo-006 `TypeEnvironmentCore` child-checker constructions from 5,259
to 1, and `with_parent_cache_constructed` from 6,197 to 939. The remaining
largest measured child-checker reason is `DelegateCrossArenaSymbol` (924 on
monorepo-006), so the next T2.2 slice should return to the #6111 residue and
its `cross_file_cache_miss_causes.bucket_empty` signal.

**2026-05-13 post-#6191 update:** the first `DelegateCrossArenaSymbol`
follow-up keeps the program scope key for stable source-file symbol-arena
results but removes requester-file scoping for the proven
single-declaration class/interface subset. On monorepo-006 this produces 96
`delegate.cache_hits_cross_file` hits, drops `DelegateCrossArenaSymbol` from 924
to 828, and reduces `bucket_empty` from 343 to 247. The remaining T2.2 work
should classify the leftover bucket-empty probes and the non-cacheable
symbol-arena misses.

**2026-05-13 post-#6208 update:** the residue classification refines #6203 by
exposing one schema,
`source_file_symbol_arena_cache_eligibility_outcomes` to
`PerfCounterSnapshot` JSON and perf-counter text. It keeps #6203's stable-key
availability signal, but splits structural rejections into concrete reasons
such as `not_class_or_interface`, `multiple_declarations`, and
`declaration_arena_mismatch`. On the #6203 monorepo-006 run, the 828 remaining
`DelegateCrossArenaSymbol` child-checker constructions split into 247 stable-key
cold reads, 540 source-file variable symbols outside the current stability
proof, and 41 declaration-file targets. The next implementation target remains
the variable-symbol slice, gated on a requester-independence proof.

**2026-05-13 post-#6260/#6286 update:** the actual bundled-lib direct path now
handles a non-DOM/non-webworker interface slice of the remaining
declaration-file residue through the existing lib resolver instead of
constructing a child checker. On monorepo-006 this drops
`DelegateCrossArenaSymbol` from 41 to 31, `checker.with_parent_cache_constructed`
from 56 to 40, and `delegate.misses` from 55 to 39 with unchanged diagnostic
count. The next implementation target is the 31 remaining declaration-file
misses: 16 type aliases plus 15 interfaces that need namespace-qualified,
merged-lib, or conformance-backed proof before admission.

**2026-05-13 post-#6260 allowlist expansion update:** the allowlist now admits
additional iterator/regexp/disposable lib interfaces and routes those names
through `resolve_lib_type_with_params` in the same direct path. On monorepo-006
this drops `DelegateCrossArenaSymbol` from 40 to 30, lowers
`checker.with_parent_cache_constructed` from 55 to 33, and reduces
`delegate.misses` from 54 to 32 with unchanged diagnostic count. The next
implementation target is the remaining declaration-file misses: type aliases
plus interfaces that still need namespace-qualified, merged-lib, or
conformance-backed proof before admission.

**2026-05-13 declaration-file residue naming update:** the attribution JSON now
includes `delegate_declaration_file_miss_residues`, a bounded table of
`(name, kind, source, target_file, count)` rows for declaration-file
`DelegateCrossArenaSymbol` misses. On current-main monorepo-006 the field
reports 27 distinct rows accounting for 30 remaining declaration-file children.
The repeated utility aliases are `FlatArray`, `IteratorResult`, and `Record` (2
each); the one-off rows are the `lib.es5.d.ts` global interfaces/utility
aliases, iterator interfaces, `Intl` option surfaces, and decorator metadata
aliases listed in
`docs/plan/perf-runs/2026-05-13-delegate-decl-residue-names.md`. The next PR
should target a concrete subset from that table rather than relying on the
aggregate 16-alias / 14-interface split.

**2026-05-13 utility-alias safety update:** the direct shortcut for repeated
actual-lib aliases (`FlatArray`, `IteratorResult`, `Record`) failed full
conformance, with regressions in iterator-return and `Record`-driven
mapped/assignability cases. The next safe slice is preparatory: preserve generic
type-parameter metadata in `lib_delegation_cache` for the next typed query while
leaving the current cache-hit return contract unchanged. Actual-lib aliases
remain on the fallback path until a typed alias-body query or canonical
`DefinitionStore` entry proves the alias shape without relying on a name
allowlist. Claim:
[`claims/perf-lib-delegation-cache-type-params-2026-05-13.md`](claims/perf-lib-delegation-cache-type-params-2026-05-13.md).

**2026-05-13 typed alias-body query follow-up:** the next implementation slice
starts that typed-query path without reopening the generic utility-alias
shortcut. After the broader non-generic alias attempt failed conformance in
assignability-sensitive aliases such as `PropertyKey`, this slice admits only
the decorator metadata aliases (`DecoratorMetadata` and
`DecoratorMetadataObject`). Their declarations are proven to come from the
bundled lib and the existing lib resolver must return a `Lazy(DefId)` with a
registered `DefinitionStore` body; the direct path then caches the registered
body rather than the opaque alias wrapper. Generic aliases such as `Record` and
conformance-sensitive non-generic aliases such as `PropertyKey` still return
`None` and stay on the child-checker fallback path. On monorepo-006 this drops
`DelegateCrossArenaSymbol` child-checkers from 28 to 26,
`checker.with_parent_cache_constructed` from 31 to 29, and `delegate.misses`
from 30 to 28 with unchanged diagnostics (`10,198`). Decision record:
[`perf-runs/2026-05-13-delegate-actual-lib-alias-body-query.md`](perf-runs/2026-05-13-delegate-actual-lib-alias-body-query.md).
Claim: [`claims/perf-actual-lib-alias-body-query-2026-05-13.md`](claims/perf-actual-lib-alias-body-query-2026-05-13.md).

**2026-05-13 `compute_type_of_symbol` interface fast path:** for local
single-declaration interfaces, we now skip three high-frequency costs when not
needed: computed-name precompute maps, member type-parameter prewarm scans, and
heritage merging when there is no local `extends` clause. On monorepo-006
attribution mode, this preserves diagnostics (`10,198`) and call buckets
(`total calls = 26,370`, `interface = 24,781`) while reducing warm-run check
time from `80.69s` to `79.60s` (`-1.35%`) and total time from `82.36s` to
`81.25s` (`-1.35%`). Decision record:
[`perf-runs/2026-05-13-compute-type-of-symbol-interface-fastpath.md`](perf-runs/2026-05-13-compute-type-of-symbol-interface-fastpath.md).

**2026-05-13 `compute_type_of_symbol` interface fast-path outcomes:** new
interface-branch counters split which skip-combination fired per interface call.
On monorepo-006, `skip_all_three` accounts for `24,767 / 24,796` interface
calls (`99.88%`), with only 18 non-`skip_all_three` rows total. This says the
current skip gates are already saturating and the next meaningful reduction
should target interface cold-call volume / lowering cost instead of more gate
tuning. Decision record:
[`perf-runs/2026-05-13-compute-type-of-symbol-interface-fastpath-outcomes.md`](perf-runs/2026-05-13-compute-type-of-symbol-interface-fastpath-outcomes.md).

**2026-05-13 `compute_type_of_symbol` interface call-site outcomes:** new
call-site counters classify interface calls by parent symbol kind in the
resolution stack. On monorepo-006, root calls dominate
(`24,782 / 24,796`, `99.94%`) while nested parent-interface calls are tiny
(`14`, `0.06%`) and all other parent-kind buckets are zero. This narrows the
next optimization lane to reducing top-level/root interface demand, not
interface-to-interface recursion tuning. Decision record:
[`perf-runs/2026-05-13-compute-type-of-symbol-interface-callsite-outcomes.md`](perf-runs/2026-05-13-compute-type-of-symbol-interface-callsite-outcomes.md).

**2026-05-13 `compute_type_of_symbol` simple-local-interface shortcut:** the
interface branch now has an early direct-lowering path for local
single-declaration interfaces with no local `extends`, no computed property
names, no type parameters, property-signature-only members, a non-empty member
list, and only primitive keyword member annotations. The original broader
shortcut measured a large monorepo-006 win (`95.75s -> 84.24s` total), but it
also admitted empty interfaces and annotations that require hybrid type
resolution; after targeted unit failures, the guarded branch now falls back to
the full interface lowering path for those cases. Treat the original timing as
historical broad-shortcut evidence. Decision record:
[`perf-runs/2026-05-13-compute-type-of-symbol-interface-simple-local-object-fastpath.md`](perf-runs/2026-05-13-compute-type-of-symbol-interface-simple-local-object-fastpath.md).

**2026-05-13 `compute_type_of_symbol` simple-local-interface hit counter:** a
new checker scalar counter,
`checker.compute_type_of_symbol_interface_simple_object_fastpath_hits`, now
records every interface-symbol call that returns through the simple local-object
shortcut. The original broad shortcut reported `24,760` hits against `24,796`
interface-kind calls (`99.85%`); this is no longer the guarded-branch baseline.
Keep the counter as the direct guardrail for future interface root-demand or
lowering-cost edits.
Decision record:
[`perf-runs/2026-05-13-compute-type-of-symbol-interface-simple-local-object-hit-counter.md`](perf-runs/2026-05-13-compute-type-of-symbol-interface-simple-local-object-hit-counter.md).

**2026-05-13 `compute_type_of_symbol` simple-object outcome buckets:** a new
named outcome array,
`compute_type_of_symbol_interface_simple_object_outcomes`, classifies why the
shortcut succeeded or rejected per interface call. On monorepo-006, `success`
is `24,760 / 24,796` (`99.85%`); the active reject residue is tiny and concrete
(`reject_out_of_arena_decl=16`, `reject_missing_interface_decl=7`,
`reject_declaration_count=1`, `reject_heritage_extends=1`, all others zero).
This narrows future shortcut-expansion work to those live buckets and avoids
spending time on inactive gates. Decision record:
[`perf-runs/2026-05-13-compute-type-of-symbol-interface-simple-object-outcomes.md`](perf-runs/2026-05-13-compute-type-of-symbol-interface-simple-object-outcomes.md).

**2026-05-13 guarded simple-local-object rerun:** monorepo-006 has now been
remeasured on the guarded branch
([`perf-runs/2026-05-13-compute-type-of-symbol-interface-simple-local-object-guarded-rerun.md`](perf-runs/2026-05-13-compute-type-of-symbol-interface-simple-local-object-guarded-rerun.md)).
The counter signal is stable across two runs: `checker.compute_type_of_symbol_interface_simple_object_fastpath_hits = 0` and
`compute_type_of_symbol_interface_simple_object_outcomes.success = 0`.
The active residue remains `reject_out_of_arena_decl=16`,
`reject_missing_interface_decl=7`, `reject_declaration_count=1`,
`reject_heritage_extends=1`. Timing varied under shared-runner contention
(`78.12s/76.36s` then `103.68s/100.79s` total/check), so treat this rerun as a
counter-baseline refresh, not a timing claim. Next step: either conformance-
proven guard relaxation that restores meaningful `success`, or dead-path
simplification if the shortcut remains inactive.

**2026-05-13 alias-body outcome instrumentation follow-up:** before admitting
any more aliases, add `direct_actual_lib_alias_body_outcomes` to the perf
counter JSON/text dump and wire it at every return point in the actual-lib
alias-body helper. This records whether the typed alias proof succeeded,
stopped at the current conservative name gate, lacked a resolver or
`DefinitionStore` body proof, or rejected a generic alias. The field is
behavior-neutral; its purpose is to make the canonical generic-aware alias
query/application PR measurable without expanding the allowlist first. Claim:
[`claims/perf-actual-lib-alias-body-outcomes-2026-05-13.md`](claims/perf-actual-lib-alias-body-outcomes-2026-05-13.md).

**2026-05-13 alias-body proof result follow-up:** the next stacked slice keeps
the same decorator-only behavior but changes the internal alias-body helper to
return a typed proof object containing the proven body, `DefinitionStore`
`DefId`, alias type parameters, and the proof outcome. The current caller still
destructures that object back into the same `(TypeId, Vec<TypeParamInfo>)`
return and still keeps
generic aliases and `PropertyKey` on fallback. This separates resolver/body
proof plumbing from the later generic alias application PR. Claim:
[`claims/perf-actual-lib-alias-body-proof-result-2026-05-13.md`](claims/perf-actual-lib-alias-body-proof-result-2026-05-13.md).
Branch-local monorepo-006 attribution on this slice records
`direct_actual_lib_alias_body_outcomes = { success: 2, name_not_admitted: 14 }`,
with the remaining utility aliases still in the declaration-file residue table;
see
[`perf-runs/2026-05-13-actual-lib-alias-proof-result-attribution.md`](perf-runs/2026-05-13-actual-lib-alias-proof-result-attribution.md).

**2026-05-13 alias proof/admission split follow-up:** the next stacked slice
keeps direct-return behavior unchanged but moves the decorator allowlist out of
the alias-body proof path. The proof helper can now return a typed proof for
proven-but-unadmitted aliases and carries the measured outcome (`success`,
`generic_alias`, or `name_not_admitted`) with the body, `DefId`, and type
parameters. `direct_actual_lib_symbol_type` still returns only
`DecoratorMetadata` / `DecoratorMetadataObject`; generic aliases and
`PropertyKey` remain on fallback. This makes the next attribution run show
which remaining aliases are generic proof successes instead of collapsing them
all into `name_not_admitted`. Claim:
[`claims/perf-actual-lib-alias-proof-admission-split-2026-05-13.md`](claims/perf-actual-lib-alias-proof-admission-split-2026-05-13.md).
Branch-local monorepo-006 attribution on this slice keeps the behavior counters
unchanged while moving alias-body outcomes to
`{ success: 2, generic_alias: 8, missing_resolver_type: 5, name_not_admitted: 1 }`;
see
[`perf-runs/2026-05-13-actual-lib-alias-proof-admission-attribution.md`](perf-runs/2026-05-13-actual-lib-alias-proof-admission-attribution.md).

**2026-05-13 value-bearing interface follow-up:** the current-main
transplant of the selected actual-lib value-interface slice admits only
`Function`, `Object`, and `RegExp` through `direct_actual_lib_symbol_type`.
Generic utility aliases, `PropertyKey`, `Iterator`, `Locale`, and namespace or
heritage-sensitive residues stay on fallback paths. On monorepo-006 this drops
`DelegateCrossArenaSymbol` child-checkers from 26 to 23,
`checker.with_parent_cache_constructed` from 29 to 26, and `delegate.misses`
from 28 to 25 with unchanged diagnostics (`10,198`). Decision record:
[`perf-runs/2026-05-13-delegate-actual-lib-value-interfaces-main.md`](perf-runs/2026-05-13-delegate-actual-lib-value-interfaces-main.md).
Claim: [`claims/perf-actual-lib-value-interfaces-main-2026-05-13.md`](claims/perf-actual-lib-value-interfaces-main-2026-05-13.md).

### PR 7A: ~~T2.1.B sequential session-reuse~~ — done

Behind `TSZ_FILE_SESSION_REUSE` flag. `CheckerContext::switch_to_file`
in `crates/tsz-checker/src/context/file_session_reset.rs` clears
file-local state at the boundary while preserving the shared
`QueryCache` and program-stable caches. Byte-identical diagnostics
to the default per-file construction path.

### PR 7B: ~~T2.1.C parallel session-reuse~~ — done

#5842 (merged at `ee20f50f0e`) extends the same boundary to the
rayon-chunked parallel driver path. The same reset semantics now
apply when each worker thread reuses its `CheckerState` across
files in a chunk.

### PR 7C: `WorkerContext` / future T2.1.D session-lease / typed-query

Goal: replace the **hottest** child-checker path with an explicit
session lease or typed query — the dominant `CheckerCreationReason`
from the post-#5863 attribution run.

Done when:

- No cross-file state leaks under stress tests.
- The target reason's `with_parent_cache_by_reason[i]` count drops.
- RSS remains bounded.

---

## 12. Test Strategy

### Correctness Tests

For checker/context/cross-file changes, prioritize:

- TypeScript conformance tests for module resolution, NodeNext, path maps,
  package exports/imports, JSON imports, and duplicate package redirects.
- Cross-file type alias, interface, and class merging.
- Global augmentation and module augmentation.
- Lib replacement packages.
- JSX namespace and intrinsic elements.
- CommonJS export surfaces and expando properties.
- Speculative overload/generic inference rollback.
- LSP/incremental cache invalidation if touched.

### Stress Fixtures

Add targeted fixtures for:

- many files importing a common alias-heavy module
- repeated `React.*` namespace lookups
- many class/interface declarations with cross-file heritage
- package.json boundary-heavy NodeNext graphs
- many negative module-resolution probes
- union/mapped/conditional-heavy files causing interner insert pressure

### Regression Guards

Use these as defaults unless a PR explains a different threshold:

| Guard | Default |
| --- | --- |
| `large-ts-repo` timing wall time | no regression > 5% unless attribution explains it |
| small vite timing wall time | no regression > 10 ms or > 5%, whichever is larger |
| migrated child-checker reason | target reason count must decrease |
| RSS | no increase > 10% without explicit approval |
| conformance | no new failures in affected domains |

---

## 13. Risk Register

| Risk | Severity | Mitigation |
| --- | --- | --- |
| Chasing the stale 890 s baseline | High | T0 hard gate; no wall-time target until measured. |
| Counter overhead distorts timing | High | Separate timing/attribution modes; compile out expensive counter timing paths. |
| Checker state leaks file-local data | High | Field inventory, reset tests, and no pooling before lifetime split. |
| Typed-query cache key is incomplete | High | Include file, symbol, query kind, request mode, options, and program fingerprint. |
| Cross-file query cycles change behavior | High | Explicit in-progress state plus fallback/error semantics. |
| Resolver cache returns wrong NodeNext/package-exports answers | High | Resolution snapshot tests and complete request keys. |
| Lib global sharing breaks augmentations | High | Gate on measurement and add augmentation/lib replacement tests. |
| Interner redesign destabilizes `TypeId` identity | High | Instrument first and prefer low-risk mitigations. |
| Future `ProgramContext` refinements diverge from current shape | Medium | Refine the existing `ProgramContext` (renamed in PR 5B) rather than building a parallel abstraction. |
| Plan drifts again | Medium | Require checked-in decision records for changed measured claims. |

---

## 14. Measurement Protocol

1. A/B against the same worktree. Rebuild release binaries for each branch.
2. Use full bench mode for PR-quality numbers. Quick mode is exploratory.
3. Quote both wall time and peak RSS for large-repo PRs.
4. Keep timing and attribution separate.
5. Use `scripts/safe-run.sh` for heavy runs.
6. Update this document in the same PR that changes a quoted number.
7. Never present local fixture overrides as canonical evidence.

---

## 15. Current Reference Index

These are the current files to inspect before implementing the next PR. Line
numbers move frequently; prefer symbol search over stale line references.

### Bench Infrastructure

- `scripts/bench/bench-vs-tsgo.sh` - fixture selection, hyperfine driver, JSON aggregation.
- `scripts/bench/scale-cliff/generate-fixtures.sh` - monorepo-001..006 generator.
- `scripts/bench/scale-cliff/run-cliff.sh` - scale-cliff runner.
- `.github/workflows/bench.yml` - benchmark matrix and GCS publishing.

### CLI And Driver

- `crates/tsz-cli/src/driver/core.rs` - `PhaseTimings` and `CompilationResult`.
- `crates/tsz-cli/src/driver/check.rs` - active project checking path, `ProgramContext` construction, shared program indices.
- `crates/tsz-cli/src/driver/check_utils.rs` - project-wide maps and helper paths used by checking.
- `crates/tsz-cli/src/bin/tsz.rs` - normal CLI parsing; perf JSON output must be build-gated or moved to a perf harness.

### Program And Checker Context

- `crates/tsz-checker/src/context/mod.rs` - `CheckerContext` and `ProgramContext`.
- `crates/tsz-checker/src/context/core.rs` - `ProgramContext` application helpers, overlay snapshot inheritance, file-index state.
- `crates/tsz-checker/src/context/constructors.rs` - checker/context constructors.
- `crates/tsz-checker/src/state/state.rs` - `CheckerState` construction and parent-cache constructors.

### Cross-File Work

- `crates/tsz-checker/src/state/type_analysis/cross_file.rs` - cross-file symbol resolution, fast paths, fallback child-checker construction.
- `crates/tsz-checker/src/state/type_resolution/import_type.rs` - import-type cross-file cases.
- `crates/tsz-checker/src/types/computation/call_helpers.rs` - call-helper child-checker sites.
- `crates/tsz-checker/src/types/queries/callable_truthiness.rs` - callable-truthiness child-checker sites.
- `crates/tsz-checker/src/types/property_access_helpers/expando.rs` - expando child-checker sites.

### Counters And Interner

- `crates/tsz-common/src/perf_counters.rs` - perf-build-only counter internals; default release builds should not expose or honor them.
- `crates/tsz-solver/src/intern/core/interner.rs` - type interner storage, shard locks, lookup/intern caches.
- `crates/tsz-solver/src/caches/query_cache.rs` - local and shared query cache layers.

### Lib Snapshot Work

- `crates/tsz-core/src/parallel/lib_snapshot.rs` - existing lib snapshot cache.
- `crates/tsz-solver/src/types.rs` - `TypeId`, `TypeData`, and layout-sensitive type definitions.

---

## 16. Focused PR Update Contract

Use this contract for changes that update measured claims in this document.

1. One measurable hypothesis per PR.
2. Update one status-row trajectory at a time unless a second row is directly
   coupled to the same measurement.
3. Include a decision record under `docs/plan/perf-runs/` and link it from the
   updated row or section.
4. Include raw attribution/diagnostics JSON paths for any new quoted numbers.
5. When another open PR already edits the same status-row counters, land an
   additive/non-overlapping docs slice first and rebase the counter edits after
   that PR merges.

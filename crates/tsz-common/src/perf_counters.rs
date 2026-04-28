//! Process-wide performance counters used to drive the perf-architectural
//! plan in `docs/plan/PERF_ARCHITECTURAL_PLAN.md`.
//!
//! Counters are gated by the `TSZ_PERF_COUNTERS` environment variable. When
//! the variable is unset the increments still fire (`AtomicU64::fetch_add`
//! is a single relaxed atomic op, which is well under a nanosecond), so we
//! could in principle just always count, but the env var also gates the
//! more expensive counters (per-shard lock-wait histograms, top-N largest
//! types, recomputation tracking) so production builds stay clean.
//!
//! Output is printed on demand via [`PerfCounters::dump`]. Drivers wire that
//! into `--extendedDiagnostics` (or `--perfCounters`) so a single bench
//! invocation produces both the standard phase timings and the counter dump.
//!
//! Per the architectural plan, this is a plan-changing PR — the data we
//! collect here decides how PRs 2–7 are scoped. Don't ship later PRs without
//! looking at the dump on `large-ts-repo` first.

use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

/// Cache-line-padded `AtomicU64` to avoid false sharing between hot
/// counters that get hammered concurrently. 64 is the typical cache-line
/// size on x86-64 and Apple Silicon's M-series; over-aligning is harmless.
///
/// Defined for use by future PRs. Not adopted in PR #1631 because the
/// [`enabled_fast`] gate already eliminates the false-sharing problem
/// in production builds (where the env var is unset, the counter writes
/// don't fire at all). Inside profiling runs we accept some perturbation;
/// when we want profiler-grade fidelity for the highest-frequency
/// counters we'll switch their fields to `PaddedAtomicU64` and call
/// `field.0.fetch_add(...)` directly. See PR #1630 review issue #2.
#[repr(align(64))]
pub struct PaddedAtomicU64(pub AtomicU64);

impl PaddedAtomicU64 {
    pub const fn new(v: u64) -> Self {
        Self(AtomicU64::new(v))
    }
}

/// Process-wide enabled flag for the perf counters. Initialized exactly
/// once on first observation and read on every counter increment via
/// [`enabled_fast`]; the increment then becomes a single predictable
/// branch that's elided in the disabled case so production builds (where
/// `TSZ_PERF_COUNTERS` is unset) pay only the cost of the load.
///
/// Why this matters: even `AtomicU64::fetch_add(_, Relaxed)` is a
/// read-modify-write on a shared cache line. On the exact codebase where
/// we're trying to measure parallel-work contention, leaving the atomic
/// always-firing creates a synthetic contention point that distorts the
/// numbers we're trying to collect.
static ENABLED_FAST: OnceLock<bool> = OnceLock::new();

/// Cheap O(1) gate readable from any hot path. Reads a `OnceLock<bool>`
/// (one branch + one load) instead of going through `counters().enabled`
/// (deref-via-OnceLock + load).
#[inline(always)]
pub fn enabled_fast() -> bool {
    *ENABLED_FAST.get_or_init(|| std::env::var_os("TSZ_PERF_COUNTERS").is_some())
}

/// Why a `CheckerState::with_parent_cache` (and the matching
/// `copy_symbol_file_targets_to`) call fired. Each variant pins one specific
/// call site so the counter dump shows attribution: "X of the 17,329
/// constructions came from `delegate_cross_arena_symbol_resolution`,
/// Y came from `jsdoc_type_construction`, ...".
///
/// Adding a new reason: add the variant, update `REASON_NAMES` to keep them
/// aligned, and increase `CHECKER_CREATION_REASON_COUNT`.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
#[repr(usize)]
pub enum CheckerCreationReason {
    /// `cross_file.rs::delegate_cross_arena_symbol_resolution` — the headline
    /// hot path; deep recursion through cross-file type queries.
    DelegateCrossArenaSymbol = 0,
    /// `cross_file.rs::delegate_cross_arena_class_instance_type`.
    DelegateCrossArenaClass = 1,
    /// `cross_file.rs::delegate_cross_arena_interface_type`.
    DelegateCrossArenaInterface = 2,
    /// Other `cross_file.rs` delegate variants (heritage, etc).
    DelegateCrossArenaOther = 3,
    /// JSDoc namespace-typedef lookups crossing arenas.
    JsDocLookup = 4,
    /// JSDoc type-construction (synthesized object/function shapes).
    JsDocTypeConstruction = 5,
    /// CommonJS `module.exports` / `exports.x` resolution + collection.
    CjsExports = 6,
    /// Cross-file type alias resolution.
    AliasResolution = 7,
    /// `import("…").Foo` indirect import-type resolution.
    ImportType = 8,
    /// Type-environment `core.rs` deep resolution helpers.
    TypeEnvironmentCore = 9,
    /// `types::queries::callable_truthiness` cross-file fall-through.
    CallableTruthiness = 10,
    /// Expando property assignments crossing files.
    ExpandoProperty = 11,
    /// `identifier::resolution` cross-file fallback.
    IdentifierResolution = 12,
    /// Generic call-helpers cross-file resolution (call_helpers.rs).
    CallHelpers = 13,
    /// `computed_helpers_binding` deep alias resolution.
    BindingHelpers = 14,
    /// `class_abstract_checker` cross-file abstract-method check.
    ClassAbstract = 15,
    /// Anything not explicitly classified above.
    Other = 16,
}

pub const CHECKER_CREATION_REASON_COUNT: usize = 17;

/// Human-readable names, one entry per `CheckerCreationReason` variant.
/// MUST stay in sync with the enum.
pub const REASON_NAMES: [&str; CHECKER_CREATION_REASON_COUNT] = [
    "DelegateCrossArenaSymbol",
    "DelegateCrossArenaClass",
    "DelegateCrossArenaInterface",
    "DelegateCrossArenaOther",
    "JsDocLookup",
    "JsDocTypeConstruction",
    "CjsExports",
    "AliasResolution",
    "ImportType",
    "TypeEnvironmentCore",
    "CallableTruthiness",
    "ExpandoProperty",
    "IdentifierResolution",
    "CallHelpers",
    "BindingHelpers",
    "ClassAbstract",
    "Other",
];

impl CheckerCreationReason {
    #[inline(always)]
    pub const fn as_index(self) -> usize {
        self as usize
    }
    pub const fn name(self) -> &'static str {
        REASON_NAMES[self as usize]
    }
}

/// One process-wide instance. Incremented from any thread, read once at
/// dump time.
pub struct PerfCounters {
    pub enabled: AtomicBool,

    // ─── delegation / cross-arena resolution ─────────────────────────────
    pub delegate_cross_arena_calls: AtomicU64,
    pub delegate_cross_arena_cache_hits_lib: AtomicU64,
    pub delegate_cross_arena_cache_hits_cross_file: AtomicU64,
    pub delegate_cross_arena_misses: AtomicU64,
    pub delegate_max_recursion_depth: AtomicU64,

    // ─── checker construction ────────────────────────────────────────────
    pub checker_state_constructed: AtomicU64,
    pub checker_state_with_parent_cache_constructed: AtomicU64,
    /// Per-`CheckerCreationReason` breakdown of `with_parent_cache` calls.
    /// `with_parent_cache_by_reason[reason as usize]` is the count for that
    /// site. Total equals `checker_state_with_parent_cache_constructed`.
    pub with_parent_cache_by_reason: [AtomicU64; CHECKER_CREATION_REASON_COUNT],

    // ─── overlay copy ────────────────────────────────────────────────────
    pub copy_symbol_file_targets_calls: AtomicU64,
    pub copy_symbol_file_targets_entries_total: AtomicU64,
    /// Largest single overlay clone observed across the whole run.
    /// Distinguishes "many medium clones" from "a few catastrophic huge
    /// clones" — both can produce the same `entries_total`, but the fix
    /// shape is different. (Per PR #1630 review.)
    pub copy_symbol_file_targets_entries_max: AtomicU64,
    /// Bucketed histogram of overlay-clone sizes. `len_ge_N` counts the
    /// number of `copy_symbol_file_targets_to` calls where the parent's
    /// overlay had ≥ N entries at copy time. The buckets are nested so
    /// `len_ge_1m ≤ len_ge_100k ≤ len_ge_10k ≤ len_ge_1k ≤ calls`.
    pub copy_symbol_file_targets_len_ge_1k: AtomicU64,
    pub copy_symbol_file_targets_len_ge_10k: AtomicU64,
    pub copy_symbol_file_targets_len_ge_100k: AtomicU64,
    pub copy_symbol_file_targets_len_ge_1m: AtomicU64,
    /// Per-`CheckerCreationReason` breakdown of overlay-copy calls.
    pub overlay_copy_calls_by_reason: [AtomicU64; CHECKER_CREATION_REASON_COUNT],
    /// Per-`CheckerCreationReason` breakdown of overlay entries copied
    /// (sum of `parent.cross_file_symbol_targets.len()` at each call).
    pub overlay_copy_entries_by_reason: [AtomicU64; CHECKER_CREATION_REASON_COUNT],
    /// Per-`CheckerCreationReason` max overlay size observed at call time.
    /// Updated via [`record_max`] so the report shows the worst single
    /// clone per reason, not just the average.
    pub overlay_copy_max_entries_by_reason: [AtomicU64; CHECKER_CREATION_REASON_COUNT],

    // ─── interner ────────────────────────────────────────────────────────
    pub interner_intern_calls: AtomicU64,
    pub interner_intern_hits: AtomicU64,
    pub interner_intern_misses: AtomicU64,
    pub interner_string_intern_calls: AtomicU64,
    pub interner_type_list_intern_calls: AtomicU64,
    pub interner_object_shape_intern_calls: AtomicU64,
    pub interner_function_shape_intern_calls: AtomicU64,
    pub interner_application_intern_calls: AtomicU64,
    pub interner_conditional_intern_calls: AtomicU64,
    pub interner_mapped_intern_calls: AtomicU64,

    // ─── compute_type_of_symbol ──────────────────────────────────────────
    pub compute_type_of_symbol_calls: AtomicU64,
    pub compute_type_of_symbol_cache_hits: AtomicU64,

    // ─── resolver / VFS ──────────────────────────────────────────────────
    pub resolver_lookup_calls: AtomicU64,
    pub resolver_is_file_calls: AtomicU64,
    pub resolver_is_dir_calls: AtomicU64,
    pub resolver_read_dir_calls: AtomicU64,
    pub resolver_read_package_json_calls: AtomicU64,
    pub resolver_candidate_paths_total: AtomicU64,
}

static COUNTERS: OnceLock<PerfCounters> = OnceLock::new();

impl PerfCounters {
    const fn new_zero() -> Self {
        // Helper to construct a zero array of the right length without
        // requiring `AtomicU64: Copy` (it isn't).
        const Z: AtomicU64 = AtomicU64::new(0);
        Self {
            enabled: AtomicBool::new(false),
            delegate_cross_arena_calls: AtomicU64::new(0),
            delegate_cross_arena_cache_hits_lib: AtomicU64::new(0),
            delegate_cross_arena_cache_hits_cross_file: AtomicU64::new(0),
            delegate_cross_arena_misses: AtomicU64::new(0),
            delegate_max_recursion_depth: AtomicU64::new(0),
            checker_state_constructed: AtomicU64::new(0),
            checker_state_with_parent_cache_constructed: AtomicU64::new(0),
            with_parent_cache_by_reason: [Z; CHECKER_CREATION_REASON_COUNT],
            copy_symbol_file_targets_calls: AtomicU64::new(0),
            copy_symbol_file_targets_entries_total: AtomicU64::new(0),
            copy_symbol_file_targets_entries_max: AtomicU64::new(0),
            copy_symbol_file_targets_len_ge_1k: AtomicU64::new(0),
            copy_symbol_file_targets_len_ge_10k: AtomicU64::new(0),
            copy_symbol_file_targets_len_ge_100k: AtomicU64::new(0),
            copy_symbol_file_targets_len_ge_1m: AtomicU64::new(0),
            overlay_copy_calls_by_reason: [Z; CHECKER_CREATION_REASON_COUNT],
            overlay_copy_entries_by_reason: [Z; CHECKER_CREATION_REASON_COUNT],
            overlay_copy_max_entries_by_reason: [Z; CHECKER_CREATION_REASON_COUNT],
            interner_intern_calls: AtomicU64::new(0),
            interner_intern_hits: AtomicU64::new(0),
            interner_intern_misses: AtomicU64::new(0),
            interner_string_intern_calls: AtomicU64::new(0),
            interner_type_list_intern_calls: AtomicU64::new(0),
            interner_object_shape_intern_calls: AtomicU64::new(0),
            interner_function_shape_intern_calls: AtomicU64::new(0),
            interner_application_intern_calls: AtomicU64::new(0),
            interner_conditional_intern_calls: AtomicU64::new(0),
            interner_mapped_intern_calls: AtomicU64::new(0),
            compute_type_of_symbol_calls: AtomicU64::new(0),
            compute_type_of_symbol_cache_hits: AtomicU64::new(0),
            resolver_lookup_calls: AtomicU64::new(0),
            resolver_is_file_calls: AtomicU64::new(0),
            resolver_is_dir_calls: AtomicU64::new(0),
            resolver_read_dir_calls: AtomicU64::new(0),
            resolver_read_package_json_calls: AtomicU64::new(0),
            resolver_candidate_paths_total: AtomicU64::new(0),
        }
    }
}

/// Get the process-wide counters. The first call also reads `TSZ_PERF_COUNTERS`
/// to set the `enabled` flag.
pub fn counters() -> &'static PerfCounters {
    COUNTERS.get_or_init(|| {
        let c = PerfCounters::new_zero();
        if std::env::var_os("TSZ_PERF_COUNTERS").is_some() {
            c.enabled.store(true, Ordering::Relaxed);
        }
        c
    })
}

/// Increment a counter when counters are enabled. The branch is the
/// only cost in the disabled case, which keeps production builds clean
/// without adding shared-cache-line traffic. See [`ENABLED_FAST`].
#[inline(always)]
pub fn inc(counter: &AtomicU64) {
    if enabled_fast() {
        counter.fetch_add(1, Ordering::Relaxed);
    }
}

/// Add `n` to a counter when counters are enabled.
#[inline(always)]
pub fn add(counter: &AtomicU64, n: u64) {
    if enabled_fast() {
        counter.fetch_add(n, Ordering::Relaxed);
    }
}

/// Set the maximum-seen value for a counter, racy but good enough for
/// "max recursion depth" / "largest overlay clone" style reporting.
/// Gated by [`enabled_fast`] for the same contention-avoidance reason.
#[inline]
pub fn record_max(counter: &AtomicU64, value: u64) {
    if !enabled_fast() {
        return;
    }
    let mut current = counter.load(Ordering::Relaxed);
    while value > current {
        match counter.compare_exchange_weak(current, value, Ordering::Relaxed, Ordering::Relaxed) {
            Ok(_) => return,
            Err(observed) => current = observed,
        }
    }
}

/// Returns true when `TSZ_PERF_COUNTERS` is set. Use this to gate the
/// expensive bookkeeping; the simple `inc` calls are always cheap enough
/// that gating them is more expensive than just doing them.
pub fn enabled() -> bool {
    counters().enabled.load(Ordering::Relaxed)
}

/// Record a `CheckerState::with_parent_cache` construction with attribution.
/// Bumps both the global counter and the per-reason bucket so PR #1631's
/// dump shows where the 17,329 constructions on subset3 come from.
#[inline]
pub fn record_with_parent_cache(reason: CheckerCreationReason) {
    let c = counters();
    inc(&c.checker_state_with_parent_cache_constructed);
    inc(&c.with_parent_cache_by_reason[reason.as_index()]);
}

/// Record an overlay copy with attribution: count + entries-copied +
/// global max + size-bucket histogram + per-reason max. The histogram
/// tells us whether `entries_total = 12.8B` is "many medium clones" or
/// "a few catastrophic huge clones" — both produce the same total but
/// imply very different fixes (per PR #1630 review).
///
/// Caller passes the parent overlay's len so we can attribute without
/// holding a borrow across the copy.
#[inline]
pub fn record_overlay_copy(reason: CheckerCreationReason, entries: u64) {
    let c = counters();
    inc(&c.copy_symbol_file_targets_calls);
    add(&c.copy_symbol_file_targets_entries_total, entries);
    record_max(&c.copy_symbol_file_targets_entries_max, entries);
    if entries >= 1_000 {
        inc(&c.copy_symbol_file_targets_len_ge_1k);
    }
    if entries >= 10_000 {
        inc(&c.copy_symbol_file_targets_len_ge_10k);
    }
    if entries >= 100_000 {
        inc(&c.copy_symbol_file_targets_len_ge_100k);
    }
    if entries >= 1_000_000 {
        inc(&c.copy_symbol_file_targets_len_ge_1m);
    }
    inc(&c.overlay_copy_calls_by_reason[reason.as_index()]);
    add(&c.overlay_copy_entries_by_reason[reason.as_index()], entries);
    record_max(
        &c.overlay_copy_max_entries_by_reason[reason.as_index()],
        entries,
    );
}

impl PerfCounters {
    /// Format the current counter snapshot as a multi-line report. Returns
    /// an empty string when the counters are disabled (so callers can
    /// unconditionally `print!("{}", PerfCounters::dump_string())` without
    /// noisy output in the common case).
    ///
    /// Counters that are NOT yet wired into their producer code (e.g. the
    /// per-kind `interner_*_intern_calls` buckets — the bucket fields are
    /// declared but the actual `tsz-solver` intern sites still need to be
    /// updated) are printed as `n/a` rather than `0`, so a reader doesn't
    /// mistake "not measured" for "didn't happen". A small `wired: false`
    /// table at the bottom of the dump lists which buckets are pending.
    pub fn dump_string() -> String {
        let c = counters();
        if !enabled_fast() {
            return String::new();
        }
        let load = |a: &AtomicU64| a.load(Ordering::Relaxed);
        format!(
            "\n=== TSZ_PERF_COUNTERS ===\n\
             Delegation (cross-arena symbol resolution):\n  \
             calls                      {:>12}\n  \
             cache hits (lib)           {:>12}\n  \
             cache hits (cross-file)    {:>12}\n  \
             misses (full work)         {:>12}\n  \
             max recursion depth        {:>12}\n\
             Checker construction:\n  \
             CheckerState::new          {:>12}\n  \
             ::with_parent_cache        {:>12}\n  \
             copy_symbol_file_targets   {:>12}\n  \
             overlay entries copied     {:>12}\n  \
             overlay entries (max)      {:>12}\n  \
             overlay len ≥ 1k           {:>12}\n  \
             overlay len ≥ 10k          {:>12}\n  \
             overlay len ≥ 100k         {:>12}\n  \
             overlay len ≥ 1M           {:>12}\n\
             compute_type_of_symbol:\n  \
             total calls                {:>12}\n  \
             cache hits                 {:>12}\n\
             TypeInterner:\n  \
             intern calls (total)             n/a  (not wired in this PR)\n  \
             intern hits                      n/a  (not wired in this PR)\n  \
             intern misses                    n/a  (not wired in this PR)\n  \
             string intern calls              n/a  (not wired in this PR)\n  \
             type-list intern calls           n/a  (not wired in this PR)\n  \
             object-shape intern calls        n/a  (not wired in this PR)\n  \
             function-shape intern calls      n/a  (not wired in this PR)\n  \
             application intern calls         n/a  (not wired in this PR)\n  \
             conditional intern calls         n/a  (not wired in this PR)\n  \
             mapped intern calls              n/a  (not wired in this PR)\n\
             Resolver:\n  \
             lookup calls               {:>12}\n  \
             is_file calls                    n/a  (not wired in this PR)\n  \
             is_dir calls                     n/a  (not wired in this PR)\n  \
             read_dir calls                   n/a  (not wired in this PR)\n  \
             read_package_json calls    {:>12}\n  \
             candidate paths total            n/a  (not wired in this PR)\n",
            load(&c.delegate_cross_arena_calls),
            load(&c.delegate_cross_arena_cache_hits_lib),
            load(&c.delegate_cross_arena_cache_hits_cross_file),
            load(&c.delegate_cross_arena_misses),
            load(&c.delegate_max_recursion_depth),
            load(&c.checker_state_constructed),
            load(&c.checker_state_with_parent_cache_constructed),
            load(&c.copy_symbol_file_targets_calls),
            load(&c.copy_symbol_file_targets_entries_total),
            load(&c.copy_symbol_file_targets_entries_max),
            load(&c.copy_symbol_file_targets_len_ge_1k),
            load(&c.copy_symbol_file_targets_len_ge_10k),
            load(&c.copy_symbol_file_targets_len_ge_100k),
            load(&c.copy_symbol_file_targets_len_ge_1m),
            load(&c.compute_type_of_symbol_calls),
            load(&c.compute_type_of_symbol_cache_hits),
            load(&c.resolver_lookup_calls),
            load(&c.resolver_read_package_json_calls),
        ) + &Self::dump_by_reason()
    }

    /// Per-reason breakdown of `with_parent_cache` and overlay-copy calls.
    /// Sorted by `with_parent_cache` count descending so the headline
    /// offenders show first. Skips reasons with zero counts.
    fn dump_by_reason() -> String {
        let c = counters();
        let load = |a: &AtomicU64| a.load(Ordering::Relaxed);
        // Collect (reason_idx, count, overlay_calls, overlay_entries, max_entries).
        let mut rows: Vec<(usize, u64, u64, u64, u64)> = (0..CHECKER_CREATION_REASON_COUNT)
            .map(|i| {
                (
                    i,
                    load(&c.with_parent_cache_by_reason[i]),
                    load(&c.overlay_copy_calls_by_reason[i]),
                    load(&c.overlay_copy_entries_by_reason[i]),
                    load(&c.overlay_copy_max_entries_by_reason[i]),
                )
            })
            .filter(|t| t.1 > 0 || t.2 > 0)
            .collect();
        if rows.is_empty() {
            return String::new();
        }
        rows.sort_by(|a, b| b.1.cmp(&a.1).then(b.3.cmp(&a.3)));
        let total_constructions = load(&c.checker_state_with_parent_cache_constructed).max(1);
        let total_overlay_entries = load(&c.copy_symbol_file_targets_entries_total).max(1);
        let mut out = String::from(
            "\n  with_parent_cache + overlay copies attributed by call site:\n  \
             reason                              cons    %  ovl_calls  ovl_entries          max  ent%\n",
        );
        for (i, cons, ovl_calls, ovl_entries, max_entries) in rows {
            let cons_pct = (cons as f64 / total_constructions as f64) * 100.0;
            let ent_pct = (ovl_entries as f64 / total_overlay_entries as f64) * 100.0;
            let row = format!(
                "  {:<32} {:>10} {:>4.1} {:>10} {:>12} {:>12} {:>5.1}\n",
                REASON_NAMES[i], cons, cons_pct, ovl_calls, ovl_entries, max_entries, ent_pct,
            );
            out.push_str(&row);
        }
        out
    }
}

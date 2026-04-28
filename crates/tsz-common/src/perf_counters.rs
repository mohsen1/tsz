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

    // ─── overlay copy ────────────────────────────────────────────────────
    pub copy_symbol_file_targets_calls: AtomicU64,
    pub copy_symbol_file_targets_entries_total: AtomicU64,

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
        Self {
            enabled: AtomicBool::new(false),
            delegate_cross_arena_calls: AtomicU64::new(0),
            delegate_cross_arena_cache_hits_lib: AtomicU64::new(0),
            delegate_cross_arena_cache_hits_cross_file: AtomicU64::new(0),
            delegate_cross_arena_misses: AtomicU64::new(0),
            delegate_max_recursion_depth: AtomicU64::new(0),
            checker_state_constructed: AtomicU64::new(0),
            checker_state_with_parent_cache_constructed: AtomicU64::new(0),
            copy_symbol_file_targets_calls: AtomicU64::new(0),
            copy_symbol_file_targets_entries_total: AtomicU64::new(0),
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

/// Increment a counter. Cheap (one relaxed atomic add) so it can fire
/// unconditionally on hot paths. The `enabled` gate is for callers that
/// want to skip more expensive bookkeeping (e.g., per-thread sampling).
#[inline(always)]
pub fn inc(counter: &AtomicU64) {
    counter.fetch_add(1, Ordering::Relaxed);
}

/// Add `n` to a counter. Same cost-shape as `inc`.
#[inline(always)]
pub fn add(counter: &AtomicU64, n: u64) {
    counter.fetch_add(n, Ordering::Relaxed);
}

/// Set the maximum-seen value for a counter, racy but good enough for
/// "max recursion depth" style reporting.
#[inline]
pub fn record_max(counter: &AtomicU64, value: u64) {
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

impl PerfCounters {
    /// Format the current counter snapshot as a multi-line report. Returns
    /// an empty string when the counters are disabled (so callers can
    /// unconditionally `print!("{}", PerfCounters::dump_string())` without
    /// noisy output in the common case).
    pub fn dump_string() -> String {
        let c = counters();
        if !c.enabled.load(Ordering::Relaxed) {
            return String::new();
        }
        let load = |a: &AtomicU64| a.load(Ordering::Relaxed);
        let total_intern_calls = load(&c.interner_intern_calls);
        let total_intern_hits = load(&c.interner_intern_hits);
        let intern_hit_rate = if total_intern_calls > 0 {
            (total_intern_hits as f64 / total_intern_calls as f64) * 100.0
        } else {
            0.0
        };
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
             copy_symbol_file_targets   {:>12}  ({} entries copied)\n\
             compute_type_of_symbol:\n  \
             total calls                {:>12}\n  \
             cache hits                 {:>12}\n\
             TypeInterner:\n  \
             intern calls (total)       {:>12}\n  \
             intern hits                {:>12}\n  \
             intern misses              {:>12}\n  \
             intern hit rate            {:>11.2}%\n  \
             string intern calls        {:>12}\n  \
             type-list intern calls     {:>12}\n  \
             object-shape intern calls  {:>12}\n  \
             function-shape intern calls{:>12}\n  \
             application intern calls   {:>12}\n  \
             conditional intern calls   {:>12}\n  \
             mapped intern calls        {:>12}\n\
             Resolver:\n  \
             lookup calls               {:>12}\n  \
             is_file calls              {:>12}\n  \
             is_dir calls               {:>12}\n  \
             read_dir calls             {:>12}\n  \
             read_package_json calls    {:>12}\n  \
             candidate paths total      {:>12}\n",
            load(&c.delegate_cross_arena_calls),
            load(&c.delegate_cross_arena_cache_hits_lib),
            load(&c.delegate_cross_arena_cache_hits_cross_file),
            load(&c.delegate_cross_arena_misses),
            load(&c.delegate_max_recursion_depth),
            load(&c.checker_state_constructed),
            load(&c.checker_state_with_parent_cache_constructed),
            load(&c.copy_symbol_file_targets_calls),
            load(&c.copy_symbol_file_targets_entries_total),
            load(&c.compute_type_of_symbol_calls),
            load(&c.compute_type_of_symbol_cache_hits),
            total_intern_calls,
            total_intern_hits,
            load(&c.interner_intern_misses),
            intern_hit_rate,
            load(&c.interner_string_intern_calls),
            load(&c.interner_type_list_intern_calls),
            load(&c.interner_object_shape_intern_calls),
            load(&c.interner_function_shape_intern_calls),
            load(&c.interner_application_intern_calls),
            load(&c.interner_conditional_intern_calls),
            load(&c.interner_mapped_intern_calls),
            load(&c.resolver_lookup_calls),
            load(&c.resolver_is_file_calls),
            load(&c.resolver_is_dir_calls),
            load(&c.resolver_read_dir_calls),
            load(&c.resolver_read_package_json_calls),
            load(&c.resolver_candidate_paths_total),
        )
    }
}

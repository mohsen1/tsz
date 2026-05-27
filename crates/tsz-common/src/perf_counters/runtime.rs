/// One process-wide instance. Incremented from any thread, read once at
/// dump time.
pub struct PerfCounters {
    pub enabled: AtomicBool,

    // ─── delegation / cross-arena resolution ─────────────────────────────
    pub delegate_cross_arena_calls: AtomicU64,
    pub delegate_cross_arena_cache_hits_lib: AtomicU64,
    pub delegate_cross_arena_cache_hits_cross_file: AtomicU64,
    pub delegate_cross_arena_misses: AtomicU64,
    /// T2.2 cross-file type-parameter memo: hits and misses on the
    /// `extract_type_params_from_decl` slow-path memoization. A hit means
    /// the slow-path's `with_parent_cache_attributed(..., TypeEnvironmentCore)`
    /// was elided.
    pub cross_file_type_params_cache_hits: AtomicU64,
    pub cross_file_type_params_cache_misses: AtomicU64,
    pub delegate_max_recursion_depth: AtomicU64,
    /// `DelegateCrossArenaSymbol` misses classified by how the target arena
    /// was found. This is a subset of `delegate_cross_arena_misses`.
    pub delegate_cross_arena_symbol_miss_by_source:
        [AtomicU64; CROSS_ARENA_SYMBOL_MISS_SOURCE_COUNT],
    /// `DelegateCrossArenaSymbol` misses classified by target symbol kind.
    pub delegate_cross_arena_symbol_miss_by_kind: [AtomicU64; CROSS_ARENA_SYMBOL_MISS_KIND_COUNT],
    pub delegate_cross_arena_symbol_miss_target_declaration_file: AtomicU64,
    pub delegate_cross_arena_symbol_miss_target_source_file: AtomicU64,
    /// Outcome buckets for the no-child alias shortcut attempted before a
    /// `DelegateCrossArenaSymbol` miss constructs a child checker.
    pub delegate_cross_arena_alias_shortcut_outcome:
        [AtomicU64; CROSS_ARENA_ALIAS_SHORTCUT_OUTCOME_COUNT],
    /// Outcome buckets for direct cross-file interface lowering attempts.
    pub direct_cross_file_interface_lowering_outcome:
        [AtomicU64; DIRECT_CROSS_FILE_INTERFACE_LOWERING_OUTCOME_COUNT],
    /// Outcome buckets for direct actual-lib alias-body attempts.
    pub direct_actual_lib_alias_body_outcome:
        [AtomicU64; DIRECT_ACTUAL_LIB_ALIAS_BODY_OUTCOME_COUNT],
    /// Outcome buckets for direct source-file type-alias lowering attempts.
    pub direct_source_file_type_alias_lowering_outcome:
        [AtomicU64; DIRECT_SOURCE_FILE_TYPE_ALIAS_LOWERING_OUTCOME_COUNT],
    /// Root syntax family for source-file alias bodies rejected by the direct
    /// lowering proof.
    pub direct_source_file_type_alias_body_rejection_kind:
        [AtomicU64; DIRECT_SOURCE_FILE_TYPE_ALIAS_BODY_REJECTION_KIND_COUNT],
    /// Structural subtype for root `TypeReference` alias bodies rejected by
    /// the direct-lowering proof.
    pub direct_source_file_type_alias_type_reference_rejection_kind:
        [AtomicU64; DIRECT_SOURCE_FILE_TYPE_ALIAS_TYPE_REFERENCE_REJECTION_KIND_COUNT],
    /// First nested `TypeReference` rejection seen per rejected source-file
    /// alias body. This is one bucket per rejected alias, unlike the all-refs
    /// counter above.
    pub direct_source_file_type_alias_first_type_reference_rejection_kind:
        [AtomicU64; DIRECT_SOURCE_FILE_TYPE_ALIAS_TYPE_REFERENCE_REJECTION_KIND_COUNT],
    /// Outcome buckets for direct actual-lib Intl interface attempts.
    pub direct_actual_lib_intl_interface_outcome:
        [AtomicU64; DIRECT_ACTUAL_LIB_INTL_INTERFACE_OUTCOME_COUNT],
    /// Track 7 stable-identity migration counter: times
    /// `TypeEnvironment::resolve_lazy` had to treat a `DefId` value as a raw
    /// `SymbolId` and redirect it to the real `DefId`.
    pub type_environment_raw_symbol_lazy_fallbacks: AtomicU64,
    /// Why each `cached_cross_file_*` reader returned `None`. See
    /// [`CrossFileCacheMissCause`] for the bucket semantics. Sum of
    /// all buckets equals the flat miss count for the four reader
    /// helpers in `crates/tsz-checker/src/context/cross_file_query.rs`.
    pub cross_file_cache_miss_cause: [AtomicU64; CROSS_FILE_CACHE_MISS_CAUSE_COUNT],
    /// Source-file symbol-arena cache eligibility/rejection buckets for
    /// `DelegateCrossArenaSymbol` delegations. This classifies the remaining
    /// post-#6191 symbol-arena residue before we widen any cache keys or direct
    /// lowering paths.
    pub source_file_symbol_arena_cache_eligibility_outcome:
        [AtomicU64; SOURCE_FILE_SYMBOL_ARENA_CACHE_ELIGIBILITY_OUTCOME_COUNT],

    // ─── checker construction ────────────────────────────────────────────
    pub checker_state_constructed: AtomicU64,
    pub checker_state_with_parent_cache_constructed: AtomicU64,
    /// Per-`CheckerCreationReason` breakdown of `with_parent_cache` calls.
    /// `with_parent_cache_by_reason[reason as usize]` is the count for that
    /// site. Total equals `checker_state_with_parent_cache_constructed`.
    pub with_parent_cache_by_reason: [AtomicU64; CHECKER_CREATION_REASON_COUNT],

    // ─── checker file-session ────────────────────────────────────────────
    /// Number of times `CheckerContext::reset_for_next_file()` has been
    /// invoked. Zero on the default per-file checker construction path;
    /// nonzero only on a sequential session-reuse path (T2.1.B).
    /// Attribution-mode verification: in a reuse run the counter equals
    /// `(files_checked - 1)` and `checker_state_constructed` falls by the
    /// same amount versus the baseline construction-per-file path.
    pub file_session_resets: AtomicU64,

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
    pub interner_callable_shape_intern_calls: AtomicU64,
    pub interner_application_intern_calls: AtomicU64,
    pub interner_conditional_intern_calls: AtomicU64,
    pub interner_mapped_intern_calls: AtomicU64,
    /// Lock-wait histogram. Each call to [`time_shard_write`] adds one
    /// observation to the bucket whose upper bound first exceeds the
    /// elapsed nanoseconds. Only populated when the
    /// `perf-counters-timing` cargo feature is enabled — otherwise
    /// `time_shard_write` compiles to a direct call of its closure
    /// and the histogram stays at all-zero.
    pub interner_lock_wait_histogram_ns: [AtomicU64; LOCK_WAIT_BUCKET_COUNT],

    // ─── compute_type_of_symbol ──────────────────────────────────────────
    pub compute_type_of_symbol_calls: AtomicU64,
    pub compute_type_of_symbol_cache_hits: AtomicU64,
    pub compute_type_of_symbol_interface_simple_object_fastpath_hits: AtomicU64,
    pub compute_type_of_symbol_source_outcome:
        [AtomicU64; COMPUTE_TYPE_OF_SYMBOL_SOURCE_OUTCOME_COUNT],
    pub compute_type_of_symbol_kind_outcome: [AtomicU64; COMPUTE_TYPE_OF_SYMBOL_KIND_OUTCOME_COUNT],
    pub compute_type_of_symbol_interface_fastpath_outcome:
        [AtomicU64; COMPUTE_TYPE_OF_SYMBOL_INTERFACE_FASTPATH_OUTCOME_COUNT],
    pub compute_type_of_symbol_interface_callsite_outcome:
        [AtomicU64; COMPUTE_TYPE_OF_SYMBOL_INTERFACE_CALLSITE_OUTCOME_COUNT],
    pub compute_type_of_symbol_interface_simple_object_outcome:
        [AtomicU64; COMPUTE_TYPE_OF_SYMBOL_INTERFACE_SIMPLE_OBJECT_OUTCOME_COUNT],
    pub compute_type_of_symbol_interface_simple_object_non_primitive_annotation_kind: [AtomicU64;
        COMPUTE_TYPE_OF_SYMBOL_INTERFACE_SIMPLE_OBJECT_NON_PRIMITIVE_ANNOTATION_KIND_COUNT],
    pub compute_type_of_symbol_interface_simple_object_type_reference_reject_outcome: [AtomicU64;
        COMPUTE_TYPE_OF_SYMBOL_INTERFACE_SIMPLE_OBJECT_TYPE_REFERENCE_REJECT_OUTCOME_COUNT],
    pub property_classification_calls: AtomicU64,
    pub property_classification_string_fallback_source_lookups: AtomicU64,
    pub property_classification_string_fallback_target_names: AtomicU64,
    pub property_classification_string_fallback_target_types: AtomicU64,

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
            cross_file_type_params_cache_hits: AtomicU64::new(0),
            cross_file_type_params_cache_misses: AtomicU64::new(0),
            delegate_max_recursion_depth: AtomicU64::new(0),
            delegate_cross_arena_symbol_miss_by_source: [const { AtomicU64::new(0) };
                CROSS_ARENA_SYMBOL_MISS_SOURCE_COUNT],
            delegate_cross_arena_symbol_miss_by_kind: [const { AtomicU64::new(0) };
                CROSS_ARENA_SYMBOL_MISS_KIND_COUNT],
            delegate_cross_arena_symbol_miss_target_declaration_file: AtomicU64::new(0),
            delegate_cross_arena_symbol_miss_target_source_file: AtomicU64::new(0),
            delegate_cross_arena_alias_shortcut_outcome: [const { AtomicU64::new(0) };
                CROSS_ARENA_ALIAS_SHORTCUT_OUTCOME_COUNT],
            direct_cross_file_interface_lowering_outcome: [const { AtomicU64::new(0) };
                DIRECT_CROSS_FILE_INTERFACE_LOWERING_OUTCOME_COUNT],
            direct_actual_lib_alias_body_outcome: [const { AtomicU64::new(0) };
                DIRECT_ACTUAL_LIB_ALIAS_BODY_OUTCOME_COUNT],
            direct_source_file_type_alias_lowering_outcome: [const { AtomicU64::new(0) };
                DIRECT_SOURCE_FILE_TYPE_ALIAS_LOWERING_OUTCOME_COUNT],
            direct_source_file_type_alias_body_rejection_kind: [const { AtomicU64::new(0) };
                DIRECT_SOURCE_FILE_TYPE_ALIAS_BODY_REJECTION_KIND_COUNT],
            direct_source_file_type_alias_type_reference_rejection_kind: [const { AtomicU64::new(0) };
                DIRECT_SOURCE_FILE_TYPE_ALIAS_TYPE_REFERENCE_REJECTION_KIND_COUNT],
            direct_source_file_type_alias_first_type_reference_rejection_kind: [const {
                AtomicU64::new(0)
            };
                DIRECT_SOURCE_FILE_TYPE_ALIAS_TYPE_REFERENCE_REJECTION_KIND_COUNT],
            direct_actual_lib_intl_interface_outcome: [const { AtomicU64::new(0) };
                DIRECT_ACTUAL_LIB_INTL_INTERFACE_OUTCOME_COUNT],
            type_environment_raw_symbol_lazy_fallbacks: AtomicU64::new(0),
            cross_file_cache_miss_cause: [const { AtomicU64::new(0) };
                CROSS_FILE_CACHE_MISS_CAUSE_COUNT],
            source_file_symbol_arena_cache_eligibility_outcome: [const { AtomicU64::new(0) };
                SOURCE_FILE_SYMBOL_ARENA_CACHE_ELIGIBILITY_OUTCOME_COUNT],
            checker_state_constructed: AtomicU64::new(0),
            checker_state_with_parent_cache_constructed: AtomicU64::new(0),
            with_parent_cache_by_reason: [const { AtomicU64::new(0) };
                CHECKER_CREATION_REASON_COUNT],
            file_session_resets: AtomicU64::new(0),
            copy_symbol_file_targets_calls: AtomicU64::new(0),
            copy_symbol_file_targets_entries_total: AtomicU64::new(0),
            copy_symbol_file_targets_entries_max: AtomicU64::new(0),
            copy_symbol_file_targets_len_ge_1k: AtomicU64::new(0),
            copy_symbol_file_targets_len_ge_10k: AtomicU64::new(0),
            copy_symbol_file_targets_len_ge_100k: AtomicU64::new(0),
            copy_symbol_file_targets_len_ge_1m: AtomicU64::new(0),
            overlay_copy_calls_by_reason: [const { AtomicU64::new(0) };
                CHECKER_CREATION_REASON_COUNT],
            overlay_copy_entries_by_reason: [const { AtomicU64::new(0) };
                CHECKER_CREATION_REASON_COUNT],
            overlay_copy_max_entries_by_reason: [const { AtomicU64::new(0) };
                CHECKER_CREATION_REASON_COUNT],
            interner_intern_calls: AtomicU64::new(0),
            interner_intern_hits: AtomicU64::new(0),
            interner_intern_misses: AtomicU64::new(0),
            interner_string_intern_calls: AtomicU64::new(0),
            interner_type_list_intern_calls: AtomicU64::new(0),
            interner_object_shape_intern_calls: AtomicU64::new(0),
            interner_function_shape_intern_calls: AtomicU64::new(0),
            interner_callable_shape_intern_calls: AtomicU64::new(0),
            interner_application_intern_calls: AtomicU64::new(0),
            interner_conditional_intern_calls: AtomicU64::new(0),
            interner_mapped_intern_calls: AtomicU64::new(0),
            interner_lock_wait_histogram_ns: [const { AtomicU64::new(0) }; LOCK_WAIT_BUCKET_COUNT],
            compute_type_of_symbol_calls: AtomicU64::new(0),
            compute_type_of_symbol_cache_hits: AtomicU64::new(0),
            compute_type_of_symbol_interface_simple_object_fastpath_hits: AtomicU64::new(0),
            compute_type_of_symbol_source_outcome: [const { AtomicU64::new(0) };
                COMPUTE_TYPE_OF_SYMBOL_SOURCE_OUTCOME_COUNT],
            compute_type_of_symbol_kind_outcome: [const { AtomicU64::new(0) };
                COMPUTE_TYPE_OF_SYMBOL_KIND_OUTCOME_COUNT],
            compute_type_of_symbol_interface_fastpath_outcome: [const { AtomicU64::new(0) };
                COMPUTE_TYPE_OF_SYMBOL_INTERFACE_FASTPATH_OUTCOME_COUNT],
            compute_type_of_symbol_interface_callsite_outcome: [const { AtomicU64::new(0) };
                COMPUTE_TYPE_OF_SYMBOL_INTERFACE_CALLSITE_OUTCOME_COUNT],
            compute_type_of_symbol_interface_simple_object_outcome: [const { AtomicU64::new(0) };
                COMPUTE_TYPE_OF_SYMBOL_INTERFACE_SIMPLE_OBJECT_OUTCOME_COUNT],
            compute_type_of_symbol_interface_simple_object_non_primitive_annotation_kind: [const {
                AtomicU64::new(0)
            };
                COMPUTE_TYPE_OF_SYMBOL_INTERFACE_SIMPLE_OBJECT_NON_PRIMITIVE_ANNOTATION_KIND_COUNT],
            compute_type_of_symbol_interface_simple_object_type_reference_reject_outcome: [const {
                AtomicU64::new(0)
            };
                COMPUTE_TYPE_OF_SYMBOL_INTERFACE_SIMPLE_OBJECT_TYPE_REFERENCE_REJECT_OUTCOME_COUNT],
            property_classification_calls: AtomicU64::new(0),
            property_classification_string_fallback_source_lookups: AtomicU64::new(0),
            property_classification_string_fallback_target_names: AtomicU64::new(0),
            property_classification_string_fallback_target_types: AtomicU64::new(0),
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

/// RAII guard that tracks recursion depth into
/// `delegate_cross_arena_symbol_resolution`. Each `enter_delegate()` increments
/// a thread-local counter and updates `delegate_max_recursion_depth` to the
/// running peak; the guard's `Drop` impl decrements when the call returns.
/// The whole thing short-circuits when counters are disabled, so timing builds
/// pay one branch per call.
pub struct DelegateDepthGuard(());

thread_local! {
    static DELEGATE_DEPTH: std::cell::Cell<u32> = const { std::cell::Cell::new(0) };
}

#[inline]
pub fn enter_delegate() -> DelegateDepthGuard {
    if !enabled_fast() {
        return DelegateDepthGuard(());
    }
    DELEGATE_DEPTH.with(|d| {
        let next = d.get().saturating_add(1);
        d.set(next);
        record_max(&counters().delegate_max_recursion_depth, u64::from(next));
    });
    DelegateDepthGuard(())
}

impl Drop for DelegateDepthGuard {
    fn drop(&mut self) {
        if !enabled_fast() {
            return;
        }
        DELEGATE_DEPTH.with(|d| d.set(d.get().saturating_sub(1)));
    }
}

/// Returns true when `TSZ_PERF_COUNTERS` is set. Use this to gate the
/// expensive bookkeeping; the simple `inc` calls are always cheap enough
/// that gating them is more expensive than just doing them.
pub fn enabled() -> bool {
    counters().enabled.load(Ordering::Relaxed)
}

/// Record a single lock-wait observation into the histogram. Buckets are
/// log-spaced over 100 ns…100 ms with a final overflow bucket; see
/// [`LOCK_WAIT_BUCKET_UPPER_BOUNDS_NS`]. Gated behind the
/// `perf-counters-timing` feature: when the feature is off this function
/// is not compiled at all (the `cfg` excludes the entire item), and the
/// only call site lives inside the feature-on variant of
/// [`time_shard_write`], which is replaced with a no-op stub that calls
/// `f()` directly.
#[cfg(feature = "perf-counters-timing")]
#[inline]
fn record_lock_wait_ns(ns: u64) {
    if !enabled_fast() {
        return;
    }
    let buckets = &counters().interner_lock_wait_histogram_ns;
    for (i, &upper) in LOCK_WAIT_BUCKET_UPPER_BOUNDS_NS.iter().enumerate() {
        if ns < upper {
            buckets[i].fetch_add(1, Ordering::Relaxed);
            return;
        }
    }
}

/// Time a contended write inside the type-interner. The closure runs in
/// both modes; the cost of the timing infrastructure is the
/// difference between the two `cfg`-gated implementations:
///
/// - **`perf-counters-timing` ON**: `Instant::now()` brackets the closure;
///   the elapsed nanos land in the lock-wait histogram (gated on
///   `enabled_fast()`, so timing-mode runs that don't enable counters
///   still pay only the gate load + closure call).
/// - **`perf-counters-timing` OFF (default)**: the wrapper compiles to a
///   direct call of `f()`. Zero `Instant::now()` calls, zero atomic
///   loads, zero histogram accesses. Default release builds do not pay
///   the timing cost the plan §4.T0.3 explicitly forbids.
///
/// `_shard_idx` is reserved for a future per-shard breakdown; today
/// every shard's observations land in the same global histogram.
#[cfg(feature = "perf-counters-timing")]
#[inline]
pub fn time_shard_write<R>(_shard_idx: u32, f: impl FnOnce() -> R) -> R {
    if !enabled_fast() {
        return f();
    }
    // `web_time::Instant` is the WASM-safe drop-in for `std::time::Instant`;
    // tsz-common is compiled for wasm32 and the arch guard bans the std
    // type even on cfg-gated paths. See `scripts/arch/arch_guard.py`.
    let start = web_time::Instant::now();
    let result = f();
    record_lock_wait_ns(start.elapsed().as_nanos() as u64);
    result
}

#[cfg(not(feature = "perf-counters-timing"))]
#[inline(always)]
pub fn time_shard_write<R>(_shard_idx: u32, f: impl FnOnce() -> R) -> R {
    f()
}

/// Whether the lock-wait histogram is *physically wired* (the
/// `perf-counters-timing` cfg feature is on). Independent of
/// `enabled_fast()`: a build with the feature on but the env var off
/// still has the histogram fields and serializes them as zeroes; a
/// build with the feature off keeps the histogram fields (so the
/// `PerfCounters` layout is feature-stable) but compiles out the
/// timing + recording logic and serializes the histogram as `null` via
/// [`PerfCounterSnapshot`].
#[inline(always)]
pub const fn lock_wait_histogram_wired() -> bool {
    cfg!(feature = "perf-counters-timing")
}

/// Record a `CheckerState::with_parent_cache` construction with attribution.
/// Bumps both the global counter and the per-reason bucket so PR #1631's
/// dump shows where the 17,329 constructions on subset3 come from.
///
/// Gate once at the top: when counters are disabled the helper returns
/// without paying the `counters()` `OnceLock` deref. When enabled the
/// two atomic increments are direct `fetch_add` calls (no per-call
/// `enabled_fast()` re-check via `inc()`).
#[inline]
pub fn record_with_parent_cache(reason: CheckerCreationReason) {
    if !enabled_fast() {
        return;
    }
    let c = counters();
    c.checker_state_with_parent_cache_constructed
        .fetch_add(1, Ordering::Relaxed);
    c.with_parent_cache_by_reason[reason.as_index()].fetch_add(1, Ordering::Relaxed);
}

/// Record an overlay copy with attribution: count + entries-copied +
/// global max + size-bucket histogram + per-reason max. The histogram
/// tells us whether `entries_total = 12.8B` is "many medium clones" or
/// "a few catastrophic huge clones" — both produce the same total but
/// imply very different fixes (per PR #1630 review).
///
/// Caller passes the parent overlay's len so we can attribute without
/// holding a borrow across the copy.
///
/// Gate once at the top: when counters are disabled the helper returns
/// without paying the `counters()` `OnceLock` deref. When enabled the
/// 10+ atomic operations are direct `fetch_add`/`compare_exchange`
/// calls instead of routing each through `inc()`/`add()`/`record_max()`
/// (which each re-check `enabled_fast()`).
#[inline]
pub fn record_overlay_copy(reason: CheckerCreationReason, entries: u64) {
    if !enabled_fast() {
        return;
    }
    let c = counters();
    c.copy_symbol_file_targets_calls
        .fetch_add(1, Ordering::Relaxed);
    c.copy_symbol_file_targets_entries_total
        .fetch_add(entries, Ordering::Relaxed);
    record_max_inner(&c.copy_symbol_file_targets_entries_max, entries);
    if entries >= 1_000 {
        c.copy_symbol_file_targets_len_ge_1k
            .fetch_add(1, Ordering::Relaxed);
    }
    if entries >= 10_000 {
        c.copy_symbol_file_targets_len_ge_10k
            .fetch_add(1, Ordering::Relaxed);
    }
    if entries >= 100_000 {
        c.copy_symbol_file_targets_len_ge_100k
            .fetch_add(1, Ordering::Relaxed);
    }
    if entries >= 1_000_000 {
        c.copy_symbol_file_targets_len_ge_1m
            .fetch_add(1, Ordering::Relaxed);
    }
    c.overlay_copy_calls_by_reason[reason.as_index()].fetch_add(1, Ordering::Relaxed);
    c.overlay_copy_entries_by_reason[reason.as_index()].fetch_add(entries, Ordering::Relaxed);
    record_max_inner(
        &c.overlay_copy_max_entries_by_reason[reason.as_index()],
        entries,
    );
}

/// `record_max` without the gate check — called from helpers that
/// already gated at the top. Keeps the CAS-loop semantics of the public
/// `record_max` while avoiding a redundant `enabled_fast()` read.
#[inline]
fn record_max_inner(counter: &AtomicU64, value: u64) {
    let mut current = counter.load(Ordering::Relaxed);
    while value > current {
        match counter.compare_exchange_weak(current, value, Ordering::Relaxed, Ordering::Relaxed) {
            Ok(_) => return,
            Err(observed) => current = observed,
        }
    }
}

/// Record one cross-arena symbol miss with source/kind/target attribution.
///
/// Gate once at the top: when counters are disabled the helper returns
/// without paying the `counters()` `OnceLock` deref. When enabled the
/// three atomic increments are direct `fetch_add` calls (no per-call
/// `enabled_fast()` re-check via `inc()`).
#[inline]
pub fn record_cross_arena_symbol_miss(
    source: CrossArenaSymbolMissSource,
    kind: CrossArenaSymbolMissKind,
    target_is_declaration_file: bool,
) {
    if !enabled_fast() {
        return;
    }
    let c = counters();
    c.delegate_cross_arena_symbol_miss_by_source[source.as_index()].fetch_add(1, Ordering::Relaxed);
    c.delegate_cross_arena_symbol_miss_by_kind[kind.as_index()].fetch_add(1, Ordering::Relaxed);
    if target_is_declaration_file {
        c.delegate_cross_arena_symbol_miss_target_declaration_file
            .fetch_add(1, Ordering::Relaxed);
    } else {
        c.delegate_cross_arena_symbol_miss_target_source_file
            .fetch_add(1, Ordering::Relaxed);
    }
}

#[inline]
pub fn record_cross_arena_declaration_file_miss_residue(
    source: CrossArenaSymbolMissSource,
    kind: CrossArenaSymbolMissKind,
    name: &str,
    target_file: Option<&str>,
) {
    if !enabled_fast() {
        return;
    }

    let source_name = CROSS_ARENA_SYMBOL_MISS_SOURCE_NAMES[source.as_index()];
    let kind_name = CROSS_ARENA_SYMBOL_MISS_KIND_NAMES[kind.as_index()];
    let target_file = target_file.map(|file| {
        std::path::Path::new(file)
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or(file)
            .to_owned()
    });
    let mut rows = delegate_declaration_file_miss_residues()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    if let Some(row) = rows.iter_mut().find(|row| {
        row.name == name
            && row.kind == kind_name
            && row.source == source_name
            && row.target_file == target_file
    }) {
        row.count += 1;
        return;
    }

    if rows.len() < DELEGATE_DECLARATION_FILE_MISS_RESIDUE_LIMIT {
        rows.push(DelegateDeclarationFileMissResidue {
            name: name.to_owned(),
            kind: kind_name,
            source: source_name,
            target_file,
            count: 1,
        });
    } else if let Some(row) = rows.iter_mut().find(|row| row.name == "__truncated__") {
        row.count += 1;
    } else {
        rows.push(DelegateDeclarationFileMissResidue {
            name: "__truncated__".to_string(),
            kind: "overflow",
            source: "overflow",
            target_file: None,
            count: 1,
        });
    }
}

#[inline]
pub fn record_cross_arena_source_file_miss_residue(
    source: CrossArenaSymbolMissSource,
    kind: CrossArenaSymbolMissKind,
    name: &str,
    target_file: Option<&str>,
) {
    if !enabled_fast() {
        return;
    }

    let source_name = CROSS_ARENA_SYMBOL_MISS_SOURCE_NAMES[source.as_index()];
    let kind_name = CROSS_ARENA_SYMBOL_MISS_KIND_NAMES[kind.as_index()];
    let target_file = target_file.map(|file| {
        std::path::Path::new(file)
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or(file)
            .to_owned()
    });
    let mut rows = delegate_source_file_miss_residues()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    if let Some(row) = rows.iter_mut().find(|row| {
        row.name == name
            && row.kind == kind_name
            && row.source == source_name
            && row.target_file == target_file
    }) {
        row.count += 1;
        return;
    }

    if rows.len() < DELEGATE_SOURCE_FILE_MISS_RESIDUE_LIMIT {
        rows.push(DelegateSourceFileMissResidue {
            name: name.to_owned(),
            kind: kind_name,
            source: source_name,
            target_file,
            count: 1,
        });
    } else if let Some(row) = rows.iter_mut().find(|row| row.name == "__truncated__") {
        row.count += 1;
    } else {
        rows.push(DelegateSourceFileMissResidue {
            name: "__truncated__".to_string(),
            kind: "overflow",
            source: "overflow",
            target_file: None,
            count: 1,
        });
    }
}

#[inline]
pub fn record_cross_arena_alias_shortcut_outcome(outcome: CrossArenaAliasShortcutOutcome) {
    if !enabled_fast() {
        return;
    }
    let c = counters();
    c.delegate_cross_arena_alias_shortcut_outcome[outcome.as_index()]
        .fetch_add(1, Ordering::Relaxed);
}

/// Classify a `cached_cross_file_*` miss. Called by the four reader
/// helpers in `crates/tsz-checker/src/context/cross_file_query.rs`
/// at each early-return point. See [`CrossFileCacheMissCause`].
#[inline]
pub fn record_cross_file_cache_miss_cause(cause: CrossFileCacheMissCause) {
    if !enabled_fast() {
        return;
    }
    let c = counters();
    c.cross_file_cache_miss_cause[cause.as_index()].fetch_add(1, Ordering::Relaxed);
}

/// Classify whether a source-file symbol-arena delegation is eligible for the
/// post-#6191 cache. Called before the cache lookup so non-cacheable residue is
/// visible in attribution JSON instead of hiding behind the flat miss count.
#[inline]
pub fn record_source_file_symbol_arena_cache_eligibility_outcome(
    outcome: SourceFileSymbolArenaCacheEligibilityOutcome,
) {
    if !enabled_fast() {
        return;
    }
    let c = counters();
    c.source_file_symbol_arena_cache_eligibility_outcome[outcome.as_index()]
        .fetch_add(1, Ordering::Relaxed);
}

/// Record a cross-arena delegate invocation that has no cache fast path —
/// i.e., every call is a miss. Increments both `delegate_cross_arena_calls`
/// and `delegate_cross_arena_misses` with a single `counters()` lookup.
///
/// The hand-rolled call-site pattern this helper replaces was:
///
/// ```rust,ignore
/// if tsz_common::perf_counters::enabled_fast() {
///     tsz_common::perf_counters::inc(
///         &tsz_common::perf_counters::counters().delegate_cross_arena_calls,
///     );
///     tsz_common::perf_counters::inc(
///         &tsz_common::perf_counters::counters().delegate_cross_arena_misses,
///     );
/// }
/// ```
///
/// — which pays two `counters()` `OnceLock` derefs per increment pair.
/// This helper folds them into one. Callers that have a cache fast path
/// (e.g. lib-delegation hit) should keep using `inc(&perf.delegate_cross_arena_calls)`
/// directly and only call this when the miss is unconditional.
#[inline]
pub fn record_delegate_cross_arena_miss() {
    if !enabled_fast() {
        return;
    }
    let c = counters();
    c.delegate_cross_arena_calls.fetch_add(1, Ordering::Relaxed);
    c.delegate_cross_arena_misses
        .fetch_add(1, Ordering::Relaxed);
}

/// Record a cross-file cache hit during cross-arena delegation. Used at
/// three sites in `state/type_analysis/cross_file.rs` where the
/// `cached_cross_file_*_type` fast path returns before the slow
/// child-checker construction would fire.
///
/// Mirrors [`record_delegate_cross_arena_miss`]: gate once, look up
/// `counters()` once, increment both the aggregate call counter and the named
/// per-outcome counter directly. These cross-file fast paths return before the
/// slow child-checker miss path can call [`record_delegate_cross_arena_miss`],
/// so the hit helper owns the aggregate `delegate_cross_arena_calls` bump.
#[inline]
pub fn record_delegate_cross_arena_cache_hit_cross_file() {
    if !enabled_fast() {
        return;
    }
    let c = counters();
    c.delegate_cross_arena_calls.fetch_add(1, Ordering::Relaxed);
    c.delegate_cross_arena_cache_hits_cross_file
        .fetch_add(1, Ordering::Relaxed);
}

/// Record a lib-cache hit during cross-arena class delegation.
#[inline]
pub fn record_delegate_cross_arena_cache_hit_lib() {
    if !enabled_fast() {
        return;
    }
    let c = counters();
    c.delegate_cross_arena_calls.fetch_add(1, Ordering::Relaxed);
    c.delegate_cross_arena_cache_hits_lib
        .fetch_add(1, Ordering::Relaxed);
}

/// Record a hit on the cross-file type-parameter extraction cache. Mirrors
/// [`record_delegate_cross_arena_miss`]: gate-once and one `counters()`
/// lookup, then increment exactly the per-outcome counter that names this
/// branch of the cache.
#[inline]
pub fn record_cross_file_type_params_cache_hit() {
    if !enabled_fast() {
        return;
    }
    counters()
        .cross_file_type_params_cache_hits
        .fetch_add(1, Ordering::Relaxed);
}

/// Record a miss on the cross-file type-parameter extraction cache. Counted
/// when the slow path runs to build a child checker, regardless of whether
/// the slow path ultimately returns `Some(_)` — see the call sites in
/// `state/type_environment/core.rs` for the rationale (counting only on
/// `Some(_)` undercounts misses when the slow path runs but extraction fails,
/// distorting attribution for Tier-2 decision-making).
#[inline]
pub fn record_cross_file_type_params_cache_miss() {
    if !enabled_fast() {
        return;
    }
    counters()
        .cross_file_type_params_cache_misses
        .fetch_add(1, Ordering::Relaxed);
}

/// Record an entry to `get_type_of_symbol`'s computation path. Sits on a
/// multi-million-call hot path (`state/type_analysis/computed/mod.rs`),
/// so gating once before the `counters()` `OnceLock` deref is the load-
/// bearing optimization — disabled builds pay one branch and one
/// relaxed atomic load on `ENABLED_FAST`, not a deref.
#[inline]
pub fn record_compute_type_of_symbol_call() {
    if !enabled_fast() {
        return;
    }
    counters()
        .compute_type_of_symbol_calls
        .fetch_add(1, Ordering::Relaxed);
}

/// Record a cache hit on `get_type_of_symbol`'s `symbol_types` lookup.
/// Used at two sites in `state/type_analysis/core.rs` (the provisional-
/// type and the standard cached-type branches of the recursion guard).
/// Compared against [`record_compute_type_of_symbol_call`] in attribution
/// mode to characterize recomputation pressure.
#[inline]
pub fn record_compute_type_of_symbol_cache_hit() {
    if !enabled_fast() {
        return;
    }
    counters()
        .compute_type_of_symbol_cache_hits
        .fetch_add(1, Ordering::Relaxed);
}

/// Record use of the simple local-interface object shortcut inside
/// `compute_type_of_symbol`.
#[inline]
pub fn record_compute_type_of_symbol_interface_simple_object_fastpath_hit() {
    if !enabled_fast() {
        return;
    }
    counters()
        .compute_type_of_symbol_interface_simple_object_fastpath_hits
        .fetch_add(1, Ordering::Relaxed);
}

/// Record how `compute_type_of_symbol` sourced the symbol payload.
#[inline]
pub fn record_compute_type_of_symbol_source_outcome(outcome: ComputeTypeOfSymbolSourceOutcome) {
    if !enabled_fast() {
        return;
    }
    counters().compute_type_of_symbol_source_outcome[outcome.as_index()]
        .fetch_add(1, Ordering::Relaxed);
}

/// Record the coarse symbol-kind bucket lowered by `compute_type_of_symbol`.
#[inline]
pub fn record_compute_type_of_symbol_kind_outcome(outcome: ComputeTypeOfSymbolKindOutcome) {
    if !enabled_fast() {
        return;
    }
    counters().compute_type_of_symbol_kind_outcome[outcome.as_index()]
        .fetch_add(1, Ordering::Relaxed);
}

/// Record which interface fast-path combination ran inside
/// `compute_type_of_symbol`.
#[inline]
pub fn record_compute_type_of_symbol_interface_fastpath_outcome(
    outcome: ComputeTypeOfSymbolInterfaceFastPathOutcome,
) {
    if !enabled_fast() {
        return;
    }
    counters().compute_type_of_symbol_interface_fastpath_outcome[outcome.as_index()]
        .fetch_add(1, Ordering::Relaxed);
}

/// Record call-site parent-kind attribution for interface calls in
/// `compute_type_of_symbol`.
#[inline]
pub fn record_compute_type_of_symbol_interface_callsite_outcome(
    outcome: ComputeTypeOfSymbolInterfaceCallsiteOutcome,
) {
    if !enabled_fast() {
        return;
    }
    counters().compute_type_of_symbol_interface_callsite_outcome[outcome.as_index()]
        .fetch_add(1, Ordering::Relaxed);
}

/// Record success/reject outcomes for the simple local-interface object
/// shortcut in `compute_type_of_symbol`.
#[inline]
pub fn record_compute_type_of_symbol_interface_simple_object_outcome(
    outcome: ComputeTypeOfSymbolInterfaceSimpleObjectOutcome,
) {
    if !enabled_fast() {
        return;
    }
    counters().compute_type_of_symbol_interface_simple_object_outcome[outcome.as_index()]
        .fetch_add(1, Ordering::Relaxed);
}

/// Record annotation-kind attribution for
/// `RejectNonPrimitiveAnnotation` outcomes in the simple local-interface
/// object shortcut.
#[inline]
pub fn record_compute_type_of_symbol_interface_simple_object_non_primitive_annotation_kind(
    kind: ComputeTypeOfSymbolInterfaceSimpleObjectNonPrimitiveAnnotationKind,
) {
    if !enabled_fast() {
        return;
    }
    counters().compute_type_of_symbol_interface_simple_object_non_primitive_annotation_kind
        [kind.as_index()]
    .fetch_add(1, Ordering::Relaxed);
}

/// Record bounded source-level residue for non-primitive annotations rejected
/// by the simple local-interface object shortcut.
#[inline]
pub fn record_compute_type_of_symbol_interface_simple_object_non_primitive_annotation_residue(
    kind: ComputeTypeOfSymbolInterfaceSimpleObjectNonPrimitiveAnnotationKind,
    interface: Option<&str>,
    property: Option<&str>,
) {
    if !enabled_fast() {
        return;
    }

    let kind_name =
        COMPUTE_TYPE_OF_SYMBOL_INTERFACE_SIMPLE_OBJECT_NON_PRIMITIVE_ANNOTATION_KIND_NAMES
            [kind.as_index()];
    let mut rows =
        compute_type_of_symbol_interface_simple_object_non_primitive_annotation_residues()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
    if let Some(row) = rows.iter_mut().find(|row| {
        row.kind == kind_name
            && row.interface.as_deref() == interface
            && row.property.as_deref() == property
    }) {
        row.count += 1;
        return;
    }

    if rows.len()
        < COMPUTE_TYPE_OF_SYMBOL_INTERFACE_SIMPLE_OBJECT_NON_PRIMITIVE_ANNOTATION_RESIDUE_LIMIT
    {
        rows.push(
            ComputeTypeOfSymbolInterfaceSimpleObjectNonPrimitiveAnnotationResidue {
                kind: kind_name,
                interface: interface.map(str::to_owned),
                property: property.map(str::to_owned),
                count: 1,
            },
        );
    } else if let Some(row) = rows
        .iter_mut()
        .find(|row| row.interface.as_deref() == Some("__truncated__"))
    {
        row.count += 1;
    } else {
        rows.push(
            ComputeTypeOfSymbolInterfaceSimpleObjectNonPrimitiveAnnotationResidue {
                kind: "overflow",
                interface: Some("__truncated__".to_string()),
                property: None,
                count: 1,
            },
        );
    }
}

/// Record bounded symbol-level residue for declaration/provenance guards
/// rejected by the simple local-interface object shortcut.
#[inline]
pub fn record_compute_type_of_symbol_interface_simple_object_declaration_provenance_residue(
    outcome: ComputeTypeOfSymbolInterfaceSimpleObjectOutcome,
    symbol: Option<&str>,
    declaration_count: usize,
) {
    if !enabled_fast() {
        return;
    }

    let outcome_name =
        COMPUTE_TYPE_OF_SYMBOL_INTERFACE_SIMPLE_OBJECT_OUTCOME_NAMES[outcome.as_index()];
    let declaration_count = declaration_count as u64;
    let mut rows = compute_type_of_symbol_interface_simple_object_declaration_provenance_residues()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    if let Some(row) = rows.iter_mut().find(|row| {
        row.outcome == outcome_name
            && row.symbol.as_deref() == symbol
            && row.declaration_count == declaration_count
    }) {
        row.count += 1;
        return;
    }

    if rows.len()
        < COMPUTE_TYPE_OF_SYMBOL_INTERFACE_SIMPLE_OBJECT_DECLARATION_PROVENANCE_RESIDUE_LIMIT
    {
        rows.push(
            ComputeTypeOfSymbolInterfaceSimpleObjectDeclarationProvenanceResidue {
                outcome: outcome_name,
                symbol: symbol.map(str::to_owned),
                declaration_count,
                count: 1,
            },
        );
    } else if let Some(row) = rows
        .iter_mut()
        .find(|row| row.symbol.as_deref() == Some("__truncated__"))
    {
        row.count += 1;
    } else {
        rows.push(
            ComputeTypeOfSymbolInterfaceSimpleObjectDeclarationProvenanceResidue {
                outcome: "overflow",
                symbol: Some("__truncated__".to_string()),
                declaration_count: 0,
                count: 1,
            },
        );
    }
}

/// Record attribution for why a `type_reference` annotation was still rejected
/// by the simple local-interface object shortcut.
#[inline]
pub fn record_compute_type_of_symbol_interface_simple_object_type_reference_reject_outcome(
    outcome: ComputeTypeOfSymbolInterfaceSimpleObjectTypeReferenceRejectOutcome,
) {
    if !enabled_fast() {
        return;
    }
    counters().compute_type_of_symbol_interface_simple_object_type_reference_reject_outcome
        [outcome.as_index()]
    .fetch_add(1, Ordering::Relaxed);
}

/// Record bounded name-level residue for type-reference annotations rejected by
/// the simple local-interface object shortcut.
#[inline]
pub fn record_compute_type_of_symbol_interface_simple_object_type_reference_reject_residue(
    outcome: ComputeTypeOfSymbolInterfaceSimpleObjectTypeReferenceRejectOutcome,
    name: &str,
) {
    if !enabled_fast() {
        return;
    }

    let outcome_name =
        COMPUTE_TYPE_OF_SYMBOL_INTERFACE_SIMPLE_OBJECT_TYPE_REFERENCE_REJECT_OUTCOME_NAMES
            [outcome.as_index()];
    let mut rows = compute_type_of_symbol_interface_simple_object_type_reference_reject_residues()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    if let Some(row) = rows
        .iter_mut()
        .find(|row| row.name == name && row.outcome == outcome_name)
    {
        row.count += 1;
        return;
    }

    if rows.len()
        < COMPUTE_TYPE_OF_SYMBOL_INTERFACE_SIMPLE_OBJECT_TYPE_REFERENCE_REJECT_RESIDUE_LIMIT
    {
        rows.push(
            ComputeTypeOfSymbolInterfaceSimpleObjectTypeReferenceRejectResidue {
                name: name.to_owned(),
                outcome: outcome_name,
                count: 1,
            },
        );
    } else if let Some(row) = rows.iter_mut().find(|row| row.name == "__truncated__") {
        row.count += 1;
    } else {
        rows.push(
            ComputeTypeOfSymbolInterfaceSimpleObjectTypeReferenceRejectResidue {
                name: "__truncated__".to_string(),
                outcome: "overflow",
                count: 1,
            },
        );
    }
}

pub fn record_property_classification_call() {
    inc(&counters().property_classification_calls);
}

pub fn record_property_classification_string_fallback_source_lookup() {
    inc(&counters().property_classification_string_fallback_source_lookups);
}

pub fn record_property_classification_string_fallback_target_name() {
    inc(&counters().property_classification_string_fallback_target_names);
}

pub fn record_property_classification_string_fallback_target_type() {
    inc(&counters().property_classification_string_fallback_target_types);
}

/// Record a `TypeInterner::intern_string` call. Mirrors the existing
/// `record_compute_type_of_symbol_*` shape: gate once, one `counters()`
/// lookup, increment exactly the named field.
///
/// `intern_string` is a fundamental hot path — every property name,
/// every string literal, every diagnostic message tag eventually flows
/// through it. The wrapper keeps that path cheap when counters are
/// disabled (one `OnceLock<bool>` read, no `OnceLock<PerfCounters>`
/// deref) without spreading the gate/deref pair across every interner
/// entry point.
#[inline]
pub fn record_interner_string_intern_call() {
    if !enabled_fast() {
        return;
    }
    counters()
        .interner_string_intern_calls
        .fetch_add(1, Ordering::Relaxed);
}

/// Record a `TypeInterner::intern_type_list` call (covers both the
/// owning `Vec` entry point and the borrowed-slice entry point).
#[inline]
pub fn record_interner_type_list_intern_call() {
    if !enabled_fast() {
        return;
    }
    counters()
        .interner_type_list_intern_calls
        .fetch_add(1, Ordering::Relaxed);
}

/// Record a `TypeInterner::intern_object_shape` call.
#[inline]
pub fn record_interner_object_shape_intern_call() {
    if !enabled_fast() {
        return;
    }
    counters()
        .interner_object_shape_intern_calls
        .fetch_add(1, Ordering::Relaxed);
}

/// Record a `TypeInterner::intern_function_shape` call.
#[inline]
pub fn record_interner_function_shape_intern_call() {
    if !enabled_fast() {
        return;
    }
    counters()
        .interner_function_shape_intern_calls
        .fetch_add(1, Ordering::Relaxed);
}

/// Record a `TypeInterner::intern_conditional_type` call.
#[inline]
pub fn record_interner_conditional_intern_call() {
    if !enabled_fast() {
        return;
    }
    counters()
        .interner_conditional_intern_calls
        .fetch_add(1, Ordering::Relaxed);
}

/// Record a `TypeInterner::intern_mapped_type` call.
#[inline]
pub fn record_interner_mapped_intern_call() {
    if !enabled_fast() {
        return;
    }
    counters()
        .interner_mapped_intern_calls
        .fetch_add(1, Ordering::Relaxed);
}

/// Record a `TypeInterner::intern_callable_shape` call. Mirrors the
/// sibling `record_interner_function_shape_intern_call` shape — gate
/// once, one `counters()` lookup, increment the named field.
#[inline]
pub fn record_interner_callable_shape_intern_call() {
    if !enabled_fast() {
        return;
    }
    counters()
        .interner_callable_shape_intern_calls
        .fetch_add(1, Ordering::Relaxed);
}

/// Record a `TypeInterner::intern_application` call.
#[inline]
pub fn record_interner_application_intern_call() {
    if !enabled_fast() {
        return;
    }
    counters()
        .interner_application_intern_calls
        .fetch_add(1, Ordering::Relaxed);
}

/// Record one `ModuleResolver::lookup()` call from
/// `crates/tsz-cli/src/driver/sources.rs` — the entry point for per-import
/// module resolution. Sibling to the fs-probe `record_resolver_*`
/// helpers but lives in a different file (sources.rs vs resolution.rs)
/// because resolution caching happens at the lookup level, above the
/// individual fs-probe wrappers.
#[inline]
pub fn record_resolver_lookup_call() {
    if !enabled_fast() {
        return;
    }
    counters()
        .resolver_lookup_calls
        .fetch_add(1, Ordering::Relaxed);
}

/// Record one `Path::is_file()` probe from the resolver fast path.
/// Used by the `count_is_file` wrapper in `crates/tsz-cli/src/driver/resolution.rs`,
/// which bundles the syscall and the counter in one place. Gate once,
/// deref `counters()` once, increment.
#[inline]
pub fn record_resolver_is_file() {
    if !enabled_fast() {
        return;
    }
    counters()
        .resolver_is_file_calls
        .fetch_add(1, Ordering::Relaxed);
}

/// Record one `Path::is_dir()` probe from the resolver fast path.
/// Sibling to [`record_resolver_is_file`].
#[inline]
pub fn record_resolver_is_dir() {
    if !enabled_fast() {
        return;
    }
    counters()
        .resolver_is_dir_calls
        .fetch_add(1, Ordering::Relaxed);
}

/// Record one `std::fs::read_dir()` call from the resolver. Sibling to
/// [`record_resolver_is_file`]. The cost of the syscall itself dwarfs
/// the counter overhead — this helper is only structural cleanup.
#[inline]
pub fn record_resolver_read_dir() {
    if !enabled_fast() {
        return;
    }
    counters()
        .resolver_read_dir_calls
        .fetch_add(1, Ordering::Relaxed);
}

/// Record one candidate path examined during module resolution
/// (path-mapping virtual roots and suffix-extension expansion).
/// Lifted into a helper so the two emit sites in
/// `crates/tsz-cli/src/driver/resolution.rs` don't re-pay the `counters()`
/// `OnceLock` deref.
#[inline]
pub fn record_resolver_candidate_path() {
    if !enabled_fast() {
        return;
    }
    counters()
        .resolver_candidate_paths_total
        .fetch_add(1, Ordering::Relaxed);
}

/// Record one uncached `package.json` read. Sits inside the resolver's
/// `read_package_json_uncached`, which `large-ts-repo` profiles flag
/// as the dominant resolver work — keeping the gate cheap matters even
/// though the surrounding `read_to_string` is several orders of
/// magnitude more expensive.
#[inline]
pub fn record_resolver_read_package_json() {
    if !enabled_fast() {
        return;
    }
    counters()
        .resolver_read_package_json_calls
        .fetch_add(1, Ordering::Relaxed);
}

/// Record a root `CheckerState` construction. Called from each of the
/// nine `CheckerState::new` / `with_*` constructors in
/// `crates/tsz-checker/src/state/state.rs`. Sibling to the other `record_*`
/// helpers — gate once, look up `counters()` once, increment.
#[inline]
pub fn record_checker_state_constructed() {
    if !enabled_fast() {
        return;
    }
    counters()
        .checker_state_constructed
        .fetch_add(1, Ordering::Relaxed);
}

/// Record an invocation of `CheckerContext::reset_for_next_file()`. Bumps
/// only on the sequential session-reuse path (T2.1.B). Sibling to the
/// other `record_*` helpers — gate once, look up `counters()` once,
/// increment. Compared against `checker_state_constructed` in
/// attribution mode to detect reuse-vs-construct directly.
#[inline]
pub fn record_file_session_reset() {
    if !enabled_fast() {
        return;
    }
    counters()
        .file_session_resets
        .fetch_add(1, Ordering::Relaxed);
}

#[inline]
pub fn record_direct_cross_file_interface_lowering_outcome(
    outcome: DirectCrossFileInterfaceLoweringOutcome,
) {
    if !enabled_fast() {
        return;
    }
    let c = counters();
    c.direct_cross_file_interface_lowering_outcome[outcome.as_index()]
        .fetch_add(1, Ordering::Relaxed);
}

#[inline]
pub fn record_direct_actual_lib_alias_body_outcome(outcome: DirectActualLibAliasBodyOutcome) {
    if !enabled_fast() {
        return;
    }
    let c = counters();
    c.direct_actual_lib_alias_body_outcome[outcome.as_index()].fetch_add(1, Ordering::Relaxed);
}

#[inline]
pub fn record_direct_source_file_type_alias_lowering_outcome(
    outcome: DirectSourceFileTypeAliasLoweringOutcome,
) {
    if !enabled_fast() {
        return;
    }
    let c = counters();
    c.direct_source_file_type_alias_lowering_outcome[outcome.as_index()]
        .fetch_add(1, Ordering::Relaxed);
}

#[inline]
pub fn record_direct_source_file_type_alias_body_rejection_kind(
    kind: DirectSourceFileTypeAliasBodyRejectionKind,
) {
    if !enabled_fast() {
        return;
    }
    let c = counters();
    c.direct_source_file_type_alias_body_rejection_kind[kind.as_index()]
        .fetch_add(1, Ordering::Relaxed);
}

#[inline]
pub fn record_direct_source_file_type_alias_type_reference_rejection_kind(
    kind: DirectSourceFileTypeAliasTypeReferenceRejectionKind,
) {
    if !enabled_fast() {
        return;
    }
    let c = counters();
    c.direct_source_file_type_alias_type_reference_rejection_kind[kind.as_index()]
        .fetch_add(1, Ordering::Relaxed);
}

#[inline]
pub fn record_direct_source_file_type_alias_first_type_reference_rejection_kind(
    kind: DirectSourceFileTypeAliasTypeReferenceRejectionKind,
) {
    if !enabled_fast() {
        return;
    }
    let c = counters();
    c.direct_source_file_type_alias_first_type_reference_rejection_kind[kind.as_index()]
        .fetch_add(1, Ordering::Relaxed);
}

#[inline]
pub fn record_direct_source_file_type_alias_body_rejection_residue(
    residue: DirectSourceFileTypeAliasBodyRejectionResidueInput<'_>,
) {
    if !enabled_fast() {
        return;
    }

    let body_kind_name =
        DIRECT_SOURCE_FILE_TYPE_ALIAS_BODY_REJECTION_KIND_NAMES[residue.body_kind.as_index()];
    let first_type_reference_kind_name = residue.first_type_reference_kind.map(|kind| {
        DIRECT_SOURCE_FILE_TYPE_ALIAS_TYPE_REFERENCE_REJECTION_KIND_NAMES[kind.as_index()]
    });
    let first_type_reference_name = residue.first_type_reference_name.map(str::to_owned);
    let first_non_lowerable_type_reference_kind_name =
        residue.first_non_lowerable_type_reference_kind.map(|kind| {
            DIRECT_SOURCE_FILE_TYPE_ALIAS_TYPE_REFERENCE_REJECTION_KIND_NAMES[kind.as_index()]
        });
    let first_non_lowerable_type_reference_name = residue
        .first_non_lowerable_type_reference_name
        .map(str::to_owned);
    let first_non_lowerable_leaf_type_reference_kind_name = residue
        .first_non_lowerable_leaf_type_reference_kind
        .map(|kind| {
            DIRECT_SOURCE_FILE_TYPE_ALIAS_TYPE_REFERENCE_REJECTION_KIND_NAMES[kind.as_index()]
        });
    let first_non_lowerable_leaf_type_reference_name = residue
        .first_non_lowerable_leaf_type_reference_name
        .map(str::to_owned);
    let target_file = residue.target_file.map(|file| {
        std::path::Path::new(file)
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or(file)
            .to_owned()
    });
    let mut rows = direct_source_file_type_alias_body_rejection_residues()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    if let Some(row) = rows.iter_mut().find(|row| {
        row.name == residue.name
            && row.body_kind == body_kind_name
            && row.first_type_reference_kind == first_type_reference_kind_name
            && row.first_type_reference_name == first_type_reference_name
            && row.first_non_lowerable_type_reference_kind
                == first_non_lowerable_type_reference_kind_name
            && row.first_non_lowerable_type_reference_name
                == first_non_lowerable_type_reference_name
            && row.first_non_lowerable_leaf_type_reference_kind
                == first_non_lowerable_leaf_type_reference_kind_name
            && row.first_non_lowerable_leaf_type_reference_name
                == first_non_lowerable_leaf_type_reference_name
            && row.target_file == target_file
    }) {
        row.count += 1;
        return;
    }

    if rows.len() < DIRECT_SOURCE_FILE_TYPE_ALIAS_BODY_REJECTION_RESIDUE_LIMIT {
        rows.push(DirectSourceFileTypeAliasBodyRejectionResidue {
            name: residue.name.to_owned(),
            body_kind: body_kind_name,
            first_type_reference_kind: first_type_reference_kind_name,
            first_type_reference_name,
            first_non_lowerable_type_reference_kind: first_non_lowerable_type_reference_kind_name,
            first_non_lowerable_type_reference_name,
            first_non_lowerable_leaf_type_reference_kind:
                first_non_lowerable_leaf_type_reference_kind_name,
            first_non_lowerable_leaf_type_reference_name,
            target_file,
            count: 1,
        });
    } else if let Some(row) = rows.iter_mut().find(|row| row.name == "__truncated__") {
        row.count += 1;
    } else {
        rows.push(DirectSourceFileTypeAliasBodyRejectionResidue {
            name: "__truncated__".to_string(),
            body_kind: "overflow",
            first_type_reference_kind: Some("overflow"),
            first_type_reference_name: Some("overflow".to_string()),
            first_non_lowerable_type_reference_kind: Some("overflow"),
            first_non_lowerable_type_reference_name: Some("overflow".to_string()),
            first_non_lowerable_leaf_type_reference_kind: Some("overflow"),
            first_non_lowerable_leaf_type_reference_name: Some("overflow".to_string()),
            target_file: None,
            count: 1,
        });
    }
}

#[inline]
pub fn record_direct_actual_lib_intl_interface_outcome(
    outcome: DirectActualLibIntlInterfaceOutcome,
) {
    if !enabled_fast() {
        return;
    }
    let c = counters();
    c.direct_actual_lib_intl_interface_outcome[outcome.as_index()].fetch_add(1, Ordering::Relaxed);
}

/// Record one semantic `check_source_file` duration in attribution mode.
///
/// This intentionally stores only a bounded top-N list. The call site gates
/// `Instant::now()` behind [`enabled_fast`], so timing-mode runs where
/// `TSZ_PERF_COUNTERS` is unset do not pay for clock reads.
pub fn record_slow_check_file_timing(file: &str, elapsed_ns: u64, diagnostics: u64) {
    if !enabled_fast() {
        return;
    }
    let mut rows = slow_check_file_timings()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    rows.push(SlowCheckFileTiming {
        file: file.to_owned(),
        elapsed_ms: elapsed_ns as f64 / 1_000_000.0,
        diagnostics,
    });
    rows.sort_by(|a, b| {
        b.elapsed_ms
            .total_cmp(&a.elapsed_ms)
            .then_with(|| a.file.cmp(&b.file))
    });
    rows.truncate(SLOW_CHECK_FILE_TIMING_LIMIT);
}

/// Record one top-level statement duration inside semantic `check_source_file`.
///
/// This is attribution-only: callers gate `Instant::now()` behind
/// [`enabled_fast`], so timing-mode runs do not pay for clock reads. The rows
/// intentionally store syntax coordinates rather than source snippets so the
/// counter stays structural and cheap.
pub fn record_slow_check_statement_timing(
    file: &str,
    kind: u16,
    pos: u32,
    end: u32,
    elapsed_ns: u64,
) {
    if !enabled_fast() {
        return;
    }
    let mut rows = slow_check_statement_timings()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    rows.push(SlowCheckStatementTiming {
        file: file.to_owned(),
        kind,
        pos,
        end,
        elapsed_ms: elapsed_ns as f64 / 1_000_000.0,
    });
    rows.sort_by(|a, b| {
        b.elapsed_ms
            .total_cmp(&a.elapsed_ms)
            .then_with(|| a.file.cmp(&b.file))
            .then_with(|| a.pos.cmp(&b.pos))
            .then_with(|| a.end.cmp(&b.end))
            .then_with(|| a.kind.cmp(&b.kind))
    });
    rows.truncate(SLOW_CHECK_STATEMENT_TIMING_LIMIT);
}

/// Record one type-alias checking phase duration in attribution mode.
///
/// Callers gate `Instant::now()` behind [`enabled_fast`], so timing-mode runs
/// do not pay for clock reads. The alias name is an output label only; it must
/// never drive compiler behavior.
pub fn record_slow_type_alias_check_timing(
    file: &str,
    name: Option<&str>,
    phase: &'static str,
    pos: u32,
    end: u32,
    elapsed_ns: u64,
) {
    if !enabled_fast() {
        return;
    }
    let mut rows = slow_type_alias_check_timings()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    rows.push(SlowTypeAliasCheckTiming {
        file: file.to_owned(),
        name: name.unwrap_or("<anonymous>").to_owned(),
        phase,
        pos,
        end,
        elapsed_ms: elapsed_ns as f64 / 1_000_000.0,
    });
    rows.sort_by(|a, b| {
        b.elapsed_ms
            .total_cmp(&a.elapsed_ms)
            .then_with(|| a.file.cmp(&b.file))
            .then_with(|| a.name.cmp(&b.name))
            .then_with(|| a.phase.cmp(b.phase))
            .then_with(|| a.pos.cmp(&b.pos))
            .then_with(|| a.end.cmp(&b.end))
    });
    rows.truncate(SLOW_TYPE_ALIAS_CHECK_TIMING_LIMIT);
}

/// Record a raw `SymbolId`-shaped `DefId` redirect inside
/// `TypeEnvironment::resolve_lazy`.
///
/// This is Track 7 instrumentation for removing legacy
/// `interner.reference(SymbolRef)` producers. It is intentionally a flat
/// counter: the call site also emits structured tracing fields with the raw
/// and redirected IDs when trace logging is enabled.
pub fn record_type_environment_raw_symbol_lazy_fallback() {
    inc(&counters().type_environment_raw_symbol_lazy_fallbacks);
}

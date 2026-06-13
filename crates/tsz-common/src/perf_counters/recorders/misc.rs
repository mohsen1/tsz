pub fn record_checker_lib_clone(file_count: u64, parallel: bool, elapsed_ns: u64) {
    if !enabled_fast() {
        return;
    }
    let c = counters();
    c.checker_lib_clone_calls.fetch_add(1, Ordering::Relaxed);
    if parallel {
        c.checker_lib_clone_parallel_calls
            .fetch_add(1, Ordering::Relaxed);
    }
    c.checker_lib_clone_files_total
        .fetch_add(file_count, Ordering::Relaxed);
    c.checker_lib_clone_elapsed_ns_total
        .fetch_add(elapsed_ns, Ordering::Relaxed);
    record_max(&c.checker_lib_clone_elapsed_ns_max, elapsed_ns);
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

#[inline]
pub fn record_cross_arena_declaration_file_miss_residue(

#[inline]
pub fn record_cross_arena_source_file_miss_residue(

#[inline]
pub fn record_cross_arena_alias_shortcut_outcome(outcome: CrossArenaAliasShortcutOutcome) {
    if !enabled_fast() {
        return;
    }
    let c = counters();
    c.delegate_cross_arena_alias_shortcut_outcome[outcome.as_index()]
        .fetch_add(1, Ordering::Relaxed);
}

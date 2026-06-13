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

/// Record that a full-work cross-arena delegation (a miss that ran a child
/// checker) completed with a sentinel (`ERROR`/`UNKNOWN`) result that the
/// shared cross-file buckets refuse to store.
#[inline]
pub fn record_delegate_cross_arena_full_work_sentinel_result() {
    if !enabled_fast() {
        return;
    }
    counters()
        .delegate_cross_arena_full_work_sentinel_results
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

#[inline]
pub fn record_direct_cross_file_interface_lowering_outcome(

#[inline]
pub fn record_direct_cross_file_interface_complex_reason(

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

#[inline]
pub fn record_direct_source_file_type_alias_body_rejection_kind(

#[inline]
pub fn record_direct_source_file_type_alias_type_reference_rejection_kind(

#[inline]
pub fn record_direct_source_file_type_alias_first_type_reference_rejection_kind(

#[inline]
pub fn record_direct_source_file_type_alias_body_rejection_residue(

#[inline]
pub fn record_direct_actual_lib_intl_interface_outcome(

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

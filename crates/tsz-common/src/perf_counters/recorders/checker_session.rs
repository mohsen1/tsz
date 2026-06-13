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

/// Record high-water retained checker-context cache sizes immediately before
/// a reused checker clears file-local state. This is attribution-only data for
/// issue #13246's session-reuse accumulation audit; it never changes reset or
/// cache behavior.
pub struct FileSessionResetCacheStatistics {
    /// Total retained cache entries observed at the reset boundary.
    pub total_entries: u64,
    /// Estimated total retained cache bytes observed at the reset boundary.
    pub total_bytes: u64,
    /// Namespace-member resolution cache entries observed at reset.
    pub namespace_member_entries: u64,
    /// Namespace-member resolution cache estimated bytes observed at reset.
    pub namespace_member_bytes: u64,
    /// `export =` named cache entries observed at reset.
    pub export_equals_entries: u64,
    /// `export =` named cache estimated bytes observed at reset.
    pub export_equals_bytes: u64,
    /// Nested-namespace candidate cache entries observed at reset.
    pub nested_namespace_entries: u64,
    /// Nested-namespace candidate cache estimated bytes observed at reset.
    pub nested_namespace_bytes: u64,
    /// Lowering entity-name resolution cache entries observed at reset.
    pub lowering_entity_name_entries: u64,
    /// Lowering entity-name resolution cache estimated bytes observed at reset.
    pub lowering_entity_name_bytes: u64,
    /// Environment evaluation cache entries observed at reset.
    pub env_eval_entries: u64,
    /// Environment evaluation cache estimated bytes observed at reset.
    pub env_eval_bytes: u64,
}

#[inline]
pub fn record_file_session_reset_cache_statistics(stats: FileSessionResetCacheStatistics) {
    let FileSessionResetCacheStatistics {
        total_entries,
        total_bytes,
        namespace_member_entries,
        namespace_member_bytes,
        export_equals_entries,
        export_equals_bytes,
        nested_namespace_entries,
        nested_namespace_bytes,
        lowering_entity_name_entries,
        lowering_entity_name_bytes,
        env_eval_entries,
        env_eval_bytes,
    } = stats;
    if !enabled_fast() {
        return;
    }
    let c = counters();
    record_max_inner(&c.file_session_reset_cache_entries_max, total_entries);
    record_max_inner(&c.file_session_reset_cache_bytes_max, total_bytes);
    record_max_inner(
        &c.file_session_reset_namespace_member_entries_max,
        namespace_member_entries,
    );
    record_max_inner(
        &c.file_session_reset_namespace_member_bytes_max,
        namespace_member_bytes,
    );
    record_max_inner(
        &c.file_session_reset_export_equals_entries_max,
        export_equals_entries,
    );
    record_max_inner(
        &c.file_session_reset_export_equals_bytes_max,
        export_equals_bytes,
    );
    record_max_inner(
        &c.file_session_reset_nested_namespace_entries_max,
        nested_namespace_entries,
    );
    record_max_inner(
        &c.file_session_reset_nested_namespace_bytes_max,
        nested_namespace_bytes,
    );
    record_max_inner(
        &c.file_session_reset_lowering_entity_name_entries_max,
        lowering_entity_name_entries,
    );
    record_max_inner(
        &c.file_session_reset_lowering_entity_name_bytes_max,
        lowering_entity_name_bytes,
    );
    record_max_inner(&c.file_session_reset_env_eval_entries_max, env_eval_entries);
    record_max_inner(&c.file_session_reset_env_eval_bytes_max, env_eval_bytes);
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
            .then_with(|| a.kind.cmp(&b.kind))
            .then_with(|| a.pos.cmp(&b.pos))
            .then_with(|| a.end.cmp(&b.end))
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
    name: &str,
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
        name: name.to_owned(),
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

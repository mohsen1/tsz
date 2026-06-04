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

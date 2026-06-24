//! Fast paths for fatal config deprecation diagnostics.

use super::{
    CliArgs, CompilationResult, Diagnostic, FileInfo, FxHashSet, PhaseTimings,
    ResolvedCompilerOptions, SourceEntry, apply_fatal_config_notice_priority,
    collect_parse_only_no_check_diagnostics, collect_source_reference_lib_diagnostics,
};
use std::path::PathBuf;
#[cfg(test)]
use std::sync::Mutex;
use std::time::{Duration, Instant};

const TRY_TSZ_WORKER_CONFIG_DEPRECATION_ENV_KEY: &str = "TSZ_TRY_TSZ_WORKER";

#[cfg(test)]
static TEST_TRY_TSZ_WORKER_CONFIG_DEPRECATION: Mutex<Option<bool>> = Mutex::new(None);

#[cfg(test)]
pub(super) fn with_try_tsz_worker_config_deprecation<T>(enabled: bool, f: impl FnOnce() -> T) -> T {
    struct Guard(Option<bool>);

    impl Drop for Guard {
        fn drop(&mut self) {
            *TEST_TRY_TSZ_WORKER_CONFIG_DEPRECATION
                .lock()
                .expect("lock try-tsz worker config deprecation override") = self.0;
        }
    }

    let previous = {
        let mut slot = TEST_TRY_TSZ_WORKER_CONFIG_DEPRECATION
            .lock()
            .expect("lock try-tsz worker config deprecation override");
        let previous = *slot;
        *slot = Some(enabled);
        previous
    };
    let _guard = Guard(previous);
    f()
}

fn try_tsz_worker_config_deprecation_enabled() -> bool {
    #[cfg(test)]
    if let Some(enabled) = *TEST_TRY_TSZ_WORKER_CONFIG_DEPRECATION
        .lock()
        .expect("lock try-tsz worker config deprecation override")
    {
        return enabled;
    }

    std::env::var_os(TRY_TSZ_WORKER_CONFIG_DEPRECATION_ENV_KEY).is_some()
}

pub(super) struct NoEmitDeprecationInput<'a> {
    pub(super) args: &'a CliArgs,
    pub(super) resolved: &'a ResolvedCompilerOptions,
    pub(super) has_deprecation_diagnostics: bool,
    pub(super) sources: &'a [SourceEntry],
    pub(super) config_diagnostics: &'a [Diagnostic],
    pub(super) binary_file_diagnostics: &'a [Diagnostic],
    pub(super) binary_file_names_to_suppress: &'a FxHashSet<String>,
    pub(super) type_file_diagnostics: &'a [Diagnostic],
    pub(super) user_files_read: &'a [PathBuf],
    pub(super) file_infos: &'a [FileInfo],
    pub(super) io_read_duration: Duration,
    pub(super) compile_start: Instant,
    pub(super) perf_log_phase: &'a dyn Fn(&'static str, Instant),
}

pub(super) fn maybe_compile_no_emit_deprecation(
    input: NoEmitDeprecationInput<'_>,
) -> Option<CompilationResult> {
    // TS5101/TS5107 config deprecations are fatal for ordinary `--noEmit`
    // CLI checks unless a real source grammar error suppresses them. Do that
    // grammar check with a parse-only pass before loading libs or merging bind
    // results; large declaration dependencies can otherwise spend seconds in
    // binder merge/drop work even though `tsc --pretty false` reports only the
    // config deprecation.
    if !try_tsz_worker_config_deprecation_enabled()
        || !input.has_deprecation_diagnostics
        || !input.resolved.no_emit
        || input.resolved.checker.no_lib
        || input.args.list_files
        || input.args.explain_files
        || input.args.diagnostics
        || input.args.extended_diagnostics
    {
        return None;
    }

    let parse_start = Instant::now();
    let compile_inputs: Vec<(String, String)> = input
        .sources
        .iter()
        .map(|source| {
            let text = source.text.clone().unwrap_or_default();
            (source.path.to_string_lossy().into_owned(), text)
        })
        .collect();
    let parse_results = tsz::parallel::parse_files_parallel(compile_inputs);
    let parse_bind_duration = parse_start.elapsed();
    (input.perf_log_phase)("parse_config_deprecation_no_emit", parse_start);

    let mut config_diagnostics = input.config_diagnostics.to_vec();
    config_diagnostics.extend(collect_source_reference_lib_diagnostics(
        &parse_results
            .iter()
            .map(|result| SourceEntry {
                path: PathBuf::from(&result.file_name),
                text: result
                    .arena
                    .get_source_file_at(result.source_file)
                    .map(|source_file| source_file.text.as_ref().to_string()),
                is_binary: false,
                suppress_parser_diagnostics: false,
            })
            .collect::<Vec<_>>(),
        input.resolved.checker.no_lib,
    ));

    let mut diagnostics = collect_parse_only_no_check_diagnostics(&parse_results, input.resolved);

    if !input.binary_file_names_to_suppress.is_empty() {
        diagnostics.retain(|d| !input.binary_file_names_to_suppress.contains(&d.file));
    }

    apply_fatal_config_notice_priority(&mut diagnostics, &mut config_diagnostics);

    diagnostics.extend(config_diagnostics);
    diagnostics.extend(input.binary_file_diagnostics.iter().cloned());
    diagnostics.extend(input.type_file_diagnostics.iter().cloned());
    diagnostics.sort_by(|left, right| left.compare(right));

    Some(CompilationResult {
        diagnostics,
        emitted_files: Vec::new(),
        files_read: input.user_files_read.to_vec(),
        file_infos: input.file_infos.to_vec(),
        no_emit: input.resolved.no_emit,
        request_cache_counters: tsz::checker::context::RequestCacheCounters::default(),
        interned_types_count: 0,
        interner_estimated_bytes: 0,
        query_cache_stats: None,
        def_store_stats: None,
        phase_timings: PhaseTimings {
            io_read_ms: input.io_read_duration.as_secs_f64() * 1000.0,
            parse_bind_ms: parse_bind_duration.as_secs_f64() * 1000.0,
            total_ms: input.compile_start.elapsed().as_secs_f64() * 1000.0,
            ..PhaseTimings::default()
        },
        residency_stats: None,
        module_dep_stats: None,
        invalidation_summaries: Vec::new(),
    })
}

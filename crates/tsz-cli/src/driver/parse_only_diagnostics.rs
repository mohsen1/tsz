use super::*;

/// Inputs for parse-only short-circuit paths that bypass the full checker
/// pipeline (either `--noCheck` without `--declaration`, or `--noEmit
/// --skipLibCheck` on a pure `.d.ts` project).
pub(super) struct ParseOnlyDiagnosticsInput<'a> {
    pub(super) program: &'a MergedProgram,
    pub(super) options: &'a ResolvedCompilerOptions,
    pub(super) program_has_real_syntax_errors: bool,
    pub(super) include_isolated_declaration_diagnostics: bool,
    pub(super) per_file_ts7016_diagnostics: &'a [Vec<Diagnostic>],
    pub(super) cache: Option<&'a mut CompilationCache>,
    pub(super) base_dir: &'a Path,
    pub(super) file_is_esm_map: &'a FxHashMap<String, bool>,
    pub(super) resolved_module_paths: &'a FxHashMap<(usize, String), usize>,
    pub(super) collect_compile_stats: bool,
    pub(super) request_cache_counters: RequestCacheCounters,
}

/// Collect parse-only diagnostics and return an early `CollectDiagnosticsResult`
/// for paths that skip the full checker pipeline.
///
/// Used by two short-circuit arms in `collect_diagnostics_with_source_resolutions`:
/// - `--noCheck` (without `--declaration`): collects parse + isolated-declarations
///   diagnostics and returns before any checker binder or `ProgramContext` setup.
/// - `--noEmit --skipLibCheck` on a pure `.d.ts` project: collects parse
///   diagnostics only and returns before expensive checker infrastructure.
///
/// Both arms share the same post-collection steps: extend with per-file TS7016
/// diagnostics, trim the `CompilationCache`, detect missing-tslib helpers, and
/// build the optional module-dependency stats.
pub(super) fn collect_parse_only_diagnostics_result(
    input: ParseOnlyDiagnosticsInput<'_>,
) -> CollectDiagnosticsResult {
    let ParseOnlyDiagnosticsInput {
        program,
        options,
        program_has_real_syntax_errors,
        include_isolated_declaration_diagnostics,
        per_file_ts7016_diagnostics,
        cache,
        base_dir,
        file_is_esm_map,
        resolved_module_paths,
        collect_compile_stats,
        request_cache_counters,
    } = input;

    let all_file_indices: Vec<usize> = (0..program.files.len()).collect();

    let mut diagnostics: Vec<Diagnostic> =
        collect_no_check_diagnostics_for_files(NoCheckDiagnosticsInput {
            files: &program.files,
            file_indices: &all_file_indices,
            options,
            program_has_real_syntax_errors,
            include_isolated_declaration_diagnostics,
        })
        .into_iter()
        .flat_map(|file_diags| file_diags.diagnostics)
        .collect();

    let mut used_paths =
        FxHashSet::with_capacity_and_hasher(program.files.len(), Default::default());
    for (file_idx, file_diags) in per_file_ts7016_diagnostics.iter().enumerate() {
        diagnostics.extend(file_diags.iter().cloned());
        if let Some(file) = program.files.get(file_idx) {
            used_paths.insert(PathBuf::from(&file.file_name));
        }
    }

    if let Some(c) = cache {
        c.type_caches.retain(|path, _| used_paths.contains(path));
        c.diagnostics.retain(|path, _| used_paths.contains(path));
        c.export_hashes.retain(|path, _| used_paths.contains(path));
    }

    diagnostics.extend(detect_missing_tslib_helper_diagnostics(
        program,
        options,
        base_dir,
        file_is_esm_map,
    ));

    let module_dep_stats = if collect_compile_stats {
        Some(compute_module_dependency_stats(
            program.files.len(),
            resolved_module_paths,
        ))
    } else {
        None
    };

    CollectDiagnosticsResult {
        diagnostics,
        request_cache_counters,
        query_cache_stats: Some(tsz_solver::construction::QueryCacheStatistics::default()),
        def_store_stats: None,
        module_dep_stats,
    }
}

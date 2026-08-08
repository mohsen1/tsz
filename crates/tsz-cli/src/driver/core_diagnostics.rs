//! Diagnostic/result helpers for the CLI compilation driver.

use super::*;
use tsz::checker::diagnostics::diagnostic_messages;

pub(super) const fn is_grammar_error_for_deprecation_priority(code: u32) -> bool {
    matches!(
        code,
        8002 | 8003
            | 8004
            | 8006
            | 8008
            | 8009
            | 8010
            | 8011
            | 8013
            | 8015
            | 8016
            | 8017
            | 8018
            | 8037
            | 8038
            | 8039
    ) || matches!(code, 17002 | 17006 | 17007 | 17008 | 17012)
        || matches!(
            code,
            1002 | 1003
                | 1005
                | 1011
                | 1034
                | 1109
                | 1110
                | 1121
                | 1124
                | 1125
                | 1126
                | 1127
                | 1128
                | 1131
                | 1134
                | 1137
                | 1144
                | 1145
                | 1198
                | 1199
                | 1389
                | 1433
                | 1434
                | 1436
                | 1440
                | 1442
                | 1489
        )
        || matches!(code, 2458 | 2754)
}

/// Drops fatal config notices that a grammar error outranks.
///
/// The deprecation branch is retained for legacy TS5101/TS5107 producers;
/// TypeScript 7 option parsing reaches this path through removal notices.
pub(super) fn remove_deprecation_diagnostics(diagnostics: &mut Vec<Diagnostic>) {
    diagnostics.retain(|d| {
        !is_deprecation_diagnostic_code(d.code) && !is_removed_option_diagnostic_code(d.code)
    });
}

/// Applies tsc's precedence between a fatal config-level notice and the
/// file-level diagnostics.
///
/// A fatal legacy deprecation or TypeScript 7 removal notice only survives when
/// the program is otherwise free of grammar errors. When a grammar error is
/// present it takes precedence and the config notice is dropped; tsc's
/// `getOptionsDiagnostics`/`getGlobalDiagnostics` never reach the reporter once
/// `getSyntacticDiagnostics` produces an error. Absent a grammar error the notice
/// is fatal and file-level semantic diagnostics are suppressed, with only the
/// global TS2318 ("Cannot find global type") and TS2792 ("Did you mean to set
/// moduleResolution?") diagnostics preserved.
///
/// This preserves the legacy deprecation precedence while sharing it with
/// removed-option handling: a removed-but-parsed option passed on the CLI follows
/// the same "config notice is fatal for semantic diagnostics" rule that tsc
/// applies, so a real type error must not leak alongside the removal notice.
pub(super) fn apply_fatal_config_notice_priority(
    diagnostics: &mut Vec<Diagnostic>,
    config_diagnostics: &mut Vec<Diagnostic>,
) {
    let has_grammar_errors = diagnostics
        .iter()
        .any(|d| is_grammar_error_for_deprecation_priority(d.code));

    if has_grammar_errors {
        // Grammar errors take precedence — drop both the deprecation and the
        // removed-option notices (tsc reports only the grammar error).
        remove_deprecation_diagnostics(config_diagnostics);
    } else {
        // The config notice is fatal — suppress file-level diagnostics,
        // preserving only the global TS2318/TS2792 diagnostics tsc still emits.
        diagnostics
            .retain(|d| (d.code == 2318 && d.file.is_empty() && d.start == 0) || d.code == 2792);
    }
}

/// Unknown compiler-option diagnostics are option-parsing failures in tsc.
/// They suppress source diagnostics but do not imply `noEmitOnError`, so the
/// emitter may still run when the user has not requested that policy.
fn apply_unknown_compiler_option_priority(diagnostics: &mut Vec<Diagnostic>) {
    diagnostics.clear();
}

pub(super) fn collect_parse_only_no_check_diagnostics(
    parse_results: &[parallel::ParseResult],
    options: &ResolvedCompilerOptions,
) -> Vec<Diagnostic> {
    let program_has_real_syntax_errors = parse_results
        .iter()
        .flat_map(|result| result.parse_diagnostics.iter())
        .any(|diag| check_utils::is_real_syntax_error(diag.code));

    #[cfg(not(target_arch = "wasm32"))]
    {
        use rayon::prelude::*;
        tsz::parallel::ensure_rayon_global_pool();
        parse_results
            .par_iter()
            .flat_map(|result| {
                check_utils::collect_no_check_parse_diagnostics_for_file(
                    &result.file_name,
                    &result.arena,
                    result.source_file,
                    &result.parse_diagnostics,
                    options,
                    program_has_real_syntax_errors,
                )
            })
            .collect()
    }

    #[cfg(target_arch = "wasm32")]
    {
        parse_results
            .iter()
            .flat_map(|result| {
                check_utils::collect_no_check_parse_diagnostics_for_file(
                    &result.file_name,
                    &result.arena,
                    result.source_file,
                    &result.parse_diagnostics,
                    options,
                    program_has_real_syntax_errors,
                )
            })
            .collect()
    }
}

pub(super) fn no_lib_core_global_type_diagnostics() -> Vec<Diagnostic> {
    [
        "Array",
        "Boolean",
        "Function",
        "IArguments",
        "Number",
        "Object",
        "RegExp",
        "String",
        "CallableFunction",
        "NewableFunction",
    ]
    .into_iter()
    .map(|name| {
        Diagnostic::error(
            String::new(),
            0,
            0,
            format!("Cannot find global type '{name}'."),
            diagnostic_codes::CANNOT_FIND_GLOBAL_TYPE,
        )
    })
    .collect()
}

pub(super) fn compile_inner(
    args: &CliArgs,
    cwd: &Path,
    cache: Option<&mut CompilationCache>,
    changed_paths: Option<&[PathBuf]>,
    forced_dirty_paths: Option<&FxHashSet<PathBuf>>,
    explicit_config_path: Option<&Path>,
) -> Result<CompilationResult> {
    let direct_cli_parse_diagnostics = ordered_direct_cli_parse_diagnostics(args)?;
    let mut result = match compile_inner_impl(
        args,
        cwd,
        cache,
        changed_paths,
        forced_dirty_paths,
        explicit_config_path,
    ) {
        Ok(result) => result,
        Err(_) if !direct_cli_parse_diagnostics.is_empty() => {
            config_error_result(None, String::new(), 0)
        }
        Err(error) => return Err(error),
    };
    if !direct_cli_parse_diagnostics.is_empty() {
        // Command-line parsing precedes every project/config/source phase in
        // tsc. Preserve any emitted output, but make the direct parse
        // diagnostics the final reported set on every return path.
        result.diagnostics = direct_cli_parse_diagnostics;
    }
    Ok(result)
}

fn compile_inner_impl(
    args: &CliArgs,
    cwd: &Path,
    mut cache: Option<&mut CompilationCache>,
    changed_paths: Option<&[PathBuf]>,
    forced_dirty_paths: Option<&FxHashSet<PathBuf>>,
    explicit_config_path: Option<&Path>,
) -> Result<CompilationResult> {
    let _compile_span = tracing::info_span!("compile", cwd = %cwd.display()).entered();
    let perf_enabled = std::env::var_os("TSZ_PERF").is_some();
    let compile_start = Instant::now();

    let perf_log_phase = |phase: &'static str, start: Instant| {
        if perf_enabled {
            tracing::info!(
                target: "wasm::perf",
                phase,
                ms = start.elapsed().as_secs_f64() * 1000.0
            );
        }
    };

    let cwd = normalize_path(cwd);
    let ignored_config_path_for_no_input = if args.ignore_config && args.files.is_empty() {
        resolve_tsconfig_path(&cwd, args.project.as_deref())
            .ok()
            .flatten()
    } else {
        None
    };
    let tsconfig_path = if args.ignore_config {
        // --ignoreConfig: skip tsconfig.json discovery and loading entirely
        None
    } else if let Some(path) = explicit_config_path {
        Some(path.to_path_buf())
    } else {
        match resolve_tsconfig_path(&cwd, args.project.as_deref()) {
            Ok(path) => path,
            Err(err) => {
                let code = match err {
                    ResolveTsconfigError::NoConfigInDirectory(_) => {
                        diagnostic_codes::CANNOT_FIND_A_TSCONFIG_JSON_FILE_AT_THE_SPECIFIED_DIRECTORY
                    }
                    ResolveTsconfigError::PathDoesNotExist(_)
                    | ResolveTsconfigError::NotAFile(_) => {
                        diagnostic_codes::THE_SPECIFIED_PATH_DOES_NOT_EXIST
                    }
                };
                return Ok(config_error_result(None, err.to_string(), code));
            }
        }
    };
    let loaded = load_config_with_diagnostics(tsconfig_path.as_deref())?;
    let config = loaded.config;
    let mut config_diagnostics = loaded.diagnostics;
    // tsc merges explicit CLI options over the config chain BEFORE the
    // removed-option check, so a chain-effective removed VALUE that the CLI
    // overrides with a valid value produces no diagnostic at all
    // (`tsc -p . --moduleResolution bundler` over node10 compiles clean).
    // Removed KEYS are not retractable: passing the key on the CLI is itself
    // a removal error.
    let cli_override_keys = cli_valid_override_keys(args, config.as_ref())?;
    config_diagnostics.extend(
        loaded
            .pending_removed_option_notices
            .into_iter()
            .filter(|notice| !(notice.is_value && cli_override_keys.contains(&notice.key)))
            .map(|notice| notice.diagnostic),
    );
    // A references-only root tsconfig (no .ts inputs, but non-empty `references[]`) is
    // the canonical TypeScript Project References pattern. tsc never emits TS18003 in
    // this case; suppress the "no inputs" diagnostic when `references[]` is non-empty.
    let has_project_references = config
        .as_ref()
        .and_then(|c| c.references.as_deref())
        .is_some_and(|refs| !refs.is_empty());
    if cli_ignore_deprecations_silences_6_0(args) {
        config_diagnostics.retain(|d| !is_deprecation_diagnostic_code(d.code));
    }
    let config_has_removed_option_diagnostic = config_diagnostics
        .iter()
        .any(|d| is_removed_option_diagnostic_code(d.code));
    let cli_option_diagnostics = validate_cli_compiler_option_diagnostics(args, config.as_ref())?;
    let cli_parse_diagnostics = ordered_direct_cli_parse_diagnostics(args)?;
    let has_unknown_cli_compiler_option_diagnostic = cli_parse_diagnostics.iter().any(|d| {
        matches!(
            d.code,
            diagnostic_codes::UNKNOWN_COMPILER_OPTION
                | diagnostic_codes::UNKNOWN_COMPILER_OPTION_DID_YOU_MEAN
        )
    });
    let has_fatal_cli_enum_diagnostic = cli_parse_diagnostics
        .iter()
        .any(|d| d.code == diagnostic_codes::ARGUMENT_FOR_OPTION_MUST_BE);
    if cli_parse_diagnostics.is_empty() {
        config_diagnostics.extend(cli_option_diagnostics);
    } else {
        // Command-line parsing happens before project/config validation in tsc.
        // Once it finds an unknown option or invalid enum, only those direct
        // parse diagnostics survive; loaded-config and removal diagnostics do not.
        config_diagnostics = cli_parse_diagnostics;
    }
    if args.source_map || args.declaration_map {
        config_diagnostics.retain(|d| {
            !(d.code
                == diagnostic_codes::OPTION_CANNOT_BE_SPECIFIED_WITHOUT_SPECIFYING_OPTION_OR_OPTION
                && d.message_text.contains("mapRoot")
                && d.message_text.contains("sourceMap")
                && d.message_text.contains("declarationMap"))
        });
    }

    let has_removed_option_value_diagnostic = config_diagnostics
        .iter()
        .any(|d| is_removed_option_value_diagnostic_code(d.code));

    // Direct CLI TS6046 and TS5108 (removed option value) stop before source
    // checking. Config-file TS6046 remains an option diagnostic that can
    // coexist with source diagnostics. TS5102 (removed option) is fatal when it
    // comes from configuration. Direct CLI TS5102 remains reportable without
    // forcing no-emit so emit baselines can still compare output for parsed
    // legacy flags. TS5103 is deliberately absent: TypeScript 7 performs no
    // `ignoreDeprecations` value validation, so tsz has no emission site for it
    // to classify (#16228).
    let has_fatal_config_diagnostic = config_diagnostics.iter().any(|d| {
        d.code == diagnostic_codes::INVALID_VALUE_FOR_REACTNAMESPACE_IS_NOT_A_VALID_IDENTIFIER
            || (config_has_removed_option_diagnostic && is_removed_option_diagnostic_code(d.code))
    });
    if has_fatal_cli_enum_diagnostic
        || has_removed_option_value_diagnostic
        || has_fatal_config_diagnostic
    {
        return Ok(CompilationResult {
            diagnostics: config_diagnostics,
            emitted_files: Vec::new(),
            files_read: Vec::new(),
            file_infos: Vec::new(),
            no_emit: args.no_emit,
            request_cache_counters: tsz::checker::context::RequestCacheCounters::default(),
            interned_types_count: 0,
            interner_estimated_bytes: 0,
            query_cache_stats: None,
            def_store_stats: None,
            phase_timings: PhaseTimings::default(),
            residency_stats: None,
            module_dep_stats: None,
            invalidation_summaries: Vec::new(),
        });
    }

    // Retain precedence handling for diagnostics from legacy deprecation producers.
    let has_deprecation_diagnostics = config_diagnostics
        .iter()
        .any(|d| is_deprecation_diagnostic_code(d.code));
    // A removed-option notice (TS5102) sourced from the CLI reaches the full
    // pipeline (config-sourced removals already returned early above). tsc treats
    // it as fatal for semantic diagnostics exactly like a deprecation notice, so
    // it must drive the same file-level suppression below.
    let has_removed_option_diagnostic = config_diagnostics
        .iter()
        .any(|d| is_removed_option_diagnostic_code(d.code));
    let has_fatal_config_notice = has_deprecation_diagnostics || has_removed_option_diagnostic;

    let mut resolved = match resolve_compiler_options(
        config
            .as_ref()
            .and_then(|cfg| cfg.compiler_options.as_ref()),
    ) {
        Ok(r) => r,
        Err(e) => {
            // If config has errors (e.g., TS5024 for a type-invalid option
            // value), return them even if compiler options resolution fails.
            // This ensures any existing config diagnostics are reported to the user.
            if !config_diagnostics.is_empty() {
                return Ok(CompilationResult {
                    diagnostics: config_diagnostics,
                    emitted_files: Vec::new(),
                    files_read: Vec::new(),
                    file_infos: Vec::new(),
                    no_emit: args.no_emit,
                    request_cache_counters: tsz::checker::context::RequestCacheCounters::default(),
                    interned_types_count: 0,
                    interner_estimated_bytes: 0,
                    query_cache_stats: None,
                    def_store_stats: None,
                    phase_timings: PhaseTimings::default(),
                    residency_stats: None,
                    module_dep_stats: None,
                    invalidation_summaries: Vec::new(),
                });
            }
            return Err(e);
        }
    };
    apply_cli_overrides_with_config_options(
        &mut resolved,
        args,
        config
            .as_ref()
            .and_then(|cfg| cfg.compiler_options.as_ref()),
    )?;
    let positional_no_config_no_emit =
        tsconfig_path.is_none() && !args.files.is_empty() && resolved.no_emit;
    if resolved.allow_importing_ts_extensions {
        resolved.checker.allow_importing_ts_extensions = true;
    }
    if resolved.rewrite_relative_import_extensions {
        resolved.checker.rewrite_relative_import_extensions = true;
        resolved.printer.rewrite_relative_import_extensions = true;
    }
    let base_dir = config_base_dir(&cwd, tsconfig_path.as_deref());
    let base_dir = if resolved.preserve_symlinks {
        normalize_path(&base_dir)
    } else {
        canonicalize_or_owned(&base_dir)
    };
    let root_dir_display = resolved.root_dir.clone();
    let root_dir = normalize_root_dir(&base_dir, resolved.root_dir.clone());
    let out_dir = normalize_output_dir(&base_dir, resolved.out_dir.clone());
    let declaration_dir = normalize_output_dir(&base_dir, resolved.declaration_dir.clone());
    let base_url = normalize_base_url(&base_dir, resolved.base_url.clone());
    let root_dirs = normalize_root_dirs(&base_dir, resolved.root_dirs.clone());
    resolved.root_dir = root_dir.clone();
    resolved.out_dir = out_dir.clone();
    resolved.declaration_dir = declaration_dir.clone();
    resolved.base_url = base_url;
    // tsc's `pathsBasePath`: the tsconfig directory anchors `paths`
    // substitutions when `baseUrl` is unset (TypeScript 4.1+). The resolver
    // prefers `base_url` and only falls back to this, so setting it
    // unconditionally is a no-op when `baseUrl` is present.
    resolved.paths_base_path = Some(base_dir.clone());
    resolved.root_dirs = root_dirs;
    resolved.type_roots = normalize_type_roots(&base_dir, resolved.type_roots.clone());

    if args.ignore_config
        && args.files.is_empty()
        && let Some(config_path) = ignored_config_path_for_no_input.as_deref()
    {
        let diagnostics = no_input_diagnostics_for_config(
            config_diagnostics,
            Some(config_path),
            None,
            None,
            resolved.allow_js,
        );
        return Ok(CompilationResult {
            diagnostics,
            emitted_files: Vec::new(),
            files_read: Vec::new(),
            file_infos: Vec::new(),
            no_emit: resolved.no_emit,
            request_cache_counters: tsz::checker::context::RequestCacheCounters::default(),
            interned_types_count: 0,
            interner_estimated_bytes: 0,
            query_cache_stats: None,
            def_store_stats: None,
            phase_timings: PhaseTimings::default(),
            residency_stats: None,
            module_dep_stats: None,
            invalidation_summaries: Vec::new(),
        });
    }

    let discovery = build_discovery_options(
        args,
        &base_dir,
        tsconfig_path.as_deref(),
        config.as_ref(),
        out_dir.as_deref(),
        &resolved,
    )?;
    let mut file_paths = discover_ts_files(&discovery)?;
    config_diagnostics.extend(unsupported_explicit_file_diagnostics(&discovery));

    // If config validation already emitted TS5110 (module/moduleResolution mismatch),
    // or TS5090 (`paths` substitutions require `baseUrl`), bail out early.
    // These are config-stage failures in the cache-backed conformance pipeline, so we
    // must not continue into file/module checking and add follow-on diagnostics.
    // tsc still emits TS18003 alongside TS5110 when no input files are found,
    // so we must check file discovery before bailing.
    if config_diagnostics.iter().any(|d| {
        d.code
            == diagnostic_codes::OPTION_MODULE_MUST_BE_SET_TO_WHEN_OPTION_MODULERESOLUTION_IS_SET_TO
            || d.code
                == diagnostic_codes::NON_RELATIVE_PATHS_ARE_NOT_ALLOWED_WHEN_BASEURL_IS_NOT_SET_DID_YOU_FORGET_A_LEAD
    }) {
        let diagnostics = if file_paths.is_empty()
            && !discovery.files_explicitly_set
            && !has_project_references
        {
            no_input_diagnostics_for_config(
                config_diagnostics,
                tsconfig_path.as_deref(),
                discovery.include.as_deref(),
                discovery.exclude.as_deref(),
                discovery.allow_js,
            )
        } else {
            config_diagnostics
        };
        return Ok(CompilationResult {
            diagnostics,
            emitted_files: Vec::new(),
            files_read: Vec::new(),
            file_infos: Vec::new(),
            no_emit: resolved.no_emit,
            request_cache_counters: tsz::checker::context::RequestCacheCounters::default(),
            interned_types_count: 0,
            interner_estimated_bytes: 0,
            query_cache_stats: None,
            def_store_stats: None,
            phase_timings: PhaseTimings::default(),
            residency_stats: None,
            module_dep_stats: None,
            invalidation_summaries: Vec::new(),
        });
    }

    // Track if we should save BuildInfo after successful compilation
    let mut should_save_build_info = false;

    // Local cache for BuildInfo-loaded compilation state
    // Only create when loading from BuildInfo (not when a cache is provided)
    let mut local_cache: Option<CompilationCache> = None;

    // `latestChangedDtsFile` from the previously saved BuildInfo. tsc seeds
    // its builder state with the old program's value and only reassigns it
    // when a declaration file is actually written, so an incremental save
    // that emits no declaration output must preserve the prior value rather
    // than clear it.
    let mut prior_latest_changed_dts_file: Option<String> = None;

    // Load BuildInfo only when incremental compilation is enabled and no cache was provided.
    // A standalone `tsBuildInfoFile` path does not activate build info reads/writes.
    if cache.is_none() && resolved.incremental {
        let tsconfig_path_ref = tsconfig_path.as_deref();
        if let Some(build_info_path) = get_build_info_path(tsconfig_path_ref, &resolved, &base_dir)
        {
            if build_info_path.exists() {
                match BuildInfo::load(&build_info_path) {
                    Ok(Some(build_info)) => {
                        // Create a local cache from BuildInfo
                        local_cache = Some(build_info_to_compilation_cache(&build_info, &base_dir));
                        prior_latest_changed_dts_file = build_info.latest_changed_dts_file;
                        tracing::info!("Loaded BuildInfo from: {}", build_info_path.display());
                    }
                    Ok(None) => {
                        tracing::info!(
                            "BuildInfo at {} is outdated or incompatible, starting fresh",
                            build_info_path.display()
                        );
                    }
                    Err(e) => {
                        tracing::warn!(
                            "Failed to load BuildInfo from {}: {}, starting fresh",
                            build_info_path.display(),
                            e
                        );
                    }
                }
            } else {
                // BuildInfo doesn't exist yet, create empty local cache for new compilation
                local_cache = Some(CompilationCache::default());
            }
            should_save_build_info = true;
        }
    }

    // Determine which cache to use: local cache from BuildInfo, or provided cache, or none
    // When cache is None, we can use local_cache; otherwise we use the provided cache
    if file_paths.is_empty() {
        // When `files` is explicitly set (e.g., `"files": []` in a solution-style
        // tsconfig) or when `references[]` is non-empty (project-references root),
        // tsc does NOT emit TS18003. The error only applies when discovery found
        // nothing due to include/exclude patterns with no project-references fallback.
        let diagnostics = if discovery.files_explicitly_set || has_project_references {
            config_diagnostics
        } else {
            no_input_diagnostics_for_config(
                config_diagnostics,
                tsconfig_path.as_deref(),
                discovery.include.as_deref(),
                discovery.exclude.as_deref(),
                discovery.allow_js,
            )
        };
        return Ok(CompilationResult {
            diagnostics,
            emitted_files: Vec::new(),
            files_read: Vec::new(),
            file_infos: Vec::new(),
            no_emit: resolved.no_emit,
            request_cache_counters: tsz::checker::context::RequestCacheCounters::default(),
            interned_types_count: 0,
            interner_estimated_bytes: 0,
            query_cache_stats: None,
            def_store_stats: None,
            phase_timings: PhaseTimings::default(),
            residency_stats: None,
            module_dep_stats: None,
            invalidation_summaries: Vec::new(),
        });
    }

    let mut root_dir_diagnostic_roots: FxHashSet<PathBuf> = FxHashSet::default();
    if let Some(ref root_dir_path) = root_dir {
        let canonical_root = canonicalize_or_owned(root_dir_path);
        for file_path in &file_paths {
            if is_declaration_file(file_path) {
                continue;
            }
            let canonical_file = canonicalize_or_owned(file_path);
            if !canonical_file.starts_with(&canonical_root) {
                root_dir_diagnostic_roots.insert(canonical_file);
            }
        }
    }

    let root_file_paths = file_paths.clone();

    // TS1149: two root files whose real on-disk paths are identical except
    // for casing (e.g. `foo.ts` and `Foo.ts` both specified as roots). tsc
    // reports this unconditionally — independent of `useCaseSensitiveFileNames`
    // — as a portability warning, with a two-line "file is in the program
    // because: / Root file specified for compilation / Root file specified
    // for compilation" chain and no source location, since neither
    // colliding path is reached via an import specifier to anchor on.
    // Import-discovered casing collisions (`tsc`'s `Imported via "..." from
    // file '...'` chain link) are a separate, unclaimed slice: they need the
    // module-resolution loop's specifier/importer attribution, not just the
    // root file list.
    {
        let mut seen_by_lowercase: FxHashMap<String, PathBuf> = FxHashMap::default();
        for path in &root_file_paths {
            let canonical = canonicalize_or_owned(path);
            let lowercase_key = canonical.to_string_lossy().to_ascii_lowercase();
            match seen_by_lowercase.get(&lowercase_key) {
                Some(existing) if existing != &canonical => {
                    let new_display = canonical.to_string_lossy().into_owned();
                    let existing_display = existing.to_string_lossy().into_owned();
                    let mut diagnostic = Diagnostic::from_code(
                        diagnostic_codes::FILE_NAME_DIFFERS_FROM_ALREADY_INCLUDED_FILE_NAME_ONLY_IN_CASING,
                        String::new(),
                        0,
                        0,
                        &[&new_display, &existing_display],
                    );
                    diagnostic
                        .related_information
                        .push(DiagnosticRelatedInformation {
                            category: DiagnosticCategory::Message,
                            code: diagnostic_codes::THE_FILE_IS_IN_THE_PROGRAM_BECAUSE,
                            file: String::new(),
                            start: 0,
                            length: 0,
                            message_text: "The file is in the program because:".to_string(),
                            depth: 0,
                            kind: RelatedInformationKind::ChainLink,
                        });
                    for _ in 0..2 {
                        diagnostic
                            .related_information
                            .push(DiagnosticRelatedInformation {
                                category: DiagnosticCategory::Message,
                                code: diagnostic_codes::ROOT_FILE_SPECIFIED_FOR_COMPILATION,
                                file: String::new(),
                                start: 0,
                                length: 0,
                                message_text: "Root file specified for compilation".to_string(),
                                depth: 1,
                                kind: RelatedInformationKind::ChainLink,
                            });
                    }
                    config_diagnostics.push(diagnostic);
                }
                Some(_) => {}
                None => {
                    seen_by_lowercase.insert(lowercase_key, canonical);
                }
            }
        }
    }

    // `@noTypesAndSymbols` is a TypeScript test-corpus pragma, not a real
    // compiler directive. Honor only the explicit value coming from
    // tsconfig/CLI; never let an ordinary source comment override the
    // project's type-root resolution. See issue #3014.

    let (type_files, unresolved_types) = collect_type_root_files(&base_dir, &resolved);

    // Add type definition files (e.g., @types packages) to the source file list.
    // Note: lib.d.ts files are NOT added here - they are loaded separately via
    // lib preloading + checker lib contexts. This prevents them from
    // being type-checked as regular source files (which would emit spurious errors).
    if !type_files.is_empty() {
        let mut merged = std::collections::BTreeSet::new();
        merged.extend(file_paths);
        merged.extend(type_files);
        file_paths = merged.into_iter().collect();
    }

    let changed_set = changed_paths.map(|paths| {
        paths
            .iter()
            .map(|path| canonicalize_or_owned(path))
            .collect::<FxHashSet<_>>()
    });

    // Create a unified effective cache reference that works for both cases
    // This follows Gemini's recommended pattern to handle the two cache sources
    let local_cache_ref = local_cache.as_mut();
    let mut effective_cache = local_cache_ref.or(cache.as_deref_mut());

    let read_sources_start = Instant::now();
    let SourceReadResult {
        sources: all_sources,
        dependencies,
        outfile_bundle_paths,
        outfile_bundle_dependencies,
        module_resolutions,
        module_resolution_misses,
        type_reference_errors,
        resolution_mode_errors,
        depth_skipped_js,
    } = {
        read_source_files(
            &file_paths,
            &base_dir,
            &resolved,
            effective_cache.as_deref(),
            changed_set.as_ref(),
        )?
    };
    let io_read_duration = read_sources_start.elapsed();
    perf_log_phase("read_sources", read_sources_start);

    if let Some(ref root_dir_path) = root_dir {
        let root_display_path = root_dir_display.as_ref().unwrap_or(root_dir_path);
        let blame_files: FxHashSet<PathBuf> = root_dir_diagnostic_roots;
        let mut blame_files: Vec<_> = blame_files.into_iter().collect();
        blame_files.sort();
        for file_path in blame_files {
            let file_display = file_path.to_string_lossy();
            let root_display = root_display_path.to_string_lossy();
            let message = format!(
                "File '{file_display}' is not under 'rootDir' '{root_display}'. 'rootDir' is expected to contain all source files."
            );
            config_diagnostics.push(Diagnostic::error(
                String::new(),
                0,
                0,
                message,
                diagnostic_codes::FILE_IS_NOT_UNDER_ROOTDIR_ROOTDIR_IS_EXPECTED_TO_CONTAIN_ALL_SOURCE_FILES,
            ));
        }
    } else if !resolved.no_emit
        && (out_dir.is_some() || declaration_dir.is_some() || resolved.out_file.is_some())
        && let Some(tsconfig) = tsconfig_path.as_deref()
    {
        // TS5011: an output location (`outDir`, `declarationDir`, or `outFile`)
        // is configured without an explicit `rootDir`, and the inferred common
        // source directory differs from the tsconfig directory. This migration
        // diagnostic covers every emit — JavaScript, declaration, and `outFile`
        // bundle — because the output would otherwise land in a layout that
        // changes in TypeScript 7, so it asks the user to pin `rootDir`. It is
        // suppressed under `noEmit` (nothing is written) and when `rootDir` is
        // set (handled by the branch above), matching tsc.
        if let Some(common) = implicit_common_source_directory(&root_file_paths, &base_dir, &cwd) {
            let tsconfig_display = display_relative_to_dir(tsconfig, &cwd);
            let common_display = display_relative_to_dir(&common, &base_dir);
            let message = format!(
                "The common source directory of '{tsconfig_display}' is '{common_display}'. The 'rootDir' setting must be explicitly set to this or another path to adjust your output's file layout.\n  {url}",
                url = diagnostic_messages::VISIT_HTTPS_AKA_MS_TS6_FOR_MIGRATION_INFORMATION
            );
            config_diagnostics.push(Diagnostic::error(
                tsconfig.to_string_lossy().into_owned(),
                0,
                0,
                message,
                diagnostic_codes::THE_COMMON_SOURCE_DIRECTORY_OF_IS_THE_ROOTDIR_SETTING_MUST_BE_EXPLICITLY_SET_TO,
            ));
        }
    }

    // Update dependencies in the cache
    if let Some(ref mut c) = effective_cache {
        c.update_dependencies(dependencies, outfile_bundle_dependencies.clone());
    }

    // Separate binary files from regular sources - binary files get TS1490
    let mut type_file_diagnostics: Vec<Diagnostic> = Vec::new();
    for (path, type_name, types_offset, types_len) in type_reference_errors {
        let file_name = path.to_string_lossy().into_owned();
        type_file_diagnostics.push(Diagnostic::error(
            file_name,
            types_offset as u32,
            types_len as u32,
            format!("Cannot find type definition file for '{type_name}'."),
            diagnostic_codes::CANNOT_FIND_TYPE_DEFINITION_FILE_FOR,
        ));
    }
    // TS1453: Invalid resolution-mode values in triple-slash directives
    for (path, start, length) in resolution_mode_errors {
        let file_name = path.to_string_lossy().into_owned();
        type_file_diagnostics.push(Diagnostic::error(
            file_name,
            start as u32,
            length as u32,
            "`resolution-mode` should be either `require` or `import`.".to_string(),
            diagnostic_codes::RESOLUTION_MODE_SHOULD_BE_EITHER_REQUIRE_OR_IMPORT,
        ));
    }
    // Emit TS2688 for unresolved entries in tsconfig `types` array
    for type_name in &unresolved_types {
        type_file_diagnostics.push(Diagnostic::error(
            String::new(),
            0,
            0,
            format!("Cannot find type definition file for '{type_name}'."),
            diagnostic_codes::CANNOT_FIND_TYPE_DEFINITION_FILE_FOR,
        ));
    }

    let mut binary_file_diagnostics: Vec<Diagnostic> = Vec::new();
    let mut binary_file_names_to_suppress: FxHashSet<String> = FxHashSet::default();
    let mut sources: Vec<SourceEntry> = Vec::with_capacity(all_sources.len());
    for source in all_sources {
        if source.is_binary {
            // Emit TS1490 "File appears to be binary." for binary files.
            let file_name = source.path.to_string_lossy().into_owned();
            if source.suppress_parser_diagnostics {
                // Hard-binary cases like invalid UTF-8 or null-byte corruption should
                // surface only TS1490, matching tsc's early binary bailout.
                binary_file_names_to_suppress.insert(file_name.clone());
            }
            binary_file_diagnostics.push(Diagnostic::error(
                file_name,
                0,
                0,
                "File appears to be binary.".to_string(),
                diagnostic_codes::FILE_APPEARS_TO_BE_BINARY,
            ));
        }
        sources.push(source);
    }

    // Collect user source files that were read before sources is moved
    let mut user_files_read: Vec<PathBuf> = sources.iter().map(|s| s.path.clone()).collect();
    user_files_read.sort();

    // Build file info with inclusion reasons
    let file_infos = build_file_infos(
        &sources,
        &file_paths,
        args,
        config.as_ref(),
        &base_dir,
        resolved.printer.target,
    );

    if let Some(result) =
        config_deprecation::maybe_compile_no_emit_deprecation(NoEmitDeprecationInput {
            args,
            resolved: &resolved,
            has_deprecation_diagnostics,
            sources: &sources,
            config_diagnostics: &config_diagnostics,
            binary_file_diagnostics: &binary_file_diagnostics,
            binary_file_names_to_suppress: &binary_file_names_to_suppress,
            type_file_diagnostics: &type_file_diagnostics,
            user_files_read: &user_files_read,
            file_infos: &file_infos,
            io_read_duration,
            compile_start,
            perf_log_phase: &perf_log_phase,
        })
    {
        return Ok(result);
    }

    if resolved.no_check && resolved.no_emit && !resolved.emit_declarations {
        let parse_start = Instant::now();
        let compile_inputs: Vec<(String, String)> = sources
            .into_iter()
            .map(|source| {
                let text = source.text.unwrap_or_default();
                (source.path.to_string_lossy().into_owned(), text)
            })
            .collect();
        let parse_results = parallel::parse_files_parallel(compile_inputs);
        let parse_bind_duration = parse_start.elapsed();
        perf_log_phase("parse_no_check", parse_start);

        let mut diagnostics = collect_parse_only_no_check_diagnostics(&parse_results, &resolved);
        if positional_no_config_no_emit && resolved.checker.no_lib {
            diagnostics.extend(no_lib_core_global_type_diagnostics());
        }

        if !binary_file_names_to_suppress.is_empty() {
            diagnostics.retain(|d| !binary_file_names_to_suppress.contains(&d.file));
        }

        if has_unknown_cli_compiler_option_diagnostic {
            apply_unknown_compiler_option_priority(&mut diagnostics);
        } else if has_fatal_config_notice {
            apply_fatal_config_notice_priority(&mut diagnostics, &mut config_diagnostics);
        }

        diagnostics.extend(config_diagnostics);
        diagnostics.extend(binary_file_diagnostics);
        diagnostics.extend(type_file_diagnostics);
        diagnostics.sort_by(|left, right| left.compare(right));

        return Ok(CompilationResult {
            diagnostics,
            emitted_files: Vec::new(),
            files_read: user_files_read,
            file_infos,
            no_emit: resolved.no_emit,
            request_cache_counters: tsz::checker::context::RequestCacheCounters::default(),
            interned_types_count: 0,
            interner_estimated_bytes: 0,
            query_cache_stats: None,
            def_store_stats: None,
            phase_timings: PhaseTimings {
                io_read_ms: io_read_duration.as_secs_f64() * 1000.0,
                parse_bind_ms: parse_bind_duration.as_secs_f64() * 1000.0,
                total_ms: compile_start.elapsed().as_secs_f64() * 1000.0,
                ..PhaseTimings::default()
            },
            residency_stats: None,
            module_dep_stats: None,
            invalidation_summaries: Vec::new(),
        });
    }

    // `skipLibCheck` suppresses semantic diagnostics for declaration files.
    // For a pure no-emit declaration-file project, there is no source file that
    // can consume lib or declaration symbols, and tsc also suppresses missing
    // imports from the skipped `.d.ts` files. Avoid loading default libs and
    // avoid binding the declaration files; parse diagnostics, config
    // diagnostics, type-reference diagnostics, and binary-file diagnostics are
    // still reported. Keep list/explain/diagnostics modes on the full path so
    // their file lists and counts continue to include libs.
    if resolved.no_emit
        && resolved.skip_lib_check
        && !resolved.emit_declarations
        && !args.list_files
        && !args.explain_files
        && !args.diagnostics
        && !args.extended_diagnostics
        && sources
            .iter()
            .all(|source| is_declaration_file(&source.path))
    {
        let parse_start = Instant::now();
        let compile_inputs: Vec<(String, String)> = sources
            .into_iter()
            .map(|source| {
                let text = source.text.unwrap_or_default();
                (source.path.to_string_lossy().into_owned(), text)
            })
            .collect();
        let parse_results = parallel::parse_files_parallel(compile_inputs);
        let parse_bind_duration = parse_start.elapsed();
        perf_log_phase("parse_skip_lib_check_declarations", parse_start);

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
            resolved.checker.no_lib,
        ));

        let mut diagnostics = collect_parse_only_no_check_diagnostics(&parse_results, &resolved);

        if !binary_file_names_to_suppress.is_empty() {
            diagnostics.retain(|d| !binary_file_names_to_suppress.contains(&d.file));
        }

        if has_unknown_cli_compiler_option_diagnostic {
            apply_unknown_compiler_option_priority(&mut diagnostics);
        } else if has_fatal_config_notice {
            apply_fatal_config_notice_priority(&mut diagnostics, &mut config_diagnostics);
        }

        diagnostics.extend(config_diagnostics);
        diagnostics.extend(binary_file_diagnostics);
        diagnostics.extend(type_file_diagnostics);
        diagnostics.sort_by(|left, right| left.compare(right));

        return Ok(CompilationResult {
            diagnostics,
            emitted_files: Vec::new(),
            files_read: user_files_read,
            file_infos,
            no_emit: resolved.no_emit,
            request_cache_counters: tsz::checker::context::RequestCacheCounters::default(),
            interned_types_count: 0,
            interner_estimated_bytes: 0,
            query_cache_stats: None,
            def_store_stats: None,
            phase_timings: PhaseTimings {
                io_read_ms: io_read_duration.as_secs_f64() * 1000.0,
                parse_bind_ms: parse_bind_duration.as_secs_f64() * 1000.0,
                total_ms: compile_start.elapsed().as_secs_f64() * 1000.0,
                ..PhaseTimings::default()
            },
            residency_stats: None,
            module_dep_stats: None,
            invalidation_summaries: Vec::new(),
        });
    }

    let disable_default_libs = resolved.lib_is_default && sources_have_no_default_lib(&sources);
    // `@noTypesAndSymbols` source comments must not override the project's
    // resolved compiler options here either. The tsc `/// <reference no-default-lib />`
    // directive above is a real TypeScript directive and stays.
    let lib_paths =
        resolve_effective_lib_paths(&resolved, &sources, &base_dir, disable_default_libs)?;
    config_diagnostics.extend(collect_source_reference_lib_diagnostics(
        &sources,
        resolved.checker.no_lib,
    ));
    let typescript_dom_replacement_globals = scan_typescript_dom_replacement_globals(&lib_paths);
    let lib_path_refs: Vec<&Path> = lib_paths.iter().map(PathBuf::as_path).collect();

    // Build files_read: lib files first (matching tsc --listFiles order), then user files
    let mut files_read: Vec<PathBuf> = Vec::with_capacity(lib_paths.len() + user_files_read.len());
    files_read.extend(lib_paths.iter().cloned());
    files_read.append(&mut user_files_read);
    // Load and bind each lib exactly once, then reuse for:
    // 1) user-file binding (global symbol availability during bind)
    // 2) checker lib contexts (global symbol/type resolution)
    let load_libs_start = Instant::now();
    let lib_files: Vec<Arc<LibFile>> = parallel::load_lib_files_for_binding_strict(&lib_path_refs)?;
    let load_libs_duration = load_libs_start.elapsed();
    perf_log_phase("load_libs", load_libs_start);

    // PERF: Start cloning checker lib binders in a background thread while we
    // build the user program. The checker needs fresh binder state (separate from
    // the binding-phase libs) because it mutates during declaration merging.
    // For single-file work this can introduce cross-thread ordering
    // nondeterminism in rare cases; keep the optimization only when it is likely
    // to help on larger projects.
    let should_clone_libs_in_parallel = !resolved.no_check && sources.len() > 1;
    let checker_lib_handle = if should_clone_libs_in_parallel {
        let lib_files_clone = lib_files.clone();
        Some(
            std::thread::Builder::new()
                .name("tsz-checker-lib-clone".to_string())
                .stack_size(tsz_common::limits::THREAD_STACK_SIZE_BYTES)
                .spawn(move || load_checker_libs(&lib_files_clone))
                .context("failed to spawn checker lib clone thread")?,
        )
    } else {
        None
    };

    let build_program_start = Instant::now();
    let (program, dirty_paths) = if let Some(ref mut c) = effective_cache {
        let result = build_program_with_cache(
            sources,
            c,
            &lib_files,
            resolved.checker.target,
            resolved.checker.module_detection,
        );
        (result.program, Some(result.dirty_paths))
    } else {
        let compile_inputs: Vec<(String, String)> = sources
            .into_iter()
            .map(|source| {
                let text = source.text.unwrap_or_else(|| {
                    // If source text is missing during compilation, use empty string
                    // This allows compilation to continue with a diagnostic error later
                    String::new()
                });
                (source.path.to_string_lossy().into_owned(), text)
            })
            .collect();
        let bind_results = parallel::parse_and_bind_parallel_with_libs_and_options(
            compile_inputs,
            &lib_files,
            resolved.checker.target,
            resolved.checker.module_detection,
        );
        (Arc::new(parallel::merge_bind_results(bind_results)), None)
    };
    let parse_bind_duration = build_program_start.elapsed();
    perf_log_phase("build_program", build_program_start);

    // Update import symbol IDs if we have a cache
    if let Some(ref mut c) = effective_cache {
        update_import_symbol_ids(&program, &resolved, &base_dir, c);
    }

    // Wait for checker lib clones (already running in background)
    let build_lib_contexts_start = Instant::now();
    let checker_libs = match checker_lib_handle {
        Some(handle) => handle.join().expect("checker lib loading panicked"),
        None if resolved.no_check => check::CheckerLibSet::default(),
        None => load_checker_libs(&lib_files),
    };
    perf_log_phase("build_lib_contexts", build_lib_contexts_start);

    let collect_diagnostics_start = Instant::now();
    let parallel_type_caches = std::sync::Mutex::new(FxHashMap::default());
    // PERF: only walk the DefinitionStore for statistics() when the CLI
    // will actually print or write them. Saves an O(N) DashMap iteration
    // (and another in `estimated_size_bytes`) on the hot collect_diagnostics
    // return path.
    let collect_compile_stats =
        args.diagnostics || args.extended_diagnostics || args.generate_trace.is_some();
    let collected = collect_diagnostics_with_source_resolutions(
        &CollectDiagnosticsInput {
            program: &program,
            options: &resolved,
            base_dir: &base_dir,
            reference_path_current_directory: (!args.files.is_empty())
                .then_some(base_dir.as_path()),
            checker_libs: &checker_libs,
            typescript_dom_replacement_globals,
            has_deprecation_diagnostics,
            collect_compile_stats,
        },
        effective_cache,
        &parallel_type_caches,
        Some(&module_resolutions),
        Some(&module_resolution_misses),
        Some(&depth_skipped_js),
    );
    let mut diagnostics: Vec<Diagnostic> = collected.diagnostics;
    let check_duration = collect_diagnostics_start.elapsed();
    perf_log_phase("collect_diagnostics", collect_diagnostics_start);

    // Get reference to type caches for declaration emit.
    // In the parallel (no-cache) path, type caches are returned via the
    // Mutex parameter. In the cached/incremental path they live in the
    // CompilationCache.
    let parallel_type_caches = parallel_type_caches
        .into_inner()
        .expect("parallel_type_caches mutex should not be poisoned");
    let type_caches_ref: &FxHashMap<_, _> = if !parallel_type_caches.is_empty() {
        &parallel_type_caches
    } else {
        local_cache
            .as_ref()
            .map(|c| &c.type_caches)
            .or_else(|| cache.as_ref().map(|c| &c.type_caches))
            .unwrap_or(&parallel_type_caches)
    };
    // For binary files, suppress all diagnostics except TS1490.
    // Parsing UTF-16/corrupted content as UTF-8 produces cascading
    // TS1127 "Invalid character" false positives; tsc detects binary files
    // early and only emits TS1490.
    if !binary_file_names_to_suppress.is_empty() {
        diagnostics.retain(|d| !binary_file_names_to_suppress.contains(&d.file));
    }

    // A fatal legacy deprecation or TypeScript 7 removal notice takes priority
    // over file-level semantic diagnostics unless a grammar error outranks it.
    // See `apply_fatal_config_notice_priority`.
    if has_unknown_cli_compiler_option_diagnostic {
        apply_unknown_compiler_option_priority(&mut diagnostics);
    } else if has_fatal_config_notice {
        apply_fatal_config_notice_priority(&mut diagnostics, &mut config_diagnostics);
    }

    // JS-only-syntactic short-circuit for semantic diagnostics.
    //
    // tsc's `emitFilesAndReportErrors` runs `getSyntacticDiagnostics` first,
    // and only proceeds to options/global/semantic if no syntactic diagnostics
    // were produced. For JavaScript source files,
    // `getJSSyntacticDiagnosticsForFile` contributes a fixed set of `TS8xxx`
    // codes (and `TS8038`) — when any of those fire, every other file in the
    // program loses its semantic diagnostics too.
    //
    // tsz emits those `TS8xxx` codes from the checker, so we replicate the
    // gate here at the program level: when any JS-only-syntactic code is
    // present, retain only diagnostics that tsc would have surfaced via the
    // syntactic phase or the early config-parsing phase.
    {
        let has_js_only_syntactic_errors = diagnostics
            .iter()
            .any(|d| check_utils::is_js_only_syntactic_diagnostic(d.code));
        if has_js_only_syntactic_errors {
            diagnostics.retain(|d| {
                check_utils::keep_diagnostic_when_js_only_syntactic_skips_semantic(d.code)
            });
        }
    }

    diagnostics.extend(config_diagnostics);
    diagnostics.extend(binary_file_diagnostics);
    diagnostics.extend(type_file_diagnostics);

    // TS2304 suppression near TS8xxx JS grammar errors.
    // When TS8xxx errors exist in a project (type annotations in JS, JSDoc tag
    // errors, etc.), our checker can emit cascading false TS2304 errors. Suppress
    // TS2304 unless it co-occurs at the exact same file+position as a TS8xxx
    // error — tsc emits both in cases like `@extends {Mismatch}` (TS2304 + TS8023).
    {
        let has_js_grammar_errors = diagnostics
            .iter()
            .any(|d| tsz::checker::diagnostics::is_js_grammar_diagnostic(d.code));
        let has_jsdoc_invalid_template_order = diagnostics.iter().any(|d| {
            d.code
                == diagnostic_codes::A_JSDOC_TEMPLATE_TAG_MAY_NOT_FOLLOW_A_TYPEDEF_CALLBACK_OR_OVERLOAD_TAG
        });
        if has_js_grammar_errors {
            let ts8xxx_positions: rustc_hash::FxHashSet<(String, u32)> = diagnostics
                .iter()
                .filter(|d| tsz::checker::diagnostics::is_js_grammar_diagnostic(d.code))
                .map(|d| (d.file.clone(), d.start))
                .collect();
            diagnostics.retain(|d| {
                let keep_jsdoc_template_name_error = has_jsdoc_invalid_template_order
                    && d.code == 2304
                    && !d.message_text.contains("'U'")
                    && diagnostic_source_line(&program, d)
                        .is_some_and(|line| line.contains("@param"));
                d.code != 2304
                    || ts8xxx_positions.contains(&(d.file.clone(), d.start))
                    || keep_jsdoc_template_name_error
            });
        }
    }

    diagnostics.sort_by(|left, right| left.compare(right));

    let has_error = diagnostics
        .iter()
        .any(|diag| diag.category == DiagnosticCategory::Error);
    let should_emit = !(resolved.no_emit || (resolved.no_emit_on_error && has_error));

    // When --declaration is set, run declaration emit for diagnostics even
    // with --noEmit, because TS2883 (non-portable inferred types) fires
    // during declaration generation. In tsc, this check happens during the
    // checker's "declaration emit pre-check" phase.
    let should_run_declaration_emit_check =
        !should_emit && resolved.emit_declarations && resolved.no_emit;

    let mut dirty_paths = dirty_paths;
    if let Some(forced) = forced_dirty_paths {
        match &mut dirty_paths {
            Some(existing) => {
                existing.extend(forced.iter().cloned());
            }
            None => {
                dirty_paths = Some(forced.clone());
            }
        }
    }

    // Output layout follows tsc's `getCommonSourceDirectory()`. When `rootDir`
    // is set, that is the root. In project (tsconfig) mode the layout is anchored
    // at the config directory (the `base_dir` fallback used inside the
    // emitter, see TS5011), so nothing extra is needed there. But when
    // compilation is driven by an explicit file list with no tsconfig, tsc lays
    // output out relative to the longest common directory of the emittable
    // source files — not the cwd. Compute that implicit root so a single input
    // `src/a.ts --outDir out` emits to `out/a.js` (like tsc) rather than
    // `out/src/a.js`.
    let emit_root_dir = if root_dir.is_some() || tsconfig_path.is_some() {
        root_dir
    } else {
        emit_common_source_directory(
            program
                .files
                .iter()
                .map(|file| PathBuf::from(&file.file_name)),
            &base_dir,
            &cwd,
        )
    };

    let emit_outputs_start = Instant::now();
    let emitted_files = if !should_emit && !should_run_declaration_emit_check {
        Vec::new()
    } else {
        let (outputs, emit_diags) = emit_outputs(EmitOutputsContext {
            program: &program,
            options: &resolved,
            base_dir: &base_dir,
            root_file_paths: &root_file_paths,
            root_dir: emit_root_dir.as_deref(),
            out_dir: out_dir.as_deref(),
            declaration_dir: declaration_dir.as_deref(),
            dirty_paths: dirty_paths.as_ref(),
            outfile_bundle_paths: Some(&outfile_bundle_paths),
            outfile_bundle_dependencies: Some(&outfile_bundle_dependencies),
            type_caches: type_caches_ref,
        })?;
        diagnostics.extend(emit_diags);
        if should_emit {
            let blocked_declaration_sources = declaration_emit_blocking_source_files(&diagnostics);
            let block_all_declaration_outputs =
                has_global_declaration_emit_blocking_diagnostic(&diagnostics);
            if blocked_declaration_sources.is_empty() && !block_all_declaration_outputs {
                write_outputs(&outputs, resolved.emit_bom)?
            } else {
                let filtered_outputs: Vec<_> = outputs
                    .into_iter()
                    .filter(|output| {
                        should_write_output_after_declaration_diagnostics(
                            output,
                            &blocked_declaration_sources,
                            block_all_declaration_outputs,
                        )
                    })
                    .collect();
                write_outputs(&filtered_outputs, resolved.emit_bom)?
            }
        } else {
            // Declaration emit ran for diagnostics only (--noEmit with --declaration)
            Vec::new()
        }
    };
    let emit_duration = emit_outputs_start.elapsed();
    perf_log_phase("emit_outputs", emit_outputs_start);

    // Recompute has_error after emit diagnostics (e.g., TS2883) are added.
    // The initial has_error was computed before emit for should_emit gating.
    normalize_ts2883_diagnostics_in_place(&mut diagnostics);
    // Re-sort since emit diagnostics were appended after the initial sort.
    diagnostics.sort_by(|left, right| left.compare(right));
    let has_error = diagnostics
        .iter()
        .any(|diag| diag.category == DiagnosticCategory::Error);

    // Most recent declaration output for BuildInfo tracking. When this build
    // wrote no declaration file, carry the previously saved value forward:
    // tsc preserves `latestChangedDtsFile` across no-emit incremental saves
    // (`createBuilderProgramState` seeds it from the old program state and
    // the write-file callback only reassigns it when a declaration file is
    // written).
    let latest_changed_dts_file =
        find_latest_dts_file(&emitted_files, &base_dir).or(prior_latest_changed_dts_file);

    // Save BuildInfo if incremental compilation is enabled
    if should_save_build_info && !has_error {
        let tsconfig_path_ref = tsconfig_path.as_deref();
        if let Some(build_info_path) = get_build_info_path(tsconfig_path_ref, &resolved, &base_dir)
        {
            // Build BuildInfo from the cache (which has been updated by collect_diagnostics)
            // If local_cache exists (from BuildInfo), use it; otherwise create minimal info
            // The most recent declaration output (used for cross-project
            // invalidation) is assigned at construction time in both branches.
            let build_info = if let Some(ref lc) = local_cache {
                compilation_cache_to_build_info(
                    lc,
                    &file_paths,
                    &base_dir,
                    &resolved,
                    latest_changed_dts_file,
                )
            } else {
                // No cache available - create minimal BuildInfo with just file info
                BuildInfo {
                    version: crate::incremental::BUILD_INFO_VERSION.to_string(),
                    compiler_version: env!("CARGO_PKG_VERSION").to_string(),
                    root_files: file_paths
                        .iter()
                        .map(|p| {
                            p.strip_prefix(&base_dir)
                                .unwrap_or(p)
                                .to_string_lossy()
                                .replace('\\', "/")
                        })
                        .collect(),
                    latest_changed_dts_file,
                    ..Default::default()
                }
            };

            if let Err(e) = build_info.save(&build_info_path) {
                let build_info_path_text = build_info_path.display().to_string();
                let formatted_error = format_file_write_error_for_diagnostic(&build_info_path, &e);
                diagnostics.push(Diagnostic::from_code(
                    diagnostic_codes::COULD_NOT_WRITE_FILE,
                    "",
                    0,
                    0,
                    &[&build_info_path_text, &formatted_error],
                ));
                tracing::warn!(
                    "Failed to save BuildInfo to {}: {}",
                    build_info_path.display(),
                    e
                );
            } else {
                tracing::info!("Saved BuildInfo to: {}", build_info_path.display());
            }
        }
    }

    if perf_enabled {
        tracing::info!(
            target: "wasm::perf",
            phase = "compile_total",
            ms = compile_start.elapsed().as_secs_f64() * 1000.0,
            files = file_paths.len(),
            libs = lib_files.len(),
            diagnostics = diagnostics.len(),
            emitted = emitted_files.len(),
            no_check = resolved.no_check
        );
    }

    Ok(CompilationResult {
        diagnostics,
        emitted_files,
        files_read,
        file_infos,
        no_emit: resolved.no_emit,
        request_cache_counters: collected.request_cache_counters,
        interned_types_count: program.type_interner.len(),
        interner_estimated_bytes: program.type_interner.estimated_size_bytes(),
        query_cache_stats: collected.query_cache_stats,
        def_store_stats: collected.def_store_stats,
        phase_timings: PhaseTimings {
            io_read_ms: io_read_duration.as_secs_f64() * 1000.0,
            load_libs_ms: load_libs_duration.as_secs_f64() * 1000.0,
            parse_bind_ms: parse_bind_duration.as_secs_f64() * 1000.0,
            check_ms: check_duration.as_secs_f64() * 1000.0,
            emit_ms: emit_duration.as_secs_f64() * 1000.0,
            total_ms: compile_start.elapsed().as_secs_f64() * 1000.0,
            // T0.2 follow-up: sub-phase buckets reserved by the
            // diagnostics-JSON schema. Driver attribution lands in a
            // future PR; for now these stay 0.0 and the leftover sits
            // in the parent bucket (`io_read_ms` / `parse_bind_ms`).
            ..PhaseTimings::default()
        },
        // PERF: residency_stats walks every unique arena (estimated_size_bytes
        // per arena), every bound file, the skeleton index, and the dep graph
        // — only worth paying for when --extendedDiagnostics actually prints
        // the numbers. See iter 4's collect_compile_stats gating for parity.
        residency_stats: collect_compile_stats.then(|| program.residency_stats()),
        module_dep_stats: collected.module_dep_stats,
        invalidation_summaries: Vec::new(),
    })
}

pub(super) fn declaration_emit_blocking_source_files(
    diagnostics: &[Diagnostic],
) -> FxHashSet<PathBuf> {
    diagnostics
        .iter()
        .filter(|diagnostic| {
            diagnostic.category == DiagnosticCategory::Error
                && is_declaration_emit_blocking_diagnostic_code(diagnostic.code)
                && !diagnostic.file.is_empty()
        })
        .map(|diagnostic| normalize_path(Path::new(&diagnostic.file)))
        .collect()
}

pub(super) fn has_global_declaration_emit_blocking_diagnostic(diagnostics: &[Diagnostic]) -> bool {
    diagnostics.iter().any(|diagnostic| {
        diagnostic.category == DiagnosticCategory::Error
            && diagnostic.code
                == diagnostic_codes::DECLARATION_AUGMENTS_DECLARATION_IN_ANOTHER_FILE_THIS_CANNOT_BE_SERIALIZED
    })
}

pub(super) const fn is_declaration_emit_blocking_diagnostic_code(code: u32) -> bool {
    // TS9007–TS9039 are the `--isolatedDeclarations` family. tsc refuses to
    // emit a `.d.ts` for any source whose isolated-declaration constraints
    // were violated, regardless of `--noCheck` (#3709 follow-up). TS4020
    // (existing entry) blocks emit when an exported name leaks an external
    // module-private symbol that can't be re-exported.
    matches!(
        code,
        diagnostic_codes::EXPORTED_VARIABLE_HAS_OR_IS_USING_NAME_FROM_EXTERNAL_MODULE_BUT_CANNOT_BE_NAMED
            | diagnostic_codes::DECLARATION_AUGMENTS_DECLARATION_IN_ANOTHER_FILE_THIS_CANNOT_BE_SERIALIZED
            | 9007..=9039,
    )
}

pub(super) fn should_write_output_after_declaration_diagnostics(
    output: &OutputFile,
    blocked_sources: &FxHashSet<PathBuf>,
    block_all_declaration_outputs: bool,
) -> bool {
    if !is_declaration_output_path(&output.path) {
        return true;
    }
    if block_all_declaration_outputs {
        return false;
    }

    let Some(source_path) = output.source_path.as_ref() else {
        return blocked_sources.is_empty();
    };
    !blocked_sources.contains(&normalize_path(source_path))
}

pub(super) fn is_declaration_output_path(path: &Path) -> bool {
    let path = path.to_string_lossy();
    path.ends_with(".d.ts") || path.ends_with(".d.ts.map")
}

pub(super) fn normalize_ts2883_diagnostics_in_place(
    diagnostics: &mut Vec<tsz_common::diagnostics::Diagnostic>,
) {
    // Step 1: Exact dedup — remove diagnostics that are complete duplicates
    // (same code, file, start, length, and message). This handles pre-existing
    // duplicate checker emissions (e.g., TS2427 emitted twice per declaration).
    let mut exact_seen: FxHashSet<(u32, String, u32, u32, String)> = FxHashSet::default();
    diagnostics.retain(|d| {
        exact_seen.insert((
            d.code,
            d.file.clone(),
            d.start,
            d.length,
            d.message_text.clone(),
        ))
    });

    // Step 2: TS2883 position dedup — the checker and declaration emitter can both
    // emit TS2883 for the same declaration. Checker diagnostics are sorted to the
    // front (line 1078 sort runs before emitter diagnostics are appended at 1121),
    // so "first wins" preserves the checker's canonical message.
    let mut seen_2883: FxHashSet<(String, u32)> = FxHashSet::default();
    diagnostics.retain(|d| {
        if d.code == 2883 {
            seen_2883.insert((d.file.clone(), d.start))
        } else {
            true
        }
    });
}

pub(super) fn config_error_result(
    file_path: Option<&Path>,
    message: String,
    code: u32,
) -> CompilationResult {
    let file = file_path
        .map(|p| p.display().to_string())
        .unwrap_or_default();
    CompilationResult {
        diagnostics: vec![Diagnostic::error(file, 0, 0, message, code)],
        emitted_files: Vec::new(),
        files_read: Vec::new(),
        file_infos: Vec::new(),
        no_emit: false,
        request_cache_counters: tsz::checker::context::RequestCacheCounters::default(),
        interned_types_count: 0,
        interner_estimated_bytes: 0,
        query_cache_stats: None,
        def_store_stats: None,
        phase_timings: PhaseTimings::default(),
        residency_stats: None,
        module_dep_stats: None,
        invalidation_summaries: Vec::new(),
    }
}

pub(super) fn no_input_diagnostics_for_config(
    mut config_diagnostics: Vec<Diagnostic>,
    tsconfig_path: Option<&Path>,
    include: Option<&[String]>,
    exclude: Option<&[String]>,
    _allow_js: bool,
) -> Vec<Diagnostic> {
    // Emit TS18003: No inputs were found in config file.
    // Match tsc: use the resolved config path shown to the compiler.
    let config_name = tsconfig_path
        .map(|path| canonicalize_or_owned(path).to_string_lossy().to_string())
        .unwrap_or_else(|| "tsconfig.json".to_string());
    let include_str = match include {
        Some(v) if !v.is_empty() => v
            .iter()
            .map(|s| format!("\"{s}\""))
            .collect::<Vec<_>>()
            .join(","),
        Some(_) => String::new(),
        None => crate::fs::default_include_display()
            .into_iter()
            .map(|s| format!("\"{s}\""))
            .collect::<Vec<_>>()
            .join(","),
    };
    let exclude_str = exclude
        .filter(|v| !v.is_empty())
        .map(|v| {
            v.iter()
                .map(|s| format!("\"{s}\""))
                .collect::<Vec<_>>()
                .join(",")
        })
        .unwrap_or_default();
    let message = format!(
        "No inputs were found in config file '{config_name}'. Specified 'include' paths were '[{include_str}]' and 'exclude' paths were '[{exclude_str}]'."
    );
    // tsc emits TS18003 without file position (file="", pos=0).
    config_diagnostics.push(Diagnostic::error(String::new(), 0, 0, message, 18003));
    config_diagnostics
}

pub(super) fn unsupported_explicit_file_diagnostics(
    discovery: &FileDiscoveryOptions,
) -> Vec<Diagnostic> {
    if discovery.files.is_empty() {
        return Vec::new();
    }

    discovery
        .files
        .iter()
        .filter_map(|file| {
            let path = if file.is_absolute() {
                file.clone()
            } else {
                discovery.base_dir.join(file)
            };
            if is_ts_file(&path)
                || is_js_file(&path)
                || (discovery.resolve_json_module && is_json_file(&path))
            {
                return None;
            }

            let supported_extensions = supported_extensions_display(discovery);
            let path_display = path.to_string_lossy().to_string();
            Some(
                Diagnostic::from_code(
                    diagnostic_codes::FILE_HAS_AN_UNSUPPORTED_EXTENSION_THE_ONLY_SUPPORTED_EXTENSIONS_ARE,
                    String::new(),
                    0,
                    0,
                    &[&path_display, &supported_extensions],
                )
                .with_related(
                    String::new(),
                    0,
                    0,
                    "The file is in the program because:\n  Part of 'files' list in tsconfig.json",
                ),
            )
        })
        .collect()
}

pub(super) fn supported_extensions_display(discovery: &FileDiscoveryOptions) -> String {
    let mut extensions: Vec<&str> = TS_FAMILY_EXTENSIONS.to_vec();
    if discovery.allow_js {
        extensions.extend(JS_FAMILY_EXTENSIONS);
    }
    if discovery.resolve_json_module {
        extensions.push(JSON_EXTENSION);
    }

    extensions
        .iter()
        .map(|ext| format!("'{ext}'"))
        .collect::<Vec<_>>()
        .join(", ")
}

#[cfg(test)]
pub(super) fn check_module_resolution_compatibility_mut(
    resolved: &ResolvedCompilerOptions,
    tsconfig_path: Option<&Path>,
    diagnostics: &mut Vec<Diagnostic>,
) -> bool {
    if let Some(diag) = check_module_resolution_compatibility(resolved, tsconfig_path) {
        diagnostics.push(diag);
        true
    } else {
        false
    }
}

#[cfg(test)]
pub(super) fn check_module_resolution_compatibility(
    resolved: &ResolvedCompilerOptions,
    tsconfig_path: Option<&Path>,
) -> Option<Diagnostic> {
    use tsz::checker::diagnostics::{diagnostic_messages, format_message};
    use tsz::config::ModuleResolutionKind;

    let module_resolution = resolved.module_resolution?;
    // Only check when moduleResolution is Node16 or NodeNext
    let is_node_resolution = matches!(
        module_resolution,
        ModuleResolutionKind::Node16 | ModuleResolutionKind::NodeNext
    );
    if !is_node_resolution {
        return None;
    }

    // tsc accepts any module in the Node16..NodeNext range with node-style resolution
    if resolved.printer.module.is_node_module() {
        return None;
    }

    // Determine the name to display in the diagnostic message
    let resolution_str = match module_resolution {
        ModuleResolutionKind::NodeNext => "NodeNext",
        _ => "Node16",
    };
    let required_str = resolution_str;

    let message = format_message(
        diagnostic_messages::OPTION_MODULE_MUST_BE_SET_TO_WHEN_OPTION_MODULERESOLUTION_IS_SET_TO,
        &[required_str, resolution_str],
    );
    let file = tsconfig_path
        .map(|p| p.display().to_string())
        .unwrap_or_default();
    Some(Diagnostic::error(
        file,
        0,
        0,
        message,
        diagnostic_codes::OPTION_MODULE_MUST_BE_SET_TO_WHEN_OPTION_MODULERESOLUTION_IS_SET_TO,
    ))
}

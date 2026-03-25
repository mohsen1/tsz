//! Diagnostics collection and per-file checking orchestration for the compilation driver.

use super::check_utils::*;
use super::*;
use tsz::checker::context::RequestCacheCounters;

pub(super) struct CollectDiagnosticsResult {
    pub diagnostics: Vec<Diagnostic>,
    pub request_cache_counters: RequestCacheCounters,
    /// Aggregate query-cache statistics from the sequential path's shared `QueryCache`.
    /// `None` in the parallel path (each thread has its own short-lived cache).
    pub query_cache_stats: Option<tsz_solver::QueryCacheStatistics>,
    /// Aggregate definition-store statistics (populated for `--extendedDiagnostics`).
    pub def_store_stats: Option<tsz_solver::StoreStatistics>,
    /// Module dependency graph statistics (populated for `--extendedDiagnostics`).
    pub module_dep_stats: Option<super::ModuleDependencyStats>,
}

/// Check if a filename is a TypeScript declaration file (.d.ts, .d.cts, .d.mts).
fn is_declaration_file(name: &str) -> bool {
    tsz::module_resolver::ModuleExtension::from_path(std::path::Path::new(name)).is_declaration()
}

/// Load lib.d.ts files and create `LibContext` objects for the checker.
///
/// The binding pipeline mutates per-file binder state while injecting lib symbols into the
/// unified program. Reusing those same `LibFile` binders as checker lib contexts leaks that
/// binding-phase state into lib type resolution and can corrupt structural relations between
/// recursive lib types like `Promise<T>` and `PromiseLike<T>`.
///
/// Build a fresh checker-facing lib context set from the same on-disk lib files so program
/// binding and checker lib resolution stay isolated.
pub(super) fn load_lib_files_for_contexts(lib_files: &[Arc<LibFile>]) -> Vec<LibContext> {
    if lib_files.is_empty() {
        return Vec::new();
    }

    let lib_paths: Vec<PathBuf> = lib_files
        .iter()
        .map(|lib| PathBuf::from(&lib.file_name))
        .collect();
    let lib_path_refs: Vec<_> = lib_paths.iter().map(PathBuf::as_path).collect();
    let fresh_lib_files = parallel::load_lib_files_for_binding_strict(&lib_path_refs)
        .expect("failed to reload lib files for checker contexts");

    fresh_lib_files
        .iter()
        .map(|lib| LibContext {
            arena: Arc::clone(&lib.arena),
            binder: Arc::clone(&lib.binder),
        })
        .collect()
}

fn program_has_real_syntax_errors(program: &MergedProgram) -> bool {
    program
        .files
        .iter()
        .flat_map(|file| file.parse_diagnostics.iter())
        .any(|diag| is_real_syntax_error(diag.code))
}

const fn is_reserved_type_name_declaration_diagnostic(code: u32) -> bool {
    matches!(code, 2427 | 2457)
}

fn keep_checker_diagnostic_when_program_has_real_syntax_errors(code: u32) -> bool {
    // tsc suppresses type-level semantic diagnostics when any source file in the
    // program has a real syntax error, but it still reports declaration-name
    // diagnostics such as TS2427/TS2457 alongside parse errors because the parser
    // accepts those names and defers validation to the checker.
    code < 2000
        || (8000..9000).contains(&code)
        || is_reserved_type_name_declaration_diagnostic(code)
}

fn post_process_checker_diagnostics(
    checker_diagnostics: &mut Vec<Diagnostic>,
    file: &BoundFile,
    options: &ResolvedCompilerOptions,
    program_has_real_syntax_errors: bool,
) {
    let is_js = is_js_file(Path::new(&file.file_name));
    let has_ts_check_pragma = js_file_has_ts_check_pragma(file);
    let has_ts_nocheck_pragma = js_file_has_ts_nocheck_pragma(file);
    let should_filter_type_errors =
        is_js && (has_ts_nocheck_pragma || (!options.check_js && !has_ts_check_pragma));

    if should_filter_type_errors {
        // Keep syntax/semantic diagnostics (< 2000) and JS grammar diagnostics
        // (TS8xxx). When `checkJs` is NOT explicitly false (the default
        // no-checkJs mode), also allow the `plainJSErrors` codes that tsc
        // surfaces even in unchecked JS files. When `checkJs: false` is
        // explicitly set, suppress ALL semantic errors.
        checker_diagnostics.retain(|diag| {
            diag.code < 2000
                || (8000..9000).contains(&diag.code)
                || (!options.explicit_check_js_false && is_plain_js_allowed_code(diag.code))
        });
    }

    // For JS files, suppress checker-emitted TS1xxx grammar codes that tsc
    // does NOT emit for JavaScript files. tsc's grammar checks (emitted via
    // grammarErrorOnNode) are suppressed for TypeScript-only constructs in JS
    // files because its parser handles them leniently. Our parser doesn't
    // distinguish JS vs TS, so checker-side grammar errors leak through.
    // Only keep TS1xxx codes that tsc is known to emit for JS files.
    if is_js {
        checker_diagnostics.retain(|diag| {
            if (1000..2000).contains(&diag.code) {
                return is_ts1xxx_allowed_in_js(diag.code);
            }
            // Also suppress checker-emitted grammar codes outside the 1xxx range
            // that tsc doesn't emit for JS files.
            if is_checker_grammar_code_suppressed_in_js(diag.code) {
                return false;
            }
            true
        });
    }

    if program_has_real_syntax_errors {
        checker_diagnostics
            .retain(|diag| keep_checker_diagnostic_when_program_has_real_syntax_errors(diag.code));
    }

    // Suppress semantic errors that cascade from structural parse failures.
    // tsc sets per-node ThisNodeHasError flags and skips semantic checks on
    // error-recovery subtrees. We approximate this by suppressing semantic
    // diagnostics that are near a structural parse error (within a distance
    // window). Only structural parse failures (missing tokens, unexpected
    // tokens) trigger suppression — grammar checks like trailing commas or
    // strict mode violations don't cause AST malformation and shouldn't
    // suppress semantic errors.
    let structural_error_positions: Vec<u32> = file
        .parse_diagnostics
        .iter()
        .filter(|d| is_structural_parse_error(d.code))
        .map(|d| d.start)
        .collect();
    if !structural_error_positions.is_empty() {
        const MAX_CASCADE_DISTANCE: u32 = 300;
        checker_diagnostics.retain(|diag| {
            // Keep parse/grammar errors (1xxx) and JS grammar errors (8xxx)
            if diag.code < 2000 || (8000..9000).contains(&diag.code) {
                return true;
            }
            // Some semantic errors are deliberately emitted alongside
            // structural parse errors and must not be suppressed.
            // TS2427 / TS2457 are checker-side validation for reserved type names
            // in interface/type-alias declarations. TSC keeps them even when the
            // surrounding file also has structural parse errors.
            if is_reserved_type_name_declaration_diagnostic(diag.code) {
                return true;
            }
            // Suppress if a structural parse error is within the cascade window
            !structural_error_positions.iter().any(|&err_pos| {
                let dist = diag.start.abs_diff(err_pos);
                dist <= MAX_CASCADE_DISTANCE
            })
        });
    }
}

pub(super) fn collect_diagnostics(
    program: &MergedProgram,
    options: &ResolvedCompilerOptions,
    base_dir: &Path,
    cache: Option<&mut CompilationCache>,
    lib_contexts: &[LibContext],
    typescript_dom_replacement_globals: (bool, bool, bool),
    type_cache_output: &std::sync::Mutex<FxHashMap<PathBuf, TypeCache>>,
    has_deprecation_diagnostics: bool,
) -> CollectDiagnosticsResult {
    let _collect_span =
        tracing::info_span!("collect_diagnostics", files = program.files.len()).entered();
    let mut diagnostics: Vec<Diagnostic> = Vec::new();
    let mut request_cache_counters = RequestCacheCounters::default();
    let mut used_paths = FxHashSet::default();
    let mut cache = cache;
    let mut resolution_cache = ModuleResolutionCache::default();
    let mut program_paths = FxHashSet::default();
    let mut canonical_to_file_name: FxHashMap<PathBuf, String> = FxHashMap::default();
    let mut canonical_to_file_idx: FxHashMap<PathBuf, usize> = FxHashMap::default();
    let program_has_real_syntax_errors = program_has_real_syntax_errors(program);

    {
        let _span = tracing::info_span!("build_program_path_maps").entered();
        for (idx, file) in program.files.iter().enumerate() {
            let canonical = canonicalize_or_owned(Path::new(&file.file_name));
            program_paths.insert(canonical.clone());
            canonical_to_file_name.insert(canonical.clone(), file.file_name.clone());
            canonical_to_file_idx.insert(canonical, idx);
        }
    }

    // Pre-create all binders for cross-file resolution
    let all_binders: Arc<Vec<Arc<BinderState>>> = Arc::new({
        let _span = tracing::info_span!("build_cross_file_binders").entered();
        program
            .files
            .iter()
            .enumerate()
            .map(|(file_idx, file)| {
                Arc::new(create_cross_file_lookup_binder(file, program, file_idx))
            })
            .collect()
    });

    // Extract is_external_module from BoundFile to preserve state across file bindings
    // This fixes TS2664 which requires accurate per-file is_external_module values
    let is_external_module_by_file: Arc<rustc_hash::FxHashMap<String, bool>> = Arc::new(
        program
            .files
            .iter()
            .map(|file| (file.file_name.clone(), file.is_external_module))
            .collect(),
    );

    // Collect all arenas for cross-file resolution
    let all_arenas: Arc<Vec<Arc<NodeArena>>> = Arc::new({
        let _span = tracing::info_span!("collect_all_arenas").entered();
        program
            .files
            .iter()
            .map(|file| Arc::clone(&file.arena))
            .collect()
    });
    let symbol_file_targets: Arc<Vec<(tsz::binder::SymbolId, usize)>> = Arc::new(
        program
            .symbol_arenas
            .iter()
            .filter_map(|(sym_id, arena)| {
                all_arenas
                    .iter()
                    .position(|file_arena| Arc::ptr_eq(file_arena, arena))
                    .map(|file_idx| (*sym_id, file_idx))
            })
            .collect(),
    );

    // Create ModuleResolver instance for proper error reporting (TS2834, TS2835, TS2792, etc.)
    let mut module_resolver = ModuleResolver::new(options);

    // Build resolved_module_paths map: (source_file_idx, specifier) -> target_file_idx
    // Also build resolved_module_errors map for specific error codes
    let mut resolved_module_paths: FxHashMap<(usize, String), usize> = FxHashMap::default();
    let mut resolved_module_specifiers: FxHashSet<(usize, String)> = FxHashSet::default();
    let mut resolved_module_errors: FxHashMap<
        (usize, String),
        tsz::checker::context::ResolutionError,
    > = FxHashMap::default();

    // Cache module specifiers per file — collected once, reused in prepare_binders
    // and check_file_for_parallel to avoid 3× redundant AST traversals.
    let mut cached_module_specifiers: Vec<
        Vec<(
            String,
            tsz::parser::NodeIndex,
            tsz::module_resolver::ImportKind,
        )>,
    > = Vec::with_capacity(program.files.len());

    {
        let _span = tracing::info_span!("build_resolved_module_maps").entered();
        for (file_idx, file) in program.files.iter().enumerate() {
            cached_module_specifiers.push(collect_module_specifiers(&file.arena, file.source_file));
            let file_path = Path::new(&file.file_name);

            for (specifier, specifier_node, import_kind) in &cached_module_specifiers[file_idx] {
                let span = if let Some(spec_node) = file.arena.get(*specifier_node) {
                    Span::new(spec_node.pos, spec_node.end)
                } else {
                    Span::new(0, 0)
                };

                let request = tsz::module_resolver::ModuleLookupRequest {
                    specifier,
                    containing_file: file_path,
                    specifier_span: span,
                    import_kind: *import_kind,
                    no_implicit_any: options.checker.no_implicit_any,
                    implied_classic_resolution: options.checker.implied_classic_resolution,
                };

                let result = module_resolver.lookup(
                    &request,
                    |spec, fp| {
                        resolve_module_specifier(
                            fp,
                            spec,
                            options,
                            base_dir,
                            &mut resolution_cache,
                            &program_paths,
                        )
                    },
                    |spec| {
                        program.declared_modules.contains(spec)
                            || program.shorthand_ambient_modules.contains(spec)
                    },
                );

                // Classify the lookup result into a driver-facing outcome
                let outcome = result.classify();

                if std::env::var_os("TSZ_DEBUG_RESOLVE").is_some() {
                    tracing::debug!(
                        "module lookup: file={} spec={} resolved={:?} is_resolved={} error={:?}",
                        file_path.display(),
                        specifier,
                        outcome.resolved_path,
                        outcome.is_resolved,
                        outcome.error,
                    );
                }

                // Map resolved path to file index
                if let Some(ref resolved_path) = outcome.resolved_path {
                    resolved_module_specifiers.insert((file_idx, specifier.clone()));
                    let canonical = canonicalize_or_owned(resolved_path);
                    if let Some(&target_idx) = canonical_to_file_idx.get(&canonical) {
                        resolved_module_paths.insert((file_idx, specifier.clone()), target_idx);
                    }
                } else if outcome.is_resolved {
                    resolved_module_specifiers.insert((file_idx, specifier.clone()));
                }

                // Record error for the checker
                if let Some(ref error) = outcome.error {
                    resolved_module_errors.insert(
                        (file_idx, specifier.clone()),
                        tsz::checker::context::ResolutionError {
                            code: error.code,
                            message: error.message.clone(),
                        },
                    );
                }
            }
        }
    }

    let resolved_module_paths = Arc::new(resolved_module_paths);
    let resolved_module_specifiers = Arc::new(resolved_module_specifiers);
    let resolved_module_errors = Arc::new(resolved_module_errors);

    // Pre-compute per-file ESM/CJS module kind for Node16/NodeNext resolution.
    // In these modes, .js/.ts files may be ESM based on package.json "type" field.
    // The checker needs this to correctly emit TS1479 (CJS importing ESM).
    let file_is_esm_map: Arc<FxHashMap<String, bool>> = Arc::new({
        let resolution_kind = options.effective_module_resolution();
        let is_node_resolution = matches!(
            resolution_kind,
            crate::config::ModuleResolutionKind::Node16
                | crate::config::ModuleResolutionKind::NodeNext
        );
        if is_node_resolution {
            program
                .files
                .iter()
                .map(|file| {
                    let file_path = Path::new(&file.file_name);
                    let kind = module_resolver.get_importing_module_kind(file_path);
                    (
                        file.file_name.clone(),
                        kind == tsz::module_resolver::ImportingModuleKind::Esm,
                    )
                })
                .collect()
        } else {
            FxHashMap::default()
        }
    });

    // Propagate noUncheckedIndexedAccess to the TypeInterner before creating the
    // QueryCache.  The `with_options` constructor intentionally skips this (to avoid
    // repeated writes from each per-file checker), so we set it once here.
    program
        .type_interner
        .set_no_unchecked_indexed_access(options.checker.no_unchecked_indexed_access);

    // Create a shared QueryCache for memoized evaluate_type/is_subtype_of calls.
    let query_cache = QueryCache::new(&program.type_interner);

    // Pre-compute declared modules from skeleton when available.
    // This avoids re-scanning all binders for declared/ambient module names in
    // each checker's `set_all_binders` call — the skeleton captured this data
    // during the parallel parse/bind phase.
    let skeleton_declared_modules: Option<Arc<tsz::checker::context::GlobalDeclaredModules>> =
        program.skeleton_index.as_ref().map(|skel| {
            let (exact, patterns) = skel.build_declared_module_sets();
            Arc::new(tsz::checker::context::GlobalDeclaredModules::from_skeleton(
                exact, patterns,
            ))
        });

    // Pre-compute expando index from skeleton when available.
    // This avoids re-scanning all binders for expando property assignments.
    let skeleton_expando_index: Option<Arc<FxHashMap<String, FxHashSet<String>>>> = program
        .skeleton_index
        .as_ref()
        .map(|skel| Arc::new(skel.expando_properties.clone()));

    // Build the project-wide shared environment once for all checkers (prime, parallel, sequential).
    // build_global_indices computes the 4 binder-derived indices once here so that
    // per-file checker creation via apply_to skips the O(N) binder scans.
    let mut project_env = tsz::checker::context::ProjectEnv {
        lib_contexts: lib_contexts.to_vec(),
        all_arenas: Arc::clone(&all_arenas),
        all_binders: Arc::clone(&all_binders),
        skeleton_declared_modules,
        skeleton_expando_index,
        symbol_file_targets: Arc::clone(&symbol_file_targets),
        resolved_module_paths: Arc::clone(&resolved_module_paths),
        resolved_module_errors: Arc::clone(&resolved_module_errors),
        is_external_module_by_file: Arc::clone(&is_external_module_by_file),
        file_is_esm_map: Arc::clone(&file_is_esm_map),
        typescript_dom_replacement_globals,
        has_deprecation_diagnostics,
        ..Default::default()
    };
    // Use fingerprint-aware rebuild when a skeleton index is available.
    // On the first build this always rebuilds; on subsequent incremental builds
    // with the same skeleton fingerprint the O(N) binder scan is skipped.
    if let Some(ref skel) = program.skeleton_index {
        project_env.build_global_indices_if_changed(skel.fingerprint);
    } else {
        project_env.build_global_indices();
    }
    // Build the shared SymbolId→file-index map once; shared via Arc across all checkers.
    // TODO: build_global_symbol_file_index not yet implemented on ProjectEnv

    // Prime Array<T> base type with global augmentations before any file checks.
    // CRITICAL: The prime checker and all file checkers MUST share the same DefinitionStore.
    if !program.files.is_empty() && !lib_contexts.is_empty() {
        let prime_idx = 0;
        let file = &program.files[prime_idx];
        let binder = parallel::create_binder_from_bound_file(file, program, prime_idx);
        let mut checker = CheckerState::with_options(
            &file.arena,
            &binder,
            &query_cache,
            file.file_name.clone(),
            &options.checker,
        );
        project_env.apply_to(&mut checker.ctx);
        checker.prime_boxed_types();
    }

    // --- SMART INVALIDATION: Work Queue Algorithm ---
    // Only type-check files that have changed or depend on files with changed export signatures

    let mut work_queue: VecDeque<usize> = VecDeque::new();
    let mut checked_files: FxHashSet<usize> = FxHashSet::default();

    // Mark all files as used for cache cleanup
    for (idx, file) in program.files.iter().enumerate() {
        let file_path = PathBuf::from(&file.file_name);
        used_paths.insert(file_path.clone());

        // Check if file has cached type information
        // If no cache or cache miss, file needs to be checked
        let needs_check = cache
            .as_deref()
            .is_none_or(|c| !c.type_caches.contains_key(&file_path)); // No cache at all -> check everything

        if needs_check {
            work_queue.push_back(idx);
            checked_files.insert(idx);
        }
    }

    // --- FILE CHECKING ---
    //
    // Two paths:
    // 1. Non-cached (first build, CI): Check ALL files in parallel using rayon.
    //    No dependency cascade needed since we're checking everything.
    // 2. Cached (watch mode): Sequential work queue with export-hash-based
    //    dependency cascade for incremental invalidation.

    // Accumulates query-cache statistics from whichever path is taken.
    // Both branches unconditionally set this, so the initial value is never read.
    #[allow(unused_assignments)]
    let mut aggregated_qc_stats: Option<tsz_solver::QueryCacheStatistics> = None;
    #[allow(unused_assignments)]
    let mut aggregated_ds_stats: Option<tsz_solver::StoreStatistics> = None;

    if cache.is_none() {
        // --- PARALLEL PATH: No cache, check all files concurrently ---
        let _parallel_span =
            tracing::info_span!("parallel_check_files", files = work_queue.len()).entered();

        // Pre-compute merged augmentations once for all files (avoids O(N²) per-file recomputation)
        let merged_augmentations = MergedAugmentations::from_program(program);

        // Pre-compute per-file module bridging (sequential, fast — uses resolved_module_paths)
        let per_file_binders: Vec<BinderState> = {
            let _prep_span = tracing::info_span!("prepare_binders").entered();
            work_queue
                .iter()
                .map(|&file_idx| {
                    let file = &program.files[file_idx];
                    let mut binder = create_binder_from_bound_file_with_augmentations(
                        file,
                        program,
                        file_idx,
                        &merged_augmentations,
                    );

                    // Bridge raw module specifiers to resolved export tables using
                    // the pre-computed resolved_module_paths map (no FS calls needed).
                    // Uses cached specifiers from build_resolved_module_maps.
                    for (specifier, _, _) in &cached_module_specifiers[file_idx] {
                        if let Some(&target_idx) =
                            resolved_module_paths.get(&(file_idx, specifier.clone()))
                        {
                            propagate_module_export_maps(
                                &mut binder,
                                specifier,
                                target_idx,
                                program,
                                &resolved_module_paths,
                            );
                        }
                    }
                    binder
                })
                .collect()
        };

        let work_items: Vec<usize> = work_queue.into_iter().collect();
        let no_check = options.no_check;
        let check_js = options.check_js;
        let explicit_check_js_false = options.explicit_check_js_false;
        let skip_lib_check = options.skip_lib_check;
        let compiler_options = options.checker.clone();
        let shared_lib_cache: Arc<dashmap::DashMap<String, Option<tsz_solver::TypeId>>> =
            Arc::new(dashmap::DashMap::new());

        // Check all files in parallel — each file gets its own CheckerState and QueryCache.
        // TypeInterner (DashMap) is thread-safe; QueryCache uses RefCell/Cell per-thread.
        #[cfg(not(target_arch = "wasm32"))]
        #[allow(clippy::type_complexity)]
        let file_results: Vec<(
            Vec<Diagnostic>,
            Option<TypeCache>,
            RequestCacheCounters,
            tsz_solver::QueryCacheStatistics,
            tsz_solver::StoreStatistics,
        )> = {
            use rayon::iter::{
                IndexedParallelIterator, IntoParallelIterator, IntoParallelRefIterator,
                ParallelIterator,
            };
            if work_items.len() <= 1 {
                work_items
                    .iter()
                    .zip(per_file_binders)
                    .map(|(&file_idx, binder)| {
                        let context = CheckFileForParallelContext {
                            file_idx,
                            binder,
                            program,
                            compiler_options: &compiler_options,
                            project_env: &project_env,
                            resolved_module_specifiers: &resolved_module_specifiers,
                            shared_lib_cache: Arc::clone(&shared_lib_cache),
                            no_check,
                            check_js,
                            explicit_check_js_false,
                            skip_lib_check,
                            program_has_real_syntax_errors,
                        };
                        check_file_for_parallel(context)
                    })
                    .collect()
            } else {
                tsz::parallel::ensure_rayon_global_pool();
                work_items
                    .par_iter()
                    .zip(per_file_binders.into_par_iter())
                    .map(|(&file_idx, binder)| {
                        let context = CheckFileForParallelContext {
                            file_idx,
                            binder,
                            program,
                            compiler_options: &compiler_options,
                            project_env: &project_env,
                            resolved_module_specifiers: &resolved_module_specifiers,
                            shared_lib_cache: Arc::clone(&shared_lib_cache),
                            no_check,
                            check_js,
                            explicit_check_js_false,
                            skip_lib_check,
                            program_has_real_syntax_errors,
                        };
                        check_file_for_parallel(context)
                    })
                    .collect()
            }
        };

        #[cfg(target_arch = "wasm32")]
        let file_results: Vec<(
            Vec<Diagnostic>,
            Option<TypeCache>,
            RequestCacheCounters,
            tsz_solver::QueryCacheStatistics,
            tsz_solver::StoreStatistics,
        )> = work_items
            .iter()
            .zip(per_file_binders.into_iter())
            .map(|(&file_idx, binder)| {
                let context = CheckFileForParallelContext {
                    file_idx,
                    binder,
                    program,
                    compiler_options: &compiler_options,
                    project_env: &project_env,
                    resolved_module_specifiers: &resolved_module_specifiers,
                    shared_lib_cache: Arc::clone(&shared_lib_cache),
                    no_check,
                    check_js,
                    explicit_check_js_false,
                    skip_lib_check,
                    program_has_real_syntax_errors,
                };
                check_file_for_parallel(context)
            })
            .collect();

        // Aggregate per-file query cache and definition store statistics from the parallel path.
        let mut parallel_qc_stats = tsz_solver::QueryCacheStatistics::default();
        let mut parallel_ds_stats = tsz_solver::StoreStatistics::default();
        {
            let mut tc_out = type_cache_output
                .lock()
                .expect("type_cache_output mutex poisoned");
            for (idx, (file_diags, type_cache, file_counters, qc_stats, ds_stats)) in
                file_results.into_iter().enumerate()
            {
                diagnostics.extend(file_diags);
                request_cache_counters.merge(file_counters);
                parallel_qc_stats.merge(&qc_stats);
                parallel_ds_stats.merge(&ds_stats);
                if let Some(tc) = type_cache {
                    let file_path = PathBuf::from(&program.files[work_items[idx]].file_name);
                    tc_out.insert(file_path, tc);
                }
            }
        }
        aggregated_qc_stats = Some(parallel_qc_stats);
        aggregated_ds_stats = Some(parallel_ds_stats);
    } else {
        // --- SEQUENTIAL PATH: Cached build with dependency cascade ---
        let mut sequential_ds_stats = tsz_solver::StoreStatistics::default();

        // Reorder work queue in topological (dependency-first) order so that
        // dependencies are checked before their dependents. This ensures that
        // cached type information and export hashes are available when checking
        // files that import them, improving incremental invalidation accuracy.
        {
            let queue_vec: Vec<usize> = work_queue.iter().copied().collect();
            let ordered = topological_file_order(&queue_vec, &resolved_module_paths);
            work_queue.clear();
            for idx in ordered {
                work_queue.push_back(idx);
            }
        }

        // Process files in the work queue
        while let Some(file_idx) = work_queue.pop_front() {
            let file = &program.files[file_idx];
            let file_path = PathBuf::from(&file.file_name);

            let mut binder = create_binder_from_bound_file(file, program, file_idx);

            // Use cached specifiers from build_resolved_module_maps.
            let module_specifiers = &cached_module_specifiers[file_idx];

            // Bridge multi-file module resolution for ES module imports.
            for (specifier, _, _) in module_specifiers {
                if let Some(resolved) = resolve_module_specifier(
                    Path::new(&file.file_name),
                    specifier,
                    options,
                    base_dir,
                    &mut resolution_cache,
                    &program_paths,
                ) {
                    let canonical = canonicalize_or_owned(&resolved);
                    if let Some(&target_idx) = canonical_to_file_idx.get(&canonical) {
                        propagate_module_export_maps(
                            &mut binder,
                            specifier,
                            target_idx,
                            program,
                            &resolved_module_paths,
                        );
                    }
                }
            }
            let cached = cache
                .as_deref_mut()
                .and_then(|cache| cache.type_caches.remove(&file_path));
            let compiler_options = options.checker.clone();
            let mut checker = if let Some(cached) = cached {
                CheckerState::with_cache(
                    &file.arena,
                    &binder,
                    &query_cache,
                    file.file_name.clone(),
                    cached,
                    compiler_options,
                )
            } else {
                CheckerState::new(
                    &file.arena,
                    &binder,
                    &query_cache,
                    file.file_name.clone(),
                    compiler_options,
                )
            };
            checker.ctx.report_unresolved_imports = true;
            project_env.apply_to(&mut checker.ctx);

            // Per-file state that varies across files:
            checker.ctx.set_current_file_idx(file_idx);
            checker.ctx.file_is_esm = project_env.file_is_esm_map.get(&file.file_name).copied();

            // Build resolved_modules set for backward compatibility
            let mut resolved_modules = rustc_hash::FxHashSet::default();
            for (specifier, _, _) in module_specifiers {
                if resolved_module_specifiers.contains(&(file_idx, specifier.clone()))
                    || resolved_module_paths.contains_key(&(file_idx, specifier.clone()))
                {
                    resolved_modules.insert(specifier.clone());
                } else if !resolved_module_errors.contains_key(&(file_idx, specifier.clone()))
                    && let Some(resolved) = resolve_module_specifier(
                        Path::new(&file.file_name),
                        specifier,
                        options,
                        base_dir,
                        &mut resolution_cache,
                        &program_paths,
                    )
                {
                    let canonical = canonicalize_or_owned(&resolved);
                    if program_paths.contains(&canonical) {
                        resolved_modules.insert(specifier.clone());
                    }
                }
            }
            checker.ctx.resolved_modules = Some(resolved_modules);
            // TSC suppresses many semantic diagnostics across the whole program when any
            // file has a real syntax parse error; mirror that behavior using the program-level
            // flag so that diagnostics like TS1361/TS1362 do not leak from syntax-error files.
            checker.ctx.has_parse_errors = program_has_real_syntax_errors;
            // Exclude codes that are grammar checks in our parser but are NOT in TSC's
            // parseDiagnostics. TSC uses hasParseDiagnostics() to suppress grammar
            // errors like TS1105/TS1108, so we must match exactly which codes count.
            // Excluded:
            //   TS1009 - Trailing comma (checker grammar error in TSC)
            //   TS1014 - Rest parameter must be last (grammar check, AST is valid)
            //   TS1185 - Merge conflict marker (not a real parse failure)
            //   TS1214 - 'yield' reserved word in strict mode (grammar check, not parse failure)
            //   TS1262 - 'await' reserved word at top level (grammar check, not parse failure)
            //   TS1359 - 'await' reserved word in async context (grammar check, not parse failure)
            checker.ctx.has_syntax_parse_errors = file
                .parse_diagnostics
                .iter()
                .any(|d| !is_non_suppressing_parse_error(d.code));
            checker.ctx.syntax_parse_error_positions = file
                .parse_diagnostics
                .iter()
                .filter(|d| !is_non_suppressing_parse_error(d.code))
                .map(|d| d.start)
                .collect();
            checker.ctx.all_parse_error_positions =
                file.parse_diagnostics.iter().map(|d| d.start).collect();
            // Track whether the file has "real" syntax errors (actual parse
            // failures like missing tokens or invalid characters) vs grammar
            // checks (strict mode violations, decorator errors, etc.).
            // Real syntax errors broadly suppress TS2304 to match tsc behavior.
            checker.ctx.has_real_syntax_errors = file
                .parse_diagnostics
                .iter()
                .any(|d| is_real_syntax_error(d.code));
            checker.ctx.real_syntax_error_positions = file
                .parse_diagnostics
                .iter()
                .filter(|d| is_real_syntax_error(d.code))
                .map(|d| d.start)
                .collect();
            let filtered_parse_diagnostics = filtered_parse_diagnostics(&file.parse_diagnostics);
            let is_js = is_js_file(Path::new(&file.file_name));
            let mut file_diagnostics = Vec::new();
            // For JS files, suppress TypeScript-grammar parser diagnostics.
            // tsc's parser is lenient with TypeScript-only syntax in JS files.
            // Keep only parser diagnostics that tsc also emits for JS, and
            // convert TS1162 (optional object member) to TS8009 for methods.
            if is_js {
                let source_text = file
                    .arena
                    .get_source_file_at(file.source_file)
                    .map(|sf| sf.text.as_ref());
                // First, convert specific codes (e.g. TS1162 -> TS8009)
                convert_js_parse_diagnostics_to_ts8xxx(
                    &file.parse_diagnostics,
                    &file.file_name,
                    &mut file_diagnostics,
                    source_text,
                );
                // Then, keep allowed parser diagnostics from the filtered list.
                for parse_diagnostic in &filtered_parse_diagnostics {
                    if is_ts1xxx_allowed_in_js(parse_diagnostic.code) {
                        file_diagnostics.push(parse_diagnostic_to_checker(
                            &file.file_name,
                            parse_diagnostic,
                        ));
                    }
                }
            } else {
                for parse_diagnostic in filtered_parse_diagnostics {
                    file_diagnostics.push(parse_diagnostic_to_checker(
                        &file.file_name,
                        parse_diagnostic,
                    ));
                }
            }
            // skipLibCheck: skip type checking of declaration files (.d.ts, .d.cts, .d.mts)
            if options.skip_lib_check && is_declaration_file(&file.file_name) {
                diagnostics.extend(file_diagnostics);
                request_cache_counters.merge(checker.ctx.request_cache_counters);
                continue;
            }

            // Note: We always run checking for all files (JS and TS).
            // TypeScript reports syntax/semantic errors like TS1210 (strict mode violations)
            // even for JS files without checkJs. Only type-level errors are gated by checkJs.
            if !options.no_check {
                let _check_span =
                    tracing::info_span!("check_file", file = %file.file_name).entered();
                tsz::checker::reset_stack_overflow_flag();
                checker.check_source_file(file.source_file);

                let mut checker_diagnostics = std::mem::take(&mut checker.ctx.diagnostics);
                post_process_checker_diagnostics(
                    &mut checker_diagnostics,
                    file,
                    options,
                    program_has_real_syntax_errors,
                );

                file_diagnostics.extend(checker_diagnostics);
            }

            // Final JS-specific filter: remove any remaining grammar codes that
            // tsc doesn't emit for JS files. Both the parser and checker can
            // produce these codes; this catch-all ensures they don't leak through.
            if is_js {
                file_diagnostics.retain(|d| !is_checker_grammar_code_suppressed_in_js(d.code));
            }

            // Apply @ts-expect-error / @ts-ignore directive suppression.
            // tsc suppresses all diagnostics on the line following such directives
            // and emits TS2578 for unused @ts-expect-error directives.
            if let Some(source) = file.arena.get_source_file_at(file.source_file) {
                apply_ts_directive_suppression(
                    &file.file_name,
                    source.text.as_ref(),
                    &mut file_diagnostics,
                );
            }

            // Update the cache and check for export signature changes.
            // Uses the unified binder-level ExportSignature (shared with LSP)
            // so body-only/comment-only/private-symbol edits produce the same
            // invalidation decisions in both CLI and LSP.
            let checker_counters = checker.ctx.request_cache_counters;
            sequential_ds_stats.merge(&checker.ctx.definition_store.statistics());

            if let Some(c) = cache.as_deref_mut() {
                let new_sig = compute_export_signature(program, file, file_idx);
                let new_hash = new_sig.0;
                let old_hash = c.export_hashes.get(&file_path).copied();

                c.type_caches
                    .insert(file_path.clone(), checker.extract_cache());
                c.diagnostics
                    .insert(file_path.clone(), file_diagnostics.clone());
                c.export_hashes.insert(file_path.clone(), new_hash);

                if old_hash != Some(new_hash)
                    && let Some(dependents) = c.reverse_dependencies.get(&file_path)
                {
                    for dep_path in dependents {
                        if let Some(&dep_idx) = canonical_to_file_idx.get(dep_path)
                            && checked_files.insert(dep_idx)
                        {
                            work_queue.push_back(dep_idx);
                            c.type_caches.remove(dep_path);
                            c.diagnostics.remove(dep_path);
                        }
                    }
                }
            } else {
                diagnostics.extend(file_diagnostics);
            }
            request_cache_counters.merge(checker_counters);
        }
        // Sequential path: single shared QueryCache — capture stats after all files.
        aggregated_qc_stats = Some(query_cache.statistics());
        aggregated_ds_stats = Some(sequential_ds_stats);
    }

    // Collect diagnostics from cache for all files
    // This includes both checked files (now in cache) and unchecked files (cached from previous run)
    if let Some(c) = cache.as_deref() {
        for file in &program.files {
            let file_path = PathBuf::from(&file.file_name);
            if let Some(cached_diags) = c.diagnostics.get(&file_path) {
                diagnostics.extend(cached_diags.clone());
            }
        }
    }

    // Cleanup unused entries from the cache
    if let Some(c) = cache {
        c.type_caches.retain(|path, _| used_paths.contains(path));
        c.diagnostics.retain(|path, _| used_paths.contains(path));
        c.export_hashes.retain(|path, _| used_paths.contains(path));
    }

    diagnostics.extend(detect_missing_tslib_helper_diagnostics(
        program,
        options,
        base_dir,
        &file_is_esm_map,
    ));

    // Use the aggregated query-cache statistics. In the parallel path, these
    // are merged from all per-file caches. In the sequential path, they come
    // from the shared query_cache. Fall back to the top-level query_cache stats
    // if neither path set the aggregated stats (shouldn't happen in practice).
    let query_cache_stats = aggregated_qc_stats.or_else(|| Some(query_cache.statistics()));

    // Compute module dependency graph statistics for --extendedDiagnostics.
    let module_dep_stats = {
        let file_count = program.files.len();
        // Build a deduplicated adjacency list from resolved_module_paths.
        let mut adj: Vec<Vec<usize>> = vec![Vec::new(); file_count];
        let mut edge_count: usize = 0;
        for ((src, _specifier), &tgt) in resolved_module_paths.iter() {
            if *src < file_count && tgt < file_count && !adj[*src].contains(&tgt) {
                adj[*src].push(tgt);
                edge_count += 1;
            }
        }
        let sccs = tarjan_scc(file_count, &adj);
        let cycles: Vec<&Vec<usize>> = sccs.iter().filter(|scc| scc.len() > 1).collect();
        let import_cycles = cycles.len();
        let largest_cycle_size = cycles.iter().map(|c| c.len()).max().unwrap_or(0);
        Some(super::ModuleDependencyStats {
            file_count,
            dependency_edges: edge_count,
            import_cycles,
            largest_cycle_size,
        })
    };

    CollectDiagnosticsResult {
        diagnostics,
        request_cache_counters,
        query_cache_stats,
        def_store_stats: aggregated_ds_stats,
        module_dep_stats,
    }
}

/// Tarjan's algorithm for finding strongly connected components.
///
/// Returns SCCs in reverse topological order. Each SCC is a `Vec<usize>` of node indices.
/// Import cycles correspond to SCCs with more than one node.
fn tarjan_scc(n: usize, adj: &[Vec<usize>]) -> Vec<Vec<usize>> {
    struct State<'a> {
        adj: &'a [Vec<usize>],
        index_counter: usize,
        stack: Vec<usize>,
        on_stack: Vec<bool>,
        indices: Vec<Option<usize>>,
        lowlinks: Vec<usize>,
        result: Vec<Vec<usize>>,
    }

    fn strongconnect(v: usize, state: &mut State<'_>) {
        state.indices[v] = Some(state.index_counter);
        state.lowlinks[v] = state.index_counter;
        state.index_counter += 1;
        state.stack.push(v);
        state.on_stack[v] = true;

        for &w in &state.adj[v] {
            if state.indices[w].is_none() {
                strongconnect(w, state);
                state.lowlinks[v] = state.lowlinks[v].min(state.lowlinks[w]);
            } else if state.on_stack[w] {
                state.lowlinks[v] = state.lowlinks[v].min(state.indices[w].unwrap());
            }
        }

        if state.lowlinks[v] == state.indices[v].unwrap() {
            let mut scc = Vec::new();
            loop {
                let w = state.stack.pop().unwrap();
                state.on_stack[w] = false;
                scc.push(w);
                if w == v {
                    break;
                }
            }
            state.result.push(scc);
        }
    }

    let mut state = State {
        adj,
        index_counter: 0,
        stack: Vec::new(),
        on_stack: vec![false; n],
        indices: vec![None; n],
        lowlinks: vec![0; n],
        result: Vec::new(),
    };

    for v in 0..n {
        if state.indices[v].is_none() {
            strongconnect(v, &mut state);
        }
    }

    state.result
}

fn propagate_module_export_maps(
    binder: &mut BinderState,
    specifier: &str,
    target_idx: usize,
    program: &MergedProgram,
    resolved_module_paths: &FxHashMap<(usize, String), usize>,
) {
    let mut worklist: Vec<(String, usize)> = vec![(specifier.to_owned(), target_idx)];
    let mut seen: rustc_hash::FxHashSet<(String, usize)> = rustc_hash::FxHashSet::default();

    while let Some((current_specifier, current_target_idx)) = worklist.pop() {
        if !seen.insert((current_specifier.clone(), current_target_idx)) {
            continue;
        }

        let target_file_name = &program.files[current_target_idx].file_name;

        if let Some(exports) = program.module_exports.get(target_file_name).cloned() {
            binder
                .module_exports
                .insert(current_specifier.clone(), exports);
        }
        if let Some(wildcards) = program.wildcard_reexports.get(target_file_name).cloned() {
            binder
                .wildcard_reexports
                .insert(current_specifier.clone(), wildcards.clone());
        }
        if let Some(type_only_flags) = program
            .wildcard_reexports_type_only
            .get(target_file_name)
            .cloned()
        {
            binder
                .wildcard_reexports_type_only
                .insert(current_specifier.clone(), type_only_flags);
        }
        if let Some(reexports) = program.reexports.get(target_file_name).cloned() {
            binder
                .reexports
                .insert(current_specifier.clone(), reexports);
        }

        if let Some(source_modules) = program.wildcard_reexports.get(target_file_name).cloned() {
            for source_module in source_modules {
                if let Some(&source_target_idx) =
                    resolved_module_paths.get(&(current_target_idx, source_module.clone()))
                {
                    worklist.push((source_module, source_target_idx));
                }
            }
        }

        // Also follow named re-exports: `export { X } from './other'`
        // Extract unique source modules from the re-export map so the
        // importing file's binder receives transitive exports.
        if let Some(file_reexports) = program.reexports.get(target_file_name).cloned() {
            let mut reexport_sources: rustc_hash::FxHashSet<String> =
                rustc_hash::FxHashSet::default();
            for (source_module, _) in file_reexports.values() {
                reexport_sources.insert(source_module.clone());
            }
            for source_module in reexport_sources {
                if let Some(&source_target_idx) =
                    resolved_module_paths.get(&(current_target_idx, source_module.clone()))
                {
                    worklist.push((source_module, source_target_idx));
                }
            }
        }
    }
}

/// Compute a topological ordering of file indices based on resolved module dependencies.
///
/// Given `resolved_module_paths` mapping `(source_file_idx, specifier) -> target_file_idx`,
/// this produces a dependency-first ordering: files with no dependencies come first,
/// followed by files that depend only on already-listed files.
///
/// If cycles exist, the cycle participants are appended at the end in their original
/// order (matching tsc behavior which gracefully handles circular imports).
///
/// Only file indices present in `file_indices` are included in the output.
fn topological_file_order(
    file_indices: &[usize],
    resolved_module_paths: &FxHashMap<(usize, String), usize>,
) -> Vec<usize> {
    if file_indices.len() <= 1 {
        return file_indices.to_vec();
    }

    // Build adjacency list: src -> [targets it imports].
    // Edge A -> B means "A depends on B" (A imports B).
    let file_set: FxHashSet<usize> = file_indices.iter().copied().collect();
    let mut deps: FxHashMap<usize, Vec<usize>> = FxHashMap::default();
    for &idx in file_indices {
        deps.insert(idx, Vec::new());
    }
    for (&(src, _), &target) in resolved_module_paths.iter() {
        if file_set.contains(&src) && file_set.contains(&target) && src != target {
            deps.entry(src).or_default().push(target);
        }
    }

    // Kahn's algorithm on the dependency graph.
    // We want dependencies first: if A imports B, B should appear before A.
    // in_degree[x] = number of imports x has (edges leaving x in the dep graph).
    // Nodes with in_degree 0 have no imports and can be processed first.
    let mut in_degree: FxHashMap<usize, usize> = FxHashMap::default();
    // reverse_deps[B] = [A, ...] means "A depends on B"
    let mut reverse_deps: FxHashMap<usize, Vec<usize>> = FxHashMap::default();
    for &idx in file_indices {
        in_degree.insert(idx, 0);
        reverse_deps.insert(idx, Vec::new());
    }
    for (&src, dep_list) in &deps {
        for &dep in dep_list {
            if dep != src {
                reverse_deps.entry(dep).or_default().push(src);
                *in_degree.entry(src).or_default() += 1;
            }
        }
    }

    // Seed queue with nodes that have no dependencies, in sorted order for determinism.
    let mut queue: VecDeque<usize> = VecDeque::new();
    let mut sorted_indices: Vec<usize> = file_indices.to_vec();
    sorted_indices.sort_unstable();
    for &idx in &sorted_indices {
        if in_degree[&idx] == 0 {
            queue.push_back(idx);
        }
    }

    let mut result = Vec::with_capacity(file_indices.len());
    while let Some(node) = queue.pop_front() {
        result.push(node);
        if let Some(dependents) = reverse_deps.get(&node) {
            let mut sorted_dependents = dependents.clone();
            sorted_dependents.sort_unstable();
            for &dependent in &sorted_dependents {
                let deg = in_degree.get_mut(&dependent).unwrap();
                *deg -= 1;
                if *deg == 0 {
                    queue.push_back(dependent);
                }
            }
        }
    }

    // If cycles exist, append remaining nodes in their original order.
    if result.len() < file_indices.len() {
        let in_result: FxHashSet<usize> = result.iter().copied().collect();
        for &idx in file_indices {
            if !in_result.contains(&idx) {
                result.push(idx);
            }
        }
    }

    result
}

pub(super) struct CheckFileForParallelContext<'a> {
    file_idx: usize,
    binder: BinderState,
    program: &'a MergedProgram,
    compiler_options: &'a tsz_common::CheckerOptions,
    /// Project-wide shared environment — replaces individual `lib_contexts`, `all_arenas`,
    /// `all_binders`, skeleton indices, `symbol_file_targets`, `resolved_module_paths/errors`,
    /// `is_external_module_by_file`, `file_is_esm_map`, `typescript_dom_replacement_globals`,
    /// and `has_deprecation_diagnostics` fields.
    project_env: &'a tsz::checker::context::ProjectEnv,
    resolved_module_specifiers: &'a Arc<FxHashSet<(usize, String)>>,
    shared_lib_cache: Arc<dashmap::DashMap<String, Option<tsz_solver::TypeId>>>,
    no_check: bool,
    check_js: bool,
    /// `true` when `checkJs: false` was explicitly specified in compiler options.
    /// When set, ALL semantic errors are suppressed for JS files, including the
    /// `plainJSErrors` allowlist that would otherwise survive the filter.
    explicit_check_js_false: bool,
    skip_lib_check: bool,
    program_has_real_syntax_errors: bool,
}

/// Check a single file for the parallel checking path.
///
/// This is extracted from the work queue loop so it can be called from rayon's `par_iter`.
/// Each invocation creates its own `CheckerState` (with its own mutable context)
/// and its own `QueryCache` (using `RefCell`/`Cell` for zero-overhead single-threaded caching).
/// The `TypeInterner` is shared across threads via `DashMap` (thread-safe).
pub(super) fn check_file_for_parallel<'a>(
    context: CheckFileForParallelContext<'a>,
) -> (
    Vec<Diagnostic>,
    Option<TypeCache>,
    RequestCacheCounters,
    tsz_solver::QueryCacheStatistics,
    tsz_solver::StoreStatistics,
) {
    let CheckFileForParallelContext {
        file_idx,
        binder,
        program,
        compiler_options,
        project_env,
        resolved_module_specifiers,
        shared_lib_cache,
        no_check,
        check_js,
        explicit_check_js_false,
        skip_lib_check,
        program_has_real_syntax_errors,
    } = context;
    let file = &program.files[file_idx];
    // skipLibCheck: skip type checking of declaration files (.d.ts, .d.cts, .d.mts)
    if skip_lib_check && is_declaration_file(&file.file_name) {
        return (
            Vec::new(),
            None,
            RequestCacheCounters::default(),
            tsz_solver::QueryCacheStatistics::default(),
            tsz_solver::StoreStatistics::default(),
        );
    }

    // Create a per-thread QueryCache (uses RefCell/Cell, no atomic overhead).
    let query_cache = QueryCache::new(&program.type_interner);

    // Build resolved_modules directly from the pre-computed resolved_module_specifiers
    // set (populated in build_resolved_module_maps). This avoids a redundant
    // collect_module_specifiers AST traversal — the third call per file.
    let resolved_modules: FxHashSet<String> = resolved_module_specifiers
        .iter()
        .filter(|(idx, _)| *idx == file_idx)
        .map(|(_, spec)| spec.clone())
        .collect();

    let mut checker = CheckerState::with_options(
        &file.arena,
        &binder,
        &query_cache,
        file.file_name.clone(),
        compiler_options,
    );
    checker.ctx.report_unresolved_imports = true;
    checker.ctx.shared_lib_type_cache = Some(shared_lib_cache);

    // Apply all project-level shared state in one call.
    project_env.apply_to(&mut checker.ctx);

    // Per-file state that varies across files:
    checker.ctx.set_current_file_idx(file_idx);
    checker.ctx.file_is_esm = project_env.file_is_esm_map.get(&file.file_name).copied();
    checker.ctx.resolved_modules = Some(resolved_modules);
    // TSC suppresses many semantic diagnostics across the whole program when any
    // file has a real syntax parse error; mirror that behavior using the program-level
    // flag so that diagnostics like TS1361/TS1362 do not leak from syntax-error files.
    checker.ctx.has_parse_errors = program_has_real_syntax_errors;
    // Exclude grammar checks that don't affect AST structure from
    // has_syntax_parse_errors so we match TSC's hasParseDiagnostics() behavior.
    //   TS1009 - Trailing comma (checker grammar error in TSC)
    //   TS1014 - Rest parameter must be last (grammar check, AST is valid)
    //   TS1185 - Merge conflict marker (not a real parse failure)
    checker.ctx.has_syntax_parse_errors = file
        .parse_diagnostics
        .iter()
        .any(|d| !is_non_suppressing_parse_error(d.code));
    checker.ctx.syntax_parse_error_positions = file
        .parse_diagnostics
        .iter()
        .filter(|d| !is_non_suppressing_parse_error(d.code))
        .map(|d| d.start)
        .collect();
    checker.ctx.all_parse_error_positions =
        file.parse_diagnostics.iter().map(|d| d.start).collect();
    checker.ctx.has_real_syntax_errors = file
        .parse_diagnostics
        .iter()
        .any(|d| is_real_syntax_error(d.code));
    checker.ctx.real_syntax_error_positions = file
        .parse_diagnostics
        .iter()
        .filter(|d| is_real_syntax_error(d.code))
        .map(|d| d.start)
        .collect();
    let filtered_parse_diagnostics = filtered_parse_diagnostics(&file.parse_diagnostics);
    let is_js = is_js_file(Path::new(&file.file_name));

    // For JS files, suppress parser diagnostics. tsc's parser is lenient
    // with TypeScript-only syntax in JS files (it parses but does not emit
    // errors). The checker emits TS8xxx codes instead. Our parser doesn't
    // distinguish JS vs TS, so we suppress parser diagnostics here.
    // Some parser diagnostics are converted to their TS8xxx equivalents.
    // Use raw (unfiltered) diagnostics for conversion.
    let mut file_diagnostics: Vec<Diagnostic> = if is_js {
        let source_text = file
            .arena
            .get_source_file_at(file.source_file)
            .map(|sf| sf.text.as_ref());
        let mut diags = Vec::new();
        convert_js_parse_diagnostics_to_ts8xxx(
            &file.parse_diagnostics,
            &file.file_name,
            &mut diags,
            source_text,
        );
        for parse_diagnostic in &filtered_parse_diagnostics {
            if is_ts1xxx_allowed_in_js(parse_diagnostic.code) {
                diags.push(parse_diagnostic_to_checker(
                    &file.file_name,
                    parse_diagnostic,
                ));
            }
        }
        diags
    } else {
        filtered_parse_diagnostics
            .into_iter()
            .map(|d| parse_diagnostic_to_checker(&file.file_name, d))
            .collect()
    };

    // Note: We always run checking for all files (JS and TS).
    // TypeScript reports syntax/semantic errors like TS1210 (strict mode violations)
    // even for JS files without checkJs. Only type-level errors are gated by checkJs.
    if !no_check {
        tsz::checker::reset_stack_overflow_flag();
        checker.check_source_file(file.source_file);
        let mut checker_diagnostics = std::mem::take(&mut checker.ctx.diagnostics);
        let effective_options = ResolvedCompilerOptions {
            check_js,
            explicit_check_js_false,
            ..ResolvedCompilerOptions::default()
        };
        post_process_checker_diagnostics(
            &mut checker_diagnostics,
            file,
            &effective_options,
            program_has_real_syntax_errors,
        );

        file_diagnostics.extend(checker_diagnostics);
    }

    // Final JS-specific filter: remove any remaining grammar codes that
    // tsc doesn't emit for JS files.
    if is_js {
        file_diagnostics.retain(|d| !is_checker_grammar_code_suppressed_in_js(d.code));
    }

    // Apply @ts-expect-error / @ts-ignore directive suppression.
    if let Some(source) = file.arena.get_source_file_at(file.source_file) {
        apply_ts_directive_suppression(
            &file.file_name,
            source.text.as_ref(),
            &mut file_diagnostics,
        );
    }

    let checker_counters = checker.ctx.request_cache_counters;
    let qc_stats = query_cache.statistics();
    let ds_stats = checker.ctx.definition_store.statistics();
    let type_cache = checker.extract_cache();
    (
        file_diagnostics,
        Some(type_cache),
        checker_counters,
        qc_stats,
        ds_stats,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::args::CliArgs;
    use std::path::PathBuf;
    use tsz_common::common::ModuleKind;

    fn collect_test_diagnostics(files: &[(&str, &str)]) -> Vec<Diagnostic> {
        let bind_results: Vec<_> = files
            .iter()
            .map(|(file_name, source)| {
                parallel::parse_and_bind_single((*file_name).to_string(), (*source).to_string())
            })
            .collect();
        let program = parallel::merge_bind_results(bind_results);
        let type_cache_output = std::sync::Mutex::new(FxHashMap::default());

        collect_diagnostics(
            &program,
            &ResolvedCompilerOptions::default(),
            std::path::Path::new("/"),
            None,
            &[],
            (false, false, false),
            &type_cache_output,
            false,
        )
        .diagnostics
    }

    fn default_cli_args_for_test() -> CliArgs {
        clap::Parser::try_parse_from(["tsz"]).expect("default args should parse")
    }

    fn resolved_options_for_es2015_strict_test() -> ResolvedCompilerOptions {
        let mut args = default_cli_args_for_test();
        args.ignore_config = true;
        args.strict = true;
        args.target = Some(crate::args::Target::Es2015);

        let mut resolved = crate::config::resolve_compiler_options(None)
            .expect("resolve default compiler options");
        crate::driver::apply_cli_overrides(&mut resolved, &args).expect("apply cli overrides");
        if matches!(resolved.printer.module, ModuleKind::None) {
            resolved.printer.module = ModuleKind::ES2015;
            resolved.checker.module = ModuleKind::ES2015;
        }
        resolved
    }

    #[test]
    fn test_compile_inner_program_build_promise_is_assignable_to_promise_like() {
        let dir = tempfile::TempDir::new().expect("temp dir");
        let file_path = dir.path().join("main.ts");
        std::fs::write(
            &file_path,
            r#"
declare const p: Promise<number>;
const q: PromiseLike<number> = p;
"#,
        )
        .expect("write source");

        let resolved = resolved_options_for_es2015_strict_test();
        let file_paths = vec![file_path];
        let SourceReadResult {
            sources,
            dependencies: _,
            type_reference_errors,
            resolution_mode_errors,
        } = super::read_source_files(&file_paths, dir.path(), &resolved, None, None)
            .expect("read source files");

        assert!(type_reference_errors.is_empty());
        assert!(resolution_mode_errors.is_empty());

        let disable_default_libs =
            resolved.lib_is_default && super::sources_have_no_default_lib(&sources);
        let lib_paths = super::resolve_effective_lib_paths(
            &resolved,
            &sources,
            dir.path(),
            disable_default_libs,
        )
        .expect("resolve effective lib paths");
        let lib_path_refs: Vec<_> = lib_paths.iter().map(PathBuf::as_path).collect();
        let lib_files =
            parallel::load_lib_files_for_binding_strict(&lib_path_refs).expect("load strict libs");
        let lib_contexts = load_lib_files_for_contexts(&lib_files);
        let compile_inputs: Vec<_> = sources
            .into_iter()
            .map(|source| {
                (
                    source.path.to_string_lossy().into_owned(),
                    source.text.unwrap_or_default(),
                )
            })
            .collect();
        let program = parallel::merge_bind_results(parallel::parse_and_bind_parallel_with_libs(
            compile_inputs,
            &lib_files,
        ));
        let type_cache_output = std::sync::Mutex::new(FxHashMap::default());

        let diagnostics = collect_diagnostics(
            &program,
            &resolved,
            dir.path(),
            None,
            &lib_contexts,
            (false, false, false),
            &type_cache_output,
            false,
        )
        .diagnostics;

        assert!(
            diagnostics.is_empty(),
            "Expected compile-inner program build Promise<T> -> PromiseLike<T> assignability, got: {diagnostics:?}"
        );
    }

    #[test]
    #[ignore = "pre-existing: remote merge regression"]
    fn test_collect_diagnostics_preserves_invariant_generic_error_elaboration_ts2322() {
        let dir = tempfile::TempDir::new().expect("temp dir");
        let file_path = dir.path().join("main.ts");
        std::fs::write(
            &file_path,
            r#"// Repro from #19746

const wat: Runtype<any> = Num;
const Foo = Obj({ foo: Num })

interface Runtype<A> {
  constraint: Constraint<this>
  witness: A
}

interface Num extends Runtype<number> {
  tag: 'number'
}
declare const Num: Num

interface Obj<O extends { [_ in string]: Runtype<any> }> extends Runtype<{[K in keyof O]: O[K]['witness'] }> {}
declare function Obj<O extends { [_: string]: Runtype<any> }>(fields: O): Obj<O>;

interface Constraint<A extends Runtype<any>> extends Runtype<A['witness']> {
  underlying: A,
  check: (x: A['witness']) => void,
}
"#,
        )
        .expect("write source");

        let resolved = resolved_options_for_es2015_strict_test();
        let file_paths = vec![file_path];
        let SourceReadResult {
            sources,
            dependencies: _,
            type_reference_errors,
            resolution_mode_errors,
        } = super::read_source_files(&file_paths, dir.path(), &resolved, None, None)
            .expect("read source files");

        assert!(type_reference_errors.is_empty());
        assert!(resolution_mode_errors.is_empty());

        let disable_default_libs =
            resolved.lib_is_default && super::sources_have_no_default_lib(&sources);
        let lib_paths = super::resolve_effective_lib_paths(
            &resolved,
            &sources,
            dir.path(),
            disable_default_libs,
        )
        .expect("resolve effective lib paths");
        let lib_path_refs: Vec<_> = lib_paths.iter().map(PathBuf::as_path).collect();
        let lib_files =
            parallel::load_lib_files_for_binding_strict(&lib_path_refs).expect("load strict libs");
        let lib_contexts = load_lib_files_for_contexts(&lib_files);
        let direct_lib_contexts: Vec<LibContext> = lib_files
            .iter()
            .map(|lib| LibContext {
                arena: Arc::clone(&lib.arena),
                binder: Arc::clone(&lib.binder),
            })
            .collect();
        let compile_inputs: Vec<_> = sources
            .into_iter()
            .map(|source| {
                (
                    source.path.to_string_lossy().into_owned(),
                    source.text.unwrap_or_default(),
                )
            })
            .collect();
        let program = parallel::merge_bind_results(parallel::parse_and_bind_parallel_with_libs(
            compile_inputs,
            &lib_files,
        ));
        let parallel_result =
            parallel::check_files_parallel(&program, &resolved.checker, &lib_files);
        let _parallel_ts2322_count = parallel_result
            .file_results
            .iter()
            .flat_map(|file| file.diagnostics.iter())
            .filter(|diag| diag.code == diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE)
            .count();
        let rebuilt_binder = create_binder_from_bound_file(&program.files[0], &program, 0);
        program
            .type_interner
            .set_no_unchecked_indexed_access(resolved.checker.no_unchecked_indexed_access);
        let query_cache = tsz_solver::QueryCache::new(&program.type_interner);
        let mut checker = CheckerState::with_options(
            &program.files[0].arena,
            &rebuilt_binder,
            &query_cache,
            program.files[0].file_name.clone(),
            &resolved.checker,
        );
        checker.ctx.set_lib_contexts(direct_lib_contexts.clone());
        checker
            .ctx
            .set_actual_lib_file_count(direct_lib_contexts.len());
        let all_arenas = Arc::new(
            program
                .files
                .iter()
                .map(|file| Arc::clone(&file.arena))
                .collect::<Vec<_>>(),
        );
        let all_binders = Arc::new(vec![Arc::new(create_binder_from_bound_file(
            &program.files[0],
            &program,
            0,
        ))]);
        checker.ctx.set_all_arenas(Arc::clone(&all_arenas));
        if let Some(ref skel) = program.skeleton_index {
            let (exact, patterns) = skel.build_declared_module_sets();
            checker.ctx.set_declared_modules_from_skeleton(Arc::new(
                tsz::checker::context::GlobalDeclaredModules::from_skeleton(exact, patterns),
            ));
            checker
                .ctx
                .set_expando_index_from_skeleton(Arc::new(skel.expando_properties.clone()));
        }
        checker.ctx.set_all_binders(Arc::clone(&all_binders));
        checker.ctx.set_current_file_idx(0);
        checker.check_source_file(program.files[0].source_file);

        let source_file = program.files[0]
            .arena
            .get(program.files[0].source_file)
            .and_then(|node| program.files[0].arena.get_source_file(node))
            .expect("missing source file");
        let var_stmt_idx = *source_file
            .statements
            .nodes
            .first()
            .expect("variable statement");
        let var_stmt_node = program.files[0]
            .arena
            .get(var_stmt_idx)
            .expect("var stmt node");
        let var_stmt_data = program.files[0]
            .arena
            .get_variable(var_stmt_node)
            .expect("var stmt data");
        let decl_list_idx = *var_stmt_data
            .declarations
            .nodes
            .first()
            .expect("declaration list");
        let decl_list_node = program.files[0]
            .arena
            .get(decl_list_idx)
            .expect("decl list node");
        let decl_list_data = program.files[0]
            .arena
            .get_variable(decl_list_node)
            .expect("decl list data");
        let decl_idx = *decl_list_data
            .declarations
            .nodes
            .first()
            .expect("declaration");
        let decl_node = program.files[0].arena.get(decl_idx).expect("decl node");
        let decl = program.files[0]
            .arena
            .get_variable_declaration(decl_node)
            .expect("decl data");
        let _source_type = checker.get_type_of_node(decl.initializer);
        let target_type = checker.get_type_from_type_node(decl.type_annotation);
        let _read_constraint_type =
            |object_type| match tsz_solver::QueryDatabase::resolve_property_access(
                &query_cache,
                object_type,
                "constraint",
            ) {
                tsz_solver::operations::property::PropertyAccessResult::Success {
                    type_id, ..
                } => Some(type_id),
                _ => None,
            };
        let _evaluated_target_type = {
            let mut evaluator =
                tsz_solver::TypeEvaluator::with_resolver(&program.type_interner, &checker.ctx);
            evaluator.evaluate(target_type)
        };
        let type_cache_output = std::sync::Mutex::new(FxHashMap::default());
        let direct_diagnostics = collect_diagnostics(
            &program,
            &resolved,
            dir.path(),
            None,
            &direct_lib_contexts,
            (false, false, false),
            &type_cache_output,
            false,
        )
        .diagnostics;
        let direct_ts2322_count = direct_diagnostics
            .iter()
            .filter(|diag| diag.code == diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE)
            .count();
        assert_eq!(
            direct_ts2322_count, 2,
            "Expected collect_diagnostics with direct lib contexts to preserve two TS2322 diagnostics, got: {direct_diagnostics:?}"
        );

        let diagnostics = collect_diagnostics(
            &program,
            &resolved,
            dir.path(),
            None,
            &lib_contexts,
            (false, false, false),
            &type_cache_output,
            false,
        )
        .diagnostics;

        let ts2322_count = diagnostics
            .iter()
            .filter(|diag| diag.code == diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE)
            .count();
        assert_eq!(
            ts2322_count, 2,
            "Expected compile-inner collect_diagnostics to preserve two TS2322 diagnostics, got: {diagnostics:?}"
        );
    }

    #[test]
    fn test_is_declaration_file() {
        assert!(is_declaration_file("types.d.ts"));
        assert!(is_declaration_file("index.d.mts"));
        assert!(is_declaration_file("index.d.cts"));
        assert!(is_declaration_file("/path/to/file.d.ts"));
        assert!(is_declaration_file("/path/to/file.d.mts"));
        assert!(is_declaration_file("/path/to/file.d.cts"));

        assert!(!is_declaration_file("index.ts"));
        assert!(!is_declaration_file("index.mts"));
        assert!(!is_declaration_file("index.cts"));
        assert!(!is_declaration_file("index.js"));
    }

    #[test]
    fn test_transitive_module_export_bridge_infers_type_only_flags() {
        let a_file = parallel::parse_and_bind_single(
            "/a.ts".to_string(),
            "export class A {}\nexport class B {}".to_string(),
        );
        let b_file = parallel::parse_and_bind_single(
            "/b.ts".to_string(),
            "export type * from \"./a\";".to_string(),
        );
        let c_file = parallel::parse_and_bind_single(
            "/c.ts".to_string(),
            "export * from \"./b\";".to_string(),
        );
        let d_file = parallel::parse_and_bind_single(
            "/d.ts".to_string(),
            r#"import { A, B } from "./c";
let _: A = new A();
let __: B = new B();"#
                .to_string(),
        );

        let program = parallel::merge_bind_results(vec![a_file, b_file, c_file, d_file]);
        let d_idx = 3;
        let d_bound = &program.files[d_idx];
        let mut binder = create_binder_from_bound_file(d_bound, &program, d_idx);

        let mut resolved_module_paths: FxHashMap<(usize, String), usize> = FxHashMap::default();
        resolved_module_paths.insert((d_idx, "./c".to_string()), 2);
        resolved_module_paths.insert((2, "./b".to_string()), 1);
        resolved_module_paths.insert((1, "./a".to_string()), 0);

        let module_specifiers = collect_module_specifiers(&d_bound.arena, d_bound.source_file);
        for (specifier, _, _) in &module_specifiers {
            if let Some(&target_idx) = resolved_module_paths.get(&(d_idx, specifier.clone())) {
                propagate_module_export_maps(
                    &mut binder,
                    specifier,
                    target_idx,
                    &program,
                    &resolved_module_paths,
                );
            }
        }

        assert!(binder.wildcard_reexports.contains_key("./c"));
        let c_wildcards = binder
            .wildcard_reexports
            .get("./c")
            .expect("expected wildcard re-exports for ./c");
        assert_eq!(c_wildcards, &vec!["./b".to_string()]);

        let b_wildcards = binder
            .wildcard_reexports
            .get("./b")
            .expect("expected wildcard re-exports for ./b");
        assert_eq!(b_wildcards, &vec!["./a".to_string()]);

        let b_type_only = binder
            .wildcard_reexports_type_only
            .get("./b")
            .expect("expected type-only metadata for ./b");
        assert!(
            b_type_only
                .iter()
                .any(|(source, is_type_only)| source == "./a" && *is_type_only)
        );

        let exports_via_c = binder
            .resolve_import_with_reexports_type_only("./c", "A")
            .expect("expected A to resolve via wildcard chain");
        assert!(exports_via_c.1, "A should be considered type-only via ./b");
    }

    #[test]
    fn test_collect_diagnostics_suppresses_ts2307_for_local_ambient_module() {
        let diagnostics = collect_test_diagnostics(&[
            (
                "/project/demo.d.ts",
                r#"
declare namespace demoNS {
    function f(): void;
}
declare module "demoModule" {
    import alias = demoNS;
    export = alias;
}
"#,
            ),
            (
                "/project/user.ts",
                r#"
import { f } from "demoModule";
let x1: string = demoNS.f;
let x2: string = f;
"#,
            ),
        ]);

        let codes = diagnostics.iter().map(|d| d.code).collect::<Vec<_>>();
        assert!(
            !codes.contains(&2307),
            "Did not expect TS2307 when a local ambient module declaration matches the import. Diagnostics: {diagnostics:?}"
        );
        assert_eq!(
            codes.iter().filter(|&&code| code == 2322).count(),
            2,
            "Expected the import to still resolve and produce two downstream TS2322 diagnostics. Diagnostics: {diagnostics:?}"
        );
    }

    #[test]
    fn real_syntax_errors_suppress_cross_file_type_diagnostics() {
        let diagnostics = collect_test_diagnostics(&[
            ("/a.ts", "const x =\n"),
            ("/b.ts", "const y: number = \"s\";\n"),
        ]);

        assert!(
            diagnostics
                .iter()
                .any(|diag| diag.file == "/a.ts" && diag.code == 1109),
            "expected the real syntax error to remain: {diagnostics:?}"
        );
        assert!(
            !diagnostics
                .iter()
                .any(|diag| diag.file == "/b.ts" && diag.code == 2322),
            "did not expect TS2322 when another file has a real syntax error: {diagnostics:?}"
        );
    }

    #[test]
    fn real_syntax_errors_preserve_checker_grammar_diagnostics() {
        let diagnostics = collect_test_diagnostics(&[
            ("/a.ts", "const x =\n"),
            ("/b.ts", "type void = string;\n"),
        ]);

        assert!(
            diagnostics
                .iter()
                .any(|diag| diag.file == "/b.ts" && diag.code == 2457),
            "expected TS2457 to survive program-level syntax suppression: {diagnostics:?}"
        );
    }

    #[test]
    fn tarjan_scc_no_edges() {
        let adj = vec![vec![], vec![], vec![]];
        let sccs = tarjan_scc(3, &adj);
        // Each node is its own SCC
        assert_eq!(sccs.len(), 3);
        for scc in &sccs {
            assert_eq!(scc.len(), 1);
        }
    }

    #[test]
    fn tarjan_scc_linear_chain() {
        // 0 -> 1 -> 2 (no cycles)
        let adj = vec![vec![1], vec![2], vec![]];
        let sccs = tarjan_scc(3, &adj);
        assert_eq!(sccs.len(), 3);
        for scc in &sccs {
            assert_eq!(scc.len(), 1);
        }
    }

    #[test]
    fn tarjan_scc_simple_cycle() {
        // 0 -> 1 -> 0 (one cycle of size 2)
        let adj = vec![vec![1], vec![0]];
        let sccs = tarjan_scc(2, &adj);
        assert_eq!(sccs.len(), 1);
        assert_eq!(sccs[0].len(), 2);
    }

    #[test]
    fn tarjan_scc_triangle_cycle() {
        // 0 -> 1 -> 2 -> 0 (one cycle of size 3)
        let adj = vec![vec![1], vec![2], vec![0]];
        let sccs = tarjan_scc(3, &adj);
        assert_eq!(sccs.len(), 1);
        assert_eq!(sccs[0].len(), 3);
    }

    #[test]
    fn tarjan_scc_mixed() {
        // 0 -> 1 -> 2 -> 1 (cycle {1,2}), 3 standalone
        let adj = vec![vec![1], vec![2], vec![1], vec![]];
        let sccs = tarjan_scc(4, &adj);
        let cycles: Vec<_> = sccs.iter().filter(|s| s.len() > 1).collect();
        assert_eq!(cycles.len(), 1, "expected exactly one cycle");
        assert_eq!(cycles[0].len(), 2, "cycle should have 2 nodes");
    }

    #[test]
    fn real_syntax_errors_preserve_reserved_interface_name_diagnostics() {
        let diagnostics = collect_test_diagnostics(&[
            ("/a.ts", "const x =\n"),
            ("/b.ts", "function function() {}\ninterface void {}\n"),
        ]);

        assert!(
            diagnostics
                .iter()
                .any(|diag| diag.file == "/b.ts" && diag.code == 2427),
            "expected TS2427 to survive parse-error suppression: {diagnostics:?}"
        );
    }

    // --- topological_file_order tests ---

    #[test]
    fn topo_order_empty() {
        let result = topological_file_order(&[], &FxHashMap::default());
        assert!(result.is_empty());
    }

    #[test]
    fn topo_order_single_file() {
        let result = topological_file_order(&[0], &FxHashMap::default());
        assert_eq!(result, vec![0]);
    }

    #[test]
    fn topo_order_no_deps() {
        // Three files with no dependencies — output should be sorted by index
        let result = topological_file_order(&[2, 0, 1], &FxHashMap::default());
        assert_eq!(result, vec![0, 1, 2]);
    }

    #[test]
    fn topo_order_linear_chain() {
        // File 0 imports file 1, file 1 imports file 2
        // Expected order: 2 (no deps), then 1, then 0
        let mut deps = FxHashMap::default();
        deps.insert((0, "./b".to_string()), 1);
        deps.insert((1, "./c".to_string()), 2);

        let result = topological_file_order(&[0, 1, 2], &deps);
        assert_eq!(result, vec![2, 1, 0]);
    }

    #[test]
    fn topo_order_diamond() {
        // File 0 imports 1 and 2; both 1 and 2 import 3
        // Expected: 3 first, then 1 and 2 (sorted), then 0
        let mut deps = FxHashMap::default();
        deps.insert((0, "./a".to_string()), 1);
        deps.insert((0, "./b".to_string()), 2);
        deps.insert((1, "./c".to_string()), 3);
        deps.insert((2, "./c".to_string()), 3);

        let result = topological_file_order(&[0, 1, 2, 3], &deps);
        assert_eq!(result, vec![3, 1, 2, 0]);
    }

    #[test]
    fn topo_order_cycle() {
        // Circular: 0 -> 1 -> 0
        // Both participate in a cycle; should still include both files
        let mut deps = FxHashMap::default();
        deps.insert((0, "./b".to_string()), 1);
        deps.insert((1, "./a".to_string()), 0);

        let result = topological_file_order(&[0, 1], &deps);
        assert_eq!(result.len(), 2);
        assert!(result.contains(&0));
        assert!(result.contains(&1));
    }

    #[test]
    fn topo_order_partial_cycle() {
        // File 2 has no deps; files 0 and 1 form a cycle
        // Expected: 2 first (no deps), then 0, 1 (cycle participants appended)
        let mut deps = FxHashMap::default();
        deps.insert((0, "./b".to_string()), 1);
        deps.insert((1, "./a".to_string()), 0);

        let result = topological_file_order(&[0, 1, 2], &deps);
        assert_eq!(result[0], 2, "dependency-free file should come first");
        assert_eq!(result.len(), 3);
    }

    #[test]
    fn topo_order_ignores_external_deps() {
        // File 0 depends on file 5, but 5 is not in file_indices — should be ignored
        let mut deps = FxHashMap::default();
        deps.insert((0, "./ext".to_string()), 5);

        let result = topological_file_order(&[0, 1], &deps);
        assert_eq!(result.len(), 2);
        // Both have no in-set dependencies, so sorted order
        assert_eq!(result, vec![0, 1]);
    }

    #[test]
    fn topo_order_self_import_ignored() {
        // File 0 imports itself — self-loops should be ignored
        let mut deps = FxHashMap::default();
        deps.insert((0, "./self".to_string()), 0);

        let result = topological_file_order(&[0, 1], &deps);
        assert_eq!(result, vec![0, 1]);
    }
}

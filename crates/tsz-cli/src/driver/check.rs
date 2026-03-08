//! Diagnostics collection and per-file checking orchestration for the compilation driver.

use super::check_utils::*;
use super::*;
use std::time::{Duration, Instant};

/// Check if a filename is a TypeScript declaration file (.d.ts, .d.cts, .d.mts).
fn is_declaration_file(name: &str) -> bool {
    name.ends_with(".d.ts") || name.ends_with(".d.mts") || name.ends_with(".d.cts")
}

/// Load lib.d.ts files and create `LibContext` objects for the checker.
///
/// This function reuses already-loaded lib files from the binding phase, avoiding a second
/// parse/bind pass during checker setup.
pub(super) fn load_lib_files_for_contexts(lib_files: &[Arc<LibFile>]) -> Vec<LibContext> {
    if lib_files.is_empty() {
        return Vec::new();
    }

    let lib_contexts: Vec<LibContext> = lib_files
        .iter()
        .map(|lib| LibContext {
            arena: Arc::clone(&lib.arena),
            binder: Arc::clone(&lib.binder),
        })
        .collect();

    // Merge all lib binders into a single binder to avoid duplicate SymbolIds
    // (e.g., "Intl" declarations across multiple lib files).
    use tsz::binder::state::LibContext as BinderLibContext;
    let mut merged_binder = tsz::binder::BinderState::new();
    let binder_lib_contexts: Vec<_> = lib_contexts
        .iter()
        .map(|ctx| BinderLibContext {
            arena: Arc::clone(&ctx.arena),
            binder: Arc::clone(&ctx.binder),
        })
        .collect();
    merged_binder.merge_lib_contexts_into_binder(&binder_lib_contexts);

    vec![LibContext {
        // Keep a lib arena available for declaration lookups.
        arena: Arc::clone(&lib_contexts[0].arena),
        binder: Arc::new(merged_binder),
    }]
}

pub(super) fn collect_diagnostics(
    program: &MergedProgram,
    options: &ResolvedCompilerOptions,
    base_dir: &Path,
    cache: Option<&mut CompilationCache>,
    lib_contexts: &[LibContext],
    type_cache_output: &std::sync::Mutex<FxHashMap<PathBuf, TypeCache>>,
    has_deprecation_diagnostics: bool,
    extended_progress_enabled: bool,
) -> Vec<Diagnostic> {
    let _collect_span =
        tracing::info_span!("collect_diagnostics", files = program.files.len()).entered();
    let report_progress = |phase: &'static str, start: Instant| {
        if extended_progress_enabled {
            eprintln!(
                "{}",
                format_extended_diagnostics_collect_progress(phase, start.elapsed())
            );
        }
    };
    let mut diagnostics: Vec<Diagnostic> = Vec::new();
    let mut used_paths = FxHashSet::default();
    let mut cache = cache;
    let mut resolution_cache = ModuleResolutionCache::default();
    let mut program_paths = FxHashSet::default();
    let mut canonical_to_file_name: FxHashMap<PathBuf, String> = FxHashMap::default();
    let mut canonical_to_file_idx: FxHashMap<PathBuf, usize> = FxHashMap::default();

    {
        let phase_start = Instant::now();
        let _span = tracing::info_span!("build_program_path_maps").entered();
        for (idx, file) in program.files.iter().enumerate() {
            let canonical = canonicalize_or_owned(Path::new(&file.file_name));
            program_paths.insert(canonical.clone());
            canonical_to_file_name.insert(canonical.clone(), file.file_name.clone());
            canonical_to_file_idx.insert(canonical, idx);
        }
        report_progress("build_program_path_maps", phase_start);
    }

    // Extract is_external_module from BoundFile to preserve state across file bindings
    // This fixes TS2664 which requires accurate per-file is_external_module values
    let is_external_module_by_file: Arc<rustc_hash::FxHashMap<String, bool>> = Arc::new(
        program
            .files
            .iter()
            .map(|file| (file.file_name.clone(), file.is_external_module))
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

    {
        let phase_start = Instant::now();
        let _span = tracing::info_span!("build_resolved_module_maps").entered();
        for (file_idx, file) in program.files.iter().enumerate() {
            let module_specifiers = collect_module_specifiers(&file.arena, file.source_file);
            let file_path = Path::new(&file.file_name);

            for (specifier, specifier_node, import_kind) in &module_specifiers {
                // Get span from the specifier node
                let span = if let Some(spec_node) = file.arena.get(*specifier_node) {
                    Span::new(spec_node.pos, spec_node.end)
                } else {
                    Span::new(0, 0) // Fallback for invalid nodes
                };

                // Always try ModuleResolver first to get specific error types (TS2834/TS2835/TS2792)
                match module_resolver.resolve_with_kind(specifier, file_path, span, *import_kind) {
                    Ok(resolved_module) => {
                        resolved_module_specifiers.insert((file_idx, specifier.clone()));
                        let canonical = canonicalize_or_owned(&resolved_module.resolved_path);
                        if let Some(&target_idx) = canonical_to_file_idx.get(&canonical) {
                            resolved_module_paths.insert((file_idx, specifier.clone()), target_idx);
                        }
                    }
                    Err(failure) => {
                        let mut resolved_override: Option<PathBuf> = None;
                        if let tsz::module_resolver::ResolutionFailure::JsxNotEnabled {
                            resolved_path,
                            ..
                        } = &failure
                        {
                            resolved_override = Some(resolved_path.clone());
                        }
                        // Check if the fallback resolver can find this module.
                        // tsc tries all resolution strategies before giving up.
                        // Our ModuleResolver may fail with ModuleResolutionModeMismatch,
                        // PackageJsonError, etc. even though a simpler resolution
                        // strategy would succeed. Try the fallback for these cases too.
                        if failure.should_try_fallback()
                            && let Some(resolved) = resolve_module_specifier(
                                file_path,
                                specifier,
                                options,
                                base_dir,
                                &mut resolution_cache,
                                &program_paths,
                            )
                        {
                            if std::env::var_os("TSZ_DEBUG_RESOLVE").is_some() {
                                tracing::debug!(
                                    "module specifier fallback success: file={} spec={} -> {}",
                                    file_path.display(),
                                    specifier,
                                    resolved.display()
                                );
                            }
                            // Validate Node16/NodeNext extension requirements for virtual files
                            let resolution_kind = options.effective_module_resolution();
                            let is_node16_or_next = matches!(
                                resolution_kind,
                                crate::config::ModuleResolutionKind::Node16
                                    | crate::config::ModuleResolutionKind::NodeNext
                            );

                            if is_node16_or_next {
                                // Check if importing file is ESM (by extension or path)
                                let file_path_str = file_path.to_string_lossy();
                                let importing_ext =
                                    tsz::module_resolver::ModuleExtension::from_path(file_path);
                                let is_esm = importing_ext.forces_esm()
                                    || file_path_str.ends_with(".mts")
                                    || file_path_str.ends_with(".mjs");

                                // Check if specifier has an extension
                                let specifier_has_extension =
                                    Path::new(specifier).extension().is_some();

                                // In Node16/NodeNext ESM mode, relative imports must have explicit extensions
                                // If the import is extensionless, TypeScript treats it as "cannot find module" (TS2307)
                                // even though the file exists, because ESM requires explicit extensions
                                if is_esm && !specifier_has_extension && specifier.starts_with('.')
                                {
                                    // Emit TS2307 error - module cannot be found with the exact specifier
                                    // (even though the file exists, ESM requires explicit extension)
                                    resolved_module_errors.insert(
                                            (file_idx, specifier.clone()),
                                            tsz::checker::context::ResolutionError {
                                                code: tsz::module_resolver::CANNOT_FIND_MODULE,
                                                message: format!(
                                                    "Cannot find module '{specifier}' or its corresponding type declarations."
                                                ),
                                            },
                                        );
                                    continue; // Don't add to resolved_modules - this is an error
                                }
                            }

                            // Fallback succeeded and passed validation - add to resolved paths
                            let canonical = canonicalize_or_owned(&resolved);
                            if let Some(&target_idx) = canonical_to_file_idx.get(&canonical) {
                                resolved_module_paths
                                    .insert((file_idx, specifier.clone()), target_idx);
                            }
                            continue; // Virtual file found and validated, skip error
                        }

                        // Check if this is a JSON module import without resolveJsonModule enabled
                        // If so, emit TS2732 instead of TS2307
                        let failure_to_use = if matches!(
                            failure,
                            tsz::module_resolver::ResolutionFailure::NotFound { .. }
                        ) && specifier.ends_with(".json")
                            && !options.resolve_json_module
                        {
                            // Create TS2732 error for JSON import without resolveJsonModule
                            tsz::module_resolver::ResolutionFailure::JsonModuleWithoutResolveJsonModule {
                                specifier: specifier.clone(),
                                containing_file: file_path.to_string_lossy().to_string(),
                                span,
                            }
                        } else {
                            failure
                        };

                        if std::env::var_os("TSZ_DEBUG_RESOLVE").is_some() {
                            tracing::debug!(
                                "module specifier resolution failed: file={} spec={} failure={:?}",
                                file_path.display(),
                                specifier,
                                failure_to_use
                            );
                        }

                        // Untyped JS module handling: When resolution fails but a JS
                        // file exists for this specifier (without declaration files),
                        // TypeScript treats it as an untyped module:
                        // - With noImplicitAny: emit TS7016 ("Could not find a declaration file")
                        // - Without noImplicitAny: silently treat as `any` (no error)
                        if matches!(
                            failure_to_use,
                            tsz::module_resolver::ResolutionFailure::NotFound { .. }
                                | tsz::module_resolver::ResolutionFailure::PackageJsonError { .. }
                        ) && let Some(js_path) =
                            module_resolver.probe_js_file(specifier, file_path, span, *import_kind)
                        {
                            if options.checker.no_implicit_any {
                                resolved_module_errors.insert(
                                        (file_idx, specifier.clone()),
                                        tsz::checker::context::ResolutionError {
                                            code: 7016,
                                            message: format!(
                                                "Could not find a declaration file for module '{}'. '{}' implicitly has an 'any' type.",
                                                specifier, js_path.display()
                                            ),
                                        },
                                    );
                            }
                            // Mark the module as "resolved" so the checker doesn't
                            // independently emit TS2307. The module is treated as
                            // untyped (type `any`) — no target file index needed.
                            resolved_module_specifiers.insert((file_idx, specifier.clone()));
                            continue;
                        }

                        // Convert ResolutionFailure to Diagnostic to get the error code and message
                        let mut diagnostic = failure_to_use.to_diagnostic();

                        // TypeScript emits TS2792 (instead of TS2307) when module resolution
                        // is Classic. The `implied_classic_resolution` flag is computed from
                        // `effective_module_resolution()` at config resolution time.
                        // tsc emits TS2792 regardless of whether the specifier has a file
                        // extension — the decision is purely based on the resolution mode.
                        if diagnostic.code == tsz::module_resolver::CANNOT_FIND_MODULE
                            && options.checker.implied_classic_resolution
                        {
                            diagnostic.code = tsz::module_resolver::MODULE_RESOLUTION_MODE_MISMATCH;
                            diagnostic.message = format!(
                                "Cannot find module '{specifier}'. Did you mean to set the 'moduleResolution' option to 'nodenext', or to add aliases to the 'paths' option?"
                            );
                        }

                        resolved_module_errors.insert(
                            (file_idx, specifier.clone()),
                            tsz::checker::context::ResolutionError {
                                code: diagnostic.code,
                                message: diagnostic.message,
                            },
                        );

                        if resolved_override.is_some() {
                            // Mark as resolved to suppress TS2307, but don't map
                            // to a target file. For JsxNotEnabled, the resolved
                            // file shouldn't have its exports validated (which
                            // would cause spurious TS1192/TS2306 errors).
                            resolved_module_specifiers.insert((file_idx, specifier.clone()));
                        }
                    }
                }
            }
        }
        report_progress("build_resolved_module_maps", phase_start);
    }

    if options.no_check {
        let phase_start = Instant::now();
        diagnostics.extend(collect_no_check_file_diagnostics(
            program,
            &resolved_module_errors,
        ));
        report_progress("collect_no_check_file_diagnostics", phase_start);
        diagnostics.extend(detect_missing_tslib_helper_diagnostics(
            program, options, base_dir,
        ));

        for file in &program.files {
            used_paths.insert(PathBuf::from(&file.file_name));
        }
        if let Some(c) = cache {
            c.type_caches.retain(|path, _| used_paths.contains(path));
            c.diagnostics.retain(|path, _| used_paths.contains(path));
            c.export_hashes.retain(|path, _| used_paths.contains(path));
        }

        return diagnostics;
    }

    // Pre-create all binders for cross-file resolution
    let all_binders: Arc<Vec<Arc<BinderState>>> = Arc::new({
        let phase_start = Instant::now();
        let _span = tracing::info_span!("build_cross_file_binders").entered();
        let binders = program
            .files
            .iter()
            .enumerate()
            .map(|(file_idx, file)| {
                Arc::new(create_cross_file_lookup_binder(file, program, file_idx))
            })
            .collect();
        report_progress("build_cross_file_binders", phase_start);
        binders
    });

    // Collect all arenas for cross-file resolution
    let all_arenas: Arc<Vec<Arc<NodeArena>>> = Arc::new({
        let phase_start = Instant::now();
        let _span = tracing::info_span!("collect_all_arenas").entered();
        let arenas = program
            .files
            .iter()
            .map(|file| Arc::clone(&file.arena))
            .collect();
        report_progress("collect_all_arenas", phase_start);
        arenas
    });

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

    // Create a shared QueryCache for memoized evaluate_type/is_subtype_of calls.
    let query_cache = QueryCache::new(&program.type_interner);

    // Prime Array<T> base type with global augmentations before any file checks.
    // CRITICAL: The prime checker and all file checkers MUST share the same DefinitionStore.
    if !program.files.is_empty() && !lib_contexts.is_empty() {
        let phase_start = Instant::now();
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
        checker.ctx.set_lib_contexts(lib_contexts.to_vec());
        checker.ctx.set_actual_lib_file_count(lib_contexts.len());
        checker.ctx.set_all_arenas(Arc::clone(&all_arenas));
        checker.ctx.set_all_binders(Arc::clone(&all_binders));
        checker.prime_boxed_types();
        report_progress("prime_boxed_types", phase_start);
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

    if cache.is_none() {
        // --- PARALLEL PATH: No cache, check all files concurrently ---
        let _parallel_span =
            tracing::info_span!("parallel_check_files", files = work_queue.len()).entered();

        // Pre-compute per-file module bridging (sequential, fast — uses resolved_module_paths)
        let per_file_binders: Vec<BinderState> = {
            let phase_start = Instant::now();
            let _prep_span = tracing::info_span!("prepare_binders").entered();
            let binders = work_queue
                .iter()
                .map(|&file_idx| {
                    let file = &program.files[file_idx];
                    let mut binder = create_binder_from_bound_file(file, program, file_idx);
                    let module_specifiers =
                        collect_module_specifiers(&file.arena, file.source_file);

                    // Bridge raw module specifiers to resolved export tables using
                    // the pre-computed resolved_module_paths map (no FS calls needed).
                    for (specifier, _, _) in &module_specifiers {
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
                .collect();
            report_progress("prepare_binders", phase_start);
            binders
        };

        let work_items: Vec<usize> = work_queue.into_iter().collect();
        let no_check = options.no_check;
        let check_js = options.check_js;
        let explicit_check_js_false = options.explicit_check_js_false;
        let skip_lib_check = options.skip_lib_check;
        let compiler_options = options.checker.clone();
        let lib_ctx_for_parallel = lib_contexts.to_vec();
        let shared_lib_cache: Arc<dashmap::DashMap<String, Option<tsz_solver::TypeId>>> =
            Arc::new(dashmap::DashMap::new());

        // Check all files in parallel — each file gets its own CheckerState.
        // TypeInterner (DashMap) and QueryCache (RwLock) are already thread-safe.
        #[cfg(not(target_arch = "wasm32"))]
        let file_results: Vec<(Vec<Diagnostic>, Option<TypeCache>)> = {
            let phase_start = Instant::now();
            use rayon::iter::{
                IndexedParallelIterator, IntoParallelIterator, IntoParallelRefIterator,
                ParallelIterator,
            };
            let results = if work_items.len() <= 1 {
                work_items
                    .iter()
                    .zip(per_file_binders)
                    .map(|(&file_idx, binder)| {
                        let context = CheckFileForParallelContext {
                            file_idx,
                            binder,
                            program,
                            query_cache: &query_cache,
                            compiler_options: &compiler_options,
                            lib_contexts: &lib_ctx_for_parallel,
                            all_arenas: &all_arenas,
                            all_binders: &all_binders,
                            resolved_module_paths: &resolved_module_paths,
                            resolved_module_specifiers: &resolved_module_specifiers,
                            resolved_module_errors: &resolved_module_errors,
                            is_external_module_by_file: &is_external_module_by_file,
                            file_is_esm_map: &file_is_esm_map,
                            shared_lib_cache: Arc::clone(&shared_lib_cache),
                            no_check,
                            check_js,
                            explicit_check_js_false,
                            skip_lib_check,
                            has_deprecation_diagnostics,
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
                            query_cache: &query_cache,
                            compiler_options: &compiler_options,
                            lib_contexts: &lib_ctx_for_parallel,
                            all_arenas: &all_arenas,
                            all_binders: &all_binders,
                            resolved_module_paths: &resolved_module_paths,
                            resolved_module_specifiers: &resolved_module_specifiers,
                            resolved_module_errors: &resolved_module_errors,
                            is_external_module_by_file: &is_external_module_by_file,
                            file_is_esm_map: &file_is_esm_map,
                            shared_lib_cache: Arc::clone(&shared_lib_cache),
                            no_check,
                            check_js,
                            explicit_check_js_false,
                            skip_lib_check,
                            has_deprecation_diagnostics,
                        };
                        check_file_for_parallel(context)
                    })
                    .collect()
            };
            report_progress("parallel_check_files", phase_start);
            results
        };

        #[cfg(target_arch = "wasm32")]
        let file_results: Vec<(Vec<Diagnostic>, Option<TypeCache>)> = {
            let phase_start = Instant::now();
            let results = work_items
                .iter()
                .zip(per_file_binders.into_iter())
                .map(|(&file_idx, binder)| {
                    let context = CheckFileForParallelContext {
                        file_idx,
                        binder,
                        program,
                        query_cache: &query_cache,
                        compiler_options: &compiler_options,
                        lib_contexts: &lib_ctx_for_parallel,
                        all_arenas: &all_arenas,
                        all_binders: &all_binders,
                        resolved_module_paths: &resolved_module_paths,
                        resolved_module_specifiers: &resolved_module_specifiers,
                        resolved_module_errors: &resolved_module_errors,
                        is_external_module_by_file: &is_external_module_by_file,
                        file_is_esm_map: &file_is_esm_map,
                        shared_lib_cache: Arc::clone(&shared_lib_cache),
                        no_check,
                        check_js,
                        explicit_check_js_false,
                        skip_lib_check,
                        has_deprecation_diagnostics,
                    };
                    check_file_for_parallel(context)
                })
                .collect();
            report_progress("parallel_check_files", phase_start);
            results
        };

        {
            let mut tc_out = type_cache_output.lock().unwrap();
            for (idx, (file_diags, type_cache)) in file_results.into_iter().enumerate() {
                diagnostics.extend(file_diags);
                if let Some(tc) = type_cache {
                    let file_path = PathBuf::from(&program.files[work_items[idx]].file_name);
                    tc_out.insert(file_path, tc);
                }
            }
        }
    } else {
        // --- SEQUENTIAL PATH: Cached build with dependency cascade ---

        // Process files in the work queue
        while let Some(file_idx) = work_queue.pop_front() {
            let file = &program.files[file_idx];
            let file_path = PathBuf::from(&file.file_name);

            let mut binder = create_binder_from_bound_file(file, program, file_idx);
            let module_specifiers = collect_module_specifiers(&file.arena, file.source_file);

            // Bridge multi-file module resolution for ES module imports.
            for (specifier, _, _) in &module_specifiers {
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
            if !lib_contexts.is_empty() {
                checker.ctx.set_lib_contexts(lib_contexts.to_vec());
                checker.ctx.set_actual_lib_file_count(lib_contexts.len());
            }
            checker.ctx.set_all_arenas(Arc::clone(&all_arenas));
            checker.ctx.set_all_binders(Arc::clone(&all_binders));
            checker
                .ctx
                .set_resolved_module_paths(Arc::clone(&resolved_module_paths));
            checker
                .ctx
                .set_resolved_module_errors(Arc::clone(&resolved_module_errors));
            checker.ctx.set_current_file_idx(file_idx);
            checker.ctx.is_external_module_by_file = Some(Arc::clone(&is_external_module_by_file));
            checker.ctx.file_is_esm = file_is_esm_map.get(&file.file_name).copied();
            checker.ctx.file_is_esm_map = Some(Arc::clone(&file_is_esm_map));

            // Build resolved_modules set for backward compatibility
            let mut resolved_modules = rustc_hash::FxHashSet::default();
            for (specifier, _, _) in &module_specifiers {
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
            checker.ctx.has_parse_errors = !file.parse_diagnostics.is_empty();
            // TS1009 (Trailing comma not allowed) is emitted by our parser but is a
            // checker grammar error in TSC (not in parseDiagnostics). Exclude it from
            // has_syntax_parse_errors so we match TSC's hasParseDiagnostics() behavior,
            // which is used to suppress TS1108/TS1105 grammar errors.
            checker.ctx.has_syntax_parse_errors = file
                .parse_diagnostics
                .iter()
                .any(|d| d.code != 1185 && d.code != 1009);
            checker.ctx.syntax_parse_error_positions = file
                .parse_diagnostics
                .iter()
                .filter(|d| d.code != 1185 && d.code != 1009)
                .map(|d| d.start)
                .collect();
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
            let mut file_diagnostics = Vec::new();
            for parse_diagnostic in &file.parse_diagnostics {
                file_diagnostics.push(parse_diagnostic_to_checker(
                    &file.file_name,
                    parse_diagnostic,
                ));
            }
            // skipLibCheck: skip type checking of declaration files (.d.ts, .d.cts, .d.mts)
            if options.skip_lib_check && is_declaration_file(&file.file_name) {
                diagnostics.extend(file_diagnostics);
                continue;
            }

            // Note: We always run checking for all files (JS and TS).
            // TypeScript reports syntax/semantic errors like TS1210 (strict mode violations)
            // even for JS files without checkJs. Only type-level errors are gated by checkJs.
            if !options.no_check {
                let _check_span =
                    tracing::info_span!("check_file", file = %file.file_name).entered();
                checker.check_source_file(file.source_file);

                // Filter diagnostics for JS files without checkJs
                let is_js = is_js_file(Path::new(&file.file_name));
                let has_ts_check_pragma = js_file_has_ts_check_pragma(file);
                let has_ts_nocheck_pragma = js_file_has_ts_nocheck_pragma(file);
                let should_filter_type_errors =
                    is_js && (has_ts_nocheck_pragma || (!options.check_js && !has_ts_check_pragma));
                let mut checker_diagnostics = std::mem::take(&mut checker.ctx.diagnostics);

                if should_filter_type_errors {
                    // Keep syntax/semantic diagnostics (< 2000) and JS grammar diagnostics
                    // (TS8xxx). When `checkJs` is NOT explicitly false (the default
                    // no-checkJs mode), also allow the `plainJSErrors` codes that tsc
                    // surfaces even in unchecked JS files. When `checkJs: false` is
                    // explicitly set, suppress ALL semantic errors — tsc treats that as
                    // a full opt-out.
                    checker_diagnostics.retain(|diag| {
                        diag.code < 2000
                            || (8000..9000).contains(&diag.code)
                            || (!options.explicit_check_js_false
                                && is_plain_js_allowed_code(diag.code))
                    });
                }

                // Suppress semantic errors that cascade from structural parse failures.
                // tsc sets per-node ThisNodeHasError flags and skips semantic checks on
                // error-recovery subtrees. We approximate this by suppressing semantic
                // diagnostics that are near a structural parse error (within a distance
                // window). Only structural parse failures (missing tokens, unexpected
                // tokens) trigger suppression — grammar checks like trailing commas or
                // strict mode violations don't cause AST malformation and shouldn't
                // suppress semantic errors.
                {
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
                            // TS2457: "Type alias name cannot be 'void'" — TSC emits
                            // this alongside TS1109 for `type void = ...`.
                            if diag.code == 2457 {
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

                file_diagnostics.extend(checker_diagnostics);
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

            // Update the cache and check for export hash changes
            if let Some(c) = cache.as_deref_mut() {
                let new_hash = compute_export_hash(program, file, file_idx, &mut checker);
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
        }
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
        program, options, base_dir,
    ));

    diagnostics
}

fn format_extended_diagnostics_collect_progress(phase: &str, elapsed: Duration) -> String {
    format!(
        "[extendedDiagnostics] collect_diagnostics::{phase}: {:.2}ms",
        elapsed.as_secs_f64() * 1000.0
    )
}

fn collect_no_check_file_diagnostics(
    program: &MergedProgram,
    resolved_module_errors: &FxHashMap<(usize, String), tsz::checker::context::ResolutionError>,
) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();

    for (file_idx, file) in program.files.iter().enumerate() {
        diagnostics.extend(
            file.parse_diagnostics
                .iter()
                .map(|d| parse_diagnostic_to_checker(&file.file_name, d)),
        );

        for (specifier, specifier_node, _) in
            collect_module_specifiers(&file.arena, file.source_file)
        {
            let Some(error) = resolved_module_errors.get(&(file_idx, specifier.clone())) else {
                continue;
            };
            let (start, length) = file
                .arena
                .get(specifier_node)
                .map(|node| (node.pos, node.end.saturating_sub(node.pos)))
                .unwrap_or((0, 0));
            diagnostics.push(Diagnostic::error(
                file.file_name.clone(),
                start,
                length,
                error.message.clone(),
                error.code,
            ));
        }
    }

    diagnostics
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

pub(super) struct CheckFileForParallelContext<'a> {
    file_idx: usize,
    binder: BinderState,
    program: &'a MergedProgram,
    query_cache: &'a QueryCache<'a>,
    compiler_options: &'a tsz_common::CheckerOptions,
    lib_contexts: &'a [LibContext],
    all_arenas: &'a Arc<Vec<Arc<tsz::parser::node::NodeArena>>>,
    all_binders: &'a Arc<Vec<Arc<BinderState>>>,
    resolved_module_paths: &'a Arc<FxHashMap<(usize, String), usize>>,
    resolved_module_specifiers: &'a Arc<FxHashSet<(usize, String)>>,
    resolved_module_errors:
        &'a Arc<FxHashMap<(usize, String), tsz::checker::context::ResolutionError>>,
    is_external_module_by_file: &'a Arc<FxHashMap<String, bool>>,
    file_is_esm_map: &'a Arc<FxHashMap<String, bool>>,
    shared_lib_cache: Arc<dashmap::DashMap<String, Option<tsz_solver::TypeId>>>,
    no_check: bool,
    check_js: bool,
    /// `true` when `checkJs: false` was explicitly specified in compiler options.
    /// When set, ALL semantic errors are suppressed for JS files, including the
    /// `plainJSErrors` allowlist that would otherwise survive the filter.
    explicit_check_js_false: bool,
    skip_lib_check: bool,
    /// When true, skip lib type resolution in the checker (TS5107/TS5101 mode).
    has_deprecation_diagnostics: bool,
}

/// Check a single file for the parallel checking path.
///
/// This is extracted from the work queue loop so it can be called from rayon's `par_iter`.
/// Each invocation creates its own `CheckerState` (with its own mutable context),
/// while sharing thread-safe structures (`TypeInterner` via `DashMap`, `QueryCache` via `RwLock`).
pub(super) fn check_file_for_parallel<'a>(
    context: CheckFileForParallelContext<'a>,
) -> (Vec<Diagnostic>, Option<TypeCache>) {
    let CheckFileForParallelContext {
        file_idx,
        binder,
        program,
        query_cache,
        compiler_options,
        lib_contexts,
        all_arenas,
        all_binders,
        resolved_module_paths,
        resolved_module_specifiers,
        resolved_module_errors,
        is_external_module_by_file,
        file_is_esm_map,
        shared_lib_cache,
        no_check,
        check_js,
        explicit_check_js_false,
        skip_lib_check,
        has_deprecation_diagnostics,
    } = context;
    let file = &program.files[file_idx];

    // skipLibCheck: skip type checking of declaration files (.d.ts, .d.cts, .d.mts)
    if skip_lib_check && is_declaration_file(&file.file_name) {
        return (Vec::new(), None);
    }

    let module_specifiers = collect_module_specifiers(&file.arena, file.source_file);

    // Build resolved_modules from the pre-computed resolved module maps
    let resolved_modules: FxHashSet<String> = module_specifiers
        .iter()
        .filter(|(specifier, _, _)| {
            resolved_module_paths.contains_key(&(file_idx, specifier.clone()))
                || resolved_module_specifiers.contains(&(file_idx, specifier.clone()))
        })
        .map(|(specifier, _, _)| specifier.clone())
        .collect();

    let mut checker = CheckerState::with_options(
        &file.arena,
        &binder,
        query_cache,
        file.file_name.clone(),
        compiler_options,
    );
    checker.ctx.report_unresolved_imports = true;
    checker.ctx.shared_lib_type_cache = Some(shared_lib_cache);
    checker.ctx.skip_lib_type_resolution = has_deprecation_diagnostics;

    if !lib_contexts.is_empty() {
        checker.ctx.set_lib_contexts(lib_contexts.to_vec());
        checker.ctx.set_actual_lib_file_count(lib_contexts.len());
    }

    checker.ctx.set_all_arenas(Arc::clone(all_arenas));
    checker.ctx.set_all_binders(Arc::clone(all_binders));
    checker
        .ctx
        .set_resolved_module_paths(Arc::clone(resolved_module_paths));
    checker
        .ctx
        .set_resolved_module_errors(Arc::clone(resolved_module_errors));
    checker.ctx.set_current_file_idx(file_idx);
    checker.ctx.is_external_module_by_file = Some(Arc::clone(is_external_module_by_file));
    checker.ctx.file_is_esm = file_is_esm_map.get(&file.file_name).copied();
    checker.ctx.file_is_esm_map = Some(Arc::clone(file_is_esm_map));
    checker.ctx.resolved_modules = Some(resolved_modules);
    checker.ctx.has_parse_errors = !file.parse_diagnostics.is_empty();
    // TS1009 (Trailing comma not allowed) is emitted by our parser but is a
    // checker grammar error in TSC (not in parseDiagnostics). Exclude it from
    // has_syntax_parse_errors so we match TSC's hasParseDiagnostics() behavior.
    checker.ctx.has_syntax_parse_errors = file
        .parse_diagnostics
        .iter()
        .any(|d| d.code != 1185 && d.code != 1009);
    checker.ctx.syntax_parse_error_positions = file
        .parse_diagnostics
        .iter()
        .filter(|d| d.code != 1185 && d.code != 1009)
        .map(|d| d.start)
        .collect();
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

    // Collect parse diagnostics
    let mut file_diagnostics: Vec<Diagnostic> = file
        .parse_diagnostics
        .iter()
        .map(|d| parse_diagnostic_to_checker(&file.file_name, d))
        .collect();

    // Note: We always run checking for all files (JS and TS).
    // TypeScript reports syntax/semantic errors like TS1210 (strict mode violations)
    // even for JS files without checkJs. Only type-level errors are gated by checkJs.
    if !no_check {
        checker.check_source_file(file.source_file);

        // Filter diagnostics for JS files without checkJs
        let is_js = is_js_file(Path::new(&file.file_name));
        let has_ts_check_pragma = js_file_has_ts_check_pragma(file);
        let has_ts_nocheck_pragma = js_file_has_ts_nocheck_pragma(file);
        let should_filter_type_errors =
            is_js && (has_ts_nocheck_pragma || (!check_js && !has_ts_check_pragma));
        let mut checker_diagnostics = std::mem::take(&mut checker.ctx.diagnostics);

        if should_filter_type_errors {
            // Keep syntax/semantic diagnostics (< 2000) and JS grammar diagnostics
            // (TS8xxx). When `checkJs` is NOT explicitly false (the default no-checkJs
            // mode), also allow the `plainJSErrors` codes that tsc surfaces even in
            // unchecked JS files. When `checkJs: false` is explicitly set, suppress
            // ALL semantic errors — tsc treats that as a full opt-out.
            checker_diagnostics.retain(|diag| {
                diag.code < 2000
                    || (8000..9000).contains(&diag.code)
                    || (!explicit_check_js_false && is_plain_js_allowed_code(diag.code))
            });
        }

        // Suppress semantic errors that cascade from structural parse failures.
        // (See multi-file path above for detailed rationale.)
        {
            let structural_error_positions: Vec<u32> = file
                .parse_diagnostics
                .iter()
                .filter(|d| is_structural_parse_error(d.code))
                .map(|d| d.start)
                .collect();
            if !structural_error_positions.is_empty() {
                const MAX_CASCADE_DISTANCE: u32 = 300;
                checker_diagnostics.retain(|diag| {
                    if diag.code < 2000 || (8000..9000).contains(&diag.code) {
                        return true;
                    }
                    // Some semantic errors are deliberately emitted alongside
                    // structural parse errors and must not be suppressed.
                    // TS2457: "Type alias name cannot be 'void'" — TSC emits
                    // this alongside TS1109 for `type void = ...`.
                    if diag.code == 2457 {
                        return true;
                    }
                    !structural_error_positions.iter().any(|&err_pos| {
                        let dist = diag.start.abs_diff(err_pos);
                        dist <= MAX_CASCADE_DISTANCE
                    })
                });
            }
        }

        file_diagnostics.extend(checker_diagnostics);
    }

    // Apply @ts-expect-error / @ts-ignore directive suppression.
    if let Some(source) = file.arena.get_source_file_at(file.source_file) {
        apply_ts_directive_suppression(
            &file.file_name,
            source.text.as_ref(),
            &mut file_diagnostics,
        );
    }

    let type_cache = checker.extract_cache();
    (file_diagnostics, Some(type_cache))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

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

        assert!(binder.wildcard_reexports.get("./c").is_some());
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
    fn test_format_extended_diagnostics_collect_progress() {
        let line = format_extended_diagnostics_collect_progress(
            "build_resolved_module_maps",
            Duration::from_millis(875),
        );
        assert_eq!(
            line,
            "[extendedDiagnostics] collect_diagnostics::build_resolved_module_maps: 875.00ms"
        );
    }
}

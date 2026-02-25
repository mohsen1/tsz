//! Diagnostics collection and per-file checking orchestration for the compilation driver.

use super::driver_check_utils::*;
use super::*;

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
) -> Vec<Diagnostic> {
    let _collect_span =
        tracing::info_span!("collect_diagnostics", files = program.files.len()).entered();
    let mut diagnostics: Vec<Diagnostic> = Vec::new();
    let mut used_paths = FxHashSet::default();
    let mut cache = cache;
    let mut resolution_cache = ModuleResolutionCache::default();
    let mut program_paths = FxHashSet::default();
    let mut canonical_to_file_name: FxHashMap<PathBuf, String> = FxHashMap::default();
    let mut canonical_to_file_idx: FxHashMap<PathBuf, usize> = FxHashMap::default();

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
                        // Check if this is NotFound and the old
                        // resolver would find it (virtual test files). In that case,
                        // validate Node16 rules before accepting the fallback.
                        if failure.is_not_found()
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
                        let has_explicit_extension =
                            std::path::Path::new(specifier).extension().is_some();
                        if diagnostic.code == tsz::module_resolver::CANNOT_FIND_MODULE
                            && !has_explicit_extension
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
    }

    let resolved_module_paths = Arc::new(resolved_module_paths);
    let resolved_module_specifiers = Arc::new(resolved_module_specifiers);
    let resolved_module_errors = Arc::new(resolved_module_errors);

    // Create a shared QueryCache for memoized evaluate_type/is_subtype_of calls.
    let query_cache = QueryCache::new(&program.type_interner);

    // Prime Array<T> base type with global augmentations before any file checks.
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
        checker.ctx.set_lib_contexts(lib_contexts.to_vec());
        checker.ctx.set_actual_lib_file_count(lib_contexts.len());
        checker.ctx.set_all_arenas(Arc::clone(&all_arenas));
        checker.ctx.set_all_binders(Arc::clone(&all_binders));
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

    if cache.is_none() {
        // --- PARALLEL PATH: No cache, check all files concurrently ---
        let _parallel_span =
            tracing::info_span!("parallel_check_files", files = work_queue.len()).entered();

        // Pre-compute per-file module bridging (sequential, fast — uses resolved_module_paths)
        let per_file_binders: Vec<BinderState> = {
            let _prep_span = tracing::info_span!("prepare_binders").entered();
            work_queue
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
                            let target_file_name = &program.files[target_idx].file_name;
                            if let Some(exports) =
                                binder.module_exports.get(target_file_name).cloned()
                            {
                                binder.module_exports.insert(specifier.clone(), exports);
                            }
                            if let Some(wildcards) =
                                binder.wildcard_reexports.get(target_file_name).cloned()
                            {
                                binder
                                    .wildcard_reexports
                                    .insert(specifier.clone(), wildcards);
                            }
                            if let Some(reexports) = binder.reexports.get(target_file_name).cloned()
                            {
                                binder.reexports.insert(specifier.clone(), reexports);
                            }
                            if let Some(source_modules) =
                                binder.wildcard_reexports.get(target_file_name).cloned()
                            {
                                for source_module in source_modules {
                                    if let Some(&source_idx) = resolved_module_paths
                                        .get(&(target_idx, source_module.clone()))
                                    {
                                        let source_file_name = &program.files[source_idx].file_name;
                                        if let Some(exports) =
                                            binder.module_exports.get(source_file_name).cloned()
                                        {
                                            binder
                                                .module_exports
                                                .insert(source_module.clone(), exports);
                                        }
                                    }
                                }
                            }
                        }
                    }
                    binder
                })
                .collect()
        };

        let work_items: Vec<usize> = work_queue.into_iter().collect();
        let no_check = options.no_check;
        let check_js = options.check_js;
        let skip_lib_check = options.skip_lib_check;
        let compiler_options = options.checker.clone();
        let lib_ctx_for_parallel = lib_contexts.to_vec();

        // Check all files in parallel — each file gets its own CheckerState.
        // TypeInterner (DashMap) and QueryCache (RwLock) are already thread-safe.
        #[cfg(not(target_arch = "wasm32"))]
        let file_results: Vec<Vec<Diagnostic>> = {
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
                            query_cache: &query_cache,
                            compiler_options: &compiler_options,
                            lib_contexts: &lib_ctx_for_parallel,
                            all_arenas: &all_arenas,
                            all_binders: &all_binders,
                            resolved_module_paths: &resolved_module_paths,
                            resolved_module_specifiers: &resolved_module_specifiers,
                            resolved_module_errors: &resolved_module_errors,
                            is_external_module_by_file: &is_external_module_by_file,
                            no_check,
                            check_js,
                            skip_lib_check,
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
                            no_check,
                            check_js,
                            skip_lib_check,
                        };
                        check_file_for_parallel(context)
                    })
                    .collect()
            }
        };

        #[cfg(target_arch = "wasm32")]
        let file_results: Vec<Vec<Diagnostic>> = work_items
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
                    no_check,
                    check_js,
                    skip_lib_check,
                };
                check_file_for_parallel(context)
            })
            .collect();

        for file_diags in file_results {
            diagnostics.extend(file_diags);
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
                    if let Some(target_file_name) = canonical_to_file_name.get(&canonical)
                        && let Some(exports) = binder.module_exports.get(target_file_name).cloned()
                    {
                        binder.module_exports.insert(specifier.clone(), exports);
                        if let Some(wildcards) =
                            binder.wildcard_reexports.get(target_file_name).cloned()
                        {
                            binder
                                .wildcard_reexports
                                .insert(specifier.clone(), wildcards);
                        }
                        if let Some(reexports) = binder.reexports.get(target_file_name).cloned() {
                            binder.reexports.insert(specifier.clone(), reexports);
                        }
                        if let Some(&target_idx) = canonical_to_file_idx.get(&canonical)
                            && let Some(source_modules) =
                                binder.wildcard_reexports.get(target_file_name).cloned()
                        {
                            for source_module in source_modules {
                                if let Some(&source_idx) =
                                    resolved_module_paths.get(&(target_idx, source_module.clone()))
                                {
                                    let source_file_name = &program.files[source_idx].file_name;
                                    if let Some(exports) =
                                        binder.module_exports.get(source_file_name).cloned()
                                    {
                                        binder
                                            .module_exports
                                            .insert(source_module.clone(), exports);
                                    }
                                }
                            }
                        }
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
            let mut file_diagnostics = Vec::new();
            for parse_diagnostic in &file.parse_diagnostics {
                file_diagnostics.push(parse_diagnostic_to_checker(
                    &file.file_name,
                    parse_diagnostic,
                ));
            }
            // skipLibCheck: skip type checking of declaration files (.d.ts)
            if options.skip_lib_check && file.file_name.ends_with(".d.ts") {
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
                    // Keep syntax/semantic diagnostics and JS grammar diagnostics (TS8xxx).
                    checker_diagnostics
                        .retain(|diag| diag.code < 2000 || (8000..9000).contains(&diag.code));
                }

                file_diagnostics.extend(checker_diagnostics);
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

    diagnostics.extend(detect_missing_tslib_helper_diagnostics(program, options));

    diagnostics
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
    no_check: bool,
    check_js: bool,
    skip_lib_check: bool,
}

/// Check a single file for the parallel checking path.
///
/// This is extracted from the work queue loop so it can be called from rayon's `par_iter`.
/// Each invocation creates its own `CheckerState` (with its own mutable context),
/// while sharing thread-safe structures (`TypeInterner` via `DashMap`, `QueryCache` via `RwLock`).
pub(super) fn check_file_for_parallel<'a>(
    context: CheckFileForParallelContext<'a>,
) -> Vec<Diagnostic> {
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
        no_check,
        check_js,
        skip_lib_check,
    } = context;
    let file = &program.files[file_idx];

    // skipLibCheck: skip type checking of declaration files (.d.ts)
    if skip_lib_check && file.file_name.ends_with(".d.ts") {
        return Vec::new();
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
            // Keep syntax/semantic diagnostics and JS grammar diagnostics (TS8xxx).
            checker_diagnostics
                .retain(|diag| diag.code < 2000 || (8000..9000).contains(&diag.code));
        }

        file_diagnostics.extend(checker_diagnostics);
    }

    file_diagnostics
}

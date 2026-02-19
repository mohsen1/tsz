//! Diagnostics collection, per-file checking, export hash computation,
//! and binder construction helpers for the compilation driver.

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

                        // TypeScript emits TS2792 (instead of TS2307) for certain module kinds.
                        // These are "classic-like" module systems (AMD, System, UMD) and ES modules.
                        use tsz_common::common::ModuleKind;
                        let module_kind_prefers_2792 = matches!(
                            options.checker.module,
                            ModuleKind::System
                                | ModuleKind::AMD
                                | ModuleKind::UMD
                                | ModuleKind::ES2015
                                | ModuleKind::ES2020
                                | ModuleKind::ES2022
                                | ModuleKind::ESNext
                                | ModuleKind::Preserve
                        );
                        let has_explicit_extension =
                            std::path::Path::new(specifier).extension().is_some();
                        if diagnostic.code == tsz::module_resolver::CANNOT_FIND_MODULE
                            && !has_explicit_extension
                            && module_kind_prefers_2792
                            && options.effective_module_resolution()
                                != tsz::config::ModuleResolutionKind::Bundler
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
                    };
                    check_file_for_parallel(context)
                })
                .collect()
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
            let mut file_diagnostics = Vec::new();
            for parse_diagnostic in &file.parse_diagnostics {
                file_diagnostics.push(parse_diagnostic_to_checker(
                    &file.file_name,
                    parse_diagnostic,
                ));
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

    if let Some(helper_diag) = detect_missing_tslib_helper_diagnostic(program, options) {
        diagnostics.push(helper_diag);
    }

    diagnostics
}

pub(super) fn detect_missing_tslib_helper_diagnostic(
    program: &MergedProgram,
    options: &ResolvedCompilerOptions,
) -> Option<Diagnostic> {
    if !options.import_helpers {
        return None;
    }

    let tslib_file = program.files.iter().find(|file| {
        Path::new(&file.file_name)
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name.eq_ignore_ascii_case("tslib.d.ts"))
    })?;

    let tslib_exports_empty = program
        .module_exports
        .get(&tslib_file.file_name)
        .is_none_or(tsz_binder::SymbolTable::is_empty);

    if !tslib_exports_empty {
        return None;
    }

    for file in &program.files {
        if file.file_name == tslib_file.file_name || file.file_name.ends_with(".d.ts") {
            continue;
        }

        if let Some((helper_name, start, length)) = first_required_helper(file) {
            return Some(Diagnostic::error(
                file.file_name.clone(),
                start,
                length,
                format!(
                    "This syntax requires an imported helper named '{helper_name}' which does not exist in 'tslib'. Consider upgrading your version of 'tslib'."
                ),
                2343,
            ));
        }
    }

    None
}

pub(super) fn first_required_helper(file: &BoundFile) -> Option<(&'static str, u32, u32)> {
    let mut saw_await: Option<(u32, u32)> = None;
    let mut saw_yield: Option<(u32, u32)> = None;

    for node_idx_raw in 0..file.arena.len() {
        let node_idx = NodeIndex(node_idx_raw as u32);
        let Some(node) = file.arena.get(node_idx) else {
            continue;
        };

        if node.kind == SyntaxKind::PrivateIdentifier as u16 {
            return Some((
                "__classPrivateFieldSet",
                node.pos,
                node.end.saturating_sub(node.pos),
            ));
        }

        if node.kind == syntax_kind_ext::DECORATOR {
            return Some(("__decorate", node.pos, node.end.saturating_sub(node.pos)));
        }

        if node.kind == syntax_kind_ext::CLASS_DECLARATION
            && let Some(class_data) = file.arena.get_class(node)
            && class_data.heritage_clauses.is_some()
        {
            return Some(("__extends", node.pos, node.end.saturating_sub(node.pos)));
        }

        if node.kind == syntax_kind_ext::AWAIT_EXPRESSION {
            saw_await = Some((node.pos, node.end.saturating_sub(node.pos)));
        }
        if node.kind == syntax_kind_ext::YIELD_EXPRESSION {
            saw_yield = Some((node.pos, node.end.saturating_sub(node.pos)));
        }
    }

    if let (Some((start, length)), Some(_)) = (saw_await, saw_yield) {
        return Some(("__asyncGenerator", start, length));
    }

    None
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
    } = context;
    let file = &program.files[file_idx];
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

pub(super) fn compute_export_hash(
    program: &MergedProgram,
    file: &BoundFile,
    file_idx: usize,
    checker: &mut CheckerState,
) -> u64 {
    let mut formatter = TypeFormatter::with_symbols(&program.type_interner, &program.symbols);
    let mut hasher = FxHasher::default();
    let mut type_str_cache: FxHashMap<TypeId, String> = FxHashMap::default();

    if let Some(file_locals) = program.file_locals.get(file_idx) {
        let mut exports: Vec<(&String, SymbolId)> = file_locals
            .iter()
            .filter_map(|(name, &sym_id)| {
                is_exported_symbol(&program.symbols, sym_id).then_some((name, sym_id))
            })
            .collect();
        exports.sort_by(|left, right| left.0.cmp(right.0));

        for (name, sym_id) in exports {
            name.hash(&mut hasher);
            let type_id = checker.get_type_of_symbol(sym_id);
            let type_str = type_str_cache
                .entry(type_id)
                .or_insert_with(|| formatter.format(type_id));
            type_str.hash(&mut hasher);
        }
    }

    let mut export_signatures = Vec::new();
    collect_export_signatures(file, checker, &mut formatter, &mut export_signatures);
    export_signatures.sort();
    for signature in export_signatures {
        signature.hash(&mut hasher);
    }

    hasher.finish()
}

pub(super) fn js_file_has_ts_check_pragma(file: &BoundFile) -> bool {
    let Some(source) = file.arena.get_source_file_at(file.source_file) else {
        return false;
    };
    let text = source.text.as_ref().to_ascii_lowercase();
    let ts_check = text.rfind("@ts-check");
    let ts_no_check = text.rfind("@ts-nocheck");
    match (ts_check, ts_no_check) {
        (Some(check_idx), Some(no_check_idx)) => check_idx > no_check_idx,
        (Some(_), None) => true,
        _ => false,
    }
}

pub(super) fn js_file_has_ts_nocheck_pragma(file: &BoundFile) -> bool {
    let Some(source) = file.arena.get_source_file_at(file.source_file) else {
        return false;
    };
    source
        .text
        .as_ref()
        .to_ascii_lowercase()
        .contains("@ts-nocheck")
}

pub(super) fn is_exported_symbol(symbols: &tsz::binder::SymbolArena, sym_id: SymbolId) -> bool {
    let Some(symbol) = symbols.get(sym_id) else {
        return false;
    };
    symbol.is_exported || (symbol.flags & symbol_flags::EXPORT_VALUE) != 0
}

pub(super) fn collect_export_signatures(
    file: &BoundFile,
    checker: &mut CheckerState,
    formatter: &mut TypeFormatter,
    signatures: &mut Vec<String>,
) {
    let arena = &file.arena;
    let Some(source) = arena.get_source_file_at(file.source_file) else {
        return;
    };

    for &stmt_idx in &source.statements.nodes {
        let Some(stmt) = arena.get(stmt_idx) else {
            continue;
        };

        if let Some(export_decl) = arena.get_export_decl(stmt) {
            if export_decl.is_default_export {
                if let Some(signature) =
                    export_default_signature(export_decl.export_clause, checker, formatter)
                {
                    signatures.push(signature);
                }
                continue;
            }

            if export_decl.module_specifier.is_none() {
                if !export_decl.export_clause.is_none() {
                    let clause_node = export_decl.export_clause;
                    if arena.get_named_imports_at(clause_node).is_some() {
                        collect_local_named_export_signatures(
                            arena,
                            file.source_file,
                            clause_node,
                            checker,
                            formatter,
                            export_type_prefix(export_decl.is_type_only),
                            signatures,
                        );
                    } else {
                        collect_exported_declaration_signatures(
                            arena,
                            clause_node,
                            checker,
                            formatter,
                            export_type_prefix(export_decl.is_type_only),
                            signatures,
                        );
                    }
                }
                continue;
            }

            let module_spec = arena
                .get_literal_text(export_decl.module_specifier)
                .unwrap_or("")
                .to_string();
            if export_decl.export_clause.is_none() {
                signatures.push(format!(
                    "{}*|{}",
                    export_type_prefix(export_decl.is_type_only),
                    module_spec
                ));
                continue;
            }

            let clause_node = export_decl.export_clause;
            if let Some(named) = arena.get_named_imports_at(clause_node) {
                let mut specifiers = Vec::new();
                for &spec_idx in &named.elements.nodes {
                    let Some(spec) = arena.get_specifier_at(spec_idx) else {
                        continue;
                    };
                    let name = arena.get_identifier_text(spec.name).unwrap_or("");
                    if spec.property_name.is_none() {
                        specifiers.push(name.to_string());
                    } else {
                        let property = arena.get_identifier_text(spec.property_name).unwrap_or("");
                        specifiers.push(format!("{property} as {name}"));
                    }
                }
                specifiers.sort();
                signatures.push(format!(
                    "{}{{{}}}|{}",
                    export_type_prefix(export_decl.is_type_only),
                    specifiers.join(","),
                    module_spec
                ));
            } else if let Some(name) = arena.get_identifier_text(clause_node) {
                signatures.push(format!(
                    "{}* as {}|{}",
                    export_type_prefix(export_decl.is_type_only),
                    name,
                    module_spec
                ));
            }

            continue;
        }

        if let Some(export_assignment) = arena.get_export_assignment(stmt)
            && !export_assignment.expression.is_none()
        {
            let type_id = checker.get_type_of_node(export_assignment.expression);
            let type_str = formatter.format(type_id);
            signatures.push(format!("export=:{type_str}"));
        }
    }
}

pub(super) fn collect_local_named_export_signatures(
    arena: &NodeArena,
    source_file: NodeIndex,
    named_idx: NodeIndex,
    checker: &mut CheckerState,
    formatter: &mut TypeFormatter,
    type_prefix: &str,
    signatures: &mut Vec<String>,
) {
    let Some(named) = arena.get_named_imports_at(named_idx) else {
        return;
    };

    for &spec_idx in &named.elements.nodes {
        let Some(spec) = arena.get_specifier_at(spec_idx) else {
            continue;
        };
        let exported_name = if !spec.name.is_none() {
            arena.get_identifier_text(spec.name).unwrap_or("")
        } else {
            arena.get_identifier_text(spec.property_name).unwrap_or("")
        };
        if exported_name.is_empty() {
            continue;
        }
        let local_name = if !spec.property_name.is_none() {
            arena.get_identifier_text(spec.property_name).unwrap_or("")
        } else {
            exported_name
        };
        let type_id = find_local_declaration(arena, source_file, local_name)
            .map_or(TypeId::ANY, |decl_idx| checker.get_type_of_node(decl_idx));
        let type_str = formatter.format(type_id);
        signatures.push(format!("{type_prefix}{exported_name}:{type_str}"));
    }
}

pub(super) fn collect_exported_declaration_signatures(
    arena: &NodeArena,
    decl_idx: NodeIndex,
    checker: &mut CheckerState,
    formatter: &mut TypeFormatter,
    type_prefix: &str,
    signatures: &mut Vec<String>,
) {
    let Some(node) = arena.get(decl_idx) else {
        return;
    };

    if node.kind == syntax_kind_ext::VARIABLE_STATEMENT {
        if let Some(var_stmt) = arena.get_variable(node) {
            for &list_idx in &var_stmt.declarations.nodes {
                collect_exported_declaration_signatures(
                    arena,
                    list_idx,
                    checker,
                    formatter,
                    type_prefix,
                    signatures,
                );
            }
        }
        return;
    }

    if node.kind == syntax_kind_ext::VARIABLE_DECLARATION_LIST {
        if let Some(list) = arena.get_variable(node) {
            for &decl_idx in &list.declarations.nodes {
                collect_exported_declaration_signatures(
                    arena,
                    decl_idx,
                    checker,
                    formatter,
                    type_prefix,
                    signatures,
                );
            }
        }
        return;
    }

    if let Some(var_decl) = arena.get_variable_declaration(node) {
        if let Some(name) = arena.get_identifier_text(var_decl.name) {
            push_exported_signature(name, decl_idx, checker, formatter, type_prefix, signatures);
        }
        return;
    }

    if let Some(func) = arena.get_function(node) {
        if let Some(name) = arena.get_identifier_text(func.name) {
            push_exported_signature(name, decl_idx, checker, formatter, type_prefix, signatures);
        }
        return;
    }

    if let Some(class) = arena.get_class(node) {
        if let Some(name) = arena.get_identifier_text(class.name) {
            push_exported_signature(name, decl_idx, checker, formatter, type_prefix, signatures);
        }
        return;
    }

    if let Some(interface) = arena.get_interface(node) {
        if let Some(name) = arena.get_identifier_text(interface.name) {
            push_exported_signature(name, decl_idx, checker, formatter, type_prefix, signatures);
        }
        return;
    }

    if let Some(type_alias) = arena.get_type_alias(node) {
        if let Some(name) = arena.get_identifier_text(type_alias.name) {
            push_exported_signature(name, decl_idx, checker, formatter, type_prefix, signatures);
        }
        return;
    }

    if let Some(enum_decl) = arena.get_enum(node) {
        if let Some(name) = arena.get_identifier_text(enum_decl.name) {
            push_exported_signature(name, decl_idx, checker, formatter, type_prefix, signatures);
        }
        return;
    }

    if let Some(module_decl) = arena.get_module(node) {
        let name = arena
            .get_identifier_text(module_decl.name)
            .or_else(|| arena.get_literal_text(module_decl.name));
        if let Some(name) = name {
            push_exported_signature(name, decl_idx, checker, formatter, type_prefix, signatures);
        }
    }
}

pub(super) fn push_exported_signature(
    name: &str,
    decl_idx: NodeIndex,
    checker: &mut CheckerState,
    formatter: &mut TypeFormatter,
    type_prefix: &str,
    signatures: &mut Vec<String>,
) {
    let type_id = checker.get_type_of_node(decl_idx);
    let type_str = formatter.format(type_id);
    signatures.push(format!("{type_prefix}{name}:{type_str}"));
}

pub(super) fn find_local_declaration(
    arena: &NodeArena,
    source_file: NodeIndex,
    name: &str,
) -> Option<NodeIndex> {
    let source = arena.get_source_file_at(source_file)?;

    for &stmt_idx in &source.statements.nodes {
        let Some(stmt) = arena.get(stmt_idx) else {
            continue;
        };
        if let Some(export_decl) = arena.get_export_decl(stmt) {
            if export_decl.export_clause.is_none() {
                continue;
            }
            let clause_idx = export_decl.export_clause;
            if arena.get_named_imports_at(clause_idx).is_some() {
                continue;
            }
            if let Some(found) = find_local_declaration_in_node(arena, clause_idx, name) {
                return Some(found);
            }
            continue;
        }

        if let Some(found) = find_local_declaration_in_node(arena, stmt_idx, name) {
            return Some(found);
        }
    }

    None
}

pub(super) fn find_local_declaration_in_node(
    arena: &NodeArena,
    node_idx: NodeIndex,
    name: &str,
) -> Option<NodeIndex> {
    let node = arena.get(node_idx)?;

    if let Some(var_decl) = arena.get_variable_declaration(node) {
        if let Some(decl_name) = arena.get_identifier_text(var_decl.name)
            && decl_name == name
        {
            return Some(node_idx);
        }
        return None;
    }

    if node.kind == syntax_kind_ext::VARIABLE_STATEMENT {
        if let Some(var_stmt) = arena.get_variable(node) {
            for &list_idx in &var_stmt.declarations.nodes {
                if let Some(found) = find_local_declaration_in_node(arena, list_idx, name) {
                    return Some(found);
                }
            }
        }
        return None;
    }

    if node.kind == syntax_kind_ext::VARIABLE_DECLARATION_LIST {
        if let Some(list) = arena.get_variable(node) {
            for &decl_idx in &list.declarations.nodes {
                if let Some(found) = find_local_declaration_in_node(arena, decl_idx, name) {
                    return Some(found);
                }
            }
        }
        return None;
    }

    if let Some(func) = arena.get_function(node) {
        if let Some(decl_name) = arena.get_identifier_text(func.name)
            && decl_name == name
        {
            return Some(node_idx);
        }
        return None;
    }

    if let Some(class) = arena.get_class(node) {
        if let Some(decl_name) = arena.get_identifier_text(class.name)
            && decl_name == name
        {
            return Some(node_idx);
        }
        return None;
    }

    if let Some(interface) = arena.get_interface(node) {
        if let Some(decl_name) = arena.get_identifier_text(interface.name)
            && decl_name == name
        {
            return Some(node_idx);
        }
        return None;
    }

    if let Some(type_alias) = arena.get_type_alias(node) {
        if let Some(decl_name) = arena.get_identifier_text(type_alias.name)
            && decl_name == name
        {
            return Some(node_idx);
        }
        return None;
    }

    if let Some(enum_decl) = arena.get_enum(node) {
        if let Some(decl_name) = arena.get_identifier_text(enum_decl.name)
            && decl_name == name
        {
            return Some(node_idx);
        }
        return None;
    }

    if let Some(module_decl) = arena.get_module(node) {
        let decl_name = arena
            .get_identifier_text(module_decl.name)
            .or_else(|| arena.get_literal_text(module_decl.name));
        if let Some(decl_name) = decl_name
            && decl_name == name
        {
            return Some(node_idx);
        }
    }

    None
}

pub(super) fn export_default_signature(
    export_clause: NodeIndex,
    checker: &mut CheckerState,
    formatter: &mut TypeFormatter,
) -> Option<String> {
    if export_clause.is_none() {
        return None;
    }
    let type_id = if let Some(sym_id) = checker.ctx.binder.get_node_symbol(export_clause) {
        checker.get_type_of_symbol(sym_id)
    } else {
        checker.get_type_of_node(export_clause)
    };
    let type_str = formatter.format(type_id);
    Some(format!("default:{type_str}"))
}

pub(super) const fn export_type_prefix(is_type_only: bool) -> &'static str {
    if is_type_only { "type:" } else { "" }
}

pub(super) fn parse_diagnostic_to_checker(
    file_name: &str,
    diagnostic: &ParseDiagnostic,
) -> Diagnostic {
    Diagnostic::error(
        file_name.to_string(),
        diagnostic.start,
        diagnostic.length,
        diagnostic.message.clone(),
        diagnostic.code,
    )
}

pub(super) fn create_binder_from_bound_file(
    file: &BoundFile,
    program: &MergedProgram,
    file_idx: usize,
) -> BinderState {
    let mut file_locals = SymbolTable::new();

    if file_idx < program.file_locals.len() {
        for (name, &sym_id) in program.file_locals[file_idx].iter() {
            file_locals.set(name.clone(), sym_id);
        }
    }

    for (name, &sym_id) in program.globals.iter() {
        if !file_locals.has(name) {
            file_locals.set(name.clone(), sym_id);
        }
    }

    // Merge module augmentations from all files
    // When checking a file, we need access to augmentations from all other files
    let mut merged_module_augmentations: rustc_hash::FxHashMap<
        String,
        Vec<tsz::binder::ModuleAugmentation>,
    > = rustc_hash::FxHashMap::default();

    for other_file in &program.files {
        for (spec, augs) in &other_file.module_augmentations {
            merged_module_augmentations
                .entry(spec.clone())
                .or_default()
                .extend(augs.clone());
        }
    }

    // Merge global augmentations from all files
    // Each augmentation is tagged with its source arena for cross-file resolution.
    let mut merged_global_augmentations: rustc_hash::FxHashMap<
        String,
        Vec<tsz::binder::GlobalAugmentation>,
    > = rustc_hash::FxHashMap::default();

    for other_file in &program.files {
        for (name, decls) in &other_file.global_augmentations {
            merged_global_augmentations
                .entry(name.clone())
                .or_default()
                .extend(decls.iter().map(|aug| {
                    tsz::binder::GlobalAugmentation::with_arena(
                        aug.node,
                        Arc::clone(&other_file.arena),
                    )
                }));
        }
    }

    let mut binder = BinderState::from_bound_state_with_scopes_and_augmentations(
        BinderOptions::default(),
        program.symbols.clone(),
        file_locals,
        file.node_symbols.clone(),
        BinderStateScopeInputs {
            scopes: file.scopes.clone(),
            node_scope_ids: file.node_scope_ids.clone(),
            global_augmentations: merged_global_augmentations,
            module_augmentations: merged_module_augmentations,
            module_exports: program.module_exports.clone(),
            reexports: program.reexports.clone(),
            wildcard_reexports: program.wildcard_reexports.clone(),
            symbol_arenas: program.symbol_arenas.clone(),
            declaration_arenas: program.declaration_arenas.clone(),
            shorthand_ambient_modules: program.shorthand_ambient_modules.clone(),
            modules_with_export_equals: Default::default(),
            flow_nodes: file.flow_nodes.clone(),
            node_flow: file.node_flow.clone(),
            switch_clause_to_switch: file.switch_clause_to_switch.clone(),
            expando_properties: file.expando_properties.clone(),
        },
    );

    binder.declared_modules = program.declared_modules.clone();
    // Restore is_external_module from BoundFile to preserve per-file state
    binder.is_external_module = file.is_external_module;
    // Track lib-originating symbols so unused checking can skip them
    binder.lib_symbol_ids = program.lib_symbol_ids.clone();
    binder
}

/// Build a lightweight binder for cross-file symbol/export lookups.
///
/// This avoids cloning per-file node/scope/flow structures when populating
/// `CheckerContext::all_binders`, which only needs symbol/file-local/module-export data.
pub(super) fn create_cross_file_lookup_binder(
    file: &BoundFile,
    program: &MergedProgram,
    file_idx: usize,
) -> BinderState {
    let mut file_locals = SymbolTable::new();

    if file_idx < program.file_locals.len() {
        for (name, &sym_id) in program.file_locals[file_idx].iter() {
            file_locals.set(name.clone(), sym_id);
        }
    }

    for (name, &sym_id) in program.globals.iter() {
        if !file_locals.has(name) {
            file_locals.set(name.clone(), sym_id);
        }
    }

    // Keep cross-file binders intentionally tiny: they are only used to map
    // import targets to that file's locals/exports.
    let mut binder = BinderState::new();
    binder.file_locals = file_locals;
    binder.node_symbols = file.node_symbols.clone();
    binder.scopes = file.scopes.clone();
    binder.node_scope_ids = file.node_scope_ids.clone();
    binder.declared_modules = program.declared_modules.clone();
    binder.shorthand_ambient_modules = program.shorthand_ambient_modules.clone();
    if let Some(exports) = program.module_exports.get(&file.file_name) {
        binder
            .module_exports
            .insert(file.file_name.clone(), exports.clone());
    }
    for module_name in program
        .declared_modules
        .iter()
        .chain(program.shorthand_ambient_modules.iter())
    {
        if let Some(exports) = program.module_exports.get(module_name) {
            binder
                .module_exports
                .insert(module_name.clone(), exports.clone());
        }
    }
    // Copy re-export data for cross-file import validation.
    // Without this, `resolve_import_in_file` can't follow wildcard/named
    // re-export chains across binder boundaries.
    if let Some(wildcards) = program.wildcard_reexports.get(&file.file_name) {
        binder
            .wildcard_reexports
            .insert(file.file_name.clone(), wildcards.clone());
    }
    if let Some(reexports) = program.reexports.get(&file.file_name) {
        binder
            .reexports
            .insert(file.file_name.clone(), reexports.clone());
    }
    binder.is_external_module = file.is_external_module;
    binder
}

//! Diagnostics collection and per-file checking orchestration for the compilation driver.

use super::check_utils::*;
use super::*;

/// Check if a filename is a TypeScript declaration file (.d.ts, .d.cts, .d.mts).
fn is_declaration_file(name: &str) -> bool {
    name.ends_with(".d.ts") || name.ends_with(".d.mts") || name.ends_with(".d.cts")
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

                        let is_ordinary_bare_specifier = !specifier.starts_with('.')
                            && !specifier.starts_with('/')
                            && !specifier.contains(':');
                        if is_ordinary_bare_specifier
                            && (program.declared_modules.contains(specifier)
                                || program.shorthand_ambient_modules.contains(specifier))
                        {
                            // Local ambient modules are discovered by binding rather than
                            // path-based resolution. Keep the specifier "resolved" here so
                            // the checker can import from the ambient module without a
                            // spurious TS2307 from the driver layer.
                            resolved_module_specifiers.insert((file_idx, specifier.clone()));
                            continue;
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
                                            code: tsz::checker::diagnostics::diagnostic_codes::COULD_NOT_FIND_A_DECLARATION_FILE_FOR_MODULE_IMPLICITLY_HAS_AN_ANY_TYPE,
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

    // Create a shared QueryCache for memoized evaluate_type/is_subtype_of calls.
    let query_cache = QueryCache::new(&program.type_interner);

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
        checker.ctx.set_lib_contexts(lib_contexts.to_vec());
        checker.ctx.set_actual_lib_file_count(lib_contexts.len());
        checker.ctx.set_typescript_dom_replacement_globals(
            typescript_dom_replacement_globals.0,
            typescript_dom_replacement_globals.1,
            typescript_dom_replacement_globals.2,
        );
        checker.ctx.set_all_arenas(Arc::clone(&all_arenas));
        checker.ctx.set_all_binders(Arc::clone(&all_binders));
        {
            let mut targets = checker.ctx.cross_file_symbol_targets.borrow_mut();
            for (sym_id, owner_idx) in symbol_file_targets.iter() {
                targets.insert(*sym_id, *owner_idx);
            }
        }
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
        let lib_ctx_for_parallel = lib_contexts.to_vec();
        let shared_lib_cache: Arc<dashmap::DashMap<String, Option<tsz_solver::TypeId>>> =
            Arc::new(dashmap::DashMap::new());

        // Check all files in parallel — each file gets its own CheckerState.
        // TypeInterner (DashMap) and QueryCache (RwLock) are already thread-safe.
        #[cfg(not(target_arch = "wasm32"))]
        let file_results: Vec<(Vec<Diagnostic>, Option<TypeCache>)> = {
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
                            symbol_file_targets: &symbol_file_targets,
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
                            typescript_dom_replacement_globals,
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
                            query_cache: &query_cache,
                            compiler_options: &compiler_options,
                            lib_contexts: &lib_ctx_for_parallel,
                            all_arenas: &all_arenas,
                            all_binders: &all_binders,
                            symbol_file_targets: &symbol_file_targets,
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
                            typescript_dom_replacement_globals,
                            program_has_real_syntax_errors,
                        };
                        check_file_for_parallel(context)
                    })
                    .collect()
            }
        };

        #[cfg(target_arch = "wasm32")]
        let file_results: Vec<(Vec<Diagnostic>, Option<TypeCache>)> = work_items
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
                    symbol_file_targets: &symbol_file_targets,
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
                    typescript_dom_replacement_globals,
                    program_has_real_syntax_errors,
                };
                check_file_for_parallel(context)
            })
            .collect();

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
            checker.ctx.set_typescript_dom_replacement_globals(
                typescript_dom_replacement_globals.0,
                typescript_dom_replacement_globals.1,
                typescript_dom_replacement_globals.2,
            );
            checker.ctx.set_all_arenas(Arc::clone(&all_arenas));
            checker.ctx.set_all_binders(Arc::clone(&all_binders));
            {
                let mut targets = checker.ctx.cross_file_symbol_targets.borrow_mut();
                for (sym_id, owner_idx) in symbol_file_targets.iter() {
                    targets.insert(*sym_id, *owner_idx);
                }
            }
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
            let filtered_parse_diagnostics = filtered_parse_diagnostics(&file.parse_diagnostics);
            let mut file_diagnostics = Vec::new();
            for parse_diagnostic in filtered_parse_diagnostics {
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

                let mut checker_diagnostics = std::mem::take(&mut checker.ctx.diagnostics);
                post_process_checker_diagnostics(
                    &mut checker_diagnostics,
                    file,
                    options,
                    program_has_real_syntax_errors,
                );

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
    symbol_file_targets: &'a Arc<Vec<(tsz::binder::SymbolId, usize)>>,
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
    typescript_dom_replacement_globals: (bool, bool, bool),
    program_has_real_syntax_errors: bool,
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
        symbol_file_targets,
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
        typescript_dom_replacement_globals,
        program_has_real_syntax_errors,
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
    checker.ctx.set_typescript_dom_replacement_globals(
        typescript_dom_replacement_globals.0,
        typescript_dom_replacement_globals.1,
        typescript_dom_replacement_globals.2,
    );

    checker.ctx.set_all_arenas(Arc::clone(all_arenas));
    checker.ctx.set_all_binders(Arc::clone(all_binders));
    {
        let mut targets = checker.ctx.cross_file_symbol_targets.borrow_mut();
        for (sym_id, owner_idx) in symbol_file_targets.iter() {
            targets.insert(*sym_id, *owner_idx);
        }
    }
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
    let filtered_parse_diagnostics = filtered_parse_diagnostics(&file.parse_diagnostics);

    // Collect parse diagnostics
    let mut file_diagnostics: Vec<Diagnostic> = filtered_parse_diagnostics
        .into_iter()
        .map(|d| parse_diagnostic_to_checker(&file.file_name, d))
        .collect();

    // Note: We always run checking for all files (JS and TS).
    // TypeScript reports syntax/semantic errors like TS1210 (strict mode violations)
    // even for JS files without checkJs. Only type-level errors are gated by checkJs.
    if !no_check {
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
        );

        assert!(
            diagnostics.is_empty(),
            "Expected compile-inner program build Promise<T> -> PromiseLike<T> assignability, got: {diagnostics:?}"
        );
    }

    #[test]
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
        let source_type = checker.get_type_of_node(decl.initializer);
        let target_type = checker.get_type_from_type_node(decl.type_annotation);
        let read_constraint_type =
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
        let evaluated_target_type = {
            let mut evaluator =
                tsz_solver::TypeEvaluator::with_resolver(&program.type_interner, &checker.ctx);
            evaluator.evaluate(target_type)
        };
        let source_symbol = match program.type_interner.lookup(source_type) {
            Some(tsz_solver::TypeData::Object(shape_id))
            | Some(tsz_solver::TypeData::ObjectWithIndex(shape_id)) => {
                format!("{:?}", program.type_interner.object_shape(shape_id).symbol)
            }
            other => format!("{other:?}"),
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
        );
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
        );

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
}

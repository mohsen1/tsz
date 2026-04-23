//! Diagnostics collection and per-file checking orchestration for the compilation driver.

use super::check_utils::*;
use super::*;
use tsz::checker::context::RequestCacheCounters;

const fn checker_resolution_mode_override(
    mode: Option<tsz::module_resolver::ImportingModuleKind>,
) -> Option<tsz::checker::context::ResolutionModeOverride> {
    match mode {
        Some(tsz::module_resolver::ImportingModuleKind::Esm) => {
            Some(tsz::checker::context::ResolutionModeOverride::Import)
        }
        Some(tsz::module_resolver::ImportingModuleKind::CommonJs) => {
            Some(tsz::checker::context::ResolutionModeOverride::Require)
        }
        None => None,
    }
}

fn checker_lookup_resolution_mode(
    module_resolver: &mut ModuleResolver,
    options: &ResolvedCompilerOptions,
    file_path: &Path,
    import_kind: tsz::module_resolver::ImportKind,
    resolution_mode_override: Option<tsz::module_resolver::ImportingModuleKind>,
) -> Option<tsz::checker::context::ResolutionModeOverride> {
    use tsz::module_resolver::{ImportKind, ImportingModuleKind, ModuleExtension};

    let mode = resolution_mode_override.unwrap_or_else(|| match options.checker.module {
        // Mirror ModuleResolver::resolve_with_kind_and_module_kind() so request-keyed
        // checker maps line up with the actual lookup mode used by the resolver.
        ModuleKind::Preserve => {
            let extension = ModuleExtension::from_path(file_path);
            if extension.forces_esm() {
                ImportingModuleKind::Esm
            } else if extension.forces_cjs() {
                ImportingModuleKind::CommonJs
            } else {
                match import_kind {
                    ImportKind::EsmImport | ImportKind::DynamicImport | ImportKind::EsmReExport => {
                        ImportingModuleKind::Esm
                    }
                    ImportKind::CjsRequire => ImportingModuleKind::CommonJs,
                }
            }
        }
        _ => module_resolver.get_importing_module_kind(file_path),
    });

    checker_resolution_mode_override(Some(mode))
}

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

#[derive(Default)]
pub(super) struct CheckerLibSet {
    pub(super) files: Vec<Arc<LibFile>>,
    pub(super) contexts: Vec<LibContext>,
}

/// Check if a filename is a TypeScript declaration file (.d.ts, .d.cts, .d.mts).
fn is_declaration_file(name: &str) -> bool {
    tsz::module_resolver::ModuleExtension::from_path(std::path::Path::new(name)).is_declaration()
}

fn should_apply_duplicate_package_redirect(importing_file: &Path) -> bool {
    importing_file
        .components()
        .any(|component| component.as_os_str() == "node_modules")
}

/// Clone lib.d.ts files and create fresh checker-facing `LibContext` objects.
///
/// The binding pipeline mutates per-file binder state while injecting lib symbols into the
/// unified program. Reusing those same `LibFile` binders as checker lib contexts leaks that
/// binding-phase state into lib type resolution and can corrupt structural relations between
/// recursive lib types like `RegExpMatchArray`, `Promise<T>`, and `PromiseLike<T>`.
///
/// Build a fresh checker-facing lib set from the already-loaded lib sources so program
/// binding and checker lib resolution stay isolated without requiring disk reloads.
pub(super) fn load_checker_libs(lib_files: &[Arc<LibFile>]) -> CheckerLibSet {
    let files = parallel::clone_lib_files_for_checker(lib_files);
    let contexts = files
        .iter()
        .map(|lib| LibContext {
            arena: Arc::clone(&lib.arena),
            binder: Arc::clone(&lib.binder),
        })
        .collect();

    CheckerLibSet { files, contexts }
}

fn should_skip_type_checking_for_file(
    file_name: &str,
    options: &ResolvedCompilerOptions,
    is_default_lib: bool,
) -> bool {
    (options.skip_lib_check && is_declaration_file(file_name))
        || (options.skip_default_lib_check && is_default_lib)
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
        || tsz::checker::diagnostics::is_js_grammar_diagnostic(code)
        || is_reserved_type_name_declaration_diagnostic(code)
}

fn post_process_checker_diagnostics(
    checker_diagnostics: &mut Vec<Diagnostic>,
    file: &BoundFile,
    options: &ResolvedCompilerOptions,
    program_has_real_syntax_errors: bool,
    has_deprecation_diagnostics: bool,
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
                || tsz::checker::diagnostics::is_js_grammar_diagnostic(diag.code)
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
            // TS1361/TS1362 are semantic type-only value-use diagnostics, not
            // parser grammar errors. Keep them for checked JS files even
            // though their codes live in the TS1xxx range.
            if !should_filter_type_errors && matches!(diag.code, 1361 | 1362) {
                return true;
            }
            if tsz::checker::diagnostics::is_parser_grammar_diagnostic(diag.code) {
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

    // TS2754 ("super may not use type arguments") indicates a fundamental class
    // hierarchy error. tsc suppresses all other semantic diagnostics when TS2754
    // is present. TS2754 is emitted by the parser, so check parse diagnostics.
    let has_ts2754 = file.parse_diagnostics.iter().any(|d| d.code == 2754);
    if has_ts2754 {
        checker_diagnostics.retain(|diag| diag.code < 2000);
    }

    // When TS5107/TS5101 deprecation diagnostics are present, suppress the most
    // common type relationship errors that tsc would not emit. Parser errors
    // (<2000) are handled separately and not affected by this filter.
    if has_deprecation_diagnostics {
        // Type relationship errors to suppress when deprecation warnings are present
        const SUPPRESSED_TYPE_CODES: &[u32] = &[
            2322, // TS2322: Type not assignable
            2345, // TS2345: Argument not assignable
            2339, // TS2339: Property does not exist
            2343, // TS2343: Access modifier error
            2882, // TS2882: Cannot find module/type declarations for side-effect import
            2304, // TS2304: Cannot find name
            2307, // TS2307: Cannot find module
            7006, // TS7006: Parameter implicitly has 'any' type
            7005, // TS7005: Variable implicitly has 'any' type
            2323, // TS2323: Cannot redeclare exported variable
            2741, // TS2741: Missing properties
            2510, // TS2510: Cannot assign to read-only property
            2694, // TS2694: Namespace not found
            2531, // TS2531: Possibly null
            2532, // TS2532: Possibly undefined
            2533, // TS2533: Object is possibly null or undefined
            2564, // TS2564: Property has no initializer
            2454, // TS2454: Variable used before being assigned
            2403, // TS2403: Subsequent variable declarations must have same type
            2411, // TS2411: Property conflict
            2300, // TS2300: Duplicate identifier
        ];
        checker_diagnostics.retain(|diag| !SUPPRESSED_TYPE_CODES.contains(&diag.code));
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
            if diag.code < 2000 || tsz::checker::diagnostics::is_js_grammar_diagnostic(diag.code) {
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
    checker_libs: &CheckerLibSet,
    typescript_dom_replacement_globals: (bool, bool, bool),
    type_cache_output: &std::sync::Mutex<FxHashMap<PathBuf, TypeCache>>,
    has_deprecation_diagnostics: bool,
) -> CollectDiagnosticsResult {
    let _collect_span =
        tracing::info_span!("collect_diagnostics", files = program.files.len()).entered();
    let mut diagnostics: Vec<Diagnostic> = Vec::new();
    let mut request_cache_counters = RequestCacheCounters::default();
    let file_count = program.files.len();
    let mut used_paths = FxHashSet::with_capacity_and_hasher(file_count, Default::default());
    let mut cache = cache;
    let mut resolution_cache = ModuleResolutionCache::default();
    // Pre-size: each map ends up with exactly one entry per file. Without
    // capacity hints the inserts go through the standard power-of-two grow
    // path (~12 rehashes for a 6000-entry map). Driver setup runs once per
    // build, but on a 6000-file project the rehash cost is real startup
    // overhead.
    let mut program_paths = FxHashSet::with_capacity_and_hasher(file_count, Default::default());
    let mut canonical_to_file_name: FxHashMap<PathBuf, String> =
        FxHashMap::with_capacity_and_hasher(file_count, Default::default());
    let mut canonical_to_file_idx: FxHashMap<PathBuf, usize> =
        FxHashMap::with_capacity_and_hasher(file_count, Default::default());
    let program_has_real_syntax_errors = program_has_real_syntax_errors(program);

    {
        let _span = tracing::info_span!("build_program_path_maps", files = file_count).entered();
        for (idx, file) in program.files.iter().enumerate() {
            let canonical = normalize_resolved_path(Path::new(&file.file_name), options);
            program_paths.insert(canonical.clone());
            canonical_to_file_name.insert(canonical.clone(), file.file_name.clone());
            canonical_to_file_idx.insert(canonical, idx);
        }
    }

    // Pre-compute merged augmentations once for all binder reconstruction paths.
    let merged_augmentations = MergedAugmentations::from_program(program);
    let affected_lib_interfaces = affected_lib_interface_names(program, checker_libs);
    let affected_lib_extension_interfaces =
        affected_lib_extension_interface_names(program, checker_libs, &affected_lib_interfaces);

    // Pre-create all binders for cross-file resolution
    let all_binders: Arc<Vec<Arc<BinderState>>> = Arc::new({
        use rayon::prelude::*;
        let _span =
            tracing::info_span!("build_cross_file_binders", files = program.files.len()).entered();
        program
            .files
            .par_iter()
            .enumerate()
            .map(|(file_idx, file)| {
                Arc::new(create_cross_file_lookup_binder_with_augmentations(
                    file,
                    program,
                    file_idx,
                    &merged_augmentations,
                ))
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
    // Build symbol → file_idx mapping in O(symbols + files) instead of
    // O(symbols × files). Previously each symbol scanned `all_arenas` linearly
    // looking for the matching `Arc<NodeArena>` via `ptr_eq` — on a large
    // project (~100K-500K symbols × 6000+ files) that exploded into
    // billion-scale pointer comparisons just to populate the map.
    //
    // `Arc::ptr_eq` compares the inner allocation address, so the raw pointer
    // value uniquely identifies an arena. A small one-shot pointer→idx
    // hashmap drops the per-symbol cost from O(N_files) to O(1).
    let symbol_file_targets: Arc<Vec<(tsz::binder::SymbolId, usize)>> = Arc::new({
        let _span = tracing::info_span!(
            "build_symbol_file_targets",
            symbols = program.symbol_arenas.len(),
            files = all_arenas.len()
        )
        .entered();
        let arena_ptr_to_idx: FxHashMap<*const tsz::parser::NodeArena, usize> = all_arenas
            .iter()
            .enumerate()
            .map(|(idx, arena)| (Arc::as_ptr(arena), idx))
            .collect();
        program
            .symbol_arenas
            .iter()
            .filter_map(|(sym_id, arena)| {
                arena_ptr_to_idx
                    .get(&Arc::as_ptr(arena))
                    .map(|&file_idx| (*sym_id, file_idx))
            })
            .collect()
    });

    // Create ModuleResolver instance for proper error reporting (TS2834, TS2835, TS2792, etc.)
    let mut module_resolver = ModuleResolver::new(options);

    // Build resolved_module_paths map: (source_file_idx, specifier) -> target_file_idx
    // Also build resolved_module_errors map for specific error codes
    let mut resolved_module_paths: FxHashMap<(usize, String), usize> = FxHashMap::default();
    let mut resolved_module_request_paths: FxHashMap<
        (
            usize,
            String,
            Option<tsz::checker::context::ResolutionModeOverride>,
        ),
        usize,
    > = FxHashMap::default();
    let mut resolved_module_specifiers: FxHashSet<(usize, String)> = FxHashSet::default();
    let mut resolved_module_errors: FxHashMap<
        (usize, String),
        tsz::checker::context::ResolutionError,
    > = FxHashMap::default();
    let mut resolved_module_request_errors: FxHashMap<
        (
            usize,
            String,
            Option<tsz::checker::context::ResolutionModeOverride>,
        ),
        tsz::checker::context::ResolutionError,
    > = FxHashMap::default();

    // Cache module specifiers per file — collected once, reused in prepare_binders
    // and check_file_for_parallel to avoid 3× redundant AST traversals.
    type CachedModuleSpecifier = (
        String,
        tsz::parser::NodeIndex,
        tsz::module_resolver::ImportKind,
        Option<tsz::module_resolver::ImportingModuleKind>,
    );

    // AST traversal is pure read-only and embarrassingly parallel: each file
    // independently scans its own arena. Doing this up-front in parallel lets
    // the subsequent (sequential) module-resolution loop iterate over a
    // pre-built `Vec<Vec<...>>` instead of interleaving the AST scan with
    // the resolution-cache mutation. On large repos this turns N sequential
    // AST passes into one N-way parallel pass.
    let cached_module_specifiers: Vec<Vec<CachedModuleSpecifier>> = {
        use rayon::prelude::*;
        let _span =
            tracing::info_span!("collect_module_specifiers", files = program.files.len()).entered();
        program
            .files
            .par_iter()
            .map(|file| collect_module_specifiers(&file.arena, file.source_file))
            .collect()
    };

    // Duplicate package redirect map
    let package_redirects: FxHashMap<PathBuf, PathBuf> = {
        let file_names: Vec<String> = program.files.iter().map(|f| f.file_name.clone()).collect();
        build_duplicate_package_redirects(&file_names, options)
    };
    {
        let _span = tracing::info_span!("build_resolved_module_maps").entered();
        for (file_idx, file) in program.files.iter().enumerate() {
            let file_path = Path::new(&file.file_name);

            for (specifier, specifier_node, import_kind, resolution_mode_override) in
                &cached_module_specifiers[file_idx]
            {
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
                    resolution_mode_override: *resolution_mode_override,
                    no_implicit_any: options.checker.no_implicit_any,
                    implied_classic_resolution: options.checker.implied_classic_resolution,
                };
                let request_mode_key = checker_lookup_resolution_mode(
                    &mut module_resolver,
                    options,
                    file_path,
                    *import_kind,
                    *resolution_mode_override,
                );

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
                    Some(&program_paths),
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
                // NOTE: Only mark as resolved if there's NO error. When there's a resolution
                // error (TS2307, etc.), the module should NOT be in resolved_module_specifiers
                // so that the checker will emit the appropriate error.
                if outcome.error.is_none() {
                    if let Some(ref resolved_path) = outcome.resolved_path {
                        resolved_module_specifiers.insert((file_idx, specifier.clone()));
                        let canonical = normalize_resolved_path(resolved_path, options);
                        // Apply duplicate package redirect
                        let canonical = if should_apply_duplicate_package_redirect(file_path) {
                            package_redirects
                                .get(&canonical)
                                .cloned()
                                .unwrap_or(canonical)
                        } else {
                            canonical
                        };
                        if let Some(&target_idx) = canonical_to_file_idx.get(&canonical) {
                            resolved_module_paths.insert((file_idx, specifier.clone()), target_idx);
                            resolved_module_request_paths.insert(
                                (file_idx, specifier.clone(), request_mode_key),
                                target_idx,
                            );
                        }
                    } else if outcome.is_resolved {
                        resolved_module_specifiers.insert((file_idx, specifier.clone()));
                    }
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
                    resolved_module_request_errors.insert(
                        (file_idx, specifier.clone(), request_mode_key),
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
    let resolved_module_request_paths = Arc::new(resolved_module_request_paths);
    let resolved_module_specifiers = Arc::new(resolved_module_specifiers);
    let resolved_module_errors = Arc::new(resolved_module_errors);
    let resolved_module_request_errors = Arc::new(resolved_module_request_errors);

    // Pre-bucket resolved-module specifiers by file_idx so each per-file
    // checker can look up its own set in O(1) instead of scanning the
    // entire cross-file `resolved_module_specifiers` map. The previous
    // pattern was `iter().filter(|(idx, _)| *idx == file_idx)` per file —
    // O(N_total_specifiers) per file → O(N_files × N_total_specifiers)
    // overall. On a 6086-file fixture with avg 20 imports per file
    // (~120 K total entries) that ballooned into ~700 M hashset
    // iterations across all checkers; the per-file checker scaled with
    // the size of the WHOLE program rather than its own import count.
    let resolved_modules_per_file: Arc<Vec<rustc_hash::FxHashSet<String>>> = Arc::new({
        let _span = tracing::info_span!(
            "bucket_resolved_modules_per_file",
            files = program.files.len()
        )
        .entered();
        let mut by_file: Vec<rustc_hash::FxHashSet<String>> = (0..program.files.len())
            .map(|_| FxHashSet::default())
            .collect();
        for (file_idx, specifier) in resolved_module_specifiers.iter() {
            if let Some(set) = by_file.get_mut(*file_idx) {
                set.insert(specifier.clone());
            }
        }
        by_file
    });

    // Pre-compute per-file TS7016 diagnostics for CJS require() calls.
    // The driver's resolution pass detects untyped JS modules (TS7016) but the
    // checker's module-not-found path skips them because the module DID resolve.
    // For CJS require() calls (not import declarations), we emit TS7016 directly.
    //
    // Pure read-only per-file work (arena + pre-computed maps), so Rayon can
    // spread the scan across cores. On large repos this turns an N-file
    // sequential post-pass into an N-way parallel pass.
    let per_file_ts7016_diagnostics: Vec<Vec<Diagnostic>> = {
        use rayon::prelude::*;
        let _span = tracing::info_span!("per_file_ts7016_diagnostics", files = program.files.len())
            .entered();
        program
            .files
            .par_iter()
            .enumerate()
            .map(|(file_idx, file)| {
                let mut diags = Vec::new();
                for (specifier, spec_node, import_kind, _) in &cached_module_specifiers[file_idx] {
                    if !matches!(import_kind, tsz::module_resolver::ImportKind::CjsRequire) {
                        continue;
                    }
                    if let Some(error) = resolved_module_errors.get(&(file_idx, specifier.clone()))
                    {
                        if error.code != 7016 {
                            continue;
                        }
                        // Find the string literal argument of the require() call for the span.
                        let (start, length) = if let Some(node) = file.arena.get(*spec_node)
                            && let Some(call) = file.arena.get_call_expr(node)
                            && let Some(args) = call.arguments.as_ref()
                            && let Some(&arg_idx) = args.nodes.first()
                            && let Some(arg_node) = file.arena.get(arg_idx)
                        {
                            (arg_node.pos, arg_node.end.saturating_sub(arg_node.pos))
                        } else if let Some(node) = file.arena.get(*spec_node) {
                            (node.pos, node.end.saturating_sub(node.pos))
                        } else {
                            continue;
                        };
                        diags.push(Diagnostic::error(
                            &file.file_name,
                            start,
                            length,
                            &error.message,
                            error.code,
                        ));
                    }
                }
                diags
            })
            .collect()
    };
    let per_file_ts7016_diagnostics = Arc::new(per_file_ts7016_diagnostics);

    // Pre-compute per-file ESM/CJS module kind for resolution modes that honor
    // package.json "type" semantics. The checker uses this shared map for
    // ESM-vs-CJS-sensitive diagnostics such as TS1479 and TS1192 suppression.
    let file_is_esm_map: Arc<FxHashMap<String, bool>> = Arc::new({
        let resolution_kind = options.effective_module_resolution();
        let uses_package_type_module_kind = matches!(
            resolution_kind,
            crate::config::ModuleResolutionKind::Bundler
                | crate::config::ModuleResolutionKind::Node16
                | crate::config::ModuleResolutionKind::NodeNext
        );
        if uses_package_type_module_kind {
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
    //
    // When no skeleton exists (small projects, sequential mode, tests), fall
    // back to the merged program-wide `declared_modules` set. This is the
    // same merged data the (now-empty) per-binder `declared_modules` used to
    // hold; consumers route through `ctx.declared_modules_contains` which
    // prefers `global_declared_modules`. Without this fallback, ambient
    // module suppression in `import_declaration` regresses (TS2307 false
    // positives on declared modules) for non-skeleton paths.
    let skeleton_declared_modules: Option<Arc<tsz::checker::context::GlobalDeclaredModules>> =
        if let Some(skel) = program.skeleton_index.as_ref() {
            let (exact, patterns) = skel.build_declared_module_sets();
            Some(Arc::new(
                tsz::checker::context::GlobalDeclaredModules::from_skeleton(exact, patterns),
            ))
        } else if !program.declared_modules.is_empty()
            || !program.shorthand_ambient_modules.is_empty()
        {
            let mut exact: FxHashSet<String> = FxHashSet::default();
            let mut patterns: Vec<String> = Vec::new();
            for name in program
                .declared_modules
                .iter()
                .chain(program.shorthand_ambient_modules.iter())
            {
                let normalized = name.trim_matches('"').trim_matches('\'');
                if normalized.contains('*') {
                    patterns.push(normalized.to_string());
                } else {
                    exact.insert(normalized.to_string());
                }
            }
            patterns.sort();
            patterns.dedup();
            Some(Arc::new(
                tsz::checker::context::GlobalDeclaredModules::from_skeleton(exact, patterns),
            ))
        } else {
            None
        };

    // Pre-compute expando index from skeleton when available.
    // This avoids re-scanning all binders for expando property assignments.
    let skeleton_expando_index: Option<Arc<FxHashMap<String, FxHashSet<String>>>> = program
        .skeleton_index
        .as_ref()
        .map(|skel| Arc::new(skel.expando_properties.clone()));

    // Build the project-wide shared environment once for all checkers (prime, parallel, sequential).
    // build_global_indices computes the 4 binder-derived indices once here so that
    // per-file checker creation via apply_to skips the O(N) binder scans.
    // Wrap the merged program-wide re-export maps in `Arc` once so N
    // cross-file lookup binders can share the single allocation instead of
    // each deep-cloning a copy. Cross-file consumers read these via
    // `ctx.reexports_for_file` / `wildcard_reexports_for_file`.
    // `program.reexports` is already `Arc`-wrapped on `MergedProgram`; cheap atomic clone.
    let program_reexports = Arc::clone(&program.reexports);
    // `program.wildcard_reexports` and `program.wildcard_reexports_type_only`
    // are already `Arc`-wrapped on `MergedProgram`; cheap atomic clone.
    let program_wildcard_reexports = Arc::clone(&program.wildcard_reexports);
    let program_wildcard_reexports_type_only = Arc::clone(&program.wildcard_reexports_type_only);
    // `program.module_exports` is already `Arc`-wrapped on `MergedProgram`;
    // cheap atomic clone for ProjectEnv install.
    let program_module_exports = Arc::clone(&program.module_exports);
    // Same rationale for `program.cross_file_node_symbols`: the outer
    // map is `FxHashMap<usize, Arc<…>>` (~24 bytes * N_files for the
    // entries plus hash overhead). Cloning into every one of N per-file
    // binders scales outer-map allocation with N². Wrap once here and
    // route consumers through `ctx.cross_file_node_symbols_for_arena`.
    let program_cross_file_node_symbols = Arc::new(program.cross_file_node_symbols.clone());
    // Same rationale for `program.alias_partners`: a single shared
    // FxHashMap<SymbolId, SymbolId> beats N per-binder deep-clones.
    let program_alias_partners = Arc::new(program.alias_partners.clone());

    let mut project_env = tsz::checker::context::ProjectEnv {
        lib_contexts: std::sync::Arc::new(checker_libs.contexts.clone()),
        all_arenas: Arc::clone(&all_arenas),
        all_binders: Arc::clone(&all_binders),
        skeleton_declared_modules,
        skeleton_expando_index,
        symbol_file_targets: Arc::clone(&symbol_file_targets),
        resolved_module_paths: Arc::clone(&resolved_module_paths),
        resolved_module_request_paths: Arc::clone(&resolved_module_request_paths),
        resolved_module_errors: Arc::clone(&resolved_module_errors),
        resolved_module_request_errors: Arc::clone(&resolved_module_request_errors),
        is_external_module_by_file: Arc::clone(&is_external_module_by_file),
        file_is_esm_map: Arc::clone(&file_is_esm_map),
        typescript_dom_replacement_globals,
        has_deprecation_diagnostics,
        program_reexports: Some(program_reexports),
        program_wildcard_reexports: Some(program_wildcard_reexports),
        program_wildcard_reexports_type_only: Some(program_wildcard_reexports_type_only),
        program_module_exports: Some(program_module_exports),
        program_cross_file_node_symbols: Some(program_cross_file_node_symbols),
        program_alias_partners: Some(program_alias_partners),
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

    // Create a shared DefinitionStore for all parallel checkers.
    // CRITICAL: All parallel checkers MUST share the same DefinitionStore so that
    // DefId allocation is globally unique. Without this, independent DefId sequences
    // in separate checkers cause TypeId collisions via Lazy(DefId) interning.
    {
        let mut all_semantic_defs = program.semantic_defs.clone();
        for file in &program.files {
            for (sym_id, entry) in &file.semantic_defs {
                all_semantic_defs.insert(*sym_id, entry.clone());
            }
        }
        let shared_store = Arc::new(tsz_solver::def::DefinitionStore::from_semantic_defs(
            &all_semantic_defs,
            |s| program.type_interner.intern_string(s),
        ));
        shared_store.init_file_locks(program.files.len());
        project_env.shared_definition_store = Some(shared_store);
    }

    // Prime Array<T> base type with global augmentations before any file checks.
    // The prime checker uses the shared DefinitionStore (via project_env.apply_to).
    if !program.files.is_empty() && !checker_libs.contexts.is_empty() {
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

    let baseline_lib_diagnostics = if !options.no_check && !checker_libs.files.is_empty() {
        collect_checker_lib_baseline_fingerprints(
            program,
            options,
            checker_libs,
            &affected_lib_interfaces,
            &affected_lib_extension_interfaces,
            &project_env,
        )
    } else {
        FxHashSet::default()
    };

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
    let checker_lib_file_env = CheckerLibFileCheckEnv {
        program,
        options,
        checker_libs,
        affected_interfaces: &affected_lib_interfaces,
        extension_interfaces: &affected_lib_extension_interfaces,
        merged_augmentations: &merged_augmentations,
        project_env: &project_env,
        program_has_real_syntax_errors,
    };

    if cache.is_none() {
        // --- PARALLEL PATH: No cache, check all files concurrently ---
        let _parallel_span =
            tracing::info_span!("parallel_check_files", files = work_queue.len()).entered();

        // Pre-compute per-file module bridging — parallel across files since
        // each binder is constructed independently from the shared
        // `program`/`merged_augmentations`/`cached_module_specifiers`/
        // `resolved_module_paths` borrows. On large-ts-repo (6086 files) this
        // moves the prepare phase from sequential to N-way parallel, since
        // the bottleneck is symbol-table cloning per binder rather than IO.
        let per_file_binders: Vec<BinderState> = {
            use rayon::prelude::*;
            let _prep_span = tracing::info_span!("prepare_binders").entered();
            work_queue
                .par_iter()
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
                    for (specifier, _, _, _) in &cached_module_specifiers[file_idx] {
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
        // `TypeCache` is consumed by the emit pipeline (JS or declaration
        // files). For a pure `--noEmit` run that does not also request
        // declarations the cache is never read, but extracting one per file
        // pins several hash maps per file in memory throughout the whole
        // check — on a 6000-file repo that grew to ~10 GB RSS and got the
        // process killed by macOS jetsam before any diagnostics emitted.
        // Skip extraction in that case and let per-file state drop as soon
        // as checking finishes.
        let extract_type_cache = !options.no_emit || options.emit_declarations;
        let shared_lib_cache: Arc<dashmap::DashMap<String, Option<tsz_solver::TypeId>>> =
            Arc::new(dashmap::DashMap::new());

        // Create shared cross-file query cache for multi-file projects.
        // Eliminates redundant type evaluations and relation checks across files.
        let shared_query_cache = if work_items.len() > 1 {
            Some(tsz_solver::SharedQueryCache::new())
        } else {
            None
        };

        // Check all files in parallel — each file gets its own CheckerState and QueryCache.
        // TypeInterner (DashMap) is thread-safe; QueryCache uses RefCell/Cell per-thread.
        #[cfg(not(target_arch = "wasm32"))]
        let file_results: Vec<CheckFileResult> = {
            use rayon::iter::{
                IndexedParallelIterator, IntoParallelIterator, IntoParallelRefIterator,
                ParallelIterator,
            };
            // Use sequential checking for small projects (<=8 files) to avoid
            // non-deterministic false positives from concurrent type interning.
            // The TypeInterner uses DashMap for thread-safe access, but concurrent
            // type evaluation can produce different results depending on thread
            // scheduling when multiple files resolve the same lib or package
            // declaration types. Sequential checking is also faster for small
            // projects due to avoiding rayon thread pool overhead.
            if work_items.len() <= 8 {
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
                            resolved_modules_per_file: &resolved_modules_per_file,
                            shared_lib_cache: Arc::clone(&shared_lib_cache),
                            shared_query_cache: shared_query_cache.as_ref(),
                            no_check,
                            check_js,
                            explicit_check_js_false,
                            skip_lib_check,
                            program_has_real_syntax_errors,
                            extract_type_cache,
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
                            resolved_modules_per_file: &resolved_modules_per_file,
                            shared_lib_cache: Arc::clone(&shared_lib_cache),
                            shared_query_cache: shared_query_cache.as_ref(),
                            no_check,
                            check_js,
                            explicit_check_js_false,
                            skip_lib_check,
                            program_has_real_syntax_errors,
                            extract_type_cache,
                        };
                        check_file_for_parallel(context)
                    })
                    .collect()
            }
        };

        #[cfg(target_arch = "wasm32")]
        let file_results: Vec<CheckFileResult> = work_items
            .iter()
            .zip(per_file_binders.into_iter())
            .map(|(&file_idx, binder)| {
                let context = CheckFileForParallelContext {
                    file_idx,
                    binder,
                    program,
                    compiler_options: &compiler_options,
                    project_env: &project_env,
                    resolved_modules_per_file: &resolved_modules_per_file,
                    shared_lib_cache: Arc::clone(&shared_lib_cache),
                    shared_query_cache: shared_query_cache.as_ref(),
                    no_check,
                    check_js,
                    explicit_check_js_false,
                    skip_lib_check,
                    program_has_real_syntax_errors,
                    extract_type_cache,
                };
                check_file_for_parallel(context)
            })
            .collect();

        // Aggregate per-file query cache statistics. DefinitionStore stats
        // come from the shared store computed once after the loop (workers
        // all see the same shared store, so summing per-file was both
        // wasted work and N× inflated).
        let mut parallel_qc_stats = tsz_solver::QueryCacheStatistics::default();
        let parallel_ds_stats = tsz_solver::StoreStatistics::default();
        {
            let mut tc_out = type_cache_output
                .lock()
                .expect("type_cache_output mutex poisoned");
            for (idx, (file_diags, type_cache, file_counters, qc_stats, _ds_stats)) in
                file_results.into_iter().enumerate()
            {
                diagnostics.extend(file_diags);
                // Inject pre-computed TS7016 diagnostics for CJS require() calls.
                let file_idx = work_items[idx];
                diagnostics.extend(per_file_ts7016_diagnostics[file_idx].iter().cloned());
                request_cache_counters.merge(file_counters);
                parallel_qc_stats.merge(&qc_stats);
                if let Some(tc) = type_cache {
                    let file_path = PathBuf::from(&program.files[file_idx].file_name);
                    tc_out.insert(file_path, tc);
                }
            }
        }
        if !options.no_check {
            for lib_idx in 0..checker_libs.files.len() {
                let query_cache = if let Some(shared) = shared_query_cache.as_ref() {
                    QueryCache::new_with_shared(&program.type_interner, shared)
                } else {
                    QueryCache::new(&program.type_interner)
                };
                let (lib_diags, lib_counters, _lib_ds_stats) = check_checker_lib_file(
                    &checker_lib_file_env,
                    lib_idx,
                    &query_cache,
                    Some(Arc::clone(&shared_lib_cache)),
                );
                let mut lib_diags = lib_diags;
                retain_program_induced_lib_diagnostics(&mut lib_diags, &baseline_lib_diagnostics);
                diagnostics.extend(lib_diags);
                request_cache_counters.merge(lib_counters);
                parallel_qc_stats.merge(&query_cache.statistics());
            }
        }
        aggregated_qc_stats = Some(parallel_qc_stats);
        aggregated_ds_stats = project_env
            .shared_definition_store
            .as_ref()
            .map(|store| store.statistics())
            .or(Some(parallel_ds_stats));
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

            let mut binder = create_binder_from_bound_file_with_augmentations(
                file,
                program,
                file_idx,
                &merged_augmentations,
            );

            // Use cached specifiers from build_resolved_module_maps.
            let module_specifiers = &cached_module_specifiers[file_idx];

            // Bridge multi-file module resolution for ES module imports.
            for (specifier, _, _, _) in module_specifiers {
                if let Some(resolved) = resolve_module_specifier(
                    Path::new(&file.file_name),
                    specifier,
                    options,
                    base_dir,
                    &mut resolution_cache,
                    &program_paths,
                ) {
                    let canonical = normalize_resolved_path(&resolved, options);
                    let canonical = if should_apply_duplicate_package_redirect(&file_path) {
                        package_redirects
                            .get(&canonical)
                            .cloned()
                            .unwrap_or(canonical)
                    } else {
                        canonical
                    };
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

            // Use the per-file pre-bucketed map; see the parallel path for the
            // O(N²) → O(1) rationale.
            let resolved_modules: rustc_hash::FxHashSet<String> = resolved_modules_per_file
                .get(file_idx)
                .cloned()
                .unwrap_or_default();
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
            // Collect positions of nullable-type syntax errors (`?T`/`T?`) for TS2677
            // widening. Exclude `!T`/`T!` errors which share the same error codes
            // (17019/17020) but should not trigger predicate type widening.
            checker.ctx.nullable_type_parse_error_positions = file
                .parse_diagnostics
                .iter()
                .filter(|d| (d.code == 17019 || d.code == 17020) && d.message.contains("'?'"))
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
            checker.ctx.has_structural_parse_errors = file
                .parse_diagnostics
                .iter()
                .any(|d| is_structural_parse_error(d.code));
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
                    has_deprecation_diagnostics,
                );

                file_diagnostics.extend(checker_diagnostics);
            }

            // Inject pre-computed TS7016 diagnostics for CJS require() calls.
            file_diagnostics.extend(per_file_ts7016_diagnostics[file_idx].iter().cloned());

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
                    options.emit_declarations && options.check_js && is_js,
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
        if !options.no_check {
            for lib_idx in 0..checker_libs.files.len() {
                let (lib_diags, lib_counters, lib_ds_stats) =
                    check_checker_lib_file(&checker_lib_file_env, lib_idx, &query_cache, None);
                let mut lib_diags = lib_diags;
                retain_program_induced_lib_diagnostics(&mut lib_diags, &baseline_lib_diagnostics);
                diagnostics.extend(lib_diags);
                request_cache_counters.merge(lib_counters);
                sequential_ds_stats.merge(&lib_ds_stats);
            }
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
            Arc::make_mut(&mut binder.module_exports).insert(current_specifier.clone(), exports);
        }
        if let Some(wildcards) = program.wildcard_reexports.get(target_file_name).cloned() {
            Arc::make_mut(&mut binder.wildcard_reexports)
                .insert(current_specifier.clone(), wildcards.clone());
        }
        if let Some(type_only_flags) = program
            .wildcard_reexports_type_only
            .get(target_file_name)
            .cloned()
        {
            Arc::make_mut(&mut binder.wildcard_reexports_type_only)
                .insert(current_specifier.clone(), type_only_flags);
        }
        if let Some(reexports) = program.reexports.get(target_file_name).cloned() {
            Arc::make_mut(&mut binder.reexports).insert(current_specifier.clone(), reexports);
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
    /// Per-file pre-bucketed resolved module specifiers (indexed by `file_idx`).
    /// Replaces a previous per-file scan over the program-wide
    /// `resolved_module_specifiers` set, which made each per-file checker
    /// scale with the size of the WHOLE program rather than its own
    /// import count.
    resolved_modules_per_file: &'a Arc<Vec<rustc_hash::FxHashSet<String>>>,
    shared_lib_cache: Arc<dashmap::DashMap<String, Option<tsz_solver::TypeId>>>,
    /// Shared cross-file query cache for multi-file projects.
    /// Eliminates redundant type evaluations and relation checks across files.
    shared_query_cache: Option<&'a tsz_solver::SharedQueryCache>,
    no_check: bool,
    check_js: bool,
    /// `true` when `checkJs: false` was explicitly specified in compiler options.
    /// When set, ALL semantic errors are suppressed for JS files, including the
    /// `plainJSErrors` allowlist that would otherwise survive the filter.
    explicit_check_js_false: bool,
    skip_lib_check: bool,
    program_has_real_syntax_errors: bool,
    /// When `false`, per-file `TypeCache` extraction is skipped entirely.
    /// `TypeCache` is used by the emit pipeline (JS / declaration files) and
    /// by incremental cache reuse. For a `--noEmit` run that does not also
    /// request `--declaration`, nothing consumes it, and extracting it for
    /// every one of N files pins several hash maps per file in memory
    /// throughout the whole check (observed at ~10 GB RSS peak on a
    /// 6000-file repo). Set this `false` in that case.
    extract_type_cache: bool,
}

/// Result of checking a single file for the parallel checking path: diagnostics,
/// optional `TypeCache` snapshot, per-file request counters, and solver
/// query-cache / definition-store statistics aggregated by the caller.
pub(super) type CheckFileResult = (
    Vec<Diagnostic>,
    Option<TypeCache>,
    RequestCacheCounters,
    tsz_solver::QueryCacheStatistics,
    tsz_solver::StoreStatistics,
);

/// Check a single file for the parallel checking path.
///
/// This is extracted from the work queue loop so it can be called from rayon's `par_iter`.
/// Each invocation creates its own `CheckerState` (with its own mutable context)
/// and its own `QueryCache` (using `RefCell`/`Cell` for zero-overhead single-threaded caching).
/// The `TypeInterner` is shared across threads via `DashMap` (thread-safe).
pub(super) fn check_file_for_parallel<'a>(
    context: CheckFileForParallelContext<'a>,
) -> CheckFileResult {
    let CheckFileForParallelContext {
        file_idx,
        binder,
        program,
        compiler_options,
        project_env,
        resolved_modules_per_file,
        shared_lib_cache,
        shared_query_cache,
        no_check,
        check_js,
        explicit_check_js_false,
        skip_lib_check,
        program_has_real_syntax_errors,
        extract_type_cache,
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
    // For multi-file projects, use shared L2 cache to avoid redundant computation.
    let query_cache = if let Some(shared) = shared_query_cache {
        QueryCache::new_with_shared(&program.type_interner, shared)
    } else {
        QueryCache::new(&program.type_interner)
    };

    // Use the pre-bucketed `resolved_modules_per_file[file_idx]` instead of
    // re-filtering the program-wide cross-file set per file. The bucketed
    // version is built once in `collect_diagnostics` and shared via `Arc`.
    let resolved_modules: FxHashSet<String> = resolved_modules_per_file
        .get(file_idx)
        .cloned()
        .unwrap_or_default();

    // apply_to (below) installs the project-wide shared DefinitionStore and
    // warms the per-file caches from it. Use the deferred constructor so we
    // don't build a throwaway per-file store first — that work showed up in
    // profiles as a non-trivial fraction of total CPU on large projects, all
    // of it overwritten moments later.
    let mut checker = CheckerState::with_options_deferred_def_store(
        &file.arena,
        &binder,
        &query_cache,
        file.file_name.clone(),
        compiler_options,
    );
    checker.ctx.report_unresolved_imports = true;
    checker.ctx.shared_lib_type_cache = Some(shared_lib_cache);

    // Apply all project-level shared state in one call. This installs the
    // shared DefinitionStore and runs warm_local_caches_from_shared_store().
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
    checker.ctx.nullable_type_parse_error_positions = file
        .parse_diagnostics
        .iter()
        .filter(|d| (d.code == 17019 || d.code == 17020) && d.message.contains("'?'"))
        .map(|d| d.start)
        .collect();
    checker.ctx.has_real_syntax_errors = file
        .parse_diagnostics
        .iter()
        .any(|d| is_real_syntax_error(d.code));
    checker.ctx.has_structural_parse_errors = file
        .parse_diagnostics
        .iter()
        .any(|d| is_structural_parse_error(d.code));
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
            project_env.has_deprecation_diagnostics,
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
            compiler_options.emit_declarations && check_js && is_js,
        );
    }

    let checker_counters = checker.ctx.request_cache_counters;
    let qc_stats = query_cache.statistics();
    // Skip per-file DefinitionStore statistics: in the parallel path all
    // checkers share the same store, so every worker would report the same
    // numbers and the aggregator was summing them N times (both wasted work
    // and inflated counts). The aggregator computes stats once on the
    // shared store after the work loop completes.
    let ds_stats = tsz_solver::StoreStatistics::default();
    let type_cache = if extract_type_cache {
        Some(checker.extract_cache())
    } else {
        None
    };
    (
        file_diagnostics,
        type_cache,
        checker_counters,
        qc_stats,
        ds_stats,
    )
}

struct CheckerLibFileCheckEnv<'a> {
    program: &'a MergedProgram,
    options: &'a ResolvedCompilerOptions,
    checker_libs: &'a CheckerLibSet,
    affected_interfaces: &'a FxHashSet<String>,
    extension_interfaces: &'a FxHashSet<String>,
    merged_augmentations: &'a MergedAugmentations,
    project_env: &'a tsz::checker::context::ProjectEnv,
    program_has_real_syntax_errors: bool,
}

fn check_checker_lib_file(
    env: &CheckerLibFileCheckEnv<'_>,
    lib_idx: usize,
    query_cache: &QueryCache,
    shared_lib_cache: Option<Arc<dashmap::DashMap<String, Option<tsz_solver::TypeId>>>>,
) -> (
    Vec<Diagnostic>,
    RequestCacheCounters,
    tsz_solver::StoreStatistics,
) {
    let program = env.program;
    let options = env.options;
    let checker_libs = env.checker_libs;
    let lib_file = &checker_libs.files[lib_idx];
    if should_skip_type_checking_for_file(&lib_file.file_name, options, true) {
        return (
            Vec::new(),
            RequestCacheCounters::default(),
            tsz_solver::StoreStatistics::default(),
        );
    }

    let lib_bound_file =
        build_lib_bound_file_for_interface_checks(program, lib_file, env.affected_interfaces);
    let binder = create_binder_from_bound_file_with_augmentations(
        &lib_bound_file,
        program,
        program.files.len(),
        env.merged_augmentations,
    );

    let mut checker = CheckerState::with_options(
        &lib_bound_file.arena,
        &binder,
        query_cache,
        lib_bound_file.file_name.clone(),
        &options.checker,
    );
    env.project_env.apply_to(&mut checker.ctx);
    if let Some(shared_lib_cache) = shared_lib_cache {
        checker.ctx.shared_lib_type_cache = Some(shared_lib_cache);
    }

    // The current lib file is already the primary binder for this checker. Exclude it from the
    // fallback lib contexts to avoid duplicate lib declarations during cross-file lookup while
    // still preserving the total count used for capability checks.
    let other_lib_contexts: Vec<LibContext> = checker_libs
        .contexts
        .iter()
        .enumerate()
        .filter(|(idx, _)| *idx != lib_idx)
        .map(|(_, ctx)| ctx.clone())
        .collect();
    checker.ctx.set_lib_contexts(other_lib_contexts);
    checker
        .ctx
        .set_actual_lib_file_count(checker_libs.contexts.len());
    checker.prime_boxed_types();

    // Lib files have no parser diagnostics in the checker pass, but semantic diagnostics from
    // them should still respect tsc's global syntax-error suppression policy.
    checker.ctx.has_parse_errors = env.program_has_real_syntax_errors;
    tsz::checker::reset_stack_overflow_flag();
    checker.check_source_file_interfaces_only_filtered_post_merge(
        lib_bound_file.source_file,
        env.affected_interfaces,
        env.extension_interfaces,
    );

    let mut diagnostics = std::mem::take(&mut checker.ctx.diagnostics);
    if env.program_has_real_syntax_errors {
        diagnostics
            .retain(|diag| keep_checker_diagnostic_when_program_has_real_syntax_errors(diag.code));
    }
    diagnostics.sort_by(|a, b| a.start.cmp(&b.start).then_with(|| a.code.cmp(&b.code)));
    diagnostics.dedup_by(|a, b| a.start == b.start && a.code == b.code);

    (
        diagnostics,
        checker.ctx.request_cache_counters,
        checker.ctx.definition_store.statistics(),
    )
}

fn check_checker_lib_file_baseline(
    project_env: &tsz::checker::context::ProjectEnv,
    options: &ResolvedCompilerOptions,
    checker_libs: &CheckerLibSet,
    lib_idx: usize,
    affected_interfaces: &FxHashSet<String>,
    extension_interfaces: &FxHashSet<String>,
    query_cache: &QueryCache,
) -> (
    Vec<Diagnostic>,
    RequestCacheCounters,
    tsz_solver::StoreStatistics,
) {
    let lib_file = &checker_libs.files[lib_idx];
    if should_skip_type_checking_for_file(&lib_file.file_name, options, true) {
        return (
            Vec::new(),
            RequestCacheCounters::default(),
            tsz_solver::StoreStatistics::default(),
        );
    }

    let mut checker = CheckerState::with_options(
        &lib_file.arena,
        lib_file.binder.as_ref(),
        query_cache,
        lib_file.file_name.clone(),
        &options.checker,
    );
    project_env.apply_to(&mut checker.ctx);
    let other_lib_contexts: Vec<LibContext> = checker_libs
        .contexts
        .iter()
        .enumerate()
        .filter(|(idx, _)| *idx != lib_idx)
        .map(|(_, ctx)| ctx.clone())
        .collect();
    checker.ctx.set_lib_contexts(other_lib_contexts);
    checker
        .ctx
        .set_actual_lib_file_count(checker_libs.contexts.len());
    checker.prime_boxed_types();
    tsz::checker::reset_stack_overflow_flag();
    checker.check_source_file_interfaces_only_filtered_post_merge(
        lib_file.root_index,
        affected_interfaces,
        extension_interfaces,
    );

    let mut diagnostics = std::mem::take(&mut checker.ctx.diagnostics);
    diagnostics.sort_by(|a, b| a.start.cmp(&b.start).then_with(|| a.code.cmp(&b.code)));
    diagnostics.dedup_by(|a, b| a.start == b.start && a.code == b.code);

    (
        diagnostics,
        checker.ctx.request_cache_counters,
        checker.ctx.definition_store.statistics(),
    )
}

fn collect_lib_interface_node_symbols(
    arena: &NodeArena,
    statements: &[NodeIndex],
    globals: &SymbolTable,
    affected_interfaces: &FxHashSet<String>,
    node_symbols: &mut FxHashMap<u32, tsz::binder::SymbolId>,
) {
    for &stmt_idx in statements {
        let Some(stmt_node) = arena.get(stmt_idx) else {
            continue;
        };

        if stmt_node.kind == tsz::parser::syntax_kind_ext::INTERFACE_DECLARATION {
            if let Some(interface) = arena.get_interface(stmt_node)
                && let Some(name) = arena.get_identifier_at(interface.name)
                && affected_interfaces.contains(&name.escaped_text)
                && let Some(sym_id) = globals.get(&name.escaped_text)
            {
                node_symbols.insert(stmt_idx.0, sym_id);
                node_symbols.insert(interface.name.0, sym_id);
                if let Some(heritage_clauses) = &interface.heritage_clauses {
                    for &clause_idx in &heritage_clauses.nodes {
                        let Some(clause_node) = arena.get(clause_idx) else {
                            continue;
                        };
                        let Some(heritage) = arena.get_heritage_clause(clause_node) else {
                            continue;
                        };
                        if heritage.token != tsz_scanner::SyntaxKind::ExtendsKeyword as u16 {
                            continue;
                        }
                        for &type_idx in &heritage.types.nodes {
                            let Some(type_node) = arena.get(type_idx) else {
                                continue;
                            };
                            let expr_idx = if let Some(expr_type_args) =
                                arena.get_expr_type_args(type_node)
                            {
                                expr_type_args.expression
                            } else if type_node.kind == tsz::parser::syntax_kind_ext::TYPE_REFERENCE
                            {
                                arena
                                    .get_type_ref(type_node)
                                    .map_or(type_idx, |type_ref| type_ref.type_name)
                            } else {
                                type_idx
                            };
                            if let Some(base_name) = entity_name_text_in_arena(arena, expr_idx)
                                && let Some(base_sym_id) = globals.get(&base_name)
                            {
                                node_symbols.insert(expr_idx.0, base_sym_id);
                            }
                        }
                    }
                }
            }
            continue;
        }

        if stmt_node.kind != tsz::parser::syntax_kind_ext::MODULE_DECLARATION {
            continue;
        }

        let Some(module_decl) = arena.get_module(stmt_node) else {
            continue;
        };
        if module_decl.body.is_none() {
            continue;
        }
        let Some(body_node) = arena.get(module_decl.body) else {
            continue;
        };
        if body_node.kind != tsz::parser::syntax_kind_ext::MODULE_BLOCK {
            continue;
        }
        let Some(block) = arena.get_module_block(body_node) else {
            continue;
        };
        let Some(inner) = &block.statements else {
            continue;
        };
        collect_lib_interface_node_symbols(
            arena,
            &inner.nodes,
            globals,
            affected_interfaces,
            node_symbols,
        );
    }
}

fn interface_name_text(arena: &NodeArena, stmt_idx: NodeIndex) -> Option<String> {
    let node = arena.get(stmt_idx)?;
    let interface = arena.get_interface(node)?;
    let ident = arena.get_identifier_at(interface.name)?;
    Some(ident.escaped_text.clone())
}

fn entity_name_text_in_arena(arena: &NodeArena, idx: NodeIndex) -> Option<String> {
    let node = arena.get(idx)?;
    if node.kind == tsz::parser::syntax_kind_ext::TYPE_REFERENCE
        && let Some(type_ref) = arena.get_type_ref(node)
    {
        return entity_name_text_in_arena(arena, type_ref.type_name);
    }
    if node.kind == tsz_scanner::SyntaxKind::Identifier as u16 {
        return arena
            .get_identifier(node)
            .map(|ident| ident.escaped_text.clone());
    }
    if node.kind == tsz::parser::syntax_kind_ext::QUALIFIED_NAME {
        let qn = arena.get_qualified_name(node)?;
        let left = entity_name_text_in_arena(arena, qn.left)?;
        let right = entity_name_text_in_arena(arena, qn.right)?;
        return Some(format!("{left}.{right}"));
    }
    if node.kind == tsz::parser::syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
        && let Some(access) = arena.get_access_expr(node)
    {
        let left = entity_name_text_in_arena(arena, access.expression)?;
        let right = arena
            .get(access.name_or_argument)
            .and_then(|right_node| arena.get_identifier(right_node))?;
        return Some(format!("{left}.{}", right.escaped_text));
    }
    None
}

fn collect_direct_base_names(
    arena: &NodeArena,
    interface: &tsz_parser::parser::node::InterfaceData,
) -> Vec<String> {
    let Some(heritage_clauses) = &interface.heritage_clauses else {
        return Vec::new();
    };

    let mut names = Vec::new();
    for &clause_idx in &heritage_clauses.nodes {
        let Some(clause_node) = arena.get(clause_idx) else {
            continue;
        };
        let Some(heritage) = arena.get_heritage_clause(clause_node) else {
            continue;
        };
        if heritage.token != tsz_scanner::SyntaxKind::ExtendsKeyword as u16 {
            continue;
        }
        for &type_idx in &heritage.types.nodes {
            let Some(type_node) = arena.get(type_idx) else {
                continue;
            };
            let expr_idx = if let Some(expr_type_args) = arena.get_expr_type_args(type_node) {
                expr_type_args.expression
            } else if type_node.kind == tsz::parser::syntax_kind_ext::TYPE_REFERENCE {
                arena
                    .get_type_ref(type_node)
                    .map_or(type_idx, |type_ref| type_ref.type_name)
            } else {
                type_idx
            };
            if let Some(name) = entity_name_text_in_arena(arena, expr_idx) {
                names.push(name);
            }
        }
    }
    names
}

fn collect_user_global_interface_seeds(program: &MergedProgram) -> FxHashSet<String> {
    let mut seeds = FxHashSet::default();

    for file in &program.files {
        if !file.is_external_module
            && let Some(source_file) = file.arena.get_source_file_at(file.source_file)
        {
            for &stmt_idx in &source_file.statements.nodes {
                if let Some(name) = interface_name_text(file.arena.as_ref(), stmt_idx) {
                    seeds.insert(name);
                }
            }
        }

        for name in file.global_augmentations.keys() {
            seeds.insert(name.clone());
        }
    }

    seeds
}

fn member_name_text(arena: &NodeArena, member_idx: NodeIndex) -> Option<String> {
    let member_node = arena.get(member_idx)?;
    if let Some(sig) = arena.get_signature(member_node) {
        return arena
            .get(sig.name)
            .and_then(|name_node| arena.get_identifier(name_node))
            .map(|ident| ident.escaped_text.clone());
    }
    if let Some(accessor) = arena.get_accessor(member_node) {
        return arena
            .get(accessor.name)
            .and_then(|name_node| arena.get_identifier(name_node))
            .map(|ident| ident.escaped_text.clone());
    }
    None
}

fn collect_user_global_interface_member_names(program: &MergedProgram) -> FxHashSet<String> {
    let mut member_names = FxHashSet::default();

    for file in &program.files {
        if file.is_external_module {
            continue;
        }
        let Some(source_file) = file.arena.get_source_file_at(file.source_file) else {
            continue;
        };
        for &stmt_idx in &source_file.statements.nodes {
            let Some(stmt_node) = file.arena.get(stmt_idx) else {
                continue;
            };
            let Some(interface) = file.arena.get_interface(stmt_node) else {
                continue;
            };
            for &member_idx in &interface.members.nodes {
                if let Some(name) = member_name_text(file.arena.as_ref(), member_idx) {
                    member_names.insert(name);
                }
            }
        }
    }

    member_names
}

fn add_user_global_interface_declaration_arenas(
    program: &MergedProgram,
    declaration_arenas: &mut tsz::binder::state::DeclarationArenaMap,
) {
    for file in &program.files {
        if file.is_external_module {
            continue;
        }
        let Some(source_file) = file.arena.get_source_file_at(file.source_file) else {
            continue;
        };
        for &stmt_idx in &source_file.statements.nodes {
            let Some(name) = interface_name_text(file.arena.as_ref(), stmt_idx) else {
                continue;
            };
            let Some(sym_id) = program.globals.get(&name) else {
                continue;
            };
            let target = declaration_arenas.entry((sym_id, stmt_idx)).or_default();
            if !target.iter().any(|arena| Arc::ptr_eq(arena, &file.arena)) {
                target.push(Arc::clone(&file.arena));
            }
        }
    }
}

fn type_node_contains_tag_name_map_indexed_access(
    arena: &NodeArena,
    type_idx: NodeIndex,
    fuel: &mut u32,
) -> bool {
    if type_idx == NodeIndex::NONE || *fuel == 0 {
        return false;
    }
    *fuel -= 1;

    let Some(node) = arena.get(type_idx) else {
        return false;
    };
    if node.kind == tsz::parser::syntax_kind_ext::INDEXED_ACCESS_TYPE {
        return arena
            .get_indexed_access_type(node)
            .and_then(|indexed| entity_name_text_in_arena(arena, indexed.object_type))
            .is_some_and(|name| name.contains("TagNameMap"));
    }

    if let Some(type_ref) = arena.get_type_ref(node) {
        return type_ref.type_arguments.as_ref().is_some_and(|args| {
            args.nodes
                .iter()
                .any(|&arg| type_node_contains_tag_name_map_indexed_access(arena, arg, fuel))
        });
    }
    if let Some(composite) = arena.get_composite_type(node) {
        return composite
            .types
            .nodes
            .iter()
            .any(|&ty| type_node_contains_tag_name_map_indexed_access(arena, ty, fuel));
    }
    if let Some(array) = arena.get_array_type(node) {
        return type_node_contains_tag_name_map_indexed_access(arena, array.element_type, fuel);
    }
    if let Some(wrapped) = arena.get_wrapped_type(node) {
        return type_node_contains_tag_name_map_indexed_access(arena, wrapped.type_node, fuel);
    }
    if let Some(type_operator) = arena.get_type_operator(node) {
        return type_node_contains_tag_name_map_indexed_access(
            arena,
            type_operator.type_node,
            fuel,
        );
    }
    if let Some(function_type) = arena.get_function_type(node) {
        if type_node_contains_tag_name_map_indexed_access(
            arena,
            function_type.type_annotation,
            fuel,
        ) {
            return true;
        }
        for &param_idx in &function_type.parameters.nodes {
            let Some(param_node) = arena.get(param_idx) else {
                continue;
            };
            let Some(param) = arena.get_parameter(param_node) else {
                continue;
            };
            if type_node_contains_tag_name_map_indexed_access(arena, param.type_annotation, fuel) {
                return true;
            }
        }
    }
    if let Some(conditional) = arena.get_conditional_type(node) {
        return [
            conditional.check_type,
            conditional.extends_type,
            conditional.true_type,
            conditional.false_type,
        ]
        .into_iter()
        .any(|ty| type_node_contains_tag_name_map_indexed_access(arena, ty, fuel));
    }

    false
}

fn interface_declares_member_named(
    arena: &NodeArena,
    interface: &tsz_parser::parser::node::InterfaceData,
    member_names: &FxHashSet<String>,
) -> bool {
    !member_names.is_empty()
        && interface.members.nodes.iter().any(|&member_idx| {
            member_name_text(arena, member_idx).is_some_and(|name| member_names.contains(&name))
        })
}

fn interface_has_indexed_access_member_type(
    arena: &NodeArena,
    interface: &tsz_parser::parser::node::InterfaceData,
) -> bool {
    for &member_idx in &interface.members.nodes {
        let Some(member_node) = arena.get(member_idx) else {
            continue;
        };
        if let Some(sig) = arena.get_signature(member_node) {
            let mut fuel = 256;
            if type_node_contains_tag_name_map_indexed_access(arena, sig.type_annotation, &mut fuel)
            {
                return true;
            }
            for &param_idx in sig.parameters.as_ref().map_or(&[][..], |p| &p.nodes) {
                let Some(param_node) = arena.get(param_idx) else {
                    continue;
                };
                let Some(param) = arena.get_parameter(param_node) else {
                    continue;
                };
                let mut fuel = 256;
                if type_node_contains_tag_name_map_indexed_access(
                    arena,
                    param.type_annotation,
                    &mut fuel,
                ) {
                    return true;
                }
            }
        }
        if let Some(accessor) = arena.get_accessor(member_node) {
            let mut fuel = 256;
            if type_node_contains_tag_name_map_indexed_access(
                arena,
                accessor.type_annotation,
                &mut fuel,
            ) {
                return true;
            }
        }
    }

    false
}

fn affected_lib_interface_names(
    program: &MergedProgram,
    checker_libs: &CheckerLibSet,
) -> FxHashSet<String> {
    let seed_interfaces = collect_user_global_interface_seeds(program);
    let mut affected = seed_interfaces.clone();
    let user_member_names = collect_user_global_interface_member_names(program);
    let mut inheritance_graph: FxHashMap<String, FxHashSet<String>> = FxHashMap::default();

    for lib in &checker_libs.files {
        let Some(source_file) = lib.arena.get_source_file_at(lib.root_index) else {
            continue;
        };
        for &stmt_idx in &source_file.statements.nodes {
            let Some(stmt_node) = lib.arena.get(stmt_idx) else {
                continue;
            };
            let Some(interface) = lib.arena.get_interface(stmt_node) else {
                continue;
            };
            let Some(name) = interface_name_text(lib.arena.as_ref(), stmt_idx) else {
                continue;
            };
            let bases = collect_direct_base_names(lib.arena.as_ref(), interface);
            inheritance_graph.entry(name).or_default().extend(bases);
        }
    }

    let mut changed = true;
    while changed {
        changed = false;
        for (name, bases) in &inheritance_graph {
            if affected.contains(name) {
                continue;
            }
            if bases.iter().any(|base| affected.contains(base)) {
                changed = affected.insert(name.clone());
            }
        }
    }

    let mut relevant = FxHashSet::default();
    for lib in &checker_libs.files {
        let Some(source_file) = lib.arena.get_source_file_at(lib.root_index) else {
            continue;
        };
        for &stmt_idx in &source_file.statements.nodes {
            let Some(stmt_node) = lib.arena.get(stmt_idx) else {
                continue;
            };
            let Some(interface) = lib.arena.get_interface(stmt_node) else {
                continue;
            };
            let Some(name) = interface_name_text(lib.arena.as_ref(), stmt_idx) else {
                continue;
            };
            if !affected.contains(&name) {
                continue;
            }
            if interface_declares_member_named(lib.arena.as_ref(), interface, &user_member_names)
                || interface_has_indexed_access_member_type(lib.arena.as_ref(), interface)
            {
                relevant.insert(name);
            }
        }
    }

    relevant.extend(seed_interfaces);
    let mut ancestor_queue: Vec<String> = relevant.iter().cloned().collect();
    while let Some(name) = ancestor_queue.pop() {
        let Some(bases) = inheritance_graph.get(&name) else {
            continue;
        };
        for base in bases {
            if relevant.insert(base.clone()) {
                ancestor_queue.push(base.clone());
            }
        }
    }

    if relevant.is_empty() {
        affected
    } else {
        relevant
    }
}

fn affected_lib_extension_interface_names(
    program: &MergedProgram,
    checker_libs: &CheckerLibSet,
    affected_interfaces: &FxHashSet<String>,
) -> FxHashSet<String> {
    let user_member_names = collect_user_global_interface_member_names(program);
    let mut extension_interfaces = FxHashSet::default();

    for lib in &checker_libs.files {
        let Some(source_file) = lib.arena.get_source_file_at(lib.root_index) else {
            continue;
        };
        for &stmt_idx in &source_file.statements.nodes {
            let Some(stmt_node) = lib.arena.get(stmt_idx) else {
                continue;
            };
            let Some(interface) = lib.arena.get_interface(stmt_node) else {
                continue;
            };
            let Some(name) = interface_name_text(lib.arena.as_ref(), stmt_idx) else {
                continue;
            };
            if affected_interfaces.contains(&name)
                && interface_declares_member_named(
                    lib.arena.as_ref(),
                    interface,
                    &user_member_names,
                )
            {
                extension_interfaces.insert(name);
            }
        }
    }

    extension_interfaces
}

fn build_lib_bound_file_for_interface_checks(
    program: &MergedProgram,
    lib_file: &Arc<LibFile>,
    affected_interfaces: &FxHashSet<String>,
) -> tsz::parallel::BoundFile {
    let mut node_symbols = FxHashMap::default();
    if let Some(source_file) = lib_file.arena.get_source_file_at(lib_file.root_index) {
        collect_lib_interface_node_symbols(
            lib_file.arena.as_ref(),
            &source_file.statements.nodes,
            &program.globals,
            affected_interfaces,
            &mut node_symbols,
        );
    }

    let mut declaration_arenas = program.declaration_arenas.clone();
    add_user_global_interface_declaration_arenas(program, &mut declaration_arenas);

    tsz::parallel::BoundFile {
        file_name: lib_file.file_name.clone(),
        source_file: lib_file.root_index,
        arena: Arc::clone(&lib_file.arena),
        node_symbols,
        symbol_arenas: program.symbol_arenas.clone(),
        declaration_arenas,
        module_declaration_exports_publicly: FxHashMap::default(),
        scopes: Vec::new(),
        node_scope_ids: FxHashMap::default(),
        parse_diagnostics: Vec::new(),
        global_augmentations: FxHashMap::default(),
        module_augmentations: FxHashMap::default(),
        augmentation_target_modules: FxHashMap::default(),
        flow_nodes: tsz::binder::FlowNodeArena::default(),
        node_flow: FxHashMap::default(),
        switch_clause_to_switch: FxHashMap::default(),
        is_external_module: lib_file.binder.is_external_module,
        expando_properties: FxHashMap::default(),
        file_features: tsz::binder::FileFeatures::NONE,
        lib_symbol_reverse_remap: FxHashMap::default(),
        semantic_defs: FxHashMap::default(),
    }
}

type LibDiagnosticFingerprint = (String, u32, u32, String);

fn lib_diagnostic_fingerprint(diag: &Diagnostic) -> LibDiagnosticFingerprint {
    (
        diag.file.clone(),
        diag.start,
        diag.code,
        diag.message_text.clone(),
    )
}

fn collect_checker_lib_baseline_fingerprints(
    program: &MergedProgram,
    options: &ResolvedCompilerOptions,
    checker_libs: &CheckerLibSet,
    affected_interfaces: &FxHashSet<String>,
    extension_interfaces: &FxHashSet<String>,
    project_env: &tsz::checker::context::ProjectEnv,
) -> FxHashSet<LibDiagnosticFingerprint> {
    let mut fingerprints = FxHashSet::default();

    for lib_idx in 0..checker_libs.files.len() {
        let query_cache = QueryCache::new(&program.type_interner);
        let (diagnostics, _, _) = check_checker_lib_file_baseline(
            project_env,
            options,
            checker_libs,
            lib_idx,
            affected_interfaces,
            extension_interfaces,
            &query_cache,
        );
        fingerprints.extend(diagnostics.iter().map(lib_diagnostic_fingerprint));
    }

    fingerprints
}

fn retain_program_induced_lib_diagnostics(
    diagnostics: &mut Vec<Diagnostic>,
    baseline_fingerprints: &FxHashSet<LibDiagnosticFingerprint>,
) {
    diagnostics.retain(|diag| !baseline_fingerprints.contains(&lib_diagnostic_fingerprint(diag)));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::args::CliArgs;
    use std::fs;
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
            &CheckerLibSet::default(),
            (false, false, false),
            &type_cache_output,
            false,
        )
        .diagnostics
    }

    fn collect_test_diagnostics_with_options(
        files: &[(&str, &str)],
        options: &ResolvedCompilerOptions,
        base_dir: &Path,
    ) -> Vec<Diagnostic> {
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
            options,
            base_dir,
            None,
            &CheckerLibSet::default(),
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

    fn mapped_type_indexed_access_constraint_repro() -> &'static str {
        r#"type Identity<T> = { [K in keyof T]: T[K] };

type M0 = { a: 1, b: 2 };

type M1 = { [K in keyof Partial<M0>]: M0[K] };

type M2 = { [K in keyof Required<M1>]: M1[K] };

type M3 = { [K in keyof Identity<Partial<M0>>]: M0[K] };

function foo<K extends keyof M0>(m1: M1[K], m2: M2[K], m3: M3[K]) {
    m1.toString();
    m1?.toString();
    m2.toString();
    m2?.toString();
    m3.toString();
    m3?.toString();
}

type Obj = {
    a: 1,
    b: 2
};

const mapped: { [K in keyof Partial<Obj>]: Obj[K] } = {};

const resolveMapped = <K extends keyof typeof mapped>(key: K) => mapped[key].toString();

const arr = ["foo", "12", 42] as const;

type Mappings = { foo: boolean, "12": number, 42: string };

type MapperArgs<K extends (typeof arr)[number]> = {
    v: K,
    i: number
};

type SetOptional<T, K extends keyof T> = Omit<T, K> & Partial<Pick<T, K>>;

type PartMappings = SetOptional<Mappings, "foo">;

const mapper: { [K in keyof PartMappings]: (o: MapperArgs<K>) => PartMappings[K] } = {
    foo: ({ v, i }) => v.length + i > 4,
    "12": ({ v, i }) => Number(v) + i,
    42: ({ v, i }) => `${v}${i}`,
};

const resolveMapper1 = <K extends keyof typeof mapper>(
    key: K, o: MapperArgs<K>) => mapper[key](o);

const resolveMapper2 = <K extends keyof typeof mapper>(
    key: K, o: MapperArgs<K>) => mapper[key]?.(o);
"#
    }

    #[test]
    fn jsx_attribute_comma_expression_survives_into_bind_results() {
        let source = r#"
declare namespace JSX {
    interface Element { }
    interface IntrinsicElements {
        [s: string]: any;
    }
}

const class1 = "foo";
const class2 = "bar";
const elem = <div className={class1, class2}/>;
"#;

        let result = parallel::parse_and_bind_single("file.tsx".to_string(), source.to_string());
        let codes: Vec<u32> = result.parse_diagnostics.iter().map(|d| d.code).collect();

        assert!(
            codes.contains(&18007),
            "expected TS18007 in bind-result parse diagnostics, got: {codes:?}"
        );
    }

    #[test]
    fn jsx_attribute_comma_expression_reports_ts18007_in_cli_diagnostics() {
        let source = r#"
declare namespace JSX {
    interface Element { }
    interface IntrinsicElements {
        [s: string]: any;
    }
}

const class1 = "foo";
const class2 = "bar";
const elem = <div className={class1, class2}/>;
"#;

        let diagnostics = collect_test_diagnostics(&[("file.tsx", source)]);
        let codes: Vec<u32> = diagnostics.iter().map(|d| d.code).collect();

        assert!(
            codes.contains(&18007),
            "expected CLI diagnostics to include TS18007, got: {diagnostics:?}"
        );
        assert!(
            codes.contains(&2695),
            "expected CLI diagnostics to include TS2695, got: {diagnostics:?}"
        );
    }

    #[test]
    fn jsx_invalid_namespace_start_keeps_colon_ts1109_in_bind_results() {
        let source = "declare var React: any;\nvar x = <:a attr={\"value\"} />;\n";
        let result = parallel::parse_and_bind_single("file.tsx".to_string(), source.to_string());
        let less_than_pos = source.find('<').expect("opening angle") as u32;
        let colon_pos = source[less_than_pos as usize + 1..]
            .find(':')
            .map(|offset| less_than_pos + 1 + offset as u32)
            .expect("colon");
        let expr_expected_positions: Vec<u32> = result
            .parse_diagnostics
            .iter()
            .filter(|diag| diag.code == 1109)
            .map(|diag| diag.start)
            .collect();

        assert!(
            expr_expected_positions.contains(&less_than_pos),
            "expected TS1109 at '<', got: {expr_expected_positions:?}"
        );
        assert!(
            expr_expected_positions.contains(&colon_pos),
            "expected TS1109 at ':', got: {expr_expected_positions:?}"
        );
    }

    #[test]
    fn jsx_invalid_namespace_start_keeps_colon_ts1109_in_cli_diagnostics() {
        let source = "declare var React: any;\nvar x = <:a attr={\"value\"} />;\n";
        let diagnostics = collect_test_diagnostics(&[("file.tsx", source)]);
        let less_than_pos = source.find('<').expect("opening angle") as u32;
        let colon_pos = source[less_than_pos as usize + 1..]
            .find(':')
            .map(|offset| less_than_pos + 1 + offset as u32)
            .expect("colon");
        let expr_expected_positions: Vec<u32> = diagnostics
            .iter()
            .filter(|diag| diag.code == 1109)
            .map(|diag| diag.start)
            .collect();

        assert!(
            expr_expected_positions.contains(&less_than_pos),
            "expected CLI TS1109 at '<', got: {diagnostics:?}"
        );
        assert!(
            expr_expected_positions.contains(&colon_pos),
            "expected CLI TS1109 at ':', got: {diagnostics:?}"
        );
    }

    #[test]
    fn test_collect_diagnostics_preserves_mapped_type_nullish_indexed_reads() {
        let dir = tempfile::TempDir::new().expect("temp dir");
        let file_path = dir.path().join("main.ts");
        std::fs::write(&file_path, mapped_type_indexed_access_constraint_repro())
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
        let checker_libs = load_checker_libs(&lib_files);
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
            &checker_libs,
            (false, false, false),
            &type_cache_output,
            false,
        )
        .diagnostics;
        let ts18048_count = diagnostics
            .iter()
            .filter(|diag| diag.code == diagnostic_codes::IS_POSSIBLY_UNDEFINED)
            .count();
        let ts2532_count = diagnostics
            .iter()
            .filter(|diag| diag.code == diagnostic_codes::OBJECT_IS_POSSIBLY_UNDEFINED)
            .count();
        let ts2722_count = diagnostics
            .iter()
            .filter(|diag| {
                diag.code == diagnostic_codes::CANNOT_INVOKE_AN_OBJECT_WHICH_IS_POSSIBLY_UNDEFINED
            })
            .count();
        let ts2349_count = diagnostics
            .iter()
            .filter(|diag| diag.code == diagnostic_codes::THIS_EXPRESSION_IS_NOT_CALLABLE)
            .count();

        assert_eq!(
            ts18048_count, 3,
            "Expected collect_diagnostics to preserve three TS18048 diagnostics, got: {diagnostics:?}"
        );
        assert_eq!(
            ts2532_count, 1,
            "Expected one TS2532 for mapped[key].toString(), got: {diagnostics:?}"
        );
        assert_eq!(
            ts2722_count, 1,
            "Expected one TS2722 for mapper[key](o), got: {diagnostics:?}"
        );
        assert_eq!(
            ts2349_count, 0,
            "Did not expect TS2349 for mapper[key](o), got: {diagnostics:?}"
        );
    }

    fn resolved_options_for_esnext_strict_test() -> ResolvedCompilerOptions {
        let mut args = default_cli_args_for_test();
        args.ignore_config = true;
        args.strict = true;
        args.target = Some(crate::args::Target::EsNext);

        let mut resolved = crate::config::resolve_compiler_options(None)
            .expect("resolve default compiler options");
        crate::driver::apply_cli_overrides(&mut resolved, &args).expect("apply cli overrides");
        if matches!(resolved.printer.module, ModuleKind::None) {
            resolved.printer.module = ModuleKind::ESNext;
            resolved.checker.module = ModuleKind::ESNext;
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
        let checker_libs = load_checker_libs(&lib_files);
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
            &checker_libs,
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
    fn test_compile_inner_program_reports_ts2851_for_async_iterator_await_using() {
        let dir = tempfile::TempDir::new().expect("temp dir");
        let file_path = dir.path().join("main.ts");
        std::fs::write(
            &file_path,
            r#"
declare const ai: AsyncIterator<string, undefined>;
declare const aio: AsyncIteratorObject<string, undefined, unknown>;
declare const ag: AsyncGenerator<string, void>;

async function f() {
    await using it0 = aio;
    await using it1 = ag;
    await using it2 = ai;
}
"#,
        )
        .expect("write source");

        let resolved = resolved_options_for_esnext_strict_test();
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
        let checker_libs = load_checker_libs(&lib_files);
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
            &checker_libs,
            (false, false, false),
            &type_cache_output,
            false,
        )
        .diagnostics;

        let ts2851_count = diagnostics.iter().filter(|diag| diag.code == 2851).count();
        assert_eq!(
            ts2851_count, 1,
            "Expected one TS2851 for await using AsyncIterator, got: {diagnostics:?}"
        );
    }

    #[test]
    fn test_compile_inner_program_reports_ts2456_for_recursive_mapped_type_aliases() {
        let dir = tempfile::TempDir::new().expect("temp dir");
        let file_path = dir.path().join("main.ts");
        std::fs::write(
            &file_path,
            r#"
type Recurse = {
    [K in keyof Recurse]: Recurse[K]
}

type Recurse1 = {
    [K in keyof Recurse2]: Recurse2[K]
}

type Recurse2 = {
    [K in keyof Recurse1]: Recurse1[K]
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
        let checker_libs = load_checker_libs(&lib_files);
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
            &checker_libs,
            (false, false, false),
            &type_cache_output,
            false,
        )
        .diagnostics;

        let ts2456_count = diagnostics.iter().filter(|diag| diag.code == 2456).count();
        assert_eq!(
            ts2456_count, 3,
            "Expected TS2456 for the recursive mapped aliases, got: {diagnostics:?}"
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
        let checker_libs = load_checker_libs(&lib_files);
        let direct_lib_contexts: Vec<LibContext> = lib_files
            .iter()
            .map(|lib| LibContext {
                arena: Arc::clone(&lib.arena),
                binder: Arc::clone(&lib.binder),
            })
            .collect();
        let direct_checker_libs = CheckerLibSet {
            files: lib_files.clone(),
            contexts: direct_lib_contexts.clone(),
        };
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
            &direct_checker_libs,
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
            &checker_libs,
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

    /// TS2883: Nested `node_modules` types should be detected as non-portable.
    /// When an exported variable's inferred type references a type from a
    /// nested `node_modules` (e.g., `foo/node_modules/nested`), tsz must
    /// emit TS2883 even when the nested package lacks a `package.json`.
    #[test]
    fn test_ts2883_nested_node_modules_non_portable_type() {
        let dir = tempfile::TempDir::new().expect("temp dir");

        // Create nested node_modules structure:
        // r/node_modules/foo/node_modules/nested/index.d.ts
        let nested_dir = dir.path().join("r/node_modules/foo/node_modules/nested");
        std::fs::create_dir_all(&nested_dir).expect("create nested dir");
        std::fs::write(
            nested_dir.join("index.d.ts"),
            "export interface NestedProps {}\n",
        )
        .expect("write nested/index.d.ts");

        let foo_dir = dir.path().join("r/node_modules/foo");
        std::fs::write(
            foo_dir.join("index.d.ts"),
            r#"import { NestedProps } from "nested";
export interface SomeProps {}
export function foo(): [SomeProps, NestedProps];
"#,
        )
        .expect("write foo/index.d.ts");

        std::fs::write(
            dir.path().join("r/entry.ts"),
            r#"import { foo } from "foo";
export const x = foo();
"#,
        )
        .expect("write r/entry.ts");

        let mut resolved = resolved_options_for_es2015_strict_test();
        resolved.checker.module = ModuleKind::CommonJS;
        resolved.printer.module = ModuleKind::CommonJS;
        resolved.checker.emit_declarations = true;

        let file_paths = vec![dir.path().join("r/entry.ts")];
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
        let checker_libs = load_checker_libs(&lib_files);
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
            &checker_libs,
            (false, false, false),
            &type_cache_output,
            false,
        )
        .diagnostics;

        let ts2883_diags: Vec<_> = diagnostics
            .iter()
            .filter(|diag| diag.code == 2883)
            .collect();
        assert!(
            !ts2883_diags.is_empty(),
            "Expected at least one TS2883 diagnostic for nested node_modules type reference, got: {diagnostics:?}"
        );
        assert!(
            ts2883_diags[0].message_text.contains("NestedProps"),
            "TS2883 message should reference 'NestedProps', got: {}",
            ts2883_diags[0].message_text
        );
        assert!(
            ts2883_diags[0]
                .message_text
                .contains("foo/node_modules/nested"),
            "TS2883 message should reference 'foo/node_modules/nested', got: {}",
            ts2883_diags[0].message_text
        );
    }

    #[test]
    fn test_collect_diagnostics_keeps_unimported_external_module_type_alias_unresolved() {
        let dir = tempfile::TempDir::new().expect("temp dir");
        std::fs::write(
            dir.path().join("Helpers.ts"),
            r#"
export type StringKeyOf<TObj> = Extract<string, keyof TObj>;
"#,
        )
        .expect("write Helpers.ts");
        std::fs::write(
            dir.path().join("FromFactor.ts"),
            r#"
export type RowToColumns<TColumns> = {
    [TName in StringKeyOf<TColumns>]: any;
};
"#,
        )
        .expect("write FromFactor.ts");

        let mut resolved = resolved_options_for_es2015_strict_test();
        resolved.checker.module = ModuleKind::CommonJS;
        resolved.printer.module = ModuleKind::CommonJS;
        resolved.checker.emit_declarations = true;

        let file_paths = vec![
            dir.path().join("Helpers.ts"),
            dir.path().join("FromFactor.ts"),
        ];
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
        let checker_libs = load_checker_libs(&lib_files);
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
            &checker_libs,
            (false, false, false),
            &type_cache_output,
            false,
        )
        .diagnostics;

        assert!(
            diagnostics.iter().any(|diag| {
                diag.code == diagnostic_codes::CANNOT_FIND_NAME
                    && diag.message_text.contains("StringKeyOf")
            }),
            "Expected TS2304 for unimported external-module type alias in collect_diagnostics, got: {diagnostics:?}"
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
        for (specifier, _, _, _) in &module_specifiers {
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
    fn test_collect_diagnostics_preserves_source_local_duplicate_package_paths() {
        let dir = std::env::temp_dir().join("tsz_check_duplicate_package_global_merge");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(dir.join("src")).unwrap();
        fs::create_dir_all(dir.join("tests")).unwrap();
        fs::create_dir_all(dir.join("node_modules/@types/react")).unwrap();
        fs::create_dir_all(dir.join("tests/node_modules/@types/react")).unwrap();

        fs::write(
            dir.join("node_modules/@types/react/package.json"),
            r#"{"name":"@types/react","version":"16.4.6"}"#,
        )
        .unwrap();
        fs::write(
            dir.join("tests/node_modules/@types/react/package.json"),
            r#"{"name":"@types/react","version":"16.4.6"}"#,
        )
        .unwrap();

        let root_react_path = dir.join("node_modules/@types/react/index.d.ts");
        let tests_react_path = dir.join("tests/node_modules/@types/react/index.d.ts");
        // Both stubs must be proper external modules so that import resolution
        // sees them as valid modules rather than producing TS2306/TS2669.
        fs::write(
            &root_react_path,
            "export declare function createElement(tag: string): any;\n",
        )
        .unwrap();
        fs::write(
            &tests_react_path,
            "export declare function createElement(tag: string): any;\n",
        )
        .unwrap();

        let src_index = dir.join("src/index.ts");
        let tests_index = dir.join("tests/index.ts");

        let options = ResolvedCompilerOptions {
            checker: tsz::checker::context::CheckerOptions {
                module: ModuleKind::CommonJS,
                target: tsz_common::common::ScriptTarget::ES2015,
                ..Default::default()
            },
            printer: tsz::emitter::PrinterOptions {
                module: ModuleKind::CommonJS,
                target: tsz_common::common::ScriptTarget::ES2015,
                ..Default::default()
            },
            module_suffixes: vec![String::new()],
            ..Default::default()
        };

        let diagnostics = collect_test_diagnostics_with_options(
            &[
                (
                    src_index.to_str().unwrap(),
                    "import * as React from 'react';\nexport var x = 1;\n",
                ),
                (
                    tests_index.to_str().unwrap(),
                    "import * as React from 'react';\nexport var y = 2;\n",
                ),
                (
                    root_react_path.to_str().unwrap(),
                    "export declare function createElement(tag: string): any;\n",
                ),
                (
                    tests_react_path.to_str().unwrap(),
                    "export declare function createElement(tag: string): any;\n",
                ),
            ],
            &options,
            &dir,
        );

        // With valid module stubs, both imports should resolve successfully.
        // The test primarily validates that having duplicate @types/react at
        // different node_modules depths does not crash or produce spurious errors.
        // No TS2307/TS2306/TS2669 diagnostics should appear.
        let error_codes: Vec<u32> = diagnostics.iter().map(|d| d.code).collect();
        assert!(
            !error_codes.contains(&2307)
                && !error_codes.contains(&2306)
                && !error_codes.contains(&2669),
            "expected no module resolution errors with valid react stubs, got: {diagnostics:?}"
        );

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_collect_diagnostics_suppresses_ts2339_after_type_only_export_equals_namespace_use() {
        let dir = std::env::temp_dir().join("tsz_check_type_only_export_equals_namespace_use");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();

        let a_path = dir.join("a.ts");
        let b_path = dir.join("b.ts");
        let f_path = dir.join("f.ts");

        let a_source = "export class A {}\n";
        let b_source = "import type * as types from './a';\nexport = types;\n";
        let f_source = "import * as types from './b';\nnew types.A();\n";

        fs::write(&a_path, a_source).unwrap();
        fs::write(&b_path, b_source).unwrap();
        fs::write(&f_path, f_source).unwrap();

        let options = ResolvedCompilerOptions {
            checker: tsz::checker::context::CheckerOptions {
                module: ModuleKind::CommonJS,
                target: tsz_common::common::ScriptTarget::ES2015,
                ..Default::default()
            },
            printer: tsz::emitter::PrinterOptions {
                module: ModuleKind::CommonJS,
                target: tsz_common::common::ScriptTarget::ES2015,
                ..Default::default()
            },
            es_module_interop: true,
            module_suffixes: vec![String::new()],
            ..Default::default()
        };

        let diagnostics = collect_test_diagnostics_with_options(
            &[
                (a_path.to_str().unwrap(), a_source),
                (b_path.to_str().unwrap(), b_source),
                (f_path.to_str().unwrap(), f_source),
            ],
            &options,
            &dir,
        );

        let f_diags: Vec<_> = diagnostics
            .iter()
            .filter(|diag| Path::new(&diag.file) == f_path.as_path() && diag.code != 2318)
            .collect();

        assert!(
            f_diags.iter().any(|diag| diag.code == 1361),
            "expected TS1361 on namespace use of a type-only export= chain, got: {f_diags:?}"
        );
        assert!(
            f_diags.iter().all(|diag| diag.code != 2339),
            "did not expect follow-on TS2339 once TS1361 fired, got: {f_diags:?}"
        );

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_collect_diagnostics_keeps_ts1362_for_checked_js_module_exports_type_only_require() {
        let dir = std::env::temp_dir().join("tsz_check_js_module_exports_type_only_require");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();

        let importer_path = dir.join("importer.cjs");
        let exporter_path = dir.join("exporter.mts");

        let importer_source = "const Foo = require(\"./exporter.mjs\");\nnew Foo();\n";
        let exporter_source =
            "export default class Foo {}\nexport type { Foo as \"module.exports\" };\n";

        fs::write(&importer_path, importer_source).unwrap();
        fs::write(&exporter_path, exporter_source).unwrap();

        let options = ResolvedCompilerOptions {
            allow_js: true,
            check_js: true,
            module_resolution: Some(crate::config::ModuleResolutionKind::NodeNext),
            module_suffixes: vec![String::new()],
            printer: tsz::emitter::PrinterOptions {
                module: ModuleKind::Node20,
                target: tsz_common::common::ScriptTarget::ES2023,
                ..Default::default()
            },
            checker: tsz::checker::context::CheckerOptions {
                module: ModuleKind::Node20,
                target: tsz_common::common::ScriptTarget::ES2023,
                ..Default::default()
            },
            ..Default::default()
        };

        let diagnostics = collect_test_diagnostics_with_options(
            &[
                (importer_path.to_str().unwrap(), importer_source),
                (exporter_path.to_str().unwrap(), exporter_source),
            ],
            &options,
            &dir,
        );

        let importer_diags: Vec<_> = diagnostics
            .iter()
            .filter(|diag| Path::new(&diag.file) == importer_path.as_path())
            .collect();

        assert!(
            importer_diags.iter().any(|diag| diag.code == 1362),
            "expected TS1362 for checked CommonJS require() of a type-only \
             \"module.exports\" binding, got: {importer_diags:?}"
        );

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_collect_diagnostics_bundler_dual_package_modes_do_not_emit_ts2305() {
        let dir = std::env::temp_dir().join("tsz_check_bundler_dual_package_modes");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(dir.join("node_modules/dual")).unwrap();

        let package_json_path = dir.join("node_modules/dual/package.json");
        let index_js_path = dir.join("node_modules/dual/index.js");
        let index_d_ts_path = dir.join("node_modules/dual/index.d.ts");
        let index_cjs_path = dir.join("node_modules/dual/index.cjs");
        let index_d_cts_path = dir.join("node_modules/dual/index.d.cts");
        let main_ts_path = dir.join("main.ts");
        let main_mts_path = dir.join("main.mts");
        let main_cts_path = dir.join("main.cts");

        fs::write(
            &package_json_path,
            r#"{
  "name": "dual",
  "version": "1.0.0",
  "type": "module",
  "main": "index.cjs",
  "types": "index.d.cts",
  "exports": {
    ".": {
      "import": "./index.js",
      "require": "./index.cjs"
    }
  }
}
"#,
        )
        .unwrap();
        fs::write(&index_js_path, "export const esm = 0;\n").unwrap();
        fs::write(&index_d_ts_path, "export const esm: number;\n").unwrap();
        fs::write(&index_cjs_path, "exports.cjs = 0;\n").unwrap();
        fs::write(&index_d_cts_path, "export const cjs: number;\n").unwrap();
        fs::write(&main_ts_path, "import { esm, cjs } from \"dual\";\n").unwrap();
        fs::write(&main_mts_path, "import { esm, cjs } from \"dual\";\n").unwrap();
        fs::write(&main_cts_path, "import { esm, cjs } from \"dual\";\n").unwrap();

        let options = ResolvedCompilerOptions {
            module_resolution: Some(crate::config::ModuleResolutionKind::Bundler),
            resolve_package_json_exports: true,
            module_suffixes: vec![String::new()],
            printer: tsz::emitter::PrinterOptions {
                module: ModuleKind::Preserve,
                target: tsz_common::common::ScriptTarget::ES2015,
                ..Default::default()
            },
            checker: tsz::checker::context::CheckerOptions {
                module: ModuleKind::Preserve,
                target: tsz_common::common::ScriptTarget::ES2015,
                ..Default::default()
            },
            ..Default::default()
        };

        let diagnostics = collect_test_diagnostics_with_options(
            &[
                (
                    main_ts_path.to_str().unwrap(),
                    "import { esm, cjs } from \"dual\";\n",
                ),
                (
                    main_mts_path.to_str().unwrap(),
                    "import { esm, cjs } from \"dual\";\n",
                ),
                (
                    main_cts_path.to_str().unwrap(),
                    "import { esm, cjs } from \"dual\";\n",
                ),
            ],
            &options,
            &dir,
        );

        let import_diags: Vec<_> = diagnostics
            .iter()
            .filter(|diag| {
                let file = Path::new(&diag.file);
                (file == main_ts_path.as_path()
                    || file == main_mts_path.as_path()
                    || file == main_cts_path.as_path())
                    && diag.code != 2318
            })
            .collect();

        assert!(
            import_diags.iter().all(|diag| diag.code != 2305),
            "expected no TS2305 for bundler dual-package import/require mode selection, got: {import_diags:?}"
        );

        let _ = fs::remove_dir_all(&dir);
    }

    #[cfg(unix)]
    #[test]
    fn test_collect_diagnostics_preserve_symlinks_keeps_original_target_error() {
        use std::fs;
        use std::os::unix::fs::symlink;

        let dir = tempfile::tempdir().unwrap();
        fs::create_dir_all(dir.path().join("linked")).unwrap();
        fs::create_dir_all(dir.path().join("app/node_modules/real")).unwrap();
        fs::create_dir_all(dir.path().join("app/node_modules/linked")).unwrap();
        fs::create_dir_all(dir.path().join("app/node_modules/linked2")).unwrap();

        fs::write(
            dir.path().join("linked/index.d.ts"),
            "export { real } from \"real\";\nexport class C { private x; }\n",
        )
        .unwrap();
        fs::write(
            dir.path().join("app/node_modules/real/index.d.ts"),
            "export const real: string;\n",
        )
        .unwrap();
        fs::write(
            dir.path().join("app/app.ts"),
            "/// <reference types=\"linked\" />\nimport { C as C1 } from \"linked\";\nimport { C as C2 } from \"linked2\";\nlet x = new C1();\nx = new C2();\n",
        )
        .unwrap();
        symlink(
            dir.path().join("linked/index.d.ts"),
            dir.path().join("app/node_modules/linked/index.d.ts"),
        )
        .unwrap();
        symlink(
            dir.path().join("linked/index.d.ts"),
            dir.path().join("app/node_modules/linked2/index.d.ts"),
        )
        .unwrap();

        let resolved = ResolvedCompilerOptions {
            module_resolution: Some(crate::config::ModuleResolutionKind::Bundler),
            preserve_symlinks: true,
            module_suffixes: vec![String::new()],
            printer: tsz::emitter::PrinterOptions {
                module: ModuleKind::ES2015,
                ..Default::default()
            },
            checker: tsz::checker::context::CheckerOptions {
                module: ModuleKind::ES2015,
                ..Default::default()
            },
            ..Default::default()
        };

        let file_paths = vec![
            dir.path().join("linked/index.d.ts"),
            dir.path().join("app/app.ts"),
        ];
        let SourceReadResult {
            sources,
            dependencies: _,
            type_reference_errors,
            resolution_mode_errors,
        } = super::read_source_files(&file_paths, dir.path(), &resolved, None, None)
            .expect("read source files");

        assert!(type_reference_errors.is_empty());
        assert!(resolution_mode_errors.is_empty());

        let source_paths: FxHashSet<PathBuf> =
            sources.iter().map(|source| source.path.clone()).collect();
        assert!(source_paths.contains(&dir.path().join("linked/index.d.ts")));
        assert!(source_paths.contains(&dir.path().join("app/node_modules/linked/index.d.ts")));
        assert!(source_paths.contains(&dir.path().join("app/node_modules/linked2/index.d.ts")));

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
            &[],
        ));
        let type_cache_output = std::sync::Mutex::new(FxHashMap::default());

        let diagnostics = collect_diagnostics(
            &program,
            &resolved,
            dir.path(),
            None,
            &CheckerLibSet::default(),
            (false, false, false),
            &type_cache_output,
            false,
        )
        .diagnostics;

        assert!(
            diagnostics.iter().any(|diag| {
                diag.code
                    == diagnostic_codes::CANNOT_FIND_MODULE_OR_ITS_CORRESPONDING_TYPE_DECLARATIONS
                    && diag.file.contains("linked/index.d.ts")
            }),
            "expected TS2307 for original linked target, got: {diagnostics:?}"
        );
    }

    #[test]
    fn test_collect_diagnostics_preserves_mapped_type_generic_indexed_access_context() {
        let dir = tempfile::TempDir::new().expect("temp dir");
        let file_path = dir.path().join("main.ts");
        std::fs::write(
            &file_path,
            r#"// repro from #49242

type Types = {
    [key: string]: object;
};

type Filled<T extends Types> = {
    [K in keyof T]: [T[K]];
}

class Test<Types extends {
    [key: string]: object;
}> {
    entries: {
        [T in keyof Types]?: Types[T][];
    } = {}

    get<T extends keyof Types>(name: T): Filled<Pick<Types, T>> {
        let entry = this.entries[name];
        if (entry) return { [name]: [entry[0]] } as Filled<Pick<Types, T>>;
        throw new Error("Entry not found");
    }
}

// repro from #49338

type TypesMap = {
    0: {
        foo: string,
    };
    1: {
        a: number,
    };
}
type P<T extends keyof TypesMap> = {
    t: T;
} & TypesMap[T];
type Handlers = { [M in keyof TypesMap]?: (p: P<M>) => void };
const typeHandlers: Handlers = {
    [0]: (p) => console.log(p.foo),
    [1]: (p) => console.log(p.a),
};

const onSomeEvent = <T extends keyof TypesMap>(p: P<T>) => typeHandlers[p.t]?.(p);
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
        let checker_libs = load_checker_libs(&lib_files);
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
            &checker_libs,
            (false, false, false),
            &type_cache_output,
            false,
        )
        .diagnostics;

        let relevant = diagnostics
            .iter()
            .filter(|diag| {
                matches!(
                    diag.code,
                    diagnostic_codes::TYPE_DOES_NOT_SATISFY_THE_CONSTRAINT
                        | diagnostic_codes::PARAMETER_IMPLICITLY_HAS_AN_TYPE
                )
            })
            .collect::<Vec<_>>();

        assert!(
            relevant.is_empty(),
            "Expected mapped generic indexed access repro to keep context in collect_diagnostics, got: {diagnostics:?}"
        );
    }

    #[test]
    fn test_collect_diagnostics_preserves_recursive_mapped_type_callback_context() {
        let dir = tempfile::TempDir::new().expect("temp dir");
        let file_path = dir.path().join("main.ts");
        std::fs::write(
            &file_path,
            r#"type MorphTuple = [string, "|>", any]

type validateMorph<def extends MorphTuple> = def[1] extends "|>"
    ? [validateDefinition<def[0]>, "|>", (In: def[0]) => unknown]
    : def

type validateDefinition<def> = def extends MorphTuple
    ? validateMorph<def>
    : {
          [k in keyof def]: validateDefinition<def[k]>
      }

declare function type<def>(def: validateDefinition<def>): def

const shallow = type(["ark", "|>", (x) => x.length])
const objectLiteral = type({ a: ["ark", "|>", (x) => x.length] })
const nestedTuple = type([["ark", "|>", (x) => x.length]])
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
        let checker_libs = load_checker_libs(&lib_files);
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
            &checker_libs,
            (false, false, false),
            &type_cache_output,
            false,
        )
        .diagnostics;

        let relevant = diagnostics
            .iter()
            .filter(|diag| {
                matches!(
                    diag.code,
                    diagnostic_codes::PARAMETER_IMPLICITLY_HAS_AN_TYPE
                )
            })
            .collect::<Vec<_>>();

        assert!(
            relevant.is_empty(),
            "Expected recursive mapped-type callback repro to keep context in collect_diagnostics, got: {diagnostics:?}"
        );
    }

    #[test]
    fn test_collect_diagnostics_preserves_union_array_method_alias_callback_context() {
        let dir = tempfile::TempDir::new().expect("temp dir");
        let file_path = dir.path().join("main.ts");
        std::fs::write(
            &file_path,
            r#"interface Fizz { id: number; fizz: string }
interface Buzz { id: number; buzz: string }
interface Arr<T> {
  filter<S extends T>(pred: (value: T) => value is S): S[];
  filter(pred: (value: T) => unknown): T[];
}
declare const m: Arr<Fizz>["filter"] | Arr<Buzz>["filter"];
m(item => item.id < 5);
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
        let checker_libs = load_checker_libs(&lib_files);
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
            &checker_libs,
            (false, false, false),
            &type_cache_output,
            false,
        )
        .diagnostics;

        let relevant = diagnostics
            .iter()
            .filter(|diag| diag.code == diagnostic_codes::PARAMETER_IMPLICITLY_HAS_AN_TYPE)
            .collect::<Vec<_>>();

        assert!(
            relevant.is_empty(),
            "Expected union overloaded array method alias repro to keep context in collect_diagnostics, got: {diagnostics:?}"
        );
    }

    #[test]
    fn test_collect_diagnostics_preserves_union_builtin_array_method_callback_context() {
        let dir = tempfile::TempDir::new().expect("temp dir");
        let file_path = dir.path().join("main.ts");
        std::fs::write(
            &file_path,
            r#"interface Fizz { id: number; fizz: string }
interface Buzz { id: number; buzz: string }

([] as Fizz[] | Buzz[]).filter(item => item.id < 5);
([] as Fizz[] | readonly Buzz[]).filter(item => item.id < 5);
([] as Fizz[] | Buzz[]).find(item => item);
([] as Fizz[] | Buzz[]).every(item => item.id < 5);
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
        let checker_libs = load_checker_libs(&lib_files);
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
            &checker_libs,
            (false, false, false),
            &type_cache_output,
            false,
        )
        .diagnostics;

        let relevant = diagnostics
            .iter()
            .filter(|diag| diag.code == diagnostic_codes::PARAMETER_IMPLICITLY_HAS_AN_TYPE)
            .collect::<Vec<_>>();

        assert!(
            relevant.is_empty(),
            "Expected union built-in array method repro to keep context in collect_diagnostics, got: {diagnostics:?}"
        );
    }

    #[test]
    #[ignore] // TODO: should report implicit any for primitive union property callback
    fn test_collect_diagnostics_reports_implicit_any_for_primitive_union_property_callback() {
        let dir = tempfile::TempDir::new().expect("temp dir");
        let file_path = dir.path().join("main.ts");
        std::fs::write(
            &file_path,
            r#"type Validate = (text: string, pos: number, self: Rule) => number | boolean;
interface FullRule {
  validate: string | RegExp | Validate;
  normalize?: (match: {x: string}) => void;
}

type Rule = string | FullRule;

const obj: {field: Rule} = {
  field: {
    validate: (_t, _p, _s) => false,
    normalize: match => match.x,
  }
};
"#,
        )
        .expect("write source");

        let resolved = resolved_options_for_esnext_strict_test();
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
        let checker_libs = load_checker_libs(&lib_files);
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
            &checker_libs,
            (false, false, false),
            &type_cache_output,
            false,
        )
        .diagnostics;

        let relevant = diagnostics
            .iter()
            .filter(|diag| diag.code == diagnostic_codes::PARAMETER_IMPLICITLY_HAS_AN_TYPE)
            .collect::<Vec<_>>();

        assert_eq!(
            relevant.len(),
            1,
            "Expected exactly one TS7006 for the primitive-union normalize callback, got: {diagnostics:?}"
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
    fn collect_diagnostics_reports_default_lib_breakage_from_global_node_merge() {
        let dir = tempfile::TempDir::new().expect("temp dir");
        let file_path = dir.path().join("main.ts");
        std::fs::write(
            &file_path,
            r#"
const enum SyntaxKind {
    Modifier,
    Decorator,
}

interface Node {
    kind: SyntaxKind;
}

interface Modifier extends Node { kind: SyntaxKind.Modifier; }
interface Decorator extends Node { kind: SyntaxKind.Decorator; }

declare function isModifier(node: Node): node is Modifier;
declare function isDecorator(node: Node): node is Decorator;

declare function every<T, U extends T>(array: readonly T[], callback: (element: T) => element is U): array is readonly U[];

declare const modifiers: readonly Decorator[] | readonly Modifier[];

function foo() {
    every(modifiers, isModifier);
    every(modifiers, isDecorator);
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
        let checker_libs = load_checker_libs(&lib_files);
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
            &checker_libs,
            (false, false, false),
            &type_cache_output,
            false,
        )
        .diagnostics;

        let lib_dom_diagnostics = diagnostics
            .iter()
            .filter(|diag| diag.file.ends_with("lib.dom.d.ts"))
            .collect::<Vec<_>>();
        let ts2344_count = lib_dom_diagnostics
            .iter()
            .filter(|diag| diag.code == diagnostic_codes::TYPE_DOES_NOT_SATISFY_THE_CONSTRAINT)
            .count();
        let ts2430_count = lib_dom_diagnostics
            .iter()
            .filter(|diag| diag.code == diagnostic_codes::INTERFACE_INCORRECTLY_EXTENDS_INTERFACE)
            .count();

        assert_eq!(
            ts2344_count, 0,
            "Expected no cascading TS2344 diagnostics from lib.dom.d.ts after merging Node.kind, got: {diagnostics:?}"
        );
        assert_eq!(
            ts2430_count, 1,
            "Expected one TS2430 diagnostic from lib.dom.d.ts after merging Node.kind, got: {diagnostics:?}"
        );
    }

    #[test]
    fn collect_diagnostics_respects_skip_default_lib_check_for_global_node_merge() {
        let dir = tempfile::TempDir::new().expect("temp dir");
        let file_path = dir.path().join("main.ts");
        std::fs::write(
            &file_path,
            r#"
const enum SyntaxKind {
    Modifier,
    Decorator,
}

interface Node {
    kind: SyntaxKind;
}
"#,
        )
        .expect("write source");

        let mut resolved = resolved_options_for_es2015_strict_test();
        resolved.skip_default_lib_check = true;
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
        let checker_libs = load_checker_libs(&lib_files);
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
            &checker_libs,
            (false, false, false),
            &type_cache_output,
            false,
        )
        .diagnostics;

        assert!(
            !diagnostics.iter().any(|diag| {
                diag.file.ends_with("lib.dom.d.ts")
                    && matches!(
                        diag.code,
                        diagnostic_codes::TYPE_DOES_NOT_SATISFY_THE_CONSTRAINT
                            | diagnostic_codes::INTERFACE_INCORRECTLY_EXTENDS_INTERFACE
                    )
            }),
            "Did not expect lib.dom.d.ts TS2344/TS2430 diagnostics when skipDefaultLibCheck is enabled, got: {diagnostics:?}"
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

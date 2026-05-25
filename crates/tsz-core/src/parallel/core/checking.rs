fn extract_skeletons_for_merge(results: &[&BindResult]) -> Vec<FileSkeleton> {
    const PARALLEL_SKELETON_FILE_THRESHOLD: usize = 128;

    if results.len() < PARALLEL_SKELETON_FILE_THRESHOLD {
        return results.iter().map(|r| extract_skeleton(r)).collect();
    }

    extract_skeletons_for_merge_large(results)
}

#[cfg(not(target_arch = "wasm32"))]
fn extract_skeletons_for_merge_large(results: &[&BindResult]) -> Vec<FileSkeleton> {
    ensure_rayon_global_pool();
    results.par_iter().map(|r| extract_skeleton(r)).collect()
}

#[cfg(target_arch = "wasm32")]
fn extract_skeletons_for_merge_large(results: &[&BindResult]) -> Vec<FileSkeleton> {
    results.iter().map(|r| extract_skeleton(r)).collect()
}

/// Full pipeline: Parse → Bind (parallel) → Merge (sequential)
///
/// This is the main entry point for multi-file compilation.
/// Lib files are automatically loaded and merged during binding.
pub fn compile_files(files: Vec<(String, String)>) -> MergedProgram {
    let lib_files = resolve_default_lib_files(ScriptTarget::ESNext)
        .unwrap_or_else(|err| panic!("failed to resolve default lib files: {err}"));
    compile_files_with_libs(files, &lib_files)
}

/// Full pipeline with explicit lib files.
///
/// Callers are responsible for providing the resolved lib file paths.
pub fn compile_files_with_libs(
    files: Vec<(String, String)>,
    lib_files: &[PathBuf],
) -> MergedProgram {
    let lib_paths: Vec<&Path> = lib_files.iter().map(PathBuf::as_path).collect();
    let bind_results = parse_and_bind_parallel_with_lib_files(files, &lib_paths);
    merge_bind_results(bind_results)
}

// =============================================================================
// Parallel Type Checking
// =============================================================================

use crate::checker::context::{CheckerOptions, LibContext};
use crate::checker::diagnostics::Diagnostic;
use crate::checker::state::CheckerState;
use crate::lib_loader::LibFile;
use crate::parser::syntax_kind_ext;
use crate::tsz_solver::TypeId;

/// Result of type checking a single function body
#[derive(Debug)]
pub struct FunctionCheckResult {
    /// Function node index within its file
    pub function_idx: NodeIndex,
    /// File index in the program
    pub file_idx: usize,
    /// Inferred return type
    pub return_type: TypeId,
    /// Diagnostics produced
    pub diagnostics: Vec<Diagnostic>,
}

/// Result of type checking all function bodies in a file
#[derive(Debug)]
pub struct FileCheckResult {
    /// File index
    pub file_idx: usize,
    /// File name
    pub file_name: String,
    /// Function check results
    pub function_results: Vec<FunctionCheckResult>,
    /// File-level diagnostics
    pub diagnostics: Vec<Diagnostic>,
}

use super::diagnostics::{
    add_parallel_global_augmentation_member_conflict_diagnostics,
    add_reexported_module_augmentation_enum_conflict_diagnostics,
    affected_lib_extension_interface_names, affected_lib_interface_names,
    build_lib_bound_file_for_interface_checks, lib_file_contains_affected_interface,
    suppress_parallel_import_shadowing_namespace_type_diagnostics,
    suppress_parallel_ts2339_cascade_diagnostics,
};

/// Result of parallel type checking
#[derive(Debug)]
pub struct CheckResult {
    /// Per-file check results
    pub file_results: Vec<FileCheckResult>,
    /// Total functions checked
    pub function_count: usize,
    /// Total diagnostics
    pub diagnostic_count: usize,
}

/// Collect all function declarations from a source file
fn collect_functions(arena: &NodeArena, source_file: NodeIndex) -> Vec<NodeIndex> {
    let mut functions = Vec::new();

    let Some(sf) = arena.get_source_file_at(source_file) else {
        return functions;
    };

    for &stmt_idx in &sf.statements.nodes {
        collect_functions_from_node(arena, stmt_idx, &mut functions);
    }

    functions
}

/// Recursively collect functions from a node
fn collect_functions_from_node(
    arena: &NodeArena,
    node_idx: NodeIndex,
    functions: &mut Vec<NodeIndex>,
) {
    let Some(node) = arena.get(node_idx) else {
        return;
    };

    match node.kind {
        k if k == syntax_kind_ext::FUNCTION_DECLARATION
            || k == syntax_kind_ext::FUNCTION_EXPRESSION
            || k == syntax_kind_ext::ARROW_FUNCTION =>
        {
            functions.push(node_idx);
            // Also collect nested functions in the body
            if let Some(func) = arena.get_function(node)
                && func.body.is_some()
            {
                collect_functions_from_node(arena, func.body, functions);
            }
        }
        k if k == syntax_kind_ext::METHOD_DECLARATION => {
            functions.push(node_idx);
            // Also collect nested functions in the body
            if let Some(method) = arena.get_method_decl(node)
                && method.body.is_some()
            {
                collect_functions_from_node(arena, method.body, functions);
            }
        }
        k if k == syntax_kind_ext::CLASS_DECLARATION => {
            if let Some(class) = arena.get_class(node) {
                for &member_idx in &class.members.nodes {
                    collect_functions_from_node(arena, member_idx, functions);
                }
            }
        }
        k if k == syntax_kind_ext::BLOCK => {
            if let Some(block) = arena.get_block(node) {
                for &stmt_idx in &block.statements.nodes {
                    collect_functions_from_node(arena, stmt_idx, &mut *functions);
                }
            }
        }
        k if k == syntax_kind_ext::VARIABLE_STATEMENT => {
            // Variable statement contains a declaration list which contains declarations
            if let Some(var_stmt) = arena.get_variable(node) {
                // var_stmt.declarations contains the VARIABLE_DECLARATION_LIST node(s)
                for &decl_list_idx in &var_stmt.declarations.nodes {
                    if let Some(decl_list_node) = arena.get(decl_list_idx) {
                        // The declaration list also uses VariableData
                        if let Some(decl_list) = arena.get_variable(decl_list_node) {
                            // Now decl_list.declarations contains the actual VARIABLE_DECLARATION nodes
                            for &decl_idx in &decl_list.declarations.nodes {
                                if let Some(decl_node) = arena.get(decl_idx)
                                    && let Some(decl) = arena.get_variable_declaration(decl_node)
                                    && decl.initializer.is_some()
                                {
                                    collect_functions_from_node(arena, decl.initializer, functions);
                                }
                            }
                        }
                    }
                }
            }
        }
        k if k == syntax_kind_ext::EXPORT_DECLARATION => {
            // Export declarations may contain function/class declarations
            if let Some(export) = arena.get_export_decl(node)
                && export.export_clause.is_some()
            {
                collect_functions_from_node(arena, export.export_clause, functions);
            }
        }
        _ => {}
    }
}

/// Type check function bodies in parallel
///
/// After binding is complete and symbols are merged, function bodies
/// can be type-checked in parallel because:
/// 1. Each function body only uses local variables and global symbols
/// 2. Local type inference doesn't modify global state
/// 3. Each function is independent
///
/// # Arguments
/// * `program` - The merged program with global symbols
///
/// # Returns
/// `CheckResult` with diagnostics from all functions
///
/// This is a reusable core/test-harness entry point, not the production CLI
/// diagnostic scheduler. CLI semantic-diagnostic behavior is owned by
/// `crates/tsz-cli/src/driver/check.rs::collect_diagnostics`, which also owns
/// file scheduling, cache/watch invalidation, checker reuse, and diagnostic
/// ordering.
pub fn check_functions_parallel(program: &MergedProgram) -> CheckResult {
    ensure_rayon_global_pool();

    let file_names: Vec<String> = program
        .files
        .iter()
        .map(|file| file.file_name.clone())
        .collect();
    let (resolved_module_paths, resolved_modules) =
        crate::checker::module_resolution::build_module_resolution_maps(&file_names);
    let resolved_module_paths = Arc::new(resolved_module_paths);

    let shared_binders: Vec<Arc<BinderState>> = program
        .files
        .iter()
        .enumerate()
        .map(|(file_idx, file)| Arc::new(create_binder_from_bound_file(file, program, file_idx)))
        .collect();
    let all_binders = Arc::new(shared_binders.clone());
    let all_arenas = Arc::new(
        program
            .files
            .iter()
            .map(|file| Arc::clone(&file.arena))
            .collect::<Vec<_>>(),
    );
    // PERF: Build arena-pointer -> file-index reverse lookup map first (O(F)),
    // then map each symbol to its file index in O(1) per symbol.
    // Total: O(S + F) instead of the previous O(S * F) nested iteration.
    let arena_to_file_idx: FxHashMap<usize, usize> = all_arenas
        .iter()
        .enumerate()
        .map(|(idx, arena)| (Arc::as_ptr(arena) as usize, idx))
        .collect();
    let symbol_file_targets: Vec<(tsz_binder::SymbolId, usize)> = program
        .symbol_arenas
        .iter()
        .filter_map(|(sym_id, arena)| {
            arena_to_file_idx
                .get(&(Arc::as_ptr(arena) as usize))
                .map(|&file_idx| (*sym_id, file_idx))
        })
        .collect();

    // Pre-compute the symbol->file index as a shared read-only map.
    // Each checker gets an Arc clone (O(1)) instead of O(N) per-checker insertion.
    let global_symbol_file_index: Arc<FxHashMap<tsz_binder::SymbolId, usize>> = Arc::new(
        symbol_file_targets
            .iter()
            .copied()
            .collect::<FxHashMap<_, _>>(),
    );

    // First, collect all functions from all files (sequential)
    let mut all_functions: Vec<(usize, NodeIndex)> = Vec::new();

    for (file_idx, file) in program.files.iter().enumerate() {
        let functions = collect_functions(&file.arena, file.source_file);
        for func_idx in functions {
            all_functions.push((file_idx, func_idx));
        }
    }

    let function_count = all_functions.len();

    // Check functions in parallel
    // Note: We need to be careful here - CheckerState holds mutable references
    // For now, we group by file and check each file's functions together
    let file_results: Vec<FileCheckResult> = maybe_parallel_iter!(program.files)
        .enumerate()
        .map(|(file_idx, file)| {
            let functions = collect_functions(&file.arena, file.source_file);

            let binder = Arc::clone(&shared_binders[file_idx]);

            // Create a per-thread QueryCache for memoized evaluate_type/is_subtype_of calls.
            // Each thread gets its own cache using RefCell/Cell (no atomic overhead).
            let query_cache = tsz_solver::construction::QueryCache::new(&program.type_interner);

            // Create checker for this file, using the shared type interner
            let compiler_options = crate::checker::context::CheckerOptions::default();
            let mut checker = CheckerState::new_with_shared_def_store(
                &file.arena,
                binder.as_ref(),
                &query_cache,
                file.file_name.clone(),
                compiler_options, // default options for internal operations
                std::sync::Arc::clone(&program.definition_store),
            );
            checker.ctx.set_all_arenas(Arc::clone(&all_arenas));
            checker.ctx.set_all_binders(Arc::clone(&all_binders));
            checker.ctx.set_current_file_idx(file_idx);
            checker
                .ctx
                .set_resolved_module_paths(Arc::clone(&resolved_module_paths));
            checker.ctx.set_resolved_modules(resolved_modules.clone());
            checker
                .ctx
                .set_global_symbol_file_index(Arc::clone(&global_symbol_file_index));

            let mut function_results = Vec::new();

            for func_idx in functions {
                // Check the function
                let return_type = checker.get_type_of_node(func_idx);

                function_results.push(FunctionCheckResult {
                    function_idx: func_idx,
                    file_idx,
                    return_type,
                    diagnostics: Vec::new(), // Diagnostics are collected at file level
                });
            }

            // Collect diagnostics from checker
            let diagnostics = std::mem::take(&mut checker.ctx.diagnostics);

            FileCheckResult {
                file_idx,
                file_name: file.file_name.clone(),
                function_results,
                diagnostics,
            }
        })
        .collect();

    let diagnostic_count: usize = file_results.iter().map(|r| r.diagnostics.len()).sum();

    CheckResult {
        file_results,
        function_count,
        diagnostic_count,
    }
}

/// Type check full source files in parallel using Rayon.
///
/// Each file gets its own `CheckerState` with file-local mutable state, sharing
/// only thread-safe structures (`Arc`-wrapped arenas/binders, `DashMap`-backed
/// `TypeInterner` and `DefinitionStore`). Per-thread `QueryCache` instances use
/// `RefCell`/`Cell` for zero-overhead single-threaded caching within each file.
///
/// Diagnostics are sorted by `(start, code)` within each file and deduplicated
/// by `(start, code)` after collection, ensuring deterministic output regardless
/// of thread scheduling.
///
/// This helper remains lower-level reusable infrastructure. Production CLI
/// diagnostics flow through `collect_diagnostics` in `tsz-cli` so the driver can
/// preserve CLI diagnostics, cache invalidation, and file-session reuse policy.
/// Feature and fidelity fixes for CLI checking should usually start in that
/// scheduler before changing this helper.
pub fn check_files_parallel(
    program: &MergedProgram,
    checker_options: &CheckerOptions,
    lib_files: &[Arc<LibFile>],
) -> CheckResult {
    // Ensure Rayon global pool has adequate stack size for deep type-checking recursion.
    ensure_rayon_global_pool();

    let file_names: Vec<String> = program
        .files
        .iter()
        .map(|file| file.file_name.clone())
        .collect();
    let (resolved_module_paths, resolved_modules) =
        crate::checker::module_resolution::build_module_resolution_maps(&file_names);
    let resolved_module_paths = Arc::new(resolved_module_paths);

    let should_clone_libs_in_parallel = program.files.len() > 1;
    let checker_lib_files = clone_lib_files_for_checker(lib_files, should_clone_libs_in_parallel);

    // Create fresh checker lib contexts from cloned lib files (contains both arena and binder).
    // Wrapped in Arc so that per-file checkers and child delegations share
    // the same Vec with O(1) clone cost (single atomic refcount increment).
    let lib_contexts: Arc<Vec<LibContext>> = Arc::new(
        checker_lib_files
            .iter()
            .map(|lib| LibContext {
                arena: Arc::clone(&lib.arena),
                binder: Arc::clone(&lib.binder),
            })
            .collect(),
    );

    // PERF: Pre-compute merged augmentation data ONCE instead of per-file.
    // This reduces augmentation merging from O(N_files^2) to O(N_files).
    let shared_binder_data = SharedBinderData::from_program(&program.files);

    let shared_binders: Vec<Arc<BinderState>> = program
        .files
        .iter()
        .enumerate()
        .map(|(file_idx, file)| {
            Arc::new(create_binder_from_bound_file_with_shared(
                file,
                program,
                file_idx,
                &shared_binder_data,
            ))
        })
        .collect();
    let all_binders = Arc::new(shared_binders.clone());
    let all_arenas = Arc::new(
        program
            .files
            .iter()
            .map(|file| Arc::clone(&file.arena))
            .collect::<Vec<_>>(),
    );
    // PERF: Build arena-pointer -> file-index reverse lookup map first (O(F)),
    // then map each symbol to its file index in O(1) per symbol.
    // Total: O(S + F) instead of the previous O(S * F) nested iteration.
    let arena_to_file_idx: FxHashMap<usize, usize> = all_arenas
        .iter()
        .enumerate()
        .map(|(idx, arena)| (Arc::as_ptr(arena) as usize, idx))
        .collect();
    let symbol_file_targets: Vec<(tsz_binder::SymbolId, usize)> = program
        .symbol_arenas
        .iter()
        .filter_map(|(sym_id, arena)| {
            arena_to_file_idx
                .get(&(Arc::as_ptr(arena) as usize))
                .map(|&file_idx| (*sym_id, file_idx))
        })
        .collect();

    // Pre-compute the symbol->file index as a shared read-only map.
    // Each checker gets an Arc clone (O(1)) instead of O(N) per-checker insertion.
    let global_symbol_file_index: Arc<FxHashMap<tsz_binder::SymbolId, usize>> = Arc::new(
        symbol_file_targets
            .iter()
            .copied()
            .collect::<FxHashMap<_, _>>(),
    );

    // Pre-compute skeleton-derived declared modules ONCE and share via Arc.
    // Previously this was computed per-file inside the closure, rebuilding the
    // same FxHashSet/Vec on every file (O(N_files * N_modules) total work).
    let shared_declared_modules: Option<Arc<crate::checker::context::GlobalDeclaredModules>> =
        program.skeleton_index.as_ref().map(|skel| {
            let (exact, patterns) = skel.build_declared_module_sets();
            Arc::new(crate::checker::context::GlobalDeclaredModules::from_skeleton(exact, patterns))
        });

    // Initialize per-file delegation locks for parallel correctness.
    program
        .definition_store
        .init_file_locks(program.files.len());

    // Create a shared cross-file query cache for multi-file projects.
    // In projects like ts-toolbelt (242 files), the same type evaluations and
    // subtype checks are performed across many files. The shared cache uses
    // DashMap for thread-safe concurrent access and eliminates redundant
    // computation across parallel file checkers.
    let shared_query_cache = if program.files.len() > 1 {
        Some(tsz_solver::construction::SharedQueryCache::new())
    } else {
        None
    };

    // Closure that checks a single file and returns its result.
    // Extracted so both sequential and parallel paths use identical logic.
    let check_one_file = |file_idx: usize, file: &BoundFile| -> FileCheckResult {
        let binder = Arc::clone(&shared_binders[file_idx]);

        // Create a per-thread QueryCache for memoized evaluate_type/is_subtype_of calls.
        // Each thread gets its own cache using RefCell/Cell (no atomic overhead).
        // For multi-file projects, the shared cache provides L2 cross-file caching.
        let query_cache = if let Some(ref shared) = shared_query_cache {
            tsz_solver::construction::QueryCache::new_with_shared(&program.type_interner, shared)
        } else {
            tsz_solver::construction::QueryCache::new(&program.type_interner)
        };

        let mut checker = CheckerState::with_options_and_shared_def_store(
            &file.arena,
            binder.as_ref(),
            &query_cache,
            file.file_name.clone(),
            checker_options,
            std::sync::Arc::clone(&program.definition_store),
        );
        checker.ctx.set_all_arenas(Arc::clone(&all_arenas));

        // Use pre-computed skeleton-derived declared modules (shared via Arc::clone).
        if let Some(ref modules) = shared_declared_modules {
            checker
                .ctx
                .set_declared_modules_from_skeleton(Arc::clone(modules));
        }

        checker.ctx.set_all_binders(Arc::clone(&all_binders));
        checker.ctx.set_current_file_idx(file_idx);
        checker
            .ctx
            .set_resolved_module_paths(Arc::clone(&resolved_module_paths));
        checker.ctx.set_resolved_modules(resolved_modules.clone());
        checker
            .ctx
            .set_global_symbol_file_index(Arc::clone(&global_symbol_file_index));

        if !lib_contexts.is_empty() {
            checker
                .ctx
                .set_lib_contexts_shared(Arc::clone(&lib_contexts));
            checker.ctx.set_actual_lib_file_count(lib_contexts.len());
        }

        checker.check_source_file(file.source_file);

        let mut diagnostics = std::mem::take(&mut checker.ctx.diagnostics);

        // Sort diagnostics by position for deterministic output within each file.
        diagnostics.sort_by(|a, b| a.start.cmp(&b.start).then_with(|| a.code.cmp(&b.code)));

        suppress_parallel_ts2339_cascade_diagnostics(file.arena.as_ref(), &mut diagnostics);

        // Deduplicate within each file: same (start, code) = same diagnostic.
        diagnostics.dedup_by(|a, b| a.start == b.start && a.code == b.code);

        FileCheckResult {
            file_idx,
            file_name: file.file_name.clone(),
            function_results: Vec::new(),
            diagnostics,
        }
    };

    let affected_lib_interfaces = affected_lib_interface_names(program, &checker_lib_files);
    let affected_lib_extension_interfaces = if affected_lib_interfaces.is_empty() {
        FxHashSet::default()
    } else {
        affected_lib_extension_interface_names(
            program,
            &checker_lib_files,
            &affected_lib_interfaces,
        )
    };

    let check_one_lib = |lib_idx: usize, lib_file: &Arc<LibFile>| -> FileCheckResult {
        if !lib_file_contains_affected_interface(lib_file.as_ref(), &affected_lib_interfaces) {
            return FileCheckResult {
                file_idx: program.files.len() + lib_idx,
                file_name: lib_file.file_name.clone(),
                function_results: Vec::new(),
                diagnostics: Vec::new(),
            };
        }

        let query_cache = if let Some(ref shared) = shared_query_cache {
            tsz_solver::construction::QueryCache::new_with_shared(&program.type_interner, shared)
        } else {
            tsz_solver::construction::QueryCache::new(&program.type_interner)
        };

        let lib_bound_file =
            build_lib_bound_file_for_interface_checks(program, lib_file, &affected_lib_interfaces);
        let mut binder =
            create_binder_from_bound_file(&lib_bound_file, program, program.files.len());
        // PERF: `build_lib_bound_file_for_interface_checks` always seeds
        // `lib_bound_file.semantic_defs` as empty, so the previous
        // clone-then-overlay collapsed to a deep clone of `program.semantic_defs`
        // and Arc-wrapping the result. With `program.semantic_defs` now
        // `Arc`-shared, the fast path is one atomic refcount bump plus a
        // potential `Arc::make_mut` only when the per-lib map actually
        // contributes entries (currently never).
        if lib_bound_file.semantic_defs.is_empty() {
            binder.semantic_defs = Arc::clone(&program.semantic_defs);
        } else {
            let mut composed_semantic_defs = (*program.semantic_defs).clone();
            for (sym_id, entry) in lib_bound_file.semantic_defs.iter() {
                composed_semantic_defs.insert(*sym_id, entry.clone());
            }
            binder.semantic_defs = Arc::new(composed_semantic_defs);
        }

        let mut checker = CheckerState::with_options(
            &lib_bound_file.arena,
            &binder,
            &query_cache,
            lib_bound_file.file_name.clone(),
            checker_options,
        );
        checker.ctx.set_all_arenas(Arc::clone(&all_arenas));
        if let Some(ref modules) = shared_declared_modules {
            checker
                .ctx
                .set_declared_modules_from_skeleton(Arc::clone(modules));
        }
        checker.ctx.set_all_binders(Arc::clone(&all_binders));
        checker
            .ctx
            .set_resolved_module_paths(Arc::clone(&resolved_module_paths));
        checker.ctx.set_resolved_modules(resolved_modules.clone());
        checker
            .ctx
            .set_global_symbol_file_index(Arc::clone(&global_symbol_file_index));

        let other_lib_contexts: Vec<LibContext> = lib_contexts
            .iter()
            .enumerate()
            .filter(|(idx, _)| *idx != lib_idx)
            .map(|(_, ctx)| ctx.clone())
            .collect();
        checker.ctx.set_lib_contexts(other_lib_contexts);
        checker.ctx.set_actual_lib_file_count(lib_contexts.len());
        checker.prime_boxed_types();

        checker.check_source_file_interfaces_only_filtered_post_merge(
            lib_bound_file.source_file,
            &affected_lib_interfaces,
            &affected_lib_extension_interfaces,
        );

        let mut diagnostics = std::mem::take(&mut checker.ctx.diagnostics);
        diagnostics.sort_by(|a, b| a.start.cmp(&b.start).then_with(|| a.code.cmp(&b.code)));
        diagnostics.dedup_by(|a, b| a.start == b.start && a.code == b.code);

        FileCheckResult {
            file_idx: program.files.len() + lib_idx,
            file_name: lib_file.file_name.clone(),
            function_results: Vec::new(),
            diagnostics,
        }
    };

    let check_one_lib_baseline = |lib_idx: usize, lib_file: &Arc<LibFile>| -> FileCheckResult {
        if !lib_file_contains_affected_interface(lib_file.as_ref(), &affected_lib_interfaces) {
            return FileCheckResult {
                file_idx: program.files.len() + lib_idx,
                file_name: lib_file.file_name.clone(),
                function_results: Vec::new(),
                diagnostics: Vec::new(),
            };
        }

        let query_cache = tsz_solver::construction::QueryCache::new(&program.type_interner);

        let mut checker = CheckerState::with_options(
            &lib_file.arena,
            lib_file.binder.as_ref(),
            &query_cache,
            lib_file.file_name.clone(),
            checker_options,
        );
        let other_lib_contexts: Vec<LibContext> = lib_contexts
            .iter()
            .enumerate()
            .filter(|(idx, _)| *idx != lib_idx)
            .map(|(_, ctx)| ctx.clone())
            .collect();
        checker.ctx.set_lib_contexts(other_lib_contexts);
        checker.ctx.set_actual_lib_file_count(lib_contexts.len());
        checker.prime_boxed_types();
        checker.check_source_file_interfaces_only_filtered_post_merge(
            lib_file.root_index,
            &affected_lib_interfaces,
            &affected_lib_extension_interfaces,
        );

        let mut diagnostics = std::mem::take(&mut checker.ctx.diagnostics);
        diagnostics.sort_by(|a, b| a.start.cmp(&b.start).then_with(|| a.code.cmp(&b.code)));
        diagnostics.dedup_by(|a, b| a.start == b.start && a.code == b.code);

        FileCheckResult {
            file_idx: program.files.len() + lib_idx,
            file_name: lib_file.file_name.clone(),
            function_results: Vec::new(),
            diagnostics,
        }
    };

    let fingerprint = |file_name: &str, diag: &Diagnostic| {
        (
            file_name.to_owned(),
            diag.start,
            diag.code,
            diag.message_text.clone(),
        )
    };
    // Single-file optimization: skip Rayon overhead when there's only one file.
    // For multi-file projects, use parallel iteration via Rayon's work-stealing
    // scheduler. `par_iter().enumerate()` preserves input ordering (file_idx) so
    // results are deterministic regardless of which thread completes first.
    let mut file_results: Vec<FileCheckResult> = if program.files.len() <= 1 {
        program
            .files
            .iter()
            .enumerate()
            .map(|(file_idx, file)| check_one_file(file_idx, file))
            .collect()
    } else {
        maybe_parallel_iter!(program.files)
            .enumerate()
            .map(|(file_idx, file)| check_one_file(file_idx, file))
            .collect()
    };

    if affected_lib_interfaces.is_empty() {
        file_results.extend(
            checker_lib_files
                .iter()
                .enumerate()
                .map(|(lib_idx, lib_file)| FileCheckResult {
                    file_idx: program.files.len() + lib_idx,
                    file_name: lib_file.file_name.clone(),
                    function_results: Vec::new(),
                    diagnostics: Vec::new(),
                }),
        );
    } else {
        let baseline_lib_diagnostics: FxHashSet<(String, u32, u32, String)> = checker_lib_files
            .iter()
            .enumerate()
            .flat_map(|(lib_idx, lib_file)| {
                let file_result = check_one_lib_baseline(lib_idx, lib_file);
                let file_name = file_result.file_name.clone();
                file_result
                    .diagnostics
                    .into_iter()
                    .map(move |diag| fingerprint(&file_name, &diag))
            })
            .collect();

        file_results.extend(
            checker_lib_files
                .iter()
                .enumerate()
                .map(|(lib_idx, lib_file)| {
                    let mut file_result = check_one_lib(lib_idx, lib_file);
                    let file_name = file_result.file_name.clone();
                    file_result.diagnostics.retain(|diag| {
                        !baseline_lib_diagnostics.contains(&fingerprint(&file_name, diag))
                    });
                    file_result
                }),
        );
    }

    add_reexported_module_augmentation_enum_conflict_diagnostics(
        program,
        resolved_module_paths.as_ref(),
        &mut file_results,
    );
    suppress_parallel_import_shadowing_namespace_type_diagnostics(
        program,
        resolved_module_paths.as_ref(),
        &mut file_results,
    );
    add_parallel_global_augmentation_member_conflict_diagnostics(program, &mut file_results);

    let diagnostic_count: usize = file_results.iter().map(|r| r.diagnostics.len()).sum();

    CheckResult {
        file_results,
        function_count: 0,
        diagnostic_count,
    }
}

/// Pre-computed data shared across all file binders in a parallel check.
///
/// These are computed ONCE from the program's files and shared via Arc,
/// eliminating `O(N_files^2)` redundant iteration in `create_binder_from_bound_file()`.
pub struct SharedBinderData {
    /// Merged module augmentations from all files.
    pub merged_module_augmentations:
        rustc_hash::FxHashMap<String, Vec<crate::binder::ModuleAugmentation>>,
    /// Merged augmentation target modules from all files.
    pub merged_augmentation_target_modules: rustc_hash::FxHashMap<crate::binder::SymbolId, String>,
    /// Merged global augmentations from all files.
    pub merged_global_augmentations:
        rustc_hash::FxHashMap<String, Vec<crate::binder::GlobalAugmentation>>,
}

impl SharedBinderData {
    /// Build shared binder data from all files in one pass.
    pub fn from_program(files: &[BoundFile]) -> Self {
        let module_augmentation_keys = files
            .iter()
            .map(|file| file.module_augmentations.len())
            .sum();
        let augmentation_target_count = files
            .iter()
            .map(|file| file.augmentation_target_modules.len())
            .sum();
        let global_augmentation_keys = files
            .iter()
            .map(|file| file.global_augmentations.len())
            .sum();

        let mut merged_module_augmentations = rustc_hash::FxHashMap::with_capacity_and_hasher(
            module_augmentation_keys,
            Default::default(),
        );
        let mut merged_augmentation_target_modules =
            rustc_hash::FxHashMap::with_capacity_and_hasher(
                augmentation_target_count,
                Default::default(),
            );
        let mut merged_global_augmentations = rustc_hash::FxHashMap::with_capacity_and_hasher(
            global_augmentation_keys,
            Default::default(),
        );

        for file in files {
            for (spec, augs) in file.module_augmentations.iter() {
                merged_module_augmentations
                    .entry(spec.clone())
                    .or_insert_with(|| Vec::with_capacity(augs.len()))
                    .extend(augs.iter().map(|aug| {
                        crate::binder::ModuleAugmentation::with_arena(
                            aug.name.clone(),
                            aug.node,
                            Arc::clone(&file.arena),
                        )
                    }));
            }
            for (&sym_id, module_spec) in file.augmentation_target_modules.iter() {
                merged_augmentation_target_modules.insert(sym_id, module_spec.clone());
            }
            for (name, decls) in file.global_augmentations.iter() {
                merged_global_augmentations
                    .entry(name.clone())
                    .or_insert_with(|| Vec::with_capacity(decls.len()))
                    .extend(decls.iter().map(|aug| {
                        crate::binder::GlobalAugmentation::with_arena(
                            aug.node,
                            Arc::clone(&file.arena),
                            aug.flags,
                        )
                    }));
            }
        }

        Self {
            merged_module_augmentations,
            merged_augmentation_target_modules,
            merged_global_augmentations,
        }
    }
}

/// Create a `BinderState` from a `BoundFile` for type checking.
///
/// This path is retained for tsz-core callers that want the legacy per-file
/// subset of `declaration_arenas` (only non-local, non-lib-originated entries,
/// as captured in `BoundFile.declaration_arenas`). The CLI driver uses its own
/// path (`create_binder_from_bound_file_with_augmentations`) which shares the
/// program-wide map via `Arc::clone` — see the perf follow-up doc §3.2.
pub fn create_binder_from_bound_file(
    file: &BoundFile,
    program: &MergedProgram,
    file_idx: usize,
) -> BinderState {
    let declaration_arenas = Arc::clone(&file.declaration_arenas);
    let sym_to_decl_indices = Arc::clone(&file.sym_to_decl_indices);
    let symbol_arenas = Arc::clone(&file.symbol_arenas);

    // Merge per-file locals with program globals via the shared helper,
    // which short-circuits to an O(1) `Arc::clone` when one side is empty.
    let file_locals = program.build_merged_file_locals(file_idx);

    let mut binder = BinderState::from_bound_state_with_scopes_and_augmentations(
        BinderOptions::default(),
        program.symbols.clone(),
        file_locals,
        // Arc::clone is O(1); cross-file lookup binders share the per-file
        // map by reference instead of deep-cloning it.
        Arc::clone(&file.node_symbols),
        BinderStateScopeInputs {
            scopes: file.scopes.clone(),
            node_scope_ids: file.node_scope_ids.clone(),
            global_augmentations: Arc::clone(&file.global_augmentations),
            module_augmentations: Arc::clone(&file.module_augmentations),
            augmentation_target_modules: Arc::clone(&file.augmentation_target_modules),
            module_exports: program.module_exports.clone(),
            module_declaration_exports_publicly: file.module_declaration_exports_publicly.clone(),
            reexports: program.reexports.clone(),
            wildcard_reexports: program.wildcard_reexports.clone(),
            wildcard_reexports_type_only: program.wildcard_reexports_type_only.clone(),
            symbol_arenas,
            declaration_arenas,
            sym_to_decl_indices,
            cross_file_node_symbols: Arc::clone(&program.cross_file_node_symbols),
            shorthand_ambient_modules: program.shorthand_ambient_modules.clone(),
            flow_nodes: file.flow_nodes.clone(),
            // Arc::clone is O(1); cross-file lookup binders share the per-file
            // node_flow map by reference instead of deep-cloning it.
            node_flow: Arc::clone(&file.node_flow),
            switch_clause_to_switch: file.switch_clause_to_switch.clone(),
            expando_properties: file.expando_properties.clone(),
            alias_partners: program.alias_partners.clone(),
        },
    );

    binder.is_external_module = file.is_external_module;
    binder.file_features = file.file_features;
    binder.lib_symbol_reverse_remap = file.lib_symbol_reverse_remap.clone();
    binder.lib_binders = program.lib_binders.clone();
    binder.lib_symbol_ids = program.lib_symbol_ids.clone();
    binder.lib_type_namespace = Arc::new(program.build_lib_type_namespace(file_idx));

    // Compose semantic_defs: start with the global map (cross-file + lib entries)
    // then overlay the file's own entries. Per-file entries take precedence for
    // symbols declared in this file, ensuring file-scoped identity is authoritative.
    //
    // PERF: When the shared DefinitionStore is fully populated (parallel path),
    // semantic_defs are never read by the checker (warm_local_caches_from_shared_store
    // and resolve_cross_batch_heritage both skip when fully_populated=true).
    // Skip the expensive clone+overlay to avoid O(files * total_defs) work.
    if !program.definition_store.is_fully_populated() {
        if file.semantic_defs.is_empty() {
            binder.semantic_defs = Arc::clone(&program.semantic_defs);
        } else {
            let mut composed_semantic_defs = (*program.semantic_defs).clone();
            for (sym_id, entry) in file.semantic_defs.iter() {
                composed_semantic_defs.insert(*sym_id, entry.clone());
            }
            binder.semantic_defs = Arc::new(composed_semantic_defs);
        }
    }
    if let Some(root_scope) = binder.scopes.first() {
        binder.current_scope = root_scope.table.clone();
        binder.current_scope_id = crate::binder::ScopeId(0);
    }

    binder.declared_modules = program.declared_modules.clone();

    // Mark lib symbols as merged since the MergedProgram's symbol arena
    // contains all remapped lib symbols with unique global IDs.
    // This enables the fast path in get_symbol() that avoids cross-binder lookups.
    binder.set_lib_symbols_merged(true);

    binder
}

/// Create a `BinderState` from a `BoundFile` using pre-computed shared augmentation data.
///
/// This avoids the `O(N_files)` augmentation merge per file by reusing data computed once
/// via `SharedBinderData::from_program`. For ts-toolbelt (242 files), this eliminates
/// ~242 * 242 = 58,564 redundant augmentation iterations.
pub fn create_binder_from_bound_file_with_shared(
    file: &BoundFile,
    program: &MergedProgram,
    file_idx: usize,
    _shared: &SharedBinderData,
) -> BinderState {
    // Keep the legacy per-file subset behavior here (see `create_binder_from_bound_file`):
    // these paths are used by `check_files_parallel` and tests that expect the
    // binder's `declaration_arenas` to exclude lib-originated symbols.
    let declaration_arenas = Arc::clone(&file.declaration_arenas);
    let sym_to_decl_indices = Arc::clone(&file.sym_to_decl_indices);
    let symbol_arenas = Arc::clone(&file.symbol_arenas);

    // Merge per-file locals with program globals via the shared helper,
    // which short-circuits to an O(1) `Arc::clone` when one side is empty.
    let file_locals = program.build_merged_file_locals(file_idx);

    let mut binder = BinderState::from_bound_state_with_scopes_and_augmentations(
        BinderOptions::default(),
        program.symbols.clone(),
        file_locals,
        // Arc::clone is O(1); cross-file lookup binders share the per-file
        // map by reference instead of deep-cloning it.
        Arc::clone(&file.node_symbols),
        BinderStateScopeInputs {
            scopes: file.scopes.clone(),
            node_scope_ids: file.node_scope_ids.clone(),
            global_augmentations: Arc::clone(&file.global_augmentations),
            module_augmentations: Arc::clone(&file.module_augmentations),
            augmentation_target_modules: Arc::clone(&file.augmentation_target_modules),
            module_exports: program.module_exports.clone(),
            module_declaration_exports_publicly: file.module_declaration_exports_publicly.clone(),
            reexports: program.reexports.clone(),
            wildcard_reexports: program.wildcard_reexports.clone(),
            wildcard_reexports_type_only: program.wildcard_reexports_type_only.clone(),
            symbol_arenas,
            declaration_arenas,
            sym_to_decl_indices,
            cross_file_node_symbols: Arc::clone(&program.cross_file_node_symbols),
            shorthand_ambient_modules: program.shorthand_ambient_modules.clone(),
            flow_nodes: file.flow_nodes.clone(),
            // Arc::clone is O(1); cross-file lookup binders share the per-file
            // node_flow map by reference instead of deep-cloning it.
            node_flow: Arc::clone(&file.node_flow),
            switch_clause_to_switch: file.switch_clause_to_switch.clone(),
            expando_properties: file.expando_properties.clone(),
            alias_partners: program.alias_partners.clone(),
        },
    );

    binder.is_external_module = file.is_external_module;
    binder.file_features = file.file_features;
    binder.lib_symbol_reverse_remap = file.lib_symbol_reverse_remap.clone();
    binder.lib_binders = program.lib_binders.clone();
    binder.lib_symbol_ids = program.lib_symbol_ids.clone();
    binder.lib_type_namespace = Arc::new(program.build_lib_type_namespace(file_idx));

    if !program.definition_store.is_fully_populated() {
        if file.semantic_defs.is_empty() {
            binder.semantic_defs = Arc::clone(&program.semantic_defs);
        } else {
            let mut composed_semantic_defs = (*program.semantic_defs).clone();
            for (sym_id, entry) in file.semantic_defs.iter() {
                composed_semantic_defs.insert(*sym_id, entry.clone());
            }
            binder.semantic_defs = Arc::new(composed_semantic_defs);
        }
    }
    if let Some(root_scope) = binder.scopes.first() {
        binder.current_scope = root_scope.table.clone();
        binder.current_scope_id = crate::binder::ScopeId(0);
    }

    binder.declared_modules = program.declared_modules.clone();
    binder.set_lib_symbols_merged(true);

    binder
}

/// Check function bodies with statistics
pub fn check_functions_with_stats(program: &MergedProgram) -> (CheckResult, CheckStats) {
    let result = check_functions_parallel(program);

    let stats = CheckStats {
        file_count: result.file_results.len(),
        function_count: result.function_count,
        diagnostic_count: result.diagnostic_count,
    };

    (result, stats)
}

/// Statistics about parallel type checking
#[derive(Debug, Clone)]
pub struct CheckStats {
    /// Number of files checked
    pub file_count: usize,
    /// Number of functions checked
    pub function_count: usize,
    /// Number of diagnostics produced
    pub diagnostic_count: usize,
}

/// Parse files and collect statistics
pub fn parse_files_with_stats(files: Vec<(String, String)>) -> (Vec<ParseResult>, ParallelStats) {
    let total_bytes: usize = files.iter().map(|(_, src)| src.len()).sum();
    let file_count = files.len();

    let results = parse_files_parallel(files);

    let total_nodes: usize = results.iter().map(|r| r.arena.len()).sum();
    let error_count: usize = results.iter().map(|r| r.parse_diagnostics.len()).sum();

    let stats = ParallelStats {
        file_count,
        total_bytes,
        total_nodes,
        error_count,
    };

    (results, stats)
}

#[cfg(test)]
#[path = "../../tests/parallel_tests.rs"]
mod tests;

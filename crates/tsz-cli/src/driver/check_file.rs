use super::*;

pub(super) struct CheckFileForParallelContext<'a> {
    pub(super) file_idx: usize,
    pub(super) binder: BinderState,
    pub(super) program: &'a MergedProgram,
    pub(super) compiler_options: &'a tsz_common::CheckerOptions,
    /// Project-wide shared environment — replaces individual `lib_contexts`, `all_arenas`,
    /// `all_binders`, skeleton indices, `symbol_file_targets`, `resolved_module_paths/errors`,
    /// `is_external_module_by_file`, `file_is_esm_map`, `typescript_dom_replacement_globals`,
    /// and `has_deprecation_diagnostics` fields.
    pub(super) program_context: &'a tsz::checker::context::ProgramContext,
    /// Per-file pre-bucketed resolved module specifiers (indexed by `file_idx`).
    /// Replaces a previous per-file scan over the program-wide
    /// `resolved_module_specifiers` set, which made each per-file checker
    /// scale with the size of the WHOLE program rather than its own
    /// import count.
    pub(super) resolved_modules_per_file: &'a Arc<Vec<Arc<rustc_hash::FxHashSet<String>>>>,
    pub(super) shared_lib_cache: Arc<dashmap::DashMap<String, Option<tsz_solver::TypeId>>>,
    /// Shared cross-file query cache for multi-file projects.
    /// Eliminates redundant type evaluations and relation checks across files.
    pub(super) shared_query_cache: Option<&'a tsz_solver::construction::SharedQueryCache>,
    pub(super) no_check: bool,
    pub(super) check_js: bool,
    /// `true` when `checkJs: false` was explicitly specified in compiler options.
    /// When set, ALL semantic errors are suppressed for JS files, including the
    /// `plainJSErrors` allowlist that would otherwise survive the filter.
    pub(super) explicit_check_js_false: bool,
    pub(super) skip_lib_check: bool,
    pub(super) program_has_real_syntax_errors: bool,
    pub(super) program_has_unsupported_js_root: bool,
    /// When `false`, per-file `TypeCache` extraction is skipped entirely.
    /// `TypeCache` is used by the emit pipeline (JS / declaration files) and
    /// by incremental cache reuse. For a `--noEmit` run that does not also
    /// request `--declaration`, nothing consumes it, and extracting it for
    /// every one of N files pins several hash maps per file in memory
    /// throughout the whole check (observed at ~10 GB RSS peak on a
    /// 6000-file repo). Set this `false` in that case.
    pub(super) extract_type_cache: bool,
}

pub(super) fn collect_no_check_file_diagnostics(
    file: &tsz::parallel::BoundFile,
    options: &ResolvedCompilerOptions,
    program_has_real_syntax_errors: bool,
) -> Vec<Diagnostic> {
    collect_no_check_parse_diagnostics_for_file(
        &file.file_name,
        &file.arena,
        file.source_file,
        &file.parse_diagnostics,
        options,
        program_has_real_syntax_errors,
    )
}

/// Per-file `CheckerContext` configuration extracted from
/// `check_file_for_parallel`. Sets the fields that vary across files in
/// a program — file index, ESM-ness, resolved modules, and the seven
/// parse-diagnostic-derived fields the checker reads to suppress or
/// classify diagnostics in syntax-error files.
///
/// The split between construction and per-file configuration is the
/// seam `PERFORMANCE_PLAN.md` §6 T2.1.B's sequential session-reuse
/// path will plug into: construct the `CheckerContext` once, then
/// repeatedly call this helper, `check_source_file()`, and
/// `reset_for_next_file()` rather than constructing a fresh
/// `CheckerState` per file.
///
/// This commit only does the extraction; the reuse loop itself is a
/// separate sub-PR.
///
/// Pure refactor: the field assignments and their derivations are
/// byte-for-byte identical to the inline version, so default behavior
/// is unchanged.
fn configure_checker_per_file<'a>(
    ctx: &mut tsz::checker::context::CheckerContext<'a>,
    file: &tsz::parallel::BoundFile,
    file_idx: usize,
    program_context: &tsz::checker::context::ProgramContext,
    resolved_modules: Arc<rustc_hash::FxHashSet<String>>,
    program_has_real_syntax_errors: bool,
) {
    ctx.set_current_file_idx(file_idx);
    ctx.file_is_esm = program_context
        .file_is_esm_map
        .get(&file.file_name)
        .copied();
    ctx.resolved_modules = Some(resolved_modules);
    // TSC suppresses many semantic diagnostics across the whole program when any
    // file has a real syntax parse error; mirror that behavior using the program-level
    // flag so that diagnostics like TS1361/TS1362 do not leak from syntax-error files.
    ctx.has_parse_errors = program_has_real_syntax_errors;
    // Exclude grammar checks that don't affect AST structure from
    // has_syntax_parse_errors so we match TSC's hasParseDiagnostics() behavior.
    //   TS1009 - Trailing comma (checker grammar error in TSC)
    //   TS1014 - Rest parameter must be last (grammar check, AST is valid)
    //   TS1185 - Merge conflict marker (not a real parse failure)
    ctx.has_syntax_parse_errors = file
        .parse_diagnostics
        .iter()
        .any(|d| !is_non_suppressing_parse_error(d.code));
    ctx.syntax_parse_error_positions = file
        .parse_diagnostics
        .iter()
        .filter(|d| !is_non_suppressing_parse_error(d.code))
        .map(|d| d.start)
        .collect();
    ctx.all_parse_error_positions = file.parse_diagnostics.iter().map(|d| d.start).collect();
    ctx.nullable_type_parse_error_positions = file
        .parse_diagnostics
        .iter()
        .filter(|d| (d.code == 17019 || d.code == 17020) && d.message.contains("'?'"))
        .map(|d| d.start)
        .collect();
    ctx.has_real_syntax_errors = file
        .parse_diagnostics
        .iter()
        .any(|d| is_real_syntax_error(d.code));
    ctx.has_structural_parse_errors = file
        .parse_diagnostics
        .iter()
        .any(|d| is_structural_parse_error(d.code));
    ctx.real_syntax_error_positions = file
        .parse_diagnostics
        .iter()
        .filter(|d| is_real_syntax_error(d.code))
        .map(|d| d.start)
        .collect();
}

/// Result of checking a single file for the parallel checking path: diagnostics,
/// optional `TypeCache` snapshot, per-file request counters, and solver
/// query-cache / definition-store statistics aggregated by the caller.
pub(super) type CheckFileResult = (
    Vec<Diagnostic>,
    Option<TypeCache>,
    RequestCacheCounters,
    tsz_solver::construction::QueryCacheStatistics,
    tsz_solver::StoreStatistics,
);

/// Boolean flags that govern per-file semantic checking behavior.
///
/// Shared by `run_check_on_existing_checker`,
/// `check_files_sequentially_with_reuse`, and
/// `check_files_in_parallel_chunks_with_reuse`.
pub(super) struct CheckFileFlags {
    pub(super) no_check: bool,
    pub(super) check_js: bool,
    /// `true` when `checkJs: false` was explicitly specified in compiler options.
    pub(super) explicit_check_js_false: bool,
    /// Skip type checking for declaration files (`.d.ts`).
    pub(super) skip_lib_check: bool,
    pub(super) program_has_real_syntax_errors: bool,
    pub(super) program_has_unsupported_js_root: bool,
    /// When `false`, per-file `TypeCache` extraction is skipped entirely.
    pub(super) extract_type_cache: bool,
}

/// Check a single file for the parallel checking path.
///
/// This is extracted from the work queue loop so it can be called from rayon's `par_iter`.
/// Each invocation creates its own `CheckerState` (with its own mutable context)
/// and its own `QueryCache` (using `RefCell`/`Cell` for zero-overhead single-threaded caching).
/// The `TypeInterner` is shared across threads via `DashMap` (thread-safe).
/// Run `check_source_file` on a fully-configured `CheckerState`, then
/// post-process and shape the resulting `Vec<Diagnostic>`.
///
/// Extracted from `check_file_for_parallel` so the T2.1.B sequential
/// session-reuse path (`PERFORMANCE_PLAN.md` §6) can reuse the same
/// per-file check pipeline against a `CheckerState` that's been
/// re-targeted at the next file via `CheckerContext::switch_to_file`,
/// instead of constructing a fresh checker per file.
///
/// **Pure refactor**: the body is byte-for-byte the post-
/// `configure_checker_per_file` portion of `check_file_for_parallel`.
/// Default behavior is unchanged because the same function is called
/// in the same order with the same arguments.
///
/// Caller's contract:
///
/// - `checker` has been constructed for `file` and configured via
///   `configure_checker_per_file` (or `switch_to_file` →
///   `configure_checker_per_file`).
/// - `program_context.apply_to(&mut checker.ctx)` has been called.
/// - `checker.ctx.diagnostics` is drained at function entry — anything
///   left over from a prior file is appended to this file's output.
///   In practice this means callers reusing a `CheckerState` across
///   files must have invoked `switch_to_file` (which drains
///   diagnostics via `reset_for_next_file`) before this function.
fn run_check_on_existing_checker<'a>(
    checker: &mut CheckerState<'a>,
    file: &tsz::parallel::BoundFile,
    compiler_options: &tsz_common::CheckerOptions,
    program_context: &tsz::checker::context::ProgramContext,
    flags: &CheckFileFlags,
) -> Vec<Diagnostic> {
    let &CheckFileFlags {
        no_check,
        check_js,
        explicit_check_js_false,
        program_has_real_syntax_errors,
        program_has_unsupported_js_root,
        ..
    } = flags;
    let filtered_parse_diagnostics =
        filtered_parse_diagnostics(&file.parse_diagnostics, program_has_real_syntax_errors);
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
    //
    // Under `--noCheck --declaration`, declaration emit still needs the
    // checker's inferred types (return types, contextual property types,
    // etc.) — tsc runs the checker for declaration emit even when
    // `--noCheck` is set (#3733). Run the checker pass when either the
    // user wants normal checking OR we need type information for
    // declaration emit; in the latter case the produced diagnostics are
    // discarded so `--noCheck` still suppresses type errors.
    let run_checker_for_decl_emit = no_check && compiler_options.emit_declarations;
    if !no_check || run_checker_for_decl_emit {
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
            program_has_unsupported_js_root,
            program_context.has_deprecation_diagnostics,
        );

        if !no_check {
            file_diagnostics.extend(checker_diagnostics);
        } else if compiler_options.isolated_declarations {
            // `--noCheck` suppresses type errors, but the
            // `--isolatedDeclarations` family (TS9007–TS9039) gates
            // declaration emission and tsc still surfaces those codes
            // (#3709). Keep them, drop everything else.
            file_diagnostics.extend(
                checker_diagnostics
                    .into_iter()
                    .filter(|d| matches!(d.code, 9007..=9039)),
            );
        }
    }

    // Final JS-specific filter: remove any remaining grammar codes that
    // tsc doesn't emit for JS files.
    if is_js {
        file_diagnostics.retain(|d| !is_checker_grammar_code_suppressed_in_js(d.code));
    }

    // Apply @ts-expect-error / @ts-ignore directive suppression only when type
    // checking ran. Under `--noCheck`, parse and JS grammar diagnostics still
    // surface in tsc and directives do not hide them.
    if !no_check && let Some(source) = file.arena.get_source_file_at(file.source_file) {
        apply_ts_directive_suppression(
            &file.file_name,
            source.text.as_ref(),
            &mut file_diagnostics,
            compiler_options.emit_declarations && check_js && is_js,
        );
    }

    file_diagnostics
}

pub(super) fn check_file_for_parallel<'a>(
    context: CheckFileForParallelContext<'a>,
) -> CheckFileResult {
    let CheckFileForParallelContext {
        file_idx,
        binder,
        program,
        compiler_options,
        program_context,
        resolved_modules_per_file,
        shared_lib_cache,
        shared_query_cache,
        no_check,
        check_js,
        explicit_check_js_false,
        skip_lib_check,
        program_has_real_syntax_errors,
        program_has_unsupported_js_root,
        extract_type_cache,
    } = context;
    let file = &program.files[file_idx];
    // skipLibCheck: skip type checking of declaration files (.d.ts, .d.cts, .d.mts)
    if skip_lib_check && is_declaration_file(&file.file_name) {
        return (
            Vec::new(),
            None,
            RequestCacheCounters::default(),
            tsz_solver::construction::QueryCacheStatistics::default(),
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
    // Per-file `Arc::clone` is a single atomic increment — no deep copy of
    // the `FxHashSet<String>` contents. Saves ~120K string clones on the
    // 6086-file large-ts-repo fixture.
    let resolved_modules: Arc<FxHashSet<String>> = resolved_modules_per_file
        .get(file_idx)
        .cloned()
        .unwrap_or_else(|| Arc::new(FxHashSet::default()));

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
    program_context.apply_to(&mut checker.ctx);

    // Per-file `CheckerContext` configuration. Extracted into a helper
    // to seam construction from per-file configuration; T2.1.B's
    // sequential session-reuse path will reuse this entry point.
    configure_checker_per_file(
        &mut checker.ctx,
        file,
        file_idx,
        program_context,
        resolved_modules,
        program_has_real_syntax_errors,
    );
    let file_diagnostics = run_check_on_existing_checker(
        &mut checker,
        file,
        compiler_options,
        program_context,
        &CheckFileFlags {
            no_check,
            check_js,
            explicit_check_js_false,
            skip_lib_check,
            program_has_real_syntax_errors,
            program_has_unsupported_js_root,
            extract_type_cache,
        },
    );

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

/// Sequential session-reuse path for T2.1.B (`PERFORMANCE_PLAN.md` §6
/// PR table item T2.1.B: "Add a sequential session-reuse path behind
/// a flag").
///
/// Differences from the default `work_items.iter().map(check_file_for_parallel).collect()`
/// path:
///
/// 1. **One `CheckerState` for the entire loop** (vs. one per file).
///    Constructed lazily on the first non-skip-lib-check file so an
///    all-declaration-file `work_items` doesn't pay setup cost.
/// 2. **One `QueryCache` for the entire loop** (vs. one per file).
///    The shared L2 path (`shared_query_cache`) already shared a
///    cache across files when present; this path also reuses the
///    primary `QueryCache` across files when `shared_query_cache` is
///    `None`.
/// 3. **`program_context.apply_to` runs once** (vs. once per file).
///    The `apply_to` work — Arc-cloning shared program-level state
///    into `ctx`, warming the local caches from the shared
///    `DefinitionStore` — is identical across files and only
///    needs to land once. Subsequent files inherit it through the
///    same `ctx`.
/// 4. **Pre-built `Vec<BinderState>`** holds every file's binder for
///    the duration of the loop, satisfying `CheckerState`'s `&'a
///    BinderState` lifetime requirement. The fresh-per-file path
///    drops the binder at each iteration's end; this path holds
///    them all so the next `switch_to_file` call has a valid
///    `&BinderState` to swap to.
///
/// Per-file work that still happens N times:
/// - `configure_checker_per_file` (file-local config: `file_idx`,
///   `resolved_modules`, parse-error positions, etc.)
/// - `CheckerContext::switch_to_file` (drains file-local caches,
///   swaps `arena`/`binder`/`file_name`/`file_idx`)
/// - The actual `check_source_file` work and diagnostic
///   post-processing (via `run_check_on_existing_checker`)
///
/// Caller's contract: OPT-IN for sequential no-emit runs via
/// `TSZ_FILE_SESSION_REUSE=1` (see `file_session_reuse_requested`
/// for why this was flipped from default-on in PR #7521).
/// `TSZ_DISABLE_FILE_SESSION_REUSE=1` continues to force off. The
/// flag-off path goes through `check_file_for_parallel` per file
/// unchanged.
///
/// **Correctness gate**: this path must produce byte-identical
/// diagnostics to the flag-off path under any conformance fixture,
/// or it is wrong (`PERFORMANCE_PLAN.md` §6 T2.1.B `DoD` line). If a
/// future change introduces a divergence, the responsible change is
/// the one to fix, not the flag — the flag exists to *measure* the
/// allocation savings, not to gate behavior changes.
/// Shared infrastructure for the sequential and parallel session-reuse check paths.
///
/// Groups the program/options/context reference params so that
/// `check_files_sequentially_with_reuse` and
/// `check_files_in_parallel_chunks_with_reuse` stay under the
/// `clippy::too_many_arguments` threshold.
#[cfg(not(target_arch = "wasm32"))]
pub(super) struct CheckFilesReuseCtx<'a> {
    pub(super) program: &'a MergedProgram,
    pub(super) compiler_options: &'a tsz_common::CheckerOptions,
    pub(super) program_context: &'a tsz::checker::context::ProgramContext,
    pub(super) resolved_modules_per_file: &'a Arc<Vec<Arc<rustc_hash::FxHashSet<String>>>>,
    pub(super) shared_lib_cache: Arc<dashmap::DashMap<String, Option<tsz_solver::TypeId>>>,
    pub(super) shared_query_cache: Option<&'a tsz_solver::construction::SharedQueryCache>,
}

#[cfg(not(target_arch = "wasm32"))]
pub(super) fn check_files_sequentially_with_reuse<F>(
    work_items: &[usize],
    ctx: &CheckFilesReuseCtx<'_>,
    flags: &CheckFileFlags,
    build_checker_binder: F,
) -> Vec<CheckFileResult>
where
    F: Fn(usize) -> tsz_binder::BinderState,
{
    let CheckFilesReuseCtx {
        program,
        compiler_options,
        program_context,
        resolved_modules_per_file,
        shared_lib_cache,
        shared_query_cache,
    } = ctx;
    let &CheckFileFlags {
        skip_lib_check,
        program_has_real_syntax_errors,
        extract_type_cache,
        ..
    } = flags;
    // Pre-build every binder via the caller-provided closure. Each
    // file's `CheckerContext::binder` is a `&'a BinderState`, so the
    // binders must outlive the `CheckerState` we construct below;
    // collecting into a `Vec` owned by this function satisfies that.
    // The closure form lets the caller hold the module-resolution
    // tables (`cached_module_specifiers`, `resolved_module_paths`,
    // `merged_augmentations`) in its own scope without threading them
    // through this function's signature.
    let binders: Vec<tsz_binder::BinderState> = work_items
        .iter()
        .map(|&file_idx| build_checker_binder(file_idx))
        .collect();

    // One `QueryCache` for the whole loop. Mirrors the per-file
    // construction in `check_file_for_parallel`, but built once.
    let query_cache = if let Some(shared) = shared_query_cache {
        QueryCache::new_with_shared(&program.type_interner, shared)
    } else {
        QueryCache::new(&program.type_interner)
    };

    let mut results: Vec<CheckFileResult> = Vec::with_capacity(work_items.len());
    let mut checker: Option<CheckerState> = None;

    for (loop_idx, &file_idx) in work_items.iter().enumerate() {
        let file = &program.files[file_idx];

        // skipLibCheck: skip type checking of declaration files. Same
        // contract as `check_file_for_parallel`'s early-return; we
        // emit an empty result and do *not* touch the shared
        // `CheckerState` for this file.
        if skip_lib_check && is_declaration_file(&file.file_name) {
            results.push((
                Vec::new(),
                None,
                RequestCacheCounters::default(),
                tsz_solver::construction::QueryCacheStatistics::default(),
                tsz_solver::StoreStatistics::default(),
            ));
            continue;
        }

        let resolved_modules: Arc<rustc_hash::FxHashSet<String>> = resolved_modules_per_file
            .get(file_idx)
            .cloned()
            .unwrap_or_else(|| Arc::new(rustc_hash::FxHashSet::default()));

        // Lazy construction on the first non-skipped file. After this,
        // subsequent iterations use `switch_to_file` to re-target the
        // same `CheckerState` at the next file.
        if checker.is_none() {
            let mut state = CheckerState::with_options_deferred_def_store(
                &file.arena,
                &binders[loop_idx],
                &query_cache,
                file.file_name.clone(),
                compiler_options,
            );
            state.ctx.report_unresolved_imports = true;
            state.ctx.shared_lib_type_cache = Some(Arc::clone(shared_lib_cache));
            // `apply_to` is the expensive setup we're amortising:
            // shared `DefinitionStore`, shared global indices,
            // resolved-module maps, file-is-ESM map, etc. Running it
            // once vs. N-times is the headline win for this path.
            program_context.apply_to(&mut state.ctx);
            if state.ctx.has_lib_loaded() {
                state.prime_boxed_types();
            }
            checker = Some(state);
        } else if let Some(ref mut state) = checker {
            state.ctx.switch_to_file(
                &file.arena,
                &binders[loop_idx],
                file.file_name.clone(),
                file_idx,
            );
        }

        let state = checker.as_mut().expect("checker constructed above");
        configure_checker_per_file(
            &mut state.ctx,
            file,
            file_idx,
            program_context,
            resolved_modules,
            program_has_real_syntax_errors,
        );

        let file_diagnostics =
            run_check_on_existing_checker(state, file, compiler_options, program_context, flags);

        let checker_counters = state.ctx.request_cache_counters;
        // `QueryCache::statistics()` is cumulative over the whole loop
        // because we reuse the same cache. The aggregator merges per-
        // file stats; emitting cumulative numbers N times would inflate
        // them. Emit them once on the last iteration to keep the
        // aggregator's invariant: sum of per-file QC stats == final
        // cumulative QC stats.
        let qc_stats = if loop_idx + 1 == work_items.len() {
            query_cache.statistics()
        } else {
            tsz_solver::construction::QueryCacheStatistics::default()
        };
        let ds_stats = tsz_solver::StoreStatistics::default();
        // The reuse path is gated on `!extract_type_cache` at the
        // call site; this loop never observes `extract_type_cache=true`,
        // so we always emit `None` for the per-file `TypeCache` slot.
        // See the call site in the sequential-branch dispatch for
        // the rationale.
        let type_cache = None;
        let _ = extract_type_cache;

        results.push((
            file_diagnostics,
            type_cache,
            checker_counters,
            qc_stats,
            ds_stats,
        ));
    }

    results
}

#[cfg(not(target_arch = "wasm32"))]
pub(super) fn check_files_in_parallel_chunks_with_reuse<F>(
    work_items: &[usize],
    ctx: &CheckFilesReuseCtx<'_>,
    flags: &CheckFileFlags,
    chunk_size: usize,
    build_checker_binder: &F,
) -> Vec<CheckFileResult>
where
    F: Fn(usize) -> tsz_binder::BinderState + Sync,
{
    use rayon::iter::ParallelIterator;
    use rayon::slice::ParallelSlice;

    debug_assert!(!flags.extract_type_cache);
    let chunk_size = chunk_size.max(1);
    work_items
        .par_chunks(chunk_size)
        .map(|chunk| {
            let chunk_ctx = CheckFilesReuseCtx {
                program: ctx.program,
                compiler_options: ctx.compiler_options,
                program_context: ctx.program_context,
                resolved_modules_per_file: ctx.resolved_modules_per_file,
                shared_lib_cache: Arc::clone(&ctx.shared_lib_cache),
                shared_query_cache: ctx.shared_query_cache,
            };
            check_files_sequentially_with_reuse(chunk, &chunk_ctx, flags, build_checker_binder)
        })
        .collect::<Vec<_>>()
        .into_iter()
        .flatten()
        .collect()
}

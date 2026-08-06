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
    /// Program-wide success tier for generic type-argument constraint proofs;
    /// see `tsz::checker::context::SharedConstraintProofCache`.
    pub(super) shared_constraint_proofs: Arc<tsz::checker::context::SharedConstraintProofCache>,
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

pub(super) struct CheckFileResultRecorder<'a> {
    pub(super) diagnostics: &'a mut Vec<Diagnostic>,
    pub(super) type_cache_output: &'a mut FxHashMap<PathBuf, TypeCache>,
    pub(super) per_file_ts7016_diagnostics: &'a [Vec<Diagnostic>],
    pub(super) request_cache_counters: &'a mut RequestCacheCounters,
    pub(super) query_cache_stats: &'a mut tsz_solver::construction::QueryCacheStatistics,
    pub(super) program: &'a MergedProgram,
}

impl CheckFileResultRecorder<'_> {
    pub(super) fn record(&mut self, file_idx: usize, result: CheckFileResult) {
        let (file_diags, type_cache, file_counters, qc_stats, _ds_stats) = result;
        self.diagnostics.extend(file_diags);
        self.diagnostics
            .extend(self.per_file_ts7016_diagnostics[file_idx].iter().cloned());
        self.request_cache_counters.merge(file_counters);
        self.query_cache_stats.merge(&qc_stats);
        if let Some(tc) = type_cache {
            let file_path = PathBuf::from(&self.program.files[file_idx].file_name);
            self.type_cache_output.insert(file_path, tc);
        }
    }
}

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
        skip_lib_check,
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

    if skip_lib_check && is_declaration_file(&file.file_name) {
        let check_start = tsz_common::perf_counters::enabled_fast().then(std::time::Instant::now);
        tsz::checker::reset_stack_overflow_flag();
        // This pass exists only to populate shared caches (lib symbol types,
        // heritage, cross-file state); every checker diagnostic it produces
        // is dropped before returning. Declare the discard up front so the
        // checker skips diagnostic presentation work entirely — failure
        // elaboration, type display, and spelling-suggestion candidate scans
        // — instead of formatting diagnostics that are thrown away below.
        // The flag is cleared again for session-reuse callers that check a
        // user file on this same `CheckerState` next (`reset_for_next_file`
        // does not touch it); top-level per-file checkers always arrive here
        // with the flag off.
        checker.ctx.diagnostics_discarded = true;
        checker.check_source_file(file.source_file);
        checker.ctx.diagnostics_discarded = false;
        tsz_common::perf_counters::record_interner_working_set_for_file();
        checker.ctx.diagnostics.clear();
        if let Some(start) = check_start {
            tsz_common::perf_counters::record_slow_check_file_timing(
                &file.file_name,
                start.elapsed().as_nanos() as u64,
                0,
            );
        }
        return file_diagnostics;
    }

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
        let check_start = tsz_common::perf_counters::enabled_fast().then(std::time::Instant::now);
        tsz::checker::reset_stack_overflow_flag();
        checker.check_source_file(file.source_file);
        // #13246: snapshot and reset the per-file interner working set at the
        // file boundary so the distinct-`TypeId` high-water / over-cache
        // buckets attribute thrash per file. No-op when counters are disabled.
        tsz_common::perf_counters::record_interner_working_set_for_file();
        // tsc reports at most one grammar diagnostic per parameter list. When
        // the checker's `check_parameter_ordering` won that race with a
        // TS1015/TS1016 on an earlier parameter, drop the parser-emitted
        // rest-parameter grammar diagnostics (TS1014/TS1047/TS1048) tsc's
        // single-early-return `checkGrammarParameterList` never reached. The
        // spans are recomputed per file (cleared at `check_source_file` entry),
        // so borrowing them here is safe across a reused checker.
        suppress_parameter_grammar_losers(
            &mut file_diagnostics,
            &checker.ctx.parameter_grammar_suppress_spans,
        );
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
        if let Some(start) = check_start {
            tsz_common::perf_counters::record_slow_check_file_timing(
                &file.file_name,
                start.elapsed().as_nanos() as u64,
                checker_diagnostics.len() as u64,
            );
        }

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
        shared_constraint_proofs,
        shared_query_cache,
        no_check,
        check_js,
        explicit_check_js_false,
        skip_lib_check,
        program_has_real_syntax_errors,
        program_has_unsupported_js_root,
        extract_type_cache,
    } = context;

    // Start every file's check from a clean per-thread guard slate so a prior
    // file that bailed mid-walk cannot leak a dirty guard onto the next file on
    // this reused worker thread. Both fresh-checker arms route through here; see
    // `reset_per_file_resolution_guards` docs for the full rationale (#13255).
    tsz::checker::reset_per_file_resolution_guards();

    let file = &program.files[file_idx];

    // Create a per-thread QueryCache (uses RefCell/Cell, no atomic overhead).
    // For multi-file projects, use shared L2 cache to avoid redundant computation.
    let mut query_cache = if let Some(shared) = shared_query_cache {
        QueryCache::new_with_shared(&program.type_interner, shared)
    } else {
        QueryCache::new(&program.type_interner)
    };
    // Attach the SAME shared DefinitionStore the per-file checker uses (installed
    // by `program_context.apply_to`) so generic-call inference can resolve
    // cross-arena declaration identity (issue #14344, `TSZ_XARENA_BASE_DECL`).
    if let Some(store) = program_context.shared_definition_store.as_deref() {
        query_cache = query_cache.with_definition_store(store);
    }

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
    checker.ctx.shared_constraint_proofs = Some(shared_constraint_proofs);

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
    pub(super) shared_constraint_proofs: Arc<tsz::checker::context::SharedConstraintProofCache>,
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
        shared_constraint_proofs,
        shared_query_cache,
    } = ctx;
    let &CheckFileFlags {
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
    let mut query_cache = if let Some(shared) = shared_query_cache {
        QueryCache::new_with_shared(&program.type_interner, shared)
    } else {
        QueryCache::new(&program.type_interner)
    };
    // Attach the SAME shared DefinitionStore the per-file checker uses so
    // generic-call inference can resolve cross-arena declaration identity
    // (issue #14344, `TSZ_XARENA_BASE_DECL`).
    if let Some(store) = program_context.shared_definition_store.as_deref() {
        query_cache = query_cache.with_definition_store(store);
    }

    let mut results: Vec<CheckFileResult> = Vec::with_capacity(work_items.len());
    let mut checker: Option<CheckerState> = None;

    for (loop_idx, &file_idx) in work_items.iter().enumerate() {
        let file = &program.files[file_idx];

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
            state.ctx.shared_constraint_proofs = Some(Arc::clone(shared_constraint_proofs));
            // `apply_to` is the expensive setup we're amortising:
            // shared `DefinitionStore`, shared global indices,
            // resolved-module maps, file-is-ESM map, etc. Running it
            // once vs. N-times is the headline win for this path.
            program_context.apply_to(&mut state.ctx);
            if state.ctx.has_lib_loaded() {
                state.prime_boxed_types();
            }
            state.prime_module_augmentation_bodies();
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
    tsz::parallel::ensure_rayon_global_pool();
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
                shared_constraint_proofs: Arc::clone(&ctx.shared_constraint_proofs),
                shared_query_cache: ctx.shared_query_cache,
            };
            check_files_sequentially_with_reuse(chunk, &chunk_ctx, flags, build_checker_binder)
        })
        .collect::<Vec<_>>()
        .into_iter()
        .flatten()
        .collect()
}

/// Check all `work_items` on a **bounded pool of `pool_size` long-lived
/// checkers** (the `TSZ_CHECKER_POOL` scheduler mode).
///
/// Unlike [`check_files_in_parallel_chunks_with_reuse`], which splits the work
/// into `ceil(files / chunk_size)` contiguous chunks — and therefore builds
/// `ceil(files / chunk_size)` `CheckerState`s — this distributes files into
/// exactly `pool_size` partitions. Each partition is one long-lived
/// `CheckerState` (built once via `apply_to`, re-targeted per file via
/// `switch_to_file`), so the expensive O(program) per-file setup is amortised
/// over `files / pool_size` files instead of `chunk_size`.
///
/// # Cost-balanced partitioning
///
/// Files are distributed by **estimated check cost** rather than statically
/// (`pos % pool_size`). A static round-robin split ignores per-file cost, so
/// under file-**size** skew — a few huge files among many tiny ones — one
/// partition can collect a disproportionate share of the heavy files and
/// become the straggler that bounds wall-time, even though the pool's
/// aggregate CPU is fine. We estimate each file's cost by its AST node count
/// (`arena.nodes.len()`, already materialised by binding, so no extra
/// traversal), sort files heaviest-first, and greedily place each into the
/// currently-lightest bin. This is the classic longest-processing-time (LPT)
/// makespan heuristic: bins finish with ~equal estimated cost, so wall-time
/// tracks the busiest **balanced** worker rather than the unluckiest static
/// partition. AST node count is a proxy (check cost is super-linear in some
/// shapes), but it captures the file-size skew this path targets.
///
/// Results are reassembled into the original `work_items` order (downstream
/// diagnostic aggregation is order-sensitive). The per-partition body is
/// [`check_files_sequentially_with_reuse`] verbatim, so the reset contract
/// (`CheckerContext::switch_to_file`) and stats accounting are unchanged, and
/// — because every file's result is stitched back by its original position —
/// diagnostics are byte-identical regardless of how files are partitioned.
/// Greedy longest-processing-time (LPT) assignment of items with the given
/// `costs` into `pool_size` bins; returns the chosen bin index for each item
/// (indexed as `costs`).
///
/// Items are placed heaviest-first into the currently-lightest bin. This is the
/// classic LPT makespan heuristic (a 4/3-approximation of the optimal): the
/// bin with the largest total cost — the straggler that bounds the pool's
/// wall-time — is provably close to the `total / pool_size` lower bound, so no
/// single bin collects a disproportionate share of the heavy items the way a
/// cost-blind `pos % pool_size` round-robin can. Ties (equal costs, equal bin
/// loads) break to the lowest index, so the assignment is deterministic and
/// independent of input ordering up to cost.
///
/// Complexity is `O(n log n + n * pool_size)` for `n = costs.len()`: the sort
/// dominates, and the per-item lightest-bin scan is cheap because `pool_size`
/// is bounded by the core count.
#[cfg(not(target_arch = "wasm32"))]
fn lpt_bin_assignment(costs: &[u64], pool_size: usize) -> Vec<usize> {
    let pool_size = pool_size.max(1);
    let mut order: Vec<usize> = (0..costs.len()).collect();
    order.sort_unstable_by(|&a, &b| costs[b].cmp(&costs[a]).then(a.cmp(&b)));

    let mut loads = vec![0u64; pool_size];
    let mut bin_of = vec![0usize; costs.len()];
    for idx in order {
        // Lightest bin wins; `min_by` returns the first element on ties and the
        // `.then(index)` comparator keeps that the lowest bin index.
        let bin = (0..pool_size)
            .min_by(|&a, &b| loads[a].cmp(&loads[b]).then(a.cmp(&b)))
            .expect("pool_size >= 1");
        loads[bin] += costs[idx];
        bin_of[idx] = bin;
    }
    bin_of
}

#[cfg(not(target_arch = "wasm32"))]
pub(super) fn check_files_cost_balanced_pool<F>(
    work_items: &[usize],
    ctx: &CheckFilesReuseCtx<'_>,
    flags: &CheckFileFlags,
    pool_size: usize,
    build_checker_binder: &F,
) -> Vec<CheckFileResult>
where
    F: Fn(usize) -> tsz_binder::BinderState + Sync,
{
    use rayon::iter::{IntoParallelIterator, ParallelIterator};

    debug_assert!(!flags.extract_type_cache);
    let pool_size = pool_size.max(1).min(work_items.len().max(1));

    // Estimate per-file check cost by AST node count (materialised during
    // binding, so no extra traversal) and assign files to `pool_size` bins by
    // longest-processing-time greedy bin-packing (see [`lpt_bin_assignment`]).
    let costs: Vec<u64> = work_items
        .iter()
        .map(|&file_idx| ctx.program.files[file_idx].arena.nodes.len().max(1) as u64)
        .collect();
    let bin_of = lpt_bin_assignment(&costs, pool_size);

    // Each file carries its original position so the partitions' results can be
    // stitched back into `work_items` order.
    let mut partitions: Vec<Vec<(usize, usize)>> = vec![Vec::new(); pool_size];
    for (pos, &bin) in bin_of.iter().enumerate() {
        partitions[bin].push((pos, work_items[pos]));
    }

    // Each partition runs on its own long-lived checker, in parallel. Bounding
    // to `pool_size` partitions bounds the number of live `CheckerState`s (and
    // their O(program) `apply_to` setups) to `pool_size`.
    tsz::parallel::ensure_rayon_global_pool();
    let partition_results: Vec<Vec<(usize, CheckFileResult)>> = partitions
        .into_par_iter()
        .map(|partition| {
            let partition_ctx = CheckFilesReuseCtx {
                program: ctx.program,
                compiler_options: ctx.compiler_options,
                program_context: ctx.program_context,
                resolved_modules_per_file: ctx.resolved_modules_per_file,
                shared_lib_cache: Arc::clone(&ctx.shared_lib_cache),
                shared_constraint_proofs: Arc::clone(&ctx.shared_constraint_proofs),
                shared_query_cache: ctx.shared_query_cache,
            };
            let (positions, file_list): (Vec<usize>, Vec<usize>) = partition.into_iter().unzip();
            let results = check_files_sequentially_with_reuse(
                &file_list,
                &partition_ctx,
                flags,
                build_checker_binder,
            );
            positions.into_iter().zip(results).collect::<Vec<_>>()
        })
        .collect();

    // Reassemble into original order. Every position is filled exactly once.
    let mut ordered: Vec<Option<CheckFileResult>> = (0..work_items.len()).map(|_| None).collect();
    for partition in partition_results {
        for (pos, result) in partition {
            ordered[pos] = Some(result);
        }
    }
    ordered
        .into_iter()
        .map(|slot| slot.expect("cost-balanced pool filled every work-item position"))
        .collect()
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod lpt_bin_assignment_tests {
    use super::lpt_bin_assignment;

    /// Total cost of the most-loaded bin — the straggler that bounds the pool's
    /// wall-time.
    fn max_bin_load(costs: &[u64], bins: &[usize], pool_size: usize) -> u64 {
        let mut loads = vec![0u64; pool_size];
        for (i, &b) in bins.iter().enumerate() {
            loads[b] += costs[i];
        }
        loads.into_iter().max().unwrap_or(0)
    }

    /// The previous static scheduler: file at position `p` goes to bin
    /// `p % pool_size`, ignoring cost.
    fn round_robin(n: usize, pool_size: usize) -> Vec<usize> {
        (0..n).map(|p| p % pool_size).collect()
    }

    #[test]
    fn assignment_is_deterministic() {
        let costs = [5, 1, 4, 1, 3, 9, 2, 6];
        assert_eq!(
            lpt_bin_assignment(&costs, 3),
            lpt_bin_assignment(&costs, 3),
            "same input must yield the same assignment"
        );
    }

    #[test]
    fn every_item_lands_in_a_valid_bin() {
        let costs = [7, 7, 7, 1, 1, 1, 1];
        let pool = 4;
        let bins = lpt_bin_assignment(&costs, pool);
        assert_eq!(bins.len(), costs.len());
        assert!(bins.iter().all(|&b| b < pool));
    }

    #[test]
    fn edge_cases() {
        // Empty input.
        assert!(lpt_bin_assignment(&[], 4).is_empty());
        // Single bin: everything in bin 0.
        assert_eq!(lpt_bin_assignment(&[3, 1, 2], 1), vec![0, 0, 0]);
        // Degenerate pool_size 0 is clamped to 1.
        assert_eq!(lpt_bin_assignment(&[3, 1], 0), vec![0, 0]);
        // Fewer items than bins: heaviest-first, one per bin.
        assert_eq!(lpt_bin_assignment(&[1, 3, 2], 8), vec![2, 0, 1]);
    }

    /// The headline property: when heavy files happen to align to the pool
    /// width, cost-blind round-robin piles them all into one straggler bin,
    /// while LPT spreads them and pads with the light files — reaching the
    /// theoretical `total / pool_size` makespan lower bound.
    #[test]
    fn lpt_beats_round_robin_under_aligned_skew() {
        let pool = 8;
        let n = 64;
        // Eight heavy files at positions 0, 8, 16, ... — all `≡ 0 (mod 8)`, so
        // round-robin sends every one of them to bin 0.
        let mut costs = vec![1u64; n];
        for k in 0..8 {
            costs[k * 8] = 100;
        }
        let total: u64 = costs.iter().sum();
        let lower_bound = total.div_ceil(pool as u64);

        let rr_max = max_bin_load(&costs, &round_robin(n, pool), pool);
        let lpt_max = max_bin_load(&costs, &lpt_bin_assignment(&costs, pool), pool);

        // Round-robin strands all eight heavies in one bin.
        assert_eq!(rr_max, 8 * 100, "round-robin clusters the aligned heavies");
        // LPT is at or near the optimal makespan, far below round-robin.
        assert!(
            lpt_max < rr_max / 4,
            "lpt makespan {lpt_max} should be far below round-robin {rr_max}"
        );
        // LPT never exceeds the lower bound plus one max-cost item (its 4/3
        // approximation guarantee comfortably implies this looser bound).
        let max_cost = *costs.iter().max().unwrap();
        assert!(
            lpt_max <= lower_bound + max_cost,
            "lpt makespan {lpt_max} exceeds lower_bound {lower_bound} + max_cost {max_cost}"
        );
    }

    /// Across a spread of continuous (power-law-ish) cost distributions and
    /// pool widths, every LPT assignment satisfies the always-true least-loaded
    /// greedy bound `makespan <= ceil(total / pool) + max_cost`, and LPT beats
    /// cost-blind round-robin in aggregate — the robustness claim that
    /// motivates replacing the static split. (Per-trial `lpt <= rr` is not
    /// asserted: LPT is a 4/3-approximation, so an unlucky round-robin can
    /// occasionally tie or edge it on a single shape; the aggregate cannot.)
    #[test]
    fn lpt_respects_greedy_bound_and_wins_in_aggregate() {
        let mut state: u64 = 0x9E37_79B9;
        let mut next = || {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            state
        };
        let (mut lpt_total, mut rr_total) = (0u64, 0u64);
        for _ in 0..200 {
            let n = 1 + (next() % 300) as usize;
            let pool = 1 + (next() % 16) as usize;
            let costs: Vec<u64> = (0..n)
                .map(|_| {
                    if next() % 16 == 0 {
                        50 + next() % 200
                    } else {
                        1
                    }
                })
                .collect();
            let total: u64 = costs.iter().sum();
            let max_cost = *costs.iter().max().unwrap();
            let lpt_max = max_bin_load(&costs, &lpt_bin_assignment(&costs, pool), pool);
            assert!(
                lpt_max <= total.div_ceil(pool as u64) + max_cost,
                "lpt {lpt_max} violated greedy bound for n={n} pool={pool}"
            );
            lpt_total += lpt_max;
            rr_total += max_bin_load(&costs, &round_robin(n, pool), pool);
        }
        assert!(
            lpt_total < rr_total,
            "lpt aggregate makespan {lpt_total} should beat round-robin {rr_total}"
        );
    }
}

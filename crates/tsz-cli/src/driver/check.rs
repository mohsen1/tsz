//! Diagnostics collection and per-file checking orchestration for the compilation driver.

use super::check_module_graph::*;
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

const fn checker_resolution_request_kind(
    kind: tsz::module_resolver::ImportKind,
) -> tsz::checker::context::ResolutionRequestKind {
    match kind {
        tsz::module_resolver::ImportKind::EsmImport => {
            tsz::checker::context::ResolutionRequestKind::EsmImport
        }
        tsz::module_resolver::ImportKind::DynamicImport => {
            tsz::checker::context::ResolutionRequestKind::DynamicImport
        }
        tsz::module_resolver::ImportKind::CjsRequire => {
            tsz::checker::context::ResolutionRequestKind::CjsRequire
        }
        tsz::module_resolver::ImportKind::EsmReExport => {
            tsz::checker::context::ResolutionRequestKind::EsmReExport
        }
    }
}

mod check_file;
#[cfg(test)]
mod check_tests;
mod checker_diagnostics;
mod checker_lib_diagnostics;
mod source_resolution_setup;

use check_file::{
    CheckFileFlags, CheckFileForParallelContext, CheckFileResult, CheckFilesReuseCtx,
    check_file_for_parallel, check_files_in_parallel_chunks_with_reuse,
    check_files_sequentially_with_reuse, collect_no_check_file_diagnostics,
};
#[cfg(test)]
use checker_diagnostics::LARGE_WILDCARD_BARREL_EXPORTS;
use checker_diagnostics::{
    has_large_wildcard_barrel, keep_checker_diagnostic_when_program_has_real_syntax_errors,
    post_process_checker_diagnostics, program_has_real_syntax_errors,
    program_has_unsupported_js_root, should_skip_type_checking_for_file,
};
use checker_lib_diagnostics::{
    CheckerLibFileCheckEnv, affected_lib_extension_interface_names, affected_lib_interface_names,
    baseline_lib_datetimeformatpart_spelling_interface_names, check_checker_lib_file,
    collect_checker_lib_baseline_diagnostics_for_codes, collect_checker_lib_baseline_fingerprints,
    has_esnext_umbrella_lib, has_parallel_order_sensitive_global_lib,
    is_datetimeformatpart_spelling_baseline_diagnostic, retain_program_induced_lib_diagnostics,
    should_preserve_datetimeformatpart_spelling_baseline,
};
use source_resolution_setup::{
    SourceResolutionSetup, SourceResolutionSetupInput, prepare_source_resolution_setup,
};

fn checker_lookup_resolution_mode(
    module_resolver: &mut ModuleResolver,
    options: &ResolvedCompilerOptions,
    file_path: &Path,
    import_kind: tsz::module_resolver::ImportKind,
    resolution_mode_override: Option<tsz::module_resolver::ImportingModuleKind>,
) -> Option<tsz::checker::context::ResolutionModeOverride> {
    use tsz::module_resolver::{ImportKind, ImportingModuleKind, ModuleExtension};

    let mode = resolution_mode_override.unwrap_or_else(|| {
        match import_kind {
            // Mirror ModuleResolver::resolve_with_kind_and_module_kind() so request-keyed
            // checker maps line up with the actual lookup mode used by the resolver.
            ImportKind::DynamicImport => ImportingModuleKind::Esm,
            ImportKind::CjsRequire => ImportingModuleKind::CommonJs,
            ImportKind::EsmImport | ImportKind::EsmReExport => match options.checker.module {
                ModuleKind::Preserve => {
                    let extension = ModuleExtension::from_path(file_path);
                    if extension.forces_esm() {
                        ImportingModuleKind::Esm
                    } else if extension.forces_cjs() {
                        ImportingModuleKind::CommonJs
                    } else {
                        ImportingModuleKind::Esm
                    }
                }
                _ => module_resolver.get_importing_module_kind(file_path),
            },
        }
    });

    checker_resolution_mode_override(Some(mode))
}

pub(super) struct CollectDiagnosticsResult {
    pub diagnostics: Vec<Diagnostic>,
    pub request_cache_counters: RequestCacheCounters,
    /// Aggregate query-cache statistics from the selected checking path.
    pub query_cache_stats: Option<tsz_solver::construction::QueryCacheStatistics>,
    /// Aggregate definition-store statistics (populated for `--extendedDiagnostics`).
    pub def_store_stats: Option<tsz_solver::StoreStatistics>,
    /// Module dependency graph statistics (populated for `--extendedDiagnostics`).
    pub module_dep_stats: Option<super::ModuleDependencyStats>,
}

#[derive(Default)]
pub(super) struct CheckerLibSet {
    pub(super) files: Vec<Arc<LibFile>>,
    pub(super) contexts: Arc<Vec<LibContext>>,
}

/// Check if a filename is a TypeScript declaration file (`.d.ts`, `.d.cts`,
/// `.d.mts`, or `.d.<ext>.ts`).
fn is_declaration_file(name: &str) -> bool {
    tsz::module_resolver::ModuleExtension::from_path(std::path::Path::new(name)).is_declaration()
}

#[cfg(test)]
thread_local! {
    static FILE_SESSION_REUSE_TEST_OVERRIDE: std::cell::Cell<Option<bool>> =
        const { std::cell::Cell::new(None) };
}

#[cfg(test)]
fn file_session_reuse_test_override() -> Option<bool> {
    FILE_SESSION_REUSE_TEST_OVERRIDE.with(std::cell::Cell::get)
}

// File-session reuse policy.
//
// Previously this defaulted to ON for all batch CLI projects (PRs #6870
// sequential and #6893 parallel), optimising the counter `state_constructed`
// on 40-400 file projects. At 1k+ files the reuse path regresses wall time by
// 4-14x; see PR #7521 and
// `docs/architecture/LSP_PERF_EXPERIMENTS_2026-05-16.md`. Measurements across
// the full scale-cliff matrix (monorepo-001..006) show reuse OFF is faster at
// every large fixture size we tested:
//
//   101 files:    1.5x faster off
//   1,010 files:  3.9x faster off
//   5,099 files:  4.6x faster off
//   5,251 files:  5.4x faster off (cross-pkg mapped types)
//   10,299 files: only finishes with reuse off (E8 1.47 M LOC synthetic)
//
// Tiny generated apps are a different regime where sequential fresh-checker
// setup dominates, but the reuse path is still not byte-identical for every
// conformance shape (alias display and checked-JS prototype evidence can
// observe retained state). Keep reuse opt-in until that semantic gap closes.
// Two env knobs remain:
//   * `TSZ_FILE_SESSION_REUSE=1` opts back in (legacy explicit-opt-in knob
//     from the pre-#6870 era).
//   * `TSZ_DISABLE_FILE_SESSION_REUSE=1` continues to force off, preserving
//     scripts that already pin the off behaviour. Takes precedence over
//     the enable knob.
//
// The LSP server binaries (`tsz_lsp`, `tsz_server`) do not consume this
// driver and are unaffected — they reuse state through the `tsz-lsp`
// `Project` API by construction.

const FILE_SESSION_REUSE_SMALL_PROJECT_MAX_FILES: usize = 32;

/// Pure policy function so tests can assert the env-var rules without
/// touching process-global state. `disable_set` is true when
/// `TSZ_DISABLE_FILE_SESSION_REUSE` is present in the environment;
/// `enable_set` is true when `TSZ_FILE_SESSION_REUSE` is present.
const fn file_session_reuse_from_env(disable_set: bool, enable_set: bool) -> bool {
    if disable_set {
        return false;
    }
    enable_set
}

const fn file_session_reuse_from_workload(
    disable_set: bool,
    enable_set: bool,
    _work_item_count: usize,
) -> bool {
    if disable_set {
        return false;
    }
    if enable_set {
        return true;
    }
    false
}

fn file_session_reuse_requested(work_item_count: usize) -> bool {
    #[cfg(test)]
    if let Some(enabled) = file_session_reuse_test_override() {
        return enabled;
    }

    file_session_reuse_from_workload(
        std::env::var_os("TSZ_DISABLE_FILE_SESSION_REUSE").is_some(),
        std::env::var_os("TSZ_FILE_SESSION_REUSE").is_some(),
        work_item_count,
    )
}

fn parallel_file_session_reuse_requested() -> bool {
    #[cfg(test)]
    if let Some(enabled) = file_session_reuse_test_override() {
        return enabled;
    }

    file_session_reuse_from_env(
        std::env::var_os("TSZ_DISABLE_FILE_SESSION_REUSE").is_some(),
        std::env::var_os("TSZ_FILE_SESSION_REUSE").is_some(),
    )
}

const fn needs_separate_boxed_prime_checker(
    no_emit: bool,
    emit_declarations: bool,
    reuse_requested: bool,
    file_count: usize,
    has_libs: bool,
) -> bool {
    if file_count == 0 || !has_libs {
        return false;
    }

    let reused_checker_covers_prime = no_emit
        && !emit_declarations
        && reuse_requested
        && file_count <= FILE_SESSION_REUSE_SMALL_PROJECT_MAX_FILES;
    !reused_checker_covers_prime
}

const FILE_SESSION_REUSE_PARALLEL_CHUNK_SIZE: usize = 8;

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
    let files = parallel::clone_lib_files_for_checker(lib_files, lib_files.len() > 1);
    let contexts = files
        .iter()
        .map(|lib| LibContext {
            arena: Arc::clone(&lib.arena),
            binder: Arc::clone(&lib.binder),
        })
        .collect();

    CheckerLibSet {
        files,
        contexts: Arc::new(contexts),
    }
}

/// Immutable, shared inputs to the diagnostics-collection pipeline.
///
/// Extracted from `collect_diagnostics_with_source_resolutions` to reduce the
/// parameter count below the `clippy::too_many_arguments` threshold while
/// keeping mutable and call-unique params (`cache`, `type_cache_output`,
/// `source_module_resolutions`) as separate arguments.
pub(super) struct CollectDiagnosticsInput<'a> {
    pub(super) program: &'a MergedProgram,
    pub(super) options: &'a ResolvedCompilerOptions,
    pub(super) base_dir: &'a Path,
    pub(super) checker_libs: &'a CheckerLibSet,
    pub(super) typescript_dom_replacement_globals: (bool, bool, bool),
    pub(super) has_deprecation_diagnostics: bool,
    pub(super) collect_compile_stats: bool,
}

type CachedModuleSpecifier = (
    String,
    tsz::parser::NodeIndex,
    tsz::module_resolver::ImportKind,
    Option<tsz::module_resolver::ImportingModuleKind>,
);

type ResolutionRequestMapKey = (
    usize,
    String,
    Option<tsz::checker::context::ResolutionModeOverride>,
    tsz::checker::context::ResolutionRequestKind,
);

#[cfg(test)]
pub(super) fn collect_diagnostics(
    input: &CollectDiagnosticsInput<'_>,
    cache: Option<&mut CompilationCache>,
    type_cache_output: &std::sync::Mutex<FxHashMap<PathBuf, TypeCache>>,
) -> CollectDiagnosticsResult {
    collect_diagnostics_with_source_resolutions(input, cache, type_cache_output, None)
}

pub(super) fn collect_diagnostics_with_source_resolutions(
    input: &CollectDiagnosticsInput<'_>,
    cache: Option<&mut CompilationCache>,
    type_cache_output: &std::sync::Mutex<FxHashMap<PathBuf, TypeCache>>,
    source_module_resolutions: Option<
        &FxHashMap<SourceModuleResolutionKey, SourceModuleResolution>,
    >,
) -> CollectDiagnosticsResult {
    let &CollectDiagnosticsInput {
        program,
        options,
        base_dir,
        checker_libs,
        typescript_dom_replacement_globals,
        has_deprecation_diagnostics,
        collect_compile_stats,
    } = input;
    let _collect_span =
        tracing::info_span!("collect_diagnostics", files = program.files.len()).entered();
    // Production CLI semantic diagnostics are scheduled here. Lower-level
    // helpers in `tsz::parallel` stay reusable infrastructure unless this
    // driver opts into changed CLI behavior.
    #[cfg(not(target_arch = "wasm32"))]
    tsz::parallel::ensure_rayon_global_pool();

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
    let program_has_unsupported_js_root = program_has_unsupported_js_root(program, options);

    // TS6504: when allowJs is disabled, emit one error per explicit JS root file.
    // tsc includes the JS file in the program but rejects it with this diagnostic
    // and skips semantic checks for that file (the suppression is in
    // post_process_checker_diagnostics).
    if program_has_unsupported_js_root {
        for file in &program.files {
            if is_js_file(Path::new(&file.file_name)) {
                let mut ts6504 = Diagnostic::from_code(
                    diagnostic_codes::FILE_IS_A_JAVASCRIPT_FILE_DID_YOU_MEAN_TO_ENABLE_THE_ALLOWJS_OPTION,
                    "",
                    0,
                    0,
                    &[&file.file_name],
                );
                ts6504
                    .related_information
                    .push(DiagnosticRelatedInformation {
                        category: DiagnosticCategory::Message,
                        code: diagnostic_codes::THE_FILE_IS_IN_THE_PROGRAM_BECAUSE,
                        file: String::new(),
                        start: 0,
                        length: 0,
                        message_text: "The file is in the program because:".to_string(),
                    });
                ts6504
                    .related_information
                    .push(DiagnosticRelatedInformation {
                        category: DiagnosticCategory::Message,
                        code: diagnostic_codes::ROOT_FILE_SPECIFIED_FOR_COMPILATION,
                        file: String::new(),
                        start: 0,
                        length: 0,
                        message_text: "Root file specified for compilation".to_string(),
                    });
                diagnostics.push(ts6504);
            }
        }
    }

    {
        let _span = tracing::info_span!("build_program_path_maps", files = file_count).entered();
        for (idx, file) in program.files.iter().enumerate() {
            let canonical = normalize_resolved_path(Path::new(&file.file_name), options);
            program_paths.insert(canonical.clone());
            canonical_to_file_name.insert(canonical.clone(), file.file_name.clone());
            canonical_to_file_idx.insert(canonical, idx);
        }
    }

    // Duplicate package redirect map
    let package_redirects: FxHashMap<PathBuf, PathBuf> = {
        let file_names: Vec<String> = program.files.iter().map(|f| f.file_name.clone()).collect();
        build_duplicate_package_redirects(&file_names, options)
    };
    let SourceResolutionSetup {
        cached_module_specifiers,
        resolved_module_paths,
        resolved_module_request_paths,
        resolved_module_ts_extension_flags,
        resolved_module_errors,
        resolved_module_request_errors,
        resolved_modules_per_file,
    } = prepare_source_resolution_setup(SourceResolutionSetupInput {
        program,
        options,
        base_dir,
        source_module_resolutions,
        canonical_to_file_idx: &canonical_to_file_idx,
        program_paths: &program_paths,
        package_redirects: &package_redirects,
        resolution_cache: &mut resolution_cache,
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
        let has_cjs_require_specifier = cached_module_specifiers.iter().any(|specifiers| {
            specifiers.iter().any(|(_, _, import_kind, _)| {
                matches!(import_kind, tsz::module_resolver::ImportKind::CjsRequire)
            })
        });
        if !has_cjs_require_specifier {
            vec![Vec::new(); program.files.len()]
        } else {
            program
                .files
                .par_iter()
                .enumerate()
                .map(|(file_idx, file)| {
                    let mut diags = Vec::new();
                    for (specifier, spec_node, import_kind, _) in
                        &cached_module_specifiers[file_idx]
                    {
                        if !matches!(import_kind, tsz::module_resolver::ImportKind::CjsRequire) {
                            continue;
                        }
                        if let Some(error) =
                            resolved_module_errors.get(&(file_idx, specifier.clone()))
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
        }
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
            let program_package_types: FxHashMap<PathBuf, bool> = program
                .files
                .iter()
                .filter_map(|file| {
                    let file_path = Path::new(&file.file_name);
                    if file_path.file_name().and_then(|name| name.to_str()) != Some("package.json")
                    {
                        return None;
                    }
                    let package_dir = file_path.parent()?.to_path_buf();
                    let text = file
                        .arena
                        .source_files
                        .first()
                        .map(|source_file| source_file.text.as_ref())?;
                    let package_type = serde_json::from_str::<serde_json::Value>(text)
                        .ok()
                        .and_then(|value| {
                            value
                                .get("type")
                                .and_then(serde_json::Value::as_str)
                                .map(|value| value == "module")
                        })
                        .unwrap_or(false);
                    Some((package_dir, package_type))
                })
                .collect();
            let mut package_type_cache = ModuleResolutionCache::default();
            program
                .files
                .iter()
                .map(|file| {
                    let file_path = Path::new(&file.file_name);
                    let file_is_esm = match file_path.extension().and_then(|ext| ext.to_str()) {
                        Some("mts" | "mjs") => true,
                        Some("cts" | "cjs") => false,
                        _ => {
                            let mut current = file_path.parent();
                            let mut from_program_package_json = None;
                            while let Some(dir) = current {
                                if let Some(&is_esm) = program_package_types.get(dir) {
                                    from_program_package_json = Some(is_esm);
                                    break;
                                }
                                current = dir.parent();
                            }
                            from_program_package_json.unwrap_or_else(|| {
                                implied_resolution_mode_for_file_with_cache(
                                    file_path,
                                    base_dir,
                                    &mut package_type_cache,
                                ) == "import"
                            })
                        }
                    };
                    (file.file_name.clone(), file_is_esm)
                })
                .collect()
        } else {
            FxHashMap::default()
        }
    });

    // The `--noCheck` short-circuit returns parse-only diagnostics and skips
    // the regular checker pipeline. That's correct when no declaration files
    // are being emitted, but `--noCheck --declaration` still needs the
    // checker's inferred type information so the declaration emitter can
    // print return types for unannotated functions, contextual types for
    // `const x = { a: 1 }`, etc. (#3733). When `emit_declarations` is set
    // we fall through to the regular pipeline (which still suppresses type
    // errors via the `if !no_check` guard around `check_source_file`); the
    // type_caches it produces feed declaration emit.
    if options.no_check && !options.emit_declarations {
        use rayon::prelude::*;

        let mut diagnostics: Vec<Diagnostic> = program
            .files
            .par_iter()
            .map(|file| {
                let mut file_diags = collect_no_check_file_diagnostics(
                    file,
                    options,
                    program_has_real_syntax_errors,
                );
                // tsc still reports the `--isolatedDeclarations` grammar
                // diagnostics (TS9007/TS9011/TS9012/etc.) under `--noCheck`
                // because they gate declaration emission, not type checking
                // (#3709). Run only the isolated-declaration grammar pass.
                if options.checker.isolated_declarations {
                    let mut binder = tsz_binder::state::BinderState::new();
                    binder.bind_source_file(&file.arena, file.source_file);
                    file_diags.extend(tsz::checker::run_isolated_declarations_pass(
                        &file.arena,
                        &binder,
                        file.source_file,
                        file.file_name.clone(),
                        options.checker.clone(),
                    ));
                }
                file_diags
            })
            .flatten()
            .collect();

        for (file_idx, file_diags) in per_file_ts7016_diagnostics.iter().enumerate() {
            diagnostics.extend(file_diags.iter().cloned());
            if let Some(file) = program.files.get(file_idx) {
                used_paths.insert(PathBuf::from(&file.file_name));
            }
        }

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

        let module_dep_stats = if collect_compile_stats {
            Some(compute_module_dependency_stats(
                program.files.len(),
                resolved_module_paths.as_ref(),
            ))
        } else {
            None
        };

        return CollectDiagnosticsResult {
            diagnostics,
            request_cache_counters,
            query_cache_stats: Some(tsz_solver::construction::QueryCacheStatistics::default()),
            def_store_stats: None,
            module_dep_stats,
        };
    }

    // `skipLibCheck` skips semantic checking for declaration files, but the
    // normal checker setup below still builds project-wide checker state before
    // discovering that every work item is skipped. Utility-type packages such
    // as type-fest are often pure `.d.ts` projects with `skipLibCheck`; for
    // pure no-emit checks, parse/module diagnostics are the only remaining
    // output. Return before constructing checker binders, `ProgramContext`, the
    // shared `DefinitionStore`, and lib recheck baselines. Mixed projects stay
    // on the normal path so `.ts` files can still consume declaration exports.
    if options.no_emit
        && options.skip_lib_check
        && !options.emit_declarations
        && program
            .files
            .iter()
            .all(|file| is_declaration_file(&file.file_name))
    {
        use rayon::prelude::*;

        let mut diagnostics: Vec<Diagnostic> = program
            .files
            .par_iter()
            .map(|file| {
                collect_no_check_file_diagnostics(file, options, program_has_real_syntax_errors)
            })
            .flatten()
            .collect();

        for (file_idx, file_diags) in per_file_ts7016_diagnostics.iter().enumerate() {
            diagnostics.extend(file_diags.iter().cloned());
            if let Some(file) = program.files.get(file_idx) {
                used_paths.insert(PathBuf::from(&file.file_name));
            }
        }

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

        let module_dep_stats = if collect_compile_stats {
            Some(compute_module_dependency_stats(
                program.files.len(),
                resolved_module_paths.as_ref(),
            ))
        } else {
            None
        };

        return CollectDiagnosticsResult {
            diagnostics,
            request_cache_counters,
            query_cache_stats: Some(tsz_solver::construction::QueryCacheStatistics::default()),
            def_store_stats: None,
            module_dep_stats,
        };
    }

    // Pre-compute merged augmentations once for all binder reconstruction paths.
    let merged_augmentations = MergedAugmentations::from_program(program);
    let can_recheck_checker_libs =
        !options.no_check && !options.skip_lib_check && !checker_libs.files.is_empty();
    let affected_lib_interfaces = if can_recheck_checker_libs {
        affected_lib_interface_names(program, checker_libs)
    } else {
        FxHashSet::default()
    };
    let affected_lib_extension_interfaces = if can_recheck_checker_libs {
        affected_lib_extension_interface_names(program, checker_libs, &affected_lib_interfaces)
    } else {
        FxHashSet::default()
    };
    let baseline_lib_datetimeformatpart_interfaces = if can_recheck_checker_libs {
        baseline_lib_datetimeformatpart_spelling_interface_names(checker_libs)
    } else {
        FxHashSet::default()
    };

    // Pre-create all binders for cross-file resolution.
    let all_binders: Arc<Vec<Arc<BinderState>>> = {
        let _span =
            tracing::info_span!("build_cross_file_binders", files = program.files.len()).entered();
        if program.files.len() <= FILE_SESSION_REUSE_SMALL_PROJECT_MAX_FILES {
            Arc::new(
                program
                    .files
                    .iter()
                    .enumerate()
                    .map(|(file_idx, file)| {
                        Arc::new(create_cross_file_lookup_binder_with_augmentations(
                            file,
                            program,
                            file_idx,
                            &merged_augmentations,
                        ))
                    })
                    .collect(),
            )
        } else {
            use rayon::prelude::*;
            Arc::new(
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
                    .collect(),
            )
        }
    };

    // Extract is_external_module from BoundFile to preserve state across file bindings.
    // This fixes TS2664 which requires accurate per-file is_external_module values.
    let is_external_module_by_file: Arc<rustc_hash::FxHashMap<String, bool>> = Arc::new(
        program
            .files
            .iter()
            .map(|file| (file.file_name.clone(), file.is_external_module))
            .collect(),
    );

    // Collect all arenas for cross-file resolution.
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
    let symbol_file_targets: Arc<Vec<(tsz::binder::SymbolId, usize)>> = {
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
        Arc::new(
            program
                .symbol_arenas
                .iter()
                .filter_map(|(sym_id, arena)| {
                    arena_ptr_to_idx
                        .get(&Arc::as_ptr(arena))
                        .map(|&file_idx| (*sym_id, file_idx))
                })
                .collect(),
        )
    };

    // Propagate noUncheckedIndexedAccess to the TypeInterner before creating the
    // QueryCache.  The `with_options` constructor intentionally skips this (to avoid
    // repeated writes from each per-file checker), so we set it once here.
    program
        .type_interner
        .set_no_unchecked_indexed_access(options.checker.no_unchecked_indexed_access);
    // Propagate exactOptionalPropertyTypes to the TypeInterner so that solver-side
    // queries (e.g. index-signature inference) see the same flag as the checker.
    program
        .type_interner
        .set_exact_optional_property_types(options.checker.exact_optional_property_types);

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
            Some(Arc::new(
                tsz::checker::context::GlobalDeclaredModules::from_module_names(
                    program
                        .declared_modules
                        .iter()
                        .chain(program.shorthand_ambient_modules.iter()),
                ),
            ))
        } else {
            None
        };

    // Pre-compute expando index from skeleton when available.
    // This avoids re-scanning all binders for expando property assignments.
    let skeleton_expando_index: Option<Arc<FxHashMap<String, FxHashSet<String>>>> = program
        .skeleton_index
        .as_ref()
        .map(|skel| Arc::clone(&skel.expando_properties));

    // Phase 2 step 2: pre-compute the merged module-augmentations index from
    // skeleton data. The skeleton recorded each augmenting declaration's name
    // + StableLocation at extract time; this projection rehydrates them into
    // the legacy `Vec<(file_idx, ModuleAugmentation)>` shape so checker
    // consumers (`module_augmentation.rs`, `property_access_augmentation.rs`)
    // see no behavior change. The legacy per-binder loop in
    // `ProgramContext::build_global_indices` is skipped when this is `Some`.
    let skeleton_module_augmentations_index: Option<
        tsz::checker::context::GlobalModuleAugmentationsIndex,
    > = program
        .skeleton_index
        .as_ref()
        .map(|skel| Arc::new(skel.build_module_augmentations_index(&all_arenas)));

    // Phase 2 step 3: pre-compute the merged augmentation-targets index from
    // skeleton data. The skeleton recorded each `(symbol, module_spec)` pair
    // (with a StableLocation) at extract time; this projection rehydrates
    // them into the legacy `Vec<(SymbolId, file_idx)>` shape so checker
    // consumers (`module_augmentation.rs`) see no behavior change. The legacy
    // per-binder loop in `ProgramContext::build_global_indices` is skipped when
    // this is `Some`.
    let skeleton_augmentation_targets_index: Option<
        tsz::checker::context::GlobalAugmentationTargetsIndex,
    > = program
        .skeleton_index
        .as_ref()
        .map(|skel| Arc::new(skel.build_augmentation_targets_index()));

    // Phase 2 step 4: pre-compute the merged module-binder index from
    // skeleton data. The skeleton recorded each file's `module_exports` keys
    // at extract time; this projection rebuilds the legacy
    // `module_spec -> Vec<file_idx>` map (including the de-quoted normalized
    // variant) so checker consumers (`import/declaration.rs`,
    // `module_entity.rs`, `type_resolution/module.rs`) see no behavior
    // change. The legacy `module_binder_index` push lines inside the
    // per-binder `module_exports.iter()` loop in
    // `ProgramContext::build_global_indices` are skipped when this is `Some`.
    let skeleton_module_binder_index: Option<Arc<FxHashMap<String, Vec<usize>>>> = program
        .skeleton_index
        .as_ref()
        .map(|skel| Arc::new(skel.build_module_binder_index()));

    // Phase 2 step 6: pre-compute the merged module-exports index from
    // skeleton data + the post-merge `program.module_exports` map. The
    // skeleton recorded each file's `(spec, [export_name])` entries at
    // extract time; this projection rebuilds the legacy
    // `spec -> export_name -> Vec<(file_idx, SymbolId)>` map by resolving
    // SymbolIds against `program.module_exports` (which holds globally-
    // remapped post-merge IDs) so checker consumers (`type_only.rs`,
    // `state/type_resolution/module.rs`, `state/type_resolution/import_type.rs`)
    // see no behavior change. The legacy inner
    // `for (export_name, sym_id) in exports.iter()` push loop in
    // `ProgramContext::build_global_indices` is skipped when this is `Some`.
    //
    // SymbolId-coupling note: unlike PR #1145 (file-locals index, closed for
    // a regression), this projection does NOT extract pre-merge local
    // SymbolIds from the skeleton — it looks them up in the post-merge
    // `program.module_exports`. The skeleton only records name strings.
    let skeleton_module_exports_index: Option<tsz::checker::context::GlobalModuleExportsIndex> =
        program
            .skeleton_index
            .as_ref()
            .map(|skel| Arc::new(skel.build_module_exports_index(&program.module_exports)));

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
    // cheap atomic clone for ProgramContext install.
    let program_module_exports = Arc::clone(&program.module_exports);
    // Same rationale for `program.cross_file_node_symbols`: the merged
    // program already owns the outer map behind `Arc`, so installing it into
    // the shared ProgramContext is an O(1) clone instead of deep-cloning the
    // `FxHashMap<usize, Arc<...>>` before re-sharing it.
    let program_cross_file_node_symbols = Arc::clone(&program.cross_file_node_symbols);
    // Same rationale for `program.alias_partners`: a single shared
    // FxHashMap<SymbolId, SymbolId> beats N per-binder deep-clones.
    let program_alias_partners = Arc::clone(&program.alias_partners);

    let mut program_context = tsz::checker::context::ProgramContext {
        lib_contexts: Arc::clone(&checker_libs.contexts),
        all_arenas: Arc::clone(&all_arenas),
        all_binders: Arc::clone(&all_binders),
        skeleton_declared_modules,
        skeleton_expando_index,
        skeleton_module_augmentations_index,
        skeleton_augmentation_targets_index,
        skeleton_module_binder_index,
        skeleton_module_exports_index,
        symbol_file_targets: Arc::clone(&symbol_file_targets),
        resolved_module_paths: Arc::clone(&resolved_module_paths),
        resolved_module_request_paths: Arc::clone(&resolved_module_request_paths),
        resolved_module_ts_extension_flags: Arc::clone(&resolved_module_ts_extension_flags),
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
        cross_file_type_params_cache: std::env::var_os("TSZ_CROSS_FILE_TYPE_PARAMS_CACHE")
            .map(|_| Arc::new(dashmap::DashMap::new())),
        ..Default::default()
    };
    // Use fingerprint-aware rebuild when a skeleton index is available.
    // On the first build this always rebuilds; on subsequent incremental builds
    // with the same skeleton fingerprint the O(N) binder scan is skipped.
    if let Some(ref skel) = program.skeleton_index {
        program_context.build_global_indices_if_changed(skel.fingerprint);
    } else {
        program_context.build_global_indices();
    }
    // Build the shared SymbolId→file-index map once; shared via Arc across all checkers.
    program_context.build_global_symbol_file_index();

    // Create a shared DefinitionStore for all parallel checkers.
    // CRITICAL: All parallel checkers MUST share the same DefinitionStore so that
    // DefId allocation is globally unique. Without this, independent DefId sequences
    // in separate checkers cause TypeId collisions via Lazy(DefId) interning.
    {
        let shared_store = Arc::new(
            tsz_solver::def::DefinitionStore::from_semantic_defs_with_overlays(
                &program.semantic_defs,
                program.files.iter().map(|file| file.semantic_defs.as_ref()),
                |s| program.type_interner.intern_string(s),
            ),
        );
        shared_store.init_file_locks(program.files.len());
        program_context.shared_definition_store = Some(shared_store);
    }

    let shared_lib_cache: Arc<dashmap::DashMap<String, Option<tsz_solver::TypeId>>> =
        Arc::new(dashmap::DashMap::new());

    // Prime Array<T> base type with global augmentations before fresh-checker
    // file checks. Tiny no-emit batches use the sequential reused-checker
    // path; that real checker primes itself before checking the first file, so
    // a separate prime checker would duplicate the same setup.
    if needs_separate_boxed_prime_checker(
        options.no_emit,
        options.emit_declarations,
        file_session_reuse_requested(program.files.len()),
        program.files.len(),
        !checker_libs.contexts.is_empty(),
    ) {
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
        checker.ctx.shared_lib_type_cache = Some(Arc::clone(&shared_lib_cache));
        program_context.apply_to(&mut checker.ctx);
        checker.prime_boxed_types();
    }

    // PERF: the post-merge default-lib recheck (collect baseline + per-lib
    // check + subtract) only produces diagnostics when user code merges into
    // a global lib interface OR extends a lib interface that contributes
    // user-relevant members. When both `affected_lib_interfaces` and
    // `affected_lib_extension_interfaces` are empty, the per-lib check
    // diagnostic set equals the baseline set: their subtraction is empty
    // and no user-induced diagnostics can surface. Skip the baseline pass
    // entirely in that case — and skip the per-lib check loops below by
    // gating them on the same condition. For typical single-file or
    // module-only TS files (no `declare global`, no interface that augments
    // a lib type) this removes a fixed ~30-lib-file recheck tax that was
    // dominating the per-invocation floor (~380–430ms on tiny files).
    let needs_lib_recheck = can_recheck_checker_libs
        && (!affected_lib_interfaces.is_empty() || !affected_lib_extension_interfaces.is_empty());
    let baseline_lib_diagnostics = if needs_lib_recheck {
        collect_checker_lib_baseline_fingerprints(
            program,
            options,
            checker_libs,
            &affected_lib_interfaces,
            &affected_lib_extension_interfaces,
            &program_context,
        )
    } else {
        FxHashSet::default()
    };
    let baseline_lib_datetimeformatpart_diagnostics = if can_recheck_checker_libs
        && !options.lib_is_default
        && !has_esnext_umbrella_lib(checker_libs)
        && should_preserve_datetimeformatpart_spelling_baseline(checker_libs)
        && !baseline_lib_datetimeformatpart_interfaces.is_empty()
    {
        let mut diagnostics = collect_checker_lib_baseline_diagnostics_for_codes(
            program,
            options,
            checker_libs,
            &baseline_lib_datetimeformatpart_interfaces,
            &FxHashSet::default(),
            &program_context,
            &[2552],
        );
        diagnostics.retain(is_datetimeformatpart_spelling_baseline_diagnostic);
        diagnostics
    } else {
        Vec::new()
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
    // CLI semantic diagnostics are scheduled here rather than through the core
    // `check_files_parallel` helper. That keeps command-mode policy, diagnostic
    // filtering, cache invalidation, and checker reuse in the driver.
    //
    // Two driver paths:
    // 1. Non-cached (first build, CI): Check ALL files in parallel using rayon.
    //    No dependency cascade needed since we're checking everything.
    // 2. Cached (watch mode): Sequential work queue with export-hash-based
    //    dependency cascade for incremental invalidation.

    let checker_lib_file_env = CheckerLibFileCheckEnv {
        program,
        options,
        checker_libs,
        affected_interfaces: &affected_lib_interfaces,
        extension_interfaces: &affected_lib_extension_interfaces,
        merged_augmentations: &merged_augmentations,
        program_context: &program_context,
        program_has_real_syntax_errors,
        program_has_unsupported_js_root,
    };

    let (query_cache_stats, aggregated_ds_stats) = if cache.is_none() {
        // --- PARALLEL PATH: No cache, check all files concurrently ---
        let _parallel_span =
            tracing::info_span!("parallel_check_files", files = work_queue.len()).entered();

        let no_check = options.no_check;
        let check_js = options.check_js;
        let explicit_check_js_false = options.explicit_check_js_false;
        let skip_lib_check = options.skip_lib_check;
        let compiler_options = options.checker.clone();
        let mut work_items: Vec<usize> = Vec::with_capacity(work_queue.len());
        let mut skipped_file_diagnostics: Vec<Vec<Diagnostic>> = Vec::new();
        for file_idx in work_queue {
            let file = &program.files[file_idx];
            if should_skip_type_checking_for_file(&file.file_name, options, false) {
                let mut file_diags = collect_no_check_file_diagnostics(
                    file,
                    options,
                    program_has_real_syntax_errors,
                );
                file_diags.extend(per_file_ts7016_diagnostics[file_idx].iter().cloned());
                skipped_file_diagnostics.push(file_diags);
            } else {
                work_items.push(file_idx);
            }
        }
        diagnostics.extend(skipped_file_diagnostics.into_iter().flatten());

        let build_checker_binder = |file_idx: usize| {
            let file = &program.files[file_idx];
            let mut binder = create_binder_from_bound_file_with_augmentations(
                file,
                program,
                file_idx,
                &merged_augmentations,
            );

            // Bridge raw module specifiers to resolved export tables using the
            // pre-computed resolved_module_paths map (no FS calls needed).
            for (specifier, _, _, _) in &cached_module_specifiers[file_idx] {
                if let Some(&target_idx) = resolved_module_paths.get(&(file_idx, specifier.clone()))
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
        };

        // `TypeCache` is consumed by the emit pipeline (JS or declaration
        // files). For a pure `--noEmit` run that does not also request
        // declarations the cache is never read, but extracting one per file
        // pins several hash maps per file in memory throughout the whole
        // check — on a 6000-file repo that grew to ~10 GB RSS and got the
        // process killed by macOS jetsam before any diagnostics emitted.
        // Skip extraction in that case and let per-file state drop as soon
        // as checking finishes.
        let extract_type_cache = !options.no_emit || options.emit_declarations;
        // Create shared cross-file query cache for multi-file projects.
        // Eliminates redundant type evaluations and relation checks across files.
        let shared_query_cache = if work_items.len() > 1 {
            Some(tsz_solver::construction::SharedQueryCache::new())
        } else {
            None
        };

        let check_file_with_fresh_checker = |file_idx: usize| {
            let binder = build_checker_binder(file_idx);
            let context = CheckFileForParallelContext {
                file_idx,
                binder,
                program,
                compiler_options: &compiler_options,
                program_context: &program_context,
                resolved_modules_per_file: &resolved_modules_per_file,
                shared_lib_cache: Arc::clone(&shared_lib_cache),
                shared_query_cache: shared_query_cache.as_ref(),
                no_check,
                check_js,
                explicit_check_js_false,
                skip_lib_check,
                program_has_real_syntax_errors,
                program_has_unsupported_js_root,
                extract_type_cache,
            };
            check_file_for_parallel(context)
        };

        // Check all files in parallel — each file gets its own CheckerState and QueryCache.
        // TypeInterner (DashMap) is thread-safe; QueryCache uses RefCell/Cell per-thread.
        #[cfg(not(target_arch = "wasm32"))]
        let file_results: Vec<CheckFileResult> = {
            use rayon::iter::{IndexedParallelIterator, IntoParallelRefIterator, ParallelIterator};
            // Use sequential checking for small projects to avoid Rayon overhead
            // and non-deterministic false positives from concurrent type
            // interning. The `TypeInterner` uses `DashMap` for thread-safe
            // access, but concurrent type evaluation can still observe
            // dependency and lib/package declaration shapes in scheduler order.
            //
            // A slightly wider small-project lane also covers cross-file JSX
            // namespace/global declaration discovery and checked-JS
            // CommonJS/JSDoc constructor evidence. Importer files can otherwise
            // observe incomplete dependency shapes and emit flaky TS2339
            // diagnostics.
            let reuse_requested = file_session_reuse_requested(work_items.len());
            let parallel_reuse_requested = parallel_file_session_reuse_requested();
            let has_parallel_order_sensitive_global_lib =
                has_parallel_order_sensitive_global_lib(checker_libs);
            let use_sequential_checking = work_items.len() <= 32
                || has_large_wildcard_barrel(program, &work_items)
                // DOM-style global declarations are order-sensitive with
                // multiple concurrent checker contexts. Keep those projects
                // on the deterministic single-worker path until global
                // lookup state is fully parallel-stable.
                || has_parallel_order_sensitive_global_lib
                // Fresh per-file checkers can observe project-level lib/global
                // state in scheduler order when run concurrently. If the
                // session-reuse path is explicitly disabled, keep the fallback
                // deterministic by using fresh checkers sequentially.
                || !reuse_requested;
            // T2.1.B (`PERFORMANCE_PLAN.md` §6 PR table): the sequential
            // no-emit path *can* construct one `CheckerState` and re-target
            // it across files via `CheckerContext::switch_to_file` instead
            // of constructing one per file. As of PR #7521 + the experiment
            // doc at `docs/architecture/LSP_PERF_EXPERIMENTS_2026-05-16.md`,
            // this remains OPT-IN (`TSZ_FILE_SESSION_REUSE=1`) because the
            // reuse path regresses wall time 4-14x at 1k+ files and is not yet
            // byte-identical for all tiny conformance shapes. The fresh-checker
            // branch below (`check_file_with_fresh_checker`) remains the default.
            // This flag applies to the sequential branch here; the parallel
            // branch below has its own chunked worker-reuse path with the
            // same opt-in default.
            // `extract_type_cache=true` (set when `--emit` or
            // `--declaration` is on) consumes the `CheckerState` per
            // file via `extract_cache(self)`. The reuse path holds
            // ONE `CheckerState` across the whole loop, so it can't
            // call the consuming variant. Pinning `--noEmit` runs is
            // exactly the bench/profiling scenario T2.1.B is built
            // for, so this restriction matches the use case rather
            // than narrowing it.
            let use_file_session_reuse =
                use_sequential_checking && !extract_type_cache && reuse_requested;
            if use_file_session_reuse {
                let reuse_ctx = CheckFilesReuseCtx {
                    program,
                    compiler_options: &compiler_options,
                    program_context: &program_context,
                    resolved_modules_per_file: &resolved_modules_per_file,
                    shared_lib_cache: Arc::clone(&shared_lib_cache),
                    shared_query_cache: shared_query_cache.as_ref(),
                };
                let reuse_flags = CheckFileFlags {
                    no_check,
                    check_js,
                    explicit_check_js_false,
                    skip_lib_check,
                    program_has_real_syntax_errors,
                    program_has_unsupported_js_root,
                    extract_type_cache,
                };
                check_files_sequentially_with_reuse(
                    &work_items,
                    &reuse_ctx,
                    &reuse_flags,
                    build_checker_binder,
                )
            } else if !use_sequential_checking && !extract_type_cache && parallel_reuse_requested {
                // T2.1.C follow-up: parallel chunked worker reuse is
                // opt-in via `TSZ_FILE_SESSION_REUSE=1` (was default-on
                // before PR #7521; see comment above on
                // `file_session_reuse_requested`).
                tsz::parallel::ensure_rayon_global_pool();
                let reuse_ctx = CheckFilesReuseCtx {
                    program,
                    compiler_options: &compiler_options,
                    program_context: &program_context,
                    resolved_modules_per_file: &resolved_modules_per_file,
                    shared_lib_cache: Arc::clone(&shared_lib_cache),
                    shared_query_cache: shared_query_cache.as_ref(),
                };
                let reuse_flags = CheckFileFlags {
                    no_check,
                    check_js,
                    explicit_check_js_false,
                    skip_lib_check,
                    program_has_real_syntax_errors,
                    program_has_unsupported_js_root,
                    extract_type_cache,
                };
                check_files_in_parallel_chunks_with_reuse(
                    &work_items,
                    &reuse_ctx,
                    &reuse_flags,
                    FILE_SESSION_REUSE_PARALLEL_CHUNK_SIZE,
                    &build_checker_binder,
                )
            } else if use_sequential_checking {
                work_items
                    .iter()
                    .map(|&file_idx| check_file_with_fresh_checker(file_idx))
                    .collect()
            } else {
                tsz::parallel::ensure_rayon_global_pool();
                // PERF: force `with_min_len(1)` so rayon's work-stealing
                // scheduler doesn't pre-chunk the file list into large blocks.
                // Per-file check time varies wildly (a file with one type
                // alias is ~ms; a file that triggers a deep
                // `delegate_cross_arena_symbol_resolution` cascade through
                // ts-essentials/react.d.ts can take seconds). With default
                // chunking the worker that draws the heavy chunk gates the
                // entire batch. Sample profiles on subset3 (1429 files, only
                // one worker active for the bulk of the check phase)
                // confirmed this skew. Fine-grained stealing lets idle
                // workers grab one file at a time from the busy worker's
                // queue.
                work_items
                    .par_iter()
                    .with_min_len(1)
                    .map(|&file_idx| check_file_with_fresh_checker(file_idx))
                    .collect()
            }
        };

        #[cfg(target_arch = "wasm32")]
        let file_results: Vec<CheckFileResult> = work_items
            .iter()
            .map(|&file_idx| check_file_with_fresh_checker(file_idx))
            .collect();

        // Aggregate per-file query cache statistics. DefinitionStore stats
        // come from the shared store computed once after the loop (workers
        // all see the same shared store, so summing per-file was both
        // wasted work and N× inflated).
        let mut parallel_qc_stats = tsz_solver::construction::QueryCacheStatistics::default();
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
        // PERF: see `needs_lib_recheck` above — when no user-defined global
        // interface merges into a lib interface and no user interface extends a
        // lib interface that contributes user-relevant members, the per-lib
        // check produces only baseline diagnostics that get subtracted away.
        // Skip the loop entirely.
        if needs_lib_recheck {
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
        // PERF: `DefinitionStore::statistics()` walks every entry (and
        // `estimated_size_bytes()` walks again) — only worth paying for
        // when --diagnostics or --extendedDiagnostics is requested.
        let aggregated_ds_stats = if collect_compile_stats {
            program_context
                .shared_definition_store
                .as_ref()
                .map(|store| store.statistics())
                .or(Some(parallel_ds_stats))
        } else {
            None
        };
        (Some(parallel_qc_stats), aggregated_ds_stats)
    } else {
        // --- SEQUENTIAL PATH: Cached build with dependency cascade ---
        // Fallback used only when no shared_definition_store exists (e.g.,
        // tests). Production paths set the shared store unconditionally.
        let sequential_ds_stats = tsz_solver::StoreStatistics::default();

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
            program_context.apply_to(&mut checker.ctx);

            // Per-file state that varies across files:
            checker.ctx.set_current_file_idx(file_idx);
            checker.ctx.file_is_esm = program_context
                .file_is_esm_map
                .get(&file.file_name)
                .copied();

            // Use the per-file pre-bucketed map; see the parallel path for the
            // O(N²) → O(1) rationale. `Arc::clone` here is a single atomic
            // increment — no deep copy of the contents.
            let resolved_modules: Arc<rustc_hash::FxHashSet<String>> = resolved_modules_per_file
                .get(file_idx)
                .cloned()
                .unwrap_or_else(|| Arc::new(rustc_hash::FxHashSet::default()));
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
            let filtered_parse_diagnostics =
                filtered_parse_diagnostics(&file.parse_diagnostics, program_has_real_syntax_errors);
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
                    program_has_unsupported_js_root,
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

            // Apply @ts-expect-error / @ts-ignore directive suppression only
            // when type checking ran. Under `--noCheck`, parse and JS grammar
            // diagnostics still surface in tsc and directives do not hide them.
            if !options.no_check
                && let Some(source) = file.arena.get_source_file_at(file.source_file)
            {
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
            // PERF: Skip per-file `definition_store.statistics()`. When
            // `program_context.shared_definition_store` is set, every per-file
            // checker.ctx.definition_store is `Arc::clone` of the SAME shared
            // store — calling .statistics() per file iterates that shared
            // store N times and produces N× inflated counts. The parallel
            // path already documents this same issue at line ~1159 ("summing
            // per-file was both wasted work and N× inflated"). Compute once
            // after the loop instead.

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
        // PERF: same gating as the parallel path above — skip the per-lib
        // post-merge recheck loop when there are no user-induced lib
        // augmentations to validate.
        if needs_lib_recheck {
            for lib_idx in 0..checker_libs.files.len() {
                let (lib_diags, lib_counters, _lib_ds_stats) =
                    check_checker_lib_file(&checker_lib_file_env, lib_idx, &query_cache, None);
                let mut lib_diags = lib_diags;
                retain_program_induced_lib_diagnostics(&mut lib_diags, &baseline_lib_diagnostics);
                diagnostics.extend(lib_diags);
                request_cache_counters.merge(lib_counters);
            }
        }
        // Sequential path: single shared QueryCache — capture stats after all files.
        let query_cache_stats = Some(query_cache.statistics());
        // PERF: skip the shared DefinitionStore stats walk unless --diagnostics
        // / --extendedDiagnostics actually consumes them. Matches the parallel
        // path's gating above.
        let aggregated_ds_stats = if collect_compile_stats {
            program_context
                .shared_definition_store
                .as_ref()
                .map(|store| store.statistics())
                .or(Some(sequential_ds_stats))
        } else {
            None
        };
        (query_cache_stats, aggregated_ds_stats)
    };

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

    if !program_has_real_syntax_errors {
        let mut seen: FxHashSet<(String, u32, u32)> = diagnostics
            .iter()
            .map(|diag| (diag.file.clone(), diag.start, diag.code))
            .collect();
        diagnostics.extend(
            tsz::parallel::collect_reexported_module_augmentation_enum_conflict_diagnostics(
                program,
                resolved_module_paths.as_ref(),
            )
            .into_iter()
            .filter(|diag| seen.insert((diag.file.clone(), diag.start, diag.code))),
        );
    }

    diagnostics.extend(detect_missing_tslib_helper_diagnostics(
        program,
        options,
        base_dir,
        &file_is_esm_map,
    ));
    diagnostics.extend(baseline_lib_datetimeformatpart_diagnostics);

    // Compute module dependency graph statistics for --extendedDiagnostics.
    // PERF: Skip the SCC computation entirely when the CLI won't print stats.
    // tarjan_scc + adjacency dedup is O(V+E) and adj allocates a Vec<Vec<usize>>
    // of file_count.
    let module_dep_stats = if collect_compile_stats {
        Some(compute_module_dependency_stats(
            program.files.len(),
            resolved_module_paths.as_ref(),
        ))
    } else {
        None
    };

    CollectDiagnosticsResult {
        diagnostics,
        request_cache_counters,
        query_cache_stats,
        def_store_stats: aggregated_ds_stats,
        module_dep_stats,
    }
}

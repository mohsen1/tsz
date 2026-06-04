//! Diagnostics collection and per-file checking orchestration for the compilation driver.

include!("check_large_methods/collect_diagnostics_with_source_resolutions_38.rs");

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
mod no_check_diagnostics;
mod source_resolution_setup;
mod wildcard_barrel_analysis;

use check_file::{
    CheckFileFlags, CheckFileForParallelContext, CheckFileResult, CheckFilesReuseCtx,
    check_file_for_parallel, check_files_in_parallel_chunks_with_reuse,
    check_files_sequentially_with_reuse,
};
use checker_diagnostics::{
    keep_checker_diagnostic_when_program_has_real_syntax_errors, post_process_checker_diagnostics,
    program_has_real_syntax_errors, program_has_unsupported_js_root,
    should_skip_type_checking_for_file,
};
use checker_lib_diagnostics::{
    CheckerLibFileCheckEnv, affected_lib_extension_interface_names, affected_lib_interface_names,
    baseline_lib_datetimeformatpart_spelling_interface_names, check_checker_lib_file,
    collect_checker_lib_baseline_diagnostics_for_codes, collect_checker_lib_baseline_fingerprints,
    has_esnext_umbrella_lib, has_parallel_order_sensitive_global_lib,
    is_datetimeformatpart_spelling_baseline_diagnostic, retain_program_induced_lib_diagnostics,
    should_preserve_datetimeformatpart_spelling_baseline,
};
use no_check_diagnostics::{NoCheckDiagnosticsInput, collect_no_check_diagnostics_for_files};
use source_resolution_setup::{
    SourceResolutionSetup, SourceResolutionSetupInput, prepare_source_resolution_setup,
};
use wildcard_barrel_analysis::{
    LARGE_WILDCARD_BARREL_EXPORTS, WildcardBarrelAnalysisInput, has_large_wildcard_barrel,
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

__tsz_split_check_collect_diagnostics_with_source_resolutions_38!();

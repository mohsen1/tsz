use anyhow::{Context, Result, bail};
use std::collections::VecDeque;

use rustc_hash::{FxHashMap, FxHashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Instant;

use crate::args::CliArgs;
use crate::config::{
    RemovedOptionNotice, ResolvedCompilerOptions, TsConfig, load_tsconfig,
    load_tsconfig_with_diagnostics_deferred, resolve_compiler_options,
    resolve_lib_files_with_options, resolve_lib_files_with_options_transitive,
};
use tsz::binder::BinderOptions;
use tsz::binder::BinderState;
use tsz::binder::{SymbolId, SymbolTable};
use tsz::checker::TypeCache;
use tsz::checker::context::LibContext;
use tsz::checker::diagnostics::{
    Diagnostic, DiagnosticCategory, DiagnosticRelatedInformation, RelatedInformationKind,
    diagnostic_codes,
};
use tsz::checker::state::CheckerState;
use tsz::lib_loader::LibFile;
use tsz::module_resolver::{ImportKind, ImportingModuleKind, ModuleResolver};
use tsz::span::Span;
use tsz_binder::state::BinderStateScopeInputs;
use tsz_common::common::ScriptTarget;
use tsz_common::file_extensions::{
    JS_FAMILY_EXTENSIONS, JSON_EXTENSION, TS_FAMILY_EXTENSIONS, is_default_lib_file, is_json_file,
};
use tsz_common::options::module_detection::ModuleDetectionKind;
// Re-export functions that other modules (e.g. watch) access via `driver::`.
use super::emit::{
    EmitOutputsContext, OutputFile, emit_outputs, normalize_root_dirs, normalize_type_roots,
    write_outputs,
};
pub(crate) use super::emit::{normalize_base_url, normalize_output_dir, normalize_root_dir};
#[cfg(test)]
use super::resolution::collect_module_specifiers;
use super::resolution::{
    ModuleResolutionCache, ProgramFileIndex, apply_json_type_import_attribute_override,
    build_duplicate_package_redirects, canonicalize_or_owned,
    collect_declaration_file_augmentation_targets_for_untyped_check, collect_export_binding_nodes,
    collect_import_bindings, collect_module_specifiers_for_check, collect_star_export_specifiers,
    collect_type_packages_from_root, default_type_roots, env_flag,
    implied_resolution_mode_for_file_with_cache, is_declaration_file,
    module_specifier_has_type_json_import_attribute, normalize_path, normalize_resolved_path,
    resolve_module_specifier,
};
use crate::fs::{FileDiscoveryOptions, discover_ts_files, is_js_file, is_ts_file};
use crate::incremental::{BuildInfo, default_build_info_path};
#[cfg(test)]
use std::cell::RefCell;
use tsz::parallel::{self, BindResult, BoundFile, MergedProgram};
use tsz::parser::node::NodeArena;
use tsz::parser::syntax_kind_ext;
use tsz::parser::{NodeIndex, ParseDiagnostic};
use tsz::scanner::SyntaxKind;
use tsz::solver_cache::StoreStatistics;
use tsz::solver_cache::construction::{QueryCache, QueryCacheStatistics};

#[path = "diagnostic_source.rs"]
mod diagnostic_source;
use diagnostic_source::diagnostic_source_line;

#[path = "core_diagnostics.rs"]
mod core_diagnostics;
use core_diagnostics::*;

/// Reason why a file was included in compilation
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FileInclusionReason {
    /// File specified as a CLI argument (`tsz a.ts`)
    RootFile,
    /// File listed in tsconfig's `files` array. tsc spells this out
    /// distinctly from `include`-pattern matches.
    FilesListEntry,
    /// File matched by include pattern in tsconfig
    IncludePattern(String),
    /// File imported from another file
    ImportedFrom(PathBuf),
    /// File is a default library for the configured target (e.g.
    /// `lib.es2020.d.ts`). The target string matches tsc's display
    /// (`es2018`, `esnext`, ...).
    DefaultLibrary(String),
    /// File is a lib file with no specific target attribution (e.g.
    /// pulled in via `/// <reference lib="..." />` or `--lib`).
    LibFile,
}

impl std::fmt::Display for FileInclusionReason {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::RootFile => write!(f, "Root file specified"),
            Self::FilesListEntry => write!(f, "Part of 'files' list in tsconfig.json"),
            Self::IncludePattern(pattern) => {
                write!(f, "Matched by include pattern '{pattern}'")
            }
            Self::ImportedFrom(path) => {
                write!(f, "Imported from '{}'", path.display())
            }
            Self::DefaultLibrary(target) => write!(f, "Default library for target '{target}'"),
            Self::LibFile => write!(f, "Library file"),
        }
    }
}

/// Information about an included file
#[derive(Debug, Clone)]
pub struct FileInfo {
    /// Path to the file
    pub path: PathBuf,
    /// Why this file was included
    pub reasons: Vec<FileInclusionReason>,
}

/// Module-level dependency graph statistics for `--extendedDiagnostics`.
#[derive(Debug, Clone, Default)]
pub struct ModuleDependencyStats {
    /// Number of source files in the program (excluding lib files).
    pub file_count: usize,
    /// Total number of resolved import edges (file A imports file B).
    pub dependency_edges: usize,
    /// Number of strongly-connected components with more than one file (import cycles).
    pub import_cycles: usize,
    /// Size of the largest import cycle (0 if no cycles).
    pub largest_cycle_size: usize,
}

/// Phase timing breakdown for `--diagnostics` / `--extendedDiagnostics`.
///
/// Matches the T0.2 diagnostics-JSON schema documented in
/// `docs/plan/PERFORMANCE_PLAN.md` §3 — fine-grained sub-phases
/// (`config_discovery`, `source_discovery`, `module_resolution`,
/// `load_libs`) split out of the legacy `io_read`/`parse_bind` totals
/// so attribution-mode runs can answer "where in the pre-check work is
/// time going?". Buckets that are not yet attributed by the driver
/// stay at 0.0 and the leftover lands in the parent bucket
/// (`io_read_ms` or `parse_bind_ms`), so existing PR text that quotes
/// `io_read` totals is still meaningful.
#[derive(Debug, Clone, Default)]
pub struct PhaseTimings {
    /// Time spent finding and parsing the active `tsconfig.json`
    /// (and any extended configs) before source discovery starts.
    /// Currently 0.0 unless the driver attributes config-load work
    /// here; the leftover stays in `io_read_ms`.
    pub config_discovery_ms: f64,
    /// Time spent enumerating root files and walking the include
    /// pattern set into a stable list of source files. Currently 0.0
    /// unless attributed here; the leftover stays in `io_read_ms`.
    pub source_discovery_ms: f64,
    /// Time spent in `tsc`-style module resolution (specifier ->
    /// resolved file). Currently 0.0 unless attributed here; the
    /// leftover stays in `io_read_ms` or `parse_bind_ms`.
    pub module_resolution_ms: f64,
    /// Time spent reading source files from disk.
    pub io_read_ms: f64,
    /// Time spent loading and binding lib files.
    pub load_libs_ms: f64,
    /// Time spent parsing and binding user files.
    pub parse_bind_ms: f64,
    /// Time spent type-checking (collecting diagnostics).
    pub check_ms: f64,
    /// Time spent emitting output files.
    pub emit_ms: f64,
    /// Total wall-clock compilation time.
    pub total_ms: f64,
}

#[derive(Debug, Clone)]
pub struct CompilationResult {
    pub diagnostics: Vec<Diagnostic>,
    pub emitted_files: Vec<PathBuf>,
    pub files_read: Vec<PathBuf>,
    /// Files with their inclusion reasons (for --explainFiles)
    pub file_infos: Vec<FileInfo>,
    /// Resolved `noEmit` option (merged from tsconfig.json + CLI overrides).
    /// tsc 7.0.2 exits `DiagnosticsPresent_OutputsSkipped` (1) for `--noEmit`
    /// with errors (no outputs were generated); the exit-code decision now
    /// keys on `emitted_files` alone, but the resolved flag stays available
    /// for other consumers.
    pub no_emit: bool,
    pub request_cache_counters: tsz::checker::context::RequestCacheCounters,
    /// Number of interned types in the shared `TypeInterner` after checking.
    pub interned_types_count: usize,
    /// Estimated heap memory of the `TypeInterner` in bytes (populated for `--extendedDiagnostics`).
    pub interner_estimated_bytes: usize,
    /// Aggregate query-cache statistics (populated for `--extendedDiagnostics`).
    pub query_cache_stats: Option<QueryCacheStatistics>,
    /// Aggregate definition-store statistics (populated for `--extendedDiagnostics`).
    pub def_store_stats: Option<StoreStatistics>,
    /// Phase timing breakdown for `--diagnostics` / `--extendedDiagnostics`.
    pub phase_timings: PhaseTimings,
    /// Merged-program residency stats (populated for `--extendedDiagnostics`).
    pub residency_stats: Option<tsz::parallel::residency::MergedProgramResidencyStats>,
    /// Module dependency graph statistics (populated for `--extendedDiagnostics`).
    pub module_dep_stats: Option<ModuleDependencyStats>,
    /// Invalidation summaries for files changed in this compilation.
    ///
    /// Populated by `compile_with_cache_and_changes` (watch-mode incremental path).
    /// Each entry records whether a file's public API changed and how many
    /// dependents were invalidated. Empty for full (non-incremental) compilations.
    pub invalidation_summaries: Vec<tsz_lsp::export_signature::InvalidationSummary>,
}

const TYPES_VERSIONS_COMPILER_VERSION_ENV_KEY: &str = "TSZ_TYPES_VERSIONS_COMPILER_VERSION";

#[cfg(test)]
thread_local! {
    static TEST_TYPES_VERSIONS_COMPILER_VERSION_OVERRIDE: RefCell<Option<Option<String>>> =
        const { RefCell::new(None) };
}

#[cfg(test)]
struct TestTypesVersionsEnvGuard {
    previous: Option<Option<String>>,
}

#[cfg(test)]
impl Drop for TestTypesVersionsEnvGuard {
    fn drop(&mut self) {
        TEST_TYPES_VERSIONS_COMPILER_VERSION_OVERRIDE.with(|slot| {
            let mut slot = slot.borrow_mut();
            *slot = self.previous.clone();
        });
    }
}

#[cfg(test)]
pub(crate) fn with_types_versions_env<T>(value: Option<&str>, f: impl FnOnce() -> T) -> T {
    let value = value.map(str::to_string);
    let previous = TEST_TYPES_VERSIONS_COMPILER_VERSION_OVERRIDE.with(|slot| {
        let mut slot = slot.borrow_mut();
        let previous = slot.clone();
        *slot = Some(value);
        previous
    });
    let _guard = TestTypesVersionsEnvGuard { previous };
    f()
}

#[cfg(test)]
fn test_types_versions_compiler_version_override() -> Option<Option<String>> {
    TEST_TYPES_VERSIONS_COMPILER_VERSION_OVERRIDE.with(|slot| slot.borrow().clone())
}

fn types_versions_compiler_version_env() -> Option<String> {
    #[cfg(test)]
    if let Some(override_value) = test_types_versions_compiler_version_override() {
        return override_value;
    }
    std::env::var(TYPES_VERSIONS_COMPILER_VERSION_ENV_KEY).ok()
}

#[derive(Default)]
pub(crate) struct CompilationCache {
    type_caches: FxHashMap<PathBuf, TypeCache>,
    bind_cache: FxHashMap<PathBuf, BindCacheEntry>,
    /// Per-file dependency lists in source-import (discovery) order.
    ///
    /// Replayed during cached project rebuilds to seed BFS discovery; the order
    /// must match the original fresh build so global `SymbolId` assignment stays
    /// stable for unchanged source graphs. See `SourceReadResult::dependencies`.
    dependencies: FxHashMap<PathBuf, Vec<PathBuf>>,
    pub(crate) outfile_bundle_dependencies: FxHashMap<PathBuf, FxHashSet<PathBuf>>,
    reverse_dependencies: FxHashMap<PathBuf, FxHashSet<PathBuf>>,
    diagnostics: FxHashMap<PathBuf, Vec<Diagnostic>>,
    export_hashes: FxHashMap<PathBuf, u64>,
    import_symbol_ids: FxHashMap<PathBuf, FxHashMap<PathBuf, Vec<SymbolId>>>,
    star_export_dependencies: FxHashMap<PathBuf, FxHashSet<PathBuf>>,
    /// Cached `MergedProgram` from the last successful unchanged-graph build.
    ///
    /// When all bind results come from `bind_cache` (`dirty_paths` is empty) and the
    /// file count is the same as when the cache was last filled, the merge phase is
    /// `O(total_symbols)` work that produces identical output. Storing the
    /// result as `Arc<MergedProgram>` lets `build_program_with_cache` return it
    /// immediately via an O(1) `Arc::clone` instead of re-running the merge.
    ///
    /// Invariants:
    /// - `None` until the first successful build with a full `bind_cache`.
    /// - Cleared by `clear()` when the user explicitly invalidates all state.
    /// - Replaced whenever `dirty_paths` is non-empty or the file count changes
    ///   (files added / removed), ensuring the cache is always coherent with the
    ///   current `bind_cache` contents.
    cached_merged_program: Option<Arc<MergedProgram>>,
    /// File count that `cached_merged_program` was built from.
    ///
    /// Used to detect file addition/removal even when individual file hashes are
    /// unchanged (i.e., `dirty_paths` is empty but the project has changed size).
    cached_file_count: usize,
}

struct BindCacheEntry {
    hash: u64,
    bind_result: BindResult,
}

impl CompilationCache {
    #[cfg(test)]
    pub(crate) fn len(&self) -> usize {
        self.type_caches.len()
    }

    #[cfg(test)]
    pub(crate) fn bind_len(&self) -> usize {
        self.bind_cache.len()
    }

    #[cfg(test)]
    pub(crate) fn diagnostics_len(&self) -> usize {
        self.diagnostics.len()
    }

    #[cfg(test)]
    pub(crate) fn symbol_cache_len(&self, path: &Path) -> Option<usize> {
        self.type_caches
            .get(path)
            .map(|cache| cache.symbol_types.len())
    }

    #[cfg(test)]
    pub(crate) fn node_cache_len(&self, path: &Path) -> Option<usize> {
        self.type_caches
            .get(path)
            .map(|cache| cache.node_types.len())
    }

    #[cfg(test)]
    pub(crate) fn invalidate_paths_with_dependents<I>(&mut self, paths: I)
    where
        I: IntoIterator<Item = PathBuf>,
    {
        let changed: FxHashSet<PathBuf> = paths.into_iter().collect();
        let affected = self.collect_dependents(changed.iter().cloned());
        for path in affected {
            self.type_caches.remove(&path);
            self.bind_cache.remove(&path);
            self.outfile_bundle_dependencies.remove(&path);
            self.diagnostics.remove(&path);
            self.export_hashes.remove(&path);
            self.import_symbol_ids.remove(&path);
            self.star_export_dependencies.remove(&path);
        }
    }

    pub(crate) fn invalidate_paths_with_dependents_symbols<I>(&mut self, paths: I)
    where
        I: IntoIterator<Item = PathBuf>,
    {
        let changed: FxHashSet<PathBuf> = paths.into_iter().collect();
        let affected = self.collect_dependents(changed.iter().cloned());
        for path in affected {
            if changed.contains(&path) {
                self.type_caches.remove(&path);
                self.bind_cache.remove(&path);
                self.outfile_bundle_dependencies.remove(&path);
                self.diagnostics.remove(&path);
                self.export_hashes.remove(&path);
                self.import_symbol_ids.remove(&path);
                self.star_export_dependencies.remove(&path);
                continue;
            }

            self.diagnostics.remove(&path);
            self.export_hashes.remove(&path);

            let mut roots = Vec::new();
            if let Some(dep_map) = self.import_symbol_ids.get(&path) {
                for changed_path in &changed {
                    if let Some(symbols) = dep_map.get(changed_path) {
                        roots.extend(symbols.iter().copied());
                    }
                }
            }

            if roots.is_empty() {
                let has_star_export =
                    self.star_export_dependencies
                        .get(&path)
                        .is_some_and(|deps| {
                            changed
                                .iter()
                                .any(|changed_path| deps.contains(changed_path))
                        });
                if has_star_export {
                    if let Some(cache) = self.type_caches.get_mut(&path) {
                        cache.node_types.clear();
                    }
                } else {
                    self.type_caches.remove(&path);
                }
                continue;
            }

            if let Some(cache) = self.type_caches.get_mut(&path) {
                cache.invalidate_symbols(&roots);
            }
        }
    }

    pub(crate) fn invalidate_paths<I>(&mut self, paths: I)
    where
        I: IntoIterator<Item = PathBuf>,
    {
        for path in paths {
            self.type_caches.remove(&path);
            self.bind_cache.remove(&path);
            self.outfile_bundle_dependencies.remove(&path);
            self.diagnostics.remove(&path);
            self.export_hashes.remove(&path);
            self.import_symbol_ids.remove(&path);
            self.star_export_dependencies.remove(&path);
        }
    }

    pub(crate) fn clear(&mut self) {
        self.type_caches.clear();
        self.bind_cache.clear();
        self.dependencies.clear();
        self.outfile_bundle_dependencies.clear();
        self.reverse_dependencies.clear();
        self.diagnostics.clear();
        self.export_hashes.clear();
        self.import_symbol_ids.clear();
        self.star_export_dependencies.clear();
        self.cached_merged_program = None;
        self.cached_file_count = 0;
    }

    pub(crate) fn update_dependencies(
        &mut self,
        dependencies: FxHashMap<PathBuf, Vec<PathBuf>>,
        outfile_bundle_dependencies: FxHashMap<PathBuf, FxHashSet<PathBuf>>,
    ) {
        let mut reverse = FxHashMap::default();
        for (source, deps) in &dependencies {
            for dep in deps {
                reverse
                    .entry(dep.clone())
                    .or_insert_with(FxHashSet::default)
                    .insert(source.clone());
            }
        }
        self.dependencies = dependencies;
        self.outfile_bundle_dependencies = outfile_bundle_dependencies;
        self.reverse_dependencies = reverse;
    }

    fn collect_dependents<I>(&self, paths: I) -> FxHashSet<PathBuf>
    where
        I: IntoIterator<Item = PathBuf>,
    {
        let mut pending = VecDeque::new();
        let mut affected = FxHashSet::default();

        for path in paths {
            if affected.insert(path.clone()) {
                pending.push_back(path);
            }
        }

        while let Some(path) = pending.pop_front() {
            let Some(dependents) = self.reverse_dependencies.get(&path) else {
                continue;
            };
            for dependent in dependents {
                if affected.insert(dependent.clone()) {
                    pending.push_back(dependent.clone());
                }
            }
        }

        affected
    }
}

/// Convert `CompilationCache` to `BuildInfo` for persistence.
///
/// `latest_changed_dts_file` is the most recently changed declaration output
/// for this build, carried forward from the previously loaded `BuildInfo`
/// when the build wrote no declaration file (tsc preserves
/// `latestChangedDtsFile` across no-emit incremental saves).
fn compilation_cache_to_build_info(
    cache: &CompilationCache,
    root_files: &[PathBuf],
    base_dir: &Path,
    options: &ResolvedCompilerOptions,
    latest_changed_dts_file: Option<String>,
) -> BuildInfo {
    use crate::incremental::{
        BuildInfoOptions, CachedDiagnostic, CachedRelatedInformation, EmitSignature,
        FileInfo as IncrementalFileInfo, compute_file_version,
    };
    use std::collections::BTreeMap;

    let mut file_infos = BTreeMap::new();
    let mut dependencies = BTreeMap::new();
    let mut emit_signatures = BTreeMap::new();

    // Convert each file's cache entry to BuildInfo format
    for (path, hash) in &cache.export_hashes {
        let relative_path: String = path
            .strip_prefix(base_dir)
            .unwrap_or(path)
            .to_string_lossy()
            .replace('\\', "/");

        // `version` is compared against the source file content on the next
        // build, while `signature` tracks the exported API shape.
        let version = compute_file_version(path).unwrap_or_else(|_| format!("{hash:016x}"));
        let signature = Some(format!("{hash:016x}"));
        file_infos.insert(
            relative_path.clone(),
            IncrementalFileInfo {
                version,
                signature,
                affected_files_pending_emit: false,
                implied_format: None,
            },
        );

        // Convert dependencies
        if let Some(deps) = cache.dependencies.get(path) {
            let dep_strs: Vec<String> = deps
                .iter()
                .map(|d| {
                    d.strip_prefix(base_dir)
                        .unwrap_or(d)
                        .to_string_lossy()
                        .replace('\\', "/")
                })
                .collect();
            dependencies.insert(relative_path.clone(), dep_strs);
        }

        // Add emit signature (empty for now, populated during emit)
        emit_signatures.insert(
            relative_path,
            EmitSignature {
                js: None,
                dts: None,
                map: None,
            },
        );
    }

    // Convert diagnostics to cached format
    let mut semantic_diagnostics_per_file = BTreeMap::new();
    for (path, diagnostics) in &cache.diagnostics {
        let relative_path: String = path
            .strip_prefix(base_dir)
            .unwrap_or(path)
            .to_string_lossy()
            .replace('\\', "/");

        let cached_diagnostics: Vec<CachedDiagnostic> = diagnostics
            .iter()
            .map(|d| {
                let file_path = Path::new(&d.file);
                CachedDiagnostic {
                    file: file_path
                        .strip_prefix(base_dir)
                        .unwrap_or(file_path)
                        .to_string_lossy()
                        .replace('\\', "/"),
                    start: d.start,
                    length: d.length,
                    message_text: d.message_text.clone(),
                    category: d.category as u8,
                    code: d.code,
                    related_information: d
                        .related_information
                        .iter()
                        .map(|r| {
                            let rel_file_path = Path::new(&r.file);
                            CachedRelatedInformation {
                                file: rel_file_path
                                    .strip_prefix(base_dir)
                                    .unwrap_or(rel_file_path)
                                    .to_string_lossy()
                                    .replace('\\', "/"),
                                start: r.start,
                                length: r.length,
                                message_text: r.message_text.clone(),
                                category: r.category as u8,
                                code: r.code,
                                depth: r.depth,
                                location_pointer: r.is_location_pointer(),
                            }
                        })
                        .collect(),
                }
            })
            .collect();

        if !cached_diagnostics.is_empty() {
            semantic_diagnostics_per_file.insert(relative_path, cached_diagnostics);
        }
    }

    // Convert root files to relative paths
    let root_files_str: Vec<String> = root_files
        .iter()
        .map(|p| {
            p.strip_prefix(base_dir)
                .unwrap_or(p)
                .to_string_lossy()
                .replace('\\', "/")
        })
        .collect();

    // Build compiler options
    let build_options = BuildInfoOptions {
        target: Some(format!("{:?}", options.checker.target)),
        module: Some(format!("{:?}", options.printer.module)),
        declaration: Some(options.emit_declarations),
        strict: Some(options.checker.strict),
    };

    BuildInfo {
        version: crate::incremental::BUILD_INFO_VERSION.to_string(),
        compiler_version: env!("CARGO_PKG_VERSION").to_string(),
        root_files: root_files_str,
        file_infos,
        dependencies,
        semantic_diagnostics_per_file,
        emit_signatures,
        latest_changed_dts_file,
        options: build_options,
        build_time: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0),
    }
}

/// Load `BuildInfo` and create an initial `CompilationCache` from it
fn build_info_to_compilation_cache(build_info: &BuildInfo, base_dir: &Path) -> CompilationCache {
    let mut cache = CompilationCache::default();

    // Convert string paths back to PathBuf and populate export_hashes
    for (path_str, file_info) in &build_info.file_infos {
        let full_path = base_dir.join(path_str);

        // Parse version hash back to u64
        if let Ok(hash) = u64::from_str_radix(&file_info.version, 16) {
            cache.export_hashes.insert(full_path.clone(), hash);
        }

        // Convert dependencies. `build_info` stores them as an ordered list
        // (source-import order); preserve that order on restore so a cached
        // rebuild replays discovery in the same order as the original build and
        // assigns identical global `SymbolId`s.
        if let Some(deps) = build_info.get_dependencies(path_str) {
            let mut dep_paths: Vec<PathBuf> = Vec::with_capacity(deps.len());
            for dep in deps {
                let dep_path = base_dir.join(dep);
                cache
                    .reverse_dependencies
                    .entry(dep_path.clone())
                    .or_default()
                    .insert(full_path.clone());
                if !dep_paths.contains(&dep_path) {
                    dep_paths.push(dep_path);
                }
            }
            cache.dependencies.insert(full_path, dep_paths);
        }
    }

    // Load diagnostics from BuildInfo
    for (path_str, cached_diagnostics) in &build_info.semantic_diagnostics_per_file {
        let full_path = base_dir.join(path_str);

        let diagnostics: Vec<Diagnostic> = cached_diagnostics
            .iter()
            .map(|cd| Diagnostic {
                file: full_path.to_string_lossy().into_owned(),
                start: cd.start,
                length: cd.length,
                message_text: cd.message_text.clone(),
                category: match cd.category {
                    0 => DiagnosticCategory::Warning,
                    1 => DiagnosticCategory::Error,
                    2 => DiagnosticCategory::Suggestion,
                    _ => DiagnosticCategory::Message,
                },
                code: cd.code,
                related_information: cd
                    .related_information
                    .iter()
                    .map(|r| DiagnosticRelatedInformation {
                        file: base_dir.join(&r.file).to_string_lossy().into_owned(),
                        start: r.start,
                        length: r.length,
                        message_text: r.message_text.clone(),
                        category: DiagnosticCategory::from_cache_index(r.category),
                        code: r.code,
                        depth: r.depth,
                        kind: if r.location_pointer {
                            RelatedInformationKind::LocationPointer
                        } else {
                            RelatedInformationKind::ChainLink
                        },
                    })
                    .collect(),
            })
            .collect();

        if !diagnostics.is_empty() {
            cache.diagnostics.insert(full_path, diagnostics);
        }
    }

    cache
}

/// Get the .tsbuildinfo file path based on compiler options
fn get_build_info_path(
    tsconfig_path: Option<&Path>,
    options: &ResolvedCompilerOptions,
    base_dir: &Path,
) -> Option<PathBuf> {
    if !options.incremental && options.ts_build_info_file.is_none() {
        return None;
    }

    if let Some(ref explicit_path) = options.ts_build_info_file {
        return Some(base_dir.join(explicit_path));
    }

    // Use tsconfig path to determine default buildinfo location
    let config_path = tsconfig_path?;
    let out_dir = options.out_dir.as_ref().map(|od| base_dir.join(od));
    let root_dir = options.root_dir.as_ref().map(|rd| base_dir.join(rd));
    Some(default_build_info_path(
        config_path,
        out_dir.as_deref(),
        root_dir.as_deref(),
    ))
}

fn format_file_write_error_for_diagnostic(path: &Path, err: &anyhow::Error) -> String {
    if let Some(io_err) = err
        .chain()
        .find_map(|cause| cause.downcast_ref::<std::io::Error>())
    {
        let quoted_path = path.display().to_string();
        return match io_err.raw_os_error() {
            Some(30) => format!("EROFS: read-only file system, open '{quoted_path}'"),
            Some(13) => format!("EACCES: permission denied, open '{quoted_path}'"),
            _ => io_err.to_string(),
        };
    }

    err.root_cause().to_string()
}

pub fn compile(args: &CliArgs, cwd: &Path) -> Result<CompilationResult> {
    compile_inner(args, cwd, None, None, None, None)
}

/// Compile a specific project by config path (used for --build mode with project references)
pub fn compile_project(
    args: &CliArgs,
    cwd: &Path,
    config_path: &Path,
) -> Result<CompilationResult> {
    compile_inner(args, cwd, None, None, None, Some(config_path))
}

pub(crate) fn compile_with_cache(
    args: &CliArgs,
    cwd: &Path,
    cache: &mut CompilationCache,
) -> Result<CompilationResult> {
    compile_inner(args, cwd, Some(cache), None, None, None)
}

pub(crate) fn compile_with_cache_and_changes(
    args: &CliArgs,
    cwd: &Path,
    cache: &mut CompilationCache,
    changed_paths: &[PathBuf],
) -> Result<CompilationResult> {
    use tsz_lsp::export_signature::InvalidationSummary;

    let canonical_paths: Vec<PathBuf> = changed_paths
        .iter()
        .map(|path| canonicalize_or_owned(path))
        .collect();
    let mut old_hashes = FxHashMap::default();
    for path in &canonical_paths {
        if let Some(&hash) = cache.export_hashes.get(path) {
            old_hashes.insert(path.clone(), hash);
        }
    }

    cache.invalidate_paths(canonical_paths.iter().cloned());
    let mut result = compile_inner(args, cwd, Some(cache), Some(&canonical_paths), None, None)?;

    // Build per-file invalidation summaries and decide whether dependents need recompilation.
    let mut any_exports_changed = false;
    let mut summaries = Vec::with_capacity(canonical_paths.len());
    for path in &canonical_paths {
        let old_hash = old_hashes.get(path).copied();
        let new_hash = cache.export_hashes.get(path).copied();
        let file_name = path.to_string_lossy().into_owned();

        match (old_hash, new_hash) {
            (Some(old), Some(new)) if old == new => {
                summaries.push(InvalidationSummary::unchanged(file_name, new));
            }
            (old, Some(new)) => {
                any_exports_changed = true;
                // Dependent count will be filled in after we compute the set below.
                summaries.push(InvalidationSummary::changed(file_name, old, new, 0));
            }
            (_, None) => {
                // File was not recompiled (e.g. parse error); treat as new.
                summaries.push(InvalidationSummary::new_file(file_name, 0));
            }
        }
    }

    if !any_exports_changed {
        result.invalidation_summaries = summaries;
        return Ok(result);
    }

    // If --assumeChangesOnlyAffectDirectDependencies is set, only recompile direct dependents
    let dependents = if args.assume_changes_only_affect_direct_dependencies {
        // Only get direct dependents (one level deep)
        let mut direct_dependents = FxHashSet::default();
        for path in &canonical_paths {
            if let Some(deps) = cache.reverse_dependencies.get(path) {
                direct_dependents.extend(deps.iter().cloned());
            }
        }
        direct_dependents
    } else {
        // Get all transitive dependents (default behavior)
        cache.collect_dependents(canonical_paths.iter().cloned())
    };

    // Fill in the dependent count for changed files.
    let dependent_count = dependents.len().saturating_sub(canonical_paths.len());
    for summary in &mut summaries {
        if summary.api_changed {
            summary.dependents_invalidated = dependent_count;
        }
    }

    cache.invalidate_paths_with_dependents_symbols(canonical_paths);
    let mut result = compile_inner(
        args,
        cwd,
        Some(cache),
        Some(changed_paths),
        Some(&dependents),
        None,
    )?;
    result.invalidation_summaries = summaries;
    Ok(result)
}

/// Build file info with inclusion reasons
fn build_file_infos(
    sources: &[SourceEntry],
    root_file_paths: &[PathBuf],
    args: &CliArgs,
    config: Option<&crate::config::TsConfig>,
    base_dir: &Path,
    target: ScriptTarget,
) -> Vec<FileInfo> {
    let root_set: FxHashSet<_> = root_file_paths.iter().collect();
    let cli_files: FxHashSet<_> = args.files.iter().collect();

    // Resolve `tsconfig.files` entries to absolute paths so we can attribute
    // each compiled source back to a specific entry. tsc renders these as
    // `Part of 'files' list in tsconfig.json`, distinct from `include`-pattern
    // matches (#3901).
    let tsconfig_files_set: FxHashSet<PathBuf> = config
        .and_then(|c| c.files.as_ref())
        .map(|files| {
            files
                .iter()
                .map(|f| {
                    let p = PathBuf::from(f);
                    if p.is_absolute() { p } else { base_dir.join(p) }
                })
                .collect()
        })
        .unwrap_or_default();

    // Get include patterns if available
    let include_patterns = config
        .and_then(|c| c.include.as_ref())
        .map_or_else(|| "**/*".to_string(), |patterns| patterns.join(", "));

    let target_display = script_target_display_for_explain_files(target).to_string();

    sources
        .iter()
        .map(|source| {
            let mut reasons = Vec::new();

            // Check if it's a CLI-specified file
            if cli_files.iter().any(|f| source.path.ends_with(f)) {
                reasons.push(FileInclusionReason::RootFile);
            }
            // tsc surfaces lib files with the configured target, not just
            // `Library file`. Default-target libs (`lib.es2018.full.d.ts`)
            // get the precise reason; explicit `--lib`/reference-pulled libs
            // fall through to the generic LibFile.
            else if is_default_lib_file(&source.path) {
                if is_default_lib_for_target(&source.path, target) {
                    reasons.push(FileInclusionReason::DefaultLibrary(target_display.clone()));
                } else {
                    reasons.push(FileInclusionReason::LibFile);
                }
            }
            // tsconfig `files` list — distinct from `include` matches.
            else if tsconfig_files_set.contains(&source.path) {
                reasons.push(FileInclusionReason::FilesListEntry);
            }
            // Check if it's a root file from discovery
            else if root_set.contains(&source.path) {
                reasons.push(FileInclusionReason::IncludePattern(
                    include_patterns.clone(),
                ));
            }
            // Otherwise it was likely imported (we don't track precise imports yet)
            else {
                reasons.push(FileInclusionReason::ImportedFrom(PathBuf::from("<import>")));
            }

            FileInfo {
                path: source.path.clone(),
                reasons,
            }
        })
        .collect()
}

/// Format a `ScriptTarget` the way tsc does in `--explainFiles` reasons:
/// lowercase ECMAScript revision names (`es2018`, `esnext`).
const fn script_target_display_for_explain_files(target: ScriptTarget) -> &'static str {
    match target {
        ScriptTarget::ES3 => "es3",
        ScriptTarget::ES5 => "es5",
        ScriptTarget::ES2015 => "es2015",
        ScriptTarget::ES2016 => "es2016",
        ScriptTarget::ES2017 => "es2017",
        ScriptTarget::ES2018 => "es2018",
        ScriptTarget::ES2019 => "es2019",
        ScriptTarget::ES2020 => "es2020",
        ScriptTarget::ES2021 => "es2021",
        ScriptTarget::ES2022 => "es2022",
        ScriptTarget::ES2023 => "es2023",
        ScriptTarget::ES2024 => "es2024",
        ScriptTarget::ES2025 => "es2025",
        ScriptTarget::ESNext => "esnext",
    }
}

/// Identify whether a lib file is the *default* lib for the configured
/// target (e.g. `lib.es2018.full.d.ts` when `target` is `es2018`). tsc
/// distinguishes the target-driven default libs from libs pulled in via
/// `--lib` or triple-slash references.
fn is_default_lib_for_target(path: &Path, target: ScriptTarget) -> bool {
    let Some(file_name) = path.file_name().and_then(|n| n.to_str()) else {
        return false;
    };
    let target_name = script_target_display_for_explain_files(target);
    matches!(
        file_name,
        f if f == format!("lib.{target_name}.full.d.ts")
            || f == format!("lib.{target_name}.d.ts")
    )
}

fn resolve_effective_lib_paths(
    resolved: &ResolvedCompilerOptions,
    sources: &[SourceEntry],
    base_dir: &Path,
    disable_default_libs: bool,
) -> Result<Vec<PathBuf>> {
    let include_config_libs =
        !(resolved.checker.no_lib || (resolved.lib_is_default && disable_default_libs));
    let can_have_lib_replacements =
        resolved.lib_replacement && typescript_lib_replacement_root_exists(base_dir);
    let mut lib_paths = Vec::new();
    let mut seen = FxHashSet::default();
    let mut lib_names = Vec::new();

    if include_config_libs {
        if can_have_lib_replacements {
            lib_names.extend(lib_names_from_paths(&resolved.lib_files));
        } else {
            append_unique_lib_paths(
                &mut lib_paths,
                &mut seen,
                resolved.lib_files.iter().cloned(),
            );
        }
    }

    // When --noLib is set, ignore /// <reference lib="..." /> directives.
    // tsc skips lib reference resolution entirely when noLib is enabled.
    if !resolved.checker.no_lib {
        let source_reference_libs = collect_source_reference_libs(sources);
        if !source_reference_libs.is_empty() {
            // Source-file `/// <reference lib="..." />` directives may name libs
            // that no longer exist in this TS version (e.g., rxjs references
            // `esnext.asynciterable`, since folded into `es2018.asynciterable`).
            // The transitive resolver silently skips unknown names at this
            // layer; user-facing TS2726 for invalid initial names is emitted
            // separately by `collect_source_reference_lib_diagnostics`.
            let expanded_source_paths =
                resolve_lib_files_with_options_transitive(&source_reference_libs, true)?;
            if can_have_lib_replacements {
                append_unique_lib_names(
                    &mut lib_names,
                    lib_names_from_paths(&expanded_source_paths),
                );
            } else {
                append_unique_lib_paths(&mut lib_paths, &mut seen, expanded_source_paths);
            }
        }
    }

    for lib_name in lib_names {
        let Some(path) = resolve_compiler_lib_path(&lib_name, resolved, base_dir)? else {
            continue;
        };
        append_unique_lib_paths(&mut lib_paths, &mut seen, std::iter::once(path));
    }
    Ok(lib_paths)
}

fn append_unique_lib_paths(
    lib_paths: &mut Vec<PathBuf>,
    seen: &mut FxHashSet<PathBuf>,
    paths: impl IntoIterator<Item = PathBuf>,
) {
    for path in paths {
        let canonical = canonicalize_or_owned(&path);
        if seen.insert(canonical.clone()) {
            lib_paths.push(canonical);
        }
    }
}

fn typescript_lib_replacement_root_exists(base_dir: &Path) -> bool {
    base_dir.join("node_modules").join("@typescript").is_dir()
}

fn collect_source_reference_libs(sources: &[SourceEntry]) -> Vec<String> {
    let mut lib_names = Vec::new();
    for source in sources {
        let refs = if let Some(text) = source.text.as_deref() {
            if source_may_contain_reference_lib_directives(text) {
                tsz::config::extract_lib_references(text)
            } else {
                Vec::new()
            }
        } else {
            std::fs::read_to_string(&source.path)
                .map(|text| {
                    if source_may_contain_reference_lib_directives(&text) {
                        tsz::config::extract_lib_references(&text)
                    } else {
                        Vec::new()
                    }
                })
                .unwrap_or_default()
        };
        append_unique_lib_names(&mut lib_names, refs);
    }
    lib_names
}

fn source_may_contain_reference_lib_directives(text: &str) -> bool {
    text.contains("///") && text.contains("reference") && text.contains("lib")
}

/// Emit `TS2726` for user-authored source-file `/// <reference lib="..." />`
/// directives whose value is empty or names a lib that does not exist.
///
/// `tsc` reports invalid initial lib names from user source files as
/// `TS2726 Cannot find lib definition for '<name>'.` anchored at the lib
/// attribute value. Transitive lib-to-lib references *inside* lib files
/// remain silently skipped — that policy lives in the resolver in
/// `tsz-core::config::resolve_lib_files_with_options_transitive`.
///
/// `no_lib` mirrors `--noLib`: when set, `tsc` ignores all lib references,
/// so we skip diagnostic emission too.
fn collect_source_reference_lib_diagnostics(
    sources: &[SourceEntry],
    no_lib: bool,
) -> Vec<Diagnostic> {
    if no_lib {
        return Vec::new();
    }
    let mut diagnostics = Vec::new();
    for source in sources {
        let positioned = if let Some(text) = source.text.as_deref() {
            if source_may_contain_reference_lib_directives(text) {
                tsz::config::extract_lib_references_with_positions(text)
            } else {
                Vec::new()
            }
        } else {
            std::fs::read_to_string(&source.path)
                .map(|text| {
                    if source_may_contain_reference_lib_directives(&text) {
                        tsz::config::extract_lib_references_with_positions(&text)
                    } else {
                        Vec::new()
                    }
                })
                .unwrap_or_default()
        };
        for reference in positioned {
            if tsz::config::is_known_lib_name(&reference.raw) {
                continue;
            }
            let message = format!("Cannot find lib definition for '{}'.", reference.raw.trim());
            diagnostics.push(Diagnostic::error(
                source.path.to_string_lossy().into_owned(),
                reference.start,
                reference.length,
                message,
                diagnostic_codes::CANNOT_FIND_LIB_DEFINITION_FOR,
            ));
        }
    }
    diagnostics
}

fn append_unique_lib_names(target: &mut Vec<String>, additional: Vec<String>) {
    let mut seen: FxHashSet<String> = target.iter().cloned().collect();
    for lib_name in additional {
        if seen.insert(lib_name.clone()) {
            target.push(lib_name);
        }
    }
}

fn lib_names_from_paths(paths: &[PathBuf]) -> Vec<String> {
    let mut lib_names = Vec::new();
    for path in paths {
        if let Some(lib_name) = lib_name_from_path(path) {
            append_unique_lib_names(&mut lib_names, vec![lib_name]);
        }
    }
    lib_names
}

fn lib_name_from_path(path: &Path) -> Option<String> {
    let file_name = path.file_name()?.to_str()?.to_ascii_lowercase();
    if let Some(package_name) = path.parent().and_then(|parent| parent.file_name())
        && let Some(package_name) = package_name.to_str()
        && let Some(root) = package_name.strip_prefix("lib-")
        && path
            .to_string_lossy()
            .contains("/node_modules/@typescript/")
    {
        return match file_name.as_str() {
            "index.d.ts" => Some(root.to_string()),
            other => other
                .strip_suffix(".d.ts")
                .map(|stem| format!("{root}.{stem}")),
        };
    }

    if file_name == "lib.d.ts" {
        return Some("lib".to_string());
    }

    let stem = file_name.strip_suffix(".d.ts")?;
    let stem = stem.strip_prefix("lib.").unwrap_or(stem);
    Some(match stem {
        "dom.generated" => "dom".to_string(),
        "dom.iterable.generated" => "dom.iterable".to_string(),
        "dom.asynciterable.generated" => "dom.asynciterable".to_string(),
        other => other.to_string(),
    })
}

fn resolve_compiler_lib_path(
    lib_name: &str,
    resolved: &ResolvedCompilerOptions,
    base_dir: &Path,
) -> Result<Option<PathBuf>> {
    if resolved.lib_replacement
        && let Some(replacement) = resolve_typescript_lib_replacement_path(base_dir, lib_name)
    {
        return Ok(Some(replacement));
    }

    Ok(
        resolve_lib_files_with_options(&[lib_name.to_string()], false)?
            .into_iter()
            .next(),
    )
}

fn resolve_typescript_lib_replacement_path(base_dir: &Path, lib_name: &str) -> Option<PathBuf> {
    let normalized = match lib_name.trim().to_ascii_lowercase().as_str() {
        "lib" => "es5".to_string(),
        "es6" => "es2015".to_string(),
        "es7" => "es2016".to_string(),
        other => other.to_string(),
    };
    let mut parts = normalized.split('.');
    let root = parts.next()?;
    let suffix = parts.collect::<Vec<_>>().join(".");
    let relative = if suffix.is_empty() {
        PathBuf::from("index.d.ts")
    } else {
        PathBuf::from(format!("{suffix}.d.ts"))
    };
    let candidate = base_dir
        .join("node_modules")
        .join("@typescript")
        .join(format!("lib-{root}"))
        .join(relative);
    candidate.is_file().then_some(candidate)
}

fn scan_typescript_dom_replacement_globals(lib_paths: &[PathBuf]) -> (bool, bool, bool) {
    let dom_paths: Vec<&PathBuf> = lib_paths
        .iter()
        .filter(|path| {
            path.to_string_lossy()
                .contains("/node_modules/@typescript/lib-dom/")
        })
        .collect();
    if dom_paths.is_empty() {
        return (false, false, false);
    }

    let has_window = dom_paths
        .iter()
        .any(|path| replacement_file_declares_global(path, "window"));
    let has_self = dom_paths
        .iter()
        .any(|path| replacement_file_declares_global(path, "self"));
    (true, has_window, has_self)
}

fn replacement_file_declares_global(path: &Path, name: &str) -> bool {
    let Ok(text) = std::fs::read_to_string(path) else {
        return false;
    };

    let declarations = [
        format!("declare var {name}"),
        format!("declare const {name}"),
        format!("declare let {name}"),
    ];
    declarations.iter().any(|needle| text.contains(needle))
}

struct SourceMeta {
    path: PathBuf,
    file_name: String,
    hash: u64,
    cached_ok: bool,
}

struct BuildProgramResult {
    program: Arc<MergedProgram>,
    dirty_paths: FxHashSet<PathBuf>,
    /// Number of times `merge_bind_results_ref` was called for this result.
    /// 0 means the fast path fired (merge skipped); 1 means a full merge ran.
    /// Only consumed by tests; production call sites only use `program`/`dirty_paths`.
    #[allow(dead_code)] // Dead in the lib build; exercised only by tests.
    merge_calls: u32,
}

fn build_program_with_cache(
    sources: Vec<SourceEntry>,
    cache: &mut CompilationCache,
    lib_files: &[Arc<LibFile>],
    language_version: ScriptTarget,
    module_detection: ModuleDetectionKind,
) -> BuildProgramResult {
    let mut meta = Vec::with_capacity(sources.len());
    let mut to_parse = Vec::new();
    let mut dirty_paths = FxHashSet::default();

    for source in sources {
        let file_name = source.path.to_string_lossy().into_owned();
        let (hash, cached_ok) = match source.text {
            Some(text) => {
                let hash = hash_text_with_language_version(&text, language_version);
                let cached_ok = cache
                    .bind_cache
                    .get(&source.path)
                    .is_some_and(|entry| entry.hash == hash);
                if !cached_ok {
                    dirty_paths.insert(source.path.clone());
                    to_parse.push((file_name.clone(), text));
                }
                (hash, cached_ok)
            }
            None => {
                // Missing source text without cached result - treat as error
                // Return default hash and mark as dirty to force re-parsing
                // This avoids crashing when cache is incomplete
                (0, false)
            }
        };

        meta.push(SourceMeta {
            path: source.path,
            file_name,
            hash,
            cached_ok,
        });
    }

    let nothing_to_parse = to_parse.is_empty();
    let parsed_results = if nothing_to_parse {
        Vec::new()
    } else {
        // Use parse_and_bind_parallel_with_libs to load prebound lib symbols
        // This ensures global symbols like console, Array, Promise are available
        // during binding, which prevents "Any poisoning" where unresolved symbols
        // default to Any type instead of emitting TS2304 errors.
        parallel::parse_and_bind_parallel_with_libs_and_options(
            to_parse,
            lib_files,
            language_version,
            module_detection,
        )
    };

    let mut parsed_map: FxHashMap<String, BindResult> = parsed_results
        .into_iter()
        .map(|result| (result.file_name.clone(), result))
        .collect();

    for entry in &meta {
        if entry.cached_ok {
            continue;
        }

        let result = match parsed_map.remove(&entry.file_name) {
            Some(r) => r,
            None => {
                // Missing parse result - this shouldn't happen in normal operation
                // Create a fallback empty result to allow compilation to continue
                // The error will be reported through diagnostics
                BindResult {
                    file_name: entry.file_name.clone(),
                    source_file: NodeIndex::NONE, // Invalid node index
                    arena: std::sync::Arc::new(NodeArena::new()),
                    symbols: Default::default(),
                    file_locals: Default::default(),
                    declared_modules: Default::default(),
                    module_exports: Default::default(),
                    node_symbols: Default::default(),
                    module_declaration_exports_publicly: Default::default(),
                    symbol_arenas: Default::default(),
                    declaration_arenas: Default::default(),
                    scopes: Default::default(),
                    node_scope_ids: Default::default(),
                    parse_diagnostics: Vec::new(),
                    shorthand_ambient_modules: Default::default(),
                    global_augmentations: Default::default(),
                    module_augmentations: Default::default(),
                    augmentation_target_modules: Default::default(),
                    reexports: Default::default(),
                    wildcard_reexports: Default::default(),
                    lib_binders: std::sync::Arc::new(Vec::new()),
                    lib_arenas: Vec::new(),
                    lib_symbol_ids: Default::default(),
                    lib_symbol_reverse_remap: Default::default(),
                    flow_nodes: Default::default(),
                    node_flow: Default::default(),
                    switch_clause_to_switch: Default::default(),
                    is_external_module: false, // Default to false for missing files
                    expando_properties: Default::default(),
                    alias_partners: Default::default(),
                    file_features: Default::default(),
                    semantic_defs: Default::default(),
                    file_import_sources: Vec::new(),
                }
            }
        };
        cache.bind_cache.insert(
            entry.path.clone(),
            BindCacheEntry {
                hash: entry.hash,
                bind_result: result,
            },
        );
    }

    let mut current_paths: FxHashSet<PathBuf> =
        FxHashSet::with_capacity_and_hasher(meta.len(), Default::default());
    for entry in &meta {
        current_paths.insert(entry.path.clone());
    }
    cache
        .bind_cache
        .retain(|path, _| current_paths.contains(path));

    // Fast path: when nothing changed (no re-parses needed) and the project
    // file set is the same size as when we last built the merged program, the
    // merge output is identical — return the cached Arc<MergedProgram> directly.
    // This skips O(total_symbols) symbol-remapping work on every
    // no-op pass (e.g. repeated benchmark row sweeps over an unchanged graph).
    if nothing_to_parse
        && meta.len() == cache.cached_file_count
        && let Some(ref cached) = cache.cached_merged_program
    {
        return BuildProgramResult {
            program: Arc::clone(cached),
            dirty_paths: FxHashSet::default(),
            merge_calls: 0,
        };
    }

    let mut ordered = Vec::with_capacity(meta.len());
    for entry in &meta {
        let Some(cached) = cache.bind_cache.get(&entry.path) else {
            continue;
        };
        ordered.push(&cached.bind_result);
    }

    let program = Arc::new(parallel::merge_bind_results_ref(&ordered));
    cache.cached_merged_program = Some(Arc::clone(&program));
    cache.cached_file_count = ordered.len();
    BuildProgramResult {
        program,
        dirty_paths,
        merge_calls: 1,
    }
}

fn update_import_symbol_ids(
    program: &MergedProgram,
    options: &ResolvedCompilerOptions,
    base_dir: &Path,
    cache: &mut CompilationCache,
) {
    let mut resolution_cache = ModuleResolutionCache::default();
    let mut import_symbol_ids: FxHashMap<PathBuf, FxHashMap<PathBuf, Vec<SymbolId>>> =
        FxHashMap::default();
    let mut star_export_dependencies: FxHashMap<PathBuf, FxHashSet<PathBuf>> = FxHashMap::default();

    // Build set of known file paths for module resolution
    let known_files: FxHashSet<PathBuf> = program
        .files
        .iter()
        .map(|f| PathBuf::from(&f.file_name))
        .collect();

    for (file_idx, file) in program.files.iter().enumerate() {
        let file_path = PathBuf::from(&file.file_name);
        let mut by_dep: FxHashMap<PathBuf, Vec<SymbolId>> = FxHashMap::default();
        let mut star_exports: FxHashSet<PathBuf> = FxHashSet::default();
        for (specifier, local_names) in collect_import_bindings(&file.arena, file.source_file) {
            let resolved = resolve_module_specifier(
                Path::new(&file.file_name),
                &specifier,
                options,
                base_dir,
                &mut resolution_cache,
                &known_files,
            );
            let Some(resolved) = resolved else {
                continue;
            };
            let canonical = normalize_resolved_path(&resolved, options);
            let entry = by_dep.entry(canonical).or_default();
            if let Some(file_locals) = program.file_locals.get(file_idx) {
                for name in local_names {
                    if let Some(sym_id) = file_locals.get(&name) {
                        entry.push(sym_id);
                    }
                }
            }
        }
        for (specifier, binding_nodes) in
            collect_export_binding_nodes(&file.arena, file.source_file)
        {
            let resolved = resolve_module_specifier(
                Path::new(&file.file_name),
                &specifier,
                options,
                base_dir,
                &mut resolution_cache,
                &known_files,
            );
            let Some(resolved) = resolved else {
                continue;
            };
            let canonical = normalize_resolved_path(&resolved, options);
            let entry = by_dep.entry(canonical).or_default();
            for node_idx in binding_nodes {
                if let Some(sym_id) = file.node_symbols.get(&node_idx.0).copied() {
                    entry.push(sym_id);
                }
            }
        }
        for specifier in collect_star_export_specifiers(&file.arena, file.source_file) {
            let resolved = resolve_module_specifier(
                Path::new(&file.file_name),
                &specifier,
                options,
                base_dir,
                &mut resolution_cache,
                &known_files,
            );
            let Some(resolved) = resolved else {
                continue;
            };
            let canonical = normalize_resolved_path(&resolved, options);
            star_exports.insert(canonical);
        }
        for symbols in by_dep.values_mut() {
            symbols.sort_by_key(|sym| sym.0);
            symbols.dedup();
        }
        if !star_exports.is_empty() {
            star_export_dependencies.insert(file_path.clone(), star_exports);
        }
        import_symbol_ids.insert(file_path, by_dep);
    }

    cache.import_symbol_ids = import_symbol_ids;
    cache.star_export_dependencies = star_export_dependencies;
}

#[path = "sources.rs"]
mod sources;
pub use sources::{FileReadResult, find_tsconfig, read_source_file};
pub(crate) use sources::{
    ResolveTsconfigError, config_base_dir, load_config, load_config_with_diagnostics,
    resolve_tsconfig_path,
};
use sources::{
    SourceEntry, SourceModuleResolution, SourceModuleResolutionKey, SourceReadResult,
    build_discovery_options, collect_type_root_files, hash_text_with_language_version,
    read_source_files, sources_have_no_default_lib,
};

#[path = "check.rs"]
mod check;
#[path = "check_module_graph.rs"]
mod check_module_graph;
#[path = "check_utils.rs"]
mod check_utils;
use check::{
    CollectDiagnosticsInput, collect_diagnostics_with_source_resolutions, load_checker_libs,
};

#[path = "config_deprecation.rs"]
mod config_deprecation;
use config_deprecation::NoEmitDeprecationInput;

#[path = "plan.rs"]
mod plan;
pub use plan::apply_cli_overrides;
use plan::{
    apply_cli_overrides_with_config_options, cli_ignore_deprecations_silences_6_0,
    cli_valid_override_keys, display_relative_to_dir, emit_common_source_directory,
    find_latest_dts_file, implicit_common_source_directory, is_deprecation_diagnostic_code,
    is_removed_option_diagnostic_code, is_removed_option_value_diagnostic_code,
    ordered_direct_cli_parse_diagnostics, validate_cli_compiler_option_diagnostics,
};

#[cfg(test)]
#[path = "config_deprecation_tests.rs"]
mod config_deprecation_tests;
#[cfg(test)]
#[path = "cross_file_alias_provider_private_reference_tests.rs"]
mod cross_file_alias_provider_private_reference_tests;
#[cfg(test)]
#[path = "cross_file_circular_alias_tests.rs"]
mod cross_file_circular_alias_tests;
#[cfg(test)]
#[path = "cross_file_conditional_alias_private_extends_tests.rs"]
mod cross_file_conditional_alias_private_extends_tests;
#[cfg(test)]
#[path = "cross_file_keyof_utility_alias_tests.rs"]
mod cross_file_keyof_utility_alias_tests;
#[cfg(test)]
#[path = "cross_file_lib_utility_indexed_access_tests.rs"]
mod cross_file_lib_utility_indexed_access_tests;
#[cfg(test)]
#[path = "cross_file_merged_value_self_typealias_typeof_tests.rs"]
mod cross_file_merged_value_self_typealias_typeof_tests;
#[cfg(test)]
#[path = "cross_file_typeof_class_constructor_tests.rs"]
mod cross_file_typeof_class_constructor_tests;
#[cfg(test)]
#[path = "declaration_qualified_typeof_reference_tests.rs"]
mod declaration_qualified_typeof_reference_tests;
#[cfg(test)]
#[path = "explain_files_reason_tests.rs"]
mod explain_files_reason_tests;
#[cfg(test)]
#[path = "core_merge_cache_tests.rs"]
mod merge_cache_tests;
#[cfg(test)]
#[path = "tests.rs"]
mod tests;

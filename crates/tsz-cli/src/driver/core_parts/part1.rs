use anyhow::{Context, Result, bail};

use std::collections::VecDeque;

use rustc_hash::{FxHashMap, FxHashSet};

use std::path::{Path, PathBuf};

use std::sync::Arc;

use std::time::Instant;

use crate::args::CliArgs;

use crate::config::{
    ResolvedCompilerOptions, TsConfig, load_tsconfig, load_tsconfig_with_diagnostics,
    resolve_compiler_options, resolve_lib_files_with_options,
    resolve_lib_files_with_options_transitive,
};

use tsz::binder::BinderOptions;

use tsz::binder::BinderState;

use tsz::binder::{SymbolId, SymbolTable};

use tsz::checker::TypeCache;

use tsz::checker::context::LibContext;

use tsz::checker::diagnostics::{
    Diagnostic, DiagnosticCategory, DiagnosticRelatedInformation, diagnostic_codes,
};

use tsz::checker::state::CheckerState;

use tsz::lib_loader::LibFile;

use tsz::module_resolver::{ImportKind, ImportingModuleKind, ModuleResolver};

use tsz::span::Span;

use tsz_binder::state::BinderStateScopeInputs;

use tsz_common::common::{ModuleKind, ScriptTarget};

use tsz_common::file_extensions::{
    JS_FAMILY_EXTENSIONS, JSON_EXTENSION, TS_FAMILY_EXTENSIONS, is_json_file,
};

use super::emit::{
    EmitOutputsContext, OutputFile, emit_outputs, normalize_root_dirs, normalize_type_roots,
    write_outputs,
};

pub(crate) use super::emit::{normalize_base_url, normalize_output_dir, normalize_root_dir};

#[cfg(test)]
use super::resolution::collect_module_specifiers;

use super::resolution::{
    ModuleResolutionCache, ProgramFileIndex, build_duplicate_package_redirects,
    canonicalize_or_owned, collect_export_binding_nodes, collect_import_bindings,
    collect_module_specifiers_for_check, collect_star_export_specifiers,
    collect_type_packages_from_root, default_type_roots, env_flag,
    implied_resolution_mode_for_file_with_cache, is_declaration_file,
    json_type_attribute_enables_json_module, module_specifier_has_type_json_import_attribute,
    normalize_path, normalize_resolved_path, resolve_module_specifier,
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

use tsz_solver::construction::QueryCache;


use diagnostic_source::diagnostic_source_line;

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
    /// Used by the CLI to pick the correct exit code: tsc returns
    /// `DiagnosticsPresent_OutputsGenerated` (2) for `--noEmit` regardless of
    /// where the option originated, since emit was disabled by configuration
    /// rather than skipped due to errors.
    pub no_emit: bool,
    pub request_cache_counters: tsz::checker::context::RequestCacheCounters,
    /// Number of interned types in the shared `TypeInterner` after checking.
    pub interned_types_count: usize,
    /// Estimated heap memory of the `TypeInterner` in bytes (populated for `--extendedDiagnostics`).
    pub interner_estimated_bytes: usize,
    /// Aggregate query-cache statistics (populated for `--extendedDiagnostics`).
    pub query_cache_stats: Option<tsz_solver::construction::QueryCacheStatistics>,
    /// Aggregate definition-store statistics (populated for `--extendedDiagnostics`).
    pub def_store_stats: Option<tsz_solver::StoreStatistics>,
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
    dependencies: FxHashMap<PathBuf, FxHashSet<PathBuf>>,
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
        dependencies: FxHashMap<PathBuf, FxHashSet<PathBuf>>,
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

/// Convert `CompilationCache` to `BuildInfo` for persistence
fn compilation_cache_to_build_info(
    cache: &CompilationCache,
    root_files: &[PathBuf],
    base_dir: &Path,
    options: &ResolvedCompilerOptions,
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
        latest_changed_dts_file: None, // TODO: Track most recently changed .d.ts file
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

        // Convert dependencies
        if let Some(deps) = build_info.get_dependencies(path_str) {
            let mut dep_paths = FxHashSet::default();
            for dep in deps {
                let dep_path = base_dir.join(dep);
                cache
                    .reverse_dependencies
                    .entry(dep_path.clone())
                    .or_default()
                    .insert(full_path.clone());
                dep_paths.insert(dep_path);
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
                        depth: 0,
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

/// Returns true if the given diagnostic code is a grammar-level error that should
/// take priority over TS5107/TS5101 deprecation diagnostics.
///
/// When deprecated compiler options produce TS5107, tsc makes them fatal (stops
/// compilation early). However, tsc suppresses TS5107 when real file-level grammar
/// errors exist. This function identifies which diagnostic codes count as "grammar
const fn is_grammar_error_for_deprecation_priority(code: u32) -> bool {
    matches!(
        code,
        8002 | 8003
            | 8004
            | 8006
            | 8008
            | 8009
            | 8010
            | 8011
            | 8013
            | 8015
            | 8016
            | 8017
            | 8018
            | 8037
            | 8038
            | 8039
    ) || matches!(code, 17002 | 17006 | 17007 | 17008 | 17012)
        || matches!(
            code,
            1002 | 1003
                | 1005
                | 1011
                | 1034
                | 1109
                | 1110
                | 1121
                | 1124
                | 1125
                | 1126
                | 1127
                | 1128
                | 1131
                | 1134
                | 1137
                | 1144
                | 1145
                | 1198
                | 1199
                | 1389
                | 1433
                | 1434
                | 1436
                | 1440
                | 1442
                | 1489
        )
        || matches!(code, 2458 | 2754)
}

fn remove_deprecation_diagnostics(diagnostics: &mut Vec<Diagnostic>) {
    diagnostics.retain(|d| !is_deprecation_diagnostic_code(d.code));
}

fn collect_parse_only_no_check_diagnostics(
    parse_results: &[parallel::ParseResult],
    options: &ResolvedCompilerOptions,
) -> Vec<Diagnostic> {
    let program_has_real_syntax_errors = parse_results
        .iter()
        .flat_map(|result| result.parse_diagnostics.iter())
        .any(|diag| check_utils::is_real_syntax_error(diag.code));

    #[cfg(not(target_arch = "wasm32"))]
    {
        use rayon::prelude::*;
        parse_results
            .par_iter()
            .flat_map(|result| {
                check_utils::collect_no_check_parse_diagnostics_for_file(
                    &result.file_name,
                    &result.arena,
                    result.source_file,
                    &result.parse_diagnostics,
                    options,
                    program_has_real_syntax_errors,
                )
            })
            .collect()
    }

    #[cfg(target_arch = "wasm32")]
    {
        parse_results
            .iter()
            .flat_map(|result| {
                check_utils::collect_no_check_parse_diagnostics_for_file(
                    &result.file_name,
                    &result.arena,
                    result.source_file,
                    &result.parse_diagnostics,
                    options,
                    program_has_real_syntax_errors,
                )
            })
            .collect()
    }
}

fn no_lib_core_global_type_diagnostics() -> Vec<Diagnostic> {
    [
        "Array",
        "Boolean",
        "Function",
        "IArguments",
        "Number",
        "Object",
        "RegExp",
        "String",
        "CallableFunction",
        "NewableFunction",
    ]
    .into_iter()
    .map(|name| {
        Diagnostic::error(
            String::new(),
            0,
            0,
            format!("Cannot find global type '{name}'."),
            diagnostic_codes::CANNOT_FIND_GLOBAL_TYPE,
        )
    })
    .collect()
}

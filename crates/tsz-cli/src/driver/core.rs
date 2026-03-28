use anyhow::{Result, bail};
use std::collections::VecDeque;

use rustc_hash::{FxHashMap, FxHashSet};
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Instant;

use crate::args::{CliArgs, Module, ModuleDetection};
use crate::config::{
    ResolvedCompilerOptions, TsConfig, checker_target_from_emitter, load_tsconfig,
    load_tsconfig_with_diagnostics, resolve_compiler_options, resolve_default_lib_files,
    resolve_lib_files, resolve_lib_files_with_options,
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
use tsz::module_resolver::ModuleResolver;
use tsz::span::Span;
use tsz_binder::state::BinderStateScopeInputs;
use tsz_common::common::ModuleKind;
// Re-export functions that other modules (e.g. watch) access via `driver::`.
use super::emit::{EmitOutputsContext, emit_outputs, normalize_type_roots, write_outputs};
pub(crate) use super::emit::{normalize_base_url, normalize_output_dir, normalize_root_dir};
use super::resolution::{
    ModuleResolutionCache, canonicalize_or_owned, collect_export_binding_nodes,
    collect_import_bindings, collect_module_specifiers, collect_star_export_specifiers,
    collect_type_packages_from_root, default_type_roots, env_flag, is_declaration_file,
    normalize_path, normalize_resolved_path, resolve_module_specifier, resolve_type_package_entry,
    resolve_type_package_from_roots,
};
use crate::fs::{FileDiscoveryOptions, default_include_patterns, discover_ts_files, is_js_file};
use crate::incremental::{BuildInfo, default_build_info_path};
use rustc_hash::FxHasher;
#[cfg(test)]
use std::cell::RefCell;
use tsz::parallel::{self, BindResult, BoundFile, MergedProgram};
use tsz::parser::NodeIndex;
use tsz::parser::ParseDiagnostic;
use tsz::parser::node::NodeArena;
use tsz::parser::syntax_kind_ext;
use tsz::scanner::SyntaxKind;
use tsz_solver::QueryCache;

/// Reason why a file was included in compilation
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FileInclusionReason {
    /// File specified as a root file (CLI argument or files list)
    RootFile,
    /// File matched by include pattern in tsconfig
    IncludePattern(String),
    /// File imported from another file
    ImportedFrom(PathBuf),
    /// File is a lib file (e.g., lib.es2020.d.ts)
    LibFile,
}

impl std::fmt::Display for FileInclusionReason {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::RootFile => write!(f, "Root file specified"),
            Self::IncludePattern(pattern) => {
                write!(f, "Matched by include pattern '{pattern}'")
            }
            Self::ImportedFrom(path) => {
                write!(f, "Imported from '{}'", path.display())
            }
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
/// Matches tsc's output categories: I/O read, parse+bind, check, emit.
#[derive(Debug, Clone, Default)]
pub struct PhaseTimings {
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
    pub request_cache_counters: tsz::checker::context::RequestCacheCounters,
    /// Number of interned types in the shared `TypeInterner` after checking.
    pub interned_types_count: usize,
    /// Estimated heap memory of the `TypeInterner` in bytes (populated for `--extendedDiagnostics`).
    pub interner_estimated_bytes: usize,
    /// Aggregate query-cache statistics (populated for `--extendedDiagnostics`).
    pub query_cache_stats: Option<tsz_solver::QueryCacheStatistics>,
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
    reverse_dependencies: FxHashMap<PathBuf, FxHashSet<PathBuf>>,
    diagnostics: FxHashMap<PathBuf, Vec<Diagnostic>>,
    export_hashes: FxHashMap<PathBuf, u64>,
    import_symbol_ids: FxHashMap<PathBuf, FxHashMap<PathBuf, Vec<SymbolId>>>,
    star_export_dependencies: FxHashMap<PathBuf, FxHashSet<PathBuf>>,
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
        self.reverse_dependencies.clear();
        self.diagnostics.clear();
        self.export_hashes.clear();
        self.import_symbol_ids.clear();
        self.star_export_dependencies.clear();
    }

    pub(crate) fn update_dependencies(
        &mut self,
        dependencies: FxHashMap<PathBuf, FxHashSet<PathBuf>>,
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
        FileInfo as IncrementalFileInfo,
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

        // Create file info with version (hash) and signature
        let version = format!("{hash:016x}");
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
                        category: match r.category {
                            0 => DiagnosticCategory::Warning,
                            1 => DiagnosticCategory::Error,
                            2 => DiagnosticCategory::Suggestion,
                            _ => DiagnosticCategory::Message,
                        },
                        code: r.code,
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
    Some(default_build_info_path(config_path, out_dir.as_deref()))
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
/// errors" for that suppression logic.
///
/// NOT a blanket 17000..18000 range: many 17xxx codes (17009 "super before this",
/// 17011 "super before property access") are checker-level semantic errors that
/// must NOT suppress TS5107.
const fn is_grammar_error_for_deprecation_priority(code: u32) -> bool {
    // Only a narrow subset of 8xxx codes are true JS grammar / parse failures.
    // JSDoc validation errors like TS8024 should not suppress TS5107.
    matches!(code,
        8002 // 'import ... =' can only be used in TypeScript files
        | 8003 // Type assertion expressions can only be used in TypeScript files
        | 8004 // 'readonly' type modifier is only permitted on array and tuple literal types
        | 8006 // 'interface' declarations can only be used in TypeScript files
        | 8008 // Type aliases can only be used in TypeScript files
        | 8009 // The '?' modifier can only be used in TypeScript files
        | 8010 // Type annotations can only be used in TypeScript files
        | 8011 // Type arguments can only be used in TypeScript files
        | 8013 // Non-null assertions can only be used in TypeScript files
        | 8015 // Namespace declarations can only be used in TypeScript files
        | 8016 // Type assertion expressions can only be used in TypeScript files
        | 8017 // Signature declarations can only be used in TypeScript files
        | 8018 // Type-only import/export syntax can only be used in TypeScript files
    )
    // Specific 17xxx grammar-level errors only.
    || matches!(code,
        17002 // Expected corresponding JSX closing tag
        | 17006 // Unary expression not allowed as LHS of exponentiation
        | 17007 // Type assertion not allowed as LHS of exponentiation
        | 17008 // JSX element has no corresponding closing tag
        | 17012 // 'import.meta' meta-property grammar error
    )
    // Specific 1xxx codes that reliably indicate real parse failures
    // (verified against tsc: these are never false positives in our parser
    // for tests where tsc expects TS5107)
    || matches!(code,
        1002  // Unterminated string literal
        | 1003  // Identifier expected
        | 1005  // 'X' expected (colon, comma, semicolon, etc.)
        | 1011  // '(' or '<' expected
        | 1034  // 'super' must be followed by argument list or member access
        | 1109 // Expression expected
        | 1110 // Type expected
        | 1121 // Octal literals are not allowed in strict mode
        | 1124 // Digit expected
        | 1125 // Hexadecimal digit expected
        | 1126 // Unexpected end of text
        | 1127 // Invalid character
        | 1128 // Declaration or statement expected
        | 1131 // Property or signature expected
        | 1134 // Variable declaration expected
        | 1137 // Expression or comma expected
        | 1144 // '{' or ';' expected
        | 1145 // '{' or JSX element expected
        | 1198 // An extended Unicode escape value must be between 0x0 and 0x10FFFF
        | 1199 // Value of type '{0}' is not callable
        // NOTE: 1359 ('await' is a reserved word) is NOT included — our parser
        // false-positives on TS1359 in async tests where tsc expects TS5107 only.
        | 1433 // Neither decorators nor modifiers may be applied to 'this' parameters
        | 1434 // Top-level 'await' expressions are only allowed...
        | 1436 // Decorators are not valid here
        | 1389 // '{0}' is not allowed as a variable declaration name
        | 1440 // Variable declaration not allowed at this location
        | 1442 // Expected '=' for property initializer
        | 1489 // Decimals with leading zeros are not allowed
    )
    // Specific 2xxx codes that tsc treats as syntactic/preprocessing errors
    // (emitted during early phases, before semantic analysis)
    || matches!(code,
        2458 // An AMD module cannot have multiple name assignments
        | 2754 // 'super' may not use type arguments
    )
}

fn compile_inner(
    args: &CliArgs,
    cwd: &Path,
    mut cache: Option<&mut CompilationCache>,
    changed_paths: Option<&[PathBuf]>,
    forced_dirty_paths: Option<&FxHashSet<PathBuf>>,
    explicit_config_path: Option<&Path>,
) -> Result<CompilationResult> {
    let _compile_span = tracing::info_span!("compile", cwd = %cwd.display()).entered();
    let perf_enabled = std::env::var_os("TSZ_PERF").is_some();
    let compile_start = Instant::now();

    let perf_log_phase = |phase: &'static str, start: Instant| {
        if perf_enabled {
            tracing::info!(
                target: "wasm::perf",
                phase,
                ms = start.elapsed().as_secs_f64() * 1000.0
            );
        }
    };

    let cwd = normalize_path(cwd);
    let tsconfig_path = if args.ignore_config {
        // --ignoreConfig: skip tsconfig.json discovery and loading entirely
        None
    } else if let Some(path) = explicit_config_path {
        Some(path.to_path_buf())
    } else {
        match resolve_tsconfig_path(&cwd, args.project.as_deref()) {
            Ok(path) => path,
            Err(err) => {
                return Ok(config_error_result(
                    None,
                    err.to_string(),
                    diagnostic_codes::CANNOT_FIND_A_TSCONFIG_JSON_FILE_AT_THE_SPECIFIED_DIRECTORY,
                ));
            }
        }
    };
    let loaded = load_config_with_diagnostics(tsconfig_path.as_deref())?;
    let config = loaded.config;
    let mut config_diagnostics = loaded.diagnostics;

    // TS5103 (invalid ignoreDeprecations value) and TS5102 (removed option) are fatal
    // in tsc: they stop compilation and report only config-level errors.
    // Match this behavior to avoid extra file-level diagnostics.
    if config_diagnostics.iter().any(|d| {
        d.code == diagnostic_codes::INVALID_VALUE_FOR_IGNOREDEPRECATIONS
            || d.code
                == diagnostic_codes::OPTION_HAS_BEEN_REMOVED_PLEASE_REMOVE_IT_FROM_YOUR_CONFIGURATION
    }) {
        return Ok(CompilationResult {
            diagnostics: config_diagnostics,
            emitted_files: Vec::new(),
            files_read: Vec::new(),
            file_infos: Vec::new(),
            request_cache_counters: tsz::checker::context::RequestCacheCounters::default(),
            interned_types_count: 0,
            interner_estimated_bytes: 0,
            query_cache_stats: None,
            def_store_stats: None,
            phase_timings: PhaseTimings::default(),
            residency_stats: None,
            module_dep_stats: None,
            invalidation_summaries: Vec::new(),
        });
    }

    // Track whether TS5107/TS5101 deprecation diagnostics exist for handling below.
    let has_deprecation_diagnostics = config_diagnostics.iter().any(|d| {
        d.code
            == diagnostic_codes::OPTION_IS_DEPRECATED_AND_WILL_STOP_FUNCTIONING_IN_TYPESCRIPT_SPECIFY_COMPILEROPT_2
            || d.code
                == diagnostic_codes::OPTION_IS_DEPRECATED_AND_WILL_STOP_FUNCTIONING_IN_TYPESCRIPT_SPECIFY_COMPILEROPT
    });

    let mut resolved = match resolve_compiler_options(
        config
            .as_ref()
            .and_then(|cfg| cfg.compiler_options.as_ref()),
    ) {
        Ok(r) => r,
        Err(e) => {
            // If config has errors (e.g., TS5103 for invalid ignoreDeprecations),
            // return them even if compiler options resolution fails.
            // This ensures config diagnostics like TS5107 are reported to the user.
            if !config_diagnostics.is_empty() {
                return Ok(CompilationResult {
                    diagnostics: config_diagnostics,
                    emitted_files: Vec::new(),
                    files_read: Vec::new(),
                    file_infos: Vec::new(),
                    request_cache_counters: tsz::checker::context::RequestCacheCounters::default(),
                    interned_types_count: 0,
                    interner_estimated_bytes: 0,
                    query_cache_stats: None,
                    def_store_stats: None,
                    phase_timings: PhaseTimings::default(),
                    residency_stats: None,
                    module_dep_stats: None,
                    invalidation_summaries: Vec::new(),
                });
            }
            return Err(e);
        }
    };
    apply_cli_overrides(&mut resolved, args)?;

    // Wire removed-but-honored suppress flags from config
    if loaded.suppress_excess_property_errors {
        resolved.checker.suppress_excess_property_errors = true;
    }
    if loaded.suppress_implicit_any_index_errors {
        resolved.checker.suppress_implicit_any_index_errors = true;
    }
    if loaded.no_implicit_use_strict {
        resolved.checker.no_implicit_use_strict = true;
    }
    if resolved.allow_importing_ts_extensions {
        resolved.checker.allow_importing_ts_extensions = true;
    }
    if resolved.rewrite_relative_import_extensions {
        resolved.checker.rewrite_relative_import_extensions = true;
        resolved.printer.rewrite_relative_import_extensions = true;
    }
    if config.is_none()
        && args.module.is_none()
        && matches!(resolved.printer.module, ModuleKind::None)
    {
        // When no tsconfig is present, align with tsc's computed module default:
        // ES2015+ targets -> ES2015 modules, older targets -> CommonJS.
        let default_module = if resolved.printer.target.supports_es2015() {
            ModuleKind::ES2015
        } else {
            ModuleKind::CommonJS
        };
        resolved.printer.module = default_module;
        resolved.checker.module = default_module;
    }

    let base_dir = config_base_dir(&cwd, tsconfig_path.as_deref());
    let base_dir = if resolved.preserve_symlinks {
        normalize_path(&base_dir)
    } else {
        canonicalize_or_owned(&base_dir)
    };
    let root_dir_display = resolved.root_dir.clone();
    let root_dir = normalize_root_dir(&base_dir, resolved.root_dir.clone());
    let out_dir = normalize_output_dir(&base_dir, resolved.out_dir.clone());
    let declaration_dir = normalize_output_dir(&base_dir, resolved.declaration_dir.clone());
    let base_url = normalize_base_url(&base_dir, resolved.base_url.clone());
    resolved.root_dir = root_dir.clone();
    resolved.out_dir = out_dir.clone();
    resolved.declaration_dir = declaration_dir.clone();
    resolved.base_url = base_url;
    resolved.type_roots = normalize_type_roots(&base_dir, resolved.type_roots.clone());

    let discovery = build_discovery_options(
        args,
        &base_dir,
        tsconfig_path.as_deref(),
        config.as_ref(),
        out_dir.as_deref(),
        &resolved,
    )?;
    let mut file_paths = discover_ts_files(&discovery)?;

    // If config validation already emitted TS5110 (module/moduleResolution mismatch),
    // bail out early — compilation cannot proceed with incompatible settings.
    // tsc still emits TS18003 alongside TS5110 when no input files are found,
    // so we must check file discovery before bailing.
    if config_diagnostics.iter().any(|d| {
        d.code
            == diagnostic_codes::OPTION_MODULE_MUST_BE_SET_TO_WHEN_OPTION_MODULERESOLUTION_IS_SET_TO
    }) {
        let diagnostics = if file_paths.is_empty() && !discovery.files_explicitly_set {
            no_input_diagnostics_for_config(
                config_diagnostics,
                tsconfig_path.as_deref(),
                discovery.include.as_deref(),
                discovery.exclude.as_deref(),
                discovery.allow_js,
            )
        } else {
            config_diagnostics
        };
        return Ok(CompilationResult {
            diagnostics,
            emitted_files: Vec::new(),
            files_read: Vec::new(),
            file_infos: Vec::new(),
            request_cache_counters: tsz::checker::context::RequestCacheCounters::default(),
            interned_types_count: 0,
            interner_estimated_bytes: 0,
            query_cache_stats: None,
            def_store_stats: None,
            phase_timings: PhaseTimings::default(),
            residency_stats: None,
            module_dep_stats: None,
            invalidation_summaries: Vec::new(),
        });
    }

    // Track if we should save BuildInfo after successful compilation
    let mut should_save_build_info = false;

    // Local cache for BuildInfo-loaded compilation state
    // Only create when loading from BuildInfo (not when a cache is provided)
    let mut local_cache: Option<CompilationCache> = None;

    // Load BuildInfo only when incremental compilation is enabled and no cache was provided.
    // A standalone `tsBuildInfoFile` path does not activate build info reads/writes.
    if cache.is_none() && resolved.incremental {
        let tsconfig_path_ref = tsconfig_path.as_deref();
        if let Some(build_info_path) = get_build_info_path(tsconfig_path_ref, &resolved, &base_dir)
        {
            if build_info_path.exists() {
                match BuildInfo::load(&build_info_path) {
                    Ok(Some(build_info)) => {
                        // Create a local cache from BuildInfo
                        local_cache = Some(build_info_to_compilation_cache(&build_info, &base_dir));
                        tracing::info!("Loaded BuildInfo from: {}", build_info_path.display());
                    }
                    Ok(None) => {
                        tracing::info!(
                            "BuildInfo at {} is outdated or incompatible, starting fresh",
                            build_info_path.display()
                        );
                    }
                    Err(e) => {
                        tracing::warn!(
                            "Failed to load BuildInfo from {}: {}, starting fresh",
                            build_info_path.display(),
                            e
                        );
                    }
                }
            } else {
                // BuildInfo doesn't exist yet, create empty local cache for new compilation
                local_cache = Some(CompilationCache::default());
            }
            should_save_build_info = true;
        }
    }

    // Determine which cache to use: local cache from BuildInfo, or provided cache, or none
    // When cache is None, we can use local_cache; otherwise we use the provided cache
    if file_paths.is_empty() {
        // When `files` is explicitly set (e.g., `"files": []` in a solution-style
        // tsconfig), tsc does NOT emit TS18003. The error only applies when discovery
        // found nothing due to include/exclude patterns.
        let diagnostics = if discovery.files_explicitly_set {
            config_diagnostics
        } else {
            no_input_diagnostics_for_config(
                config_diagnostics,
                tsconfig_path.as_deref(),
                discovery.include.as_deref(),
                discovery.exclude.as_deref(),
                discovery.allow_js,
            )
        };
        return Ok(CompilationResult {
            diagnostics,
            emitted_files: Vec::new(),
            files_read: Vec::new(),
            file_infos: Vec::new(),
            request_cache_counters: tsz::checker::context::RequestCacheCounters::default(),
            interned_types_count: 0,
            interner_estimated_bytes: 0,
            query_cache_stats: None,
            def_store_stats: None,
            phase_timings: PhaseTimings::default(),
            residency_stats: None,
            module_dep_stats: None,
            invalidation_summaries: Vec::new(),
        });
    }

    let mut root_dir_diagnostic_roots: FxHashSet<PathBuf> = FxHashSet::default();
    if let Some(ref root_dir_path) = root_dir {
        let canonical_root = canonicalize_or_owned(root_dir_path);
        for file_path in &file_paths {
            if is_declaration_file(file_path) {
                continue;
            }
            let canonical_file = canonicalize_or_owned(file_path);
            if !canonical_file.starts_with(&canonical_root) {
                root_dir_diagnostic_roots.insert(canonical_file);
            }
        }
    }

    let root_file_paths = file_paths.clone();
    let source_directive_no_types_and_symbols = if resolved.checker.no_types_and_symbols {
        true
    } else {
        file_paths.iter().any(|path| match read_source_file(path) {
            FileReadResult::Text(text) => sources::has_no_types_and_symbols_directive(&text),
            FileReadResult::Binary { text, .. } => {
                sources::has_no_types_and_symbols_directive(&text)
            }
            FileReadResult::Error(_) => false,
        })
    };
    resolved.checker.no_types_and_symbols = source_directive_no_types_and_symbols;

    let (type_files, unresolved_types) = collect_type_root_files(&base_dir, &resolved);

    // Add type definition files (e.g., @types packages) to the source file list.
    // Note: lib.d.ts files are NOT added here - they are loaded separately via
    // lib preloading + checker lib contexts. This prevents them from
    // being type-checked as regular source files (which would emit spurious errors).
    if !type_files.is_empty() {
        let mut merged = std::collections::BTreeSet::new();
        merged.extend(file_paths);
        merged.extend(type_files);
        file_paths = merged.into_iter().collect();
    }

    let changed_set = changed_paths.map(|paths| {
        paths
            .iter()
            .map(|path| canonicalize_or_owned(path))
            .collect::<FxHashSet<_>>()
    });

    // Create a unified effective cache reference that works for both cases
    // This follows Gemini's recommended pattern to handle the two cache sources
    let local_cache_ref = local_cache.as_mut();
    let mut effective_cache = local_cache_ref.or(cache.as_deref_mut());

    let read_sources_start = Instant::now();
    let SourceReadResult {
        sources: all_sources,
        dependencies,
        type_reference_errors,
        resolution_mode_errors,
    } = {
        read_source_files(
            &file_paths,
            &base_dir,
            &resolved,
            effective_cache.as_deref(),
            changed_set.as_ref(),
        )?
    };
    let io_read_duration = read_sources_start.elapsed();
    perf_log_phase("read_sources", read_sources_start);

    if let Some(ref root_dir_path) = root_dir {
        let canonical_root = canonicalize_or_owned(root_dir_path);
        let root_display_path = root_dir_display.as_ref().unwrap_or(root_dir_path);
        let mut blame_files: FxHashSet<PathBuf> = root_dir_diagnostic_roots;
        for root_file in &root_file_paths {
            if is_declaration_file(root_file) {
                continue;
            }
            let canonical_root_file = canonicalize_or_owned(root_file);
            if blame_files.contains(&canonical_root_file) {
                continue;
            }
            let Some(deps) = dependencies.get(&canonical_root_file) else {
                continue;
            };
            if deps.iter().any(|dep| {
                is_declaration_file(dep) && !canonicalize_or_owned(dep).starts_with(&canonical_root)
            }) {
                blame_files.insert(canonical_root_file);
            }
        }
        let mut blame_files: Vec<_> = blame_files.into_iter().collect();
        blame_files.sort();
        for file_path in blame_files {
            let file_display = file_path.to_string_lossy();
            let root_display = root_display_path.to_string_lossy();
            let message = format!(
                "File '{file_display}' is not under 'rootDir' '{root_display}'. 'rootDir' is expected to contain all source files."
            );
            config_diagnostics.push(Diagnostic::error(
                String::new(),
                0,
                0,
                message,
                diagnostic_codes::FILE_IS_NOT_UNDER_ROOTDIR_ROOTDIR_IS_EXPECTED_TO_CONTAIN_ALL_SOURCE_FILES,
            ));
        }
    }

    // Update dependencies in the cache
    if let Some(ref mut c) = effective_cache {
        c.update_dependencies(dependencies);
    }

    // Separate binary files from regular sources - binary files get TS1490
    let mut type_file_diagnostics: Vec<Diagnostic> = Vec::new();
    for (path, type_name, types_offset, types_len) in type_reference_errors {
        let file_name = path.to_string_lossy().into_owned();
        type_file_diagnostics.push(Diagnostic::error(
            file_name,
            types_offset as u32,
            types_len as u32,
            format!("Cannot find type definition file for '{type_name}'."),
            diagnostic_codes::CANNOT_FIND_TYPE_DEFINITION_FILE_FOR,
        ));
    }
    // TS1453: Invalid resolution-mode values in triple-slash directives
    for (path, start, length) in resolution_mode_errors {
        let file_name = path.to_string_lossy().into_owned();
        type_file_diagnostics.push(Diagnostic::error(
            file_name,
            start as u32,
            length as u32,
            "`resolution-mode` should be either `require` or `import`.".to_string(),
            diagnostic_codes::RESOLUTION_MODE_SHOULD_BE_EITHER_REQUIRE_OR_IMPORT,
        ));
    }
    // Emit TS2688 for unresolved entries in tsconfig `types` array
    for type_name in &unresolved_types {
        type_file_diagnostics.push(Diagnostic::error(
            String::new(),
            0,
            0,
            format!("Cannot find type definition file for '{type_name}'."),
            diagnostic_codes::CANNOT_FIND_TYPE_DEFINITION_FILE_FOR,
        ));
    }

    let mut binary_file_diagnostics: Vec<Diagnostic> = Vec::new();
    let mut binary_file_names_to_suppress: FxHashSet<String> = FxHashSet::default();
    let mut sources: Vec<SourceEntry> = Vec::with_capacity(all_sources.len());
    for source in all_sources {
        if source.is_binary {
            // Emit TS1490 "File appears to be binary." for binary files.
            let file_name = source.path.to_string_lossy().into_owned();
            if source.suppress_parser_diagnostics {
                // Hard-binary cases like invalid UTF-8 or null-byte corruption should
                // surface only TS1490, matching tsc's early binary bailout.
                binary_file_names_to_suppress.insert(file_name.clone());
            }
            binary_file_diagnostics.push(Diagnostic::error(
                file_name,
                0,
                0,
                "File appears to be binary.".to_string(),
                diagnostic_codes::FILE_APPEARS_TO_BE_BINARY,
            ));
        }
        sources.push(source);
    }

    // Collect user source files that were read before sources is moved
    let mut user_files_read: Vec<PathBuf> = sources.iter().map(|s| s.path.clone()).collect();
    user_files_read.sort();

    // Build file info with inclusion reasons
    let file_infos = build_file_infos(&sources, &file_paths, args, config.as_ref(), &base_dir);

    let disable_default_libs = resolved.lib_is_default && sources_have_no_default_lib(&sources);
    resolved.checker.no_types_and_symbols =
        resolved.checker.no_types_and_symbols || sources_have_no_types_and_symbols(&sources);
    let lib_paths =
        resolve_effective_lib_paths(&resolved, &sources, &base_dir, disable_default_libs)?;
    let typescript_dom_replacement_globals = scan_typescript_dom_replacement_globals(&lib_paths);
    let lib_path_refs: Vec<&Path> = lib_paths.iter().map(PathBuf::as_path).collect();

    // Build files_read: lib files first (matching tsc --listFiles order), then user files
    let mut files_read: Vec<PathBuf> = Vec::with_capacity(lib_paths.len() + user_files_read.len());
    files_read.extend(lib_paths.iter().cloned());
    files_read.append(&mut user_files_read);
    // Load and bind each lib exactly once, then reuse for:
    // 1) user-file binding (global symbol availability during bind)
    // 2) checker lib contexts (global symbol/type resolution)
    let load_libs_start = Instant::now();
    let lib_files: Vec<Arc<LibFile>> = parallel::load_lib_files_for_binding_strict(&lib_path_refs)?;
    let load_libs_duration = load_libs_start.elapsed();
    perf_log_phase("load_libs", load_libs_start);

    // PERF: Start loading checker lib contexts in a background thread while we
    // build the user program. The checker needs fresh binder state (separate from
    // the binding-phase libs) because it mutates during declaration merging.
    // By overlapping this with user file parsing+binding, we save ~100ms on
    // workloads that load many lib files (e.g., default target with dom.d.ts).
    let checker_lib_handle = if !resolved.no_check {
        let lib_files_clone = lib_files.clone();
        Some(std::thread::spawn(move || {
            load_lib_files_for_contexts(&lib_files_clone)
        }))
    } else {
        None
    };

    let build_program_start = Instant::now();
    let (program, dirty_paths) = if let Some(ref mut c) = effective_cache {
        let result = build_program_with_cache(sources, c, &lib_files);
        (result.program, Some(result.dirty_paths))
    } else {
        let compile_inputs: Vec<(String, String)> = sources
            .into_iter()
            .map(|source| {
                let text = source.text.unwrap_or_else(|| {
                    // If source text is missing during compilation, use empty string
                    // This allows compilation to continue with a diagnostic error later
                    String::new()
                });
                (source.path.to_string_lossy().into_owned(), text)
            })
            .collect();
        let bind_results = parallel::parse_and_bind_parallel_with_libs(compile_inputs, &lib_files);
        (parallel::merge_bind_results(bind_results), None)
    };
    let parse_bind_duration = build_program_start.elapsed();
    perf_log_phase("build_program", build_program_start);

    // Update import symbol IDs if we have a cache
    if let Some(ref mut c) = effective_cache {
        update_import_symbol_ids(&program, &resolved, &base_dir, c);
    }

    // Wait for checker lib contexts (already running in background)
    let build_lib_contexts_start = Instant::now();
    let lib_contexts = match checker_lib_handle {
        Some(handle) => handle.join().expect("checker lib loading panicked"),
        None => Vec::new(),
    };
    perf_log_phase("build_lib_contexts", build_lib_contexts_start);

    let collect_diagnostics_start = Instant::now();
    let parallel_type_caches = std::sync::Mutex::new(FxHashMap::default());
    let collected = collect_diagnostics(
        &program,
        &resolved,
        &base_dir,
        effective_cache,
        &lib_contexts,
        typescript_dom_replacement_globals,
        &parallel_type_caches,
        has_deprecation_diagnostics,
    );
    let mut diagnostics: Vec<Diagnostic> = collected.diagnostics;
    let check_duration = collect_diagnostics_start.elapsed();
    perf_log_phase("collect_diagnostics", collect_diagnostics_start);

    // Get reference to type caches for declaration emit.
    // In the parallel (no-cache) path, type caches are returned via the
    // Mutex parameter. In the cached/incremental path they live in the
    // CompilationCache.
    let parallel_type_caches = parallel_type_caches
        .into_inner()
        .expect("parallel_type_caches mutex should not be poisoned");
    let type_caches_ref: &FxHashMap<_, _> = if !parallel_type_caches.is_empty() {
        &parallel_type_caches
    } else {
        local_cache
            .as_ref()
            .map(|c| &c.type_caches)
            .or_else(|| cache.as_ref().map(|c| &c.type_caches))
            .unwrap_or(&parallel_type_caches)
    };
    // For binary files, suppress all diagnostics except TS1490.
    // Parsing UTF-16/corrupted content as UTF-8 produces cascading
    // TS1127 "Invalid character" false positives; tsc detects binary files
    // early and only emits TS1490.
    if !binary_file_names_to_suppress.is_empty() {
        diagnostics.retain(|d| !binary_file_names_to_suppress.contains(&d.file));
    }
    // tsc 6.0 deprecation diagnostic handling:
    // TS5107/TS5101 are fatal in tsc 6.0: tsc stops compilation early and never emits
    // file-level diagnostics (syntactic or semantic) alongside them.
    //
    // tsc suppresses TS5107 when real file-level grammar errors exist (preferring file
    // errors over config deprecation warnings). We use a narrow whitelist of grammar
    // error codes that tsc reliably emits — our parser can produce false-positive 1xxx
    // codes that would wrongly suppress TS5107 if we checked the full range.
    if has_deprecation_diagnostics {
        let has_reliable_grammar_errors = diagnostics
            .iter()
            .any(|d| is_grammar_error_for_deprecation_priority(d.code));
        if has_reliable_grammar_errors {
            // Real grammar errors take priority — drop TS5107 from config diagnostics.
            config_diagnostics.retain(|d| {
                d.code
                    != diagnostic_codes::OPTION_IS_DEPRECATED_AND_WILL_STOP_FUNCTIONING_IN_TYPESCRIPT_SPECIFY_COMPILEROPT_2
                    && d.code
                        != diagnostic_codes::OPTION_IS_DEPRECATED_AND_WILL_STOP_FUNCTIONING_IN_TYPESCRIPT_SPECIFY_COMPILEROPT
            });
        } else {
            // No reliable file-level errors — TS5107 takes priority (fatal).
            // Preserve global-level TS2318 ("Cannot find global type") because
            // tsc emits these alongside deprecation warnings. These are
            // identified by empty file name and position 0 (global diagnostics),
            // as opposed to file-level TS2318 from type checking which tsc
            // suppresses along with other file-level errors.
            diagnostics.retain(|d| d.code == 2318 && d.file.is_empty() && d.start == 0);
        }
    }
    diagnostics.extend(config_diagnostics);
    diagnostics.extend(binary_file_diagnostics);
    diagnostics.extend(type_file_diagnostics);

    // TS2304 suppression near TS8xxx JS grammar errors.
    // When TS8xxx errors exist in a project (type annotations in JS, JSDoc tag
    // errors, etc.), our checker can emit cascading false TS2304 errors. Suppress
    // TS2304 unless it co-occurs at the exact same file+position as a TS8xxx
    // error — tsc emits both in cases like `@extends {Mismatch}` (TS2304 + TS8023).
    {
        let has_js_grammar_errors = diagnostics.iter().any(|d| (8000..9000).contains(&d.code));
        if has_js_grammar_errors {
            let ts8xxx_positions: rustc_hash::FxHashSet<(String, u32)> = diagnostics
                .iter()
                .filter(|d| (8000..9000).contains(&d.code))
                .map(|d| (d.file.clone(), d.start))
                .collect();
            diagnostics.retain(|d| {
                d.code != 2304 || ts8xxx_positions.contains(&(d.file.clone(), d.start))
            });
        }
    }

    diagnostics.sort_by(|left, right| {
        left.file
            .cmp(&right.file)
            .then(left.start.cmp(&right.start))
            .then(left.code.cmp(&right.code))
    });

    let has_error = diagnostics
        .iter()
        .any(|diag| diag.category == DiagnosticCategory::Error);
    let should_emit = !(resolved.no_emit || (resolved.no_emit_on_error && has_error));

    // When --declaration is set, run declaration emit for diagnostics even
    // with --noEmit, because TS2883 (non-portable inferred types) fires
    // during declaration generation. In tsc, this check happens during the
    // checker's "declaration emit pre-check" phase.
    let should_run_declaration_emit_check =
        !should_emit && resolved.emit_declarations && resolved.no_emit;

    let mut dirty_paths = dirty_paths;
    if let Some(forced) = forced_dirty_paths {
        match &mut dirty_paths {
            Some(existing) => {
                existing.extend(forced.iter().cloned());
            }
            None => {
                dirty_paths = Some(forced.clone());
            }
        }
    }

    let emit_outputs_start = Instant::now();
    let emitted_files = if !should_emit && !should_run_declaration_emit_check {
        Vec::new()
    } else {
        let (outputs, emit_diags) = emit_outputs(EmitOutputsContext {
            program: &program,
            options: &resolved,
            base_dir: &base_dir,
            root_dir: root_dir.as_deref(),
            out_dir: out_dir.as_deref(),
            declaration_dir: declaration_dir.as_deref(),
            dirty_paths: dirty_paths.as_ref(),
            type_caches: type_caches_ref,
        })?;
        diagnostics.extend(emit_diags);
        if should_emit {
            write_outputs(&outputs)?
        } else {
            // Declaration emit ran for diagnostics only (--noEmit with --declaration)
            Vec::new()
        }
    };
    let emit_duration = emit_outputs_start.elapsed();
    perf_log_phase("emit_outputs", emit_outputs_start);

    // Recompute has_error after emit diagnostics (e.g., TS2883) are added.
    // The initial has_error was computed before emit for should_emit gating.
    normalize_ts2883_diagnostics_in_place(&mut diagnostics);
    // Re-sort since emit diagnostics were appended after the initial sort.
    diagnostics.sort_by(|left, right| {
        left.file
            .cmp(&right.file)
            .then(left.start.cmp(&right.start))
            .then(left.code.cmp(&right.code))
    });
    let has_error = diagnostics
        .iter()
        .any(|diag| diag.category == DiagnosticCategory::Error);

    // Find the most recent .d.ts file for BuildInfo tracking
    let latest_changed_dts_file = if !emitted_files.is_empty() {
        find_latest_dts_file(&emitted_files, &base_dir)
    } else {
        None
    };

    // Save BuildInfo if incremental compilation is enabled
    if should_save_build_info && !has_error {
        let tsconfig_path_ref = tsconfig_path.as_deref();
        if let Some(build_info_path) = get_build_info_path(tsconfig_path_ref, &resolved, &base_dir)
        {
            // Build BuildInfo from the cache (which has been updated by collect_diagnostics)
            // If local_cache exists (from BuildInfo), use it; otherwise create minimal info
            let mut build_info = if let Some(ref lc) = local_cache {
                compilation_cache_to_build_info(lc, &file_paths, &base_dir, &resolved)
            } else {
                // No cache available - create minimal BuildInfo with just file info
                BuildInfo {
                    version: crate::incremental::BUILD_INFO_VERSION.to_string(),
                    compiler_version: env!("CARGO_PKG_VERSION").to_string(),
                    root_files: file_paths
                        .iter()
                        .map(|p| {
                            p.strip_prefix(&base_dir)
                                .unwrap_or(p)
                                .to_string_lossy()
                                .replace('\\', "/")
                        })
                        .collect(),
                    ..Default::default()
                }
            };

            // Set the most recent .d.ts file for cross-project invalidation
            build_info.latest_changed_dts_file = latest_changed_dts_file;

            if let Err(e) = build_info.save(&build_info_path) {
                let build_info_path_text = build_info_path.display().to_string();
                let formatted_error = format_file_write_error_for_diagnostic(&build_info_path, &e);
                diagnostics.push(Diagnostic::from_code(
                    diagnostic_codes::COULD_NOT_WRITE_FILE,
                    "",
                    0,
                    0,
                    &[&build_info_path_text, &formatted_error],
                ));
                tracing::warn!(
                    "Failed to save BuildInfo to {}: {}",
                    build_info_path.display(),
                    e
                );
            } else {
                tracing::info!("Saved BuildInfo to: {}", build_info_path.display());
            }
        }
    }

    if perf_enabled {
        tracing::info!(
            target: "wasm::perf",
            phase = "compile_total",
            ms = compile_start.elapsed().as_secs_f64() * 1000.0,
            files = file_paths.len(),
            libs = lib_files.len(),
            diagnostics = diagnostics.len(),
            emitted = emitted_files.len(),
            no_check = resolved.no_check
        );
    }

    Ok(CompilationResult {
        diagnostics,
        emitted_files,
        files_read,
        file_infos,
        request_cache_counters: collected.request_cache_counters,
        interned_types_count: program.type_interner.len(),
        interner_estimated_bytes: program.type_interner.estimated_size_bytes(),
        query_cache_stats: collected.query_cache_stats,
        def_store_stats: collected.def_store_stats,
        phase_timings: PhaseTimings {
            io_read_ms: io_read_duration.as_secs_f64() * 1000.0,
            load_libs_ms: load_libs_duration.as_secs_f64() * 1000.0,
            parse_bind_ms: parse_bind_duration.as_secs_f64() * 1000.0,
            check_ms: check_duration.as_secs_f64() * 1000.0,
            emit_ms: emit_duration.as_secs_f64() * 1000.0,
            total_ms: compile_start.elapsed().as_secs_f64() * 1000.0,
        },
        residency_stats: Some(program.residency_stats()),
        module_dep_stats: collected.module_dep_stats,
        invalidation_summaries: Vec::new(),
    })
}

fn normalize_ts2883_diagnostics_in_place(
    diagnostics: &mut Vec<tsz_common::diagnostics::Diagnostic>,
) {
    use rustc_hash::FxHashSet;

    let mut canonical_sites = FxHashSet::default();
    let mut exact_seen = FxHashSet::default();
    let mut unique = Vec::with_capacity(diagnostics.len());

    for diagnostic in diagnostics.drain(..) {
        let exact_key = (
            diagnostic.code,
            diagnostic.file.clone(),
            diagnostic.start,
            diagnostic.length,
            diagnostic.message_text.clone(),
        );
        if !exact_seen.insert(exact_key) {
            continue;
        }

        if diagnostic.code == 2883
            && let Some((first, second)) =
                parse_ts2883_named_reference_message(&diagnostic.message_text)
            && !looks_like_module_path(&first)
            && looks_like_module_path(&second)
        {
            canonical_sites.insert((diagnostic.file.clone(), diagnostic.start, diagnostic.length));
        }

        unique.push(diagnostic);
    }

    *diagnostics = unique
        .into_iter()
        .filter(|diagnostic| {
            if diagnostic.code != 2883 {
                return true;
            }

            let Some((first, second)) =
                parse_ts2883_named_reference_message(&diagnostic.message_text)
            else {
                return true;
            };

            if !looks_like_module_path(&first) || looks_like_module_path(&second) {
                return true;
            }

            !canonical_sites.contains(&(
                diagnostic.file.clone(),
                diagnostic.start,
                diagnostic.length,
            ))
        })
        .collect();
}

fn parse_ts2883_named_reference_message(message: &str) -> Option<(String, String)> {
    let prefix = "cannot be named without a reference to '";
    let start = message.find(prefix)? + prefix.len();
    let rest = &message[start..];
    let (first, tail) = rest.split_once("' from '")?;
    let (second, _) = tail.split_once('\'')?;
    Some((first.to_string(), second.to_string()))
}

fn looks_like_module_path(text: &str) -> bool {
    text.starts_with('.')
        || text.starts_with('/')
        || text.contains('/')
        || text.contains('\\')
        || text.contains("node_modules")
}

fn config_error_result(file_path: Option<&Path>, message: String, code: u32) -> CompilationResult {
    let file = file_path
        .map(|p| p.display().to_string())
        .unwrap_or_default();
    CompilationResult {
        diagnostics: vec![Diagnostic::error(file, 0, 0, message, code)],
        emitted_files: Vec::new(),
        files_read: Vec::new(),
        file_infos: Vec::new(),
        request_cache_counters: tsz::checker::context::RequestCacheCounters::default(),
        interned_types_count: 0,
        interner_estimated_bytes: 0,
        query_cache_stats: None,
        def_store_stats: None,
        phase_timings: PhaseTimings::default(),
        residency_stats: None,
        module_dep_stats: None,
        invalidation_summaries: Vec::new(),
    }
}

pub(super) fn no_input_diagnostics_for_config(
    mut config_diagnostics: Vec<Diagnostic>,
    tsconfig_path: Option<&Path>,
    include: Option<&[String]>,
    exclude: Option<&[String]>,
    allow_js: bool,
) -> Vec<Diagnostic> {
    // Emit TS18003: No inputs were found in config file.
    // Match tsc: use the resolved config path shown to the compiler.
    let config_name = tsconfig_path
        .map(|path| canonicalize_or_owned(path).to_string_lossy().to_string())
        .unwrap_or_else(|| "tsconfig.json".to_string());
    let include_str = match include {
        Some(v) if !v.is_empty() => v
            .iter()
            .map(|s| format!("\"{s}\""))
            .collect::<Vec<_>>()
            .join(","),
        Some(_) => String::new(),
        None => default_include_patterns(allow_js, false)
            .into_iter()
            .map(|s| format!("\"{s}\""))
            .collect::<Vec<_>>()
            .join(","),
    };
    let exclude_str = exclude
        .filter(|v| !v.is_empty())
        .map(|v| {
            v.iter()
                .map(|s| format!("\"{s}\""))
                .collect::<Vec<_>>()
                .join(",")
        })
        .unwrap_or_default();
    let message = format!(
        "No inputs were found in config file '{config_name}'. Specified 'include' paths were '[{include_str}]' and 'exclude' paths were '[{exclude_str}]'."
    );
    // tsc emits TS18003 without file position (file="", pos=0).
    config_diagnostics.push(Diagnostic::error(String::new(), 0, 0, message, 18003));
    config_diagnostics
}

#[cfg(test)]
fn check_module_resolution_compatibility_mut(
    resolved: &ResolvedCompilerOptions,
    tsconfig_path: Option<&Path>,
    diagnostics: &mut Vec<Diagnostic>,
) -> bool {
    if let Some(diag) = check_module_resolution_compatibility(resolved, tsconfig_path) {
        diagnostics.push(diag);
        true
    } else {
        false
    }
}

#[cfg(test)]
fn check_module_resolution_compatibility(
    resolved: &ResolvedCompilerOptions,
    tsconfig_path: Option<&Path>,
) -> Option<Diagnostic> {
    use tsz::checker::diagnostics::{diagnostic_messages, format_message};
    use tsz::config::ModuleResolutionKind;

    let module_resolution = resolved.module_resolution?;
    // Only check when moduleResolution is Node16 or NodeNext
    let is_node_resolution = matches!(
        module_resolution,
        ModuleResolutionKind::Node16 | ModuleResolutionKind::NodeNext
    );
    if !is_node_resolution {
        return None;
    }

    // tsc accepts any module in the Node16..NodeNext range with node-style resolution
    if resolved.printer.module.is_node_module() {
        return None;
    }

    // Determine the name to display in the diagnostic message
    let resolution_str = match module_resolution {
        ModuleResolutionKind::NodeNext => "NodeNext",
        _ => "Node16",
    };
    let required_str = resolution_str;

    let message = format_message(
        diagnostic_messages::OPTION_MODULE_MUST_BE_SET_TO_WHEN_OPTION_MODULERESOLUTION_IS_SET_TO,
        &[required_str, resolution_str],
    );
    let file = tsconfig_path
        .map(|p| p.display().to_string())
        .unwrap_or_default();
    Some(Diagnostic::error(
        file,
        0,
        0,
        message,
        diagnostic_codes::OPTION_MODULE_MUST_BE_SET_TO_WHEN_OPTION_MODULERESOLUTION_IS_SET_TO,
    ))
}

/// Build file info with inclusion reasons
fn build_file_infos(
    sources: &[SourceEntry],
    root_file_paths: &[PathBuf],
    args: &CliArgs,
    config: Option<&crate::config::TsConfig>,
    _base_dir: &Path,
) -> Vec<FileInfo> {
    let root_set: FxHashSet<_> = root_file_paths.iter().collect();
    let cli_files: FxHashSet<_> = args.files.iter().collect();

    // Get include patterns if available
    let include_patterns = config
        .and_then(|c| c.include.as_ref())
        .map_or_else(|| "**/*".to_string(), |patterns| patterns.join(", "));

    sources
        .iter()
        .map(|source| {
            let mut reasons = Vec::new();

            // Check if it's a CLI-specified file
            if cli_files.iter().any(|f| source.path.ends_with(f)) {
                reasons.push(FileInclusionReason::RootFile);
            }
            // Check if it's a lib file (based on filename pattern)
            else if is_lib_file(&source.path) {
                reasons.push(FileInclusionReason::LibFile);
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

/// Check if a file is a TypeScript library file
fn is_lib_file(path: &Path) -> bool {
    let file_name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");

    (file_name.starts_with("lib.") && file_name.ends_with(".d.ts"))
        || path
            .to_string_lossy()
            .contains("/node_modules/@typescript/lib-")
}

fn resolve_effective_lib_paths(
    resolved: &ResolvedCompilerOptions,
    sources: &[SourceEntry],
    base_dir: &Path,
    disable_default_libs: bool,
) -> Result<Vec<PathBuf>> {
    let include_config_libs =
        !(resolved.checker.no_lib || (resolved.lib_is_default && disable_default_libs));
    let mut lib_names = if include_config_libs {
        lib_names_from_paths(&resolved.lib_files)
    } else {
        Vec::new()
    };

    // When --noLib is set, ignore /// <reference lib="..." /> directives.
    // tsc skips lib reference resolution entirely when noLib is enabled.
    if !resolved.checker.no_lib {
        let source_reference_libs = collect_source_reference_libs(sources);
        if !source_reference_libs.is_empty() {
            let expanded_source_paths =
                resolve_lib_files_with_options(&source_reference_libs, true)?;
            append_unique_lib_names(&mut lib_names, lib_names_from_paths(&expanded_source_paths));
        }
    }

    let mut lib_paths = Vec::with_capacity(lib_names.len());
    let mut seen = FxHashSet::default();
    for lib_name in lib_names {
        let Some(path) = resolve_compiler_lib_path(&lib_name, resolved, base_dir)? else {
            continue;
        };
        let canonical = canonicalize_or_owned(&path);
        if seen.insert(canonical.clone()) {
            lib_paths.push(canonical);
        }
    }
    Ok(lib_paths)
}

fn collect_source_reference_libs(sources: &[SourceEntry]) -> Vec<String> {
    let mut lib_names = Vec::new();
    for source in sources {
        let refs = if let Some(text) = source.text.as_deref() {
            tsz::config::extract_lib_references(text)
        } else {
            std::fs::read_to_string(&source.path)
                .map(|text| tsz::config::extract_lib_references(&text))
                .unwrap_or_default()
        };
        append_unique_lib_names(&mut lib_names, refs);
    }
    lib_names
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
    program: MergedProgram,
    dirty_paths: FxHashSet<PathBuf>,
}

fn build_program_with_cache(
    sources: Vec<SourceEntry>,
    cache: &mut CompilationCache,
    lib_files: &[Arc<LibFile>],
) -> BuildProgramResult {
    let mut meta = Vec::with_capacity(sources.len());
    let mut to_parse = Vec::new();
    let mut dirty_paths = FxHashSet::default();

    for source in sources {
        let file_name = source.path.to_string_lossy().into_owned();
        let (hash, cached_ok) = match source.text {
            Some(text) => {
                let hash = hash_text(&text);
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

    let parsed_results = if to_parse.is_empty() {
        Vec::new()
    } else {
        // Use parse_and_bind_parallel_with_libs to load prebound lib symbols
        // This ensures global symbols like console, Array, Promise are available
        // during binding, which prevents "Any poisoning" where unresolved symbols
        // default to Any type instead of emitting TS2304 errors.
        parallel::parse_and_bind_parallel_with_libs(to_parse, lib_files)
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
                    scopes: Vec::new(),
                    node_scope_ids: Default::default(),
                    parse_diagnostics: Vec::new(),
                    shorthand_ambient_modules: Default::default(),
                    global_augmentations: Default::default(),
                    module_augmentations: Default::default(),
                    augmentation_target_modules: Default::default(),
                    reexports: Default::default(),
                    wildcard_reexports: Default::default(),
                    wildcard_reexports_type_only: Default::default(),
                    lib_binders: Vec::new(),
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

    let mut ordered = Vec::with_capacity(meta.len());
    for entry in &meta {
        let Some(cached) = cache.bind_cache.get(&entry.path) else {
            continue;
        };
        ordered.push(&cached.bind_result);
    }

    BuildProgramResult {
        program: parallel::merge_bind_results_ref(&ordered),
        dirty_paths,
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

fn hash_text(text: &str) -> u64 {
    let mut hasher = FxHasher::default();
    text.hash(&mut hasher);
    hasher.finish()
}

#[path = "sources.rs"]
mod sources;
#[cfg(test)]
pub(crate) use sources::has_no_types_and_symbols_directive;
pub use sources::{FileReadResult, read_source_file};
use sources::{
    SourceEntry, SourceReadResult, build_discovery_options, collect_type_root_files,
    read_source_files, sources_have_no_default_lib, sources_have_no_types_and_symbols,
};
pub(crate) use sources::{
    config_base_dir, load_config, load_config_with_diagnostics, resolve_tsconfig_path,
};

#[path = "check.rs"]
mod check;
#[path = "check_utils.rs"]
mod check_utils;
use check::{collect_diagnostics, load_lib_files_for_contexts};

pub fn apply_cli_overrides(options: &mut ResolvedCompilerOptions, args: &CliArgs) -> Result<()> {
    if let Some(target) = args.target {
        options.printer.target = target.to_script_target();
        options.checker.target = checker_target_from_emitter(options.printer.target);
    }
    if let Some(module) = args.module {
        options.printer.module = module.to_module_kind();
        options.checker.module = module.to_module_kind();
        options.checker.module_explicitly_set = true;
    }
    if let Some(module_resolution) = args.module_resolution {
        options.module_resolution = Some(module_resolution.to_module_resolution_kind());
    }
    if let Some(resolve_package_json_exports) = args.resolve_package_json_exports {
        options.resolve_package_json_exports = resolve_package_json_exports;
    }
    if let Some(resolve_package_json_imports) = args.resolve_package_json_imports {
        options.resolve_package_json_imports = resolve_package_json_imports;
    }
    if let Some(module_suffixes) = args.module_suffixes.as_ref() {
        options.module_suffixes = module_suffixes.clone();
    }
    if args.resolve_json_module {
        options.resolve_json_module = true;
    }
    if args.allow_arbitrary_extensions {
        options.allow_arbitrary_extensions = true;
    }
    if args.allow_importing_ts_extensions {
        options.allow_importing_ts_extensions = true;
    }
    if let Some(use_define_for_class_fields) = args.use_define_for_class_fields {
        options.printer.use_define_for_class_fields = use_define_for_class_fields;
    } else {
        // Default: true for target >= ES2022, false otherwise (matches tsc behavior)
        options.printer.use_define_for_class_fields =
            (options.printer.target as u32) >= (tsz::emitter::ScriptTarget::ES2022 as u32);
    }
    if args.rewrite_relative_import_extensions {
        options.rewrite_relative_import_extensions = true;
        options.printer.rewrite_relative_import_extensions = true;
    }
    if let Some(custom_conditions) = args.custom_conditions.as_ref() {
        options.custom_conditions = custom_conditions.clone();
    }
    if let Some(out_dir) = args.out_dir.as_ref() {
        options.out_dir = Some(out_dir.clone());
    }
    if let Some(root_dir) = args.root_dir.as_ref() {
        options.root_dir = Some(root_dir.clone());
    }
    if args.composite {
        options.composite = true;
        // composite implies declaration and incremental
        options.emit_declarations = true;
        options.incremental = true;
    }
    if args.declaration {
        options.emit_declarations = true;
    }
    if args.declaration_map {
        options.declaration_map = true;
    }
    if args.source_map {
        options.source_map = true;
    }
    if let Some(out_file) = args.out_file.as_ref() {
        options.out_file = Some(out_file.clone());
    }
    if let Some(ts_build_info_file) = args.ts_build_info_file.as_ref() {
        options.ts_build_info_file = Some(ts_build_info_file.clone());
    }
    if args.incremental {
        options.incremental = true;
    }
    if args.import_helpers {
        options.import_helpers = true;
        options.printer.import_helpers = true;
        // importHelpers means "import from tslib" — suppress inline helper emission
        options.printer.no_emit_helpers = true;
    }
    if args.strict {
        options.checker.strict = true;
        // Expand --strict to individual flags (matching TypeScript behavior)
        options.checker.no_implicit_any = true;
        options.checker.no_implicit_returns = true;
        options.checker.strict_null_checks = true;
        options.checker.strict_function_types = true;
        options.checker.strict_bind_call_apply = true;
        options.checker.strict_property_initialization = true;
        options.checker.no_implicit_this = true;
        options.checker.use_unknown_in_catch_variables = true;
        options.checker.always_strict = true;
        options.printer.always_strict = true;
    }
    // Individual strict flag overrides (must come after --strict expansion)
    if let Some(val) = args.strict_null_checks {
        options.checker.strict_null_checks = val;
    }
    if let Some(val) = args.strict_function_types {
        options.checker.strict_function_types = val;
    }
    if let Some(val) = args.strict_property_initialization {
        options.checker.strict_property_initialization = val;
    }
    if let Some(val) = args.strict_bind_call_apply {
        options.checker.strict_bind_call_apply = val;
    }
    if let Some(val) = args.no_implicit_this {
        options.checker.no_implicit_this = val;
    }
    if let Some(val) = args.no_implicit_any {
        options.checker.no_implicit_any = val;
    }
    if let Some(val) = args.use_unknown_in_catch_variables {
        options.checker.use_unknown_in_catch_variables = val;
    }
    if args.no_unchecked_indexed_access {
        options.checker.no_unchecked_indexed_access = true;
    }
    if args.no_property_access_from_index_signature {
        options.checker.no_property_access_from_index_signature = true;
    }
    if args.no_implicit_returns {
        options.checker.no_implicit_returns = true;
    }
    if let Some(val) = args.always_strict {
        options.checker.always_strict = val;
        options.printer.always_strict = val;
    }
    if let Some(val) = args.allow_unreachable_code {
        options.checker.allow_unreachable_code = Some(val);
    }
    if let Some(val) = args.allow_unused_labels {
        options.checker.allow_unused_labels = Some(val);
    }
    if args.sound {
        options.checker.sound_mode = true;
    }
    if args.experimental_decorators {
        options.checker.experimental_decorators = true;
        options.printer.legacy_decorators = true;
    }
    if args.emit_decorator_metadata {
        options.printer.emit_decorator_metadata = true;
    }
    // Pass strictNullChecks to printer for metadata union serialization.
    // Only set to true when explicitly enabled via --strict or --strictNullChecks true.
    // The printer default is false (unlike CheckerOptions which defaults to true).
    if args.strict {
        options.printer.strict_null_checks = true;
    }
    if let Some(val) = args.strict_null_checks {
        options.printer.strict_null_checks = val;
    }
    if args.no_unused_locals {
        options.checker.no_unused_locals = true;
    }
    if args.no_unused_parameters {
        options.checker.no_unused_parameters = true;
    }
    if args.no_implicit_override {
        options.checker.no_implicit_override = true;
    }
    if args.no_implicit_use_strict {
        options.checker.no_implicit_use_strict = true;
    }
    if args.es_module_interop {
        options.es_module_interop = true;
        options.checker.es_module_interop = true;
        options.printer.es_module_interop = true;
        // esModuleInterop implies allowSyntheticDefaultImports
        options.allow_synthetic_default_imports = true;
        options.checker.allow_synthetic_default_imports = true;
    }
    if args.no_emit {
        options.no_emit = true;
    }
    if args.no_emit_on_error {
        options.no_emit_on_error = true;
    }
    if args.no_resolve {
        options.no_resolve = true;
        options.checker.no_resolve = true;
    }
    if args.preserve_symlinks {
        options.preserve_symlinks = true;
    }
    if args.no_check {
        options.no_check = true;
    }
    if args.skip_lib_check {
        options.skip_lib_check = true;
    }
    if args.allow_js {
        options.allow_js = true;
        options.checker.allow_js = true;
    }
    if args.check_js {
        options.check_js = true;
        options.checker.check_js = true;
    }
    if let Some(depth) = args.max_node_module_js_depth {
        options.max_node_module_js_depth = depth;
    }
    if args.isolated_declarations {
        options.isolated_declarations = true;
        options.checker.isolated_declarations = true;
    }
    if let Some(version) = args.types_versions_compiler_version.as_ref() {
        options.types_versions_compiler_version = Some(version.clone());
    } else if let Some(version) = types_versions_compiler_version_env() {
        let version = version.trim();
        if !version.is_empty() {
            options.types_versions_compiler_version = Some(version.to_string());
        }
    }
    if let Some(lib_list) = args.lib.as_ref() {
        options.lib_files = resolve_lib_files(lib_list)?;
        options.lib_is_default = false;
    }
    if args.lib_replacement {
        options.lib_replacement = true;
    }
    if args.no_lib {
        options.checker.no_lib = true;
        options.lib_files.clear();
        options.lib_is_default = false;
    }
    if args.downlevel_iteration {
        options.printer.downlevel_iteration = true;
    }
    if args.no_emit_helpers {
        options.printer.no_emit_helpers = true;
    }
    // Implement tsc's getEmitModuleDetectionKind for CLI overrides:
    // - Explicit "force" -> all non-declaration files are modules
    // - Explicit "auto"/"legacy" -> override config default (may undo Node16+ auto-force)
    // - Not set -> preserve config-level default
    match args.module_detection {
        Some(ModuleDetection::Force) => {
            options.printer.module_detection_force = true;
        }
        Some(ModuleDetection::Auto | ModuleDetection::Legacy) => {
            // Explicitly opting out of force mode
            options.printer.module_detection_force = false;
        }
        None => {
            // When module detection is not set via CLI, check if the CLI also overrides
            // the module kind. If module is now a node module, apply tsc's default (Force).
            if let Some(ref module_val) = args.module
                && matches!(
                    module_val,
                    Module::Node16 | Module::Node18 | Module::Node20 | Module::NodeNext
                )
            {
                options.printer.module_detection_force = true;
            }
        }
    }
    if args.preserve_const_enums {
        options.printer.preserve_const_enums = true;
    }
    // isolatedModules implies preserveConstEnums: const enums cannot be
    // inlined across file boundaries, so they must be emitted as regular enums.
    // Also disables const enum value inlining at usage sites.
    if args.isolated_modules {
        options.printer.preserve_const_enums = true;
        options.printer.no_const_enum_inlining = true;
        options.checker.isolated_modules = true;
    }
    // verbatimModuleSyntax implies preserveConstEnums (tsc 5.0+): import/export
    // syntax is preserved verbatim, so const enums must be emitted as regular
    // enums rather than erased+inlined.
    if args.verbatim_module_syntax {
        options.printer.preserve_const_enums = true;
        options.printer.no_const_enum_inlining = true;
        options.printer.verbatim_module_syntax = true;
        options.checker.verbatim_module_syntax = true;
    }
    if let Some(jsx) = args.jsx {
        let jsx_emit = match jsx {
            crate::args::JsxEmit::Preserve => crate::config::JsxEmit::Preserve,
            crate::args::JsxEmit::React => crate::config::JsxEmit::React,
            crate::args::JsxEmit::ReactJsx => crate::config::JsxEmit::ReactJsx,
            crate::args::JsxEmit::ReactJsxDev => crate::config::JsxEmit::ReactJsxDev,
            crate::args::JsxEmit::ReactNative => crate::config::JsxEmit::ReactNative,
        };
        options.jsx = Some(jsx_emit);
    }
    if let Some(ref factory) = args.jsx_factory {
        options.checker.jsx_factory = factory.clone();
        options.checker.jsx_factory_from_config = true;
    }
    if let Some(ref frag) = args.jsx_fragment_factory {
        options.checker.jsx_fragment_factory = frag.clone();
        options.checker.jsx_fragment_factory_from_config = true;
    }
    if let Some(ref source) = args.jsx_import_source {
        options.checker.jsx_import_source = source.clone();
    }
    if args.remove_comments {
        options.printer.remove_comments = true;
    }
    if args.strip_internal {
        options.strip_internal = true;
    }
    if args.target.is_some() && options.lib_is_default && !options.checker.no_lib {
        options.lib_files = resolve_default_lib_files(options.printer.target)?;
    }

    // Wire removed-but-honored suppress flags from CLI
    if args.suppress_excess_property_errors {
        options.checker.suppress_excess_property_errors = true;
    }
    if args.suppress_implicit_any_index_errors {
        options.checker.suppress_implicit_any_index_errors = true;
    }

    Ok(())
}

/// Find the most recent .d.ts file from a list of emitted files
/// Returns the relative path (from `base_dir`) as a String, or None if no .d.ts files were found
fn find_latest_dts_file(emitted_files: &[PathBuf], base_dir: &Path) -> Option<String> {
    use std::collections::BTreeMap;

    let mut dts_files_with_times: BTreeMap<std::time::SystemTime, PathBuf> = BTreeMap::new();

    // Filter for .d.ts files and get their modification times
    for path in emitted_files {
        if path.extension().and_then(|s| s.to_str()) == Some("d.ts")
            && let Ok(metadata) = std::fs::metadata(path)
            && let Ok(modified) = metadata.modified()
        {
            dts_files_with_times.insert(modified, path.clone());
        }
    }

    // Get the most recent file (highest time in BTreeMap)
    if let Some((_, latest_path)) = dts_files_with_times.last_key_value() {
        // Convert to relative path from base_dir
        let relative = latest_path
            .strip_prefix(base_dir)
            .unwrap_or(latest_path)
            .to_string_lossy()
            .replace('\\', "/");
        Some(relative)
    } else {
        None
    }
}

#[cfg(test)]
#[path = "tests.rs"]
mod tests;

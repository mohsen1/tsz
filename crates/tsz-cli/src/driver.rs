use anyhow::{Result, bail};
use std::collections::VecDeque;

use rustc_hash::{FxHashMap, FxHashSet};
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Instant;

use crate::args::CliArgs;
use crate::config::{
    ResolvedCompilerOptions, TsConfig, checker_target_from_emitter, load_tsconfig,
    resolve_compiler_options, resolve_default_lib_files, resolve_lib_files,
};
use tsz::binder::BinderOptions;
use tsz::binder::BinderState;
use tsz::binder::{SymbolId, SymbolTable, symbol_flags};
use tsz::checker::TypeCache;
use tsz::checker::context::LibContext;
use tsz::checker::diagnostics::{
    Diagnostic, DiagnosticCategory, DiagnosticRelatedInformation, diagnostic_codes,
    diagnostic_messages, format_message,
};
use tsz::checker::state::CheckerState;
use tsz::lib_loader::LibFile;
use tsz::module_resolver::ModuleResolver;
use tsz::span::Span;
use tsz_binder::state::BinderStateScopeInputs;
// Re-export functions that other modules (e.g. watch) access via `driver::`.
use crate::driver_resolution::{
    EmitOutputsContext, ModuleResolutionCache, canonicalize_or_owned, collect_export_binding_nodes,
    collect_import_bindings, collect_module_specifiers, collect_module_specifiers_from_text,
    collect_star_export_specifiers, collect_type_packages_from_root, default_type_roots,
    emit_outputs, env_flag, normalize_type_roots, resolve_module_specifier,
    resolve_type_package_entry, resolve_type_package_from_roots, write_outputs,
};
pub(crate) use crate::driver_resolution::{
    normalize_base_url, normalize_output_dir, normalize_root_dir,
};
use crate::fs::{FileDiscoveryOptions, discover_ts_files, is_js_file};
use crate::incremental::{BuildInfo, default_build_info_path};
use rustc_hash::FxHasher;
#[cfg(test)]
use std::cell::RefCell;
use tsz::parallel::{self, BindResult, BoundFile, MergedProgram};
use tsz::parser::NodeIndex;
use tsz::parser::ParseDiagnostic;
use tsz::parser::node::{NodeAccess, NodeArena};
use tsz::parser::syntax_kind_ext;
use tsz::scanner::SyntaxKind;
use tsz_solver::{QueryCache, TypeFormatter, TypeId};

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
    /// Type reference from another file
    TypeReference(PathBuf),
    /// Referenced in a /// <reference> directive
    TripleSlashReference(PathBuf),
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
            Self::TypeReference(path) => {
                write!(f, "Type reference from '{}'", path.display())
            }
            Self::TripleSlashReference(path) => {
                write!(f, "Referenced from '{}'", path.display())
            }
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

#[derive(Debug, Clone)]
pub struct CompilationResult {
    pub diagnostics: Vec<Diagnostic>,
    pub emitted_files: Vec<PathBuf>,
    pub files_read: Vec<PathBuf>,
    /// Files with their inclusion reasons (for --explainFiles)
    pub file_infos: Vec<FileInfo>,
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
    let result = compile_inner(args, cwd, Some(cache), Some(&canonical_paths), None, None)?;

    let exports_changed = canonical_paths
        .iter()
        .any(|path| old_hashes.get(path).copied() != cache.export_hashes.get(path).copied());
    if !exports_changed {
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

    cache.invalidate_paths_with_dependents_symbols(canonical_paths);
    compile_inner(
        args,
        cwd,
        Some(cache),
        Some(changed_paths),
        Some(&dependents),
        None,
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

    let cwd = canonicalize_or_owned(cwd);
    let tsconfig_path = if let Some(path) = explicit_config_path {
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
    let config = load_config(tsconfig_path.as_deref())?;

    let mut resolved = resolve_compiler_options(
        config
            .as_ref()
            .and_then(|cfg| cfg.compiler_options.as_ref()),
    )?;
    apply_cli_overrides(&mut resolved, args)?;

    if let Some(diag) = check_module_resolution_compatibility(&resolved, tsconfig_path.as_deref()) {
        return Ok(CompilationResult {
            diagnostics: vec![diag],
            emitted_files: Vec::new(),
            files_read: Vec::new(),
            file_infos: Vec::new(),
        });
    }

    let base_dir = config_base_dir(&cwd, tsconfig_path.as_deref());
    let base_dir = canonicalize_or_owned(&base_dir);
    let root_dir = normalize_root_dir(&base_dir, resolved.root_dir.clone());
    let out_dir = normalize_output_dir(&base_dir, resolved.out_dir.clone());
    let declaration_dir = normalize_output_dir(&base_dir, resolved.declaration_dir.clone());
    let base_url = normalize_base_url(&base_dir, resolved.base_url.clone());
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

    // Track if we should save BuildInfo after successful compilation
    let mut should_save_build_info = false;

    // Local cache for BuildInfo-loaded compilation state
    // Only create when loading from BuildInfo (not when a cache is provided)
    let mut local_cache: Option<CompilationCache> = None;

    // Load BuildInfo if incremental compilation is enabled and no cache was provided
    if cache.is_none() && (resolved.incremental || resolved.ts_build_info_file.is_some()) {
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
    let type_files = collect_type_root_files(&base_dir, &resolved);

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
    if file_paths.is_empty() {
        // Emit TS18003: No inputs were found in config file.
        let config_name = tsconfig_path
            .as_deref()
            .map_or_else(|| "tsconfig.json".to_string(), |p| p.display().to_string());
        let include_str = discovery
            .include
            .as_ref()
            .filter(|v| !v.is_empty())
            .map(|v| {
                v.iter()
                    .map(|s| format!("\"{s}\""))
                    .collect::<Vec<_>>()
                    .join(",")
            })
            .unwrap_or_default();
        let exclude_str = discovery
            .exclude
            .as_ref()
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
        return Ok(CompilationResult {
            diagnostics: vec![Diagnostic::error(config_name, 0, 0, message, 18003)],
            emitted_files: Vec::new(),
            files_read: Vec::new(),
            file_infos: Vec::new(),
        });
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
    } = {
        read_source_files(
            &file_paths,
            &base_dir,
            &resolved,
            effective_cache.as_deref(),
            changed_set.as_ref(),
        )?
    };
    perf_log_phase("read_sources", read_sources_start);

    // Update dependencies in the cache
    if let Some(ref mut c) = effective_cache {
        c.update_dependencies(dependencies);
    }

    // Separate binary files from regular sources - binary files get TS1490
    let mut binary_file_diagnostics: Vec<Diagnostic> = Vec::new();
    let mut binary_file_names: FxHashSet<String> = FxHashSet::default();
    let mut sources: Vec<SourceEntry> = Vec::with_capacity(all_sources.len());
    for source in all_sources {
        if source.is_binary {
            // Emit TS1490 "File appears to be binary." for binary files.
            // Track the file name so we can suppress parser diagnostics
            // (e.g. TS1127 "Invalid character") that cascade from parsing
            // UTF-16/corrupted content as UTF-8.
            let file_name = source.path.to_string_lossy().into_owned();
            binary_file_names.insert(file_name.clone());
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

    // Collect all files that were read (including dependencies) before sources is moved
    let mut files_read: Vec<PathBuf> = sources.iter().map(|s| s.path.clone()).collect();
    files_read.sort();

    // Build file info with inclusion reasons
    let file_infos = build_file_infos(&sources, &file_paths, args, config.as_ref(), &base_dir);

    let disable_default_libs = resolved.lib_is_default && sources_have_no_default_lib(&sources);
    // `@noTypesAndSymbols` in source comments is a conformance-harness directive.
    // It should not change CLI semantic compilation behavior (tsc ignores it when
    // compiling files directly), so keep detection for harness plumbing only.
    let _no_types_and_symbols =
        resolved.checker.no_types_and_symbols || sources_have_no_types_and_symbols(&sources);
    let lib_paths: Vec<PathBuf> =
        if (resolved.checker.no_lib && resolved.lib_is_default) || disable_default_libs {
            Vec::new()
        } else {
            resolved.lib_files.clone()
        };
    let lib_path_refs: Vec<&Path> = lib_paths.iter().map(PathBuf::as_path).collect();
    // Load and bind each lib exactly once, then reuse for:
    // 1) user-file binding (global symbol availability during bind)
    // 2) checker lib contexts (global symbol/type resolution)
    let load_libs_start = Instant::now();
    let lib_files: Vec<Arc<LibFile>> = parallel::load_lib_files_for_binding_strict(&lib_path_refs)?;
    perf_log_phase("load_libs", load_libs_start);

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
    perf_log_phase("build_program", build_program_start);

    // Update import symbol IDs if we have a cache
    if let Some(ref mut c) = effective_cache {
        update_import_symbol_ids(&program, &resolved, &base_dir, c);
    }

    // Load lib files only when type checking is needed (lazy loading for faster startup)
    let build_lib_contexts_start = Instant::now();
    let lib_contexts = if resolved.no_check {
        Vec::new() // Skip lib loading when --noCheck is set
    } else {
        load_lib_files_for_contexts(&lib_files)
    };
    perf_log_phase("build_lib_contexts", build_lib_contexts_start);

    let collect_diagnostics_start = Instant::now();
    let mut diagnostics: Vec<Diagnostic> = collect_diagnostics(
        &program,
        &resolved,
        &base_dir,
        effective_cache,
        &lib_contexts,
    );
    perf_log_phase("collect_diagnostics", collect_diagnostics_start);

    // Get reference to type caches for declaration emit
    // Create a longer-lived empty FxHashMap for the fallback case
    let empty_type_caches = FxHashMap::default();
    let type_caches_ref: &FxHashMap<_, _> = local_cache
        .as_ref()
        .map(|c| &c.type_caches)
        .or_else(|| cache.as_ref().map(|c| &c.type_caches))
        .unwrap_or(&empty_type_caches);
    // For binary files, suppress all diagnostics except TS1490.
    // Parsing UTF-16/corrupted content as UTF-8 produces cascading
    // TS1127 "Invalid character" false positives; TSC only emits TS1490.
    if !binary_file_names.is_empty() {
        diagnostics.retain(|d| !binary_file_names.contains(&d.file));
    }
    diagnostics.extend(binary_file_diagnostics);
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
    let emitted_files = if !should_emit {
        Vec::new()
    } else {
        let outputs = emit_outputs(EmitOutputsContext {
            program: &program,
            options: &resolved,
            base_dir: &base_dir,
            root_dir: root_dir.as_deref(),
            out_dir: out_dir.as_deref(),
            declaration_dir: declaration_dir.as_deref(),
            dirty_paths: dirty_paths.as_ref(),
            type_caches: type_caches_ref,
        })?;
        write_outputs(&outputs)?
    };
    perf_log_phase("emit_outputs", emit_outputs_start);

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
    })
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
    }
}

fn check_module_resolution_compatibility(
    resolved: &ResolvedCompilerOptions,
    tsconfig_path: Option<&Path>,
) -> Option<Diagnostic> {
    use tsz::config::ModuleResolutionKind;
    use tsz_common::common::ModuleKind;

    let module_resolution = resolved.module_resolution?;
    let required = match module_resolution {
        ModuleResolutionKind::Node16 => ModuleKind::Node16,
        ModuleResolutionKind::NodeNext => ModuleKind::NodeNext,
        _ => return None,
    };

    if resolved.printer.module == required {
        return None;
    }

    let required_str = match required {
        ModuleKind::NodeNext => "NodeNext",
        _ => "Node16",
    };
    let resolution_str = match module_resolution {
        ModuleResolutionKind::NodeNext => "NodeNext",
        _ => "Node16",
    };

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

    file_name.starts_with("lib.") && file_name.ends_with(".d.ts")
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
                    symbol_arenas: Default::default(),
                    declaration_arenas: Default::default(),
                    scopes: Vec::new(),
                    node_scope_ids: Default::default(),
                    parse_diagnostics: Vec::new(),
                    shorthand_ambient_modules: Default::default(),
                    global_augmentations: Default::default(),
                    module_augmentations: Default::default(),
                    reexports: Default::default(),
                    wildcard_reexports: Default::default(),
                    lib_binders: Vec::new(),
                    lib_symbol_ids: Default::default(),
                    lib_symbol_reverse_remap: Default::default(),
                    flow_nodes: Default::default(),
                    node_flow: Default::default(),
                    switch_clause_to_switch: Default::default(),
                    is_external_module: false, // Default to false for missing files
                    expando_properties: Default::default(),
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
            let canonical = canonicalize_or_owned(&resolved);
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
            let canonical = canonicalize_or_owned(&resolved);
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
            let canonical = canonicalize_or_owned(&resolved);
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

#[path = "driver_sources.rs"]
mod driver_sources;
pub use driver_sources::{FileReadResult, read_source_file};
use driver_sources::{
    SourceEntry, SourceReadResult, build_discovery_options, collect_type_root_files,
    read_source_files, sources_have_no_default_lib, sources_have_no_types_and_symbols,
};
pub(crate) use driver_sources::{
    config_base_dir, has_no_types_and_symbols_directive, load_config, resolve_tsconfig_path,
};

#[path = "driver_check.rs"]
mod driver_check;
use driver_check::{collect_diagnostics, load_lib_files_for_contexts};

pub fn apply_cli_overrides(options: &mut ResolvedCompilerOptions, args: &CliArgs) -> Result<()> {
    if let Some(target) = args.target {
        options.printer.target = target.to_script_target();
        options.checker.target = checker_target_from_emitter(options.printer.target);
    }
    if let Some(module) = args.module {
        options.printer.module = module.to_module_kind();
        options.checker.module = module.to_module_kind();
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
    if args.sound {
        options.checker.sound_mode = true;
    }
    if args.experimental_decorators {
        options.checker.experimental_decorators = true;
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
    if args.no_emit {
        options.no_emit = true;
    }
    if args.no_resolve {
        options.no_resolve = true;
        options.checker.no_resolve = true;
    }
    if args.no_check {
        options.no_check = true;
    }
    if args.allow_js {
        options.allow_js = true;
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
    if args.target.is_some() && options.lib_is_default && !options.checker.no_lib {
        options.lib_files = resolve_default_lib_files(options.printer.target)?;
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
#[path = "driver_tests.rs"]
mod tests;

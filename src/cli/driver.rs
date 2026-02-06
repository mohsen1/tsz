use anyhow::{Result, bail};
use std::collections::VecDeque;

use rustc_hash::{FxHashMap, FxHashSet};
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::binder::BinderOptions;
use crate::binder::BinderState;
use crate::binder::{SymbolId, SymbolTable, symbol_flags};
use crate::checker::TypeCache;
use crate::checker::context::LibContext;
use crate::checker::state::CheckerState;
use crate::checker::types::diagnostics::{
    Diagnostic, DiagnosticCategory, DiagnosticRelatedInformation, diagnostic_codes,
};
use crate::cli::args::CliArgs;
use crate::cli::config::{
    ResolvedCompilerOptions, TsConfig, checker_target_from_emitter, load_tsconfig,
    resolve_compiler_options, resolve_default_lib_files, resolve_lib_files,
};
use crate::lib_loader::LibFile;
use crate::module_resolver::ModuleResolver;
use crate::span::Span;
// Re-export functions that other modules (e.g. watch) access via `driver::`.
use crate::cli::driver_resolution::{
    ModuleResolutionCache, canonicalize_or_owned, collect_export_binding_nodes,
    collect_import_bindings, collect_module_specifiers, collect_module_specifiers_from_text,
    collect_star_export_specifiers, collect_type_packages_from_root, default_type_roots,
    emit_outputs, env_flag, normalize_type_roots, resolve_module_specifier,
    resolve_type_package_entry, resolve_type_package_from_roots, write_outputs,
};
pub(crate) use crate::cli::driver_resolution::{
    normalize_base_url, normalize_output_dir, normalize_root_dir,
};
use crate::cli::fs::{FileDiscoveryOptions, discover_ts_files};
use crate::cli::incremental::{BuildInfo, default_build_info_path};
use crate::parallel::{self, BindResult, BoundFile, MergedProgram};
use crate::parser::NodeIndex;
use crate::parser::ParseDiagnostic;
use crate::parser::node::{NodeAccess, NodeArena};
use crate::parser::syntax_kind_ext;
use crate::solver::{TypeFormatter, TypeId};
use rustc_hash::FxHasher;

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
            FileInclusionReason::RootFile => write!(f, "Root file specified"),
            FileInclusionReason::IncludePattern(pattern) => {
                write!(f, "Matched by include pattern '{}'", pattern)
            }
            FileInclusionReason::ImportedFrom(path) => {
                write!(f, "Imported from '{}'", path.display())
            }
            FileInclusionReason::LibFile => write!(f, "Library file"),
            FileInclusionReason::TypeReference(path) => {
                write!(f, "Type reference from '{}'", path.display())
            }
            FileInclusionReason::TripleSlashReference(path) => {
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
    pub(crate) fn invalidate_paths_with_dependents<I>(&mut self, paths: I)
    where
        I: IntoIterator<Item = PathBuf>,
    {
        let affected = self.collect_dependents(paths);
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
                let has_star_export = self
                    .star_export_dependencies
                    .get(&path)
                    .map(|deps| {
                        changed
                            .iter()
                            .any(|changed_path| deps.contains(changed_path))
                    })
                    .unwrap_or(false);
                if has_star_export {
                    if let Some(cache) = self.type_caches.get_mut(&path) {
                        cache.node_types.clear();
                        cache.relation_cache.clear();
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
    #[allow(dead_code)]
    pub(crate) fn export_hash(&self, path: &Path) -> Option<u64> {
        self.export_hashes.get(path).copied()
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

/// Convert CompilationCache to BuildInfo for persistence
fn compilation_cache_to_build_info(
    cache: &CompilationCache,
    root_files: &[PathBuf],
    base_dir: &Path,
    options: &ResolvedCompilerOptions,
) -> BuildInfo {
    use crate::cli::incremental::{
        BuildInfoOptions, CachedDiagnostic, CachedRelatedInformation, EmitSignature,
        FileInfo as IncrementalFileInfo,
    };
    use std::collections::BTreeMap;

    let mut file_infos = BTreeMap::new();
    let mut dependencies = BTreeMap::new();
    let mut emit_signatures = BTreeMap::new();

    // Convert each file's cache entry to BuildInfo format
    for (path, hash) in &cache.export_hashes {
        let relative_path = path
            .strip_prefix(base_dir)
            .unwrap_or(path)
            .to_string_lossy()
            .replace('\\', "/");

        // Create file info with version (hash) and signature
        let version = format!("{:016x}", hash);
        let signature = Some(format!("{:016x}", hash));
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
                        .to_string()
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
        let relative_path = path
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
                        .replace('\\', "/")
                        .to_string(),
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
                                    .replace('\\', "/")
                                    .to_string(),
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
                .to_string()
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
        version: crate::cli::incremental::BUILD_INFO_VERSION.to_string(),
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

/// Load BuildInfo and create an initial CompilationCache from it
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
    let cwd = canonicalize_or_owned(cwd);
    let tsconfig_path = if let Some(path) = explicit_config_path {
        Some(path.to_path_buf())
    } else {
        resolve_tsconfig_path(&cwd, args.project.as_deref())?
    };
    let config = load_config(tsconfig_path.as_deref())?;

    let mut resolved = resolve_compiler_options(
        config
            .as_ref()
            .and_then(|cfg| cfg.compiler_options.as_ref()),
    )?;
    apply_cli_overrides(&mut resolved, args)?;

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
            .map(|p| p.display().to_string())
            .unwrap_or_else(|| "tsconfig.json".to_string());
        let include_str = discovery
            .include
            .as_ref()
            .filter(|v| !v.is_empty())
            .map(|v| {
                v.iter()
                    .map(|s| format!("\"{}\"", s))
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
                    .map(|s| format!("\"{}\"", s))
                    .collect::<Vec<_>>()
                    .join(",")
            })
            .unwrap_or_default();
        let message = format!(
            "No inputs were found in config file '{}'. Specified 'include' paths were '[{}]' and 'exclude' paths were '[{}]'.",
            config_name, include_str, exclude_str
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
    let mut local_cache_ref = local_cache.as_mut();
    let mut effective_cache = local_cache_ref.as_deref_mut().or(cache.as_deref_mut());

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

    // Update dependencies in the cache
    if let Some(ref mut c) = effective_cache {
        c.update_dependencies(dependencies);
    }

    // Separate binary files from regular sources - binary files get TS1490
    let mut binary_file_diagnostics: Vec<Diagnostic> = Vec::new();
    let mut sources: Vec<SourceEntry> = Vec::with_capacity(all_sources.len());
    for source in all_sources {
        if source.is_binary {
            // Emit TS1490 "File appears to be binary." for binary files
            binary_file_diagnostics.push(Diagnostic::error(
                source.path.to_string_lossy().into_owned(),
                0,
                0,
                "File appears to be binary.".to_string(),
                diagnostic_codes::FILE_APPEARS_TO_BE_BINARY,
            ));
        } else {
            sources.push(source);
        }
    }

    // Collect all files that were read (including dependencies) before sources is moved
    let mut files_read: Vec<PathBuf> = sources.iter().map(|s| s.path.clone()).collect();
    files_read.sort();

    // Build file info with inclusion reasons
    let file_infos = build_file_infos(&sources, &file_paths, args, config.as_ref(), &base_dir);

    let disable_default_libs = resolved.lib_is_default && sources_have_no_default_lib(&sources);
    let no_types_and_symbols = sources_have_no_types_and_symbols(&sources);
    let lib_paths: Vec<PathBuf> =
        if resolved.checker.no_lib || disable_default_libs || no_types_and_symbols {
            Vec::new()
        } else {
            resolved.lib_files.clone()
        };
    let lib_path_refs: Vec<&Path> = lib_paths.iter().map(PathBuf::as_path).collect();
    // Load and bind each lib exactly once, then reuse for:
    // 1) user-file binding (global symbol availability during bind)
    // 2) checker lib contexts (global symbol/type resolution)
    let lib_files: Vec<Arc<LibFile>> = parallel::load_lib_files_for_binding(&lib_path_refs);

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

    // Update import symbol IDs if we have a cache
    if let Some(ref mut c) = effective_cache {
        update_import_symbol_ids(&program, &resolved, &base_dir, c);
    }

    // Load lib files only when type checking is needed (lazy loading for faster startup)
    let lib_contexts = if resolved.no_check {
        Vec::new() // Skip lib loading when --noCheck is set
    } else {
        load_lib_files_for_contexts(&lib_files)
    };

    let mut diagnostics = collect_diagnostics(
        &program,
        &resolved,
        &base_dir,
        effective_cache,
        &lib_contexts,
    );

    // Get reference to type caches for declaration emit
    // Create a longer-lived empty FxHashMap for the fallback case
    let empty_type_caches = FxHashMap::default();
    let type_caches_ref: &FxHashMap<_, _> = local_cache
        .as_ref()
        .map(|c| &c.type_caches)
        .or_else(|| cache.as_ref().map(|c| &c.type_caches))
        .unwrap_or(&empty_type_caches);
    // Add TS1490 diagnostics for binary files
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
                dirty_paths = Some(forced.iter().cloned().collect());
            }
        }
    }

    let emitted_files = if !should_emit {
        Vec::new()
    } else {
        let outputs = emit_outputs(
            &program,
            &resolved,
            &base_dir,
            root_dir.as_deref(),
            out_dir.as_deref(),
            declaration_dir.as_deref(),
            dirty_paths.as_ref(),
            type_caches_ref,
        )?;
        write_outputs(&outputs)?
    };

    // Find the most recent .d.ts file for BuildInfo tracking
    let latest_changed_dts_file = if !emitted_files.is_empty() {
        find_latest_dts_file(&emitted_files, &base_dir)
    } else {
        None
    };

    // Save BuildInfo if incremental compilation is enabled
    if should_save_build_info && has_error == false {
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
                    version: crate::cli::incremental::BUILD_INFO_VERSION.to_string(),
                    compiler_version: env!("CARGO_PKG_VERSION").to_string(),
                    root_files: file_paths
                        .iter()
                        .map(|p| {
                            p.strip_prefix(&base_dir)
                                .unwrap_or(p)
                                .to_string_lossy()
                                .replace('\\', "/")
                                .to_string()
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

    Ok(CompilationResult {
        diagnostics,
        emitted_files,
        files_read,
        file_infos,
    })
}

/// Build file info with inclusion reasons
fn build_file_infos(
    sources: &[SourceEntry],
    root_file_paths: &[PathBuf],
    args: &CliArgs,
    config: Option<&crate::cli::config::TsConfig>,
    _base_dir: &Path,
) -> Vec<FileInfo> {
    let root_set: FxHashSet<_> = root_file_paths.iter().collect();
    let cli_files: FxHashSet<_> = args.files.iter().collect();

    // Get include patterns if available
    let include_patterns = config
        .and_then(|c| c.include.as_ref())
        .map(|patterns| patterns.join(", "))
        .unwrap_or_else(|| "**/*".to_string());

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
                    .map(|entry| entry.hash == hash)
                    .unwrap_or(false);
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
                    flow_nodes: Default::default(),
                    node_flow: Default::default(),
                    switch_clause_to_switch: Default::default(),
                    is_external_module: false, // Default to false for missing files
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

/// Result of reading a source file - either valid text or binary/unreadable
#[derive(Debug, Clone)]
pub enum FileReadResult {
    /// File was successfully read as UTF-8 text
    Text(String),
    /// File appears to be binary (emit TS1490)
    Binary,
    /// File could not be read (I/O error)
    Error(String),
}

/// Read a source file, detecting binary files that should emit TS1490.
///
/// TypeScript detects binary files by checking for:
/// - UTF-16 BOM (FE FF for BE, FF FE for LE)  
/// - Non-valid UTF-8 sequences
/// - Files with many null bytes
pub fn read_source_file(path: &Path) -> FileReadResult {
    // Read as bytes first
    let bytes = match std::fs::read(path) {
        Ok(b) => b,
        Err(e) => return FileReadResult::Error(e.to_string()),
    };

    // Check for binary indicators
    if is_binary_file(&bytes) {
        return FileReadResult::Binary;
    }

    // Try to decode as UTF-8
    match String::from_utf8(bytes) {
        Ok(text) => FileReadResult::Text(text),
        Err(_) => FileReadResult::Binary, // Invalid UTF-8 = treat as binary
    }
}

/// Check if file content appears to be binary (not valid source code).
///
/// Matches TypeScript's binary detection:
/// - UTF-16 BOM at start
/// - Many consecutive null bytes (embedded binaries, corrupted files)
fn is_binary_file(bytes: &[u8]) -> bool {
    if bytes.is_empty() {
        return false;
    }

    // Check for UTF-16 BOM
    // UTF-16 BE: FE FF
    // UTF-16 LE: FF FE
    if bytes.len() >= 2 {
        if (bytes[0] == 0xFE && bytes[1] == 0xFF) || (bytes[0] == 0xFF && bytes[1] == 0xFE) {
            return true;
        }
    }

    // Check for many null bytes (binary file indicator)
    // TypeScript considers files with many nulls as binary
    let null_count = bytes.iter().take(1024).filter(|&&b| b == 0).count();
    if null_count > 10 {
        return true;
    }

    // Check for consecutive null bytes (UTF-16 or binary)
    // UTF-16 text will have null bytes between ASCII characters
    let mut consecutive_nulls = 0;
    for &byte in bytes.iter().take(512) {
        if byte == 0 {
            consecutive_nulls += 1;
            if consecutive_nulls >= 4 {
                return true;
            }
        } else {
            consecutive_nulls = 0;
        }
    }

    false
}

#[derive(Debug, Clone)]
pub(crate) struct SourceEntry {
    path: PathBuf,
    text: Option<String>,
    /// If true, this file appears to be binary (emit TS1490)
    is_binary: bool,
}

fn sources_have_no_default_lib(sources: &[SourceEntry]) -> bool {
    sources.iter().any(source_has_no_default_lib)
}

fn source_has_no_default_lib(source: &SourceEntry) -> bool {
    if let Some(text) = source.text.as_deref() {
        return has_no_default_lib_directive(text);
    }
    let Ok(text) = std::fs::read_to_string(&source.path) else {
        return false;
    };
    has_no_default_lib_directive(&text)
}

fn has_no_default_lib_directive(source: &str) -> bool {
    for line in source.lines() {
        let trimmed = line.trim_start();
        if !trimmed.starts_with("///") {
            if trimmed.is_empty() {
                continue;
            }
            break;
        }
        if let Some(true) = parse_reference_no_default_lib_value(trimmed) {
            return true;
        }
    }
    false
}

fn parse_reference_no_default_lib_value(line: &str) -> Option<bool> {
    let needle = "no-default-lib";
    let lower = line.to_ascii_lowercase();
    let idx = lower.find(needle)?;
    let mut rest = &line[idx + needle.len()..];
    rest = rest.trim_start();
    if !rest.starts_with('=') {
        return None;
    }
    rest = rest[1..].trim_start();
    let quote = rest.as_bytes().first().copied()?;
    if quote != b'"' && quote != b'\'' {
        return None;
    }
    let rest = &rest[1..];
    let end = rest.find(quote as char)?;
    let value = rest[..end].trim();
    match value.to_ascii_lowercase().as_str() {
        "true" => Some(true),
        "false" => Some(false),
        _ => None,
    }
}

/// Check if any source file has @noTypesAndSymbols: true comment
pub(crate) fn sources_have_no_types_and_symbols(sources: &[SourceEntry]) -> bool {
    sources.iter().any(source_has_no_types_and_symbols)
}

pub(crate) fn source_has_no_types_and_symbols(source: &SourceEntry) -> bool {
    if let Some(text) = source.text.as_deref() {
        return has_no_types_and_symbols_directive(text);
    }
    let Ok(text) = std::fs::read_to_string(&source.path) else {
        return false;
    };
    has_no_types_and_symbols_directive(&text)
}

pub(crate) fn has_no_types_and_symbols_directive(source: &str) -> bool {
    // Parse @noTypesAndSymbols from source file comments (first 32 lines)
    for line in source.lines().take(32) {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let is_comment =
            trimmed.starts_with("//") || trimmed.starts_with("/*") || trimmed.starts_with('*');
        if !is_comment {
            break;
        }

        // Check for @noTypesAndSymbols: true pattern
        let lower = trimmed.to_ascii_lowercase();
        if let Some(pos) = lower.find("@notypesandsymbols") {
            let after_key = &lower[pos + "@notypesandsymbols".len()..];
            if let Some(colon_pos) = after_key.find(':') {
                let value = after_key[colon_pos + 1..].trim();
                let value_clean = if let Some(comma_pos) = value.find(',') {
                    &value[..comma_pos]
                } else if let Some(semicolon_pos) = value.find(';') {
                    &value[..semicolon_pos]
                } else {
                    value
                }
                .trim();

                if value_clean == "true" {
                    return true;
                }
            }
        }
    }
    false
}

struct SourceReadResult {
    sources: Vec<SourceEntry>,
    dependencies: FxHashMap<PathBuf, FxHashSet<PathBuf>>,
}

pub(crate) fn find_tsconfig(cwd: &Path) -> Option<PathBuf> {
    let candidate = cwd.join("tsconfig.json");
    if candidate.is_file() {
        Some(canonicalize_or_owned(&candidate))
    } else {
        None
    }
}

pub(crate) fn resolve_tsconfig_path(cwd: &Path, project: Option<&Path>) -> Result<Option<PathBuf>> {
    let Some(project) = project else {
        return Ok(find_tsconfig(cwd));
    };

    let mut candidate = if project.is_absolute() {
        project.to_path_buf()
    } else {
        cwd.join(project)
    };

    if candidate.is_dir() {
        candidate = candidate.join("tsconfig.json");
    }

    if !candidate.exists() {
        bail!("tsconfig not found at {}", candidate.display());
    }

    if !candidate.is_file() {
        bail!("project path is not a file: {}", candidate.display());
    }

    Ok(Some(canonicalize_or_owned(&candidate)))
}

pub(crate) fn load_config(path: Option<&Path>) -> Result<Option<TsConfig>> {
    let Some(path) = path else {
        return Ok(None);
    };

    let config = load_tsconfig(path)?;
    Ok(Some(config))
}

pub(crate) fn config_base_dir(cwd: &Path, tsconfig_path: Option<&Path>) -> PathBuf {
    tsconfig_path
        .and_then(|path| path.parent().map(Path::to_path_buf))
        .unwrap_or_else(|| cwd.to_path_buf())
}

fn build_discovery_options(
    args: &CliArgs,
    base_dir: &Path,
    tsconfig_path: Option<&Path>,
    config: Option<&TsConfig>,
    out_dir: Option<&Path>,
) -> Result<FileDiscoveryOptions> {
    let follow_links = env_flag("TSZ_FOLLOW_SYMLINKS");
    if !args.files.is_empty() {
        return Ok(FileDiscoveryOptions {
            base_dir: base_dir.to_path_buf(),
            files: args.files.clone(),
            include: None,
            exclude: None,
            out_dir: out_dir.map(Path::to_path_buf),
            follow_links,
        });
    }

    let Some(config) = config else {
        bail!("no input files specified and no tsconfig.json found");
    };
    let Some(tsconfig_path) = tsconfig_path else {
        bail!("no tsconfig.json path available");
    };

    let mut options = FileDiscoveryOptions::from_tsconfig(tsconfig_path, config, out_dir);
    options.follow_links = follow_links;
    Ok(options)
}

fn collect_type_root_files(base_dir: &Path, options: &ResolvedCompilerOptions) -> Vec<PathBuf> {
    let roots = match options.type_roots.as_ref() {
        Some(roots) => roots.clone(),
        None => default_type_roots(base_dir),
    };
    if roots.is_empty() {
        return Vec::new();
    }

    let mut files = std::collections::BTreeSet::new();
    if let Some(types) = options.types.as_ref() {
        for name in types {
            if let Some(entry) = resolve_type_package_from_roots(name, &roots, options) {
                files.insert(entry);
            }
        }
        return files.into_iter().collect();
    }

    for root in roots {
        for package_root in collect_type_packages_from_root(&root) {
            if let Some(entry) = resolve_type_package_entry(&package_root, options) {
                files.insert(entry);
            }
        }
    }

    files.into_iter().collect()
}

fn read_source_files(
    paths: &[PathBuf],
    base_dir: &Path,
    options: &ResolvedCompilerOptions,
    cache: Option<&CompilationCache>,
    changed_paths: Option<&FxHashSet<PathBuf>>,
) -> Result<SourceReadResult> {
    let mut sources: FxHashMap<PathBuf, (Option<String>, bool)> = FxHashMap::default(); // (text, is_binary)
    let mut dependencies: FxHashMap<PathBuf, FxHashSet<PathBuf>> = FxHashMap::default();
    let mut seen = FxHashSet::default();
    let mut pending = VecDeque::new();
    let mut resolution_cache = ModuleResolutionCache::default();
    let use_cache = cache.is_some() && changed_paths.is_some();

    for path in paths {
        let canonical = canonicalize_or_owned(path);
        if seen.insert(canonical.clone()) {
            pending.push_back(canonical);
        }
    }

    while let Some(path) = pending.pop_front() {
        // Use cached bind result only when we know the file hasn't changed
        // (changed_paths is provided and this file is not in it)
        if use_cache
            && let Some(cache) = cache
            && let Some(changed_paths) = changed_paths
            && !changed_paths.contains(&path)
            && let (Some(_), Some(cached_deps)) =
                (cache.bind_cache.get(&path), cache.dependencies.get(&path))
        {
            dependencies.insert(path.clone(), cached_deps.clone());
            sources.insert(path.clone(), (None, false)); // Cached files are not binary
            for dep in cached_deps {
                if seen.insert(dep.clone()) {
                    pending.push_back(dep.clone());
                }
            }
            continue;
        }

        // Read file with binary detection
        let (text, is_binary) = match read_source_file(&path) {
            FileReadResult::Text(t) => (t, false),
            FileReadResult::Binary => (String::new(), true), // Binary files get empty content + TS1490
            FileReadResult::Error(e) => {
                return Err(anyhow::anyhow!("failed to read {}: {}", path.display(), e));
            }
        };
        let specifiers = if is_binary {
            vec![] // Don't try to parse module specifiers from binary files
        } else {
            collect_module_specifiers_from_text(&path, &text)
        };
        sources.insert(path.clone(), (Some(text), is_binary));
        let entry = dependencies.entry(path.clone()).or_default();

        for specifier in specifiers {
            if let Some(resolved) = resolve_module_specifier(
                &path,
                &specifier,
                options,
                base_dir,
                &mut resolution_cache,
                &seen,
            ) {
                let canonical = canonicalize_or_owned(&resolved);
                entry.insert(canonical.clone());
                if seen.insert(canonical.clone()) {
                    pending.push_back(canonical);
                }
            }
        }
    }

    let mut list: Vec<SourceEntry> = sources
        .into_iter()
        .map(|(path, (text, is_binary))| SourceEntry {
            path,
            text,
            is_binary,
        })
        .collect();
    list.sort_by(|left, right| {
        left.path
            .to_string_lossy()
            .cmp(&right.path.to_string_lossy())
    });
    Ok(SourceReadResult {
        sources: list,
        dependencies,
    })
}

/// Load lib.d.ts files and create LibContext objects for the checker.
///
/// This function reuses already-loaded lib files from the binding phase, avoiding a second
/// parse/bind pass during checker setup.
fn load_lib_files_for_contexts(lib_files: &[Arc<LibFile>]) -> Vec<LibContext> {
    if lib_files.is_empty() {
        return Vec::new();
    }

    let lib_contexts: Vec<LibContext> = lib_files
        .iter()
        .map(|lib| LibContext {
            arena: Arc::clone(&lib.arena),
            binder: Arc::clone(&lib.binder),
        })
        .collect();

    // Merge all lib binders into a single binder to avoid duplicate SymbolIds
    // (e.g., "Intl" declarations across multiple lib files).
    use crate::binder::state::LibContext as BinderLibContext;
    let mut merged_binder = crate::binder::BinderState::new();
    let binder_lib_contexts: Vec<_> = lib_contexts
        .iter()
        .map(|ctx| BinderLibContext {
            arena: Arc::clone(&ctx.arena),
            binder: Arc::clone(&ctx.binder),
        })
        .collect();
    merged_binder.merge_lib_contexts_into_binder(&binder_lib_contexts);

    vec![LibContext {
        // Keep a lib arena available for declaration lookups.
        arena: Arc::clone(&lib_contexts[0].arena),
        binder: Arc::new(merged_binder),
    }]
}

fn collect_diagnostics(
    program: &MergedProgram,
    options: &ResolvedCompilerOptions,
    base_dir: &Path,
    cache: Option<&mut CompilationCache>,
    lib_contexts: &[LibContext],
) -> Vec<Diagnostic> {
    let _collect_span =
        tracing::info_span!("collect_diagnostics", files = program.files.len()).entered();
    let mut diagnostics = Vec::new();
    let mut used_paths = FxHashSet::default();
    let mut cache = cache;
    let mut resolution_cache = ModuleResolutionCache::default();
    let mut program_paths = FxHashSet::default();
    let mut canonical_to_file_name: FxHashMap<PathBuf, String> = FxHashMap::default();
    let mut canonical_to_file_idx: FxHashMap<PathBuf, usize> = FxHashMap::default();

    {
        let _span = tracing::info_span!("build_program_path_maps").entered();
        for (idx, file) in program.files.iter().enumerate() {
            let canonical = canonicalize_or_owned(Path::new(&file.file_name));
            program_paths.insert(canonical.clone());
            canonical_to_file_name.insert(canonical.clone(), file.file_name.clone());
            canonical_to_file_idx.insert(canonical, idx);
        }
    }

    // Pre-create all binders for cross-file resolution
    let all_binders: Arc<Vec<Arc<BinderState>>> = Arc::new({
        let _span = tracing::info_span!("build_cross_file_binders").entered();
        program
            .files
            .iter()
            .enumerate()
            .map(|(file_idx, file)| {
                Arc::new(create_cross_file_lookup_binder(file, program, file_idx))
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

    // Create ModuleResolver instance for proper error reporting (TS2834, TS2835, TS2792, etc.)
    let mut module_resolver = ModuleResolver::new(options);

    // Build resolved_module_paths map: (source_file_idx, specifier) -> target_file_idx
    // Also build resolved_module_errors map for specific error codes
    let mut resolved_module_paths: FxHashMap<(usize, String), usize> = FxHashMap::default();
    let mut resolved_module_errors: FxHashMap<
        (usize, String),
        crate::checker::context::ResolutionError,
    > = FxHashMap::default();

    {
        let _span = tracing::info_span!("build_resolved_module_maps").entered();
        for (file_idx, file) in program.files.iter().enumerate() {
            let module_specifiers = collect_module_specifiers(&file.arena, file.source_file);
            let file_path = Path::new(&file.file_name);

            for (specifier, specifier_node) in &module_specifiers {
                // Get span from the specifier node
                let span = if let Some(spec_node) = file.arena.get(*specifier_node) {
                    Span::new(spec_node.pos, spec_node.end)
                } else {
                    Span::new(0, 0) // Fallback for invalid nodes
                };

                // Always try ModuleResolver first to get specific error types (TS2834/TS2835/TS2792)
                match module_resolver.resolve(specifier, file_path, span) {
                    Ok(resolved_module) => {
                        let canonical = canonicalize_or_owned(&resolved_module.resolved_path);
                        if let Some(&target_idx) = canonical_to_file_idx.get(&canonical) {
                            resolved_module_paths.insert((file_idx, specifier.clone()), target_idx);
                        }
                    }
                    Err(failure) => {
                        // Check if this is NotFound and the old resolver would find it (virtual test files)
                        // In that case, validate Node16 rules before accepting the fallback
                        if failure.is_not_found() {
                            if let Some(resolved) = resolve_module_specifier(
                                file_path,
                                specifier,
                                options,
                                base_dir,
                                &mut resolution_cache,
                                &program_paths,
                            ) {
                                // Validate Node16/NodeNext extension requirements for virtual files
                                let resolution_kind = options.effective_module_resolution();
                                let is_node16_or_next = matches!(
                                    resolution_kind,
                                    crate::cli::config::ModuleResolutionKind::Node16
                                        | crate::cli::config::ModuleResolutionKind::NodeNext
                                );

                                if is_node16_or_next {
                                    // Check if importing file is ESM (by extension or path)
                                    let file_path_str = file_path.to_string_lossy();
                                    let importing_ext =
                                        crate::module_resolver::ModuleExtension::from_path(
                                            file_path,
                                        );
                                    let is_esm = importing_ext.forces_esm()
                                        || file_path_str.ends_with(".mts")
                                        || file_path_str.ends_with(".mjs");

                                    // Check if specifier has an extension
                                    let specifier_has_extension =
                                        Path::new(specifier).extension().is_some();

                                    // In Node16/NodeNext ESM mode, relative imports must have explicit extensions
                                    // If the import is extensionless, TypeScript treats it as "cannot find module" (TS2307)
                                    // even though the file exists, because ESM requires explicit extensions
                                    if is_esm
                                        && !specifier_has_extension
                                        && specifier.starts_with('.')
                                    {
                                        // Emit TS2307 error - module cannot be found with the exact specifier
                                        // (even though the file exists, ESM requires explicit extension)
                                        resolved_module_errors.insert(
                                            (file_idx, specifier.clone()),
                                            crate::checker::context::ResolutionError {
                                                code: crate::module_resolver::CANNOT_FIND_MODULE,
                                                message: format!(
                                                    "Cannot find module '{}' or its corresponding type declarations.",
                                                    specifier
                                                ),
                                            },
                                        );
                                        continue; // Don't add to resolved_modules - this is an error
                                    }
                                }

                                // Fallback succeeded and passed validation - add to resolved paths
                                let canonical = canonicalize_or_owned(&resolved);
                                if let Some(&target_idx) = canonical_to_file_idx.get(&canonical) {
                                    resolved_module_paths
                                        .insert((file_idx, specifier.clone()), target_idx);
                                }
                                continue; // Virtual file found and validated, skip error
                            }
                        }

                        // Convert ResolutionFailure to Diagnostic to get the error code and message
                        let diagnostic = failure.to_diagnostic();
                        resolved_module_errors.insert(
                            (file_idx, specifier.clone()),
                            crate::checker::context::ResolutionError {
                                code: diagnostic.code,
                                message: diagnostic.message,
                            },
                        );
                    }
                }
            }
        }
    }

    let resolved_module_paths = Arc::new(resolved_module_paths);
    let resolved_module_errors = Arc::new(resolved_module_errors);

    // Create a shared QueryCache for memoized evaluate_type/is_subtype_of calls.
    let query_cache = crate::solver::QueryCache::new(&program.type_interner);

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
            .map(|c| !c.type_caches.contains_key(&file_path))
            .unwrap_or(true); // No cache at all -> check everything

        if needs_check {
            work_queue.push_back(idx);
            checked_files.insert(idx);
        }
    }

    // Process files in the work queue
    while let Some(file_idx) = work_queue.pop_front() {
        let file = &program.files[file_idx];
        let file_path = PathBuf::from(&file.file_name);

        let mut binder = create_binder_from_bound_file(file, program, file_idx);
        let module_specifiers = collect_module_specifiers(&file.arena, file.source_file);

        // Bridge multi-file module resolution for ES module imports.
        //
        // `MergedProgram.module_exports` is keyed by *file paths*, but import symbols store the raw
        // module specifier string (e.g. "./math", "../utils/helpers"). For this file's checker
        // run, add additional `module_exports` entries keyed by the raw specifiers, pointing at
        // the resolved target file's export table.
        for (specifier, _) in &module_specifiers {
            if let Some(resolved) = resolve_module_specifier(
                Path::new(&file.file_name),
                specifier,
                options,
                base_dir,
                &mut resolution_cache,
                &program_paths,
            ) {
                let canonical = canonicalize_or_owned(&resolved);
                if let Some(target_file_name) = canonical_to_file_name.get(&canonical)
                    && let Some(exports) = binder.module_exports.get(target_file_name).cloned()
                {
                    binder.module_exports.insert(specifier.clone(), exports);
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
        // Set lib contexts for global symbol resolution (console, Array, Promise, etc.)
        if !lib_contexts.is_empty() {
            checker.ctx.set_lib_contexts(lib_contexts.to_vec());
            // Set actual lib file count for has_lib_loaded() check
            // This enables proper filtering of lib symbols in check_duplicate_identifiers
            checker.ctx.set_actual_lib_file_count(lib_contexts.len());
        }
        // Set cross-file resolution context for import type resolution
        checker.ctx.set_all_arenas(Arc::clone(&all_arenas));
        checker.ctx.set_all_binders(Arc::clone(&all_binders));
        checker
            .ctx
            .set_resolved_module_paths(Arc::clone(&resolved_module_paths));
        checker
            .ctx
            .set_resolved_module_errors(Arc::clone(&resolved_module_errors));
        checker.ctx.set_current_file_idx(file_idx);
        // Set per-file is_external_module cache to preserve state across file bindings
        checker.ctx.is_external_module_by_file = Some(Arc::clone(&is_external_module_by_file));

        // Build resolved_modules set for backward compatibility
        // Only include modules that were successfully resolved (not in error map)
        let mut resolved_modules = rustc_hash::FxHashSet::default();
        for (specifier, _) in &module_specifiers {
            // Check if this specifier is in resolved_module_paths (successfully resolved)
            if resolved_module_paths.contains_key(&(file_idx, specifier.clone())) {
                resolved_modules.insert(specifier.clone());
            } else if !resolved_module_errors.contains_key(&(file_idx, specifier.clone())) {
                // Not in error map either - might be external module, check with old resolver
                if let Some(resolved) = resolve_module_specifier(
                    Path::new(&file.file_name),
                    specifier,
                    options,
                    base_dir,
                    &mut resolution_cache,
                    &program_paths,
                ) {
                    let canonical = canonicalize_or_owned(&resolved);
                    if program_paths.contains(&canonical) {
                        resolved_modules.insert(specifier.clone());
                    }
                }
            }
        }
        checker.ctx.resolved_modules = Some(resolved_modules);
        let mut file_diagnostics = Vec::new();
        for parse_diagnostic in &file.parse_diagnostics {
            file_diagnostics.push(parse_diagnostic_to_checker(
                &file.file_name,
                parse_diagnostic,
            ));
        }
        // Module resolution errors (TS2307, TS2834, TS2835, TS2792) are now handled by the checker
        // via resolved_module_errors map, so we don't emit them here anymore.
        // Skip full type checking when --noCheck is set; only parse/emit diagnostics are reported.
        if !options.no_check {
            let _check_span = tracing::info_span!("check_file", file = %file.file_name).entered();
            checker.check_source_file(file.source_file);
            file_diagnostics.extend(std::mem::take(&mut checker.ctx.diagnostics));
        }

        // Update the cache and check for export hash changes
        if let Some(c) = cache.as_deref_mut() {
            let new_hash = compute_export_hash(program, file, file_idx, &mut checker);
            let old_hash = c.export_hashes.get(&file_path).copied();

            // Always update cache with new results
            c.type_caches
                .insert(file_path.clone(), checker.extract_cache());
            c.diagnostics
                .insert(file_path.clone(), file_diagnostics.clone());
            c.export_hashes.insert(file_path.clone(), new_hash);

            // If export hash changed (or was missing), invalidate dependents
            if old_hash != Some(new_hash) {
                // Find all files that depend on this file and queue them for checking
                if let Some(dependents) = c.reverse_dependencies.get(&file_path) {
                    for dep_path in dependents {
                        if let Some(&dep_idx) = canonical_to_file_idx.get(dep_path) {
                            // Only add if not already checked (prevent infinite loops)
                            if checked_files.insert(dep_idx) {
                                work_queue.push_back(dep_idx);
                                // Remove stale cache entries for the dependent
                                c.type_caches.remove(dep_path);
                                c.diagnostics.remove(dep_path);
                            }
                        }
                    }
                }
            }
        } else {
            // No cache available, collect diagnostics directly
            diagnostics.extend(file_diagnostics);
        }
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

    diagnostics
}

fn compute_export_hash(
    program: &MergedProgram,
    file: &BoundFile,
    file_idx: usize,
    checker: &mut CheckerState,
) -> u64 {
    let mut formatter = TypeFormatter::with_symbols(&program.type_interner, &program.symbols);
    let mut hasher = FxHasher::default();
    let mut type_str_cache: FxHashMap<TypeId, String> = FxHashMap::default();

    if let Some(file_locals) = program.file_locals.get(file_idx) {
        let mut exports: Vec<(&String, SymbolId)> = file_locals
            .iter()
            .filter_map(|(name, &sym_id)| {
                is_exported_symbol(&program.symbols, sym_id).then_some((name, sym_id))
            })
            .collect();
        exports.sort_by(|left, right| left.0.cmp(right.0));

        for (name, sym_id) in exports {
            name.hash(&mut hasher);
            let type_id = checker.get_type_of_symbol(sym_id);
            let type_str = type_str_cache
                .entry(type_id)
                .or_insert_with(|| formatter.format(type_id));
            type_str.hash(&mut hasher);
        }
    }

    let mut export_signatures = Vec::new();
    collect_export_signatures(file, checker, &mut formatter, &mut export_signatures);
    export_signatures.sort();
    for signature in export_signatures {
        signature.hash(&mut hasher);
    }

    hasher.finish()
}

fn is_exported_symbol(symbols: &crate::binder::SymbolArena, sym_id: SymbolId) -> bool {
    let Some(symbol) = symbols.get(sym_id) else {
        return false;
    };
    symbol.is_exported || (symbol.flags & symbol_flags::EXPORT_VALUE) != 0
}

fn collect_export_signatures(
    file: &BoundFile,
    checker: &mut CheckerState,
    formatter: &mut TypeFormatter,
    signatures: &mut Vec<String>,
) {
    let arena = &file.arena;
    let Some(node) = arena.get(file.source_file) else {
        return;
    };
    let Some(source) = arena.get_source_file(node) else {
        return;
    };

    for &stmt_idx in &source.statements.nodes {
        let Some(stmt) = arena.get(stmt_idx) else {
            continue;
        };

        if let Some(export_decl) = arena.get_export_decl(stmt) {
            if export_decl.is_default_export {
                if let Some(signature) =
                    export_default_signature(export_decl.export_clause, checker, formatter)
                {
                    signatures.push(signature);
                }
                continue;
            }

            if export_decl.module_specifier.is_none() {
                if !export_decl.export_clause.is_none() {
                    let clause_node = export_decl.export_clause;
                    let clause_node_ref = arena.get(clause_node);
                    if clause_node_ref
                        .and_then(|node| arena.get_named_imports(node))
                        .is_some()
                    {
                        collect_local_named_export_signatures(
                            arena,
                            file.source_file,
                            clause_node,
                            checker,
                            formatter,
                            export_type_prefix(export_decl.is_type_only),
                            signatures,
                        );
                    } else {
                        collect_exported_declaration_signatures(
                            arena,
                            clause_node,
                            checker,
                            formatter,
                            export_type_prefix(export_decl.is_type_only),
                            signatures,
                        );
                    }
                }
                continue;
            }

            let module_spec = arena
                .get_literal_text(export_decl.module_specifier)
                .unwrap_or("")
                .to_string();
            if export_decl.export_clause.is_none() {
                signatures.push(format!(
                    "{}*|{}",
                    export_type_prefix(export_decl.is_type_only),
                    module_spec
                ));
                continue;
            }

            let clause_node = export_decl.export_clause;
            let clause_node_ref = arena.get(clause_node);
            if let Some(named) = clause_node_ref.and_then(|node| arena.get_named_imports(node)) {
                let mut specifiers = Vec::new();
                for &spec_idx in &named.elements.nodes {
                    let Some(spec_node) = arena.get(spec_idx) else {
                        continue;
                    };
                    let Some(spec) = arena.get_specifier(spec_node) else {
                        continue;
                    };
                    let name = arena.get_identifier_text(spec.name).unwrap_or("");
                    if spec.property_name.is_none() {
                        specifiers.push(name.to_string());
                    } else {
                        let property = arena.get_identifier_text(spec.property_name).unwrap_or("");
                        specifiers.push(format!("{} as {}", property, name));
                    }
                }
                specifiers.sort();
                signatures.push(format!(
                    "{}{{{}}}|{}",
                    export_type_prefix(export_decl.is_type_only),
                    specifiers.join(","),
                    module_spec
                ));
            } else if let Some(name) = arena.get_identifier_text(clause_node) {
                signatures.push(format!(
                    "{}* as {}|{}",
                    export_type_prefix(export_decl.is_type_only),
                    name,
                    module_spec
                ));
            }

            continue;
        }

        if let Some(export_assignment) = arena.get_export_assignment(stmt)
            && !export_assignment.expression.is_none()
        {
            let type_id = checker.get_type_of_node(export_assignment.expression);
            let type_str = formatter.format(type_id);
            signatures.push(format!("export=:{type_str}"));
        }
    }
}

fn collect_local_named_export_signatures(
    arena: &NodeArena,
    source_file: NodeIndex,
    named_idx: NodeIndex,
    checker: &mut CheckerState,
    formatter: &mut TypeFormatter,
    type_prefix: &str,
    signatures: &mut Vec<String>,
) {
    let Some(named_node) = arena.get(named_idx) else {
        return;
    };
    let Some(named) = arena.get_named_imports(named_node) else {
        return;
    };

    for &spec_idx in &named.elements.nodes {
        let Some(spec_node) = arena.get(spec_idx) else {
            continue;
        };
        let Some(spec) = arena.get_specifier(spec_node) else {
            continue;
        };
        let exported_name = if !spec.name.is_none() {
            arena.get_identifier_text(spec.name).unwrap_or("")
        } else {
            arena.get_identifier_text(spec.property_name).unwrap_or("")
        };
        if exported_name.is_empty() {
            continue;
        }
        let local_name = if !spec.property_name.is_none() {
            arena.get_identifier_text(spec.property_name).unwrap_or("")
        } else {
            exported_name
        };
        let type_id = find_local_declaration(arena, source_file, local_name)
            .map(|decl_idx| checker.get_type_of_node(decl_idx))
            .unwrap_or(TypeId::ANY);
        let type_str = formatter.format(type_id);
        signatures.push(format!("{type_prefix}{exported_name}:{type_str}"));
    }
}

fn collect_exported_declaration_signatures(
    arena: &NodeArena,
    decl_idx: NodeIndex,
    checker: &mut CheckerState,
    formatter: &mut TypeFormatter,
    type_prefix: &str,
    signatures: &mut Vec<String>,
) {
    let Some(node) = arena.get(decl_idx) else {
        return;
    };

    if node.kind == syntax_kind_ext::VARIABLE_STATEMENT {
        if let Some(var_stmt) = arena.get_variable(node) {
            for &list_idx in &var_stmt.declarations.nodes {
                collect_exported_declaration_signatures(
                    arena,
                    list_idx,
                    checker,
                    formatter,
                    type_prefix,
                    signatures,
                );
            }
        }
        return;
    }

    if node.kind == syntax_kind_ext::VARIABLE_DECLARATION_LIST {
        if let Some(list) = arena.get_variable(node) {
            for &decl_idx in &list.declarations.nodes {
                collect_exported_declaration_signatures(
                    arena,
                    decl_idx,
                    checker,
                    formatter,
                    type_prefix,
                    signatures,
                );
            }
        }
        return;
    }

    if let Some(var_decl) = arena.get_variable_declaration(node) {
        if let Some(name) = arena.get_identifier_text(var_decl.name) {
            push_exported_signature(name, decl_idx, checker, formatter, type_prefix, signatures);
        }
        return;
    }

    if let Some(func) = arena.get_function(node) {
        if let Some(name) = arena.get_identifier_text(func.name) {
            push_exported_signature(name, decl_idx, checker, formatter, type_prefix, signatures);
        }
        return;
    }

    if let Some(class) = arena.get_class(node) {
        if let Some(name) = arena.get_identifier_text(class.name) {
            push_exported_signature(name, decl_idx, checker, formatter, type_prefix, signatures);
        }
        return;
    }

    if let Some(interface) = arena.get_interface(node) {
        if let Some(name) = arena.get_identifier_text(interface.name) {
            push_exported_signature(name, decl_idx, checker, formatter, type_prefix, signatures);
        }
        return;
    }

    if let Some(type_alias) = arena.get_type_alias(node) {
        if let Some(name) = arena.get_identifier_text(type_alias.name) {
            push_exported_signature(name, decl_idx, checker, formatter, type_prefix, signatures);
        }
        return;
    }

    if let Some(enum_decl) = arena.get_enum(node) {
        if let Some(name) = arena.get_identifier_text(enum_decl.name) {
            push_exported_signature(name, decl_idx, checker, formatter, type_prefix, signatures);
        }
        return;
    }

    if let Some(module_decl) = arena.get_module(node) {
        let name = arena
            .get_identifier_text(module_decl.name)
            .or_else(|| arena.get_literal_text(module_decl.name));
        if let Some(name) = name {
            push_exported_signature(name, decl_idx, checker, formatter, type_prefix, signatures);
        }
    }
}

fn push_exported_signature(
    name: &str,
    decl_idx: NodeIndex,
    checker: &mut CheckerState,
    formatter: &mut TypeFormatter,
    type_prefix: &str,
    signatures: &mut Vec<String>,
) {
    let type_id = checker.get_type_of_node(decl_idx);
    let type_str = formatter.format(type_id);
    signatures.push(format!("{type_prefix}{name}:{type_str}"));
}

fn find_local_declaration(
    arena: &NodeArena,
    source_file: NodeIndex,
    name: &str,
) -> Option<NodeIndex> {
    let Some(node) = arena.get(source_file) else {
        return None;
    };
    let Some(source) = arena.get_source_file(node) else {
        return None;
    };

    for &stmt_idx in &source.statements.nodes {
        let Some(stmt) = arena.get(stmt_idx) else {
            continue;
        };
        if let Some(export_decl) = arena.get_export_decl(stmt) {
            if export_decl.export_clause.is_none() {
                continue;
            }
            let clause_idx = export_decl.export_clause;
            let Some(clause_node) = arena.get(clause_idx) else {
                continue;
            };
            if arena.get_named_imports(clause_node).is_some() {
                continue;
            }
            if let Some(found) = find_local_declaration_in_node(arena, clause_idx, name) {
                return Some(found);
            }
            continue;
        }

        if let Some(found) = find_local_declaration_in_node(arena, stmt_idx, name) {
            return Some(found);
        }
    }

    None
}

fn find_local_declaration_in_node(
    arena: &NodeArena,
    node_idx: NodeIndex,
    name: &str,
) -> Option<NodeIndex> {
    let Some(node) = arena.get(node_idx) else {
        return None;
    };

    if let Some(var_decl) = arena.get_variable_declaration(node) {
        if let Some(decl_name) = arena.get_identifier_text(var_decl.name)
            && decl_name == name
        {
            return Some(node_idx);
        }
        return None;
    }

    if node.kind == syntax_kind_ext::VARIABLE_STATEMENT {
        if let Some(var_stmt) = arena.get_variable(node) {
            for &list_idx in &var_stmt.declarations.nodes {
                if let Some(found) = find_local_declaration_in_node(arena, list_idx, name) {
                    return Some(found);
                }
            }
        }
        return None;
    }

    if node.kind == syntax_kind_ext::VARIABLE_DECLARATION_LIST {
        if let Some(list) = arena.get_variable(node) {
            for &decl_idx in &list.declarations.nodes {
                if let Some(found) = find_local_declaration_in_node(arena, decl_idx, name) {
                    return Some(found);
                }
            }
        }
        return None;
    }

    if let Some(func) = arena.get_function(node) {
        if let Some(decl_name) = arena.get_identifier_text(func.name)
            && decl_name == name
        {
            return Some(node_idx);
        }
        return None;
    }

    if let Some(class) = arena.get_class(node) {
        if let Some(decl_name) = arena.get_identifier_text(class.name)
            && decl_name == name
        {
            return Some(node_idx);
        }
        return None;
    }

    if let Some(interface) = arena.get_interface(node) {
        if let Some(decl_name) = arena.get_identifier_text(interface.name)
            && decl_name == name
        {
            return Some(node_idx);
        }
        return None;
    }

    if let Some(type_alias) = arena.get_type_alias(node) {
        if let Some(decl_name) = arena.get_identifier_text(type_alias.name)
            && decl_name == name
        {
            return Some(node_idx);
        }
        return None;
    }

    if let Some(enum_decl) = arena.get_enum(node) {
        if let Some(decl_name) = arena.get_identifier_text(enum_decl.name)
            && decl_name == name
        {
            return Some(node_idx);
        }
        return None;
    }

    if let Some(module_decl) = arena.get_module(node) {
        let decl_name = arena
            .get_identifier_text(module_decl.name)
            .or_else(|| arena.get_literal_text(module_decl.name));
        if let Some(decl_name) = decl_name
            && decl_name == name
        {
            return Some(node_idx);
        }
    }

    None
}

fn export_default_signature(
    export_clause: NodeIndex,
    checker: &mut CheckerState,
    formatter: &mut TypeFormatter,
) -> Option<String> {
    if export_clause.is_none() {
        return None;
    }
    let type_id = if let Some(sym_id) = checker.ctx.binder.get_node_symbol(export_clause) {
        checker.get_type_of_symbol(sym_id)
    } else {
        checker.get_type_of_node(export_clause)
    };
    let type_str = formatter.format(type_id);
    Some(format!("default:{type_str}"))
}

fn export_type_prefix(is_type_only: bool) -> &'static str {
    if is_type_only { "type:" } else { "" }
}

fn parse_diagnostic_to_checker(file_name: &str, diagnostic: &ParseDiagnostic) -> Diagnostic {
    Diagnostic {
        file: file_name.to_string(),
        start: diagnostic.start,
        length: diagnostic.length,
        message_text: diagnostic.message.clone(),
        category: DiagnosticCategory::Error,
        code: diagnostic.code,
        related_information: Vec::new(),
    }
}

fn create_binder_from_bound_file(
    file: &BoundFile,
    program: &MergedProgram,
    file_idx: usize,
) -> BinderState {
    let mut file_locals = SymbolTable::new();

    if file_idx < program.file_locals.len() {
        for (name, &sym_id) in program.file_locals[file_idx].iter() {
            file_locals.set(name.clone(), sym_id);
        }
    }

    for (name, &sym_id) in program.globals.iter() {
        if !file_locals.has(name) {
            file_locals.set(name.clone(), sym_id);
        }
    }

    // Merge module augmentations from all files
    // When checking a file, we need access to augmentations from all other files
    let mut merged_module_augmentations: rustc_hash::FxHashMap<
        String,
        Vec<crate::binder::ModuleAugmentation>,
    > = rustc_hash::FxHashMap::default();

    for other_file in &program.files {
        for (spec, augs) in &other_file.module_augmentations {
            merged_module_augmentations
                .entry(spec.clone())
                .or_default()
                .extend(augs.clone());
        }
    }

    let mut binder = BinderState::from_bound_state_with_scopes_and_augmentations(
        BinderOptions::default(),
        program.symbols.clone(),
        file_locals,
        file.node_symbols.clone(),
        file.scopes.clone(),
        file.node_scope_ids.clone(),
        file.global_augmentations.clone(),
        merged_module_augmentations,
        program.module_exports.clone(),
        program.reexports.clone(),
        program.wildcard_reexports.clone(),
        program.symbol_arenas.clone(),
        program.declaration_arenas.clone(),
        program.shorthand_ambient_modules.clone(),
        file.flow_nodes.clone(),
        file.node_flow.clone(),
        file.switch_clause_to_switch.clone(),
    );

    binder.declared_modules = program.declared_modules.clone();
    // Restore is_external_module from BoundFile to preserve per-file state
    binder.is_external_module = file.is_external_module;
    binder
}

/// Build a lightweight binder for cross-file symbol/export lookups.
///
/// This avoids cloning per-file node/scope/flow structures when populating
/// `CheckerContext::all_binders`, which only needs symbol/file-local/module-export data.
fn create_cross_file_lookup_binder(
    file: &BoundFile,
    program: &MergedProgram,
    file_idx: usize,
) -> BinderState {
    let mut file_locals = SymbolTable::new();

    if file_idx < program.file_locals.len() {
        for (name, &sym_id) in program.file_locals[file_idx].iter() {
            file_locals.set(name.clone(), sym_id);
        }
    }

    for (name, &sym_id) in program.globals.iter() {
        if !file_locals.has(name) {
            file_locals.set(name.clone(), sym_id);
        }
    }

    // Keep cross-file binders intentionally tiny: they are only used to map
    // import targets to that file's locals/exports.
    let mut binder = BinderState::new();
    binder.file_locals = file_locals;
    if let Some(exports) = program.module_exports.get(&file.file_name) {
        binder
            .module_exports
            .insert(file.file_name.clone(), exports.clone());
    }
    binder.is_external_module = file.is_external_module;
    binder
}

pub fn apply_cli_overrides(options: &mut ResolvedCompilerOptions, args: &CliArgs) -> Result<()> {
    if let Some(target) = args.target {
        options.printer.target = target.to_script_target();
        options.checker.target = checker_target_from_emitter(options.printer.target);
    }
    if let Some(module) = args.module {
        options.printer.module = module.to_module_kind();
    }
    if let Some(module_resolution) = args.module_resolution {
        options.module_resolution = Some(module_resolution.to_module_resolution_kind());
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
    if args.strict {
        options.checker.strict = true;
        // Expand --strict to individual flags (matching TypeScript behavior)
        options.checker.no_implicit_any = true;
        options.checker.no_implicit_returns = true;
        options.checker.strict_null_checks = true;
        options.checker.strict_function_types = true;
        options.checker.strict_property_initialization = true;
        options.checker.no_implicit_this = true;
        options.checker.use_unknown_in_catch_variables = true;
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
    if let Some(val) = args.allow_unreachable_code {
        options.checker.allow_unreachable_code = val;
    }
    if args.sound {
        options.checker.sound_mode = true;
    }
    if args.experimental_decorators {
        options.checker.experimental_decorators = true;
    }
    if args.no_emit {
        options.no_emit = true;
    }
    if args.no_check {
        options.no_check = true;
    }
    if let Some(version) = args.types_versions_compiler_version.as_ref() {
        options.types_versions_compiler_version = Some(version.clone());
    } else if let Ok(version) = std::env::var("TSZ_TYPES_VERSIONS_COMPILER_VERSION") {
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
/// Returns the relative path (from base_dir) as a String, or None if no .d.ts files were found
fn find_latest_dts_file(emitted_files: &[PathBuf], base_dir: &Path) -> Option<String> {
    use std::collections::BTreeMap;

    let mut dts_files_with_times: BTreeMap<std::time::SystemTime, PathBuf> = BTreeMap::new();

    // Filter for .d.ts files and get their modification times
    for path in emitted_files {
        if path.extension().and_then(|s| s.to_str()) == Some("d.ts") {
            if let Ok(metadata) = std::fs::metadata(path) {
                if let Ok(modified) = metadata.modified() {
                    dts_files_with_times.insert(modified, path.clone());
                }
            }
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

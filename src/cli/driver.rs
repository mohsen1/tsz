use anyhow::{Context, Result, bail};
use std::collections::{HashMap, HashSet, VecDeque};
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::binder::BinderState;
use crate::binder::{SymbolId, SymbolTable, symbol_flags};
use crate::checker::TypeCache;
use crate::checker::context::LibContext;
use crate::checker::state::CheckerState;
use crate::checker::types::diagnostics::{
    Diagnostic, DiagnosticCategory, diagnostic_codes, diagnostic_messages, format_message,
};
use crate::cli::args::CliArgs;
use crate::cli::config::{
    ResolvedCompilerOptions, TsConfig, checker_target_from_emitter, load_tsconfig,
    resolve_compiler_options, resolve_default_lib_files, resolve_lib_files,
};
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
use crate::parallel::{self, BindResult, BoundFile, MergedProgram};
use crate::parser::NodeIndex;
use crate::parser::ParseDiagnostic;
use crate::parser::node::{NodeAccess, NodeArena};
use crate::parser::syntax_kind_ext;
use crate::solver::{TypeFormatter, TypeId};
use rustc_hash::{FxHashMap, FxHasher};

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
    type_caches: HashMap<PathBuf, TypeCache>,
    bind_cache: HashMap<PathBuf, BindCacheEntry>,
    dependencies: HashMap<PathBuf, HashSet<PathBuf>>,
    reverse_dependencies: HashMap<PathBuf, HashSet<PathBuf>>,
    diagnostics: HashMap<PathBuf, Vec<Diagnostic>>,
    export_hashes: HashMap<PathBuf, u64>,
    import_symbol_ids: HashMap<PathBuf, HashMap<PathBuf, Vec<SymbolId>>>,
    star_export_dependencies: HashMap<PathBuf, HashSet<PathBuf>>,
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
        let changed: HashSet<PathBuf> = paths.into_iter().collect();
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

    pub(crate) fn update_dependencies(&mut self, dependencies: HashMap<PathBuf, HashSet<PathBuf>>) {
        let mut reverse = HashMap::new();
        for (source, deps) in &dependencies {
            for dep in deps {
                reverse
                    .entry(dep.clone())
                    .or_insert_with(HashSet::new)
                    .insert(source.clone());
            }
        }
        self.dependencies = dependencies;
        self.reverse_dependencies = reverse;
    }

    fn collect_dependents<I>(&self, paths: I) -> HashSet<PathBuf>
    where
        I: IntoIterator<Item = PathBuf>,
    {
        let mut pending = VecDeque::new();
        let mut affected = HashSet::new();

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
    let mut old_hashes = HashMap::new();
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
        let mut direct_dependents = HashSet::new();
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
    forced_dirty_paths: Option<&HashSet<PathBuf>>,
    explicit_config_path: Option<&Path>,
) -> Result<CompilationResult> {
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
    let type_files = collect_type_root_files(&base_dir, &resolved);

    // Add type definition files (e.g., @types packages) to the source file list.
    // Note: lib.d.ts files are NOT added here - they are loaded separately via
    // load_lib_files_for_contexts() for symbol resolution. This prevents them from
    // being type-checked as regular source files (which would emit spurious errors).
    if !type_files.is_empty() {
        let mut merged = std::collections::BTreeSet::new();
        merged.extend(file_paths);
        merged.extend(type_files);
        file_paths = merged.into_iter().collect();
    }
    if file_paths.is_empty() {
        bail!("no input files found");
    }

    let changed_set = changed_paths.map(|paths| {
        paths
            .iter()
            .map(|path| canonicalize_or_owned(path))
            .collect::<HashSet<_>>()
    });
    let SourceReadResult {
        sources,
        dependencies,
    } = {
        let cache_ref = cache.as_deref();
        read_source_files(
            &file_paths,
            &base_dir,
            &resolved,
            cache_ref,
            changed_set.as_ref(),
        )?
    };
    if let Some(cache) = cache.as_deref_mut() {
        cache.update_dependencies(dependencies);
    }

    // Collect all files that were read (including dependencies) before sources is moved
    let mut files_read: Vec<PathBuf> = sources.iter().map(|s| s.path.clone()).collect();
    files_read.sort();

    // Build file info with inclusion reasons
    let file_infos = build_file_infos(&sources, &file_paths, args, config.as_ref(), &base_dir);

    let disable_default_libs = resolved.lib_is_default && sources_have_no_default_lib(&sources);
    let lib_paths: Vec<PathBuf> = if resolved.checker.no_lib || disable_default_libs {
        Vec::new()
    } else {
        resolved.lib_files.clone()
    };

    let (program, dirty_paths) = if let Some(cache) = cache.as_deref_mut() {
        let result = build_program_with_cache(sources, cache, &lib_paths);
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
        (
            parallel::compile_files_with_libs(compile_inputs, &lib_paths),
            None,
        )
    };
    if let Some(cache) = cache.as_deref_mut() {
        update_import_symbol_ids(&program, &resolved, &base_dir, cache);
    }

    // Load lib files only when type checking is needed (lazy loading for faster startup)
    let lib_contexts = if resolved.no_check {
        Vec::new() // Skip lib loading when --noCheck is set
    } else {
        load_lib_files_for_contexts(&lib_paths, resolved.printer.target)
    };
    let mut diagnostics = collect_diagnostics(&program, &resolved, &base_dir, cache, &lib_contexts);
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
        )?;
        write_outputs(&outputs)?
    };

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
    let root_set: std::collections::HashSet<_> = root_file_paths.iter().collect();
    let cli_files: std::collections::HashSet<_> = args.files.iter().collect();

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
    dirty_paths: HashSet<PathBuf>,
}

fn build_program_with_cache(
    sources: Vec<SourceEntry>,
    cache: &mut CompilationCache,
    lib_paths: &[PathBuf],
) -> BuildProgramResult {
    let mut meta = Vec::with_capacity(sources.len());
    let mut to_parse = Vec::new();
    let mut dirty_paths = HashSet::new();

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
        // Use parse_and_bind_parallel_with_lib_files to load lib.d.ts symbols
        // This ensures global symbols like console, Array, Promise are available
        // during binding, which prevents "Any poisoning" where unresolved symbols
        // default to Any type instead of emitting TS2304 errors.
        let lib_path_refs: Vec<&Path> = lib_paths.iter().map(PathBuf::as_path).collect();
        parallel::parse_and_bind_parallel_with_lib_files(to_parse, &lib_path_refs)
    };

    let mut parsed_map: HashMap<String, BindResult> = parsed_results
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
                    scopes: Vec::new(),
                    node_scope_ids: Default::default(),
                    parse_diagnostics: Vec::new(),
                    shorthand_ambient_modules: Default::default(),
                    global_augmentations: Default::default(),
                    reexports: Default::default(),
                    wildcard_reexports: Default::default(),
                    lib_binders: Vec::new(),
                    flow_nodes: Default::default(),
                    node_flow: Default::default(),
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

    let mut current_paths = HashSet::with_capacity(meta.len());
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
    let mut import_symbol_ids: HashMap<PathBuf, HashMap<PathBuf, Vec<SymbolId>>> = HashMap::new();
    let mut star_export_dependencies: HashMap<PathBuf, HashSet<PathBuf>> = HashMap::new();

    // Build set of known file paths for module resolution
    let known_files: HashSet<PathBuf> = program
        .files
        .iter()
        .map(|f| PathBuf::from(&f.file_name))
        .collect();

    for (file_idx, file) in program.files.iter().enumerate() {
        let file_path = PathBuf::from(&file.file_name);
        let mut by_dep: HashMap<PathBuf, Vec<SymbolId>> = HashMap::new();
        let mut star_exports: HashSet<PathBuf> = HashSet::new();
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

#[derive(Debug, Clone)]
struct SourceEntry {
    path: PathBuf,
    text: Option<String>,
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

struct SourceReadResult {
    sources: Vec<SourceEntry>,
    dependencies: HashMap<PathBuf, HashSet<PathBuf>>,
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
    changed_paths: Option<&HashSet<PathBuf>>,
) -> Result<SourceReadResult> {
    let mut sources: HashMap<PathBuf, Option<String>> = HashMap::new();
    let mut dependencies: HashMap<PathBuf, HashSet<PathBuf>> = HashMap::new();
    let mut seen = HashSet::new();
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
        if use_cache
            && let (Some(cache), Some(changed_paths)) = (cache, changed_paths)
            && !changed_paths.contains(&path)
            && let (Some(_), Some(cached_deps)) =
                (cache.bind_cache.get(&path), cache.dependencies.get(&path))
        {
            dependencies.insert(path.clone(), cached_deps.clone());
            sources.insert(path.clone(), None);
            for dep in cached_deps {
                if seen.insert(dep.clone()) {
                    pending.push_back(dep.clone());
                }
            }
            continue;
        }

        let text = std::fs::read_to_string(&path)
            .with_context(|| format!("failed to read {}", path.display()))?;
        let specifiers = collect_module_specifiers_from_text(&path, &text);
        sources.insert(path.clone(), Some(text));
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
        .map(|(path, text)| SourceEntry { path, text })
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
/// This function loads the specified lib.d.ts files (e.g., lib.dom.d.ts, lib.es*.d.ts)
/// and returns LibContext objects that can be used by the checker to resolve global
/// symbols like `console`, `Array`, `Promise`, etc.
///
/// If disk files are not available or fail to load, this function falls back to embedded libs
/// for the specified target to ensure global types are always available.
///
/// IMPORTANT: This function now recursively resolves lib dependencies (/// <reference lib="..." />)
/// to prevent duplicate declarations. Previously, it would load only the explicitly specified libs
/// from disk, then load embedded libs as a "fallback" for missing dependencies, which caused
/// duplicate symbol declarations.
fn load_lib_files_for_contexts(
    lib_files: &[PathBuf],
    _target: crate::emitter::ScriptTarget,
) -> Vec<LibContext> {
    use crate::binder::BinderState;
    use crate::parser::ParserState;
    use rayon::prelude::*;
    use std::collections::HashSet;
    use std::sync::Arc;

    // Deduplicate lib paths by file stem (lib name)
    // resolve_lib_files already resolved all /// <reference lib="..." /> directives,
    // so we just need to dedupe and read the files.
    let mut seen_libs = HashSet::new();
    let unique_lib_paths: Vec<_> = lib_files
        .iter()
        .filter(|path| {
            let lib_name = path
                .file_stem()
                .and_then(|s| s.to_str())
                .and_then(|s| s.strip_prefix("lib."))
                .and_then(|s| s.strip_suffix(".generated"))
                .unwrap_or_else(|| path.file_stem().and_then(|s| s.to_str()).unwrap_or(""));
            path.exists() && seen_libs.insert(lib_name.to_string())
        })
        .collect();

    // Read all lib files in PARALLEL (major speedup - eliminates sequential I/O bottleneck)
    let lib_contents: Vec<(String, String)> = unique_lib_paths
        .into_par_iter()
        .filter_map(|lib_path| {
            let source_text = std::fs::read_to_string(lib_path).ok()?;
            let file_name = lib_path.to_string_lossy().to_string();
            Some((file_name, source_text))
        })
        .collect();

    // Parse and bind all libs in parallel
    let lib_contexts: Vec<LibContext> = lib_contents
        .into_par_iter()
        .filter_map(|(file_name, source_text)| {
            let mut lib_parser = ParserState::new(file_name, source_text);
            let source_file_idx = lib_parser.parse_source_file();

            if !lib_parser.get_diagnostics().is_empty() {
                return None;
            }

            let mut lib_binder = BinderState::new();
            lib_binder.bind_source_file(lib_parser.get_arena(), source_file_idx);

            let arena = Arc::new(lib_parser.into_arena());
            let binder = Arc::new(lib_binder);

            Some(LibContext { arena, binder })
        })
        .collect();

    // Merge all lib binders into a single binder to avoid duplicate SymbolIds
    // This is necessary because different lib files may declare the same symbols
    // (e.g., "Intl" is declared in lib.esnext.d.ts, lib.es2024.d.ts, etc.)
    if !lib_contexts.is_empty() {
        use crate::binder::state::LibContext as BinderLibContext;

        let mut merged_binder = crate::binder::BinderState::new();
        let binder_lib_contexts: Vec<_> = lib_contexts
            .iter()
            .map(|ctx| BinderLibContext {
                arena: std::sync::Arc::clone(&ctx.arena),
                binder: std::sync::Arc::clone(&ctx.binder),
            })
            .collect();

        merged_binder.merge_lib_contexts_into_binder(&binder_lib_contexts);

        // Replace multiple lib contexts with a single merged one
        let merged_arena = lib_contexts
            .first()
            .map(|ctx| std::sync::Arc::clone(&ctx.arena))
            .unwrap();
        let merged_binder = std::sync::Arc::new(merged_binder);

        return vec![LibContext {
            arena: merged_arena,
            binder: merged_binder,
        }];
    }

    lib_contexts
}

fn collect_diagnostics(
    program: &MergedProgram,
    options: &ResolvedCompilerOptions,
    base_dir: &Path,
    cache: Option<&mut CompilationCache>,
    lib_contexts: &[LibContext],
) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    let mut used_paths = HashSet::new();
    let mut cache = cache;
    let mut resolution_cache = ModuleResolutionCache::default();
    let mut program_paths = HashSet::new();
    let mut canonical_to_file_name: HashMap<PathBuf, String> = HashMap::new();
    let mut canonical_to_file_idx: HashMap<PathBuf, usize> = HashMap::new();

    for (idx, file) in program.files.iter().enumerate() {
        let canonical = canonicalize_or_owned(Path::new(&file.file_name));
        program_paths.insert(canonical.clone());
        canonical_to_file_name.insert(canonical.clone(), file.file_name.clone());
        canonical_to_file_idx.insert(canonical, idx);
    }

    // Pre-create all binders for cross-file resolution
    let all_binders: Vec<Arc<BinderState>> = program
        .files
        .iter()
        .enumerate()
        .map(|(file_idx, file)| Arc::new(create_binder_from_bound_file(file, program, file_idx)))
        .collect();

    // Collect all arenas for cross-file resolution
    let all_arenas: Vec<Arc<NodeArena>> = program
        .files
        .iter()
        .map(|file| Arc::clone(&file.arena))
        .collect();

    // Build resolved_module_paths map: (source_file_idx, specifier) -> target_file_idx
    let mut resolved_module_paths: FxHashMap<(usize, String), usize> = FxHashMap::default();
    for (file_idx, file) in program.files.iter().enumerate() {
        let module_specifiers = collect_module_specifiers(&file.arena, file.source_file);
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
                if let Some(&target_idx) = canonical_to_file_idx.get(&canonical) {
                    resolved_module_paths.insert((file_idx, specifier.clone()), target_idx);
                }
            }
        }
    }

    for (file_idx, file) in program.files.iter().enumerate() {
        let file_path = PathBuf::from(&file.file_name);
        used_paths.insert(file_path.clone());
        if let Some(cached) = cache
            .as_deref()
            .and_then(|cache| cache.diagnostics.get(&file_path))
        {
            diagnostics.extend(cached.clone());
            continue;
        }

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
                &program.type_interner,
                file.file_name.clone(),
                cached,
                compiler_options,
            )
        } else {
            CheckerState::new(
                &file.arena,
                &binder,
                &program.type_interner,
                file.file_name.clone(),
                compiler_options,
            )
        };
        checker.ctx.report_unresolved_imports = true;
        // Set lib contexts for global symbol resolution (console, Array, Promise, etc.)
        if !lib_contexts.is_empty() {
            checker.ctx.set_lib_contexts(lib_contexts.to_vec());
        }
        // Set cross-file resolution context for import type resolution
        checker.ctx.set_all_arenas(all_arenas.clone());
        checker.ctx.set_all_binders(all_binders.clone());
        checker
            .ctx
            .set_resolved_module_paths(resolved_module_paths.clone());
        checker.ctx.set_current_file_idx(file_idx);

        let mut resolved_modules = HashSet::new();
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
                if program_paths.contains(&canonical) {
                    resolved_modules.insert(specifier.clone());
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
        let file_path_for_resolve = Path::new(&file.file_name);
        for (specifier, specifier_node) in module_specifiers {
            if specifier.is_empty() {
                continue;
            }
            // Skip ambient module declarations (declared_modules has body, shorthand has none)
            // Both should not emit TS2307 as they provide type information
            if program.declared_modules.contains(specifier.as_str()) {
                continue;
            }
            if program
                .shorthand_ambient_modules
                .contains(specifier.as_str())
            {
                continue;
            }
            let resolved = resolve_module_specifier(
                file_path_for_resolve,
                &specifier,
                options,
                base_dir,
                &mut resolution_cache,
                &program_paths,
            );
            if let Some(resolved_path) = resolved {
                let canonical = canonicalize_or_owned(&resolved_path);
                if program_paths.contains(&canonical) {
                    continue;
                }
                // Module resolved to a path outside program_paths (e.g., node_modules)
                // Still emit TS2307 since it's not part of the compilation
            }
            if specifier_node.is_none() {
                continue;
            }
            let Some(spec_node) = file.arena.get(specifier_node) else {
                continue;
            };
            let start = spec_node.pos;
            let length = spec_node.end.saturating_sub(spec_node.pos);
            let message = format_message(
                diagnostic_messages::CANNOT_FIND_MODULE,
                &[specifier.as_str()],
            );
            file_diagnostics.push(Diagnostic::error(
                file.file_name.clone(),
                start,
                length,
                message,
                diagnostic_codes::CANNOT_FIND_MODULE,
            ));
        }
        // Skip full type checking when --noCheck is set; only parse/emit diagnostics are reported.
        if !options.no_check {
            checker.check_source_file(file.source_file);
            file_diagnostics.extend(std::mem::take(&mut checker.ctx.diagnostics));
        }
        diagnostics.extend(file_diagnostics.clone());
        let export_hash = compute_export_hash(program, file, file_idx, &mut checker);

        if let Some(cache) = cache.as_deref_mut() {
            cache
                .type_caches
                .insert(file_path.clone(), checker.extract_cache());
            cache
                .diagnostics
                .insert(file_path.clone(), file_diagnostics);
            cache.export_hashes.insert(file_path, export_hash);
        }
    }

    if let Some(cache) = cache {
        cache
            .type_caches
            .retain(|path, _| used_paths.contains(path));
        cache
            .diagnostics
            .retain(|path, _| used_paths.contains(path));
        cache
            .export_hashes
            .retain(|path, _| used_paths.contains(path));
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
            let type_str = formatter.format(type_id);
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

    let mut binder = BinderState::from_bound_state_with_scopes_and_augmentations(
        program.symbols.clone(),
        file_locals,
        file.node_symbols.clone(),
        file.scopes.clone(),
        file.node_scope_ids.clone(),
        file.global_augmentations.clone(),
        program.module_exports.clone(),
        program.reexports.clone(),
        program.wildcard_reexports.clone(),
        program.symbol_arenas.clone(),
        program.shorthand_ambient_modules.clone(),
        file.flow_nodes.clone(),
        file.node_flow.clone(),
    );

    binder.declared_modules = program.declared_modules.clone();
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
    if args.target.is_some() && options.lib_is_default && !options.checker.no_lib {
        options.lib_files = resolve_default_lib_files(options.printer.target)?;
    }

    Ok(())
}

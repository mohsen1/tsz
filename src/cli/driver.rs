use anyhow::{Context, Result, bail};
use rayon::prelude::*;
use serde::Deserialize;
use std::collections::{HashMap, HashSet, VecDeque};
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};

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
    JsxEmit, ModuleResolutionKind, PathMapping, ResolvedCompilerOptions, TsConfig,
    checker_target_from_emitter, load_tsconfig, resolve_compiler_options,
    resolve_default_lib_files, resolve_lib_files,
};
use crate::cli::fs::{FileDiscoveryOptions, discover_ts_files, is_valid_module_file};
use crate::declaration_emitter::DeclarationEmitter;
use crate::emitter::{ModuleKind, NewLineKind, Printer};
use crate::parallel::{self, BindResult, BoundFile, MergedProgram};
use crate::parser::NodeIndex;
use crate::parser::ParseDiagnostic;
use crate::parser::ParserState;
use crate::parser::node::{NodeAccess, NodeArena};
use crate::parser::syntax_kind_ext;
use crate::scanner::SyntaxKind;
use crate::solver::{TypeFormatter, TypeId};
use rustc_hash::FxHasher;

#[derive(Debug, Clone)]
pub struct CompilationResult {
    pub diagnostics: Vec<Diagnostic>,
    pub emitted_files: Vec<PathBuf>,
    pub files_read: Vec<PathBuf>,
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
    compile_inner(args, cwd, None, None, None)
}

pub(crate) fn compile_with_cache(
    args: &CliArgs,
    cwd: &Path,
    cache: &mut CompilationCache,
) -> Result<CompilationResult> {
    compile_inner(args, cwd, Some(cache), None, None)
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
    let result = compile_inner(args, cwd, Some(cache), Some(&canonical_paths), None)?;

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
    )
}

fn compile_inner(
    args: &CliArgs,
    cwd: &Path,
    mut cache: Option<&mut CompilationCache>,
    changed_paths: Option<&[PathBuf]>,
    forced_dirty_paths: Option<&HashSet<PathBuf>>,
) -> Result<CompilationResult> {
    let cwd = canonicalize_or_owned(cwd);
    let tsconfig_path = resolve_tsconfig_path(&cwd, args.project.as_deref())?;
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

    let lib_contexts = load_lib_files_for_contexts(&lib_paths, resolved.printer.target);
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
    })
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

#[derive(Debug, Clone)]
struct OutputFile {
    path: PathBuf,
    contents: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PackageType {
    Module,
    CommonJs,
}

#[derive(Default)]
struct ModuleResolutionCache {
    package_type_by_dir: HashMap<PathBuf, Option<PackageType>>,
}

impl ModuleResolutionCache {
    fn package_type_for_dir(&mut self, dir: &Path, base_dir: &Path) -> Option<PackageType> {
        let mut current = dir;
        let mut visited = Vec::new();

        loop {
            if let Some(value) = self.package_type_by_dir.get(current).copied() {
                for path in visited {
                    self.package_type_by_dir.insert(path, value);
                }
                return value;
            }

            visited.push(current.to_path_buf());

            if let Some(package_json) = read_package_json(&current.join("package.json")) {
                let value = package_type_from_json(Some(&package_json));
                for path in visited {
                    self.package_type_by_dir.insert(path, value);
                }
                return value;
            }

            if current == base_dir {
                for path in visited {
                    self.package_type_by_dir.insert(path, None);
                }
                return None;
            }

            let Some(parent) = current.parent() else {
                for path in visited {
                    self.package_type_by_dir.insert(path, None);
                }
                return None;
            };
            current = parent;
        }
    }
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

fn resolve_type_package_from_roots(
    name: &str,
    roots: &[PathBuf],
    options: &ResolvedCompilerOptions,
) -> Option<PathBuf> {
    let candidates = type_package_candidates(name);
    if candidates.is_empty() {
        return None;
    }

    for root in roots {
        for candidate in &candidates {
            let package_root = root.join(candidate);
            if !package_root.is_dir() {
                continue;
            }
            if let Some(entry) = resolve_type_package_entry(&package_root, options) {
                return Some(entry);
            }
        }
    }

    None
}

fn type_package_candidates(name: &str) -> Vec<String> {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return Vec::new();
    }

    let normalized = trimmed.replace('\\', "/");
    let mut candidates = Vec::new();

    if let Some(stripped) = normalized.strip_prefix("@types/")
        && !stripped.is_empty()
    {
        candidates.push(stripped.to_string());
    }

    if !candidates.iter().any(|value| value == &normalized) {
        candidates.push(normalized);
    }

    candidates
}

fn collect_type_packages_from_root(root: &Path) -> Vec<PathBuf> {
    let mut packages = Vec::new();
    let entries = match std::fs::read_dir(root) {
        Ok(entries) => entries,
        Err(_) => return packages,
    };

    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if name.starts_with('.') {
            continue;
        }
        if name.starts_with('@') {
            if let Ok(scope_entries) = std::fs::read_dir(&path) {
                for scope_entry in scope_entries.flatten() {
                    let scope_path = scope_entry.path();
                    if scope_path.is_dir() {
                        packages.push(scope_path);
                    }
                }
            }
            continue;
        }
        packages.push(path);
    }

    packages
}

fn resolve_type_package_entry(
    package_root: &Path,
    options: &ResolvedCompilerOptions,
) -> Option<PathBuf> {
    let package_json = read_package_json(&package_root.join("package.json"));
    let package_type = package_type_from_json(package_json.as_ref());
    let resolved =
        resolve_package_root(package_root, package_json.as_ref(), options, package_type)?;
    if is_declaration_file(&resolved) {
        Some(resolved)
    } else {
        None
    }
}

fn default_type_roots(base_dir: &Path) -> Vec<PathBuf> {
    let candidate = base_dir.join("node_modules").join("@types");
    if candidate.is_dir() {
        vec![canonicalize_or_owned(&candidate)]
    } else {
        Vec::new()
    }
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

fn collect_module_specifiers_from_text(path: &Path, text: &str) -> Vec<String> {
    let file_name = path.to_string_lossy().into_owned();
    let mut parser = ParserState::new(file_name, text.to_string());
    let source_file = parser.parse_source_file();
    let (arena, _diagnostics) = parser.into_parts();
    collect_module_specifiers(&arena, source_file)
        .into_iter()
        .map(|(specifier, _)| specifier)
        .collect()
}

fn collect_module_specifiers(
    arena: &NodeArena,
    source_file: NodeIndex,
) -> Vec<(String, NodeIndex)> {
    let mut specifiers = Vec::new();

    let Some(node) = arena.get(source_file) else {
        return specifiers;
    };
    let Some(source) = arena.get_source_file(node) else {
        return specifiers;
    };

    for &stmt_idx in &source.statements.nodes {
        if stmt_idx.is_none() {
            continue;
        }
        let Some(stmt) = arena.get(stmt_idx) else {
            continue;
        };
        if let Some(import_decl) = arena.get_import_decl(stmt)
            && let Some(text) = arena.get_literal_text(import_decl.module_specifier)
        {
            specifiers.push((text.to_string(), import_decl.module_specifier));
        }
        if let Some(export_decl) = arena.get_export_decl(stmt) {
            if let Some(text) = arena.get_literal_text(export_decl.module_specifier) {
                specifiers.push((text.to_string(), export_decl.module_specifier));
            } else if !export_decl.export_clause.is_none()
                && let Some(clause_node) = arena.get(export_decl.export_clause)
                && let Some(import_decl) = arena.get_import_decl(clause_node)
                && let Some(text) = arena.get_literal_text(import_decl.module_specifier)
            {
                specifiers.push((text.to_string(), import_decl.module_specifier));
            }
        }
    }

    specifiers
}

fn collect_import_bindings(
    arena: &NodeArena,
    source_file: NodeIndex,
) -> Vec<(String, Vec<String>)> {
    let mut bindings = Vec::new();
    let Some(node) = arena.get(source_file) else {
        return bindings;
    };
    let Some(source) = arena.get_source_file(node) else {
        return bindings;
    };

    for &stmt_idx in &source.statements.nodes {
        if stmt_idx.is_none() {
            continue;
        }
        let Some(stmt) = arena.get(stmt_idx) else {
            continue;
        };
        let Some(import_decl) = arena.get_import_decl(stmt) else {
            continue;
        };
        let Some(specifier) = arena.get_literal_text(import_decl.module_specifier) else {
            continue;
        };
        let local_names = collect_import_local_names(arena, import_decl);
        if !local_names.is_empty() {
            bindings.push((specifier.to_string(), local_names));
        }
    }

    bindings
}

fn collect_export_binding_nodes(
    arena: &NodeArena,
    source_file: NodeIndex,
) -> Vec<(String, Vec<NodeIndex>)> {
    let mut bindings = Vec::new();
    let Some(node) = arena.get(source_file) else {
        return bindings;
    };
    let Some(source) = arena.get_source_file(node) else {
        return bindings;
    };

    for &stmt_idx in &source.statements.nodes {
        if stmt_idx.is_none() {
            continue;
        }
        let Some(stmt) = arena.get(stmt_idx) else {
            continue;
        };
        let Some(export_decl) = arena.get_export_decl(stmt) else {
            continue;
        };
        if export_decl.export_clause.is_none() {
            continue;
        }
        let clause_idx = export_decl.export_clause;
        let Some(clause_node) = arena.get(clause_idx) else {
            continue;
        };

        let import_decl = arena.get_import_decl(clause_node);
        let mut specifier = arena
            .get_literal_text(export_decl.module_specifier)
            .map(|text| text.to_string());
        if specifier.is_none()
            && let Some(import_decl) = import_decl
            && let Some(text) = arena.get_literal_text(import_decl.module_specifier)
        {
            specifier = Some(text.to_string());
        }
        let Some(specifier) = specifier else {
            continue;
        };

        let mut nodes = Vec::new();
        if import_decl.is_some() {
            nodes.push(clause_idx);
        } else if let Some(named) = arena.get_named_imports(clause_node) {
            for &spec_idx in &named.elements.nodes {
                if !spec_idx.is_none() {
                    nodes.push(spec_idx);
                }
            }
        } else if arena.get_identifier_text(clause_idx).is_some() {
            nodes.push(clause_idx);
        }

        if !nodes.is_empty() {
            bindings.push((specifier.to_string(), nodes));
        }
    }

    bindings
}

fn collect_star_export_specifiers(arena: &NodeArena, source_file: NodeIndex) -> Vec<String> {
    let mut specifiers = Vec::new();
    let Some(node) = arena.get(source_file) else {
        return specifiers;
    };
    let Some(source) = arena.get_source_file(node) else {
        return specifiers;
    };

    for &stmt_idx in &source.statements.nodes {
        if stmt_idx.is_none() {
            continue;
        }
        let Some(stmt) = arena.get(stmt_idx) else {
            continue;
        };
        let Some(export_decl) = arena.get_export_decl(stmt) else {
            continue;
        };
        if !export_decl.export_clause.is_none() {
            continue;
        }
        if let Some(text) = arena.get_literal_text(export_decl.module_specifier) {
            specifiers.push(text.to_string());
        }
    }

    specifiers
}

fn collect_import_local_names(
    arena: &NodeArena,
    import_decl: &crate::parser::node::ImportDeclData,
) -> Vec<String> {
    let mut names = Vec::new();
    if import_decl.import_clause.is_none() {
        return names;
    }

    let clause_idx = import_decl.import_clause;
    if let Some(clause_node) = arena.get(clause_idx) {
        if let Some(clause) = arena.get_import_clause(clause_node) {
            if !clause.name.is_none()
                && let Some(name) = arena.get_identifier_text(clause.name)
            {
                names.push(name.to_string());
            }

            if !clause.named_bindings.is_none()
                && let Some(bindings_node) = arena.get(clause.named_bindings)
            {
                if bindings_node.kind == SyntaxKind::Identifier as u16 {
                    if let Some(name) = arena.get_identifier_text(clause.named_bindings) {
                        names.push(name.to_string());
                    }
                } else if let Some(named) = arena.get_named_imports(bindings_node) {
                    if !named.name.is_none()
                        && let Some(name) = arena.get_identifier_text(named.name)
                    {
                        names.push(name.to_string());
                    }
                    for &spec_idx in &named.elements.nodes {
                        let Some(spec_node) = arena.get(spec_idx) else {
                            continue;
                        };
                        let Some(spec) = arena.get_specifier(spec_node) else {
                            continue;
                        };
                        let local_ident = if !spec.name.is_none() {
                            spec.name
                        } else {
                            spec.property_name
                        };
                        if let Some(name) = arena.get_identifier_text(local_ident) {
                            names.push(name.to_string());
                        }
                    }
                }
            }
        } else if let Some(name) = arena.get_identifier_text(clause_idx) {
            names.push(name.to_string());
        }
    } else if let Some(name) = arena.get_identifier_text(clause_idx) {
        names.push(name.to_string());
    }

    names
}

fn resolve_module_specifier(
    from_file: &Path,
    module_specifier: &str,
    options: &ResolvedCompilerOptions,
    base_dir: &Path,
    resolution_cache: &mut ModuleResolutionCache,
    known_files: &HashSet<PathBuf>,
) -> Option<PathBuf> {
    let specifier = module_specifier.trim();
    if specifier.is_empty() {
        return None;
    }
    let specifier = specifier.replace('\\', "/");
    let resolution = options.effective_module_resolution();
    if specifier.starts_with('#') {
        if matches!(
            resolution,
            ModuleResolutionKind::Node
                | ModuleResolutionKind::Node16
                | ModuleResolutionKind::NodeNext
                | ModuleResolutionKind::Bundler
        ) {
            return resolve_package_imports_specifier(from_file, &specifier, base_dir, options);
        }
        return None;
    }
    let mut candidates = Vec::new();

    let from_dir = from_file.parent().unwrap_or(base_dir);
    let package_type = match resolution {
        ModuleResolutionKind::Node16 | ModuleResolutionKind::NodeNext => {
            resolution_cache.package_type_for_dir(from_dir, base_dir)
        }
        _ => None,
    };

    let mut allow_node_modules = false;
    let mut path_mapping_attempted = false;

    if Path::new(&specifier).is_absolute() {
        candidates.extend(expand_module_path_candidates(
            &PathBuf::from(specifier.as_str()),
            options,
            package_type,
        ));
    } else if specifier.starts_with('.') {
        let joined = from_dir.join(&specifier);
        candidates.extend(expand_module_path_candidates(
            &joined,
            options,
            package_type,
        ));
    } else if matches!(resolution, ModuleResolutionKind::Classic) {
        if let Some(paths) = options.paths.as_ref()
            && let Some((mapping, wildcard)) = select_path_mapping(paths, &specifier)
        {
            path_mapping_attempted = true;
            let base = options.base_url.as_deref().unwrap_or(from_dir);
            for target in &mapping.targets {
                let substituted = substitute_path_target(target, &wildcard);
                let path = if Path::new(&substituted).is_absolute() {
                    PathBuf::from(substituted)
                } else {
                    base.join(substituted)
                };
                candidates.extend(expand_module_path_candidates(&path, options, package_type));
            }
        }

        if candidates.is_empty() {
            let base = options.base_url.as_deref().unwrap_or(from_dir);
            candidates.extend(expand_module_path_candidates(
                &base.join(&specifier),
                options,
                package_type,
            ));
        }
    } else if let Some(base_url) = options.base_url.as_ref() {
        allow_node_modules = true;
        if let Some(paths) = options.paths.as_ref()
            && let Some((mapping, wildcard)) = select_path_mapping(paths, &specifier)
        {
            path_mapping_attempted = true;
            for target in &mapping.targets {
                let substituted = substitute_path_target(target, &wildcard);
                let path = if Path::new(&substituted).is_absolute() {
                    PathBuf::from(substituted)
                } else {
                    base_url.join(substituted)
                };
                candidates.extend(expand_module_path_candidates(&path, options, package_type));
            }
        }

        if candidates.is_empty() {
            candidates.extend(expand_module_path_candidates(
                &base_url.join(&specifier),
                options,
                package_type,
            ));
        }
    } else {
        allow_node_modules = true;
    }

    for candidate in candidates {
        // Check if candidate exists in known files (for virtual test files) or on filesystem
        let exists = known_files.contains(&candidate)
            || (candidate.is_file() && is_valid_module_file(&candidate));

        if exists {
            return Some(canonicalize_or_owned(&candidate));
        }
    }

    // If path mapping was attempted but no file was found, return None early
    // to emit TS2307 rather than falling through to node_modules resolution
    if path_mapping_attempted {
        return None;
    }

    if allow_node_modules {
        return resolve_node_module_specifier(from_file, &specifier, base_dir, options);
    }

    None
}

fn select_path_mapping<'a>(
    mappings: &'a [PathMapping],
    specifier: &str,
) -> Option<(&'a PathMapping, String)> {
    let mut best: Option<(&PathMapping, String)> = None;
    let mut best_score = 0usize;
    let mut best_pattern_len = 0usize;

    for mapping in mappings {
        let Some(wildcard) = mapping.match_specifier(specifier) else {
            continue;
        };
        let score = mapping.specificity();
        let pattern_len = mapping.pattern.len();

        let is_better = match &best {
            None => true,
            Some((current, _)) => {
                score > best_score
                    || (score == best_score && pattern_len > best_pattern_len)
                    || (score == best_score
                        && pattern_len == best_pattern_len
                        && mapping.pattern < current.pattern)
            }
        };

        if is_better {
            best_score = score;
            best_pattern_len = pattern_len;
            best = Some((mapping, wildcard));
        }
    }

    best
}

fn substitute_path_target(target: &str, wildcard: &str) -> String {
    if target.contains('*') {
        target.replace('*', wildcard)
    } else {
        target.to_string()
    }
}

fn expand_module_path_candidates(
    path: &Path,
    options: &ResolvedCompilerOptions,
    package_type: Option<PackageType>,
) -> Vec<PathBuf> {
    let base = normalize_path(path);
    if let Some(extension) = base.extension().and_then(|ext| ext.to_str()) {
        let resolution = options.effective_module_resolution();
        if matches!(
            resolution,
            ModuleResolutionKind::Node16 | ModuleResolutionKind::NodeNext
        ) && let Some(rewritten) = node16_extension_substitution(&base, extension)
        {
            return rewritten;
        }
        return vec![base];
    }

    let extensions = extension_candidates_for_resolution(options, package_type);
    let mut candidates = Vec::new();
    for ext in extensions {
        candidates.push(base.with_extension(ext));
    }
    for ext in extensions {
        candidates.push(base.join("index").with_extension(ext));
    }
    candidates
}

fn node16_extension_substitution(path: &Path, extension: &str) -> Option<Vec<PathBuf>> {
    let replacements: &[&str] = match extension {
        "js" => &["ts", "tsx", "d.ts"],
        "jsx" => &["tsx", "d.ts"],
        "mjs" => &["mts", "d.mts"],
        "cjs" => &["cts", "d.cts"],
        _ => return None,
    };

    Some(
        replacements
            .iter()
            .map(|ext| path.with_extension(ext))
            .collect(),
    )
}

fn extension_candidates_for_resolution(
    options: &ResolvedCompilerOptions,
    package_type: Option<PackageType>,
) -> &'static [&'static str] {
    match options.effective_module_resolution() {
        ModuleResolutionKind::Node16 | ModuleResolutionKind::NodeNext => match package_type {
            Some(PackageType::Module) => &NODE16_MODULE_EXTENSION_CANDIDATES,
            Some(PackageType::CommonJs) => &NODE16_COMMONJS_EXTENSION_CANDIDATES,
            None => &TS_EXTENSION_CANDIDATES,
        },
        _ => &TS_EXTENSION_CANDIDATES,
    }
}

fn normalize_path(path: &Path) -> PathBuf {
    let mut normalized = PathBuf::new();

    for component in path.components() {
        match component {
            std::path::Component::CurDir => {}
            std::path::Component::ParentDir => {
                normalized.pop();
            }
            std::path::Component::RootDir
            | std::path::Component::Normal(_)
            | std::path::Component::Prefix(_) => {
                normalized.push(component.as_os_str());
            }
        }
    }

    normalized
}

const TS_EXTENSION_CANDIDATES: [&str; 7] = ["ts", "tsx", "d.ts", "mts", "cts", "d.mts", "d.cts"];
const NODE16_MODULE_EXTENSION_CANDIDATES: [&str; 7] =
    ["mts", "d.mts", "ts", "tsx", "d.ts", "cts", "d.cts"];
const NODE16_COMMONJS_EXTENSION_CANDIDATES: [&str; 7] =
    ["cts", "d.cts", "ts", "tsx", "d.ts", "mts", "d.mts"];

#[derive(Debug, Deserialize)]
struct PackageJson {
    #[serde(default)]
    types: Option<String>,
    #[serde(default)]
    typings: Option<String>,
    #[serde(default)]
    main: Option<String>,
    #[serde(default)]
    module: Option<String>,
    #[serde(default, rename = "type")]
    package_type: Option<String>,
    #[serde(default)]
    exports: Option<serde_json::Value>,
    #[serde(default)]
    imports: Option<serde_json::Value>,
    #[serde(default, rename = "typesVersions")]
    types_versions: Option<serde_json::Value>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
struct SemVer {
    major: u32,
    minor: u32,
    patch: u32,
}

impl SemVer {
    const ZERO: SemVer = SemVer {
        major: 0,
        minor: 0,
        patch: 0,
    };
}

// NOTE: Keep this in sync with the TypeScript version this compiler targets.
// TODO: Make this configurable once CLI plumbing is available.
const TYPES_VERSIONS_COMPILER_VERSION_FALLBACK: SemVer = SemVer {
    major: 6,
    minor: 0,
    patch: 0,
};

fn types_versions_compiler_version(options: &ResolvedCompilerOptions) -> SemVer {
    options
        .types_versions_compiler_version
        .as_deref()
        .and_then(parse_semver)
        .unwrap_or_else(default_types_versions_compiler_version)
}

fn default_types_versions_compiler_version() -> SemVer {
    // Use the fallback version directly since the project's package.json version
    // is not a TypeScript version. The fallback represents the TypeScript version
    // that this compiler is compatible with for typesVersions resolution.
    TYPES_VERSIONS_COMPILER_VERSION_FALLBACK
}

fn export_conditions(options: &ResolvedCompilerOptions) -> Vec<&'static str> {
    let resolution = options.effective_module_resolution();
    let mut conditions = Vec::new();
    push_condition(&mut conditions, "types");

    match resolution {
        ModuleResolutionKind::Bundler => push_condition(&mut conditions, "browser"),
        ModuleResolutionKind::Classic
        | ModuleResolutionKind::Node
        | ModuleResolutionKind::Node16
        | ModuleResolutionKind::NodeNext => {
            push_condition(&mut conditions, "node");
        }
    }

    match options.printer.module {
        ModuleKind::CommonJS | ModuleKind::AMD | ModuleKind::UMD | ModuleKind::System => {
            push_condition(&mut conditions, "require");
        }
        ModuleKind::ES2015
        | ModuleKind::ES2020
        | ModuleKind::ES2022
        | ModuleKind::ESNext
        | ModuleKind::Node16
        | ModuleKind::NodeNext => {
            push_condition(&mut conditions, "import");
        }
        _ => {}
    }

    push_condition(&mut conditions, "default");
    match resolution {
        ModuleResolutionKind::Bundler => {
            push_condition(&mut conditions, "import");
            push_condition(&mut conditions, "require");
            push_condition(&mut conditions, "node");
        }
        ModuleResolutionKind::Classic
        | ModuleResolutionKind::Node
        | ModuleResolutionKind::Node16
        | ModuleResolutionKind::NodeNext => {
            push_condition(&mut conditions, "import");
            push_condition(&mut conditions, "require");
            push_condition(&mut conditions, "browser");
        }
    }

    conditions
}

fn push_condition(conditions: &mut Vec<&'static str>, condition: &'static str) {
    if !conditions.contains(&condition) {
        conditions.push(condition);
    }
}

fn resolve_node_module_specifier(
    from_file: &Path,
    module_specifier: &str,
    base_dir: &Path,
    options: &ResolvedCompilerOptions,
) -> Option<PathBuf> {
    let (package_name, subpath) = split_package_specifier(module_specifier)?;
    let conditions = export_conditions(options);
    let mut current = from_file.parent().unwrap_or(base_dir);

    loop {
        // 1. Look for the package itself in node_modules
        let package_root = current.join("node_modules").join(&package_name);
        if package_root.is_dir() {
            let package_json = read_package_json(&package_root.join("package.json"));
            let resolved = resolve_package_specifier(
                &package_root,
                subpath.as_deref(),
                package_json.as_ref(),
                &conditions,
                options,
            );
            if resolved.is_some() {
                return resolved;
            }
        }

        // 2. Look for @types package (if not already looking for one)
        // TypeScript looks up @types/foo for 'foo', and @types/scope__pkg for '@scope/pkg'
        if !package_name.starts_with("@types/") {
            let types_package_name = if let Some(scope_pkg) = package_name.strip_prefix('@') {
                // Scoped package: @scope/pkg -> @types/scope__pkg
                // Skip the '@' (1 char) and replace '/' with '__'
                format!("@types/{}", scope_pkg.replace('/', "__"))
            } else {
                format!("@types/{}", package_name)
            };

            let types_root = current.join("node_modules").join(&types_package_name);
            if types_root.is_dir() {
                let package_json = read_package_json(&types_root.join("package.json"));
                let resolved = resolve_package_specifier(
                    &types_root,
                    subpath.as_deref(),
                    package_json.as_ref(),
                    &conditions,
                    options,
                );
                if resolved.is_some() {
                    return resolved;
                }
            }
        }

        if current == base_dir {
            break;
        }
        let Some(parent) = current.parent() else {
            break;
        };
        current = parent;
    }

    None
}

fn resolve_package_imports_specifier(
    from_file: &Path,
    module_specifier: &str,
    base_dir: &Path,
    options: &ResolvedCompilerOptions,
) -> Option<PathBuf> {
    let conditions = export_conditions(options);
    let mut current = from_file.parent().unwrap_or(base_dir);

    loop {
        let package_json_path = current.join("package.json");
        if package_json_path.is_file()
            && let Some(package_json) = read_package_json(&package_json_path)
            && let Some(imports) = package_json.imports.as_ref()
            && let Some(target) = resolve_imports_subpath(imports, module_specifier, &conditions)
        {
            let package_type = package_type_from_json(Some(&package_json));
            if let Some(resolved) = resolve_package_entry(current, &target, options, package_type) {
                return Some(resolved);
            }
        }

        if current == base_dir {
            break;
        }
        let Some(parent) = current.parent() else {
            break;
        };
        current = parent;
    }

    None
}

fn resolve_package_specifier(
    package_root: &Path,
    subpath: Option<&str>,
    package_json: Option<&PackageJson>,
    conditions: &[&str],
    options: &ResolvedCompilerOptions,
) -> Option<PathBuf> {
    let package_type = package_type_from_json(package_json);
    if let Some(package_json) = package_json {
        if let Some(exports) = package_json.exports.as_ref() {
            let subpath_key = match subpath {
                Some(value) => format!("./{}", value),
                None => ".".to_string(),
            };
            if let Some(target) = resolve_exports_subpath(exports, &subpath_key, conditions)
                && let Some(resolved) =
                    resolve_package_entry(package_root, &target, options, package_type)
            {
                return Some(resolved);
            }
        }

        if let Some(types_versions) = package_json.types_versions.as_ref() {
            let types_subpath = subpath.unwrap_or("index");
            if let Some(resolved) = resolve_types_versions(
                package_root,
                types_subpath,
                types_versions,
                options,
                package_type,
            ) {
                return Some(resolved);
            }
        }
    }

    if let Some(subpath) = subpath {
        return resolve_package_entry(package_root, subpath, options, package_type);
    }

    resolve_package_root(package_root, package_json, options, package_type)
}

fn split_package_specifier(specifier: &str) -> Option<(String, Option<String>)> {
    let mut parts = specifier.split('/');
    let first = parts.next()?;

    if first.starts_with('@') {
        let second = parts.next()?;
        let package = format!("{first}/{second}");
        let rest = parts.collect::<Vec<_>>().join("/");
        let subpath = if rest.is_empty() { None } else { Some(rest) };
        return Some((package, subpath));
    }

    let rest = parts.collect::<Vec<_>>().join("/");
    let subpath = if rest.is_empty() { None } else { Some(rest) };
    Some((first.to_string(), subpath))
}

fn resolve_package_root(
    package_root: &Path,
    package_json: Option<&PackageJson>,
    options: &ResolvedCompilerOptions,
    package_type: Option<PackageType>,
) -> Option<PathBuf> {
    let mut candidates = Vec::new();

    if let Some(package_json) = package_json {
        candidates = collect_package_entry_candidates(package_json);
    }

    if !candidates
        .iter()
        .any(|entry| entry == "index" || entry == "./index")
    {
        candidates.push("index".to_string());
    }

    for entry in candidates {
        if let Some(resolved) = resolve_package_entry(package_root, &entry, options, package_type) {
            return Some(resolved);
        }
    }

    None
}

fn resolve_package_entry(
    package_root: &Path,
    entry: &str,
    options: &ResolvedCompilerOptions,
    package_type: Option<PackageType>,
) -> Option<PathBuf> {
    let entry = entry.trim();
    if entry.is_empty() {
        return None;
    }
    let entry = entry.trim_start_matches("./");
    let path = if Path::new(entry).is_absolute() {
        PathBuf::from(entry)
    } else {
        package_root.join(entry)
    };

    for candidate in expand_module_path_candidates(&path, options, package_type) {
        if candidate.is_file() && is_valid_module_file(&candidate) {
            return Some(canonicalize_or_owned(&candidate));
        }
    }

    None
}

fn package_type_from_json(package_json: Option<&PackageJson>) -> Option<PackageType> {
    let Some(package_json) = package_json else {
        return None;
    };

    match package_json.package_type.as_deref() {
        Some("module") => Some(PackageType::Module),
        Some("commonjs") => Some(PackageType::CommonJs),
        Some(_) => None,
        None => Some(PackageType::CommonJs),
    }
}

fn read_package_json(path: &Path) -> Option<PackageJson> {
    let contents = std::fs::read_to_string(path).ok()?;
    serde_json::from_str(&contents).ok()
}

fn collect_package_entry_candidates(package_json: &PackageJson) -> Vec<String> {
    let mut seen = HashSet::new();
    let mut candidates = Vec::new();

    for value in [package_json.types.as_ref(), package_json.typings.as_ref()]
        .into_iter()
        .flatten()
    {
        if seen.insert(value.clone()) {
            candidates.push(value.clone());
        }
    }

    for value in [package_json.module.as_ref(), package_json.main.as_ref()]
        .into_iter()
        .flatten()
    {
        if seen.insert(value.clone()) {
            candidates.push(value.clone());
        }
    }

    candidates
}

fn resolve_types_versions(
    package_root: &Path,
    subpath: &str,
    types_versions: &serde_json::Value,
    options: &ResolvedCompilerOptions,
    package_type: Option<PackageType>,
) -> Option<PathBuf> {
    let compiler_version = types_versions_compiler_version(options);
    let paths = select_types_versions_paths(types_versions, compiler_version)?;
    let mut best_pattern: Option<&String> = None;
    let mut best_value: Option<&serde_json::Value> = None;
    let mut best_wildcard = String::new();
    let mut best_specificity = 0usize;
    let mut best_len = 0usize;

    for (pattern, value) in paths {
        let Some(wildcard) = match_types_versions_pattern(pattern, subpath) else {
            continue;
        };
        let specificity = types_versions_specificity(pattern);
        let pattern_len = pattern.len();
        let is_better = match best_pattern {
            None => true,
            Some(current) => {
                specificity > best_specificity
                    || (specificity == best_specificity && pattern_len > best_len)
                    || (specificity == best_specificity
                        && pattern_len == best_len
                        && pattern < current)
            }
        };

        if is_better {
            best_specificity = specificity;
            best_len = pattern_len;
            best_pattern = Some(pattern);
            best_value = Some(value);
            best_wildcard = wildcard;
        }
    }

    let Some(value) = best_value else {
        return None;
    };

    let mut targets = Vec::new();
    match value {
        serde_json::Value::String(value) => targets.push(value.as_str()),
        serde_json::Value::Array(list) => {
            for entry in list {
                if let Some(value) = entry.as_str() {
                    targets.push(value);
                }
            }
        }
        _ => {}
    }

    for target in targets {
        let substituted = substitute_path_target(target, &best_wildcard);
        if let Some(resolved) =
            resolve_package_entry(package_root, &substituted, options, package_type)
        {
            return Some(resolved);
        }
    }

    None
}

fn select_types_versions_paths(
    types_versions: &serde_json::Value,
    compiler_version: SemVer,
) -> Option<&serde_json::Map<String, serde_json::Value>> {
    select_types_versions_paths_for_version(types_versions, compiler_version)
}

fn select_types_versions_paths_for_version(
    types_versions: &serde_json::Value,
    compiler_version: SemVer,
) -> Option<&serde_json::Map<String, serde_json::Value>> {
    let map = types_versions.as_object()?;
    let mut best_score: Option<RangeScore> = None;
    let mut best_key: Option<&str> = None;
    let mut best_value: Option<&serde_json::Map<String, serde_json::Value>> = None;

    for (key, value) in map {
        let Some(value_map) = value.as_object() else {
            continue;
        };
        let Some(score) = match_types_versions_range(key, compiler_version) else {
            continue;
        };
        let is_better = match best_score {
            None => true,
            Some(best) => {
                score > best
                    || (score == best && best_key.is_none_or(|best_key| key.as_str() < best_key))
            }
        };

        if is_better {
            best_score = Some(score);
            best_key = Some(key);
            best_value = Some(value_map);
        }
    }

    best_value
}

fn match_types_versions_pattern(pattern: &str, subpath: &str) -> Option<String> {
    if !pattern.contains('*') {
        return if pattern == subpath {
            Some(String::new())
        } else {
            None
        };
    }

    let star = pattern.find('*')?;
    let (prefix, suffix) = pattern.split_at(star);
    let suffix = &suffix[1..];

    if !subpath.starts_with(prefix) || !subpath.ends_with(suffix) {
        return None;
    }

    let start = prefix.len();
    let end = subpath.len().saturating_sub(suffix.len());
    if end < start {
        return None;
    }

    Some(subpath[start..end].to_string())
}

fn types_versions_specificity(pattern: &str) -> usize {
    if let Some(star) = pattern.find('*') {
        star + (pattern.len() - star - 1)
    } else {
        pattern.len()
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
struct RangeScore {
    constraints: usize,
    min_version: SemVer,
    key_len: usize,
}

fn match_types_versions_range(range: &str, compiler_version: SemVer) -> Option<RangeScore> {
    let range = range.trim();
    if range.is_empty() || range == "*" {
        return Some(RangeScore {
            constraints: 0,
            min_version: SemVer::ZERO,
            key_len: range.len(),
        });
    }

    let mut best: Option<RangeScore> = None;
    for segment in range.split("||") {
        let segment = segment.trim();
        let Some(score) =
            match_types_versions_range_segment(segment, compiler_version, range.len())
        else {
            continue;
        };
        if best.is_none_or(|current| score > current) {
            best = Some(score);
        }
    }

    best
}

fn match_types_versions_range_segment(
    segment: &str,
    compiler_version: SemVer,
    key_len: usize,
) -> Option<RangeScore> {
    if segment.is_empty() {
        return None;
    }
    if segment == "*" {
        return Some(RangeScore {
            constraints: 0,
            min_version: SemVer::ZERO,
            key_len,
        });
    }

    let mut min_version = SemVer::ZERO;
    let mut constraints = 0usize;

    for token in segment.split_whitespace() {
        if token.is_empty() || token == "*" {
            continue;
        }
        let (op, version) = parse_range_token(token)?;
        if !compare_range(compiler_version, op, version) {
            return None;
        }
        constraints += 1;
        if matches!(op, RangeOp::Gt | RangeOp::Gte | RangeOp::Eq) && version > min_version {
            min_version = version;
        }
    }

    Some(RangeScore {
        constraints,
        min_version,
        key_len,
    })
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum RangeOp {
    Gt,
    Gte,
    Lt,
    Lte,
    Eq,
}

fn parse_range_token(token: &str) -> Option<(RangeOp, SemVer)> {
    let token = token.trim();
    if token.is_empty() {
        return None;
    }

    let (op, rest) = if let Some(rest) = token.strip_prefix(">=") {
        (RangeOp::Gte, rest)
    } else if let Some(rest) = token.strip_prefix("<=") {
        (RangeOp::Lte, rest)
    } else if let Some(rest) = token.strip_prefix('>') {
        (RangeOp::Gt, rest)
    } else if let Some(rest) = token.strip_prefix('<') {
        (RangeOp::Lt, rest)
    } else if let Some(rest) = token.strip_prefix('=') {
        (RangeOp::Eq, rest)
    } else {
        (RangeOp::Eq, token)
    };

    parse_semver(rest).map(|version| (op, version))
}

fn compare_range(version: SemVer, op: RangeOp, bound: SemVer) -> bool {
    match op {
        RangeOp::Gt => version > bound,
        RangeOp::Gte => version >= bound,
        RangeOp::Lt => version < bound,
        RangeOp::Lte => version <= bound,
        RangeOp::Eq => version == bound,
    }
}

fn parse_semver(value: &str) -> Option<SemVer> {
    let value = value.trim();
    if value.is_empty() {
        return None;
    }
    let core = value.split(['-', '+']).next().unwrap_or(value);
    let mut parts = core.split('.');
    let major: u32 = parts.next()?.parse().ok()?;
    let minor: u32 = parts.next().unwrap_or("0").parse().ok()?;
    let patch: u32 = parts.next().unwrap_or("0").parse().ok()?;
    Some(SemVer {
        major,
        minor,
        patch,
    })
}

fn resolve_exports_subpath(
    exports: &serde_json::Value,
    subpath_key: &str,
    conditions: &[&str],
) -> Option<String> {
    match exports {
        serde_json::Value::String(value) => {
            if subpath_key == "." {
                Some(value.clone())
            } else {
                None
            }
        }
        serde_json::Value::Array(list) => {
            for entry in list {
                if let Some(resolved) = resolve_exports_subpath(entry, subpath_key, conditions) {
                    return Some(resolved);
                }
            }
            None
        }
        serde_json::Value::Object(map) => {
            let has_subpath_keys = map.keys().any(|key| key.starts_with('.'));
            if has_subpath_keys {
                if let Some(value) = map.get(subpath_key)
                    && let Some(target) = resolve_exports_target(value, conditions)
                {
                    return Some(target);
                }

                let mut best_match: Option<(usize, String, &serde_json::Value)> = None;
                for (key, value) in map {
                    let Some(wildcard) = match_exports_subpath(key, subpath_key) else {
                        continue;
                    };
                    let specificity = key.len();
                    let is_better = match &best_match {
                        None => true,
                        Some((best_len, _, _)) => specificity > *best_len,
                    };
                    if is_better {
                        best_match = Some((specificity, wildcard, value));
                    }
                }

                if let Some((_, wildcard, value)) = best_match
                    && let Some(target) = resolve_exports_target(value, conditions)
                {
                    return Some(apply_exports_subpath(&target, &wildcard));
                }

                None
            } else if subpath_key == "." {
                resolve_exports_target(exports, conditions)
            } else {
                None
            }
        }
        _ => None,
    }
}

fn resolve_exports_target(target: &serde_json::Value, conditions: &[&str]) -> Option<String> {
    match target {
        serde_json::Value::String(value) => Some(value.clone()),
        serde_json::Value::Array(list) => {
            for entry in list {
                if let Some(resolved) = resolve_exports_target(entry, conditions) {
                    return Some(resolved);
                }
            }
            None
        }
        serde_json::Value::Object(map) => {
            for condition in conditions {
                if let Some(value) = map.get(*condition)
                    && let Some(resolved) = resolve_exports_target(value, conditions)
                {
                    return Some(resolved);
                }
            }
            None
        }
        _ => None,
    }
}

fn resolve_imports_subpath(
    imports: &serde_json::Value,
    subpath_key: &str,
    conditions: &[&str],
) -> Option<String> {
    let serde_json::Value::Object(map) = imports else {
        return None;
    };

    let has_subpath_keys = map.keys().any(|key| key.starts_with('#'));
    if !has_subpath_keys {
        return None;
    }

    if let Some(value) = map.get(subpath_key) {
        return resolve_exports_target(value, conditions);
    }

    let mut best_match: Option<(usize, String, &serde_json::Value)> = None;
    for (key, value) in map {
        let Some(wildcard) = match_imports_subpath(key, subpath_key) else {
            continue;
        };
        let specificity = key.len();
        let is_better = match &best_match {
            None => true,
            Some((best_len, _, _)) => specificity > *best_len,
        };
        if is_better {
            best_match = Some((specificity, wildcard, value));
        }
    }

    if let Some((_, wildcard, value)) = best_match
        && let Some(target) = resolve_exports_target(value, conditions)
    {
        return Some(apply_exports_subpath(&target, &wildcard));
    }

    None
}

fn match_exports_subpath(pattern: &str, subpath_key: &str) -> Option<String> {
    if !pattern.contains('*') {
        return None;
    }
    let pattern = pattern.strip_prefix("./")?;
    let subpath = subpath_key.strip_prefix("./")?;

    let star = pattern.find('*')?;
    let (prefix, suffix) = pattern.split_at(star);
    let suffix = &suffix[1..];

    if !subpath.starts_with(prefix) || !subpath.ends_with(suffix) {
        return None;
    }

    let start = prefix.len();
    let end = subpath.len().saturating_sub(suffix.len());
    if end < start {
        return None;
    }

    Some(subpath[start..end].to_string())
}

fn match_imports_subpath(pattern: &str, subpath_key: &str) -> Option<String> {
    if !pattern.contains('*') {
        return None;
    }
    let pattern = pattern.strip_prefix('#')?;
    let subpath = subpath_key.strip_prefix('#')?;

    let star = pattern.find('*')?;
    let (prefix, suffix) = pattern.split_at(star);
    let suffix = &suffix[1..];

    if !subpath.starts_with(prefix) || !subpath.ends_with(suffix) {
        return None;
    }

    let start = prefix.len();
    let end = subpath.len().saturating_sub(suffix.len());
    if end < start {
        return None;
    }

    Some(subpath[start..end].to_string())
}

fn apply_exports_subpath(target: &str, wildcard: &str) -> String {
    if target.contains('*') {
        target.replace('*', wildcard)
    } else {
        target.to_string()
    }
}

/// Load lib.d.ts files and create LibContext objects for the checker.
///
/// This function loads the specified lib.d.ts files (e.g., lib.dom.d.ts, lib.es*.d.ts)
/// and returns LibContext objects that can be used by the checker to resolve global
/// symbols like `console`, `Array`, `Promise`, etc.
///
/// If disk files are not available or fail to load, this function falls back to embedded libs
/// for the specified target to ensure global types are always available.
fn load_lib_files_for_contexts(
    lib_files: &[PathBuf],
    target: crate::emitter::ScriptTarget,
) -> Vec<LibContext> {
    use crate::binder::BinderState;
    use crate::lib_loader;
    use crate::parser::ParserState;
    use std::sync::Arc;

    let mut lib_contexts = Vec::new();

    // First, try to load from disk files
    let mut has_core_types = false; // Track if we loaded essential types like Object, Array
    for lib_path in lib_files {
        // Skip if the file doesn't exist
        if !lib_path.exists() {
            continue;
        }

        // Read the lib file content
        let source_text = match std::fs::read_to_string(lib_path) {
            Ok(content) => content,
            Err(_) => continue,
        };

        // Parse the lib file
        let file_name = lib_path.to_string_lossy().to_string();
        let mut lib_parser = ParserState::new(file_name.clone(), source_text);
        let source_file_idx = lib_parser.parse_source_file();

        // Skip if there are parse errors (lib files may use advanced syntax)
        if !lib_parser.get_diagnostics().is_empty() {
            continue;
        }

        // Bind the lib file
        let mut lib_binder = BinderState::new();
        lib_binder.bind_source_file(lib_parser.get_arena(), source_file_idx);

        // Check if this lib has core types
        if lib_binder.file_locals.has("Object") && lib_binder.file_locals.has("Array") {
            has_core_types = true;
        }

        // Create the LibContext
        let arena = Arc::new(lib_parser.into_arena());
        let binder = Arc::new(lib_binder);

        lib_contexts.push(LibContext { arena, binder });
    }

    // If no disk files were loaded OR core types are missing, fall back to embedded libs
    // This ensures global types are always available even when disk files fail to parse
    // IMPORTANT: Only load embedded libs if lib_files was not intentionally empty (i.e., noLib is false)
    // When lib_files is empty and we tried to load disk files, it means either:
    // 1. noLib is true (don't load ANY libs)
    // 2. Disk files don't exist (load embedded libs as fallback)
    let should_fallback_to_embedded = !lib_files.is_empty() || !lib_contexts.is_empty();
    if (lib_contexts.is_empty() || !has_core_types) && should_fallback_to_embedded {
        // Load embedded libs using the actual target from compiler options
        let config = lib_loader::LibResolverConfig::new(target).with_include_dom(true);
        let embedded_libs = lib_loader::resolve_libs(&config);

        // Add embedded libs to provide missing types
        for lib_file in embedded_libs {
            lib_contexts.push(LibContext {
                arena: lib_file.arena.clone(),
                binder: lib_file.binder.clone(),
            });
        }
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

    for file in &program.files {
        let canonical = canonicalize_or_owned(Path::new(&file.file_name));
        program_paths.insert(canonical);
        canonical_to_file_name.insert(
            canonicalize_or_owned(Path::new(&file.file_name)),
            file.file_name.clone(),
        );
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
        checker.check_source_file(file.source_file);
        file_diagnostics.extend(std::mem::take(&mut checker.ctx.diagnostics));
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

fn emit_outputs(
    program: &MergedProgram,
    options: &ResolvedCompilerOptions,
    base_dir: &Path,
    root_dir: Option<&Path>,
    out_dir: Option<&Path>,
    declaration_dir: Option<&Path>,
    dirty_paths: Option<&HashSet<PathBuf>>,
) -> Result<Vec<OutputFile>> {
    let mut outputs = Vec::new();
    let new_line = new_line_str(options.printer.new_line);

    for file in &program.files {
        let input_path = PathBuf::from(&file.file_name);
        if let Some(dirty_paths) = dirty_paths
            && !dirty_paths.contains(&input_path)
        {
            continue;
        }

        if let Some(js_path) = js_output_path(base_dir, root_dir, out_dir, options.jsx, &input_path)
        {
            let mut printer = Printer::with_options(&file.arena, options.printer.clone());
            let map_info = if options.source_map {
                map_output_info(&js_path)
            } else {
                None
            };

            if let Some((_, _, output_name)) = map_info.as_ref() {
                if let Some(source_text) = file
                    .arena
                    .get(file.source_file)
                    .and_then(|node| file.arena.get_source_file(node))
                    .map(|source| source.text.as_ref())
                {
                    printer.set_source_map_text(source_text);
                }
                printer.enable_source_map(output_name, &file.file_name);
            }

            printer.emit(file.source_file);
            let map_json = map_info
                .as_ref()
                .and_then(|_| printer.generate_source_map_json());
            let mut contents = printer.take_output();
            let mut map_output = None;

            if let Some((map_path, map_name, _)) = map_info
                && let Some(map_json) = map_json
            {
                append_source_mapping_url(&mut contents, &map_name, new_line);
                map_output = Some(OutputFile {
                    path: map_path,
                    contents: map_json,
                });
            }

            outputs.push(OutputFile {
                path: js_path,
                contents,
            });
            if let Some(map_output) = map_output {
                outputs.push(map_output);
            }
        }

        if options.emit_declarations {
            let decl_base = declaration_dir.or(out_dir);
            if let Some(dts_path) =
                declaration_output_path(base_dir, root_dir, decl_base, &input_path)
            {
                let mut emitter = DeclarationEmitter::new(&file.arena);
                let map_info = if options.declaration_map {
                    map_output_info(&dts_path)
                } else {
                    None
                };

                if let Some((_, _, output_name)) = map_info.as_ref() {
                    if let Some(source_text) = file
                        .arena
                        .get(file.source_file)
                        .and_then(|node| file.arena.get_source_file(node))
                        .map(|source| source.text.as_ref())
                    {
                        emitter.set_source_map_text(source_text);
                    }
                    emitter.enable_source_map(output_name, &file.file_name);
                }

                let mut contents = emitter.emit(file.source_file);
                let map_json = map_info
                    .as_ref()
                    .and_then(|_| emitter.generate_source_map_json());
                let mut map_output = None;

                if let Some((map_path, map_name, _)) = map_info
                    && let Some(map_json) = map_json
                {
                    append_source_mapping_url(&mut contents, &map_name, new_line);
                    map_output = Some(OutputFile {
                        path: map_path,
                        contents: map_json,
                    });
                }

                outputs.push(OutputFile {
                    path: dts_path,
                    contents,
                });
                if let Some(map_output) = map_output {
                    outputs.push(map_output);
                }
            }
        }
    }

    Ok(outputs)
}

fn map_output_info(output_path: &Path) -> Option<(PathBuf, String, String)> {
    let output_name = output_path.file_name()?.to_string_lossy().into_owned();
    let map_name = format!("{output_name}.map");
    let map_path = output_path.with_file_name(&map_name);
    Some((map_path, map_name, output_name))
}

fn append_source_mapping_url(contents: &mut String, map_name: &str, new_line: &str) {
    if !contents.is_empty() && !contents.ends_with(new_line) {
        contents.push_str(new_line);
    }
    contents.push_str("//# sourceMappingURL=");
    contents.push_str(map_name);
}

fn new_line_str(kind: NewLineKind) -> &'static str {
    match kind {
        NewLineKind::LineFeed => "\n",
        NewLineKind::CarriageReturnLineFeed => "\r\n",
    }
}

fn write_outputs(outputs: &[OutputFile]) -> Result<Vec<PathBuf>> {
    outputs.par_iter().try_for_each(|output| -> Result<()> {
        if let Some(parent) = output.path.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("failed to create directory {}", parent.display()))?;
        }
        std::fs::write(&output.path, &output.contents)
            .with_context(|| format!("failed to write {}", output.path.display()))?;
        Ok(())
    })?;

    Ok(outputs.iter().map(|output| output.path.clone()).collect())
}

fn js_output_path(
    base_dir: &Path,
    root_dir: Option<&Path>,
    out_dir: Option<&Path>,
    jsx: Option<JsxEmit>,
    input_path: &Path,
) -> Option<PathBuf> {
    if is_declaration_file(input_path) {
        return None;
    }

    let extension = js_extension_for(input_path, jsx)?;
    let relative = output_relative_path(base_dir, root_dir, input_path);
    let mut output = match out_dir {
        Some(out_dir) => out_dir.join(relative),
        None => input_path.to_path_buf(),
    };
    output.set_extension(extension);
    Some(output)
}

fn declaration_output_path(
    base_dir: &Path,
    root_dir: Option<&Path>,
    out_dir: Option<&Path>,
    input_path: &Path,
) -> Option<PathBuf> {
    if is_declaration_file(input_path) {
        return None;
    }

    let relative = output_relative_path(base_dir, root_dir, input_path);
    let file_name = relative.file_name()?.to_str()?;
    let new_name = declaration_file_name(file_name)?;

    let mut output = match out_dir {
        Some(out_dir) => out_dir.join(relative),
        None => input_path.to_path_buf(),
    };
    output.set_file_name(new_name);
    Some(output)
}

fn output_relative_path(base_dir: &Path, root_dir: Option<&Path>, input_path: &Path) -> PathBuf {
    if let Some(root_dir) = root_dir
        && let Ok(relative) = input_path.strip_prefix(root_dir)
    {
        return relative.to_path_buf();
    }

    input_path
        .strip_prefix(base_dir)
        .unwrap_or(input_path)
        .to_path_buf()
}

fn declaration_file_name(file_name: &str) -> Option<String> {
    if file_name.ends_with(".mts") {
        return Some(file_name.trim_end_matches(".mts").to_string() + ".d.mts");
    }
    if file_name.ends_with(".cts") {
        return Some(file_name.trim_end_matches(".cts").to_string() + ".d.cts");
    }
    if file_name.ends_with(".tsx") {
        return Some(file_name.trim_end_matches(".tsx").to_string() + ".d.ts");
    }
    if file_name.ends_with(".ts") {
        return Some(file_name.trim_end_matches(".ts").to_string() + ".d.ts");
    }

    None
}

fn is_declaration_file(path: &Path) -> bool {
    let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
        return false;
    };

    name.ends_with(".d.ts") || name.ends_with(".d.mts") || name.ends_with(".d.cts")
}

fn js_extension_for(path: &Path, jsx: Option<JsxEmit>) -> Option<&'static str> {
    let name = path.file_name().and_then(|name| name.to_str())?;
    if name.ends_with(".mts") {
        return Some("mjs");
    }
    if name.ends_with(".cts") {
        return Some("cjs");
    }

    match path.extension().and_then(|ext| ext.to_str()) {
        Some("ts") => Some("js"),
        Some("tsx") => match jsx {
            Some(JsxEmit::Preserve) => Some("jsx"),
            Some(JsxEmit::ReactNative) | None => Some("js"),
        },
        _ => None,
    }
}

pub(crate) fn normalize_base_url(base_dir: &Path, dir: Option<PathBuf>) -> Option<PathBuf> {
    dir.map(|dir| {
        let resolved = if dir.is_absolute() {
            dir
        } else {
            base_dir.join(dir)
        };
        canonicalize_or_owned(&resolved)
    })
}

pub(crate) fn normalize_output_dir(base_dir: &Path, dir: Option<PathBuf>) -> Option<PathBuf> {
    dir.map(|dir| {
        if dir.is_absolute() {
            dir
        } else {
            base_dir.join(dir)
        }
    })
}

pub(crate) fn normalize_root_dir(base_dir: &Path, dir: Option<PathBuf>) -> Option<PathBuf> {
    dir.map(|dir| {
        let resolved = if dir.is_absolute() {
            dir
        } else {
            base_dir.join(dir)
        };
        canonicalize_or_owned(&resolved)
    })
}

fn normalize_type_roots(base_dir: &Path, roots: Option<Vec<PathBuf>>) -> Option<Vec<PathBuf>> {
    let roots = roots?;
    let mut normalized = Vec::new();
    for root in roots {
        let resolved = if root.is_absolute() {
            root
        } else {
            base_dir.join(root)
        };
        let resolved = canonicalize_or_owned(&resolved);
        if resolved.is_dir() {
            normalized.push(resolved);
        }
    }
    Some(normalized)
}

fn canonicalize_or_owned(path: &Path) -> PathBuf {
    std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
}

fn env_flag(name: &str) -> bool {
    let Ok(value) = std::env::var(name) else {
        return false;
    };
    let normalized = value.trim().to_ascii_lowercase();
    matches!(normalized.as_str(), "1" | "true" | "yes" | "on")
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
    }
    if args.no_emit {
        options.no_emit = true;
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

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
            else if is_lib_file(&source.path) {
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
    #[allow(dead_code)]
    merge_calls: u32,
}

fn build_program_with_cache(
    sources: Vec<SourceEntry>,
    cache: &mut CompilationCache,
    lib_files: &[Arc<LibFile>],
    language_version: ScriptTarget,
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
        parallel::parse_and_bind_parallel_with_libs_and_target(
            to_parse,
            lib_files,
            language_version,
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




use check::{
    CollectDiagnosticsInput, collect_diagnostics_with_source_resolutions, load_checker_libs,
};


use config_deprecation::NoEmitDeprecationInput;


pub use plan::apply_cli_overrides;

use plan::{
    apply_cli_overrides_with_config_options, cli_ignore_deprecations_silences_6_0,
    display_relative_to_dir, find_latest_dts_file, implicit_common_source_directory,
    is_deprecation_diagnostic_code, is_removed_option_diagnostic_code,
    is_removed_option_value_diagnostic_code, validate_cli_compiler_option_diagnostics,
};

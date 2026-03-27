//! Source file I/O, config helpers, and file reading for the compilation driver.

use super::*;

/// Count how many `node_modules` segments appear in a file path.
/// For example, `/a/node_modules/b/node_modules/c/index.js` has depth 2.
fn node_modules_depth(path: &Path) -> u32 {
    path.components()
        .filter(|c| c.as_os_str() == "node_modules")
        .count() as u32
}

/// Check if a JS file should be skipped due to `maxNodeModuleJsDepth`.
/// Returns true if the file is a `.js` file inside `node_modules` and its
/// nesting depth exceeds the allowed maximum.
fn should_skip_js_in_node_modules(path: &Path, max_depth: u32) -> bool {
    if !is_js_file(path) {
        return false;
    }
    let depth = node_modules_depth(path);
    if depth == 0 {
        return false;
    }
    depth > max_depth
}

/// Result of reading a source file - either valid text or binary/unreadable
#[derive(Debug, Clone)]
pub enum FileReadResult {
    /// File was successfully read as UTF-8 text
    Text(String),
    /// File appears to be binary (emit TS1490), with best-effort text retained.
    Binary {
        text: String,
        suppress_parser_diagnostics: bool,
    },
    /// File could not be read (I/O error)
    Error(String),
}

/// Read a source file, detecting binary files that should emit TS1490.
///
/// TypeScript detects binary files by checking for:
/// - UTF-16 BOM (FE FF for BE, FF FE for LE)
/// - Non-valid UTF-8 sequences
/// - Many control bytes (not expected in source files)
/// - Files with many null bytes
pub fn read_source_file(path: &Path) -> FileReadResult {
    // Read as bytes first
    let bytes = match std::fs::read(path) {
        Ok(b) => b,
        Err(e) => return FileReadResult::Error(e.to_string()),
    };

    // Check for UTF-16 BOM
    // UTF-16 BE: FE FF
    // UTF-16 LE: FF FE
    if bytes.len() >= 2 {
        if bytes[0] == 0xFE && bytes[1] == 0xFF {
            // Decode UTF-16 BE
            let u16_words: Vec<u16> = bytes[2..]
                .chunks_exact(2)
                .map(|chunk| {
                    if chunk.len() == 2 {
                        u16::from_be_bytes([chunk[0], chunk[1]])
                    } else {
                        0
                    }
                })
                .collect();
            return FileReadResult::Text(String::from_utf16_lossy(&u16_words));
        } else if bytes[0] == 0xFF && bytes[1] == 0xFE {
            // Decode UTF-16 LE
            let u16_words: Vec<u16> = bytes[2..]
                .chunks_exact(2)
                .map(|chunk| {
                    if chunk.len() == 2 {
                        u16::from_le_bytes([chunk[0], chunk[1]])
                    } else {
                        0
                    }
                })
                .collect();
            return FileReadResult::Text(String::from_utf16_lossy(&u16_words));
        }
    }

    // Check for binary indicators
    if let Some(suppress_parser_diagnostics) = classify_binary_file(&bytes) {
        return FileReadResult::Binary {
            text: String::from_utf8_lossy(&bytes).to_string(),
            suppress_parser_diagnostics,
        };
    }

    // Try to decode as UTF-8
    match String::from_utf8(bytes) {
        Ok(text) => FileReadResult::Text(text),
        Err(err) => FileReadResult::Binary {
            text: String::from_utf8_lossy(err.as_bytes()).to_string(),
            suppress_parser_diagnostics: true,
        },
    }
}

/// Check if file content appears to be binary (not valid source code).
///
/// Matches TypeScript's binary detection:
/// - UTF-16 BOM at start
/// - Many consecutive null bytes (embedded binaries, corrupted files)
/// - Repeated control bytes in first 1024 bytes
pub(super) fn classify_binary_file(bytes: &[u8]) -> Option<bool> {
    if bytes.is_empty() {
        return None;
    }

    // Check for many null bytes (binary file indicator)
    // TypeScript considers files with many nulls as binary
    let null_count = bytes.iter().take(1024).filter(|&&b| b == 0).count();
    if null_count > 10 {
        return Some(true);
    }

    // Check for consecutive null bytes (UTF-16 or binary)
    // UTF-16 text will have null bytes between ASCII characters
    let mut consecutive_nulls = 0;
    for &byte in bytes.iter().take(512) {
        if byte == 0 {
            consecutive_nulls += 1;
            if consecutive_nulls >= 4 {
                return Some(true);
            }
        } else {
            consecutive_nulls = 0;
        }
    }

    // Check for non-whitespace control bytes.
    // Preserve parser diagnostics for this softer case: tsc still reports TS1490,
    // but malformed-text recovery can also produce real scanner/parser diagnostics.
    let control_count = bytes
        .iter()
        .take(1024)
        .filter(|&&b| {
            b < 0x20 && b != b'\t' && b != b'\n' && b != b'\r' && b != b'\x0C' && b != b'\x0B'
        })
        .count();
    if control_count >= 4 {
        return Some(soft_control_binary_should_suppress(bytes));
    }

    None
}

fn soft_control_binary_should_suppress(bytes: &[u8]) -> bool {
    let payload = bytes
        .iter()
        .rposition(|&b| b == b'\n')
        .map_or(bytes, |idx| &bytes[idx + 1..]);
    let printable_ascii_count = payload.iter().filter(|&&b| b.is_ascii_graphic()).count();

    printable_ascii_count < 2
}

#[derive(Debug, Clone)]
pub(super) struct SourceEntry {
    pub(super) path: PathBuf,
    pub(super) text: Option<String>,
    /// If true, this file appears to be binary (emit TS1490)
    pub(super) is_binary: bool,
    /// If true, suppress parser diagnostics and keep only TS1490 for this file.
    pub(super) suppress_parser_diagnostics: bool,
}

pub(super) fn sources_have_no_default_lib(sources: &[SourceEntry]) -> bool {
    sources.iter().any(source_has_no_default_lib)
}

pub(super) fn source_has_no_default_lib(source: &SourceEntry) -> bool {
    if let Some(text) = source.text.as_deref() {
        return has_no_default_lib_directive(text);
    }
    let Ok(text) = std::fs::read_to_string(&source.path) else {
        return false;
    };
    has_no_default_lib_directive(&text)
}

pub(super) fn has_no_default_lib_directive(source: &str) -> bool {
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

pub(super) fn sources_have_no_types_and_symbols(sources: &[SourceEntry]) -> bool {
    sources.iter().any(source_has_no_types_and_symbols)
}

pub(super) fn source_has_no_types_and_symbols(source: &SourceEntry) -> bool {
    if let Some(text) = source.text.as_deref() {
        return has_no_types_and_symbols_directive(text);
    }
    let Ok(text) = std::fs::read_to_string(&source.path) else {
        return false;
    };
    has_no_types_and_symbols_directive(&text)
}

pub(crate) fn has_no_types_and_symbols_directive(source: &str) -> bool {
    for line in source.lines().take(32) {
        let trimmed = line.trim_start();
        if !trimmed.starts_with("//") {
            continue;
        }

        let lower = trimmed.to_ascii_lowercase();
        let Some(idx) = lower.find("@notypesandsymbols") else {
            continue;
        };

        let mut rest = &trimmed[idx + "@noTypesAndSymbols".len()..];
        rest = rest.trim_start();
        if !rest.starts_with(':') {
            continue;
        }
        rest = rest[1..].trim_start();

        let value = rest
            .split(|c: char| c == ',' || c == ';' || c.is_whitespace())
            .find(|s| !s.is_empty())
            .unwrap_or("");
        return value.eq_ignore_ascii_case("true");
    }
    false
}

pub(super) fn parse_reference_no_default_lib_value(line: &str) -> Option<bool> {
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

pub(super) struct SourceReadResult {
    pub(super) sources: Vec<SourceEntry>,
    pub(super) dependencies: FxHashMap<PathBuf, FxHashSet<PathBuf>>,
    /// Tuples of (`file_path`, `type_name`, `byte_offset_of_types_attr`, `span_length`).
    pub(super) type_reference_errors: Vec<(PathBuf, String, usize, usize)>,
    /// TS1453: Invalid `resolution-mode` values in `/// <reference types="..." />` directives.
    /// Tuples of (`file_path`, `byte_offset`, `span_length`).
    pub(super) resolution_mode_errors: Vec<(PathBuf, usize, usize)>,
}

pub(crate) fn find_tsconfig(cwd: &Path) -> Option<PathBuf> {
    let candidate = cwd.join("tsconfig.json");
    candidate.is_file().then(|| normalize_path(&candidate))
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

    Ok(Some(normalize_path(&candidate)))
}

pub(crate) fn load_config(path: Option<&Path>) -> Result<Option<TsConfig>> {
    let Some(path) = path else {
        return Ok(None);
    };

    let config = load_tsconfig(path)?;
    Ok(Some(config))
}

/// Return type for config loading that includes removed-but-honored suppress flags.
pub(crate) struct LoadedConfig {
    pub config: Option<TsConfig>,
    pub diagnostics: Vec<Diagnostic>,
    pub suppress_excess_property_errors: bool,
    pub suppress_implicit_any_index_errors: bool,
    pub no_implicit_use_strict: bool,
}

pub(crate) fn load_config_with_diagnostics(path: Option<&Path>) -> Result<LoadedConfig> {
    let Some(path) = path else {
        return Ok(LoadedConfig {
            config: None,
            diagnostics: Vec::new(),
            suppress_excess_property_errors: false,
            suppress_implicit_any_index_errors: false,
            no_implicit_use_strict: false,
        });
    };

    let parsed = load_tsconfig_with_diagnostics(path)?;
    Ok(LoadedConfig {
        config: Some(parsed.config),
        diagnostics: parsed.diagnostics,
        suppress_excess_property_errors: parsed.suppress_excess_property_errors,
        suppress_implicit_any_index_errors: parsed.suppress_implicit_any_index_errors,
        no_implicit_use_strict: parsed.no_implicit_use_strict,
    })
}

pub(crate) fn config_base_dir(cwd: &Path, tsconfig_path: Option<&Path>) -> PathBuf {
    tsconfig_path
        .and_then(|path| path.parent().map(Path::to_path_buf))
        .unwrap_or_else(|| cwd.to_path_buf())
}

pub(super) fn build_discovery_options(
    args: &CliArgs,
    base_dir: &Path,
    tsconfig_path: Option<&Path>,
    config: Option<&TsConfig>,
    out_dir: Option<&Path>,
    resolved: &ResolvedCompilerOptions,
) -> Result<FileDiscoveryOptions> {
    let follow_links = env_flag("TSZ_FOLLOW_SYMLINKS") && !resolved.preserve_symlinks;
    if !args.files.is_empty() {
        return Ok(FileDiscoveryOptions {
            base_dir: base_dir.to_path_buf(),
            files: args.files.clone(),
            files_explicitly_set: true,
            include: None,
            exclude: None,
            out_dir: out_dir.map(Path::to_path_buf),
            follow_links,
            allow_js: resolved.allow_js,
            resolve_json_module: resolved.resolve_json_module,
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
    options.allow_js = resolved.allow_js;
    options.resolve_json_module = resolved.resolve_json_module;
    Ok(options)
}

/// Returns (resolved files, unresolved type names from tsconfig `types` array).
pub(super) fn collect_type_root_files(
    base_dir: &Path,
    options: &ResolvedCompilerOptions,
) -> (Vec<PathBuf>, Vec<String>) {
    if options.checker.no_types_and_symbols {
        return (Vec::new(), Vec::new());
    }

    let roots = match options.type_roots.as_ref() {
        Some(roots) => roots.clone(),
        None => default_type_roots(base_dir),
    };
    if roots.is_empty() {
        // When no valid type roots exist, try to resolve explicitly requested types
        // via node_modules fallback before marking them as unresolved.
        let mut files = std::collections::BTreeSet::new();
        let mut unresolved = Vec::new();
        if let Some(types) = options.types.as_ref() {
            let synthetic_from_file = base_dir.join("__types__.ts");
            for name in types {
                if name.as_str() == "*" || name.trim().is_empty() {
                    continue;
                }
                if let Some(entry) =
                    crate::driver::resolution::resolve_type_reference_from_node_modules(
                        name,
                        &synthetic_from_file,
                        base_dir,
                        None,
                        options,
                    )
                {
                    files.insert(entry);
                } else {
                    unresolved.push(name.clone());
                }
            }
        }
        return (files.into_iter().collect(), unresolved);
    }

    let mut files = std::collections::BTreeSet::new();
    if let Some(types) = options.types.as_ref() {
        // Filter out "*" wildcard — it means "include all type packages"
        // rather than a literal package name. When present, fall through
        // to the auto-discovery path below.
        let has_wildcard = types.iter().any(|t| t == "*" || t.trim().is_empty());
        if !has_wildcard {
            let mut unresolved = Vec::new();
            let synthetic_from_file = base_dir.join("__types__.ts");
            for name in types {
                if let Some(entry) = resolve_type_package_from_roots(name, &roots, options) {
                    files.insert(entry);
                } else if let Some(entry) =
                    crate::driver::resolution::resolve_type_reference_from_node_modules(
                        name,
                        &synthetic_from_file,
                        base_dir,
                        None,
                        options,
                    )
                {
                    // Found via node_modules fallback — include the file but don't
                    // report TS2688 since the package exists, just not in @types/.
                    files.insert(entry);
                } else {
                    unresolved.push(name.clone());
                }
            }
            return (files.into_iter().collect(), unresolved);
        }
    }

    for root in roots {
        for package_root in collect_type_packages_from_root(&root) {
            if let Some(entry) = resolve_type_package_entry(&package_root, options) {
                files.insert(entry);
            }
        }
    }

    (files.into_iter().collect(), Vec::new())
}

pub(super) fn read_source_files(
    paths: &[PathBuf],
    base_dir: &Path,
    options: &ResolvedCompilerOptions,
    cache: Option<&CompilationCache>,
    changed_paths: Option<&FxHashSet<PathBuf>>,
) -> Result<SourceReadResult> {
    let mut sources: FxHashMap<PathBuf, (Option<String>, bool, bool)> = FxHashMap::default(); // (text, is_binary, suppress_parser_diagnostics)
    let mut dependencies: FxHashMap<PathBuf, FxHashSet<PathBuf>> = FxHashMap::default();
    let mut seen = FxHashSet::default();
    let mut pending = VecDeque::new();
    let mut resolution_cache = ModuleResolutionCache::default();
    let mut module_resolver = ModuleResolver::new(options);
    let mut type_reference_errors = Vec::new();
    let mut resolution_mode_errors = Vec::new();
    let use_cache = cache.is_some() && changed_paths.is_some();

    for path in paths {
        let canonical = normalize_resolved_path(path, options);
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
            sources.insert(path.clone(), (None, false, false)); // Cached files are not binary
            for dep in cached_deps {
                if seen.insert(dep.clone()) {
                    pending.push_back(dep.clone());
                }
            }
            continue;
        }

        // Skip JS files in node_modules that exceed maxNodeModuleJsDepth.
        // These files are recorded as dependencies but treated as untyped
        // (no parsing, no import resolution). This matches tsc's behavior:
        // with the default maxNodeModuleJsDepth=0, JS files inside node_modules
        // are never parsed.
        if should_skip_js_in_node_modules(&path, options.max_node_module_js_depth) {
            sources.insert(path.clone(), (None, false, false));
            continue;
        }

        // Read file with binary detection
        let (text, is_binary, suppress_parser_diagnostics) = match read_source_file(&path) {
            FileReadResult::Text(t) => (t, false, false),
            FileReadResult::Binary {
                text,
                suppress_parser_diagnostics,
            } => (text, true, suppress_parser_diagnostics),
            FileReadResult::Error(e) => {
                return Err(anyhow::anyhow!("failed to read {}: {}", path.display(), e));
            }
        };
        let specifiers = if is_binary {
            Vec::new()
        } else {
            crate::driver::resolution::collect_module_requests_from_text(&path, &text)
        };
        let type_refs = if is_binary {
            Vec::new()
        } else {
            tsz::checker::triple_slash_validator::extract_reference_types(&text)
        };
        let reference_paths = if is_binary || options.no_resolve {
            vec![]
        } else {
            tsz::checker::triple_slash_validator::extract_reference_paths(&text)
        };

        sources.insert(
            path.clone(),
            (Some(text), is_binary, suppress_parser_diagnostics),
        );
        let entry = dependencies.entry(path.clone()).or_default();

        if !options.no_resolve {
            for (specifier, import_kind, resolution_mode_override) in specifiers {
                let request = tsz::module_resolver::ModuleLookupRequest {
                    specifier: &specifier,
                    containing_file: &path,
                    specifier_span: tsz_common::Span::new(0, 0),
                    import_kind,
                    resolution_mode_override,
                    no_implicit_any: options.checker.no_implicit_any,
                    implied_classic_resolution: options.checker.implied_classic_resolution,
                };
                let outcome = module_resolver
                    .lookup(
                        &request,
                        |spec, fp| {
                            resolve_module_specifier(
                                fp,
                                spec,
                                options,
                                base_dir,
                                &mut resolution_cache,
                                &seen,
                            )
                        },
                        |_| false,
                    )
                    .classify();
                if let Some(resolved) = outcome.resolved_path {
                    let canonical = normalize_resolved_path(&resolved, options);
                    entry.insert(canonical.clone());
                    if seen.insert(canonical.clone()) {
                        pending.push_back(canonical);
                    }
                }
            }
        }

        // Resolve /// <reference types="..." /> directives
        if !type_refs.is_empty() && !options.no_resolve && !options.checker.no_types_and_symbols {
            let type_roots = options
                .type_roots
                .clone()
                .unwrap_or_else(|| default_type_roots(base_dir));
            for (type_name, resolution_mode, types_offset, types_len) in type_refs {
                // TS1453: Validate resolution-mode attribute value.
                // tsc anchors this diagnostic at the `types` attribute value span.
                // When invalid, tsc resolves the type reference without an explicit
                // mode. Empirically, tsc includes the package such that globals from
                // all export conditions are available. We emulate this by resolving
                // with both "import" and "require" conditions.
                let invalid_mode = if let Some(ref mode) = resolution_mode
                    && mode != "import"
                    && mode != "require"
                {
                    resolution_mode_errors.push((path.clone(), types_offset, types_len));
                    true
                } else {
                    false
                };
                let effective_resolution_mode = if invalid_mode {
                    None
                } else {
                    resolution_mode.as_ref()
                };
                let resolved =
                    if let Some(mode) = effective_resolution_mode {
                        // With explicit resolution-mode, use exports map with the specified condition
                        let candidates =
                            crate::driver::resolution::type_package_candidates_pub(&type_name);
                        let mut result = None;
                        for root in &type_roots {
                            for candidate in &candidates {
                                let package_root = root.join(candidate);
                                if package_root.is_dir()
                                    && let Some(entry) =
                                    crate::driver::resolution::resolve_type_package_entry_with_mode(
                                        &package_root, mode, options,
                                    )
                                {
                                    result = Some(entry);
                                    break;
                                }
                            }
                            if result.is_some() {
                                break;
                            }
                        }
                        result
                    } else {
                        resolve_type_package_from_roots(&type_name, &type_roots, options)
                    };
                // If type roots resolution failed, fall back to searching node_modules/
                // directly. tsc's resolveTypeReferenceDirective always uses node_modules
                // walk-up as a secondary fallback after typeRoots, regardless of the
                // configured module resolution mode (including Classic).
                let resolved = resolved.or_else(|| {
                    crate::driver::resolution::resolve_type_reference_from_node_modules(
                        &type_name,
                        &path,
                        base_dir,
                        effective_resolution_mode.map(|s| s.as_str()),
                        options,
                    )
                });
                if let Some(resolved) = resolved {
                    let canonical = normalize_resolved_path(&resolved, options);
                    entry.insert(canonical.clone());
                    if seen.insert(canonical.clone()) {
                        pending.push_back(canonical);
                    }
                } else if !invalid_mode {
                    type_reference_errors.push((
                        path.clone(),
                        type_name.clone(),
                        types_offset,
                        types_len,
                    ));
                }
                // When resolution-mode is invalid, also try the other condition
                // so that globals from both export paths are available.
                // tsc appears to make all globals available in this case.
                if invalid_mode {
                    for mode in &["import", "require"] {
                        if let Some(alt) =
                            crate::driver::resolution::resolve_type_reference_from_node_modules(
                                &type_name,
                                &path,
                                base_dir,
                                Some(mode),
                                options,
                            )
                        {
                            let canonical = normalize_resolved_path(&alt, options);
                            entry.insert(canonical.clone());
                            if seen.insert(canonical.clone()) {
                                pending.push_back(canonical);
                            }
                        }
                    }
                }
            }
        }

        // Resolve /// <reference path="..." /> directives
        if !reference_paths.is_empty() {
            let base_dir = path.parent().unwrap_or_else(|| Path::new(""));
            for (reference_path, _line_num, _quote_offset) in reference_paths {
                if reference_path.is_empty() {
                    continue;
                }
                let mut candidates = Vec::new();
                let direct_reference = base_dir.join(&reference_path);
                candidates.push(direct_reference);
                if !reference_path.contains('.') {
                    for ext in [".ts", ".tsx", ".d.ts"] {
                        candidates.push(base_dir.join(format!("{reference_path}{ext}")));
                    }
                }

                let Some(resolved_reference) = candidates
                    .iter()
                    .find(|candidate| candidate.is_file())
                    .map(|candidate| normalize_resolved_path(candidate, options))
                else {
                    continue;
                };
                entry.insert(resolved_reference.clone());
                if seen.insert(resolved_reference.clone()) {
                    pending.push_back(resolved_reference);
                }
            }
        }
    }

    let mut list: Vec<SourceEntry> = sources
        .into_iter()
        .map(
            |(path, (text, is_binary, suppress_parser_diagnostics))| SourceEntry {
                path,
                text,
                is_binary,
                suppress_parser_diagnostics,
            },
        )
        .collect();
    list.sort_by(|left, right| {
        left.path
            .to_string_lossy()
            .cmp(&right.path.to_string_lossy())
    });
    Ok(SourceReadResult {
        sources: list,
        dependencies,
        type_reference_errors,
        resolution_mode_errors,
    })
}

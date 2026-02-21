//! Source file I/O, config helpers, and file reading for the compilation driver.

use super::*;

/// Result of reading a source file - either valid text or binary/unreadable
#[derive(Debug, Clone)]
pub enum FileReadResult {
    /// File was successfully read as UTF-8 text
    Text(String),
    /// File appears to be binary (emit TS1490), but keep best-effort text for parser diagnostics
    Binary(String),
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
    if is_binary_file(&bytes) {
        return FileReadResult::Binary(String::from_utf8_lossy(&bytes).to_string());
    }

    // Try to decode as UTF-8
    match String::from_utf8(bytes) {
        Ok(text) => FileReadResult::Text(text),
        Err(err) => FileReadResult::Binary(String::from_utf8_lossy(err.as_bytes()).to_string()),
    }
}

/// Check if file content appears to be binary (not valid source code).
///
/// Matches TypeScript's binary detection:
/// - UTF-16 BOM at start
/// - Many consecutive null bytes (embedded binaries, corrupted files)
/// - Repeated control bytes in first 1024 bytes
pub(super) fn is_binary_file(bytes: &[u8]) -> bool {
    if bytes.is_empty() {
        return false;
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

    // Check for non-whitespace control bytes (e.g. U+0000/Control-Range from garbled UTF-16 read as UTF-8)
    let control_count = bytes
        .iter()
        .take(1024)
        .filter(|&&b| {
            b < 0x20 && b != b'\t' && b != b'\n' && b != b'\r' && b != b'\x0C' && b != b'\x0B'
        })
        .count();
    if control_count >= 4 {
        return true;
    }

    false
}

#[derive(Debug, Clone)]
pub(super) struct SourceEntry {
    pub(super) path: PathBuf,
    pub(super) text: Option<String>,
    /// If true, this file appears to be binary (emit TS1490)
    pub(super) is_binary: bool,
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
    pub(super) type_reference_errors: Vec<(PathBuf, String)>,
}

pub(crate) fn find_tsconfig(cwd: &Path) -> Option<PathBuf> {
    let candidate = cwd.join("tsconfig.json");
    candidate
        .is_file()
        .then(|| canonicalize_or_owned(&candidate))
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

pub(crate) fn load_config_with_diagnostics(
    path: Option<&Path>,
) -> Result<(Option<TsConfig>, Vec<Diagnostic>)> {
    let Some(path) = path else {
        return Ok((None, Vec::new()));
    };

    let parsed = load_tsconfig_with_diagnostics(path)?;
    Ok((Some(parsed.config), parsed.diagnostics))
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
    let follow_links = env_flag("TSZ_FOLLOW_SYMLINKS");
    if !args.files.is_empty() {
        return Ok(FileDiscoveryOptions {
            base_dir: base_dir.to_path_buf(),
            files: args.files.clone(),
            include: None,
            exclude: None,
            out_dir: out_dir.map(Path::to_path_buf),
            follow_links,
            allow_js: resolved.allow_js,
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
    Ok(options)
}

pub(super) fn collect_type_root_files(
    base_dir: &Path,
    options: &ResolvedCompilerOptions,
) -> Vec<PathBuf> {
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

pub(super) fn read_source_files(
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
    let mut type_reference_errors = Vec::new();
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
            FileReadResult::Binary(text) => (text, true),
            FileReadResult::Error(e) => {
                return Err(anyhow::anyhow!("failed to read {}: {}", path.display(), e));
            }
        };
        let (specifiers, type_refs) = if is_binary {
            (vec![], vec![])
        } else {
            (
                collect_module_specifiers_from_text(&path, &text),
                tsz::checker::triple_slash_validator::extract_reference_types(&text),
            )
        };
        let reference_paths = if is_binary || options.no_resolve {
            vec![]
        } else {
            tsz::checker::triple_slash_validator::extract_reference_paths(&text)
        };

        sources.insert(path.clone(), (Some(text), is_binary));
        let entry = dependencies.entry(path.clone()).or_default();

        if !options.no_resolve {
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

        // Resolve /// <reference types="..." /> directives
        if !type_refs.is_empty() && !options.no_resolve {
            let type_roots = options
                .type_roots
                .clone()
                .unwrap_or_else(|| default_type_roots(base_dir));
            for (type_name, resolution_mode, _line) in type_refs {
                let resolved =
                    if let Some(ref mode) = resolution_mode {
                        // With explicit resolution-mode, use exports map with the specified condition
                        let candidates =
                            crate::driver_resolution::type_package_candidates_pub(&type_name);
                        let mut result = None;
                        for root in &type_roots {
                            for candidate in &candidates {
                                let package_root = root.join(candidate);
                                if package_root.is_dir()
                                    && let Some(entry) =
                                    crate::driver_resolution::resolve_type_package_entry_with_mode(
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
                if let Some(resolved) = resolved {
                    let canonical = canonicalize_or_owned(&resolved);
                    entry.insert(canonical.clone());
                    if seen.insert(canonical.clone()) {
                        pending.push_back(canonical);
                    }
                } else {
                    type_reference_errors.push((path.clone(), type_name));
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
                    .map(|candidate| canonicalize_or_owned(candidate))
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
        type_reference_errors,
    })
}

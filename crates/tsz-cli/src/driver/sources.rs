//! Source file I/O, config helpers, and file reading for the compilation driver.

use super::*;
use crate::fs::is_ts_file;

/// Count how many `node_modules` segments appear in a file path.
/// For example, `/a/node_modules/b/node_modules/c/index.js` has depth 2.
fn node_modules_depth(path: &Path) -> u32 {
    path.components()
        .filter(|c| c.as_os_str() == "node_modules")
        .count() as u32
}

/// Check whether a path's extension identifies a TypeScript/JavaScript source
/// or a JSON module that may be part of the program. Used to filter resolved
/// module paths so that package.json `"main"` entries pointing at non-source
/// files (e.g. `"main": "normalize.css"`) are silently ignored instead of being
/// parsed as TypeScript.
fn has_source_file_extension(path: &Path) -> bool {
    if is_ts_file(path) || is_js_file(path) {
        return true;
    }
    matches!(path.extension().and_then(|ext| ext.to_str()), Some("json"))
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
            let explicit_type_roots = options.type_roots.is_some();
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
                    // `compilerOptions.types` still owes TS2688 when explicit
                    // typeRoots did not contain the package, but tsc also makes
                    // the fallback package globals visible from node_modules.
                    files.insert(entry);
                    if explicit_type_roots {
                        unresolved.push(name.clone());
                    }
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

/// Per-file work that the parallel `read_source_files` BFS phase produces.
/// Bundled together so the file is opened and scanned exactly once per BFS
/// visit, with all per-file work running on a rayon worker before the
/// (necessarily serial) module-resolver phase consumes it.
struct ParsedSource {
    read_result: FileReadResult,
    specifiers: Vec<(
        String,
        tsz::module_resolver::ImportKind,
        Option<tsz::module_resolver::ImportingModuleKind>,
    )>,
    type_refs: Vec<(String, Option<String>, usize, usize)>,
    reference_paths: Vec<(String, usize, usize)>,
}

/// Read one source file and run the in-text scanners that the BFS used to
/// inline. Pure function — no shared state, safe to invoke from any thread.
fn parse_source_for_bfs(path: &Path, no_resolve: bool) -> ParsedSource {
    let read_result = read_source_file(path);
    let (text, is_binary) = match &read_result {
        FileReadResult::Text(t) => (Some(t.as_str()), false),
        FileReadResult::Binary { text, .. } => (Some(text.as_str()), true),
        FileReadResult::Error(_) => (None, false),
    };
    let specifiers = match text {
        Some(text) if !is_binary => {
            crate::driver::resolution::collect_module_requests_from_text(path, text)
        }
        _ => Vec::new(),
    };
    let type_refs = match text {
        Some(text) if !is_binary => {
            tsz::checker::triple_slash_validator::extract_reference_types(text)
        }
        _ => Vec::new(),
    };
    let reference_paths = match text {
        Some(text) if !is_binary && !no_resolve => {
            tsz::checker::triple_slash_validator::extract_reference_paths(text)
        }
        _ => Vec::new(),
    };
    ParsedSource {
        read_result,
        specifiers,
        type_refs,
        reference_paths,
    }
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

    // PERF: cache `normalize_resolved_path` results for the BFS lifetime.
    // The function calls `canonicalize` (= `realpath` syscall on macOS / Linux)
    // plus a `path_has_symlinked_package_ancestor` walk that does
    // `symlink_metadata` syscalls at every ancestor. Each unique resolved
    // path is normalized once per `read_source_files` call; with thousands of
    // import lookups in workspace projects this dominates BFS time after
    // the module-resolver caches kick in. Keyed by raw resolved path; result
    // is the canonical path returned by `normalize_resolved_path`.
    let mut normalize_cache: FxHashMap<PathBuf, PathBuf> = FxHashMap::default();
    let mut normalize = |path: &Path, options: &ResolvedCompilerOptions| -> PathBuf {
        if let Some(cached) = normalize_cache.get(path) {
            return cached.clone();
        }
        let canonical = normalize_resolved_path(path, options);
        normalize_cache.insert(path.to_path_buf(), canonical.clone());
        canonical
    };

    for path in paths {
        let canonical = normalize(path, options);
        if seen.insert(canonical.clone()) {
            pending.push_back(canonical);
        }
    }

    // PERF: BFS-by-level parallelism for the I/O-bound part of the loop.
    //
    // The original loop popped one path at a time and did the file read +
    // import-text scan + reference-text scan inline. On a 6086-file workspace
    // this single-threaded BFS spent ~85% of total wall time inside
    // `read_source_files`, all of it sequenced through the open()/read()
    // syscalls and the in-memory regex-based scanners. Profile (samply,
    // large-ts-repo full bench): the calling thread held 100% of CPU while
    // the rayon worker pool sat idle.
    //
    // Restructuring as a level-synchronous BFS lets every path discovered in
    // the previous iteration's resolution phase be read in parallel before
    // the (necessarily serial) module-resolver step that mutates
    // `module_resolver`, `resolution_cache`, `seen`, and `pending`. The serial
    // phase still pops items from a freshly-drained per-level batch in the
    // original BFS order, so the visited-set ordering and dependency
    // propagation are unchanged.

    /// Per-batch action for one path. Computed once on the calling thread,
    /// then `Read` items get their file body materialized in parallel before
    /// the serial resolution phase consumes the result.
    enum BatchAction {
        Cached,
        SkipJs,
        Read,
    }

    while !pending.is_empty() {
        let batch: Vec<PathBuf> = pending.drain(..).collect();

        // Phase 1 (serial): classify each path. The cache + skip checks are
        // cheap (HashMap lookups + path component scans) and need read access
        // to `cache`/`changed_paths`, so we keep them on the calling thread.
        let actions: Vec<BatchAction> = batch
            .iter()
            .map(|path| {
                let cached = use_cache
                    && cache.is_some_and(|c| {
                        changed_paths.is_some_and(|cp| !cp.contains(path))
                            && c.bind_cache.contains_key(path)
                            && c.dependencies.contains_key(path)
                    });
                if cached {
                    BatchAction::Cached
                } else if should_skip_js_in_node_modules(path, options.max_node_module_js_depth) {
                    BatchAction::SkipJs
                } else {
                    BatchAction::Read
                }
            })
            .collect();

        // Phase 2 (parallel): read + parse imports/refs for `Read` paths.
        // Each task is independent — no shared mutable state — and the closure
        // returns owned data. Per-path overhead is dominated by the open()
        // syscall plus the linear scanners over the file body, both of which
        // benefit from saturating the disk queue and CPU cores in parallel.
        use rayon::prelude::*;
        let no_resolve = options.no_resolve;
        let parsed: Vec<Option<ParsedSource>> = batch
            .par_iter()
            .zip(actions.par_iter())
            .map(|(path, action)| match action {
                BatchAction::Read => Some(parse_source_for_bfs(path, no_resolve)),
                BatchAction::Cached | BatchAction::SkipJs => None,
            })
            .collect();

        // Phase 3 (serial): apply each batch entry's action, queueing newly
        // discovered deps into `pending` for the next BFS level.
        for ((path, action), maybe_parsed) in
            batch.into_iter().zip(actions).zip(parsed)
        {
            match action {
                BatchAction::Cached => {
                    let cache = cache.expect("cached arm only fires when cache is Some");
                    let cached_deps = cache
                        .dependencies
                        .get(&path)
                        .expect("cached arm only fires when dependencies entry exists");
                    dependencies.insert(path.clone(), cached_deps.clone());
                    sources.insert(path.clone(), (None, false, false));
                    for dep in cached_deps {
                        if seen.insert(dep.clone()) {
                            pending.push_back(dep.clone());
                        }
                    }
                    continue;
                }
                BatchAction::SkipJs => {
                    sources.insert(path.clone(), (None, false, false));
                    continue;
                }
                BatchAction::Read => {}
            }

            let parsed = maybe_parsed.expect("Read action always produces parsed source");
            let ParsedSource {
                read_result,
                specifiers,
                type_refs,
                reference_paths,
            } = parsed;

            let (text, is_binary, suppress_parser_diagnostics) = match read_result {
                FileReadResult::Text(t) => (t, false, false),
                FileReadResult::Binary {
                    text,
                    suppress_parser_diagnostics,
                } => (text, true, suppress_parser_diagnostics),
                FileReadResult::Error(e) => {
                    return Err(anyhow::anyhow!("failed to read {}: {}", path.display(), e));
                }
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
                        Some(&seen),
                    )
                    .classify();
                if let Some(resolved) = outcome.resolved_path {
                    let canonical = normalize(&resolved, options);
                    entry.insert(canonical.clone());
                    if has_source_file_extension(&canonical) && seen.insert(canonical.clone()) {
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
                    let canonical = normalize(&resolved, options);
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
                            let canonical = normalize(&alt, options);
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
                    .map(|candidate| normalize(candidate, options))
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::tempdir;

    // ---------------- node_modules_depth ----------------

    #[test]
    fn node_modules_depth_zero_for_paths_without_segment() {
        assert_eq!(node_modules_depth(Path::new("/a/b/c.ts")), 0);
        assert_eq!(node_modules_depth(Path::new("relative/file.js")), 0);
        assert_eq!(node_modules_depth(Path::new("")), 0);
    }

    #[test]
    fn node_modules_depth_counts_each_segment_independently() {
        assert_eq!(
            node_modules_depth(Path::new("/proj/node_modules/foo/index.js")),
            1
        );
        assert_eq!(
            node_modules_depth(Path::new(
                "/proj/node_modules/foo/node_modules/bar/index.js"
            )),
            2
        );
        assert_eq!(
            node_modules_depth(Path::new(
                "/a/node_modules/b/node_modules/c/node_modules/d/x.js"
            )),
            3
        );
    }

    #[test]
    fn node_modules_depth_does_not_match_substring_segments() {
        // A directory whose name merely contains "node_modules" must not count.
        assert_eq!(
            node_modules_depth(Path::new("/proj/my_node_modules_clone/x.js")),
            0
        );
        assert_eq!(
            node_modules_depth(Path::new("/proj/node_modules_extra/x.js")),
            0
        );
    }

    // ---------------- has_source_file_extension ----------------

    #[test]
    fn has_source_file_extension_accepts_ts_family() {
        for path in [
            "a.ts", "a.tsx", "a.mts", "a.cts", "a.d.ts", "a.d.mts", "a.d.cts",
        ] {
            assert!(
                has_source_file_extension(Path::new(path)),
                "expected ts-family path to be accepted: {path}"
            );
        }
    }

    #[test]
    fn has_source_file_extension_accepts_js_family() {
        for path in ["a.js", "a.jsx", "a.mjs", "a.cjs"] {
            assert!(
                has_source_file_extension(Path::new(path)),
                "expected js-family path to be accepted: {path}"
            );
        }
    }

    #[test]
    fn has_source_file_extension_accepts_json() {
        assert!(has_source_file_extension(Path::new("pkg/data.json")));
    }

    #[test]
    fn has_source_file_extension_rejects_unrelated_extensions() {
        for path in ["a.css", "a.html", "a.md", "a.wasm", "a.json5", "a.node"] {
            assert!(
                !has_source_file_extension(Path::new(path)),
                "expected non-source path to be rejected: {path}"
            );
        }
    }

    #[test]
    fn has_source_file_extension_rejects_no_extension_or_empty() {
        assert!(!has_source_file_extension(Path::new("README")));
        assert!(!has_source_file_extension(Path::new("")));
    }

    // ---------------- should_skip_js_in_node_modules ----------------

    #[test]
    fn should_skip_js_in_node_modules_false_for_ts_files() {
        // TS files are never skipped by this gate, regardless of depth.
        assert!(!should_skip_js_in_node_modules(
            Path::new("/p/node_modules/foo/index.ts"),
            0
        ));
        assert!(!should_skip_js_in_node_modules(
            Path::new("/p/node_modules/foo/node_modules/bar/x.tsx"),
            0
        ));
    }

    #[test]
    fn should_skip_js_in_node_modules_false_when_depth_zero() {
        // JS file outside node_modules has depth 0; never skipped.
        assert!(!should_skip_js_in_node_modules(
            Path::new("/proj/src/index.js"),
            0
        ));
        assert!(!should_skip_js_in_node_modules(
            Path::new("/proj/src/index.js"),
            5
        ));
    }

    #[test]
    fn should_skip_js_in_node_modules_threshold_boundary() {
        // depth=1 with max_depth=0 -> skip (1 > 0)
        assert!(should_skip_js_in_node_modules(
            Path::new("/p/node_modules/foo/index.js"),
            0
        ));
        // depth=1 with max_depth=1 -> keep (1 > 1 is false)
        assert!(!should_skip_js_in_node_modules(
            Path::new("/p/node_modules/foo/index.js"),
            1
        ));
        // depth=2 with max_depth=1 -> skip
        assert!(should_skip_js_in_node_modules(
            Path::new("/p/node_modules/foo/node_modules/bar/index.js"),
            1
        ));
        // depth=2 with max_depth=2 -> keep
        assert!(!should_skip_js_in_node_modules(
            Path::new("/p/node_modules/foo/node_modules/bar/index.js"),
            2
        ));
    }

    #[test]
    fn should_skip_js_in_node_modules_jsx_mjs_cjs_branches() {
        for ext in ["js", "jsx", "mjs", "cjs"] {
            let path_str = format!("/p/node_modules/foo/index.{ext}");
            assert!(
                should_skip_js_in_node_modules(Path::new(&path_str), 0),
                "expected js-family `{ext}` inside node_modules to be skipped at max=0"
            );
        }
    }

    // ---------------- classify_binary_file ----------------

    #[test]
    fn classify_binary_file_empty_returns_none() {
        assert_eq!(classify_binary_file(b""), None);
    }

    #[test]
    fn classify_binary_file_plain_utf8_returns_none() {
        let text = b"export const x: number = 1;\n// hello\n";
        assert_eq!(classify_binary_file(text), None);
    }

    #[test]
    fn classify_binary_file_many_nulls_returns_some_true() {
        // 11 null bytes scattered in the first 1024 bytes -> binary, suppress.
        let mut bytes = vec![b'a'; 1024];
        for slot in bytes.iter_mut().take(11) {
            *slot = 0;
        }
        assert_eq!(classify_binary_file(&bytes), Some(true));
    }

    #[test]
    fn classify_binary_file_consecutive_nulls_returns_some_true() {
        // 4 consecutive nulls inside the first 512 bytes -> binary.
        // Keep total nulls <= 10 so the many-null branch does not fire first.
        let mut bytes = vec![b'a'; 64];
        bytes[10] = 0;
        bytes[11] = 0;
        bytes[12] = 0;
        bytes[13] = 0;
        assert_eq!(classify_binary_file(&bytes), Some(true));
    }

    #[test]
    fn classify_binary_file_three_consecutive_nulls_not_enough() {
        // 3 consecutive nulls (total nulls = 3) -> not enough, returns None.
        let mut bytes = vec![b'a'; 64];
        bytes[10] = 0;
        bytes[11] = 0;
        bytes[12] = 0;
        assert_eq!(classify_binary_file(&bytes), None);
    }

    #[test]
    fn classify_binary_file_control_bytes_route_through_soft_check() {
        // 4 stray control bytes (non-whitespace, < 0x20) trigger the "control"
        // branch which delegates to soft_control_binary_should_suppress.
        // With no printable trailing payload, suppression should be true.
        let bytes: Vec<u8> = vec![0x01, 0x02, 0x03, 0x04];
        assert_eq!(classify_binary_file(&bytes), Some(true));
    }

    #[test]
    fn classify_binary_file_whitespace_controls_do_not_count() {
        // tab/newline/CR/FF/VT are excluded from the control-byte tally.
        let bytes: Vec<u8> = vec![b'\t', b'\n', b'\r', 0x0C, 0x0B, b'a', b'b'];
        assert_eq!(classify_binary_file(&bytes), None);
    }

    #[test]
    fn classify_binary_file_three_control_bytes_not_enough() {
        // Only 3 control bytes; control-bytes branch needs >= 4. Returns None.
        let bytes: Vec<u8> = vec![0x01, 0x02, 0x03, b'a', b'b', b'c'];
        assert_eq!(classify_binary_file(&bytes), None);
    }

    // ---------------- soft_control_binary_should_suppress ----------------

    #[test]
    fn soft_control_binary_suppresses_when_payload_is_short() {
        // No newline at all -> entire input is the payload. Only one printable
        // ASCII byte ('a') -> suppress.
        let bytes: Vec<u8> = vec![0x01, 0x02, b'a'];
        assert!(soft_control_binary_should_suppress(&bytes));
    }

    #[test]
    fn soft_control_binary_keeps_diagnostics_when_payload_has_text() {
        // Payload "abc" has 3 printable ASCII bytes -> do not suppress.
        let bytes: Vec<u8> = vec![0x01, 0x02, b'a', b'b', b'c'];
        assert!(!soft_control_binary_should_suppress(&bytes));
    }

    #[test]
    fn soft_control_binary_uses_payload_after_last_newline() {
        // Last newline at index 5; payload after it = b"hi" (2 printable) ->
        // not suppressed (printable_ascii_count is 2, condition is `< 2`).
        let bytes: Vec<u8> = vec![b'a', b'b', b'c', b'd', b'e', b'\n', b'h', b'i'];
        assert!(!soft_control_binary_should_suppress(&bytes));
    }

    #[test]
    fn soft_control_binary_suppresses_when_post_newline_payload_is_short() {
        // Payload after last newline is just one printable char -> suppress.
        let bytes: Vec<u8> = vec![b'a', b'b', b'c', b'\n', b'q'];
        assert!(soft_control_binary_should_suppress(&bytes));
    }

    // ---------------- read_source_file ----------------

    fn write_temp(dir: &std::path::Path, name: &str, bytes: &[u8]) -> PathBuf {
        let path = dir.join(name);
        let mut f = std::fs::File::create(&path).expect("create temp file");
        f.write_all(bytes).expect("write temp file");
        path
    }

    #[test]
    fn read_source_file_plain_utf8_returns_text() {
        let dir = tempdir().unwrap();
        let path = write_temp(dir.path(), "ascii.ts", b"export const x = 1;\n");
        match read_source_file(&path) {
            FileReadResult::Text(t) => assert_eq!(t, "export const x = 1;\n"),
            other => panic!("expected Text, got {other:?}"),
        }
    }

    #[test]
    fn read_source_file_utf16_be_bom_decodes_text() {
        let dir = tempdir().unwrap();
        // "Hi" in UTF-16 BE with BOM.
        let bytes: Vec<u8> = vec![0xFE, 0xFF, 0x00, b'H', 0x00, b'i'];
        let path = write_temp(dir.path(), "u16be.ts", &bytes);
        match read_source_file(&path) {
            FileReadResult::Text(t) => assert_eq!(t, "Hi"),
            other => panic!("expected Text from UTF-16 BE BOM, got {other:?}"),
        }
    }

    #[test]
    fn read_source_file_utf16_le_bom_decodes_text() {
        let dir = tempdir().unwrap();
        // "Hi" in UTF-16 LE with BOM.
        let bytes: Vec<u8> = vec![0xFF, 0xFE, b'H', 0x00, b'i', 0x00];
        let path = write_temp(dir.path(), "u16le.ts", &bytes);
        match read_source_file(&path) {
            FileReadResult::Text(t) => assert_eq!(t, "Hi"),
            other => panic!("expected Text from UTF-16 LE BOM, got {other:?}"),
        }
    }

    #[test]
    fn read_source_file_binary_marks_suppression() {
        let dir = tempdir().unwrap();
        // 11 null bytes -> classify_binary_file returns Some(true).
        let mut bytes = vec![b'a'; 64];
        for slot in bytes.iter_mut().take(11) {
            *slot = 0;
        }
        let path = write_temp(dir.path(), "bin.bin", &bytes);
        match read_source_file(&path) {
            FileReadResult::Binary {
                suppress_parser_diagnostics,
                ..
            } => assert!(suppress_parser_diagnostics),
            other => panic!("expected Binary, got {other:?}"),
        }
    }

    #[test]
    fn read_source_file_missing_file_returns_error() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("does_not_exist.ts");
        match read_source_file(&path) {
            FileReadResult::Error(msg) => assert!(!msg.is_empty()),
            other => panic!("expected Error, got {other:?}"),
        }
    }

    #[test]
    fn read_source_file_invalid_utf8_falls_back_to_lossy_binary() {
        let dir = tempdir().unwrap();
        // Stray 0xFF byte not paired with 0xFE makes invalid UTF-8 but does not
        // hit BOM or many-nulls branches: from_utf8 fails -> Binary{ suppress=true }.
        let bytes: Vec<u8> = vec![b'a', b'b', 0xFF, b'c'];
        let path = write_temp(dir.path(), "bad-utf8.ts", &bytes);
        match read_source_file(&path) {
            FileReadResult::Binary {
                suppress_parser_diagnostics,
                ..
            } => assert!(suppress_parser_diagnostics),
            other => panic!("expected Binary fallback, got {other:?}"),
        }
    }

    // ---------------- has_no_default_lib_directive ----------------

    #[test]
    fn has_no_default_lib_directive_true_for_canonical_form() {
        let src = "/// <reference no-default-lib=\"true\" />\nexport {};\n";
        assert!(has_no_default_lib_directive(src));
    }

    #[test]
    fn has_no_default_lib_directive_true_for_single_quotes() {
        let src = "/// <reference no-default-lib='true' />\n";
        assert!(has_no_default_lib_directive(src));
    }

    #[test]
    fn has_no_default_lib_directive_false_when_value_false() {
        let src = "/// <reference no-default-lib=\"false\" />\n";
        assert!(!has_no_default_lib_directive(src));
    }

    #[test]
    fn has_no_default_lib_directive_skips_blank_lines_before_first_triple_slash() {
        let src = "\n\n   \n/// <reference no-default-lib=\"true\" />\n";
        assert!(has_no_default_lib_directive(src));
    }

    #[test]
    fn has_no_default_lib_directive_stops_at_first_non_directive_non_blank() {
        // A non-`///` non-blank line breaks the prefix scan, so a later directive
        // is ignored.
        let src = "import x from './a';\n/// <reference no-default-lib=\"true\" />\n";
        assert!(!has_no_default_lib_directive(src));
    }

    #[test]
    fn has_no_default_lib_directive_false_when_absent() {
        assert!(!has_no_default_lib_directive(
            "/// <reference path=\"./other.d.ts\" />\n"
        ));
        assert!(!has_no_default_lib_directive(""));
    }

    // ---------------- has_no_types_and_symbols_directive ----------------

    #[test]
    fn has_no_types_and_symbols_directive_canonical_true() {
        let src = "// @noTypesAndSymbols: true\nexport {};\n";
        assert!(has_no_types_and_symbols_directive(src));
    }

    #[test]
    fn has_no_types_and_symbols_directive_case_insensitive_key() {
        let src = "// @NOTYPESANDSYMBOLS: true\n";
        assert!(has_no_types_and_symbols_directive(src));
    }

    #[test]
    fn has_no_types_and_symbols_directive_case_insensitive_value() {
        let src = "// @noTypesAndSymbols: TRUE\n";
        assert!(has_no_types_and_symbols_directive(src));
    }

    #[test]
    fn has_no_types_and_symbols_directive_false_when_value_false() {
        let src = "// @noTypesAndSymbols: false\n";
        assert!(!has_no_types_and_symbols_directive(src));
    }

    #[test]
    fn has_no_types_and_symbols_directive_requires_colon() {
        // No colon between key and value -> not honored.
        let src = "// @noTypesAndSymbols true\n";
        assert!(!has_no_types_and_symbols_directive(src));
    }

    #[test]
    fn has_no_types_and_symbols_directive_only_first_32_lines_scanned() {
        // 32 filler lines then the directive on line 33 -> not honored.
        let mut src = String::new();
        for _ in 0..32 {
            src.push_str("// filler\n");
        }
        src.push_str("// @noTypesAndSymbols: true\n");
        assert!(!has_no_types_and_symbols_directive(&src));

        // Same directive on line 32 (within window) -> honored.
        let mut src_in = String::new();
        for _ in 0..31 {
            src_in.push_str("// filler\n");
        }
        src_in.push_str("// @noTypesAndSymbols: true\n");
        assert!(has_no_types_and_symbols_directive(&src_in));
    }

    #[test]
    fn has_no_types_and_symbols_directive_false_when_absent() {
        assert!(!has_no_types_and_symbols_directive(
            "// some unrelated comment\nexport {};\n"
        ));
        assert!(!has_no_types_and_symbols_directive(""));
    }

    // ---------------- parse_reference_no_default_lib_value ----------------

    #[test]
    fn parse_reference_no_default_lib_value_true_double_quotes() {
        assert_eq!(
            parse_reference_no_default_lib_value("/// <reference no-default-lib=\"true\" />"),
            Some(true)
        );
    }

    #[test]
    fn parse_reference_no_default_lib_value_true_single_quotes() {
        assert_eq!(
            parse_reference_no_default_lib_value("/// <reference no-default-lib='true' />"),
            Some(true)
        );
    }

    #[test]
    fn parse_reference_no_default_lib_value_false() {
        assert_eq!(
            parse_reference_no_default_lib_value("/// <reference no-default-lib=\"false\" />"),
            Some(false)
        );
    }

    #[test]
    fn parse_reference_no_default_lib_value_case_insensitive_value() {
        assert_eq!(
            parse_reference_no_default_lib_value("/// <reference no-default-lib=\"TRUE\" />"),
            Some(true)
        );
        assert_eq!(
            parse_reference_no_default_lib_value("/// <reference no-default-lib=\"False\" />"),
            Some(false)
        );
    }

    #[test]
    fn parse_reference_no_default_lib_value_unknown_value_is_none() {
        assert_eq!(
            parse_reference_no_default_lib_value("/// <reference no-default-lib=\"yes\" />"),
            None
        );
    }

    #[test]
    fn parse_reference_no_default_lib_value_unquoted_value_is_none() {
        assert_eq!(
            parse_reference_no_default_lib_value("/// <reference no-default-lib=true />"),
            None
        );
    }

    #[test]
    fn parse_reference_no_default_lib_value_missing_equals_is_none() {
        assert_eq!(
            parse_reference_no_default_lib_value("/// <reference no-default-lib \"true\" />"),
            None
        );
    }

    #[test]
    fn parse_reference_no_default_lib_value_needle_absent_is_none() {
        assert_eq!(
            parse_reference_no_default_lib_value("/// <reference path=\"./a.d.ts\" />"),
            None
        );
        assert_eq!(parse_reference_no_default_lib_value(""), None);
    }

    #[test]
    fn parse_reference_no_default_lib_value_tolerates_extra_spaces() {
        assert_eq!(
            parse_reference_no_default_lib_value(
                "/// <reference   no-default-lib   =   \"true\"   />"
            ),
            Some(true)
        );
    }
}

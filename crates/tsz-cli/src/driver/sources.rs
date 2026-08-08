//! Source file I/O, config helpers, and file reading for the compilation driver.

use super::*;
use crate::fs::is_ts_file;
use rustc_hash::FxHasher;
use std::hash::{Hash, Hasher};

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
pub(super) fn should_skip_js_in_node_modules(path: &Path, max_depth: u32) -> bool {
    if !is_js_file(path) {
        return false;
    }
    let depth = node_modules_depth(path);
    if depth == 0 {
        return false;
    }
    depth > max_depth
}

pub(super) fn hash_text_with_language_version(text: &str, language_version: ScriptTarget) -> u64 {
    let mut hasher = FxHasher::default();
    text.hash(&mut hasher);
    language_version.ts_numeric_value().hash(&mut hasher);
    hasher.finish()
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
        return text_may_contain_no_default_lib_directive(text)
            && has_no_default_lib_directive(text);
    }
    let Ok(text) = std::fs::read_to_string(&source.path) else {
        return false;
    };
    text_may_contain_no_default_lib_directive(&text) && has_no_default_lib_directive(&text)
}

fn text_may_contain_no_default_lib_directive(text: &str) -> bool {
    text.contains("///") && text.contains("reference") && text.contains("no-default-lib")
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
    /// Per-file resolved dependencies in source-import (discovery) order.
    ///
    /// Order is load-bearing: cached project rebuilds replay this list to seed
    /// BFS discovery, and discovery order determines the global `SymbolId`
    /// assignment in `merge_user_files`. Storing an order-preserving `Vec`
    /// (deduplicated on insert) rather than a hashed set keeps the replayed
    /// discovery order identical to the original fresh build, so unchanged
    /// project rows do not reconstruct different `SymbolId` values.
    pub(super) dependencies: FxHashMap<PathBuf, Vec<PathBuf>>,
    pub(super) outfile_bundle_paths: FxHashSet<PathBuf>,
    pub(super) outfile_bundle_dependencies: FxHashMap<PathBuf, FxHashSet<PathBuf>>,
    pub(super) module_resolutions: FxHashMap<SourceModuleResolutionKey, SourceModuleResolution>,
    pub(super) module_resolution_misses: FxHashSet<SourceModuleResolutionKey>,
    /// Tuples of (`file_path`, `type_name`, `byte_offset_of_types_attr`, `span_length`).
    pub(super) type_reference_errors: Vec<(PathBuf, String, usize, usize)>,
    /// TS1453: Invalid `resolution-mode` values in `/// <reference types="..." />` directives.
    /// Tuples of (`file_path`, `byte_offset`, `span_length`).
    pub(super) resolution_mode_errors: Vec<(PathBuf, usize, usize)>,
    /// Paths the `maxNodeModuleJsDepth` BFS gate skipped rather than read
    /// (`BatchAction::SkipJs`). Each such path keeps a `SourceFile` registered
    /// in the program (so `require()`/import specifiers pointing at it still
    /// resolve) but with permanently empty source text, never a real parse of
    /// the file's actual content. Checker consumers that build a CJS/JS export
    /// shape for a `require()` target need this set to tell "never read" apart
    /// from "read and genuinely has no exports" — see #16934.
    pub(super) depth_skipped_js_paths: FxHashSet<PathBuf>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(super) struct SourceModuleResolutionKey {
    pub(super) containing_file: PathBuf,
    pub(super) specifier: String,
    pub(super) import_kind: ImportKind,
    pub(super) resolution_mode_override: Option<ImportingModuleKind>,
}

#[derive(Debug, Clone)]
pub(super) struct SourceModuleResolution {
    pub(super) canonical_path: PathBuf,
    pub(super) resolved_using_ts_extension: bool,
}

/// Locate the nearest `tsconfig.json`, starting at `cwd` and walking up parent
/// directories until one is found or the filesystem root is reached.
///
/// Matches TypeScript's no-argument project discovery (`findConfigFile`), where
/// running `tsc` with no file arguments from a project subdirectory still finds
/// the parent project's config.
pub fn find_tsconfig(cwd: &Path) -> Option<PathBuf> {
    let mut current = Some(cwd);
    while let Some(dir) = current {
        let candidate = dir.join("tsconfig.json");
        if candidate.is_file() {
            return Some(normalize_path(&candidate));
        }
        current = dir.parent();
    }
    None
}

/// Failure modes for `--project` resolution. Distinguishes the two error
/// codes tsc emits: TS5057 (existing directory missing tsconfig.json) and
/// TS5058 (path does not exist on disk). The original user-supplied path is
/// preserved so the diagnostic message matches tsc's relative-path rendering.
#[derive(Debug)]
pub(crate) enum ResolveTsconfigError {
    /// User-supplied `--project` path does not exist on disk. Maps to TS5058.
    PathDoesNotExist(PathBuf),
    /// User-supplied `--project` path is an existing directory that does not
    /// contain a `tsconfig.json`. Maps to TS5057.
    NoConfigInDirectory(PathBuf),
    /// User-supplied `--project` path exists but is neither a file nor a
    /// directory (e.g., a broken symlink resolved by `exists()` but not by
    /// `is_file()`). Falls back to TS5058.
    NotAFile(PathBuf),
}

impl std::fmt::Display for ResolveTsconfigError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::PathDoesNotExist(p) | Self::NotAFile(p) => {
                write!(f, "The specified path does not exist: '{}'.", p.display())
            }
            Self::NoConfigInDirectory(p) => write!(
                f,
                "Cannot find a tsconfig.json file at the specified directory: '{}'.",
                p.display()
            ),
        }
    }
}

impl std::error::Error for ResolveTsconfigError {}

pub(crate) fn resolve_tsconfig_path(
    cwd: &Path,
    project: Option<&Path>,
) -> std::result::Result<Option<PathBuf>, ResolveTsconfigError> {
    let Some(project) = project else {
        return Ok(find_tsconfig(cwd));
    };

    let absolute = if project.is_absolute() {
        project.to_path_buf()
    } else {
        cwd.join(project)
    };

    if absolute.is_dir() {
        let candidate = absolute.join("tsconfig.json");
        if !candidate.is_file() {
            return Err(ResolveTsconfigError::NoConfigInDirectory(
                project.to_path_buf(),
            ));
        }
        return Ok(Some(normalize_path(&candidate)));
    }

    if !absolute.exists() {
        return Err(ResolveTsconfigError::PathDoesNotExist(
            project.to_path_buf(),
        ));
    }

    if !absolute.is_file() {
        return Err(ResolveTsconfigError::NotAFile(project.to_path_buf()));
    }

    Ok(Some(normalize_path(&absolute)))
}

pub(crate) fn load_config(path: Option<&Path>) -> Result<Option<TsConfig>> {
    let Some(path) = path else {
        return Ok(None);
    };

    let config = load_tsconfig(path)?;
    Ok(Some(config))
}

pub(crate) struct LoadedConfig {
    pub config: Option<TsConfig>,
    pub diagnostics: Vec<Diagnostic>,
    /// Entry-anchored removed-option notices not yet committed to
    /// `diagnostics`: the driver drops the ones the CLI overrides with valid
    /// values (tsc validates removals on the CLI-merged effective options),
    /// then flushes the rest.
    pub pending_removed_option_notices: Vec<RemovedOptionNotice>,
}

pub(crate) fn load_config_with_diagnostics(path: Option<&Path>) -> Result<LoadedConfig> {
    let Some(path) = path else {
        return Ok(LoadedConfig {
            config: None,
            diagnostics: Vec::new(),
            pending_removed_option_notices: Vec::new(),
        });
    };

    let parsed = load_tsconfig_with_diagnostics_deferred(path)?;
    Ok(LoadedConfig {
        config: Some(parsed.config),
        diagnostics: parsed.diagnostics,
        pending_removed_option_notices: parsed.pending_removed_option_notices,
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
    let mut resolution_cache = ModuleResolutionCache::default();
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
                    crate::driver::resolution::resolve_type_reference_from_node_modules_with_cache(
                        name,
                        &synthetic_from_file,
                        base_dir,
                        None,
                        options,
                        &mut resolution_cache,
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
                if let Some(entry) =
                    crate::driver::resolution::resolve_type_package_from_roots_with_cache(
                        name,
                        &roots,
                        options,
                        &mut resolution_cache,
                    )
                {
                    files.insert(entry);
                } else if let Some(entry) =
                    crate::driver::resolution::resolve_type_reference_from_node_modules_with_cache(
                        name,
                        &synthetic_from_file,
                        base_dir,
                        None,
                        options,
                        &mut resolution_cache,
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

    // Auto-include every `@types/*` package found across the (nearest-first)
    // type roots. The `types: ["*"]` conformance harness wildcard keeps
    // auto-discovery project-local when default roots are used; explicit
    // package names above still use the full ancestor walk.
    let canonical_base_dir = canonicalize_or_owned(base_dir);
    let auto_roots: Vec<&PathBuf> = if options.type_roots.is_none()
        && options
            .types
            .as_ref()
            .is_some_and(|types| types.iter().any(|t| t == "*" || t.trim().is_empty()))
    {
        roots
            .iter()
            .filter(|root| root.starts_with(&canonical_base_dir))
            .collect()
    } else {
        roots.iter().collect()
    };
    // tsc's `getAutomaticTypeDirectiveNames` keys discovered packages by name
    // and resolves each once, so a package present in both a nested and a
    // hoisted `node_modules/@types` (common in monorepos now that ancestor
    // roots are walked) is loaded a single time from the nearest root.
    // Without this the same global-augmenting package (e.g. `@types/node`)
    // would be inserted twice and produce spurious duplicate-declaration
    // diagnostics.
    let mut seen_names = FxHashSet::default();
    for root in auto_roots {
        for package_root in collect_type_packages_from_root(root) {
            let package_name = package_root
                .strip_prefix(root)
                .unwrap_or(&package_root)
                .to_path_buf();
            if !seen_names.insert(package_name) {
                continue;
            }
            if let Some(entry) = crate::driver::resolution::resolve_type_package_entry_with_cache(
                &package_root,
                options,
                &mut resolution_cache,
            ) {
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
        bool,
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
        Some(text) if !is_binary && text_may_contain_reference_directives(text) => {
            tsz::checker::triple_slash_validator::extract_reference_types(text)
        }
        _ => Vec::new(),
    };
    let reference_paths = match text {
        Some(text) if !is_binary && !no_resolve && text_may_contain_reference_directives(text) => {
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

fn text_may_contain_reference_directives(text: &str) -> bool {
    text.contains("///") && text.contains("reference")
}

/// Append `dep` to a per-file dependency list, preserving insertion
/// (source-import) order and skipping duplicates.
///
/// Dependency lists are short (a file's direct imports), so the linear
/// membership scan is cheaper than the allocation/hashing a set would add, and
/// it keeps the order deterministic for cached-rebuild replay.
fn push_unique_dep(deps: &mut Vec<PathBuf>, dep: PathBuf) {
    if !deps.contains(&dep) {
        deps.push(dep);
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
    // Per-file dependency lists are kept in source-import order (see the doc on
    // `SourceReadResult::dependencies`); `push_unique_dep` preserves that order
    // while deduplicating repeated imports of the same module within a file.
    let mut dependencies: FxHashMap<PathBuf, Vec<PathBuf>> = FxHashMap::default();
    let mut outfile_bundle_paths: FxHashSet<PathBuf> = FxHashSet::default();
    let mut outfile_bundle_dependencies: FxHashMap<PathBuf, FxHashSet<PathBuf>> =
        FxHashMap::default();
    let mut module_resolutions: FxHashMap<SourceModuleResolutionKey, SourceModuleResolution> =
        FxHashMap::default();
    let mut module_resolution_misses: FxHashSet<SourceModuleResolutionKey> = FxHashSet::default();
    let mut seen = FxHashSet::default();
    let mut discovery_order: FxHashMap<PathBuf, usize> = FxHashMap::default();
    let mut next_discovery_order = 0usize;
    let mut pending = VecDeque::new();
    let mut resolution_cache = ModuleResolutionCache::default();
    let mut module_resolver = ModuleResolver::new(options);
    let mut type_reference_errors = Vec::new();
    let mut resolution_mode_errors = Vec::new();
    let mut depth_skipped_js_paths: FxHashSet<PathBuf> = FxHashSet::default();
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

    // Explicit program roots (tsc's `rootNames` — `files`/CLI-argument
    // entries) must never be skipped by the `maxNodeModuleJsDepth` BFS gate
    // below, even when they physically sit under `node_modules`. tsc's
    // `maxNodeModuleJsDepth` bounds how many JS-requires-JS hops the BFS
    // follows away from the program's real inputs; a root is a real input by
    // definition; it is never reached "by descending into node_modules". A
    // root JS file that is skipped here keeps its `SourceFile` registered
    // (roots enter `pending` unconditionally above) but with a permanently
    // empty statement list, which starves every downstream CJS `module.exports`
    // surface computation — see #16928.
    let mut root_paths: FxHashSet<PathBuf> = FxHashSet::default();

    for path in paths {
        let canonical = normalize(path, options);
        outfile_bundle_paths.insert(canonical.clone());
        root_paths.insert(canonical.clone());
        if seen.insert(canonical.clone()) {
            discovery_order.insert(canonical.clone(), next_discovery_order);
            next_discovery_order += 1;
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
                } else if !root_paths.contains(path)
                    && should_skip_js_in_node_modules(path, options.max_node_module_js_depth)
                {
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
        let parsed: Vec<Option<ParsedSource>> =
            tsz::parallel::run_with_rayon_pool_for_work_items(batch.len(), || {
                batch
                    .par_iter()
                    .zip(actions.par_iter())
                    .map(|(path, action)| match action {
                        BatchAction::Read => Some(parse_source_for_bfs(path, no_resolve)),
                        BatchAction::Cached | BatchAction::SkipJs => None,
                    })
                    .collect()
            });

        // Phase 3 (serial): apply each batch entry's action, queueing newly
        // discovered deps into `pending` for the next BFS level.
        for ((path, action), maybe_parsed) in batch.into_iter().zip(actions).zip(parsed) {
            match action {
                BatchAction::Cached => {
                    let cache = cache.expect("cached arm only fires when cache is Some");
                    let cached_deps = cache
                        .dependencies
                        .get(&path)
                        .expect("cached arm only fires when dependencies entry exists");
                    dependencies.insert(path.clone(), cached_deps.clone());
                    sources.insert(path.clone(), (None, false, false));
                    if let Some(cached_bundle_deps) = cache.outfile_bundle_dependencies.get(&path) {
                        outfile_bundle_paths.extend(cached_bundle_deps.iter().cloned());
                    }
                    for dep in cached_deps {
                        if seen.insert(dep.clone()) {
                            discovery_order.insert(dep.clone(), next_discovery_order);
                            next_discovery_order += 1;
                            pending.push_back(dep.clone());
                        }
                    }
                    continue;
                }
                BatchAction::SkipJs => {
                    sources.insert(path.clone(), (None, false, false));
                    depth_skipped_js_paths.insert(path.clone());
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
            let bundle_entry = outfile_bundle_dependencies.entry(path.clone()).or_default();

            if !options.no_resolve {
                for (
                    specifier,
                    import_kind,
                    resolution_mode_override,
                    has_type_json_import_attribute,
                ) in specifiers
                {
                    let request = tsz::module_resolver::ModuleLookupRequest {
                        specifier: &specifier,
                        containing_file: &path,
                        specifier_span: tsz_common::Span::new(0, 0),
                        import_kind,
                        resolution_mode_override,
                        no_implicit_any: options.checker.no_implicit_any,
                        implied_classic_resolution: options.checker.implied_classic_resolution,
                    };
                    tsz_common::perf_counters::record_resolver_lookup_call();
                    let mut outcome = module_resolver
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
                    apply_json_type_import_attribute_override(
                        &mut outcome,
                        has_type_json_import_attribute,
                        &path,
                        &specifier,
                        options,
                        base_dir,
                        &mut resolution_cache,
                        &seen,
                    );
                    if let Some(resolved) = outcome.resolved_path {
                        let canonical = normalize(&resolved, options);
                        if outcome.error.is_none() {
                            module_resolutions.insert(
                                SourceModuleResolutionKey {
                                    containing_file: path.clone(),
                                    specifier: specifier.clone(),
                                    import_kind,
                                    resolution_mode_override,
                                },
                                SourceModuleResolution {
                                    canonical_path: canonical.clone(),
                                    resolved_using_ts_extension: outcome
                                        .resolved_using_ts_extension,
                                },
                            );
                        }
                        push_unique_dep(entry, canonical.clone());
                        if import_kind != tsz::module_resolver::ImportKind::DynamicImport {
                            outfile_bundle_paths.insert(canonical.clone());
                            bundle_entry.insert(canonical.clone());
                        }
                        if has_source_file_extension(&canonical) && seen.insert(canonical.clone()) {
                            discovery_order.insert(canonical.clone(), next_discovery_order);
                            next_discovery_order += 1;
                            pending.push_back(canonical);
                        }
                    } else {
                        module_resolution_misses.insert(SourceModuleResolutionKey {
                            containing_file: path.clone(),
                            specifier: specifier.clone(),
                            import_kind,
                            resolution_mode_override,
                        });
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
                    let resolved = if let Some(mode) = effective_resolution_mode {
                        // With explicit resolution-mode, use exports map with the specified condition
                        let candidates =
                            crate::driver::resolution::type_package_candidates_pub(&type_name);
                        let mut result = None;
                        for root in &type_roots {
                            for candidate in &candidates {
                                let package_root = root.join(candidate);
                                if resolution_cache.package_root_dir_exists(&package_root)
                                    && let Some(entry) =
                                    crate::driver::resolution::resolve_type_package_entry_with_mode_and_cache(
                                        &package_root,
                                        mode,
                                        options,
                                        &mut resolution_cache,
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
                        crate::driver::resolution::resolve_type_package_from_roots_with_cache(
                            &type_name,
                            &type_roots,
                            options,
                            &mut resolution_cache,
                        )
                    };
                    // If type roots resolution failed, fall back to searching node_modules/
                    // directly. tsc's resolveTypeReferenceDirective always uses node_modules
                    // walk-up as a secondary fallback after typeRoots, regardless of the
                    // configured module resolution mode (including Classic).
                    let resolved = resolved.or_else(|| {
                        crate::driver::resolution::resolve_type_reference_from_node_modules_with_cache(
                            &type_name,
                            &path,
                            base_dir,
                            effective_resolution_mode.map(|s| s.as_str()),
                            options,
                            &mut resolution_cache,
                        )
                    });
                    if let Some(resolved) = resolved {
                        let canonical = normalize(&resolved, options);
                        push_unique_dep(entry, canonical.clone());
                        outfile_bundle_paths.insert(canonical.clone());
                        bundle_entry.insert(canonical.clone());
                        if seen.insert(canonical.clone()) {
                            discovery_order.insert(canonical.clone(), next_discovery_order);
                            next_discovery_order += 1;
                            pending.push_back(canonical);
                        }
                    } else if !invalid_mode && !options.no_check {
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
                                crate::driver::resolution::resolve_type_reference_from_node_modules_with_cache(
                                    &type_name,
                                    &path,
                                    base_dir,
                                    Some(mode),
                                    options,
                                    &mut resolution_cache,
                                )
                            {
                                let canonical = normalize(&alt, options);
                                push_unique_dep(entry, canonical.clone());
                                outfile_bundle_paths.insert(canonical.clone());
                                bundle_entry.insert(canonical.clone());
                                if seen.insert(canonical.clone()) {
                                    discovery_order.insert(canonical.clone(), next_discovery_order);
                                    next_discovery_order += 1;
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
                    // Probe `.ts`/`.tsx`/`.d.ts` when the reference's FILE NAME has no
                    // extension. Testing the whole string for `.` misfires on a `./` or
                    // `../` relative prefix, silently skipping the probe. Mirror the
                    // diagnostic path in state_checking/directive.rs, which uses
                    // `Path::file_name()`.
                    let file_name_lacks_extension = !Path::new(&reference_path)
                        .file_name()
                        .and_then(|f| f.to_str())
                        .unwrap_or(reference_path.as_str())
                        .contains('.');
                    if file_name_lacks_extension {
                        for ext in
                            tsz::checker::triple_slash_validator::reference_path_probe_extensions(
                                options.allow_js,
                            )
                        {
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
                    push_unique_dep(entry, resolved_reference.clone());
                    outfile_bundle_paths.insert(resolved_reference.clone());
                    bundle_entry.insert(resolved_reference.clone());
                    if seen.insert(resolved_reference.clone()) {
                        discovery_order.insert(resolved_reference.clone(), next_discovery_order);
                        next_discovery_order += 1;
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
        let left_order = discovery_order
            .get(&left.path)
            .copied()
            .unwrap_or(usize::MAX);
        let right_order = discovery_order
            .get(&right.path)
            .copied()
            .unwrap_or(usize::MAX);
        left_order.cmp(&right_order).then_with(|| {
            left.path
                .to_string_lossy()
                .cmp(&right.path.to_string_lossy())
        })
    });
    Ok(SourceReadResult {
        sources: list,
        dependencies,
        outfile_bundle_paths,
        outfile_bundle_dependencies,
        module_resolutions,
        module_resolution_misses,
        type_reference_errors,
        resolution_mode_errors,
        depth_skipped_js_paths,
    })
}

#[cfg(test)]
#[path = "sources/tests.rs"]
mod tests;

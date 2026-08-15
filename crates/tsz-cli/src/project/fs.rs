use anyhow::{Context, Result, bail};
use globset::{Glob, GlobSet, GlobSetBuilder};
use std::collections::{BTreeSet, HashSet};
use std::path::{Path, PathBuf};
use tsz_common::file_extensions::{
    default_discovery_include_patterns, include_pattern_has_supported_extension, is_json_file,
    strip_ts_declaration_extension_from_path, strip_ts_source_extension_from_path,
};
pub(crate) use tsz_common::file_extensions::{
    is_js_file, is_ts_file, is_valid_module_file, is_valid_module_or_js_file,
};
use walkdir::{DirEntry, WalkDir};

use crate::config::TsConfig;

pub(crate) const DEFAULT_EXCLUDES: [&str; 3] =
    ["node_modules", "bower_components", "jspm_packages"];

#[derive(Debug, Clone)]
pub struct FileDiscoveryOptions {
    pub base_dir: PathBuf,
    pub files: Vec<PathBuf>,
    /// True when the tsconfig explicitly set `"files"` (even to `[]`).
    /// Distinguishes `"files": []` (no files, no default glob) from a
    /// missing `files` key (default `**/*` glob applies).
    pub files_explicitly_set: bool,
    pub include: Option<Vec<String>>,
    pub exclude: Option<Vec<String>>,
    pub out_dir: Option<PathBuf>,
    pub follow_links: bool,
    pub allow_js: bool,
    pub resolve_json_module: bool,
}

impl FileDiscoveryOptions {
    pub fn from_tsconfig(config_path: &Path, config: &TsConfig, out_dir: Option<&Path>) -> Self {
        let base_dir = config_path
            .parent()
            .map_or_else(|| PathBuf::from("."), Path::to_path_buf);

        let files_explicitly_set = config.files.is_some();
        let files = config
            .files
            .as_ref()
            .map(|list| list.iter().map(PathBuf::from).collect())
            .unwrap_or_default();

        Self {
            base_dir,
            files,
            files_explicitly_set,
            include: config.include.clone(),
            exclude: config.exclude.clone(),
            out_dir: out_dir.map(Path::to_path_buf),
            follow_links: false,
            allow_js: false,
            resolve_json_module: false,
        }
    }
}

pub fn discover_ts_files(options: &FileDiscoveryOptions) -> Result<Vec<PathBuf>> {
    let mut seen: HashSet<PathBuf> = HashSet::new();
    let mut explicit_files = Vec::new();

    for file in &options.files {
        let path = resolve_file_path(&options.base_dir, file);
        ensure_file_exists(&path, file)?;
        // Explicitly listed files (from CLI positional args or tsconfig "files" array)
        // are always compiled, including .js/.jsx/.mjs/.cjs files, regardless of
        // the allowJs setting. This matches tsc behavior where allowJs only controls
        // pattern-matched file discovery (include/exclude), not explicit file lists.
        let is_valid_explicit_file = is_ts_file(&path)
            || is_js_file(&path)
            || (options.resolve_json_module && is_json_file(&path));
        if is_valid_explicit_file && seen.insert(path.clone()) {
            explicit_files.push(path);
        }
    }

    let include_patterns = build_include_patterns(options);
    // tsc's `matchFiles` (compiler/utilities.ts) buckets each discovered file by
    // the FIRST include *spec* that matches it and flattens the buckets in
    // spec order — it does not merge-then-alphabetize matches across specs.
    // Each user-written include entry is one spec, even when it expands to
    // several glob patterns (a bare directory like `"src"` expands to `"src"`
    // and `"src/**/*"`, which must still share one bucket). The zero-config
    // default has exactly one spec (tsc's real default is the single pattern
    // `**/*`) even though `default_include_patterns` synthesizes several
    // per-extension glob strings for that one spec — bucketing by the
    // *expanded* pattern index instead of the originating spec index would
    // put `.ts` ahead of `.js` in the default case, which is not what tsc
    // does there (see docs/specs/TSC_ROOT_FILE_ORDER.md; this exact mistake
    // was landed in #17423 and reverted in #17428). Only an explicit
    // multi-pattern `include` (e.g. `["*.ts","*.js"]`) actually orders `.ts`
    // ahead of `.js`.
    let spec_count = include_patterns
        .iter()
        .map(|(spec_index, _)| *spec_index)
        .max()
        .map_or(0, |max_index| max_index + 1);
    let mut buckets: Vec<Vec<PathBuf>> = vec![Vec::new(); spec_count];
    if !include_patterns.is_empty() {
        let pattern_strings: Vec<String> = include_patterns
            .iter()
            .map(|(_, pattern)| pattern.clone())
            .collect();
        let spec_indices: Vec<usize> = include_patterns
            .iter()
            .map(|(spec_index, _)| *spec_index)
            .collect();
        let include_set =
            build_globset(&pattern_strings).context("failed to build include globset")?;
        let exclude_patterns = build_exclude_patterns(options);
        let exclude_set = if exclude_patterns.is_empty() {
            None
        } else {
            Some(build_globset(&exclude_patterns).context("failed to build exclude globset")?)
        };

        for walk_root in include_walk_roots(&options.base_dir, &pattern_strings) {
            let walker = WalkDir::new(&walk_root)
                .follow_links(options.follow_links)
                .into_iter()
                .filter_entry(|entry| allow_entry(entry, &walk_root, exclude_set.as_ref()));

            for entry in walker {
                let entry = entry.context("failed to read directory entry")?;
                if !entry.file_type().is_file() {
                    continue;
                }

                let path = entry.path();
                if !(is_ts_file(path) || (options.allow_js && is_js_file(path))) {
                    continue;
                }

                let Some(bucket_index) = first_matching_pattern_index(
                    path,
                    &options.base_dir,
                    &walk_root,
                    &include_set,
                    &spec_indices,
                ) else {
                    continue;
                };

                if let Some(exclude) = exclude_set.as_ref()
                    && matches_discovery_patterns(path, &options.base_dir, &walk_root, exclude)
                {
                    continue;
                }

                let resolved =
                    resolve_discovered_path(path, &options.base_dir, options.follow_links);
                if seen.insert(resolved.clone()) {
                    buckets[bucket_index].push(resolved);
                }
            }
        }
    }
    // Files within a single bucket come from directory-tree order, which is
    // not guaranteed alphabetical; sort within each bucket (tsc sorts
    // directory entries alphabetically during its own walk) without merging
    // across buckets.
    for bucket in &mut buckets {
        bucket.sort();
    }
    let discovered: Vec<PathBuf> = buckets.into_iter().flatten().collect();

    // tsc excludes `.d.ts` files from the program when a corresponding `.ts`
    // (or `.tsx`) source file exists in the same directory.  This prevents the
    // declaration file from shadowing the source file's exports.
    let discovered = exclude_shadowed_declaration_files(discovered);

    // tsc also excludes a wildcard-matched `.js`/`.jsx` (or `.mjs`/`.cjs`)
    // file when a same-stem, higher-priority source file is present in the
    // same directory — a same-named `a.ts`/`a.js` pair resolves to one
    // module, not two. Explicitly listed files (CLI positional args or a
    // tsconfig `files` array) are never shadowed.
    let discovered = exclude_shadowed_js_files(discovered, &explicit_files);

    let mut list = explicit_files;
    list.extend(discovered);
    Ok(list)
}

fn resolve_discovered_path(path: &Path, base_dir: &Path, follow_links: bool) -> PathBuf {
    if !follow_links {
        return path.to_path_buf();
    }

    // Avoid canonicalizing package-link paths whose lexical path is outside
    // node_modules but whose real target lives under node_modules. Ordinary
    // resolved package files should still canonicalize so tempdir aliases like
    // /var -> /private/var collapse to a stable path.
    let canonical = std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    let preserve_symlink_identity =
        !path_has_node_modules_component(path) && path_has_node_modules_component(&canonical);
    if preserve_symlink_identity || path_has_symlinked_package_ancestor(path, base_dir) {
        path.to_path_buf()
    } else {
        canonical
    }
}

fn path_has_symlinked_package_ancestor(path: &Path, base_dir: &Path) -> bool {
    let mut current = path.parent();
    while let Some(dir) = current {
        if dir == base_dir {
            return false;
        }
        if std::fs::symlink_metadata(dir)
            .map(|metadata| metadata.file_type().is_symlink())
            .unwrap_or(false)
        {
            let canonical = std::fs::canonicalize(dir).unwrap_or_else(|_| dir.to_path_buf());
            return path_has_node_modules_component(&canonical);
        }
        current = dir.parent();
    }
    false
}

fn path_has_node_modules_component(path: &Path) -> bool {
    path.components().any(|component| {
        matches!(
            component,
            std::path::Component::Normal(part) if part.to_str() == Some("node_modules")
        )
    })
}

fn include_walk_roots(base_dir: &Path, include_patterns: &[String]) -> Vec<PathBuf> {
    let mut roots = BTreeSet::new();
    for pattern in include_patterns {
        if is_absolute_pattern(pattern) {
            roots.insert(fixed_pattern_prefix(pattern));
        } else {
            roots.insert(base_dir.to_path_buf());
        }
    }
    roots.into_iter().collect()
}

fn is_absolute_pattern(pattern: &str) -> bool {
    Path::new(pattern).is_absolute()
}

fn fixed_pattern_prefix(pattern: &str) -> PathBuf {
    let mut prefix = PathBuf::new();
    for component in Path::new(pattern).components() {
        let text = component.as_os_str().to_string_lossy();
        if text.contains('*') || text.contains('?') || text.contains('[') {
            break;
        }
        prefix.push(component.as_os_str());
    }
    if prefix.as_os_str().is_empty() {
        PathBuf::from("/")
    } else {
        prefix
    }
}

fn matches_discovery_patterns(
    path: &Path,
    base_dir: &Path,
    walk_root: &Path,
    patterns: &GlobSet,
) -> bool {
    patterns.is_match(path)
        || path
            .strip_prefix(base_dir)
            .is_ok_and(|rel| patterns.is_match(rel))
        || path
            .strip_prefix(walk_root)
            .is_ok_and(|rel| patterns.is_match(rel))
}

/// Like [`matches_discovery_patterns`], but returns the *spec index* (see
/// [`build_include_patterns`]) of the earliest-matching include spec instead
/// of a bool. `patterns` is built from the flattened, expanded pattern list;
/// `spec_indices[i]` gives the originating spec index for `patterns`'s `i`-th
/// glob, so multiple expanded patterns from one spec (or from tsc's
/// synthesized zero-config default, which is entirely one spec) collapse to
/// the same bucket. Mirrors tsc's `matchFiles`, which assigns each file to
/// the bucket of the first include spec that matches it.
fn first_matching_pattern_index(
    path: &Path,
    base_dir: &Path,
    walk_root: &Path,
    patterns: &GlobSet,
    spec_indices: &[usize],
) -> Option<usize> {
    let min_spec_index = |glob_indices: Vec<usize>| -> Option<usize> {
        glob_indices
            .into_iter()
            .filter_map(|glob_index| spec_indices.get(glob_index).copied())
            .min()
    };
    min_spec_index(patterns.matches(path))
        .or_else(|| {
            path.strip_prefix(base_dir)
                .ok()
                .and_then(|rel| min_spec_index(patterns.matches(rel)))
        })
        .or_else(|| {
            path.strip_prefix(walk_root)
                .ok()
                .and_then(|rel| min_spec_index(patterns.matches(rel)))
        })
}

/// Include patterns to match files against, each tagged with the index of
/// the include *spec* it was expanded from. A spec is one user-written
/// `include` array entry, or — in the zero-config default case — the single
/// implicit spec tsc's real default (`**/*`) represents, even though
/// [`default_include_patterns`] synthesizes several per-extension glob
/// strings for that one spec. Bucketing discovered files by spec index
/// (rather than by expanded-pattern index) keeps every pattern derived from
/// one spec — and the entire zero-config default — in a single bucket.
fn build_include_patterns(options: &FileDiscoveryOptions) -> Vec<(usize, String)> {
    match options.include.as_ref() {
        Some(patterns) if patterns.is_empty() => Vec::new(),
        Some(patterns) => expand_include_patterns(&normalize_patterns(patterns)),
        None => {
            // Only default to **/* when the tsconfig did not explicitly set
            // `"files"`. A solution-style config like `{ "files": [], "references": [...] }`
            // must not trigger a full directory walk — tsc treats it as zero input files.
            if options.files.is_empty() && !options.files_explicitly_set {
                // tsc's real zero-config default is the single pattern `**/*`
                // (files matched, then filtered by extension) — one spec, one
                // bucket, so the result is alphabetical. Tag every synthesized
                // per-extension pattern with the same spec index (0) rather
                // than letting each one become its own bucket.
                default_include_patterns(options.allow_js, options.resolve_json_module)
                    .into_iter()
                    .map(|pattern| (0, pattern))
                    .collect()
            } else {
                Vec::new()
            }
        }
    }
}

pub fn default_include_patterns(allow_js: bool, resolve_json_module: bool) -> Vec<String> {
    default_discovery_include_patterns(allow_js, resolve_json_module)
}

/// The display string for default include patterns, matching tsc's output.
/// tsc shows `["**/*"]` as the default include in TS18003 messages, even though
/// internally it filters by file extension.
pub fn default_include_display() -> Vec<String> {
    vec!["**/*".to_string()]
}

/// Expand include patterns to match files in directories, tagging each
/// expanded pattern with the index of the user-written spec it came from.
/// A spec that expands to multiple glob patterns (e.g. a bare directory)
/// keeps them all under its own spec index, so they bucket together in
/// [`discover_ts_files`] instead of each claiming a separate bucket.
///
/// TypeScript's include patterns work as follows:
/// - `src` matches `src/` directory and expands to `src/**/*`
/// - `src/*` matches files directly in src, but for directories, adds `/**/*`
/// - Patterns with extensions (e.g., `*.ts`) are used as-is
fn expand_include_patterns(patterns: &[String]) -> Vec<(usize, String)> {
    let mut expanded = Vec::new();
    for (spec_index, pattern) in patterns.iter().enumerate() {
        // If pattern already has glob metacharacters with extensions, use as-is
        if include_pattern_has_supported_extension(pattern) {
            expanded.push((spec_index, pattern.clone()));
            continue;
        }

        // If pattern ends with /**/* or /**/*.*, it's already expanded
        if pattern.ends_with("/**/*") || pattern.ends_with("/**/*.*") {
            expanded.push((spec_index, pattern.clone()));
            continue;
        }

        if is_terminal_wildcard_pattern(pattern) {
            let base = pattern.trim_end_matches('/');
            expanded.push((spec_index, base.to_string()));
            expanded.push((spec_index, format!("{base}/**/*")));
            continue;
        }

        // Directory pattern (no extension or glob at end) - expand to match all files
        expanded.push((spec_index, directory_recursive_glob(pattern)));
    }
    expanded
}

/// Expand a bare directory include entry to its recursive glob.
///
/// A directory spec matches every supported file beneath it, mirroring tsc's
/// `getFileMatcherPatterns`, which appends `/**/*` to a directory. The
/// current-directory specs `"."` and `"./"` (the latter normalized to an empty
/// string by [`normalize_patterns`]) denote the project root: they must expand
/// to a root-relative `**/*`, never `./**/*` or `/**/*`. The discovery walk
/// matches files by their path relative to `base_dir` (see
/// [`matches_discovery_patterns`]), and `globset` does not strip a leading `./`
/// or treat a leading `/` as the walk root, so those spellings would match
/// nothing and surface a spurious TS18003 "No inputs were found".
fn directory_recursive_glob(pattern: &str) -> String {
    let base = pattern.trim_end_matches('/');
    if base.is_empty() || base == "." {
        "**/*".to_string()
    } else {
        format!("{base}/**/*")
    }
}

fn is_terminal_wildcard_pattern(pattern: &str) -> bool {
    let pattern = pattern.trim_end_matches('/');
    pattern == "*" || pattern.ends_with("/*")
}

fn build_exclude_patterns(options: &FileDiscoveryOptions) -> Vec<String> {
    let mut patterns = match options.exclude.as_ref() {
        Some(patterns) => normalize_patterns(patterns),
        None => normalize_patterns(
            &DEFAULT_EXCLUDES
                .iter()
                .map(std::string::ToString::to_string)
                .collect::<Vec<_>>(),
        ),
    };

    if options.exclude.is_none()
        && let Some(out_dir) = options.out_dir.as_ref()
        && let Some(out_pattern) = path_to_pattern(&options.base_dir, out_dir)
    {
        patterns.push(out_pattern);
    }

    expand_exclude_patterns(&patterns)
}

fn normalize_patterns(patterns: &[String]) -> Vec<String> {
    patterns
        .iter()
        .filter_map(|pattern| {
            let trimmed = pattern.trim();
            if trimmed.is_empty() {
                return None;
            }
            // Normalize path separators and strip leading "./" prefix
            // TypeScript treats "./**/*.ts" the same as "**/*.ts"
            let normalized = trimmed.replace('\\', "/");
            let stripped = normalized.strip_prefix("./").unwrap_or(&normalized);
            Some(stripped.to_string())
        })
        .collect()
}

fn expand_exclude_patterns(patterns: &[String]) -> Vec<String> {
    let mut expanded = Vec::new();
    for pattern in patterns {
        expanded.push(pattern.clone());
        if let Some(root_relative) = pattern.strip_prefix("**/") {
            expanded.push(root_relative.to_string());
        }
        if !contains_glob_meta(pattern) && !pattern.ends_with("/**") {
            let base = pattern.trim_end_matches('/');
            expanded.push(format!("{base}/**"));
            // tsc treats bare directory names (like "node_modules") as matching
            // at any depth in the tree — not just at the project root. Expand to
            // include **/name and **/name/** so nested occurrences are excluded.
            if !pattern.contains('/') {
                expanded.push(format!("**/{base}"));
                expanded.push(format!("**/{base}/**"));
            }
        }
    }
    expanded
}

fn contains_glob_meta(pattern: &str) -> bool {
    pattern.contains('*') || pattern.contains('?') || pattern.contains('[') || pattern.contains(']')
}

fn build_globset(patterns: &[String]) -> Result<GlobSet> {
    let mut builder = GlobSetBuilder::new();
    for pattern in patterns {
        let glob =
            Glob::new(pattern).with_context(|| format!("invalid glob pattern '{pattern}'"))?;
        builder.add(glob);
    }

    Ok(builder.build()?)
}

fn allow_entry(entry: &DirEntry, base_dir: &Path, exclude: Option<&GlobSet>) -> bool {
    let Some(exclude) = exclude else {
        return true;
    };

    let path = entry.path();
    if path == base_dir {
        return true;
    }
    if exclude.is_match(path) {
        return false;
    }

    // Use safe path handling instead of unwrap_or for panic hardening
    let rel_path = match path.strip_prefix(base_dir) {
        Ok(stripped) => stripped,
        Err(_) => {
            // If path is not under base_dir, use the path itself for matching
            return !exclude.is_match(path);
        }
    };
    !exclude.is_match(rel_path)
}

fn resolve_file_path(base_dir: &Path, file: &Path) -> PathBuf {
    if file.is_absolute() {
        file.to_path_buf()
    } else {
        base_dir.join(file)
    }
}

fn ensure_file_exists(path: &Path, original: &Path) -> Result<()> {
    if !path.exists() {
        // Use the original (relative) path in the error message to match tsc's TS6053 format.
        // The marker prefix lets the CLI layer detect this and format it properly.
        bail!("TS6053: File '{}' not found.", original.display());
    }

    if !path.is_file() {
        // The CLI layer formats this marker into tsc's full TS6231 diagnostic.
        // tsc normalizes a bare `.` to an empty display path.
        let display = original.display().to_string();
        let normalized = if display == "." {
            String::new()
        } else {
            display
        };
        bail!("TS6231: {normalized}");
    }

    Ok(())
}

/// When both a `.d.ts` declaration file and a `.ts`/`.tsx` source file exist
/// with the same stem, tsc excludes the declaration file from the program.
/// This replicates that behavior: for each `.d.ts`/`.d.mts`/`.d.cts` file in
/// the set, drop it if the corresponding source extension is also present.
fn exclude_shadowed_declaration_files(files: Vec<PathBuf>) -> Vec<PathBuf> {
    // Quick exit when the set is small enough that no shadowing is possible.
    if files.len() <= 1 {
        return files;
    }

    // Build a set of all non-declaration stems for O(1) lookup.
    let mut source_stems: BTreeSet<PathBuf> = BTreeSet::new();
    for path in &files {
        if let Some(stem) = strip_ts_source_extension_from_path(path) {
            source_stems.insert(stem);
        }
    }

    files
        .into_iter()
        .filter(|path| {
            if let Some(decl_stem) = strip_ts_declaration_extension_from_path(path) {
                // Keep the declaration file only if no source file shares its stem.
                !source_stems.contains(&decl_stem)
            } else {
                true
            }
        })
        .collect()
}

/// Extension families used by [`exclude_shadowed_js_files`], each ordered
/// from highest to lowest priority. Oracle-verified (pinned `tsc` 7.0.2):
/// `.ts`/`.tsx` shadow a same-stem `.js`/`.jsx` (and `.js` alone shadows a
/// same-stem `.jsx`); `.mts` shadows only a same-stem `.mjs`; `.cts` shadows
/// only a same-stem `.cjs`. The three families are independent — an `.mts`
/// file never shadows, or is shadowed by, a `.js`/`.cjs` file.
const JS_SHADOW_FAMILIES: &[&[&str]] = &[
    &[".ts", ".tsx", ".js", ".jsx"],
    &[".mts", ".mjs"],
    &[".cts", ".cjs"],
];

/// tsc excludes a wildcard-discovered `.js`-family file when a higher-priority
/// source file with the same stem is present (see [`JS_SHADOW_FAMILIES`]).
/// This mirrors that for `include`-glob-matched files. Explicitly listed
/// files (CLI positional args or a tsconfig `files` array) are never removed,
/// even when a higher-priority sibling shadows them — only what a lower-
/// priority *wildcard* candidate loses to a higher-priority one changes.
fn exclude_shadowed_js_files(files: Vec<PathBuf>, explicit_files: &[PathBuf]) -> Vec<PathBuf> {
    if files.len() <= 1 {
        return files;
    }
    let explicit: std::collections::HashSet<&PathBuf> = explicit_files.iter().collect();

    let mut to_remove: BTreeSet<PathBuf> = BTreeSet::new();
    for family in JS_SHADOW_FAMILIES {
        // stem -> best (lowest-index, i.e. highest priority) extension seen for that stem.
        let mut best_priority: std::collections::HashMap<PathBuf, usize> =
            std::collections::HashMap::new();
        let mut members: Vec<(PathBuf, usize, &PathBuf)> = Vec::new();
        for path in &files {
            let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
                continue;
            };
            let Some((priority, ext)) = family
                .iter()
                .enumerate()
                .find(|(_, ext)| name.ends_with(*ext))
            else {
                continue;
            };
            let stem = path.with_file_name(&name[..name.len() - ext.len()]);
            best_priority
                .entry(stem.clone())
                .and_modify(|p| *p = (*p).min(priority))
                .or_insert(priority);
            members.push((stem, priority, path));
        }
        for (stem, priority, path) in members {
            if explicit.contains(path) {
                continue;
            }
            if best_priority
                .get(&stem)
                .is_some_and(|&best| best < priority)
            {
                to_remove.insert(path.clone());
            }
        }
    }

    files
        .into_iter()
        .filter(|p| !to_remove.contains(p))
        .collect()
}

fn path_to_pattern(base_dir: &Path, path: &Path) -> Option<String> {
    let rel = if path.is_absolute() {
        path.strip_prefix(base_dir).ok()?.to_path_buf()
    } else {
        path.to_path_buf()
    };
    let value = rel.to_string_lossy().replace('\\', "/");
    if value.is_empty() { None } else { Some(value) }
}

/// Compute a relative path from `base` to `path`, collapsing common prefix
/// components and emitting `..` for each remaining component of `base`.
///
/// Returns `None` only when both paths are absolute and share no common prefix
/// (e.g. different drive letters on Windows).
pub fn diff_paths(path: &Path, base: &Path) -> Option<PathBuf> {
    use std::path::Component;
    let path_components: Vec<Component<'_>> = path.components().collect();
    let base_components: Vec<Component<'_>> = base.components().collect();
    let common_len = path_components
        .iter()
        .zip(base_components.iter())
        .take_while(|(a, b)| a == b)
        .count();
    if common_len == 0 && path.is_absolute() && base.is_absolute() {
        return None;
    }
    let mut result = PathBuf::new();
    for _ in common_len..base_components.len() {
        result.push("..");
    }
    for component in &path_components[common_len..] {
        result.push(component);
    }
    Some(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn unique_temp_dir(label: &str) -> PathBuf {
        let mut dir = std::env::temp_dir();
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        dir.push(format!(
            "tsz_fs_unit_{label}_{}_{}",
            std::process::id(),
            nanos
        ));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn test_build_include_patterns_defaults_only_when_files_are_not_explicit() {
        let implicit_options = FileDiscoveryOptions {
            base_dir: PathBuf::from("."),
            files: Vec::new(),
            files_explicitly_set: false,
            include: None,
            exclude: None,
            out_dir: None,
            follow_links: false,
            allow_js: false,
            resolve_json_module: false,
        };
        let implicit_patterns = build_include_patterns(&implicit_options);
        let pattern_strings: Vec<&str> = implicit_patterns
            .iter()
            .map(|(_, pattern)| pattern.as_str())
            .collect();
        assert_eq!(
            pattern_strings,
            vec![
                "*.ts", "*.tsx", "*.mts", "*.cts", "**/*.ts", "**/*.tsx", "**/*.mts", "**/*.cts"
            ]
        );
        // The zero-config default is ONE spec (tsc's real default is the
        // single pattern `**/*`), even though it synthesizes several
        // per-extension glob strings — every one of them must share spec
        // index 0 so they bucket together instead of ordering `.ts` ahead
        // of `.js` the way an explicit multi-pattern `include` would.
        assert!(
            implicit_patterns
                .iter()
                .all(|(spec_index, _)| *spec_index == 0),
            "zero-config default patterns must all share spec index 0"
        );

        let explicit_options = FileDiscoveryOptions {
            files_explicitly_set: true,
            ..implicit_options
        };
        assert!(build_include_patterns(&explicit_options).is_empty());
    }

    #[test]
    fn test_build_include_patterns_include_json_when_enabled() {
        let options = FileDiscoveryOptions {
            base_dir: PathBuf::from("."),
            files: Vec::new(),
            files_explicitly_set: false,
            include: None,
            exclude: None,
            out_dir: None,
            follow_links: false,
            allow_js: false,
            resolve_json_module: true,
        };

        let pattern_strings: Vec<String> = build_include_patterns(&options)
            .into_iter()
            .map(|(_, pattern)| pattern)
            .collect();
        assert_eq!(
            pattern_strings,
            vec![
                "*.ts", "*.tsx", "*.mts", "*.cts", "**/*.ts", "**/*.tsx", "**/*.mts", "**/*.cts",
            ]
        );
    }

    #[test]
    fn test_normalize_patterns_trims_drops_empty_and_normalizes_prefixes() {
        let normalized = normalize_patterns(&[
            "  ./src\\nested  ".to_string(),
            "".to_string(),
            "   ".to_string(),
            ".\\tests\\case.ts".to_string(),
        ]);

        assert_eq!(normalized, vec!["src/nested", "tests/case.ts"]);
    }

    #[test]
    fn test_expand_include_patterns_preserves_explicit_files_and_expands_directories() {
        let expanded: Vec<String> = expand_include_patterns(&[
            "src".to_string(),
            "tests/".to_string(),
            "src/*".to_string(),
            "already/**/*".to_string(),
            "index.ts".to_string(),
            "subdir/*.tsx".to_string(),
        ])
        .into_iter()
        .map(|(_, pattern)| pattern)
        .collect();

        assert_eq!(
            expanded,
            vec![
                "src/**/*".to_string(),
                "tests/**/*".to_string(),
                "src/*".to_string(),
                "src/*/**/*".to_string(),
                "already/**/*".to_string(),
                "index.ts".to_string(),
                "subdir/*.tsx".to_string(),
            ]
        );
    }

    #[test]
    fn test_expand_include_patterns_keeps_one_spec_index_per_directory_entry() {
        // "src/*" is one user-written spec that expands to two glob patterns
        // ("src/*" itself and "src/*/**/*"); both must carry the same spec
        // index so discover_ts_files buckets them together instead of
        // letting the expansion silently create a second bucket.
        let expanded = expand_include_patterns(&["src/*".to_string(), "*.ts".to_string()]);
        let spec_indices: Vec<usize> = expanded.iter().map(|(index, _)| *index).collect();
        assert_eq!(spec_indices, vec![0, 0, 1]);
    }

    #[test]
    fn test_expand_include_current_directory_is_root_recursive() {
        // tsc expands a directory spec to `<dir>/**/*`; the current-directory
        // spellings `"."` and `"./"` (the latter normalized to "") must become a
        // root-relative `**/*`, not `./**/*` or `/**/*` (which globset cannot
        // match against discovery-relative paths). See `directory_recursive_glob`.
        let patterns_only = |patterns: Vec<(usize, String)>| -> Vec<String> {
            patterns.into_iter().map(|(_, pattern)| pattern).collect()
        };
        assert_eq!(
            patterns_only(expand_include_patterns(&normalize_patterns(&[
                ".".to_string()
            ]))),
            vec!["**/*".to_string()]
        );
        assert_eq!(
            patterns_only(expand_include_patterns(&normalize_patterns(&[
                "./".to_string()
            ]))),
            vec!["**/*".to_string()]
        );
        assert_eq!(
            patterns_only(expand_include_patterns(&normalize_patterns(&[
                "./src".to_string()
            ]))),
            vec!["src/**/*".to_string()]
        );
    }

    #[test]
    fn test_discover_current_directory_include_recurses() {
        // Regression: `include: ["."]` must discover every nested source file
        // (matching tsc), not resolve to zero inputs / TS18003.
        let dir = unique_temp_dir("current_dir_include");
        fs::create_dir_all(dir.join("src/nested")).unwrap();
        fs::write(dir.join("top.ts"), "export const top = 1;").unwrap();
        fs::write(dir.join("src/a.ts"), "export const a = 1;").unwrap();
        fs::write(dir.join("src/nested/b.ts"), "export const b = 1;").unwrap();

        for spec in ["./", "."] {
            let options = FileDiscoveryOptions {
                base_dir: dir.clone(),
                files: Vec::new(),
                files_explicitly_set: false,
                include: Some(vec![spec.to_string()]),
                exclude: None,
                out_dir: None,
                follow_links: false,
                allow_js: false,
                resolve_json_module: false,
            };

            let result = discover_ts_files(&options).unwrap();
            for expected in ["top.ts", "src/a.ts", "src/nested/b.ts"] {
                assert!(
                    result.iter().any(|path| path.ends_with(expected)),
                    "include [{spec:?}] should discover {expected}, got: {result:?}"
                );
            }
        }

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_discover_terminal_include_star_matches_direct_files() {
        let dir = unique_temp_dir("terminal_include_star");
        fs::create_dir_all(dir.join("src/nested")).unwrap();
        fs::write(dir.join("src/a.js"), "const direct = 1;").unwrap();
        fs::write(dir.join("src/nested/b.js"), "const nested = 1;").unwrap();

        let options = FileDiscoveryOptions {
            base_dir: dir.clone(),
            files: Vec::new(),
            files_explicitly_set: false,
            include: Some(vec!["src/*".to_string()]),
            exclude: None,
            out_dir: None,
            follow_links: false,
            allow_js: true,
            resolve_json_module: false,
        };

        let result = discover_ts_files(&options).unwrap();
        assert!(
            result.iter().any(|path| path.ends_with("src/a.js")),
            "terminal include star should match direct files, got: {result:?}"
        );
        assert!(
            result.iter().any(|path| path.ends_with("src/nested/b.js")),
            "terminal include star should also recurse through matched directories, got: {result:?}"
        );

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_build_exclude_patterns_adds_defaults_and_relative_out_dir() {
        let base_dir = PathBuf::from("/repo");
        let options = FileDiscoveryOptions {
            base_dir: base_dir.clone(),
            files: Vec::new(),
            files_explicitly_set: false,
            include: None,
            exclude: None,
            out_dir: Some(base_dir.join("dist")),
            follow_links: false,
            allow_js: false,
            resolve_json_module: false,
        };

        let patterns = build_exclude_patterns(&options);

        assert!(patterns.contains(&"node_modules".to_string()));
        assert!(patterns.contains(&"**/node_modules/**".to_string()));
        assert!(patterns.contains(&"dist".to_string()));
        assert!(patterns.contains(&"dist/**".to_string()));
    }

    #[test]
    fn test_leading_globstar_exclude_matches_include_root_relative_path() {
        let dir = unique_temp_dir("leading_globstar_exclude");
        fs::create_dir_all(dir.join("src/dialect/mssql")).unwrap();
        fs::create_dir_all(dir.join("src/dialect/mysql")).unwrap();
        fs::write(
            dir.join("src/dialect/mssql/skip.ts"),
            "export const skip = 1;",
        )
        .unwrap();
        fs::write(
            dir.join("src/dialect/mysql/keep.ts"),
            "export const keep = 1;",
        )
        .unwrap();

        let options = FileDiscoveryOptions {
            base_dir: dir.clone(),
            files: Vec::new(),
            files_explicitly_set: false,
            include: Some(vec!["src/**/*.ts".to_string()]),
            exclude: Some(vec!["**/dialect/mssql/**".to_string()]),
            out_dir: None,
            follow_links: false,
            allow_js: false,
            resolve_json_module: false,
        };

        let result = discover_ts_files(&options).unwrap();
        assert!(
            result
                .iter()
                .any(|path| path.ends_with("src/dialect/mysql/keep.ts")),
            "expected non-excluded mysql file, got: {result:?}"
        );
        assert!(
            !result
                .iter()
                .any(|path| path.ends_with("src/dialect/mssql/skip.ts")),
            "leading globstar exclude should match paths relative to the include root, got: {result:?}"
        );

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_allow_entry_handles_paths_outside_base_dir() {
        let base_dir = unique_temp_dir("base");
        let outside_dir = unique_temp_dir("outside");
        let outside_file = outside_dir.join("skip.ts");
        fs::write(&outside_file, "export const skip = 1;").unwrap();

        let exclude = build_globset(&[outside_file.to_string_lossy().to_string()]).unwrap();
        let entry = walkdir::WalkDir::new(&outside_file)
            .max_depth(0)
            .into_iter()
            .next()
            .unwrap()
            .unwrap();

        assert!(!allow_entry(&entry, &base_dir, Some(&exclude)));

        let _ = fs::remove_dir_all(&base_dir);
        let _ = fs::remove_dir_all(&outside_dir);
    }

    #[test]
    fn test_module_file_predicates_distinguish_ts_js_and_json() {
        assert!(is_ts_file(Path::new("types.d.ts")));
        assert!(is_ts_file(Path::new("types.d.mts")));
        assert!(is_valid_module_file(Path::new("config.json")));
        assert!(!is_valid_module_file(Path::new("script.js")));
        assert!(is_valid_module_or_js_file(Path::new("script.js")));
        assert!(!is_valid_module_or_js_file(Path::new("README.md")));
    }

    #[test]
    fn test_path_to_pattern_handles_absolute_relative_and_empty_paths() {
        let base_dir = Path::new("/repo");
        assert_eq!(
            path_to_pattern(base_dir, Path::new("src\\nested")),
            Some("src/nested".to_string())
        );
        assert_eq!(
            path_to_pattern(base_dir, Path::new("/repo/dist")),
            Some("dist".to_string())
        );
        assert_eq!(path_to_pattern(base_dir, Path::new("")), None);
        assert_eq!(path_to_pattern(base_dir, Path::new("/other/place")), None);
    }

    #[test]
    fn test_path_has_node_modules_component_matches_whole_component() {
        assert!(path_has_node_modules_component(Path::new(
            "project/node_modules/pkg/index.d.ts"
        )));
        assert!(path_has_node_modules_component(Path::new(
            "/repo/node_modules"
        )));
        assert!(!path_has_node_modules_component(Path::new(
            "project/not_node_modules/pkg/index.d.ts"
        )));
        assert!(!path_has_node_modules_component(Path::new(
            "project/node_modules_cache/pkg/index.d.ts"
        )));
    }

    #[test]
    fn test_ensure_file_exists_rejects_directory_paths() {
        let dir = unique_temp_dir("directory");
        let err = ensure_file_exists(&dir, Path::new("directory")).unwrap_err();
        let msg = err.to_string();
        assert_eq!(msg, "TS6231: directory");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_ensure_file_exists_normalizes_current_dir_to_empty() {
        let dir = unique_temp_dir("dot");
        let err = ensure_file_exists(&dir, Path::new(".")).unwrap_err();
        let msg = err.to_string();
        assert_eq!(msg, "TS6231: ");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_build_globset_reports_invalid_pattern() {
        let err = build_globset(&["[".to_string()]).unwrap_err();
        assert!(err.to_string().contains("invalid glob pattern"));
    }

    #[test]
    fn test_discover_explicitly_listed_js_file_without_allow_js() {
        // Explicitly listed .js files should be included even when allow_js is false.
        // This matches tsc behavior where CLI positional args and tsconfig "files"
        // entries are always compiled regardless of the allowJs setting.
        let dir = std::env::temp_dir().join("tsz_fs_test_explicit_js");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("app.ts"), "const x = 1;").unwrap();
        fs::write(dir.join("lib.js"), "var y = 2;").unwrap();

        let options = FileDiscoveryOptions {
            base_dir: dir.clone(),
            files: vec![PathBuf::from("app.ts"), PathBuf::from("lib.js")],
            files_explicitly_set: true,
            include: None,
            exclude: None,
            out_dir: None,
            follow_links: false,
            allow_js: false, // NOT set, but .js should still be included
            resolve_json_module: false,
        };

        let result = discover_ts_files(&options).unwrap();
        assert!(
            result.iter().any(|p| p.ends_with("app.ts")),
            "explicitly listed .ts file should be included"
        );
        assert!(
            result.iter().any(|p| p.ends_with("lib.js")),
            "explicitly listed .js file should be included even without allowJs"
        );

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_discover_explicit_files_preserves_list_order() {
        let dir = std::env::temp_dir().join("tsz_fs_test_explicit_order");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("b.js"), "let a = 10;").unwrap();
        fs::write(dir.join("a.ts"), "let b = 30;").unwrap();

        let options = FileDiscoveryOptions {
            base_dir: dir.clone(),
            files: vec![PathBuf::from("b.js"), PathBuf::from("a.ts")],
            files_explicitly_set: true,
            include: None,
            exclude: None,
            out_dir: None,
            follow_links: false,
            allow_js: false,
            resolve_json_module: false,
        };

        let result = discover_ts_files(&options).unwrap();
        let names: Vec<_> = result
            .iter()
            .map(|path| path.file_name().unwrap().to_string_lossy().to_string())
            .collect();
        assert_eq!(names, vec!["b.js", "a.ts"]);

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_discover_wildcard_matched_ts_precedes_alphabetically_earlier_js() {
        // tsc's `matchFiles` (compiler/utilities.ts) buckets each discovered
        // file by the FIRST include pattern that matches it and flattens the
        // buckets in include-list order; it does not merge every match into
        // one alphabetically sorted list. Because `*.ts`-family patterns are
        // listed ahead of `*.js`-family ones (the exact list used by tsc's own
        // test harness, and by tsz's default discovery), every `.ts` file in
        // a project must sort ahead of every `.js` file, even when the `.js`
        // file's name is alphabetically earlier. This determines which
        // cross-file `var` declaration a mixed `.ts`/`.js` project treats as
        // primary for TS2403 declaration-merge checks.
        let dir = std::env::temp_dir().join("tsz_fs_test_ts_before_js_wildcard_order");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("a.js"), "var x = function(){};").unwrap();
        fs::write(dir.join("b.ts"), "var x = 1;").unwrap();

        let options = FileDiscoveryOptions {
            base_dir: dir.clone(),
            files: vec![],
            files_explicitly_set: false,
            include: Some(
                [
                    "*.ts", "*.tsx", "*.js", "*.jsx", "**/*.ts", "**/*.tsx", "**/*.js", "**/*.jsx",
                ]
                .into_iter()
                .map(String::from)
                .collect(),
            ),
            exclude: None,
            out_dir: None,
            follow_links: false,
            allow_js: true,
            resolve_json_module: false,
        };

        let result = discover_ts_files(&options).unwrap();
        let names: Vec<_> = result
            .iter()
            .map(|path| path.file_name().unwrap().to_string_lossy().to_string())
            .collect();
        assert_eq!(
            names,
            vec!["b.ts".to_string(), "a.js".to_string()],
            "a `.ts` file must sort ahead of an alphabetically-earlier `.js` file when \
             discovered through a multi-extension include list, matching tsc's per-pattern \
             bucketing instead of a global alphabetical merge"
        );

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_discover_default_include_stays_alphabetical_across_extensions() {
        // Counterpart to `test_discover_wildcard_matched_ts_precedes_alphabetically_earlier_js`:
        // tsc's REAL zero-config default is the single pattern `**/*` (files
        // matched, then filtered by extension) — one spec, one bucket, so the
        // result is alphabetical, extension family notwithstanding. Bucketing
        // by the *expanded* per-extension pattern list (rather than by
        // originating spec) would put every `.ts` file ahead of every `.js`
        // file here too, which is exactly the regression #17423 landed and
        // #17428 reverted (see docs/specs/TSC_ROOT_FILE_ORDER.md). With no
        // explicit `include`, `default_include_patterns` still synthesizes a
        // multi-pattern per-extension list for discovery, but every pattern
        // in it must collapse to spec index 0.
        let dir = std::env::temp_dir().join("tsz_fs_test_default_include_alphabetical");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("a.js"), "var x = 1;").unwrap();
        fs::write(dir.join("b.ts"), "var x = \"s\";").unwrap();

        let options = FileDiscoveryOptions {
            base_dir: dir.clone(),
            files: vec![],
            files_explicitly_set: false,
            include: None,
            exclude: None,
            out_dir: None,
            follow_links: false,
            allow_js: true,
            resolve_json_module: false,
        };

        let result = discover_ts_files(&options).unwrap();
        let names: Vec<_> = result
            .iter()
            .map(|path| path.file_name().unwrap().to_string_lossy().to_string())
            .collect();
        assert_eq!(
            names,
            vec!["a.js".to_string(), "b.ts".to_string()],
            "with no explicit `include`, discovery must stay alphabetical across \
             extensions (tsc's zero-config default is the single pattern `**/*`, not a \
             per-extension bucket list)"
        );

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_discover_follow_links_preserves_symlink_ancestor_identity() {
        use std::os::unix::fs::symlink;

        let dir = std::env::temp_dir().join("tsz_fs_test_symlink_ancestor");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(dir.join("core/node_modules/package-a")).unwrap();
        fs::write(
            dir.join("core/node_modules/package-a/index.d.ts"),
            "export interface Box {}",
        )
        .unwrap();
        symlink(
            dir.join("core/node_modules/package-a"),
            dir.join("package-a"),
        )
        .unwrap();

        let options = FileDiscoveryOptions {
            base_dir: dir.clone(),
            files: vec![],
            files_explicitly_set: false,
            include: None,
            exclude: None,
            out_dir: None,
            follow_links: true,
            allow_js: false,
            resolve_json_module: false,
        };

        let result = discover_ts_files(&options).unwrap();
        assert!(
            result.iter().any(|p| p.ends_with("package-a/index.d.ts")),
            "symlinked package root should stay in its original path"
        );
        assert!(
            !result.iter().any(|p| p
                .to_string_lossy()
                .contains("core/node_modules/package-a/index.d.ts")),
            "canonical target path should not replace the symlink path"
        );

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_discover_pattern_matched_js_file_requires_allow_js() {
        // Pattern-matched .js files (from include/exclude) should NOT be included
        // when allow_js is false. This is the correct tsc behavior.
        let dir = std::env::temp_dir().join("tsz_fs_test_pattern_js");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(dir.join("src")).unwrap();
        fs::write(dir.join("src/app.ts"), "const x = 1;").unwrap();
        fs::write(dir.join("src/lib.js"), "var y = 2;").unwrap();

        // Without allowJs, pattern-matched .js files are excluded
        let options = FileDiscoveryOptions {
            base_dir: dir.clone(),
            files: vec![],
            files_explicitly_set: false,
            include: Some(vec!["src".to_string()]),
            exclude: None,
            out_dir: None,
            follow_links: false,
            allow_js: false,
            resolve_json_module: false,
        };

        let result = discover_ts_files(&options).unwrap();
        assert!(
            result.iter().any(|p| p.ends_with("app.ts")),
            ".ts file should be included from pattern"
        );
        assert!(
            !result.iter().any(|p| p.ends_with("lib.js")),
            ".js file should NOT be included from pattern without allowJs"
        );

        // With allowJs, pattern-matched .js files are included
        let options_with_js = FileDiscoveryOptions {
            base_dir: dir.clone(),
            files: vec![],
            files_explicitly_set: false,
            include: Some(vec!["src".to_string()]),
            exclude: None,
            out_dir: None,
            follow_links: false,
            allow_js: true,
            resolve_json_module: false,
        };

        let result_with_js = discover_ts_files(&options_with_js).unwrap();
        assert!(
            result_with_js.iter().any(|p| p.ends_with("lib.js")),
            ".js file should be included from pattern with allowJs"
        );

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_discover_absolute_include_walks_pattern_prefix() {
        let dir = std::env::temp_dir().join("tsz_fs_test_absolute_include");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(dir.join("base/src")).unwrap();
        fs::create_dir_all(dir.join("app")).unwrap();
        fs::write(dir.join("base/src/a.ts"), "export const x = 1;").unwrap();

        let options = FileDiscoveryOptions {
            base_dir: dir.join("app"),
            files: vec![],
            files_explicitly_set: false,
            include: Some(vec![
                dir.join("base/src/**/*.ts").to_string_lossy().into_owned(),
            ]),
            exclude: None,
            out_dir: None,
            follow_links: false,
            allow_js: false,
            resolve_json_module: false,
        };

        let result = discover_ts_files(&options).unwrap();
        assert_eq!(result, vec![dir.join("base/src/a.ts")]);

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_discover_pattern_matched_json_file_is_not_a_root() {
        let dir = std::env::temp_dir().join("tsz_fs_test_pattern_json");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(dir.join("src")).unwrap();
        fs::write(dir.join("src/app.ts"), "const x = 1;").unwrap();
        fs::write(dir.join("src/data.json"), "{ \"a\": 1 }").unwrap();

        let options = FileDiscoveryOptions {
            base_dir: dir.clone(),
            files: vec![],
            files_explicitly_set: false,
            include: Some(vec!["src".to_string()]),
            exclude: None,
            out_dir: None,
            follow_links: false,
            allow_js: false,
            resolve_json_module: false,
        };

        let result = discover_ts_files(&options).unwrap();
        assert!(
            !result.iter().any(|p| p.ends_with("data.json")),
            ".json file should not be included from patterns"
        );

        let options_with_json = FileDiscoveryOptions {
            resolve_json_module: true,
            ..options
        };
        let result_with_json = discover_ts_files(&options_with_json).unwrap();
        assert!(
            !result_with_json.iter().any(|p| p.ends_with("data.json")),
            "resolveJsonModule should not make pattern-matched JSON files roots"
        );

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_discover_excludes_json_from_default_include_even_with_resolve_json_module() {
        let dir = std::env::temp_dir().join("tsz_fs_test_config_json_excluded");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("tsconfig.json"), r#"{ "compilerOptions": {} }"#).unwrap();
        fs::write(dir.join("jsconfig.json"), r#"{ "compilerOptions": {} }"#).unwrap();
        fs::write(dir.join("data.json"), r#"{ "key": "value" }"#).unwrap();
        fs::write(dir.join("app.ts"), "const x = 1;").unwrap();

        let options = FileDiscoveryOptions {
            base_dir: dir.clone(),
            files: vec![],
            files_explicitly_set: false,
            include: None, // defaults to **/*
            exclude: None,
            out_dir: None,
            follow_links: false,
            allow_js: false,
            resolve_json_module: true,
        };

        let result = discover_ts_files(&options).unwrap();
        assert!(
            result.iter().any(|p| p.ends_with("app.ts")),
            "should discover .ts files"
        );
        assert!(!result.iter().any(|p| p.ends_with("data.json")));
        assert!(
            !result.iter().any(|p| p.ends_with("tsconfig.json")),
            "tsconfig.json must not be included as program input"
        );
        assert!(
            !result.iter().any(|p| p.ends_with("jsconfig.json")),
            "jsconfig.json must not be included as program input"
        );

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_discover_excludes_json_for_explicit_json_include() {
        let dir = std::env::temp_dir().join("tsz_fs_test_explicit_config_json_excluded");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("tsconfig.json"), r#"{ "compilerOptions": {} }"#).unwrap();
        fs::write(dir.join("jsconfig.json"), r#"{ "compilerOptions": {} }"#).unwrap();
        fs::write(dir.join("data.json"), r#"{ "key": "value" }"#).unwrap();

        let options = FileDiscoveryOptions {
            base_dir: dir.clone(),
            files: vec![],
            files_explicitly_set: false,
            include: Some(vec!["*.json".to_string()]),
            exclude: None,
            out_dir: None,
            follow_links: false,
            allow_js: false,
            resolve_json_module: true,
        };

        let result = discover_ts_files(&options).unwrap();
        assert!(
            !result.iter().any(|p| p.ends_with("data.json")),
            "explicit JSON include should not make JSON files roots"
        );
        assert!(
            !result.iter().any(|p| p.ends_with("tsconfig.json")),
            "tsconfig.json must not be included as program input"
        );
        assert!(
            !result.iter().any(|p| p.ends_with("jsconfig.json")),
            "jsconfig.json must not be included as program input"
        );

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_discover_treats_d_tsx_as_tsx_source_not_shadowed_declaration() {
        let dir = std::env::temp_dir().join("tsz_fs_test_d_tsx_source");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("index.tsx"), "export const x = <div />;").unwrap();
        fs::write(dir.join("index.d.tsx"), "export const y = <div />;").unwrap();

        let options = FileDiscoveryOptions {
            base_dir: dir.clone(),
            files: vec![],
            files_explicitly_set: false,
            include: None,
            exclude: None,
            out_dir: None,
            follow_links: false,
            allow_js: false,
            resolve_json_module: false,
        };

        let result = discover_ts_files(&options).unwrap();
        assert!(
            result.iter().any(|p| p.ends_with("index.tsx")),
            "regular .tsx source should be discovered"
        );
        assert!(
            result.iter().any(|p| p.ends_with("index.d.tsx")),
            ".d.tsx should be discovered as a .tsx source, not dropped as a declaration"
        );

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_default_discovery_includes_mts_cts_and_module_js_variants() {
        // Distinct stems so none of these shadow each other (a same-stem
        // `.mts`/`.mjs` or `.cts`/`.cjs` pair is covered by the dedicated
        // `exclude_shadowed_js_files` shadowing tests below).
        let dir = std::env::temp_dir().join("tsz_fs_test_default_include_extensions");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("mfile.mts"), "export const x = 1;").unwrap();
        fs::write(dir.join("cfile.cts"), "export = 1;").unwrap();
        fs::write(dir.join("mjsfile.mjs"), "export const x = 1;").unwrap();
        fs::write(dir.join("cjsfile.cjs"), "module.exports = 1;").unwrap();

        // With allow_js: true, all module extensions should be discovered
        let options = FileDiscoveryOptions {
            base_dir: dir.clone(),
            files: vec![],
            files_explicitly_set: false,
            include: None,
            exclude: None,
            out_dir: None,
            follow_links: false,
            allow_js: true,
            resolve_json_module: false,
        };

        let result = discover_ts_files(&options).unwrap();
        assert_eq!(
            result.len(),
            4,
            "default include discovery should find .mts/.cts/.mjs/.cjs files, got: {result:?}"
        );

        // Without allow_js, only .mts/.cts should be found (not .mjs/.cjs)
        let options_no_js = FileDiscoveryOptions {
            base_dir: dir.clone(),
            files: vec![],
            files_explicitly_set: false,
            include: None,
            exclude: None,
            out_dir: None,
            follow_links: false,
            allow_js: false,
            resolve_json_module: false,
        };

        let result_no_js = discover_ts_files(&options_no_js).unwrap();
        assert_eq!(
            result_no_js.len(),
            2,
            "default include without allowJs should find .mts/.cts but not .mjs/.cjs, got: {result_no_js:?}"
        );

        let _ = fs::remove_dir_all(&dir);
    }

    /// Oracle-verified against pinned `tsc` 7.0.2: a wildcard-discovered
    /// `.js`-family file is dropped from the program when a higher-priority
    /// same-stem source file is also discovered, so a same-named `a.ts` +
    /// `a.js` pair resolves to a single module (`a.ts`), matching how tsc's
    /// own project-mode file discovery treats it. Regression coverage for
    /// the `salsa/inferingFromAny.ts` conformance false-positive: the
    /// conformance harness compiles multi-`@fileName` fixtures via a
    /// synthetic tsconfig `include` glob, so this same shadowing must apply
    /// there too, not just to hand-authored real projects.
    fn discover_names(dir: &Path, allow_js: bool) -> Vec<String> {
        let options = FileDiscoveryOptions {
            base_dir: dir.to_path_buf(),
            files: vec![],
            files_explicitly_set: false,
            include: None,
            exclude: None,
            out_dir: None,
            follow_links: false,
            allow_js,
            resolve_json_module: false,
        };
        let mut names: Vec<String> = discover_ts_files(&options)
            .unwrap()
            .into_iter()
            .map(|p| p.file_name().unwrap().to_string_lossy().to_string())
            .collect();
        names.sort();
        names
    }

    #[test]
    fn test_discover_ts_shadows_same_stem_js_wildcard_match() {
        let dir = std::env::temp_dir().join("tsz_fs_test_ts_shadows_js");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("a.ts"), "export const x = 1;").unwrap();
        fs::write(dir.join("a.js"), "module.exports.x = 1;").unwrap();

        assert_eq!(
            discover_names(&dir, true),
            vec!["a.ts".to_string()],
            "a same-stem wildcard-matched .js should be shadowed by .ts"
        );

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_discover_tsx_shadows_same_stem_jsx_cross_extension() {
        // Renamed-binder / cross-extension adjacent case: .tsx shadows a
        // same-stem .jsx even though the pair isn't the "matching" tsx/jsx
        // pair by naming convention — tsc's rule is family-wide priority,
        // not paired-extension matching (oracle-verified).
        let dir = std::env::temp_dir().join("tsz_fs_test_tsx_shadows_jsx");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("widget.tsx"), "export const x = 1;").unwrap();
        fs::write(dir.join("widget.jsx"), "module.exports.x = 1;").unwrap();

        assert_eq!(
            discover_names(&dir, true),
            vec!["widget.tsx".to_string()],
            ".tsx should shadow a same-stem .jsx"
        );

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_discover_js_shadows_same_stem_jsx_without_ts() {
        // Within the js-only tier, .js outranks .jsx for the same stem even
        // when no ts-family file is present at all (oracle-verified).
        let dir = std::env::temp_dir().join("tsz_fs_test_js_shadows_jsx");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("widget.js"), "module.exports.x = 1;").unwrap();
        fs::write(dir.join("widget.jsx"), "module.exports.x = 1;").unwrap();

        assert_eq!(
            discover_names(&dir, true),
            vec!["widget.js".to_string()],
            ".js should shadow a same-stem .jsx even without a ts sibling"
        );

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_discover_mts_shadows_only_same_stem_mjs_not_cross_family() {
        // .mts/.mjs is an independent family from .ts/.js: a same-stem .ts
        // does NOT shadow .mjs, and .mts does NOT shadow a same-stem .js
        // (oracle-verified — cross-family pairs coexist).
        let dir = std::env::temp_dir().join("tsz_fs_test_mts_family_independent");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("a.mts"), "export const x = 1;").unwrap();
        fs::write(dir.join("a.mjs"), "module.exports.x = 1;").unwrap();
        fs::write(dir.join("b.ts"), "export const y = 1;").unwrap();
        fs::write(dir.join("b.mjs"), "module.exports.y = 1;").unwrap();
        fs::write(dir.join("c.mts"), "export const z = 1;").unwrap();
        fs::write(dir.join("c.js"), "module.exports.z = 1;").unwrap();

        assert_eq!(
            discover_names(&dir, true),
            vec![
                "a.mts".to_string(),
                "b.mjs".to_string(),
                "b.ts".to_string(),
                "c.js".to_string(),
                "c.mts".to_string(),
            ],
            "only same-stem .mts/.mjs pairs shadow; cross-family pairs coexist"
        );

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_discover_cts_shadows_same_stem_cjs() {
        let dir = std::env::temp_dir().join("tsz_fs_test_cts_shadows_cjs");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("a.cts"), "export = 1;").unwrap();
        fs::write(dir.join("a.cjs"), "module.exports = 1;").unwrap();

        assert_eq!(
            discover_names(&dir, true),
            vec!["a.cts".to_string()],
            "a same-stem .cjs should be shadowed by .cts"
        );

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_discover_explicit_js_file_not_shadowed_by_same_stem_ts() {
        // Explicitly listed files (CLI positional args / tsconfig `files`)
        // are never shadowed, even when a same-stem higher-priority file
        // also exists (oracle-verified: `tsc --project` with an explicit
        // `files: ["a.ts", "a.js"]` keeps both).
        let dir = std::env::temp_dir().join("tsz_fs_test_explicit_js_not_shadowed");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("a.ts"), "export const x = 1;").unwrap();
        fs::write(dir.join("a.js"), "module.exports.x = 1;").unwrap();

        let options = FileDiscoveryOptions {
            base_dir: dir.clone(),
            files: vec![PathBuf::from("a.ts"), PathBuf::from("a.js")],
            files_explicitly_set: true,
            include: None,
            exclude: None,
            out_dir: None,
            follow_links: false,
            allow_js: false,
            resolve_json_module: false,
        };

        let mut names: Vec<String> = discover_ts_files(&options)
            .unwrap()
            .into_iter()
            .map(|p| p.file_name().unwrap().to_string_lossy().to_string())
            .collect();
        names.sort();
        assert_eq!(
            names,
            vec!["a.js".to_string(), "a.ts".to_string()],
            "explicitly listed files must not be shadowed"
        );

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_discover_ts_shadows_js_with_renamed_binder() {
        // Same rule, different stem name — proves this is structural
        // (extension-priority), not keyed off a specific file/identifier name.
        let dir = std::env::temp_dir().join("tsz_fs_test_ts_shadows_js_renamed");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("zorbaflux.ts"), "export const q = 1;").unwrap();
        fs::write(dir.join("zorbaflux.js"), "module.exports.q = 1;").unwrap();

        assert_eq!(
            discover_names(&dir, true),
            vec!["zorbaflux.ts".to_string()],
            "shadowing must not depend on the specific stem name"
        );

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_explicit_include_without_mts_excludes_mts_root() {
        let dir = std::env::temp_dir().join("tsz_fs_test_explicit_default_include_mts");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("index.mts"), "export const x = 1;").unwrap();

        // Explicit include patterns that do NOT include .mts should not discover .mts files
        let options = FileDiscoveryOptions {
            base_dir: dir.clone(),
            files: vec![],
            files_explicitly_set: false,
            include: Some(vec![
                "*.ts".to_string(),
                "*.tsx".to_string(),
                "*.js".to_string(),
                "*.jsx".to_string(),
                "**/*.ts".to_string(),
                "**/*.tsx".to_string(),
                "**/*.js".to_string(),
                "**/*.jsx".to_string(),
            ]),
            exclude: Some(vec!["node_modules".to_string()]),
            out_dir: None,
            follow_links: false,
            allow_js: true,
            resolve_json_module: false,
        };

        let result = discover_ts_files(&options).unwrap();
        assert!(
            result.is_empty(),
            "explicit include without .mts patterns should ignore .mts files, got: {result:?}"
        );

        let _ = fs::remove_dir_all(&dir);
    }
}

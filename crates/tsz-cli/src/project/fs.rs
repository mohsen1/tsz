use anyhow::{Context, Result, bail};
use globset::{Glob, GlobSet, GlobSetBuilder};
use std::cmp::Ordering;
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
    // ahead of `.js`. For the order *within* one bucket see
    // `compare_discovery_order`.
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
    // Files within a single bucket arrive in `WalkDir` order, which is not the
    // order tsc emits them in; re-order each bucket into tsc's own walk order
    // without merging across buckets.
    for bucket in &mut buckets {
        bucket.sort_by(|left, right| compare_discovery_order(left, right));
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

/// Order two discovered paths the way tsc's `matchFiles` emits them.
///
/// tsc's `visitDirectory` (`compiler/utilities.ts`) emits the files of the
/// directory it is visiting — sorted — *before* recursing into that
/// directory's subdirectories, which are themselves visited in sorted order.
/// A lexicographic sort of whole paths does not reproduce that walk: it
/// interleaves a subdirectory's files among the files of their own parent
/// whenever a parent file name sorts between two subdirectory names. For
/// `mmm.ts`, `aaa/x.ts` and `zzz/y.ts`, whole-path order yields
/// `aaa/x.ts, mmm.ts, zzz/y.ts` while tsc yields `mmm.ts, aaa/x.ts, zzz/y.ts`.
///
/// Root order decides which declaration a cross-file merge treats as primary,
/// so the difference is observable as the anchor and the reported types of
/// `TS2403`.
///
/// Rather than re-walking, compare component-wise: at the first component the
/// two paths disagree on, a path that ends there is a file of the directory
/// reached so far, while a path that continues past it lies in a
/// subdirectory of that same directory — so the file comes first. Otherwise
/// both sides are files, or both are subdirectories, and the differing
/// component breaks the tie.
fn compare_discovery_order(left: &Path, right: &Path) -> Ordering {
    let mut left_rest = left.components();
    let mut right_rest = right.components();

    loop {
        match (left_rest.next(), right_rest.next()) {
            (Some(left_component), Some(right_component)) => {
                if left_component == right_component {
                    continue;
                }
                let left_is_file = left_rest.clone().next().is_none();
                let right_is_file = right_rest.clone().next().is_none();
                return match (left_is_file, right_is_file) {
                    (true, false) => Ordering::Less,
                    (false, true) => Ordering::Greater,
                    _ => left_component.as_os_str().cmp(right_component.as_os_str()),
                };
            }
            // One path is a prefix of the other, so the shorter one names a
            // directory on the longer one's way down. Two distinct discovered
            // files never reach this.
            (None, Some(_)) => return Ordering::Less,
            (Some(_), None) => return Ordering::Greater,
            (None, None) => return Ordering::Equal,
        }
    }
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
    // An include spec is written relative to the directory holding the
    // `tsconfig.json`, so that is the interpretation to try first. Matching the
    // absolute path first would let a *later* spec claim the file: a recursive
    // glob (`**/*.ts`) matches an absolute path, while a directory-scoped spec
    // (`sub/*`) does not, so `["sub/*", "*.ts"]` bucketed `sub/nested.ts` under
    // `*.ts` and lost the spec ordering entirely. The absolute attempt remains
    // as a fallback for genuinely absolute include patterns, which no
    // base-relative path can match.
    path.strip_prefix(base_dir)
        .ok()
        .and_then(|rel| min_spec_index(patterns.matches(rel)))
        .or_else(|| min_spec_index(patterns.matches(path)))
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
mod tests;

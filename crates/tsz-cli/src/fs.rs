use anyhow::{Context, Result, bail};
use globset::{Glob, GlobSet, GlobSetBuilder};
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
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
    let mut files = BTreeSet::new();

    for file in &options.files {
        let path = resolve_file_path(&options.base_dir, file);
        ensure_file_exists(&path, file)?;
        // Explicitly listed files (from CLI positional args or tsconfig "files" array)
        // are always compiled, including .js/.jsx/.mjs/.cjs files, regardless of
        // the allowJs setting. This matches tsc behavior where allowJs only controls
        // pattern-matched file discovery (include/exclude), not explicit file lists.
        if is_ts_file(&path)
            || is_js_file(&path)
            || (options.resolve_json_module && is_json_file(&path))
        {
            files.insert(path);
        }
    }

    let include_patterns = build_include_patterns(options);
    if !include_patterns.is_empty() {
        let include_set =
            build_globset(&include_patterns).context("failed to build include globset")?;
        let exclude_patterns = build_exclude_patterns(options);
        let exclude_set = if exclude_patterns.is_empty() {
            None
        } else {
            Some(build_globset(&exclude_patterns).context("failed to build exclude globset")?)
        };

        let walker = WalkDir::new(&options.base_dir)
            .follow_links(options.follow_links)
            .into_iter()
            .filter_entry(|entry| allow_entry(entry, &options.base_dir, exclude_set.as_ref()));

        for entry in walker {
            let entry = entry.context("failed to read directory entry")?;
            if !entry.file_type().is_file() {
                continue;
            }

            let path = entry.path();
            if !(is_ts_file(path)
                || (options.allow_js && is_js_file(path))
                || (options.resolve_json_module && is_json_file(path)))
            {
                continue;
            }

            // tsc never includes config files (tsconfig.json, jsconfig.json) as
            // program inputs, even when resolveJsonModule is enabled. Skip them
            // during pattern-based discovery.
            if is_json_file(path) && is_config_json(path) {
                continue;
            }

            let rel_path = path.strip_prefix(&options.base_dir).unwrap_or(path);
            if !include_set.is_match(rel_path) {
                continue;
            }

            if let Some(exclude) = exclude_set.as_ref()
                && exclude.is_match(rel_path)
            {
                continue;
            }

            // Avoid canonicalizing unless following links; canonicalizing can change
            // the base prefix (e.g., /var -> /private/var on macOS) which breaks
            // relative path expectations in the CLI.
            let resolved = if options.follow_links {
                std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
            } else {
                path.to_path_buf()
            };
            files.insert(resolved);
        }
    }

    let mut list: Vec<PathBuf> = files.into_iter().collect();
    list.sort_by(|a, b| a.to_string_lossy().cmp(&b.to_string_lossy()));
    Ok(list)
}

fn build_include_patterns(options: &FileDiscoveryOptions) -> Vec<String> {
    match options.include.as_ref() {
        Some(patterns) if patterns.is_empty() => Vec::new(),
        Some(patterns) => expand_include_patterns(&normalize_patterns(patterns)),
        None => {
            // Only default to **/* when the tsconfig did not explicitly set
            // `"files"`. A solution-style config like `{ "files": [], "references": [...] }`
            // must not trigger a full directory walk — tsc treats it as zero input files.
            if options.files.is_empty() && !options.files_explicitly_set {
                default_include_patterns(options.allow_js)
            } else {
                Vec::new()
            }
        }
    }
}

pub fn default_include_patterns(allow_js: bool) -> Vec<String> {
    let mut patterns = vec![
        "*.ts".to_string(),
        "*.tsx".to_string(),
        "**/*.ts".to_string(),
        "**/*.tsx".to_string(),
    ];
    if allow_js {
        patterns.extend([
            "*.js".to_string(),
            "*.jsx".to_string(),
            "**/*.js".to_string(),
            "**/*.jsx".to_string(),
        ]);
    }
    patterns
}

/// Expand include patterns to match files in directories.
///
/// TypeScript's include patterns work as follows:
/// - `src` matches `src/` directory and expands to `src/**/*`
/// - `src/*` matches files directly in src, but for directories, adds `/**/*`
/// - Patterns with extensions (e.g., `*.ts`) are used as-is
fn expand_include_patterns(patterns: &[String]) -> Vec<String> {
    let mut expanded = Vec::new();
    for pattern in patterns {
        // If pattern already has glob metacharacters with extensions, use as-is
        if pattern.ends_with(".ts")
            || pattern.ends_with(".tsx")
            || pattern.ends_with(".js")
            || pattern.ends_with(".jsx")
            || pattern.ends_with(".mts")
            || pattern.ends_with(".cts")
            || pattern.ends_with(".mjs")
            || pattern.ends_with(".cjs")
        {
            expanded.push(pattern.clone());
            continue;
        }

        // If pattern ends with /**/* or /**/*.*, it's already expanded
        if pattern.ends_with("/**/*") || pattern.ends_with("/**/*.*") {
            expanded.push(pattern.clone());
            continue;
        }

        // Directory pattern (no extension or glob at end) - expand to match all files
        let base = pattern.trim_end_matches('/');
        expanded.push(format!("{base}/**/*"));
    }
    expanded
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
        bail!("path is not a file: {}", path.display());
    }

    Ok(())
}

pub(crate) fn is_js_file(path: &Path) -> bool {
    matches!(
        path.extension().and_then(|ext| ext.to_str()),
        Some("js") | Some("jsx") | Some("mjs") | Some("cjs")
    )
}

pub(crate) fn is_ts_file(path: &Path) -> bool {
    let name = match path.file_name().and_then(|name| name.to_str()) {
        Some(name) => name,
        None => return false,
    };

    if name.ends_with(".d.ts") || name.ends_with(".d.mts") || name.ends_with(".d.cts") {
        return true;
    }

    matches!(
        path.extension().and_then(|ext| ext.to_str()),
        Some("ts") | Some("tsx") | Some("mts") | Some("cts")
    )
}

/// Check if a path is a valid module file for module resolution purposes.
/// This includes TypeScript files AND .json files (which can be imported with resolveJsonModule).
/// NOTE: This intentionally excludes JS files (.js/.jsx/.mjs/.cjs). For export map
/// resolution, tsc does not accept raw JS files as valid targets — the package author
/// must provide declaration files via a `types` condition. JS files are only accepted
/// as resolution targets in non-export contexts (e.g., `imports` field, `main` field)
/// via `is_valid_module_or_js_file`.
pub(crate) fn is_valid_module_file(path: &Path) -> bool {
    is_ts_file(path) || is_json_file(path)
}

/// Like `is_valid_module_file` but also accepts JavaScript files.
/// Used in non-export resolution paths (package.json `imports` field, `main` field,
/// direct file resolution) where tsc will resolve to JS source files during
/// import-following for source discovery.
pub(crate) fn is_valid_module_or_js_file(path: &Path) -> bool {
    is_ts_file(path) || is_js_file(path) || is_json_file(path)
}

fn is_json_file(path: &Path) -> bool {
    matches!(path.extension().and_then(|ext| ext.to_str()), Some("json"))
}

/// Returns true for tsconfig.json / jsconfig.json files, which tsc excludes
/// from program inputs even when resolveJsonModule is enabled.
fn is_config_json(path: &Path) -> bool {
    path.file_name()
        .and_then(|f| f.to_str())
        .is_some_and(|name| {
            name.eq_ignore_ascii_case("tsconfig.json") || name.eq_ignore_ascii_case("jsconfig.json")
        })
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
        assert_eq!(
            build_include_patterns(&implicit_options),
            vec!["*.ts", "*.tsx", "**/*.ts", "**/*.tsx"]
        );

        let explicit_options = FileDiscoveryOptions {
            files_explicitly_set: true,
            ..implicit_options
        };
        assert!(build_include_patterns(&explicit_options).is_empty());
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
        let expanded = expand_include_patterns(&[
            "src".to_string(),
            "tests/".to_string(),
            "already/**/*".to_string(),
            "index.ts".to_string(),
            "subdir/*.tsx".to_string(),
        ]);

        assert_eq!(
            expanded,
            vec![
                "src/**/*".to_string(),
                "tests/**/*".to_string(),
                "already/**/*".to_string(),
                "index.ts".to_string(),
                "subdir/*.tsx".to_string(),
            ]
        );
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
    fn test_ensure_file_exists_rejects_directory_paths() {
        let dir = unique_temp_dir("directory");
        let err = ensure_file_exists(&dir, Path::new("directory")).unwrap_err();
        assert!(err.to_string().contains("path is not a file"));
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
    fn test_discover_pattern_matched_json_file_requires_resolve_json_module() {
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
            ".json file should NOT be included from pattern without resolveJsonModule"
        );

        let options_with_json = FileDiscoveryOptions {
            resolve_json_module: true,
            ..options
        };
        let result_with_json = discover_ts_files(&options_with_json).unwrap();
        assert!(
            result_with_json.iter().any(|p| p.ends_with("data.json")),
            ".json file should be included from pattern with resolveJsonModule"
        );

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_discover_excludes_tsconfig_json_even_with_resolve_json_module() {
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
        assert!(
            result.iter().any(|p| p.ends_with("data.json")),
            "should discover regular .json files with resolveJsonModule"
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
    fn test_default_discovery_excludes_mts_cts_and_module_js_variants() {
        let dir = std::env::temp_dir().join("tsz_fs_test_default_include_extensions");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("index.mts"), "export const x = 1;").unwrap();
        fs::write(dir.join("index.cts"), "export = 1;").unwrap();
        fs::write(dir.join("index.mjs"), "export const x = 1;").unwrap();
        fs::write(dir.join("index.cjs"), "module.exports = 1;").unwrap();

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
        assert!(
            result.is_empty(),
            "default include discovery should ignore .mts/.cts/.mjs/.cjs roots, got: {result:?}"
        );

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_explicit_default_include_excludes_mts_root() {
        let dir = std::env::temp_dir().join("tsz_fs_test_explicit_default_include_mts");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("index.mts"), "export const x = 1;").unwrap();

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
            "explicit default include should ignore a lone .mts root, got: {result:?}"
        );

        let _ = fs::remove_dir_all(&dir);
    }
}

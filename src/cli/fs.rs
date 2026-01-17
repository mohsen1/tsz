use anyhow::{Context, Result, bail};
use globset::{Glob, GlobSet, GlobSetBuilder};
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use walkdir::{DirEntry, WalkDir};

use crate::cli::config::TsConfig;

pub(crate) const DEFAULT_EXCLUDES: [&str; 3] =
    ["node_modules", "bower_components", "jspm_packages"];

#[derive(Debug, Clone)]
pub struct FileDiscoveryOptions {
    pub base_dir: PathBuf,
    pub files: Vec<PathBuf>,
    pub include: Option<Vec<String>>,
    pub exclude: Option<Vec<String>>,
    pub out_dir: Option<PathBuf>,
    pub follow_links: bool,
}

impl FileDiscoveryOptions {
    pub fn from_tsconfig(config_path: &Path, config: &TsConfig, out_dir: Option<&Path>) -> Self {
        let base_dir = config_path
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_else(|| PathBuf::from("."));

        let files = config
            .files
            .as_ref()
            .map(|list| list.iter().map(PathBuf::from).collect())
            .unwrap_or_default();

        FileDiscoveryOptions {
            base_dir,
            files,
            include: config.include.clone(),
            exclude: config.exclude.clone(),
            out_dir: out_dir.map(Path::to_path_buf),
            follow_links: false,
        }
    }
}

pub fn discover_ts_files(options: &FileDiscoveryOptions) -> Result<Vec<PathBuf>> {
    let mut files = BTreeSet::new();

    for file in &options.files {
        let path = resolve_file_path(&options.base_dir, file);
        ensure_file_exists(&path)?;
        if is_ts_file(&path) {
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
            if !is_ts_file(path) {
                continue;
            }

            let rel_path = path.strip_prefix(&options.base_dir).unwrap_or(path);
            if !include_set.is_match(rel_path) {
                continue;
            }

            if let Some(exclude) = exclude_set.as_ref() {
                if exclude.is_match(rel_path) {
                    continue;
                }
            }

            let resolved = std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
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
        Some(patterns) => normalize_patterns(patterns),
        None => {
            if options.files.is_empty() {
                vec!["**/*".to_string()]
            } else {
                Vec::new()
            }
        }
    }
}

fn build_exclude_patterns(options: &FileDiscoveryOptions) -> Vec<String> {
    let mut patterns = match options.exclude.as_ref() {
        Some(patterns) => normalize_patterns(patterns),
        None => normalize_patterns(
            &DEFAULT_EXCLUDES
                .iter()
                .map(|s| s.to_string())
                .collect::<Vec<_>>(),
        ),
    };

    if options.exclude.is_none() {
        if let Some(out_dir) = options.out_dir.as_ref() {
            if let Some(out_pattern) = path_to_pattern(&options.base_dir, out_dir) {
                patterns.push(out_pattern);
            }
        }
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
            Some(trimmed.replace('\\', "/"))
        })
        .collect()
}

fn expand_exclude_patterns(patterns: &[String]) -> Vec<String> {
    let mut expanded = Vec::new();
    for pattern in patterns {
        expanded.push(pattern.clone());
        if !contains_glob_meta(pattern) && !pattern.ends_with("/**") {
            expanded.push(format!("{}/**", pattern.trim_end_matches('/')));
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
            Glob::new(pattern).with_context(|| format!("invalid glob pattern '{}'", pattern))?;
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

fn ensure_file_exists(path: &Path) -> Result<()> {
    if !path.exists() {
        bail!("file not found: {}", path.display());
    }

    if !path.is_file() {
        bail!("path is not a file: {}", path.display());
    }

    Ok(())
}

pub(crate) fn is_ts_file(path: &Path) -> bool {
    let name = match path.file_name().and_then(|name| name.to_str()) {
        Some(name) => name,
        None => return false,
    };

    if name.ends_with(".d.ts") || name.ends_with(".d.mts") || name.ends_with(".d.cts") {
        return true;
    }

    match path.extension().and_then(|ext| ext.to_str()) {
        Some("ts") | Some("tsx") | Some("mts") | Some("cts") => true,
        _ => false,
    }
}

/// Check if a path is a valid module file for module resolution purposes.
/// This includes TypeScript files AND .json files (which can be imported with resolveJsonModule).
pub(crate) fn is_valid_module_file(path: &Path) -> bool {
    let name = match path.file_name().and_then(|name| name.to_str()) {
        Some(name) => name,
        None => return false,
    };

    if name.ends_with(".d.ts") || name.ends_with(".d.mts") || name.ends_with(".d.cts") {
        return true;
    }

    match path.extension().and_then(|ext| ext.to_str()) {
        Some("ts") | Some("tsx") | Some("mts") | Some("cts") | Some("json") => true,
        _ => false,
    }
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

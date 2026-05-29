use rustc_hash::{FxHashMap, FxHashSet};
use std::path::{Path, PathBuf};

use crate::config::{ModuleResolutionKind, PathMapping, ResolvedCompilerOptions};
use crate::fs::is_valid_module_or_js_file;
use tsz::module_resolver::{PackageType, is_path_relative};

#[allow(unused_imports)]
use super::*;

pub(crate) fn resolve_module_specifier(
    from_file: &Path,
    module_specifier: &str,
    options: &ResolvedCompilerOptions,
    base_dir: &Path,
    resolution_cache: &mut ModuleResolutionCache,
    known_files: &FxHashSet<PathBuf>,
) -> Option<PathBuf> {
    let debug = std::env::var_os("TSZ_DEBUG_RESOLVE").is_some();
    if debug {
        tracing::debug!(
            "resolve_module_specifier: from_file={from_file:?}, specifier={module_specifier:?}, resolution={:?}, base_url={:?}",
            options.effective_module_resolution(),
            options.base_url
        );
    }
    let specifier = module_specifier.trim();
    if specifier.is_empty() {
        return None;
    }
    let specifier = specifier.replace('\\', "/");
    let resolution = options.effective_module_resolution();
    if specifier.starts_with('#') {
        if is_invalid_package_import_specifier(&specifier, resolution) {
            return None;
        }
        if options.resolve_package_json_imports {
            return resolve_package_imports_specifier(
                from_file,
                &specifier,
                base_dir,
                options,
                resolution_cache,
            );
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
    } else if is_path_relative(&specifier) {
        let joined = from_dir.join(&specifier);
        candidates.extend(expand_module_path_candidates(
            &joined,
            options,
            package_type,
        ));
        for candidate in root_dirs_relative_candidates(from_dir, &specifier, options) {
            candidates.extend(expand_module_path_candidates(
                &candidate,
                options,
                package_type,
            ));
        }
    } else if matches!(resolution, ModuleResolutionKind::Classic) {
        if let Some(paths) = options.paths.as_ref()
            && let Some((mapping, wildcard)) =
                resolution_cache.select_path_mapping(paths, &specifier)
        {
            path_mapping_attempted = true;
            let base = options.base_url.as_deref().unwrap_or(base_dir);
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

        // Classic resolution always walks up the directory tree from the containing
        // file's directory, probing for <specifier>.ts/.tsx/.d.ts and related candidates.
        // This runs even when baseUrl/path-mapping candidates were generated, matching
        // TypeScript behavior where classic resolution falls back to relative ancestor checks.
        // Unlike Node resolution, Classic resolution walks up for all specifiers including
        // bare module specifiers (e.g., "module3") since it has no node_modules concept.
        {
            let mut current = from_dir.to_path_buf();
            loop {
                candidates.extend(expand_module_path_candidates(
                    &current.join(&specifier),
                    options,
                    package_type,
                ));

                match current.parent() {
                    Some(parent) if parent != current => current = parent.to_path_buf(),
                    _ => break,
                }
            }
        }
    } else {
        allow_node_modules = true;
        if let Some(paths) = options.paths.as_ref()
            && let Some((mapping, wildcard)) =
                resolution_cache.select_path_mapping(paths, &specifier)
        {
            path_mapping_attempted = true;
            let base = options.base_url.as_deref().unwrap_or(base_dir);
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

        if candidates.is_empty()
            && let Some(base_url) = options.base_url.as_ref()
        {
            candidates.extend(expand_module_path_candidates(
                &base_url.join(&specifier),
                options,
                package_type,
            ));
        }
    }

    for candidate in candidates {
        // Check if candidate exists in known files (for virtual test files) or on filesystem
        let exists = known_files.contains(&candidate)
            || (resolution_cache.file_exists(&candidate) && is_valid_module_or_js_file(&candidate));
        if debug {
            tracing::debug!("candidate={candidate:?} exists={exists}");
        }

        if exists {
            return Some(normalize_resolved_path(&candidate, options));
        }
    }

    // TypeScript falls through to Classic-style directory walking when path mappings
    // were attempted but did not resolve. This matches behavior where path mapping
    // misses are not treated as terminal failures in classic mode.
    if path_mapping_attempted && matches!(resolution, ModuleResolutionKind::Classic) {
        let mut current = from_dir.to_path_buf();
        loop {
            for candidate in
                expand_module_path_candidates(&current.join(&specifier), options, package_type)
            {
                let exists = known_files.contains(&candidate)
                    || (resolution_cache.file_exists(&candidate)
                        && is_valid_module_or_js_file(&candidate));
                if debug {
                    tracing::debug!("classic-fallback candidate={candidate:?} exists={exists}");
                }
                if exists {
                    return Some(normalize_resolved_path(&candidate, options));
                }
            }

            match current.parent() {
                Some(parent) if parent != current => current = parent.to_path_buf(),
                _ => break,
            }
        }
    }

    if allow_node_modules {
        return resolve_node_module_specifier(
            from_file,
            &specifier,
            base_dir,
            options,
            resolution_cache,
        );
    }

    None
}

pub(crate) fn root_dirs_relative_candidates(
    from_dir: &Path,
    specifier: &str,
    options: &ResolvedCompilerOptions,
) -> Vec<PathBuf> {
    if options.root_dirs.is_empty() {
        return Vec::new();
    }

    let from_dir = normalize_path(from_dir);
    let direct_candidate = normalize_path(&from_dir.join(specifier));
    let mut candidates = Vec::new();

    for origin_root in &options.root_dirs {
        let origin_root = normalize_path(origin_root);
        if from_dir.strip_prefix(&origin_root).is_err() {
            continue;
        }
        let Ok(virtual_path) = direct_candidate.strip_prefix(&origin_root) else {
            continue;
        };

        for target_root in &options.root_dirs {
            let candidate = normalize_path(&target_root.join(virtual_path));
            if candidate == direct_candidate || candidates.iter().any(|seen| seen == &candidate) {
                continue;
            }
            count_candidate_path();
            candidates.push(candidate);
        }
    }

    candidates
}

pub(crate) fn select_path_mapping(
    mappings: &[PathMapping],
    specifier: &str,
) -> Option<(usize, String)> {
    // `build_path_mappings` sorts by TypeScript precedence:
    // longest prefix, then longest pattern, then lexical pattern. The first
    // match is therefore the best match.
    for (idx, mapping) in mappings.iter().enumerate() {
        let Some(wildcard) = mapping.match_specifier(specifier) else {
            continue;
        };
        return Some((idx, wildcard));
    }

    None
}

pub(crate) fn substitute_path_target(target: &str, wildcard: &str) -> String {
    if target.contains('*') {
        target.replace('*', wildcard)
    } else {
        target.to_string()
    }
}

pub(crate) fn expand_module_path_candidates(
    path: &Path,
    options: &ResolvedCompilerOptions,
    package_type: Option<PackageType>,
) -> Vec<PathBuf> {
    let base = normalize_path(path);
    let mut default_suffixes: Vec<String> = Vec::new();
    let suffixes = if options.module_suffixes.is_empty() {
        default_suffixes.push(String::new());
        &default_suffixes
    } else {
        &options.module_suffixes
    };
    if let Some((base_no_ext, extension)) = split_path_extension(&base) {
        // Try extension substitution (.js → .ts/.tsx/.d.ts) for all resolution modes.
        // TypeScript resolves `.js` imports to `.ts` sources in all modes.
        let mut candidates = Vec::new();
        if let Some(rewritten) = node16_extension_substitution(&base, extension) {
            for candidate in rewritten {
                candidates.extend(candidates_with_suffixes(&candidate, suffixes));
            }
        }
        // Also include the original extension as fallback
        candidates.extend(candidates_with_suffixes_and_extension(
            &base_no_ext,
            extension,
            suffixes,
        ));
        return candidates;
    }

    let extensions = extension_candidates_for_resolution(options, package_type);
    let mut candidates = Vec::new();
    for ext in extensions {
        candidates.extend(candidates_with_suffixes_and_extension(&base, ext, suffixes));
    }
    let index = base.join("index");
    for ext in extensions {
        candidates.extend(candidates_with_suffixes_and_extension(
            &index, ext, suffixes,
        ));
    }
    candidates
}

pub(crate) fn expand_export_path_candidates(
    path: &Path,
    options: &ResolvedCompilerOptions,
    package_type: Option<PackageType>,
) -> Vec<PathBuf> {
    let base = normalize_path(path);
    let suffixes = &options.module_suffixes;
    if let Some((base_no_ext, extension)) = split_path_extension(&base) {
        // Package `exports` targets participate in declaration-sidecar lookup
        // during program discovery. This keeps the driver aligned with the
        // checker `ModuleResolver`, which resolves `./entry.js` to adjacent
        // `./entry.d.ts` / `./entry.d.mts` / `./entry.d.cts` files when those
        // are the type-bearing program inputs.
        let mut candidates = Vec::new();
        if let Some(rewritten) = node16_extension_substitution(&base, extension) {
            for candidate in rewritten {
                candidates.extend(candidates_with_suffixes(&candidate, suffixes));
            }
        }
        candidates.extend(candidates_with_suffixes_and_extension(
            &base_no_ext,
            extension,
            suffixes,
        ));
        return candidates;
    }

    let extensions = extension_candidates_for_resolution(options, package_type);
    let mut candidates = Vec::new();
    for ext in extensions {
        candidates.extend(candidates_with_suffixes_and_extension(&base, ext, suffixes));
    }
    if options.resolve_json_module {
        candidates.extend(candidates_with_suffixes_and_extension(
            &base, "json", suffixes,
        ));
    }
    let index = base.join("index");
    for ext in extensions {
        candidates.extend(candidates_with_suffixes_and_extension(
            &index, ext, suffixes,
        ));
    }
    if options.resolve_json_module {
        candidates.extend(candidates_with_suffixes_and_extension(
            &index, "json", suffixes,
        ));
    }
    candidates
}

pub(crate) fn split_path_extension(path: &Path) -> Option<(PathBuf, &'static str)> {
    let path_str = path.to_string_lossy();
    for ext in KNOWN_EXTENSIONS {
        if path_str.ends_with(ext) {
            let base = &path_str[..path_str.len().saturating_sub(ext.len())];
            if base.is_empty() {
                return None;
            }
            return Some((PathBuf::from(base), ext.trim_start_matches('.')));
        }
    }
    None
}

pub(crate) fn candidates_with_suffixes(path: &Path, suffixes: &[String]) -> Vec<PathBuf> {
    let Some((base, extension)) = split_path_extension(path) else {
        return Vec::new();
    };
    candidates_with_suffixes_and_extension(&base, extension, suffixes)
}

pub(crate) fn candidates_with_suffixes_and_extension(
    base: &Path,
    extension: &str,
    suffixes: &[String],
) -> Vec<PathBuf> {
    let mut candidates = Vec::new();
    for suffix in suffixes {
        if let Some(candidate) = path_with_suffix_and_extension(base, suffix, extension) {
            count_candidate_path();
            candidates.push(candidate);
        }
    }
    candidates
}

pub(crate) fn path_with_suffix_and_extension(
    base: &Path,
    suffix: &str,
    extension: &str,
) -> Option<PathBuf> {
    let file_name = base.file_name()?.to_string_lossy();
    let mut candidate = base.to_path_buf();
    let mut new_name = String::with_capacity(file_name.len() + suffix.len() + extension.len() + 1);
    new_name.push_str(&file_name);
    new_name.push_str(suffix);
    new_name.push('.');
    new_name.push_str(extension);
    candidate.set_file_name(new_name);
    Some(candidate)
}

pub(crate) fn node16_extension_substitution(path: &Path, extension: &str) -> Option<Vec<PathBuf>> {
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

pub(crate) const fn extension_candidates_for_resolution(
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

pub(crate) fn normalize_path(path: &Path) -> PathBuf {
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

pub(crate) fn normalize_resolved_path(path: &Path, options: &ResolvedCompilerOptions) -> PathBuf {
    let normalized = normalize_path(path);
    if options.preserve_symlinks {
        normalized
    } else {
        let canonical = canonicalize_or_owned(path);
        let preserve_package_link_identity = path_has_symlinked_package_ancestor(path)
            || (!has_node_modules_component(path) && has_node_modules_component(&canonical));
        if preserve_package_link_identity {
            normalized
        } else {
            canonical
        }
    }
}

/// Find the innermost `node_modules/<package>/` root for a file path.
pub(crate) fn find_node_modules_package_root(path: &Path) -> Option<PathBuf> {
    let components: Vec<_> = path.components().collect();
    for i in (0..components.len()).rev() {
        if components[i].as_os_str() == "node_modules" && i + 1 < components.len() {
            let next = components[i + 1].as_os_str().to_string_lossy();
            let pkg_end = if next.starts_with('@') {
                if i + 2 < components.len() {
                    i + 3
                } else {
                    continue;
                }
            } else {
                i + 2
            };
            if pkg_end <= components.len() {
                let mut root = PathBuf::new();
                for c in &components[..pkg_end] {
                    root.push(c);
                }
                return Some(root);
            }
        }
    }
    None
}

pub(crate) fn path_has_symlinked_package_ancestor(path: &Path) -> bool {
    let mut current = path.parent();
    while let Some(dir) = current {
        if std::fs::symlink_metadata(dir)
            .map(|metadata| metadata.file_type().is_symlink())
            .unwrap_or(false)
        {
            if is_root_alias_symlink(dir) {
                current = dir.parent();
                continue;
            }

            let canonical = std::fs::canonicalize(dir).unwrap_or_else(|_| dir.to_path_buf());
            return canonical.components().any(|component| {
                matches!(
                    component,
                    std::path::Component::Normal(part) if part.to_str() == Some("node_modules")
                )
            });
        }
        current = dir.parent();
    }
    false
}

pub(crate) fn has_node_modules_component(path: &Path) -> bool {
    path.components().any(|component| {
        matches!(
            component,
            std::path::Component::Normal(part) if part.to_str() == Some("node_modules")
        )
    })
}

pub(crate) fn is_root_alias_symlink(dir: &Path) -> bool {
    if !dir.is_absolute() {
        return false;
    }

    let Ok(relative_to_root) = dir.strip_prefix(Path::new("/")) else {
        return false;
    };
    let Ok(canonical) = std::fs::canonicalize(dir) else {
        return false;
    };
    let Ok(canonical_relative_to_root) = canonical.strip_prefix(Path::new("/")) else {
        return false;
    };

    canonical_relative_to_root.ends_with(relative_to_root)
}

/// Build a redirect map for duplicate packages (same name+version at different
/// `node_modules` paths). The shallowest copy becomes canonical.
pub(crate) fn build_duplicate_package_redirects(
    file_names: &[String],
    options: &ResolvedCompilerOptions,
) -> FxHashMap<PathBuf, PathBuf> {
    use std::collections::hash_map::Entry;

    let mut canonical_packages: FxHashMap<(String, String), (PathBuf, usize)> =
        FxHashMap::default();
    let mut package_roots: FxHashSet<PathBuf> = FxHashSet::default();
    for file_name in file_names {
        if let Some(pkg_root) = find_node_modules_package_root(Path::new(file_name)) {
            package_roots.insert(pkg_root);
        }
    }

    for root in &package_roots {
        tracing::debug!(target: "tsz::dup_pkg", root = %root.display(), "found package root");
    }

    let mut root_redirects: FxHashMap<PathBuf, PathBuf> = FxHashMap::default();
    for pkg_root in &package_roots {
        let pkg_json_path = pkg_root.join("package.json");
        let (name, version) = match std::fs::read_to_string(&pkg_json_path) {
            Ok(content) => match serde_json::from_str::<serde_json::Value>(&content) {
                Ok(val) => {
                    let name = val.get("name").and_then(|v| v.as_str()).unwrap_or("");
                    let version = val.get("version").and_then(|v| v.as_str()).unwrap_or("");
                    if name.is_empty() || version.is_empty() {
                        continue;
                    }
                    (name.to_string(), version.to_string())
                }
                Err(_) => continue,
            },
            Err(e) => {
                tracing::debug!(target: "tsz::dup_pkg", path = %pkg_json_path.display(), error = %e, "cannot read package.json");
                continue;
            }
        };
        tracing::debug!(target: "tsz::dup_pkg", name = %name, version = %version, root = %pkg_root.display(), "package found");
        let depth = pkg_root
            .components()
            .filter(|c| c.as_os_str() == "node_modules")
            .count();
        match canonical_packages.entry((name, version)) {
            Entry::Vacant(e) => {
                e.insert((pkg_root.clone(), depth));
            }
            Entry::Occupied(mut e) => {
                let (existing_root, existing_depth) = e.get().clone();
                let current_rank = (
                    depth,
                    normalize_resolved_path(pkg_root, options)
                        .to_string_lossy()
                        .into_owned(),
                );
                let existing_rank = (
                    existing_depth,
                    normalize_resolved_path(&existing_root, options)
                        .to_string_lossy()
                        .into_owned(),
                );
                if current_rank < existing_rank {
                    root_redirects.insert(existing_root, pkg_root.clone());
                    e.insert((pkg_root.clone(), depth));
                } else {
                    root_redirects.insert(pkg_root.clone(), existing_root);
                }
            }
        }
    }
    if root_redirects.is_empty() {
        return FxHashMap::default();
    }
    for (from, to) in &root_redirects {
        tracing::debug!(target: "tsz::dup_pkg", from = %from.display(), to = %to.display(), "root redirect");
    }
    let mut file_redirects: FxHashMap<PathBuf, PathBuf> = FxHashMap::default();
    for file_name in file_names {
        let file_path = Path::new(file_name);
        if let Some(pkg_root) = find_node_modules_package_root(file_path)
            && let Some(canonical_root) = root_redirects.get(&pkg_root)
            && let Ok(relative) = file_path.strip_prefix(&pkg_root)
        {
            let canonical_file = canonical_root.join(relative);
            let from = normalize_resolved_path(file_path, options);
            let to = normalize_resolved_path(&canonical_file, options);
            tracing::debug!(target: "tsz::dup_pkg", from = %from.display(), to = %to.display(), "file redirect");
            if from != to {
                file_redirects.insert(from, to);
            }
        }
    }
    file_redirects
}

pub(crate) const KNOWN_EXTENSIONS: [&str; 12] = [
    ".d.mts", ".d.cts", ".d.ts", ".mts", ".cts", ".tsx", ".ts", ".mjs", ".cjs", ".jsx", ".js",
    ".json",
];
pub(crate) const TS_EXTENSION_CANDIDATES: [&str; 7] =
    ["ts", "tsx", "d.ts", "mts", "cts", "d.mts", "d.cts"];
pub(crate) const PACKAGE_INDEX_FALLBACK_EXTENSIONS: [&str; 3] = ["ts", "tsx", "d.ts"];
pub(crate) const PACKAGE_INDEX_FALLBACK_ALLOW_JS_EXTENSIONS: [&str; 5] =
    ["ts", "tsx", "d.ts", "js", "jsx"];

pub(crate) const NODE16_MODULE_EXTENSION_CANDIDATES: [&str; 7] =
    ["mts", "d.mts", "ts", "tsx", "d.ts", "cts", "d.cts"];
pub(crate) const NODE16_COMMONJS_EXTENSION_CANDIDATES: [&str; 7] =
    ["cts", "d.cts", "ts", "tsx", "d.ts", "mts", "d.mts"];

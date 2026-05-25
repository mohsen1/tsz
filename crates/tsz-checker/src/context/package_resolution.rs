use std::path::{Component, Path, PathBuf};

use crate::module_resolution::{
    module_specifier_candidates, probe_file_name_index, resolve_specifier_via_file_index,
};

use super::CheckerContext;

impl<'a> CheckerContext<'a> {
    /// Resolve an import specifier to its target file index.
    /// Uses the `resolved_module_paths` map populated by the driver.
    /// Returns None if the import cannot be resolved (e.g., external module).
    pub fn resolve_import_target(&self, specifier: &str) -> Option<usize> {
        self.resolve_import_target_from_file(self.current_file_idx, specifier)
    }

    /// Resolve an import specifier from a specific file to its target file index.
    /// Like `resolve_import_target` but for any source file, not just the current one.
    ///
    /// Stage 1: `resolved_module_paths` for bare specifiers that may be
    /// redirected by package metadata such as `exports` or `typesVersions`.
    /// Stage 2: `global_file_name_index` (always populated by `set_all_arenas`).
    /// Stage 3: `resolved_module_paths` fallback for path-mapped imports.
    pub fn resolve_import_target_from_file(
        &self,
        source_file_idx: usize,
        specifier: &str,
    ) -> Option<usize> {
        let is_bare_specifier = !specifier.starts_with("./")
            && !specifier.starts_with("../")
            && !specifier.starts_with('/')
            && !specifier.starts_with('\\');
        if is_bare_specifier && let Some(paths) = self.resolved_module_paths.as_ref() {
            for candidate in module_specifier_candidates(specifier) {
                if let Some(target_idx) = paths.get(&(source_file_idx, candidate)) {
                    return Some(
                        self.types_versions_redirected_target_index(specifier, *target_idx)
                            .unwrap_or(*target_idx),
                    );
                }
            }
        }

        if let Some(idx) = self.global_file_name_index.as_ref() {
            // Absolute paths (both POSIX `/` and Windows `\`): empty src_dir
            // causes resolve_specifier_via_file_index to use the specifier verbatim.
            if specifier.starts_with('/') || specifier.starts_with('\\') {
                if let Some(result) = resolve_specifier_via_file_index("", specifier, idx) {
                    return Some(result);
                }
            } else if let Some(source_file_name) = self
                .all_arenas
                .as_ref()
                .and_then(|a| a.get(source_file_idx))
                .and_then(|a| a.source_files.first())
                .map(|sf| sf.file_name.as_str())
            {
                if let Some(result) =
                    resolve_specifier_via_file_index(source_file_name, specifier, idx)
                {
                    return Some(
                        self.types_versions_redirected_target_index(specifier, result)
                            .unwrap_or(result),
                    );
                }
                // Bare names (single-segment like `types` or multi-segment like
                // `packages/foo/src/bar`) that resolve_specifier_via_file_index
                // rejects as potential package subpaths are probed directly
                // against the index. External npm packages won't be in the index;
                // project-relative bare paths will match by file name or stem.
                if !specifier.starts_with("./")
                    && !specifier.starts_with("../")
                    && !specifier.starts_with('/')
                    && let Some(result) = probe_file_name_index(specifier, idx)
                {
                    return Some(
                        self.types_versions_redirected_target_index(specifier, result)
                            .unwrap_or(result),
                    );
                }
            }
        }

        if let Some(paths) = self.resolved_module_paths.as_ref() {
            for candidate in module_specifier_candidates(specifier) {
                if let Some(target_idx) = paths.get(&(source_file_idx, candidate)) {
                    return Some(
                        self.types_versions_redirected_target_index(specifier, *target_idx)
                            .unwrap_or(*target_idx),
                    );
                }
            }
        }

        None
    }

    pub(super) fn types_versions_redirected_target_index(
        &self,
        specifier: &str,
        target_idx: usize,
    ) -> Option<usize> {
        let (package_name, package_subpath) = Self::split_bare_package_specifier(specifier)?;
        let target_file_name = self
            .all_arenas
            .as_ref()
            .and_then(|arenas| arenas.get(target_idx))
            .and_then(|arena| arena.source_files.first())
            .map(|source_file| source_file.file_name.as_str())?;
        let package_root =
            Self::node_modules_package_root_for_name(target_file_name, &package_name)?;
        let package_json_text = std::fs::read_to_string(package_root.join("package.json")).ok()?;
        let package_json = serde_json::from_str::<serde_json::Value>(&package_json_text).ok()?;
        let types_versions = package_json.get("typesVersions")?.as_object()?;
        let public_subpath = package_subpath.as_deref().unwrap_or("index");

        let mut candidates = Vec::new();
        for mappings in types_versions
            .values()
            .filter_map(|value| value.as_object())
        {
            for (pattern, targets) in mappings {
                let Some(wildcard) = Self::match_types_versions_pattern(pattern, public_subpath)
                else {
                    continue;
                };
                let Some(targets) = targets.as_array() else {
                    continue;
                };
                for target in targets.iter().filter_map(|value| value.as_str()) {
                    let target = target.replace('*', &wildcard);
                    candidates.push((pattern.len(), target));
                }
            }
        }
        candidates.sort_by_key(|(pattern_len, _)| std::cmp::Reverse(*pattern_len));

        for (_, target) in candidates {
            if let Some(idx) = self.file_index_for_package_relative_path(&package_root, &target) {
                return Some(idx);
            }
        }

        None
    }

    fn file_index_for_package_relative_path(
        &self,
        package_root: &Path,
        target: &str,
    ) -> Option<usize> {
        let target = Self::normalize_package_relative_path(target);
        let target = Self::strip_resolution_extension(&target);
        let arenas = self.all_arenas.as_ref()?;
        arenas.iter().enumerate().find_map(|(idx, arena)| {
            let file_name = arena.source_files.first()?.file_name.as_str();
            let relative = Path::new(file_name).strip_prefix(package_root).ok()?;
            let relative = Self::normalize_package_relative_path(&relative.to_string_lossy());
            let relative = Self::strip_resolution_extension(&relative);
            (relative == target || relative == format!("{target}/index")).then_some(idx)
        })
    }

    fn split_bare_package_specifier(specifier: &str) -> Option<(String, Option<String>)> {
        if specifier.starts_with('.') || specifier.starts_with('/') || specifier.starts_with('\\') {
            return None;
        }

        let mut parts = specifier.split('/');
        let first = parts.next()?;
        if first.starts_with('@') {
            let second = parts.next()?;
            let package_name = format!("{first}/{second}");
            let rest = parts.collect::<Vec<_>>().join("/");
            return Some((package_name, (!rest.is_empty()).then_some(rest)));
        }

        let rest = parts.collect::<Vec<_>>().join("/");
        Some((first.to_string(), (!rest.is_empty()).then_some(rest)))
    }

    fn node_modules_package_root_for_name(path: &str, package_name: &str) -> Option<PathBuf> {
        let components: Vec<_> = Path::new(path).components().collect();
        components
            .iter()
            .enumerate()
            .filter_map(|(idx, component)| {
                matches!(
                    component,
                    Component::Normal(part) if part.to_str() == Some("node_modules")
                )
                .then_some(idx)
            })
            .rev()
            .find_map(|nm_idx| {
                let pkg_start = nm_idx + 1;
                let pkg_len = if package_name.starts_with('@') { 2 } else { 1 };
                if components.len() < pkg_start + pkg_len {
                    return None;
                }
                let found_name = components[pkg_start..pkg_start + pkg_len]
                    .iter()
                    .filter_map(|component| match component {
                        Component::Normal(part) => part.to_str(),
                        _ => None,
                    })
                    .collect::<Vec<_>>()
                    .join("/");
                if found_name != package_name {
                    return None;
                }
                Some(components[..pkg_start + pkg_len].iter().fold(
                    PathBuf::new(),
                    |mut path, component| {
                        path.push(component.as_os_str());
                        path
                    },
                ))
            })
    }

    fn match_types_versions_pattern(pattern: &str, subpath: &str) -> Option<String> {
        if let Some((prefix, suffix)) = pattern.split_once('*') {
            if subpath.starts_with(prefix) && subpath.ends_with(suffix) {
                let end = subpath.len() - suffix.len();
                return Some(subpath[prefix.len()..end].to_string());
            }
            return None;
        }
        (pattern == subpath).then(String::new)
    }

    fn normalize_package_relative_path(path: &str) -> String {
        path.replace('\\', "/")
            .trim_start_matches("./")
            .trim_start_matches('/')
            .to_string()
    }

    fn strip_resolution_extension(path: &str) -> String {
        for ext in [
            ".d.ts", ".d.tsx", ".d.mts", ".d.cts", ".ts", ".tsx", ".mts", ".cts", ".js", ".jsx",
            ".mjs", ".cjs",
        ] {
            if let Some(stripped) = path.strip_suffix(ext) {
                return stripped.to_string();
            }
        }
        path.to_string()
    }
}

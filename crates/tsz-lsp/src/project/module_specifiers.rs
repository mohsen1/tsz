//! Module specifier resolution: computing the import path string for a target file.
//!
//! Given a source file and a target file, this module determines the best module specifier
//! to use in an import statement. It handles relative paths, path mappings (`paths` in
//! tsconfig.json), `rootDirs`, package.json `exports`/`imports`, and extension style
//! inference.

use std::cmp::Ordering;
use std::path::{Component, Path, PathBuf};

use rustc_hash::FxHashSet;

use tsz_parser::parser::node::NodeAccess;

use super::{ImportSpecifierPreference, Project};

use tsz_common::file_extensions::TSC_TS_RESOLUTION_EXTENSIONS_BARE as TS_EXTENSION_CANDIDATES;

mod helpers;

#[cfg(test)]
mod tests;

use helpers::*;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum RelativeImportStyle {
    Minimal,
    Ts,
    Js,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ExportsResolutionMode {
    Import,
    Require,
    Both,
}

/// Whether a file inside a `node_modules` package can be reached as an
/// auto-import from a given importer, honoring the target package's
/// `package.json` `exports` map.
///
/// This is the authoritative reachability verdict the module resolver owns.
/// Ancillary import-candidate collectors (e.g. the CLI code-fix fallbacks)
/// consult it instead of re-deriving a bare specifier straight from the file
/// path, which would ignore `exports` gating and offer files the package
/// deliberately hides.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum NodeModulesExportReachability {
    /// The importer enforces package `exports` and the target package's map
    /// does not expose this file. No specifier reaches it, so it must not be
    /// offered as an auto-import.
    Unreachable,
    /// The `exports` map exposes the target file; the wrapped value is the
    /// specifier a fresh import should use (already `exports`-remapped, so it
    /// may differ from the on-disk path — e.g. `pack/foo` for a file at
    /// `pack/dist/foo`).
    Reachable(String),
    /// The target package declares no `exports` map, or the importer's module
    /// resolution predates `exports`. Reachability is unconstrained and the
    /// caller may derive the specifier by its own rules.
    Unconstrained,
}

/// Relative-path-dependent candidate specifiers for one source → target pair.
struct RelativeCandidates {
    /// Direct relative specifier (e.g. `./utils` or `../shared/helpers`).
    relative: String,
    /// Specifier from `rootDirs` flattening, if applicable.
    root_dirs_relative: Option<String>,
    /// Specifiers derived from tsconfig `paths` mappings.
    path_mappings: Vec<String>,
    /// Specifiers derived from package.json `imports` (`#…` form).
    package_imports: Vec<String>,
}

/// All candidate specifiers for a source → target pair, grouped by origin.
///
/// Constructed by [`Project::collect_specifier_candidates`]; ranked into a
/// final ordered list by [`rank_specifier_candidates`].
struct SpecifierCandidateSet {
    /// Relative-path candidates. `None` when source and target share no
    /// resolvable file-system root (no common prefix after normalization).
    relative: Option<RelativeCandidates>,
    /// Workspace monorepo package specifier for targets in a sibling package.
    workspace_package: Option<String>,
    /// `node_modules` package specifier for targets inside a package tree.
    node_modules_package: Option<String>,
    /// When true the ranker strips any relative path that traverses into
    /// a `node_modules` directory (a deep-relative import would break on publish).
    target_in_node_modules: bool,
}

impl Project {
    pub(crate) fn resolve_module_specifier(
        &self,
        from_file: &str,
        module_specifier: &str,
    ) -> Option<String> {
        let candidates = self.module_specifier_candidates(from_file, module_specifier);
        candidates
            .into_iter()
            .find(|candidate| self.files.contains_key(candidate))
    }

    pub(crate) fn auto_import_module_specifiers_from_files(
        &self,
        from_file: &str,
        target_file: &str,
    ) -> Vec<String> {
        let candidates = self.collect_specifier_candidates(from_file, target_file);
        rank_specifier_candidates(candidates, self.import_module_specifier_preference)
    }

    /// Collect all candidate specifiers for a source → target pair, grouped
    /// by origin (relative, rootDirs, paths, package imports, workspace
    /// package, `node_modules` package). The returned set carries no ordering
    /// policy; pass it to [`rank_specifier_candidates`] to get the final list.
    fn collect_specifier_candidates(
        &self,
        from_file: &str,
        target_file: &str,
    ) -> SpecifierCandidateSet {
        let target_in_node_modules = target_file.replace('\\', "/").contains("/node_modules/");
        let supports_package_exports = self.module_resolution_supports_package_exports(from_file);
        let exports_mode = self.exports_resolution_mode_for_importer(from_file);
        let node_modules_package = self.package_specifier_from_node_modules_with_mode(
            target_file,
            supports_package_exports,
            exports_mode,
        );
        let workspace_package = self.workspace_package_dependency_specifier(
            from_file,
            target_file,
            target_in_node_modules,
            supports_package_exports,
            exports_mode,
        );
        let Some(relative) = self.relative_module_specifier_from_files(from_file, target_file)
        else {
            return SpecifierCandidateSet {
                relative: None,
                workspace_package,
                node_modules_package,
                target_in_node_modules,
            };
        };
        let root_dirs_relative =
            self.root_dirs_relative_specifier_from_files(from_file, target_file);
        let path_mappings = self.path_mapping_specifiers_from_files(from_file, target_file);
        let package_imports = self.package_import_specifiers_from_files(from_file, target_file);
        SpecifierCandidateSet {
            relative: Some(RelativeCandidates {
                relative,
                root_dirs_relative,
                path_mappings,
                package_imports,
            }),
            workspace_package,
            node_modules_package,
            target_in_node_modules,
        }
    }

    fn workspace_package_dependency_specifier(
        &self,
        from_file: &str,
        target_file: &str,
        target_in_node_modules: bool,
        supports_package_exports: bool,
        exports_mode: ExportsResolutionMode,
    ) -> Option<String> {
        if target_in_node_modules {
            return None;
        }

        let from_package = self.nearest_package_json(from_file);
        let normalized_target_file = normalize_path(Path::new(target_file))
            .to_string_lossy()
            .replace('\\', "/");

        let mut target_package_dir = None;
        let mut target_package_json = None;
        let mut dependency_specifier = None;

        if let Some((candidate_target_dir, candidate_target_json)) =
            self.nearest_package_json(target_file)
        {
            target_package_dir = Some(candidate_target_dir);
            target_package_json = Some(candidate_target_json);
        }

        if let (
            Some((from_package_dir, from_package_json)),
            Some(candidate_target_dir),
            Some(candidate_target_json),
        ) = (
            from_package.as_ref(),
            target_package_dir.as_ref(),
            target_package_json.as_ref(),
        ) && let Some(target_package_name) = candidate_target_json
            .get("name")
            .and_then(serde_json::Value::as_str)
            .map(str::trim)
            .filter(|name| !name.is_empty())
            && let Some(specifier) = Self::dependency_specifier_for_target_package(
                from_package_dir,
                from_package_json,
                candidate_target_dir,
                target_package_name,
            )
        {
            dependency_specifier = Some(specifier);
        }

        if dependency_specifier.is_none()
            && let Some((from_package_dir, from_package_json)) = from_package.as_ref()
            && let Some((specifier, resolved_target_dir)) =
                Self::dependency_specifier_for_target_path(
                    from_package_dir,
                    from_package_json,
                    &normalized_target_file,
                )
        {
            dependency_specifier = Some(specifier);
            target_package_dir = Some(resolved_target_dir);
        }

        if dependency_specifier.is_none()
            && let Some((_, from_package_json)) = from_package.as_ref()
            && let Some(candidate_target_dir) = target_package_dir.as_deref()
        {
            dependency_specifier = Self::dependency_specifier_for_target_dir_basename(
                from_package_json,
                candidate_target_dir,
            );
        }

        if dependency_specifier.is_none()
            && from_package.is_some()
            && let Some(candidate_target_json) = target_package_json.as_ref()
            && let Some(target_package_name) = candidate_target_json
                .get("name")
                .and_then(serde_json::Value::as_str)
                .map(str::trim)
                .filter(|name| !name.is_empty())
        {
            // Fourslash/virtual test hosts do not always include the requesting
            // file's package.json in the in-memory snapshot. When dependency
            // metadata is missing, use the target package name as a best-effort
            // package specifier fallback instead of collapsing to deep relatives.
            dependency_specifier = Some(target_package_name.to_string());
        }

        if dependency_specifier.is_none()
            && from_package.is_none()
            && self.prefers_project_relative_workspace_fallback_without_requesting_package()
            && let Some(candidate_target_dir) = target_package_dir.as_deref()
            && let Some(candidate_target_json) = target_package_json.as_ref()
            && Self::target_matches_package_root_specifier(
                target_file,
                candidate_target_dir,
                Some(candidate_target_json),
            )
            && let Some(target_package_name) = candidate_target_json
                .get("name")
                .and_then(serde_json::Value::as_str)
                .map(str::trim)
                .filter(|name| !name.is_empty())
        {
            dependency_specifier = Some(target_package_name.to_string());
        }

        if dependency_specifier.is_none()
            && from_package.is_some()
            && let Some((inferred_specifier, inferred_package_dir)) =
                Self::inferred_workspace_package_specifier_from_path(&normalized_target_file)
        {
            dependency_specifier = Some(inferred_specifier);
            target_package_dir = Some(inferred_package_dir);
        }

        if dependency_specifier.is_none()
            && from_package.is_none()
            && self.prefers_project_relative_workspace_fallback_without_requesting_package()
            && let Some((inferred_specifier, inferred_package_dir)) =
                Self::inferred_workspace_package_specifier_from_path(&normalized_target_file)
            && Self::target_matches_package_root_specifier(
                target_file,
                &inferred_package_dir,
                target_package_json.as_ref(),
            )
        {
            dependency_specifier = Some(inferred_specifier);
            target_package_dir = Some(inferred_package_dir);
        }

        let dependency_specifier = dependency_specifier?;
        let target_package_dir = target_package_dir?;

        let mut target_candidates = vec![
            normalize_path(Path::new(target_file))
                .to_string_lossy()
                .replace('\\', "/"),
        ];
        target_candidates.extend(self.project_output_target_alternatives(target_file));
        dedup_in_place(&mut target_candidates);

        if supports_package_exports
            && let Some(target_package_json) = target_package_json.as_ref()
            && let Some(exports_value) = target_package_json.get("exports")
        {
            for candidate in &target_candidates {
                if let Some(specifier) = self.package_specifier_from_package_exports_value(
                    candidate,
                    &dependency_specifier,
                    &target_package_dir,
                    exports_value,
                    exports_mode,
                ) {
                    return Some(specifier);
                }
            }
            return None;
        }

        for candidate in &target_candidates {
            let package_dir_prefix = format!("{target_package_dir}/");
            let target_relative = candidate
                .strip_prefix(&package_dir_prefix)
                .unwrap_or_default();
            let target_relative =
                path_to_string(&strip_js_ts_extension(Path::new(target_relative)))
                    .replace('\\', "/");
            let runtime_relative = package_runtime_specifier_from_target_path(&target_relative);
            let runtime_spec = if runtime_relative.is_empty() {
                dependency_specifier.clone()
            } else {
                format!("{dependency_specifier}/{runtime_relative}")
            };

            if let Some(target_package_json) = target_package_json.as_ref()
                && let Some(specifier) = package_main_module_specifier_for_target(
                    target_package_json,
                    &dependency_specifier,
                    &runtime_spec,
                    candidate,
                )
            {
                return Some(specifier);
            }

            let specifier = normalize_node_modules_package_specifier(&runtime_spec);
            if !specifier.is_empty() {
                return Some(specifier);
            }
        }

        None
    }

    fn nearest_package_json(&self, file: &str) -> Option<(String, serde_json::Value)> {
        let mut current = Path::new(file).parent();
        while let Some(dir) = current {
            let package_json_path = normalize_path(&dir.join("package.json"));
            let package_json_key = path_to_string(&package_json_path).replace('\\', "/");
            let package_json_text = self
                .files
                .get(&package_json_key)
                .map(|f| f.source_text().to_string())
                .or_else(|| std::fs::read_to_string(&package_json_key).ok());
            if let Some(package_json_text) = package_json_text
                && let Ok(package_json) =
                    serde_json::from_str::<serde_json::Value>(&package_json_text)
            {
                return Some((
                    path_to_string(&normalize_path(dir)).replace('\\', "/"),
                    package_json,
                ));
            }
            current = dir.parent();
        }
        None
    }

    fn dependency_specifier_for_target_package(
        from_package_dir: &str,
        from_package_json: &serde_json::Value,
        target_package_dir: &str,
        target_package_name: &str,
    ) -> Option<String> {
        const DEP_FIELDS: [&str; 4] = [
            "dependencies",
            "devDependencies",
            "peerDependencies",
            "optionalDependencies",
        ];

        for field in DEP_FIELDS {
            let Some(deps) = from_package_json
                .get(field)
                .and_then(serde_json::Value::as_object)
            else {
                continue;
            };

            if deps.contains_key(target_package_name) {
                return Some(target_package_name.to_string());
            }

            for (dep_name, dep_version) in deps {
                let Some(dep_version) = dep_version.as_str() else {
                    continue;
                };
                let Some(resolved_path) =
                    Self::resolve_dependency_path(from_package_dir, dep_version)
                else {
                    continue;
                };
                if resolved_path == target_package_dir {
                    return Some(dep_name.clone());
                }
            }
        }

        None
    }

    fn dependency_specifier_for_target_path(
        from_package_dir: &str,
        from_package_json: &serde_json::Value,
        normalized_target_file: &str,
    ) -> Option<(String, String)> {
        const DEP_FIELDS: [&str; 4] = [
            "dependencies",
            "devDependencies",
            "peerDependencies",
            "optionalDependencies",
        ];

        let mut best: Option<(String, String)> = None;

        for field in DEP_FIELDS {
            let Some(deps) = from_package_json
                .get(field)
                .and_then(serde_json::Value::as_object)
            else {
                continue;
            };

            for (dep_name, dep_version) in deps {
                let Some(dep_version) = dep_version.as_str() else {
                    continue;
                };
                let Some(resolved_path) =
                    Self::resolve_dependency_path(from_package_dir, dep_version)
                else {
                    continue;
                };

                let is_match = normalized_target_file == resolved_path
                    || normalized_target_file
                        .strip_prefix(&resolved_path)
                        .is_some_and(|rest| rest.starts_with('/'));
                if !is_match {
                    continue;
                }

                let should_replace = best
                    .as_ref()
                    .is_none_or(|(_, best_path)| resolved_path.len() > best_path.len());
                if should_replace {
                    best = Some((dep_name.clone(), resolved_path));
                }
            }
        }

        best
    }

    fn dependency_specifier_for_target_dir_basename(
        from_package_json: &serde_json::Value,
        target_package_dir: &str,
    ) -> Option<String> {
        const DEP_FIELDS: [&str; 4] = [
            "dependencies",
            "devDependencies",
            "peerDependencies",
            "optionalDependencies",
        ];

        let target_basename = Path::new(target_package_dir)
            .file_name()
            .and_then(|segment| segment.to_str())
            .map(str::trim)
            .filter(|segment| !segment.is_empty())?;

        let mut scoped_match: Option<String> = None;

        for field in DEP_FIELDS {
            let Some(deps) = from_package_json
                .get(field)
                .and_then(serde_json::Value::as_object)
            else {
                continue;
            };

            for dep_name in deps.keys() {
                if dep_name == target_basename {
                    return Some(dep_name.clone());
                }

                if dep_name
                    .rsplit('/')
                    .next()
                    .is_some_and(|tail| tail == target_basename)
                {
                    let should_replace = scoped_match
                        .as_ref()
                        .is_none_or(|current| dep_name.len() < current.len());
                    if should_replace {
                        scoped_match = Some(dep_name.clone());
                    }
                }
            }
        }

        scoped_match
    }

    fn inferred_workspace_package_specifier_from_path(
        normalized_target_file: &str,
    ) -> Option<(String, String)> {
        let marker = "/packages/";
        let marker_idx = normalized_target_file.find(marker)?;
        let package_root_start = marker_idx + marker.len();
        let tail = normalized_target_file.get(package_root_start..)?;
        if tail.is_empty() {
            return None;
        }

        let mut segments = tail.split('/').filter(|segment| !segment.is_empty());
        let first = segments.next()?;

        let (package_specifier, package_root_rel) = if first.starts_with('@') {
            let second = segments.next()?;
            (format!("{first}/{second}"), format!("{first}/{second}"))
        } else {
            (first.to_string(), first.to_string())
        };

        let package_root = format!(
            "{}{}{}",
            &normalized_target_file[..package_root_start],
            package_root_rel,
            ""
        );
        Some((package_specifier, package_root))
    }

    fn prefers_project_relative_workspace_fallback_without_requesting_package(&self) -> bool {
        self.import_module_specifier_preference == Some(ImportSpecifierPreference::ProjectRelative)
    }

    fn target_matches_package_root_specifier(
        target_file: &str,
        target_package_dir: &str,
        target_package_json: Option<&serde_json::Value>,
    ) -> bool {
        let normalized_target_file = normalize_path(Path::new(target_file))
            .to_string_lossy()
            .replace('\\', "/");
        let package_dir_prefix = format!("{target_package_dir}/");
        let target_relative = normalized_target_file
            .strip_prefix(&package_dir_prefix)
            .unwrap_or_default();
        let target_relative =
            path_to_string(&strip_js_ts_extension(Path::new(target_relative))).replace('\\', "/");
        if target_relative.is_empty() {
            return true;
        }

        if let Some(target_package_json) = target_package_json {
            let package_root = "__pkg__";
            let runtime_relative = package_runtime_specifier_from_target_path(&target_relative);
            let runtime_spec = if runtime_relative.is_empty() {
                package_root.to_string()
            } else {
                format!("{package_root}/{runtime_relative}")
            };
            return package_main_module_specifier_for_target(
                target_package_json,
                package_root,
                &runtime_spec,
                &normalized_target_file,
            )
            .as_deref()
                == Some(package_root);
        }

        normalize_package_entry_for_match(&target_relative) == "index"
    }

    fn resolve_dependency_path(from_package_dir: &str, specifier: &str) -> Option<String> {
        let path = if let Some(rest) = specifier.strip_prefix("file:") {
            rest
        } else if let Some(rest) = specifier.strip_prefix("link:") {
            rest
        } else {
            let rest = specifier.strip_prefix("workspace:")?;
            if !(rest.starts_with('.') || rest.starts_with('/')) {
                return None;
            }
            rest
        };

        let path = path.trim();
        if path.is_empty() {
            return None;
        }

        let resolved = if Path::new(path).is_absolute() {
            normalize_path(Path::new(path))
        } else {
            normalize_path(&Path::new(from_package_dir).join(path))
        };
        Some(path_to_string(&resolved).replace('\\', "/"))
    }

    fn path_mapping_specifiers_from_files(
        &self,
        from_file: &str,
        target_file: &str,
    ) -> Vec<String> {
        let Some((config_dir, compiler_options)) =
            self.nearest_compiler_options_for_file(from_file)
        else {
            return Vec::new();
        };

        let Some(paths) = compiler_options
            .get("paths")
            .and_then(serde_json::Value::as_object)
        else {
            return Vec::new();
        };

        let base_dir = base_dir_for_compiler_options(&config_dir, &compiler_options);
        let normalized_target_file = path_to_string(&strip_js_ts_extension(&normalize_path(
            Path::new(target_file),
        )))
        .replace('\\', "/");
        let mut target_candidates = vec![normalized_target_file];
        target_candidates.extend(self.project_output_target_alternatives(target_file));
        dedup_in_place(&mut target_candidates);

        let mut specifiers = Vec::new();
        for (alias_pattern, mapped_targets) in paths {
            let Some(mapped_targets) = mapped_targets.as_array() else {
                continue;
            };
            for mapped_target in mapped_targets {
                let Some(mapped_target) = mapped_target.as_str() else {
                    continue;
                };
                let resolved_mapped_target =
                    resolve_path_mapping_target(mapped_target, &base_dir, &config_dir);

                let Some(capture) = target_candidates.iter().find_map(|candidate| {
                    wildcard_capture_case_insensitive(&resolved_mapped_target, candidate)
                }) else {
                    continue;
                };
                let Some(specifier) = apply_wildcard_capture(alias_pattern, &capture) else {
                    continue;
                };
                specifiers.push(normalize_path_mapping_specifier(&specifier));
            }
        }

        dedup_in_place(&mut specifiers);
        specifiers
    }

    /// Compute the new module specifier for a path-aliased import when a file is
    /// renamed.
    ///
    /// Returns `Some(new_specifier)` if `current_specifier` is a `paths`-alias
    /// import in the importer's nearest tsconfig that resolves to `old_target`
    /// and the same alias pattern can be applied to `new_target`.
    ///
    /// Returns `None` if the specifier is not path-mapped to `old_target`, or if
    /// the alias pattern cannot accommodate `new_target` (e.g. the file moved
    /// outside the alias root, or the alias has no wildcard and the target is
    /// pinned). Callers should fall back to other rewrite strategies (e.g. a
    /// relative specifier) in those cases.
    pub(crate) fn rename_path_alias_specifier(
        &self,
        from_file: &str,
        current_specifier: &str,
        old_target: &str,
        new_target: &str,
    ) -> Option<String> {
        let (config_dir, compiler_options) = self.nearest_compiler_options_for_file(from_file)?;
        let paths = compiler_options
            .get("paths")
            .and_then(serde_json::Value::as_object)?;

        let base_dir = base_dir_for_compiler_options(&config_dir, &compiler_options);
        let old_normalized = path_to_string(&strip_js_ts_extension(&normalize_path(Path::new(
            old_target,
        ))))
        .replace('\\', "/");
        let new_normalized = path_to_string(&strip_js_ts_extension(&normalize_path(Path::new(
            new_target,
        ))))
        .replace('\\', "/");

        for (alias_pattern, mapped_targets) in paths {
            let Some(mapped_targets) = mapped_targets.as_array() else {
                continue;
            };
            // Capture the wildcard portion of `current_specifier` under this
            // alias. If the alias has no wildcard, an exact match yields `""`.
            let Some(current_capture) =
                wildcard_capture_case_insensitive(alias_pattern, current_specifier)
            else {
                continue;
            };

            let resolved_targets: Vec<String> = mapped_targets
                .iter()
                .filter_map(serde_json::Value::as_str)
                .map(|t| resolve_path_mapping_target(t, &base_dir, &config_dir))
                .collect();

            // Confirm the alias actually resolves `current_specifier` to
            // `old_target`. Without this an unrelated alias that happens to
            // share a prefix could incorrectly claim the import.
            let points_at_old_target = resolved_targets.iter().any(|resolved| {
                apply_wildcard_capture(resolved, &current_capture)
                    .is_some_and(|substituted| substituted.eq_ignore_ascii_case(&old_normalized))
            });
            if !points_at_old_target {
                continue;
            }

            // Find a `mapped_target` under the same alias that can host
            // `new_target`, preserving the alias pattern the user chose. If
            // none can, the alias is no longer valid for the new path; return
            // `None` so the caller can fall back to a relative rewrite.
            return resolved_targets.iter().find_map(|resolved| {
                let new_capture = wildcard_capture_case_insensitive(resolved, &new_normalized)?;
                apply_wildcard_capture(alias_pattern, &new_capture)
                    .map(|s| normalize_path_mapping_specifier(&s))
            });
        }

        None
    }

    fn root_dirs_relative_specifier_from_files(
        &self,
        from_file: &str,
        target_file: &str,
    ) -> Option<String> {
        let (config_dir, compiler_options) = self.nearest_compiler_options_for_file(from_file)?;
        let root_dirs = compiler_options
            .get("rootDirs")
            .and_then(serde_json::Value::as_array)?;
        if root_dirs.is_empty() {
            return None;
        }

        let roots: Vec<PathBuf> = root_dirs
            .iter()
            .filter_map(serde_json::Value::as_str)
            .map(|root| normalize_path(&config_dir.join(root)))
            .collect();
        if roots.is_empty() {
            return None;
        }

        let from_path = strip_ts_path_extension(&normalize_path(Path::new(from_file)));
        let target_path = strip_ts_path_extension(&normalize_path(Path::new(target_file)));
        let style = self.relative_import_style(from_file);
        let mut best_spec: Option<String> = None;

        for from_root in &roots {
            let Ok(from_rel) = from_path.strip_prefix(from_root) else {
                continue;
            };
            let from_rel_dir = from_rel.parent().unwrap_or_else(|| Path::new(""));
            for target_root in &roots {
                let Ok(target_rel) = target_path.strip_prefix(target_root) else {
                    continue;
                };

                let relative = relative_path(from_rel_dir, target_rel);
                let mut spec = path_to_string(&relative).replace('\\', "/");
                if spec.is_empty() {
                    continue;
                }
                if !spec.starts_with('.') {
                    spec = format!("./{spec}");
                }

                // Preserve existing extension style behavior for relative imports.
                match style {
                    RelativeImportStyle::Minimal => {}
                    RelativeImportStyle::Ts => {
                        if let Some(ext) = ts_source_extension(target_file) {
                            spec.push_str(ext);
                        }
                    }
                    RelativeImportStyle::Js => spec.push_str(".js"),
                }

                if let Some(current_best) = best_spec.as_ref() {
                    if compare_module_specifier_candidates(&spec, current_best) == Ordering::Less {
                        best_spec = Some(spec);
                    }
                } else {
                    best_spec = Some(spec);
                }
            }
        }

        best_spec
    }

    pub(crate) fn nearest_compiler_options_for_file(
        &self,
        from_file: &str,
    ) -> Option<(PathBuf, serde_json::Map<String, serde_json::Value>)> {
        let mut current = Path::new(from_file).parent();
        while let Some(dir) = current {
            for config_name in ["tsconfig.json", "jsconfig.json"] {
                let config_path = normalize_path(&dir.join(config_name));
                let config_key = path_to_string(&config_path).replace('\\', "/");
                let Some((compiler_options, _)) =
                    self.resolve_tsconfig_compiler_options(&config_key, &mut FxHashSet::default())
                else {
                    continue;
                };
                return Some((normalize_path(dir), compiler_options));
            }
            current = dir.parent();
        }
        None
    }

    /// Resolve a tsconfig/jsconfig file, following any `extends` chain.
    /// Returns the merged compilerOptions plus the effective config dir.
    fn resolve_tsconfig_compiler_options(
        &self,
        config_key: &str,
        visited: &mut FxHashSet<String>,
    ) -> Option<(serde_json::Map<String, serde_json::Value>, PathBuf)> {
        if !visited.insert(config_key.to_string()) {
            return None;
        }
        let config_text = self
            .files
            .get(config_key)
            .map(|f| f.source_text().to_string())
            .or_else(|| std::fs::read_to_string(config_key).ok())?;
        let config_json = parse_typescript_config_json(&config_text)?;
        let config_dir = Path::new(config_key)
            .parent()
            .map(normalize_path)
            .unwrap_or_else(|| PathBuf::from(""));
        let mut merged: serde_json::Map<String, serde_json::Value> = serde_json::Map::new();
        // Handle `extends` first: single path or array of paths.
        if let Some(extends_value) = config_json.get("extends") {
            let extend_entries: Vec<&str> = if let Some(text) = extends_value.as_str() {
                vec![text]
            } else if let Some(arr) = extends_value.as_array() {
                arr.iter().filter_map(serde_json::Value::as_str).collect()
            } else {
                Vec::new()
            };
            for entry in extend_entries {
                let candidate = if entry.starts_with('.') || entry.starts_with('/') {
                    let joined = normalize_path(&config_dir.join(entry));
                    let joined_str = path_to_string(&joined).replace('\\', "/");
                    let candidates = if joined_str.ends_with(".json") {
                        vec![joined_str.clone()]
                    } else {
                        vec![
                            format!("{joined_str}.json"),
                            format!("{joined_str}/tsconfig.json"),
                        ]
                    };
                    candidates.into_iter().find(|path| {
                        self.files.contains_key(path) || std::fs::metadata(path).is_ok()
                    })
                } else {
                    None
                };
                if let Some(base_path) = candidate
                    && let Some((base_options, base_dir)) =
                        self.resolve_tsconfig_compiler_options(&base_path, visited)
                {
                    let rebased = Self::rebase_path_options(base_options, &base_dir, &config_dir);
                    for (key, value) in rebased {
                        merged.insert(key, value);
                    }
                }
            }
        }
        if let Some(own_options) = config_json
            .get("compilerOptions")
            .and_then(serde_json::Value::as_object)
        {
            for (key, value) in own_options {
                merged.insert(key.clone(), value.clone());
            }
        }
        if merged.is_empty() {
            return None;
        }
        Some((merged, config_dir))
    }

    /// Rewrite path-valued options inherited from a base tsconfig so they
    /// are expressed relative to the extending tsconfig's directory.
    /// Matches tsc's rule that "any relative paths in extended tsconfig
    /// files are resolved relative to the containing file" — i.e., the
    /// base's paths should point to files relative to the base, not the
    /// extending tsconfig.
    fn rebase_path_options(
        mut options: serde_json::Map<String, serde_json::Value>,
        base_dir: &Path,
        _config_dir: &Path,
    ) -> serde_json::Map<String, serde_json::Value> {
        let rebase_text = |text: &str| -> Option<String> {
            if text.starts_with("${configDir}") || text.starts_with('/') {
                return None;
            }
            // Resolve the base-relative path to an absolute form. Using an
            // absolute string here side-steps the brittle relativization of
            // paths that traverse above the extending tsconfig's dir, which
            // can mis-normalize (e.g. `/../x` → `x`).
            let abs = normalize_path(&base_dir.join(text));
            let mut s = path_to_string(&abs).replace('\\', "/");
            if !s.starts_with('/') {
                s = format!("/{s}");
            }
            Some(s)
        };
        if let Some(base_url_val) = options.get("baseUrl").cloned()
            && let Some(base_url) = base_url_val.as_str()
            && let Some(rebased) = rebase_text(base_url)
        {
            options.insert("baseUrl".to_string(), serde_json::json!(rebased));
        }
        if let Some(paths_val) = options.get("paths").cloned()
            && let Some(paths_obj) = paths_val.as_object()
        {
            let mut new_paths = serde_json::Map::new();
            for (alias, targets) in paths_obj {
                if let Some(targets_arr) = targets.as_array() {
                    let mut new_targets = Vec::new();
                    for t in targets_arr {
                        if let Some(text) = t.as_str() {
                            match rebase_text(text) {
                                Some(rebased) => new_targets.push(serde_json::json!(rebased)),
                                None => new_targets.push(serde_json::json!(text)),
                            }
                        }
                    }
                    new_paths.insert(alias.clone(), serde_json::json!(new_targets));
                }
            }
            options.insert("paths".to_string(), serde_json::json!(new_paths));
        }
        options
    }

    fn module_resolution_supports_package_exports(&self, from_file: &str) -> bool {
        let Some((_, compiler_options)) = self.nearest_compiler_options_for_file(from_file) else {
            return true;
        };

        if let Some(module_resolution) = compiler_options
            .get("moduleResolution")
            .and_then(serde_json::Value::as_str)
        {
            return module_resolution.eq_ignore_ascii_case("node16")
                || module_resolution.eq_ignore_ascii_case("nodenext")
                || module_resolution.eq_ignore_ascii_case("bundler");
        }

        compiler_options
            .get("module")
            .and_then(serde_json::Value::as_str)
            .is_some_and(|module| {
                module.eq_ignore_ascii_case("node16") || module.eq_ignore_ascii_case("nodenext")
            })
    }

    /// Whether `from_file` resolves modules under `node16`/`nodenext` — the
    /// two resolution kinds where Node's own loader is on the other end, as
    /// opposed to `bundler` (a bundler resolves extension-less specifiers
    /// itself, so tsc's specifier preference does not force an extension
    /// there).
    fn module_resolution_is_node16_or_nodenext(&self, from_file: &str) -> bool {
        let Some((_, compiler_options)) = self.nearest_compiler_options_for_file(from_file) else {
            return false;
        };

        if let Some(module_resolution) = compiler_options
            .get("moduleResolution")
            .and_then(serde_json::Value::as_str)
        {
            return module_resolution.eq_ignore_ascii_case("node16")
                || module_resolution.eq_ignore_ascii_case("nodenext");
        }

        compiler_options
            .get("module")
            .and_then(serde_json::Value::as_str)
            .is_some_and(|module| {
                module.eq_ignore_ascii_case("node16") || module.eq_ignore_ascii_case("nodenext")
            })
    }

    fn exports_resolution_mode_for_importer(&self, from_file: &str) -> ExportsResolutionMode {
        if from_file.ends_with(".cts") || from_file.ends_with(".cjs") {
            return ExportsResolutionMode::Require;
        }
        if from_file.ends_with(".mts") || from_file.ends_with(".mjs") {
            return ExportsResolutionMode::Import;
        }
        // For ambiguous .ts/.tsx/.js/.jsx files, fall back to the nearest
        // package.json `type` field (Node's rules for resolving the dual
        // conditions). `"type": "module"` implies ESM import resolution;
        // anything else defaults to require.
        if let Some((_, package_json)) = self.nearest_package_json(from_file)
            && let Some(pkg_type) = package_json.get("type").and_then(serde_json::Value::as_str)
        {
            return if pkg_type.eq_ignore_ascii_case("module") {
                ExportsResolutionMode::Import
            } else {
                ExportsResolutionMode::Require
            };
        }

        ExportsResolutionMode::Both
    }

    pub(crate) fn auto_imports_allowed_for_file(&self, from_file: &str) -> bool {
        let Some((_, compiler_options)) = self.nearest_compiler_options_for_file(from_file) else {
            return self.auto_imports_allowed_without_tsconfig;
        };

        let module_none = compiler_options
            .get("module")
            .and_then(serde_json::Value::as_str)
            .is_some_and(|module| module.eq_ignore_ascii_case("none"));
        if !module_none {
            return true;
        }

        compiler_options
            .get("target")
            .and_then(serde_json::Value::as_str)
            .is_some_and(target_supports_import_syntax)
    }

    fn relative_module_specifier_from_files(
        &self,
        from_file: &str,
        target_file: &str,
    ) -> Option<String> {
        let style = self.relative_import_style(from_file);
        let from_dir = Path::new(from_file)
            .parent()
            .unwrap_or_else(|| Path::new(""));
        let target_path = strip_ts_path_extension(Path::new(target_file));
        let relative = relative_path(from_dir, &target_path);

        let mut spec = path_to_string(&relative).replace('\\', "/");
        if spec.is_empty() {
            return None;
        }
        if !spec.starts_with('.') {
            spec = format!("./{spec}");
        }

        match style {
            RelativeImportStyle::Minimal => {}
            RelativeImportStyle::Ts => {
                if let Some(ext) = ts_source_extension(target_file) {
                    spec.push_str(ext);
                }
            }
            RelativeImportStyle::Js => {
                spec.push_str(".js");
            }
        }

        Some(spec)
    }

    fn package_import_specifiers_from_files(
        &self,
        from_file: &str,
        target_file: &str,
    ) -> Vec<String> {
        let additional_targets = self.package_import_target_alternatives(from_file, target_file);
        let mut current = Path::new(from_file).parent();
        while let Some(dir) = current {
            let package_json_path = normalize_path(&dir.join("package.json"));
            let package_json_key = path_to_string(&package_json_path).replace('\\', "/");
            let Some(package_json_text) = self
                .files
                .get(&package_json_key)
                .map(|f| f.source_text().to_string())
                .or_else(|| std::fs::read_to_string(&package_json_key).ok())
            else {
                current = dir.parent();
                continue;
            };

            let package_dir = path_to_string(dir).replace('\\', "/");
            return package_import_specifiers_for_target(
                &package_json_text,
                &package_dir,
                target_file,
                self.allow_importing_ts_extensions,
                &additional_targets,
            );
        }

        Vec::new()
    }

    fn package_import_target_alternatives(
        &self,
        from_file: &str,
        target_file: &str,
    ) -> Vec<String> {
        let mut current = Path::new(from_file).parent();
        while let Some(dir) = current {
            let tsconfig_path = normalize_path(&dir.join("tsconfig.json"));
            let tsconfig_key = path_to_string(&tsconfig_path).replace('\\', "/");
            let Some(tsconfig_text) = self
                .files
                .get(&tsconfig_key)
                .map(|f| f.source_text().to_string())
                .or_else(|| std::fs::read_to_string(&tsconfig_key).ok())
            else {
                current = dir.parent();
                continue;
            };

            let Some(tsconfig) = parse_typescript_config_json(&tsconfig_text) else {
                return Vec::new();
            };
            let Some(compiler_options) = tsconfig
                .get("compilerOptions")
                .and_then(serde_json::Value::as_object)
            else {
                return Vec::new();
            };

            let root_dir = compiler_options
                .get("rootDir")
                .and_then(serde_json::Value::as_str);
            let out_dir = compiler_options
                .get("outDir")
                .and_then(serde_json::Value::as_str);
            let declaration_dir = compiler_options
                .get("declarationDir")
                .and_then(serde_json::Value::as_str);

            let Some(root_dir) = root_dir else {
                return Vec::new();
            };

            let config_dir = normalize_path(dir);
            let root_dir = normalize_path(&config_dir.join(root_dir));
            let target_path = strip_js_ts_extension(&normalize_path(Path::new(target_file)));
            let Ok(relative) = target_path.strip_prefix(&root_dir) else {
                return Vec::new();
            };

            let mut alternatives = Vec::new();
            if let Some(out_dir) = out_dir {
                let out_dir = normalize_path(&config_dir.join(out_dir));
                alternatives.push(path_to_string(&out_dir.join(relative)).replace('\\', "/"));
            }
            if let Some(declaration_dir) = declaration_dir {
                let declaration_dir = normalize_path(&config_dir.join(declaration_dir));
                alternatives
                    .push(path_to_string(&declaration_dir.join(relative)).replace('\\', "/"));
            }

            return alternatives;
        }

        Vec::new()
    }

    fn project_output_target_alternatives(&self, target_file: &str) -> Vec<String> {
        let Some((config_dir, compiler_options)) =
            self.nearest_compiler_options_for_file(target_file)
        else {
            return Vec::new();
        };

        let out_dir = compiler_options
            .get("outDir")
            .and_then(serde_json::Value::as_str);
        let declaration_dir = compiler_options
            .get("declarationDir")
            .and_then(serde_json::Value::as_str);
        if out_dir.is_none() && declaration_dir.is_none() {
            return Vec::new();
        }

        let root_dir = compiler_options
            .get("rootDir")
            .and_then(serde_json::Value::as_str)
            .map(|root| normalize_path(&config_dir.join(root)))
            .or_else(|| {
                compiler_options
                    .get("composite")
                    .and_then(serde_json::Value::as_bool)
                    .filter(|enabled| *enabled)
                    .map(|_| normalize_path(&config_dir))
            });
        let Some(root_dir) = root_dir else {
            return Vec::new();
        };

        let target_path = strip_js_ts_extension(&normalize_path(Path::new(target_file)));
        let Ok(relative) = target_path.strip_prefix(&root_dir) else {
            return Vec::new();
        };

        let mut alternatives = Vec::new();
        if let Some(out_dir) = out_dir {
            let out_dir = normalize_path(&config_dir.join(out_dir));
            alternatives.push(path_to_string(&out_dir.join(relative)).replace('\\', "/"));
        }
        if let Some(declaration_dir) = declaration_dir {
            let declaration_dir = normalize_path(&config_dir.join(declaration_dir));
            alternatives.push(path_to_string(&declaration_dir.join(relative)).replace('\\', "/"));
        }

        alternatives
    }

    fn relative_import_style(&self, from_file: &str) -> RelativeImportStyle {
        if self.import_module_specifier_ending.as_deref() == Some("js") {
            return RelativeImportStyle::Ts;
        }

        if from_file.ends_with(".mts") {
            return RelativeImportStyle::Minimal;
        }

        // tsc's `getAllowedEndingsInPreferredOrder` (moduleSpecifiers.ts) hard-forces
        // an extension-bearing ending whenever the importing file's implied module
        // format is ESM under `node16`/`nodenext` resolution — Node's own ESM loader
        // requires an explicit extension, so this does not depend on whatever ending
        // style the file's existing imports (if any) happen to use.
        if self.module_resolution_is_node16_or_nodenext(from_file)
            && self.exports_resolution_mode_for_importer(from_file) == ExportsResolutionMode::Import
        {
            return if self.allow_importing_ts_extensions {
                RelativeImportStyle::Ts
            } else {
                RelativeImportStyle::Js
            };
        }

        let Some(file) = self.files.get(from_file) else {
            return RelativeImportStyle::Minimal;
        };
        let arena = file.arena();
        let Some(source_file) = arena.get_source_file_at(file.root()) else {
            return RelativeImportStyle::Minimal;
        };

        let mut saw_ts = false;
        let mut saw_js = false;

        for &stmt_idx in &source_file.statements.nodes {
            let Some(stmt_node) = arena.get(stmt_idx) else {
                continue;
            };
            if stmt_node.kind != tsz_parser::syntax_kind_ext::IMPORT_DECLARATION {
                continue;
            }
            let Some(import_decl) = arena.get_import_decl(stmt_node) else {
                continue;
            };
            let Some(module_text) = arena.get_literal_text(import_decl.module_specifier) else {
                continue;
            };
            if !module_text.starts_with('.') {
                continue;
            }

            if has_ts_extension(module_text) {
                saw_ts = true;
            } else if has_js_extension(module_text) {
                saw_js = true;
            }
        }

        if saw_js {
            RelativeImportStyle::Js
        } else if saw_ts {
            RelativeImportStyle::Ts
        } else {
            RelativeImportStyle::Minimal
        }
    }

    pub(crate) fn module_specifier_candidates(
        &self,
        from_file: &str,
        module_specifier: &str,
    ) -> Vec<String> {
        let mut candidates = Vec::new();

        if module_specifier.starts_with('.') {
            let base_dir = Path::new(from_file)
                .parent()
                .unwrap_or_else(|| Path::new(""));
            let joined = normalize_path(&base_dir.join(module_specifier));

            if joined.extension().is_some() {
                candidates.push(path_to_string(&joined));
            } else {
                for ext in TS_EXTENSION_CANDIDATES {
                    candidates.push(path_to_string(&joined.with_extension(ext)));
                }
                for ext in TS_EXTENSION_CANDIDATES {
                    candidates.push(path_to_string(&joined.join("index").with_extension(ext)));
                }
            }
        } else {
            candidates.push(module_specifier.to_string());
            if Path::new(module_specifier).extension().is_none() {
                for ext in TS_EXTENSION_CANDIDATES {
                    candidates.push(format!("{module_specifier}.{ext}"));
                }
            }
        }

        candidates
    }

    #[cfg(test)]
    fn package_specifier_from_node_modules(&self, target_file: &str) -> Option<String> {
        self.package_specifier_from_node_modules_with_mode(
            target_file,
            true,
            ExportsResolutionMode::Both,
        )
    }

    /// Classify whether `target_file` (a file inside a `node_modules` package)
    /// can be auto-imported from `from_file` under the target package's
    /// `exports` map.
    ///
    /// Returns [`NodeModulesExportReachability::Unreachable`] only when the
    /// importer's module resolution enforces package `exports` *and* the
    /// target package declares an `exports` map that does not expose the file.
    /// Packages without an `exports` map — or importers whose module resolution
    /// predates `exports` (classic/node10) — are
    /// [`NodeModulesExportReachability::Unconstrained`], leaving specifier
    /// choice to the caller. When the map does expose the file, the
    /// [`NodeModulesExportReachability::Reachable`] specifier already reflects
    /// any `exports` remapping.
    pub fn node_modules_export_reachability(
        &self,
        from_file: &str,
        target_file: &str,
    ) -> NodeModulesExportReachability {
        let normalized = target_file.replace('\\', "/");
        if !normalized.contains("/node_modules/") {
            return NodeModulesExportReachability::Unconstrained;
        }
        if !self.module_resolution_supports_package_exports(from_file) {
            return NodeModulesExportReachability::Unconstrained;
        }
        // Only an `exports` map constrains reachability; without one, Node
        // resolves any subpath and the caller keeps its own specifier heuristic.
        let has_exports_map = self
            .nearest_package_json(&normalized)
            .is_some_and(|(_, json)| json.get("exports").is_some());
        if !has_exports_map {
            return NodeModulesExportReachability::Unconstrained;
        }
        let exports_mode = self.exports_resolution_mode_for_importer(from_file);
        match self.package_specifier_from_node_modules_with_mode(&normalized, true, exports_mode) {
            Some(specifier) => NodeModulesExportReachability::Reachable(specifier),
            None => NodeModulesExportReachability::Unreachable,
        }
    }

    fn package_specifier_from_node_modules_with_mode(
        &self,
        target_file: &str,
        supports_package_exports: bool,
        exports_mode: ExportsResolutionMode,
    ) -> Option<String> {
        let original = target_file.replace('\\', "/");
        // Pnpm's virtual store places the real package under
        // `node_modules/.pnpm/<pkg>@<ver>/node_modules/<actual>`. Rewrite
        // to the outer-layout equivalent (`node_modules/<actual>/...`) for
        // specifier computation, but remember the pnpm-real path so we can
        // still find `package.json` at the original location below.
        let pnpm_inner_marker = "/node_modules/.pnpm/";
        let (normalized, pnpm_real_prefix) = if let Some(pnpm_start) =
            original.find(pnpm_inner_marker)
        {
            let after = &original[pnpm_start + pnpm_inner_marker.len()..];
            if let Some(inner) = after.find("/node_modules/") {
                let shifted = pnpm_start + pnpm_inner_marker.len() + inner + "/node_modules/".len();
                let real_prefix = original[..shifted].to_string();
                let rewritten = format!(
                    "{}/node_modules/{}",
                    &original[..pnpm_start],
                    &original[shifted..]
                );
                (rewritten, Some(real_prefix))
            } else {
                (original, None)
            }
        } else {
            (original, None)
        };
        let marker = "/node_modules/";
        let marker_idx = normalized.find(marker)?;
        let node_modules_root = &normalized[..marker_idx + marker.len() - 1];
        if let Some(specifier) = self.package_specifier_from_nearest_package_manifest(
            &normalized,
            node_modules_root,
            supports_package_exports,
            exports_mode,
        ) {
            return Some(specifier);
        }

        let package_path = &normalized[marker_idx + marker.len()..];
        if package_path.is_empty() {
            return None;
        }

        let (package_root, _package_suffix) = split_node_modules_package_path(package_path)?;
        let package_root = normalize_node_modules_package_specifier(&package_root);
        let package_prefix = &normalized[..marker_idx + marker.len()];
        let package_json_path = format!("{package_prefix}{package_root}/package.json");
        let package_json = self
            .files
            .get(&package_json_path)
            .map(|f| f.source_text().to_string())
            .or_else(|| std::fs::read_to_string(&package_json_path).ok())
            .or_else(|| {
                // Pnpm-virtual packages keep their package.json at the
                // original `/node_modules/.pnpm/<pkg>/node_modules/<actual>`
                // path even after we rewrite the target into outer-layout
                // form. Fall back to that location so `main`/`types`
                // collapsing still works for pnpm packages.
                let real_prefix = pnpm_real_prefix.as_deref()?;
                let pnpm_package_json_path = format!("{real_prefix}{package_root}/package.json");
                self.files
                    .get(&pnpm_package_json_path)
                    .map(|f| f.source_text().to_string())
                    .or_else(|| std::fs::read_to_string(&pnpm_package_json_path).ok())
            })
            .and_then(|text| serde_json::from_str::<serde_json::Value>(&text).ok());

        if supports_package_exports
            && package_json
                .as_ref()
                .and_then(|json| json.get("exports"))
                .is_some()
        {
            return self.package_specifier_from_package_exports(
                &normalized,
                &package_root,
                package_prefix,
                &package_json_path,
                exports_mode,
            );
        }

        let runtime_spec = package_runtime_specifier_from_target_path(package_path);
        if let Some(package_json) = package_json.as_ref()
            && let Some(specifier) = package_main_module_specifier_for_target(
                package_json,
                &package_root,
                &runtime_spec,
                target_file,
            )
        {
            return Some(specifier);
        }

        let spec = normalize_node_modules_package_specifier(&runtime_spec);
        if spec.is_empty() { None } else { Some(spec) }
    }

    fn package_specifier_from_nearest_package_manifest(
        &self,
        normalized_target: &str,
        node_modules_root: &str,
        supports_package_exports: bool,
        exports_mode: ExportsResolutionMode,
    ) -> Option<String> {
        let mut current_dir = Path::new(normalized_target).parent();
        while let Some(dir) = current_dir {
            let dir_normalized = path_to_string(&normalize_path(dir)).replace('\\', "/");
            if !dir_normalized.starts_with(node_modules_root) {
                break;
            }

            let package_json_path = format!("{dir_normalized}/package.json");
            let package_json = self
                .files
                .get(&package_json_path)
                .map(|f| f.source_text().to_string())
                .or_else(|| std::fs::read_to_string(&package_json_path).ok())
                .and_then(|text| serde_json::from_str::<serde_json::Value>(&text).ok());

            if let Some(package_json) = package_json
                && let Some(manifest_package_name) = package_json
                    .get("name")
                    .and_then(serde_json::Value::as_str)
                    .map(str::trim)
                    .filter(|name| !name.is_empty())
                    .map(normalize_node_modules_package_specifier)
                    .filter(|name| !name.is_empty())
                    .or_else(|| Self::infer_package_name_from_node_modules_dir(&dir_normalized))
            {
                let package_name = Self::package_name_for_node_modules_manifest(
                    &dir_normalized,
                    &manifest_package_name,
                )
                .unwrap_or(manifest_package_name);
                if supports_package_exports && let Some(exports_value) = package_json.get("exports")
                {
                    return self.package_specifier_from_package_exports_value(
                        normalized_target,
                        &package_name,
                        &dir_normalized,
                        exports_value,
                        exports_mode,
                    );
                }

                let package_dir_prefix = format!("{dir_normalized}/");
                let target_relative = normalized_target
                    .strip_prefix(&package_dir_prefix)
                    .unwrap_or_default();
                let runtime_relative = package_runtime_specifier_from_target_path(target_relative);
                let runtime_spec = if runtime_relative.is_empty() {
                    package_name.clone()
                } else {
                    format!("{package_name}/{runtime_relative}")
                };

                if let Some(specifier) = package_main_module_specifier_for_target(
                    &package_json,
                    &package_name,
                    &runtime_spec,
                    normalized_target,
                ) {
                    return Some(specifier);
                }

                let spec = normalize_node_modules_package_specifier(&runtime_spec);
                if !spec.is_empty() {
                    return Some(spec);
                }
            }

            if dir_normalized == node_modules_root {
                break;
            }
            current_dir = dir.parent();
        }

        None
    }

    fn package_name_for_node_modules_manifest(
        dir_normalized: &str,
        manifest_package_name: &str,
    ) -> Option<String> {
        let marker = "/node_modules/";
        let marker_idx = dir_normalized.rfind(marker)?;
        let package_path = &dir_normalized[marker_idx + marker.len()..];
        let (package_root, package_suffix) = split_node_modules_package_path(package_path)?;
        if package_suffix.is_empty() {
            return None;
        }

        let package_root = normalize_node_modules_package_specifier(&package_root);
        if package_root == ".store" {
            return None;
        }

        let manifest_package_name = normalize_node_modules_package_specifier(manifest_package_name);
        if manifest_package_name == package_root
            || manifest_package_name
                .strip_prefix(&package_root)
                .is_some_and(|rest| rest.starts_with('/'))
        {
            return None;
        }

        let package_path_name =
            normalize_node_modules_package_specifier(&format!("{package_root}/{package_suffix}"));
        if package_path_name.is_empty() {
            None
        } else {
            Some(package_path_name)
        }
    }

    fn infer_package_name_from_node_modules_dir(dir_normalized: &str) -> Option<String> {
        let marker = "/node_modules/";
        let marker_idx = dir_normalized.rfind(marker)?;
        let package_path = &dir_normalized[marker_idx + marker.len()..];
        if package_path.is_empty() {
            return None;
        }
        let (package_root, _suffix) = split_node_modules_package_path(package_path)?;
        let package_name = normalize_node_modules_package_specifier(&package_root);
        if package_name.is_empty() {
            None
        } else {
            Some(package_name)
        }
    }

    fn package_specifier_from_package_exports(
        &self,
        normalized_target: &str,
        package_root: &str,
        package_prefix: &str,
        package_json_path: &str,
        exports_mode: ExportsResolutionMode,
    ) -> Option<String> {
        let package_json_text = if let Some(file) = self.files.get(package_json_path) {
            Some(file.source_text().to_string())
        } else {
            std::fs::read_to_string(package_json_path).ok()
        }?;

        let package_dir = format!("{package_prefix}{package_root}");
        let package_json = serde_json::from_str::<serde_json::Value>(&package_json_text).ok()?;
        let exports_value = package_json.get("exports")?;
        self.package_specifier_from_package_exports_value(
            normalized_target,
            package_root,
            &package_dir,
            exports_value,
            exports_mode,
        )
    }

    fn package_specifier_from_package_exports_value(
        &self,
        normalized_target: &str,
        package_specifier: &str,
        package_dir: &str,
        exports_value: &serde_json::Value,
        exports_mode: ExportsResolutionMode,
    ) -> Option<String> {
        let package_dir_prefix = format!("{package_dir}/");
        let target_relative_with_ext = normalized_target.strip_prefix(&package_dir_prefix)?;
        let target_runtime_extension = runtime_extension_for_source_path(target_relative_with_ext);
        let target_relative =
            path_to_string(&strip_js_ts_extension(Path::new(target_relative_with_ext)))
                .replace('\\', "/");

        if let Some(exports_target) = exports_value.as_str() {
            let target_pattern = path_to_string(&strip_js_ts_extension(Path::new(exports_target)))
                .replace('\\', "/");
            let target_pattern = target_pattern.strip_prefix("./").unwrap_or(&target_pattern);
            if wildcard_capture_case_insensitive(target_pattern, &target_relative).is_some() {
                return Some(package_specifier.to_string());
            }
            return None;
        }
        let exports_object = exports_value.as_object()?;

        // When no key starts with "./" and no key is exactly ".", the whole
        // object is treated as a top-level conditions map for the "." export.
        let has_subpath_entry = exports_object
            .keys()
            .any(|key| key == "." || key.starts_with("./"));
        if !has_subpath_entry {
            let (type_targets, default_targets) =
                collect_exports_targets(exports_value, exports_mode);
            for target_pattern in type_targets.iter().chain(default_targets.iter()) {
                let target_pattern = target_pattern.replace('\\', "/");
                let target_pattern = target_pattern.strip_prefix("./").unwrap_or(&target_pattern);
                let target_pattern =
                    path_to_string(&strip_js_ts_extension(Path::new(target_pattern)))
                        .replace('\\', "/");
                if wildcard_capture_case_insensitive(&target_pattern, &target_relative).is_some() {
                    return Some(package_specifier.to_string());
                }
            }
            return None;
        }

        for (export_key, export_target) in exports_object {
            let key_pattern = if export_key == "." {
                ""
            } else if let Some(rest) = export_key.strip_prefix("./") {
                rest
            } else {
                continue;
            };

            let (type_targets, default_targets) =
                collect_exports_targets(export_target, exports_mode);
            let should_append_js = key_pattern.contains('*')
                && !has_source_extension(key_pattern)
                && default_targets
                    .iter()
                    .any(|target| !has_source_extension(target));
            // If the exports key explicitly spells an extension (e.g.
            // `./b/*.js`), only files whose runtime extension matches that
            // extension should resolve through this entry. This prevents
            // `.mts`/`.cts` source files from being routed through a `.js`-
            // only wildcard, matching Node's resolution semantics.
            let required_runtime_ext = if key_pattern.ends_with(".js") {
                Some(".js")
            } else if key_pattern.ends_with(".mjs") {
                Some(".mjs")
            } else if key_pattern.ends_with(".cjs") {
                Some(".cjs")
            } else {
                None
            };

            for target_pattern in type_targets.iter().chain(default_targets.iter()) {
                let target_pattern = target_pattern.replace('\\', "/");
                let target_pattern = target_pattern.strip_prefix("./").unwrap_or(&target_pattern);
                let target_pattern =
                    path_to_string(&strip_js_ts_extension(Path::new(target_pattern)))
                        .replace('\\', "/");

                let Some(capture) =
                    wildcard_capture_case_insensitive(&target_pattern, &target_relative)
                else {
                    continue;
                };

                if let Some(required_ext) = required_runtime_ext
                    && target_runtime_extension != required_ext
                {
                    continue;
                }

                if export_key == "." {
                    return Some(package_specifier.to_string());
                }

                let mut subpath = apply_wildcard_capture(key_pattern, &capture)?;
                if should_append_js && !has_source_extension(&subpath) {
                    subpath.push_str(target_runtime_extension);
                }
                if subpath.is_empty() {
                    return Some(package_specifier.to_string());
                }
                return Some(format!("{package_specifier}/{subpath}"));
            }
        }

        None
    }
}

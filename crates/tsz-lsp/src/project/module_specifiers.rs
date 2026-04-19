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

use super::Project;

const TS_EXTENSION_CANDIDATES: [&str; 7] = ["ts", "tsx", "d.ts", "mts", "cts", "d.mts", "d.cts"];
const TS_EXTENSION_SUFFIXES: [&str; 7] =
    [".d.ts", ".d.mts", ".d.cts", ".ts", ".tsx", ".mts", ".cts"];

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
        let target_in_node_modules = target_file.replace('\\', "/").contains("/node_modules/");
        let supports_package_exports = self.module_resolution_supports_package_exports(from_file);
        let exports_mode = self.exports_resolution_mode_for_importer(from_file);
        let package_specifier = self.package_specifier_from_node_modules_with_mode(
            target_file,
            supports_package_exports,
            exports_mode,
        );
        let workspace_package_specifier = self.workspace_package_dependency_specifier(
            from_file,
            target_file,
            target_in_node_modules,
            supports_package_exports,
            exports_mode,
        );

        let Some(relative) = self.relative_module_specifier_from_files(from_file, target_file)
        else {
            let mut only_packages: Vec<String> = workspace_package_specifier.into_iter().collect();
            only_packages.extend(package_specifier);
            let mut seen = FxHashSet::default();
            only_packages.retain(|spec| seen.insert(spec.clone()));
            return only_packages;
        };

        let root_dirs_relative =
            self.root_dirs_relative_specifier_from_files(from_file, target_file);
        let path_mappings = self.path_mapping_specifiers_from_files(from_file, target_file);
        let package_imports = self.package_import_specifiers_from_files(from_file, target_file);
        let pref = self.import_module_specifier_preference.as_deref();
        let mut candidates = Vec::new();

        if pref == Some("non-relative") {
            candidates.extend(path_mappings);
            candidates.extend(package_imports);
            if let Some(workspace_package_specifier) = workspace_package_specifier.as_ref() {
                candidates.push(workspace_package_specifier.clone());
            }
            if let Some(package_specifier) = package_specifier.as_ref() {
                candidates.push(package_specifier.clone());
            }
            candidates.push(relative);
            if let Some(root_dirs_relative) = root_dirs_relative {
                candidates.push(root_dirs_relative);
            }
        } else if pref == Some("relative") || pref == Some("project-relative") {
            // TypeScript still prefers dependency package specifiers (workspace links /
            // node_modules) over deep relative traversals, even under explicit
            // `relative` preference.
            if let Some(workspace_package_specifier) = workspace_package_specifier.as_ref() {
                candidates.push(workspace_package_specifier.clone());
            }
            if let Some(package_specifier) = package_specifier.as_ref() {
                candidates.push(package_specifier.clone());
            }
            candidates.push(relative);
            if let Some(root_dirs_relative) = root_dirs_relative {
                candidates.push(root_dirs_relative);
            }
            candidates.extend(path_mappings);
            candidates.extend(package_imports);
        } else {
            candidates.push(relative);
            if let Some(root_dirs_relative) = root_dirs_relative {
                candidates.push(root_dirs_relative);
            }
            candidates.extend(path_mappings);
            candidates.extend(package_imports);
            if let Some(workspace_package_specifier) = workspace_package_specifier.as_ref() {
                candidates.push(workspace_package_specifier.clone());
            }
            if let Some(package_specifier) = package_specifier.as_ref() {
                candidates.push(package_specifier.clone());
            }
        }

        let mut seen = FxHashSet::default();
        candidates.retain(|spec| seen.insert(spec.clone()));
        if target_in_node_modules {
            candidates.retain(|spec| !spec.replace('\\', "/").contains("node_modules/"));
        }

        if pref.is_none() || pref == Some("shortest") {
            candidates.sort_by(compare_module_specifier_candidates);
        } else if pref == Some("non-relative") {
            candidates.sort_by(|a, b| {
                let a_relative = a.starts_with('.');
                let b_relative = b.starts_with('.');
                a_relative
                    .cmp(&b_relative)
                    .then_with(|| compare_module_specifier_candidates(a, b))
            });
        }

        candidates
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
        let mut seen_targets = FxHashSet::default();
        target_candidates.retain(|candidate| seen_targets.insert(candidate.clone()));

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
        self.import_module_specifier_preference.as_deref() == Some("project-relative")
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
        } else if let Some(rest) = specifier.strip_prefix("workspace:") {
            if rest.starts_with('.') || rest.starts_with('/') {
                rest
            } else {
                return None;
            }
        } else {
            return None;
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

        let base_url = compiler_options
            .get("baseUrl")
            .and_then(serde_json::Value::as_str)
            .unwrap_or(".");
        let base_dir = normalize_path(&config_dir.join(base_url));
        let normalized_target_file = path_to_string(&strip_js_ts_extension(&normalize_path(
            Path::new(target_file),
        )))
        .replace('\\', "/");
        let mut target_candidates = vec![normalized_target_file];
        target_candidates.extend(self.project_output_target_alternatives(target_file));
        let mut seen_targets = FxHashSet::default();
        target_candidates.retain(|candidate| seen_targets.insert(candidate.clone()));

        let mut specifiers = Vec::new();
        for (alias_pattern, mapped_targets) in paths {
            let Some(mapped_targets) = mapped_targets.as_array() else {
                continue;
            };
            for mapped_target in mapped_targets {
                let Some(mapped_target) = mapped_target.as_str() else {
                    continue;
                };
                let mapped_target = mapped_target.replace('\\', "/");
                let mapped_target = if let Some(rest) = mapped_target.strip_prefix("${configDir}/")
                {
                    path_to_string(&normalize_path(&config_dir.join(rest))).replace('\\', "/")
                } else {
                    path_to_string(&normalize_path(&base_dir.join(&mapped_target)))
                        .replace('\\', "/")
                };
                let mapped_target =
                    path_to_string(&strip_js_ts_extension(Path::new(&mapped_target)))
                        .replace('\\', "/");

                let Some(capture) = target_candidates.iter().find_map(|candidate| {
                    wildcard_capture_case_insensitive(&mapped_target, candidate)
                }) else {
                    continue;
                };
                let Some(specifier) = apply_wildcard_capture(alias_pattern, &capture) else {
                    continue;
                };
                specifiers.push(normalize_path_mapping_specifier(&specifier));
            }
        }

        let mut seen = FxHashSet::default();
        specifiers.retain(|specifier| seen.insert(specifier.clone()));
        specifiers
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

        let from_path = strip_ts_extension(&normalize_path(Path::new(from_file)));
        let target_path = strip_ts_extension(&normalize_path(Path::new(target_file)));
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
                let config_text = self
                    .files
                    .get(&config_key)
                    .map(|f| f.source_text().to_string())
                    .or_else(|| std::fs::read_to_string(&config_key).ok());
                let Some(config_text) = config_text else {
                    continue;
                };
                let Some(config_json) = parse_typescript_config_json(&config_text) else {
                    continue;
                };
                let Some(compiler_options) = config_json
                    .get("compilerOptions")
                    .and_then(serde_json::Value::as_object)
                    .cloned()
                else {
                    continue;
                };
                return Some((normalize_path(dir), compiler_options));
            }
            current = dir.parent();
        }
        None
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
            if let Some(allow) = self.auto_imports_allowed_from_fourslash_directives(from_file) {
                return allow;
            }
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

    fn auto_imports_allowed_from_fourslash_directives(&self, from_file: &str) -> Option<bool> {
        self.files
            .get(from_file)
            .and_then(|file| Self::fourslash_auto_import_directive_result(file.source_text()))
            .or_else(|| {
                self.files.values().find_map(|file| {
                    (file.file_name != from_file)
                        .then(|| Self::fourslash_auto_import_directive_result(file.source_text()))
                        .flatten()
                })
            })
    }

    fn fourslash_auto_import_directive_result(source_text: &str) -> Option<bool> {
        let mut saw_module = false;
        let mut module_none = false;
        let mut saw_target = false;
        let mut target_supports_imports = false;

        for line in source_text.lines().take(64) {
            let trimmed = line.trim_start();
            if let Some(rest) = trimmed.strip_prefix("// @module:") {
                saw_module = true;
                module_none = rest.split(',').map(str::trim).any(|value| {
                    value.eq_ignore_ascii_case("none") || value.parse::<i64>().ok() == Some(0)
                });
                continue;
            }

            if let Some(rest) = trimmed.strip_prefix("// @target:") {
                saw_target = true;
                target_supports_imports = rest
                    .split(',')
                    .map(str::trim)
                    .any(target_supports_import_syntax);
            }
        }

        if saw_module && module_none {
            return Some(saw_target && target_supports_imports);
        }

        None
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
        let target_path = strip_ts_extension(Path::new(target_file));
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

    fn package_specifier_from_node_modules_with_mode(
        &self,
        target_file: &str,
        supports_package_exports: bool,
        exports_mode: ExportsResolutionMode,
    ) -> Option<String> {
        let normalized = target_file.replace('\\', "/");
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
                && let Some(package_name) = package_json
                    .get("name")
                    .and_then(serde_json::Value::as_str)
                    .map(str::trim)
                    .filter(|name| !name.is_empty())
                    .map(normalize_node_modules_package_specifier)
                    .filter(|name| !name.is_empty())
                    .or_else(|| Self::infer_package_name_from_node_modules_dir(&dir_normalized))
            {
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

fn normalize_path(path: &Path) -> PathBuf {
    let mut normalized = PathBuf::new();

    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                normalized.pop();
            }
            Component::RootDir | Component::Normal(_) | Component::Prefix(_) => {
                normalized.push(component.as_os_str());
            }
        }
    }

    normalized
}

fn strip_ts_extension(path: &Path) -> PathBuf {
    let Some(file_name) = path.file_name().and_then(|name| name.to_str()) else {
        return path.to_path_buf();
    };

    for suffix in TS_EXTENSION_SUFFIXES {
        if let Some(base_name) = file_name.strip_suffix(suffix) {
            if base_name.is_empty() {
                return path.to_path_buf();
            }
            let mut base = PathBuf::new();
            if let Some(parent) = path.parent() {
                base.push(parent);
            }
            base.push(base_name);
            return base;
        }
    }

    path.to_path_buf()
}
fn split_node_modules_package_path(package_path: &str) -> Option<(String, String)> {
    let mut segments = package_path.split('/');
    let first = segments.next()?;
    if first.is_empty() {
        return None;
    }

    if first.starts_with('@') {
        let second = segments.next()?;
        let package_root = format!("{first}/{second}");
        let suffix = segments.collect::<Vec<_>>().join("/");
        Some((package_root, suffix))
    } else {
        let suffix = segments.collect::<Vec<_>>().join("/");
        Some((first.to_string(), suffix))
    }
}

fn normalize_node_modules_package_specifier(package_specifier: &str) -> String {
    let mut normalized = package_specifier.replace('\\', "/");
    if let Some(stripped) = normalized.strip_suffix("/index")
        && !stripped.is_empty()
    {
        normalized = stripped.to_string();
    }

    if let Some(stripped) = normalized.strip_prefix("@types/") {
        let mut parts = stripped.splitn(2, '/');
        let package_name = parts.next().unwrap_or_default();
        let rest = parts.next();

        let package_name = if let Some((scope, name)) = package_name.split_once("__") {
            format!("@{scope}/{name}")
        } else {
            package_name.to_string()
        };

        return match rest {
            Some(rest) if !rest.is_empty() && rest != "index" => {
                format!("{package_name}/{rest}")
            }
            _ => package_name,
        };
    }

    normalized
}

fn normalize_path_mapping_specifier(specifier: &str) -> String {
    specifier
        .strip_suffix("/index")
        .unwrap_or(specifier)
        .to_string()
}

fn package_runtime_specifier_from_target_path(package_path: &str) -> String {
    let normalized = package_path.replace('\\', "/");

    if let Some(base) = normalized.strip_suffix(".d.mts") {
        return format!("{base}.mjs");
    }
    if let Some(base) = normalized.strip_suffix(".d.cts") {
        return format!("{base}.cjs");
    }
    if let Some(base) = normalized.strip_suffix(".d.ts") {
        return base.to_string();
    }
    // For TS/TSX source files under node_modules (symlinked packages), the
    // runtime specifier is the extension-less form so downstream normalization
    // can collapse `pkg/index` to `pkg`.
    if let Some(base) = normalized.strip_suffix(".mts") {
        return format!("{base}.mjs");
    }
    if let Some(base) = normalized.strip_suffix(".cts") {
        return format!("{base}.cjs");
    }
    if let Some(base) = normalized
        .strip_suffix(".ts")
        .or_else(|| normalized.strip_suffix(".tsx"))
    {
        return base.to_string();
    }

    normalized
}

fn is_declaration_source_path(path: &str) -> bool {
    path.ends_with(".d.ts") || path.ends_with(".d.mts") || path.ends_with(".d.cts")
}

fn normalize_package_entry_for_match(path: &str) -> String {
    let path = path.replace('\\', "/");
    let path = path.strip_prefix("./").unwrap_or(&path);
    let stripped = path_to_string(&strip_js_ts_extension(Path::new(path))).replace('\\', "/");
    stripped
        .strip_suffix("/index")
        .unwrap_or(&stripped)
        .to_string()
}

fn package_main_module_specifier_for_target(
    package_json: &serde_json::Value,
    package_root: &str,
    runtime_package_spec: &str,
    target_file: &str,
) -> Option<String> {
    let package_prefix = format!("{package_root}/");
    let runtime_subpath = runtime_package_spec.strip_prefix(&package_prefix)?;
    let runtime_normalized = normalize_package_entry_for_match(runtime_subpath);
    if runtime_normalized.is_empty() {
        return None;
    }

    let package_type_module = package_json
        .get("type")
        .and_then(serde_json::Value::as_str)
        .is_some_and(|value| value == "module");

    if is_declaration_source_path(target_file) {
        // For declaration targets, only treat package `types`/`typings` entries
        // as root aliases. Runtime `main`/`module` declarations should not
        // collapse arbitrary .d.ts subpaths to the package root.
        for entry_field in ["types", "typings"] {
            let Some(entry) = package_json
                .get(entry_field)
                .and_then(serde_json::Value::as_str)
            else {
                continue;
            };
            let entry_normalized = normalize_package_entry_for_match(entry);
            if !entry_normalized.is_empty() && entry_normalized == runtime_normalized {
                return Some(package_root.to_string());
            }
        }
        return None;
    }

    for entry_field in ["module", "main"] {
        let Some(entry) = package_json
            .get(entry_field)
            .and_then(serde_json::Value::as_str)
        else {
            continue;
        };
        let entry_normalized = normalize_package_entry_for_match(entry);
        if entry_normalized.is_empty() || entry_normalized != runtime_normalized {
            continue;
        }

        if package_type_module {
            return Some(format!("{package_root}/{entry_normalized}"));
        }

        return Some(package_root.to_string());
    }

    None
}

fn has_ts_extension(module_text: &str) -> bool {
    module_text.ends_with(".ts")
        || module_text.ends_with(".tsx")
        || module_text.ends_with(".mts")
        || module_text.ends_with(".cts")
}

fn has_js_extension(module_text: &str) -> bool {
    module_text.ends_with(".js")
        || module_text.ends_with(".jsx")
        || module_text.ends_with(".mjs")
        || module_text.ends_with(".cjs")
}

fn ts_source_extension(target_file: &str) -> Option<&'static str> {
    if target_file.ends_with(".tsx") {
        Some(".tsx")
    } else if target_file.ends_with(".ts") && !target_file.ends_with(".d.ts") {
        Some(".ts")
    } else if target_file.ends_with(".mts") && !target_file.ends_with(".d.mts") {
        Some(".mts")
    } else if target_file.ends_with(".cts") && !target_file.ends_with(".d.cts") {
        Some(".cts")
    } else {
        None
    }
}

fn target_supports_import_syntax(target: &str) -> bool {
    let target = target.trim();
    if let Ok(numeric_target) = target.parse::<i64>() {
        return numeric_target >= 2;
    }

    target.eq_ignore_ascii_case("es6")
        || target.eq_ignore_ascii_case("es2015")
        || target.eq_ignore_ascii_case("es2016")
        || target.eq_ignore_ascii_case("es2017")
        || target.eq_ignore_ascii_case("es2018")
        || target.eq_ignore_ascii_case("es2019")
        || target.eq_ignore_ascii_case("es2020")
        || target.eq_ignore_ascii_case("es2021")
        || target.eq_ignore_ascii_case("es2022")
        || target.eq_ignore_ascii_case("es2023")
        || target.eq_ignore_ascii_case("es2024")
        || target.eq_ignore_ascii_case("esnext")
        || target.eq_ignore_ascii_case("latest")
}

fn relative_path(from: &Path, to: &Path) -> PathBuf {
    let from_components: Vec<_> = from
        .components()
        .filter(|c| *c != Component::CurDir)
        .collect();
    let to_components: Vec<_> = to
        .components()
        .filter(|c| *c != Component::CurDir)
        .collect();

    let mut common = 0;
    while common < from_components.len()
        && common < to_components.len()
        && from_components[common] == to_components[common]
    {
        common += 1;
    }

    let mut result = PathBuf::new();
    for _ in common..from_components.len() {
        result.push("..");
    }
    for component in &to_components[common..] {
        result.push(component.as_os_str());
    }

    if result.as_os_str().is_empty() {
        result.push(".");
    }

    result
}

fn path_to_string(path: &Path) -> String {
    path.to_string_lossy().to_string()
}

fn parse_typescript_config_json(text: &str) -> Option<serde_json::Value> {
    serde_json::from_str(text)
        .ok()
        .or_else(|| json5::from_str::<serde_json::Value>(text).ok())
}

fn compare_module_specifier_candidates(a: &String, b: &String) -> Ordering {
    let a_segments = a.matches('/').count();
    let b_segments = b.matches('/').count();
    let candidate_rank = |candidate: &str| -> u8 {
        if candidate.starts_with("./") {
            0
        } else if !candidate.starts_with('.') {
            1
        } else if candidate.starts_with("../") {
            2
        } else {
            3
        }
    };
    let a_rank = candidate_rank(a);
    let b_rank = candidate_rank(b);
    a_segments
        .cmp(&b_segments)
        .then_with(|| a_rank.cmp(&b_rank))
        .then_with(|| a.len().cmp(&b.len()))
        .then_with(|| a.cmp(b))
}

fn package_import_specifiers_for_target(
    package_json_text: &str,
    package_dir: &str,
    target_file: &str,
    allow_importing_ts_extensions: bool,
    additional_targets: &[String],
) -> Vec<String> {
    let Some(package_json) = serde_json::from_str::<serde_json::Value>(package_json_text).ok()
    else {
        return Vec::new();
    };

    let Some(imports) = package_json
        .get("imports")
        .and_then(serde_json::Value::as_object)
    else {
        return Vec::new();
    };
    let package_type_module = package_json
        .get("type")
        .and_then(serde_json::Value::as_str)
        .is_some_and(|v| v == "module");

    let package_dir = normalize_path(Path::new(package_dir));
    let target_path = strip_js_ts_extension(Path::new(target_file));
    let target_normalized = path_to_string(&target_path).replace('\\', "/");

    let mut specs = Vec::new();

    for (specifier_pattern, target_mapping) in imports {
        if !specifier_pattern.starts_with('#') {
            continue;
        }

        let target_patterns = collect_import_targets(target_mapping);
        for target_pattern in target_patterns {
            let target_pattern = target_pattern.replace('\\', "/");
            if !target_pattern.starts_with("./") {
                continue;
            }

            let resolved = normalize_path(&package_dir.join(&target_pattern));
            let resolved_stripped =
                path_to_string(&strip_js_ts_extension(&resolved)).replace('\\', "/");

            let is_prefix_mapping = !specifier_pattern.contains('*')
                && !target_pattern.contains('*')
                && specifier_pattern.ends_with('/')
                && target_pattern.ends_with('/');
            let direct_capture =
                wildcard_capture_case_insensitive(&resolved_stripped, &target_normalized).or_else(
                    || {
                        if is_prefix_mapping {
                            prefix_capture_case_insensitive(&resolved_stripped, &target_normalized)
                        } else {
                            None
                        }
                    },
                );
            let additional_capture = additional_targets.iter().find_map(|candidate| {
                wildcard_capture_case_insensitive(&resolved_stripped, candidate).or_else(|| {
                    if is_prefix_mapping {
                        prefix_capture_case_insensitive(&resolved_stripped, candidate)
                    } else {
                        None
                    }
                })
            });
            let matched_via_additional_target =
                direct_capture.is_none() && additional_capture.is_some();
            let capture = direct_capture.or(additional_capture);
            let Some(capture) = capture else {
                continue;
            };

            let mut specifier =
                if let Some(specifier) = apply_wildcard_capture(specifier_pattern, &capture) {
                    specifier
                } else if is_prefix_mapping {
                    format!("{specifier_pattern}{capture}")
                } else {
                    continue;
                };

            if (specifier_pattern.contains('*') || is_prefix_mapping)
                && !specifier_pattern.ends_with(".js")
                && !specifier_pattern.ends_with(".ts")
                && !has_source_extension(&target_pattern)
                && !has_source_extension(&specifier)
            {
                let prefer_ts_extension = allow_importing_ts_extensions
                    && !matched_via_additional_target
                    && (specifier_pattern.contains('/')
                        || (package_type_module && resolved_stripped.contains("/src/")));
                if prefer_ts_extension {
                    if let Some(ext) = ts_source_extension(target_file) {
                        specifier.push_str(ext);
                    } else {
                        specifier.push_str(".js");
                    }
                } else {
                    specifier.push_str(".js");
                }
            }

            specs.push(specifier);
        }
    }

    let mut seen = FxHashSet::default();
    specs.retain(|spec| seen.insert(spec.clone()));
    specs.sort_by(compare_module_specifier_candidates);
    specs
}

fn collect_import_targets(value: &serde_json::Value) -> Vec<String> {
    match value {
        serde_json::Value::String(text) => vec![text.to_string()],
        serde_json::Value::Array(items) => items.iter().flat_map(collect_import_targets).collect(),
        serde_json::Value::Object(map) => map.values().flat_map(collect_import_targets).collect(),
        _ => Vec::new(),
    }
}

fn collect_exports_targets(
    value: &serde_json::Value,
    mode: ExportsResolutionMode,
) -> (Vec<String>, Vec<String>) {
    let mut types = Vec::new();
    let mut defaults = Vec::new();
    collect_exports_targets_inner(value, false, mode, &mut types, &mut defaults);
    (types, defaults)
}

fn collect_exports_targets_inner(
    value: &serde_json::Value,
    is_types_branch: bool,
    mode: ExportsResolutionMode,
    types: &mut Vec<String>,
    defaults: &mut Vec<String>,
) {
    match value {
        serde_json::Value::String(text) => {
            if is_types_branch {
                types.push(text.to_string());
            } else {
                defaults.push(text.to_string());
            }
        }
        serde_json::Value::Array(items) => {
            // Per Node's resolution algorithm, only the FIRST array element
            // that yields a resolvable target should be used. Recurse into
            // items one at a time and stop once either target list grows,
            // matching tsserver's exports-map behavior for alternates.
            let initial_types = types.len();
            let initial_defaults = defaults.len();
            for item in items {
                collect_exports_targets_inner(item, is_types_branch, mode, types, defaults);
                if types.len() > initial_types || defaults.len() > initial_defaults {
                    break;
                }
            }
        }
        serde_json::Value::Object(map) => {
            for (key, item) in map {
                let key_is_types = key == "types";
                let include_default_branch = match key.as_str() {
                    "types" => false,
                    "import" => mode != ExportsResolutionMode::Require,
                    "require" => mode != ExportsResolutionMode::Import,
                    // Preserve fallback behavior for `default`, subpath maps, and
                    // unknown conditions by treating them as available.
                    _ => true,
                };
                if !key_is_types && !include_default_branch {
                    continue;
                }
                collect_exports_targets_inner(
                    item,
                    is_types_branch || key_is_types,
                    mode,
                    types,
                    defaults,
                );
            }
        }
        _ => {}
    }
}

fn apply_wildcard_capture(specifier_pattern: &str, capture: &str) -> Option<String> {
    if let Some((prefix, suffix)) = specifier_pattern.split_once('*') {
        let mut spec = String::with_capacity(prefix.len() + capture.len() + suffix.len());
        spec.push_str(prefix);
        spec.push_str(capture);
        spec.push_str(suffix);
        return Some(spec);
    }

    if capture.is_empty() {
        return Some(specifier_pattern.to_string());
    }

    None
}

fn wildcard_capture_case_insensitive(pattern: &str, target: &str) -> Option<String> {
    fn capture(pattern: &str, target: &str) -> Option<String> {
        let pattern_lower = pattern.to_ascii_lowercase();
        let target_lower = target.to_ascii_lowercase();
        if let Some((prefix, suffix)) = pattern_lower.split_once('*') {
            if !target_lower.starts_with(prefix) || !target_lower.ends_with(suffix) {
                return None;
            }
            let start = prefix.len();
            let end = target_lower.len().saturating_sub(suffix.len());
            return Some(target[start..end].to_string());
        }
        (pattern_lower == target_lower).then_some(String::new())
    }

    let pattern = pattern.replace('\\', "/");
    let target = target.replace('\\', "/");

    capture(&pattern, &target)
        .or_else(|| pattern.strip_prefix('/').and_then(|p| capture(p, &target)))
        .or_else(|| target.strip_prefix('/').and_then(|t| capture(&pattern, t)))
        .or_else(|| {
            pattern
                .strip_prefix('/')
                .zip(target.strip_prefix('/'))
                .and_then(|(p, t)| capture(p, t))
        })
}

fn prefix_capture_case_insensitive(prefix_pattern: &str, target: &str) -> Option<String> {
    let pattern = prefix_pattern.replace('\\', "/");
    let target = target.replace('\\', "/");
    let pattern = pattern.trim_end_matches('/');

    if pattern.is_empty() {
        return None;
    }

    fn capture(pattern: &str, target: &str) -> Option<String> {
        let pattern_lower = pattern.to_ascii_lowercase();
        let target_lower = target.to_ascii_lowercase();
        if target_lower == pattern_lower {
            return Some(String::new());
        }
        if !target_lower.starts_with(&pattern_lower) {
            return None;
        }
        let rest = target.get(pattern.len()..)?;
        let rest = rest.strip_prefix('/')?;
        Some(rest.to_string())
    }

    capture(pattern, &target)
        .or_else(|| pattern.strip_prefix('/').and_then(|p| capture(p, &target)))
        .or_else(|| target.strip_prefix('/').and_then(|t| capture(pattern, t)))
        .or_else(|| {
            pattern
                .strip_prefix('/')
                .zip(target.strip_prefix('/'))
                .and_then(|(p, t)| capture(p, t))
        })
}

fn strip_js_ts_extension(path: &Path) -> PathBuf {
    const SOURCE_SUFFIXES: [&str; 11] = [
        ".d.ts", ".d.mts", ".d.cts", ".ts", ".tsx", ".mts", ".cts", ".js", ".jsx", ".mjs", ".cjs",
    ];
    let Some(file_name) = path.file_name().and_then(|name| name.to_str()) else {
        return path.to_path_buf();
    };

    for suffix in SOURCE_SUFFIXES {
        if let Some(base_name) = file_name.strip_suffix(suffix) {
            if base_name.is_empty() {
                return path.to_path_buf();
            }
            let mut base = PathBuf::new();
            if let Some(parent) = path.parent() {
                base.push(parent);
            }
            base.push(base_name);
            return base;
        }
    }

    path.to_path_buf()
}

/// Returns the runtime (emit) extension for a source file path, preserving
/// the ESM/CJS flavor. `.mts`/`.d.mts`/`.mjs` → `.mjs`, `.cts`/`.d.cts`/`.cjs`
/// → `.cjs`, everything else → `.js`.
fn runtime_extension_for_source_path(path: &str) -> &'static str {
    let normalized = path.replace('\\', "/");
    if normalized.ends_with(".d.mts")
        || normalized.ends_with(".mts")
        || normalized.ends_with(".mjs")
    {
        return ".mjs";
    }
    if normalized.ends_with(".d.cts")
        || normalized.ends_with(".cts")
        || normalized.ends_with(".cjs")
    {
        return ".cjs";
    }
    ".js"
}

fn has_source_extension(path: &str) -> bool {
    let normalized = path.replace('\\', "/");
    normalized.ends_with(".d.ts")
        || normalized.ends_with(".d.mts")
        || normalized.ends_with(".d.cts")
        || normalized.ends_with(".ts")
        || normalized.ends_with(".tsx")
        || normalized.ends_with(".mts")
        || normalized.ends_with(".cts")
        || normalized.ends_with(".js")
        || normalized.ends_with(".jsx")
        || normalized.ends_with(".mjs")
        || normalized.ends_with(".cjs")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn package_specifier_prefers_package_root_for_commonjs_main_module_entrypoint() {
        let mut project = Project::new();
        project.set_file(
            "/node_modules/pkg/package.json".to_string(),
            r#"{
  "name": "pkg",
  "version": "1.0.0",
  "main": "lib",
  "module": "lib"
}"#
            .to_string(),
        );
        project.set_file(
            "/node_modules/pkg/lib/index.js".to_string(),
            "export function foo() {}".to_string(),
        );

        assert_eq!(
            project.package_specifier_from_node_modules("/node_modules/pkg/lib/index.js"),
            Some("pkg".to_string())
        );
    }

    #[test]
    fn package_specifier_uses_subpath_for_type_module_main_entrypoint() {
        let mut project = Project::new();
        project.set_file(
            "/node_modules/pkg/package.json".to_string(),
            r#"{
  "name": "pkg",
  "version": "1.0.0",
  "main": "lib",
  "type": "module"
}"#
            .to_string(),
        );
        project.set_file(
            "/node_modules/pkg/lib/index.js".to_string(),
            "export function foo() {}".to_string(),
        );

        assert_eq!(
            project.package_specifier_from_node_modules("/node_modules/pkg/lib/index.js"),
            Some("pkg/lib".to_string())
        );
    }

    #[test]
    fn package_specifier_maps_dmts_to_mjs_without_collapsing_to_package_root() {
        let mut project = Project::new();
        project.set_file(
            "/node_modules/pkg/package.json".to_string(),
            r#"{
  "name": "pkg",
  "version": "1.0.0",
  "main": "lib"
}"#
            .to_string(),
        );
        project.set_file(
            "/node_modules/pkg/lib/index.d.mts".to_string(),
            "export declare function foo(): any;".to_string(),
        );

        assert_eq!(
            project.package_specifier_from_node_modules("/node_modules/pkg/lib/index.d.mts"),
            Some("pkg/lib/index.mjs".to_string())
        );
    }

    #[test]
    fn package_specifier_maps_dcts_to_cjs_when_no_package_json_exists() {
        let mut project = Project::new();
        project.set_file(
            "/node_modules/lit/index.d.cts".to_string(),
            "export declare function customElement(name: string): any;".to_string(),
        );

        assert_eq!(
            project.package_specifier_from_node_modules("/node_modules/lit/index.d.cts"),
            Some("lit/index.cjs".to_string())
        );
    }

    #[test]
    fn package_specifier_collapses_extensionless_root_index_to_package_name() {
        let mut project = Project::new();
        project.set_file(
            "/node_modules/bar/index.d.ts".to_string(),
            "export declare const fromBar: number;".to_string(),
        );

        assert_eq!(
            project.package_specifier_from_node_modules("/node_modules/bar/index.d.ts"),
            Some("bar".to_string())
        );
    }

    #[test]
    fn workspace_dependency_uses_declared_package_name_specifier() {
        let mut project = Project::new();
        project.set_file(
            "/home/src/workspaces/project/packages/common/package.json".to_string(),
            r#"{
  "name": "@company/common",
  "version": "1.0.0",
  "main": "./lib/index.tsx"
}"#
            .to_string(),
        );
        project.set_file(
            "/home/src/workspaces/project/packages/common/lib/index.tsx".to_string(),
            "export function Tooltip() {}".to_string(),
        );
        project.set_file(
            "/home/src/workspaces/project/packages/app/package.json".to_string(),
            r#"{
  "name": "@company/app",
  "version": "1.0.0",
  "dependencies": {
    "@company/common": "1.0.0"
  }
}"#
            .to_string(),
        );
        project.set_file(
            "/home/src/workspaces/project/packages/app/lib/index.ts".to_string(),
            "Tooltip".to_string(),
        );

        let specifiers = project.auto_import_module_specifiers_from_files(
            "/home/src/workspaces/project/packages/app/lib/index.ts",
            "/home/src/workspaces/project/packages/common/lib/index.tsx",
        );
        assert!(
            specifiers
                .iter()
                .any(|specifier| specifier == "@company/common"),
            "expected @company/common specifier from workspace dependency, got {specifiers:?}"
        );
    }

    #[test]
    fn workspace_file_dependency_alias_uses_dependency_name_specifier() {
        let mut project = Project::new();
        project.set_file(
            "/home/src/workspaces/solution/packages/utils/package.json".to_string(),
            r#"{
  "name": "utils",
  "version": "1.0.0",
  "exports": "./dist/index.js"
}"#
            .to_string(),
        );
        project.set_file(
            "/home/src/workspaces/solution/packages/utils/tsconfig.json".to_string(),
            r#"{
  "compilerOptions": {
    "lib": ["es5"],
    "composite": true,
    "module": "nodenext",
    "rootDir": "src",
    "outDir": "dist"
  },
  "include": ["src"]
}"#
            .to_string(),
        );
        project.set_file(
            "/home/src/workspaces/solution/packages/utils/src/index.ts".to_string(),
            "export function gainUtility() { return 0; }".to_string(),
        );
        project.set_file(
            "/home/src/workspaces/solution/packages/web/package.json".to_string(),
            r#"{
  "name": "web",
  "version": "1.0.0",
  "dependencies": {
    "@monorepo/utils": "file:../utils"
  }
}"#
            .to_string(),
        );
        project.set_file(
            "/home/src/workspaces/solution/packages/web/src/index.ts".to_string(),
            "gainUtility".to_string(),
        );

        let specifiers = project.auto_import_module_specifiers_from_files(
            "/home/src/workspaces/solution/packages/web/src/index.ts",
            "/home/src/workspaces/solution/packages/utils/src/index.ts",
        );
        assert!(
            specifiers
                .iter()
                .any(|specifier| specifier == "@monorepo/utils"),
            "expected @monorepo/utils specifier from file-linked dependency alias, got {specifiers:?}"
        );
    }

    #[test]
    fn project_relative_preference_still_prefers_workspace_dependency_bare_specifier() {
        let mut project = Project::new();
        project.set_import_module_specifier_preference(Some("project-relative".to_string()));
        project.set_file(
            "/home/src/workspaces/project/package.json".to_string(),
            r#"{
  "dependencies": {
    "mylib": "file:packages/mylib"
  }
}"#
            .to_string(),
        );
        project.set_file(
            "/home/src/workspaces/project/src/index.ts".to_string(),
            "const value = MyClass".to_string(),
        );
        project.set_file(
            "/home/src/workspaces/project/packages/mylib/package.json".to_string(),
            r#"{
  "name": "mylib",
  "version": "1.0.0",
  "main": "index.js",
  "types": "index"
}"#
            .to_string(),
        );
        project.set_file(
            "/home/src/workspaces/project/packages/mylib/index.ts".to_string(),
            "export class MyClass {}".to_string(),
        );

        let specifiers = project.auto_import_module_specifiers_from_files(
            "/home/src/workspaces/project/src/index.ts",
            "/home/src/workspaces/project/packages/mylib/index.ts",
        );

        assert_eq!(
            specifiers.first().map(String::as_str),
            Some("mylib"),
            "expected workspace package bare specifier to win under project-relative preference, got {specifiers:?}"
        );
        assert!(
            !specifiers.iter().any(|specifier| specifier.contains(".ts")),
            "expected runtime-safe specifiers without .ts extensions, got {specifiers:?}"
        );
    }

    #[test]
    fn workspace_file_dependency_alias_works_without_target_package_manifest_loaded() {
        let mut project = Project::new();
        project.set_import_module_specifier_preference(Some("project-relative".to_string()));
        project.set_file(
            "/home/src/workspaces/project/package.json".to_string(),
            r#"{
  "dependencies": {
    "mylib": "file:packages/mylib"
  }
}"#
            .to_string(),
        );
        project.set_file(
            "/home/src/workspaces/project/src/index.ts".to_string(),
            "const value = MyClass".to_string(),
        );
        // Intentionally omit /packages/mylib/package.json to mirror server runs
        // where only a subset of project files is loaded into the in-memory snapshot.
        project.set_file(
            "/home/src/workspaces/project/packages/mylib/index.ts".to_string(),
            "export class MyClass {}".to_string(),
        );

        let specifiers = project.auto_import_module_specifiers_from_files(
            "/home/src/workspaces/project/src/index.ts",
            "/home/src/workspaces/project/packages/mylib/index.ts",
        );

        assert_eq!(
            specifiers.first().map(String::as_str),
            Some("mylib"),
            "expected file-linked dependency alias to survive without target package.json, got {specifiers:?}"
        );
    }

    #[test]
    fn workspace_file_dependency_alias_works_without_requesting_package_manifest_loaded() {
        let mut project = Project::new();
        project.set_import_module_specifier_preference(Some("project-relative".to_string()));
        // Intentionally omit /project/package.json to mirror adapter snapshots
        // where only source files and some config files are opened.
        project.set_file(
            "/home/src/workspaces/project/src/index.ts".to_string(),
            "const value = MyClass".to_string(),
        );
        project.set_file(
            "/home/src/workspaces/project/packages/mylib/package.json".to_string(),
            r#"{
  "name": "mylib",
  "version": "1.0.0",
  "main": "index.js",
  "types": "index"
}"#
            .to_string(),
        );
        project.set_file(
            "/home/src/workspaces/project/packages/mylib/index.ts".to_string(),
            "export class MyClass {}".to_string(),
        );

        let specifiers = project.auto_import_module_specifiers_from_files(
            "/home/src/workspaces/project/src/index.ts",
            "/home/src/workspaces/project/packages/mylib/index.ts",
        );

        assert_eq!(
            specifiers.first().map(String::as_str),
            Some("mylib"),
            "expected target package name fallback to avoid deep relative import, got {specifiers:?}"
        );
    }

    #[test]
    fn workspace_package_path_fallback_avoids_deep_relative_when_manifests_are_missing() {
        let mut project = Project::new();
        project.set_import_module_specifier_preference(Some("project-relative".to_string()));
        project.set_file(
            "/home/src/workspaces/project/src/index.ts".to_string(),
            "const value = MyClass".to_string(),
        );
        project.set_file(
            "/home/src/workspaces/project/packages/mylib/index.ts".to_string(),
            "export class MyClass {}".to_string(),
        );

        let specifiers = project.auto_import_module_specifiers_from_files(
            "/home/src/workspaces/project/src/index.ts",
            "/home/src/workspaces/project/packages/mylib/index.ts",
        );

        assert_eq!(
            specifiers.first().map(String::as_str),
            Some("mylib"),
            "expected /packages path fallback to prefer inferred package specifier, got {specifiers:?}"
        );
    }

    #[test]
    fn workspace_dependency_respects_package_exports_visibility() {
        let mut project = Project::new();
        project.set_file(
            "/repo/packages/pack/package.json".to_string(),
            r#"{
  "name": "pack",
  "version": "1.0.0",
  "exports": {
    ".": "./dist/main.mjs"
  }
}"#
            .to_string(),
        );
        project.set_file(
            "/repo/packages/pack/tsconfig.json".to_string(),
            r#"{
  "compilerOptions": {
    "lib": ["es5"],
    "composite": true,
    "module": "nodenext",
    "rootDir": "src",
    "outDir": "dist"
  },
  "include": ["src"]
}"#
            .to_string(),
        );
        project.set_file(
            "/repo/packages/pack/src/unreachable.ts".to_string(),
            "export const fromUnreachable = 0;".to_string(),
        );
        project.set_file(
            "/repo/packages/app/package.json".to_string(),
            r#"{
  "name": "app",
  "dependencies": {
    "pack": "file:../pack"
  }
}"#
            .to_string(),
        );
        project.set_file(
            "/repo/packages/app/src/index.ts".to_string(),
            "x".to_string(),
        );

        let specifiers = project.auto_import_module_specifiers_from_files(
            "/repo/packages/app/src/index.ts",
            "/repo/packages/pack/src/unreachable.ts",
        );
        assert!(
            !specifiers.iter().any(|specifier| specifier == "pack"),
            "expected hidden exports target to avoid pack bare specifier, got {specifiers:?}"
        );
    }

    #[test]
    fn package_specifier_uses_package_name_from_store_layout_package_json() {
        let mut project = Project::new();
        project.set_file(
            "/home/src/workspaces/project/node_modules/.store/@remix-run-server-runtime-virtual-c72daf0d/package/package.json".to_string(),
            r#"{
  "name": "@remix-run/server-runtime",
  "version": "0.0.0",
  "main": "index.js"
}"#
            .to_string(),
        );
        project.set_file(
            "/home/src/workspaces/project/node_modules/.store/@remix-run-server-runtime-virtual-c72daf0d/package/index.d.ts".to_string(),
            "export declare function ServerRuntimeMetaFunction(): void;".to_string(),
        );

        assert_eq!(
            project.package_specifier_from_node_modules(
                "/home/src/workspaces/project/node_modules/.store/@remix-run-server-runtime-virtual-c72daf0d/package/index.d.ts"
            ),
            Some("@remix-run/server-runtime".to_string())
        );
    }

    #[test]
    fn package_specifier_uses_nested_pnpm_node_modules_package_name() {
        let mut project = Project::new();
        project.set_file(
            "/repo/node_modules/.pnpm/@scope+pkg@1.0.0/node_modules/@scope/pkg/package.json"
                .to_string(),
            r#"{
  "name": "@scope/pkg",
  "version": "1.0.0"
}"#
            .to_string(),
        );
        project.set_file(
            "/repo/node_modules/.pnpm/@scope+pkg@1.0.0/node_modules/@scope/pkg/sub/path/file.d.ts"
                .to_string(),
            "export declare const value: number;".to_string(),
        );

        assert_eq!(
            project.package_specifier_from_node_modules(
                "/repo/node_modules/.pnpm/@scope+pkg@1.0.0/node_modules/@scope/pkg/sub/path/file.d.ts"
            ),
            Some("@scope/pkg/sub/path/file".to_string())
        );
    }

    #[test]
    fn node10_module_resolution_does_not_use_exports_subpath_aliases() {
        let mut project = Project::new();
        project.set_file(
            "/home/src/workspaces/project/tsconfig.json".to_string(),
            r#"{
  "compilerOptions": {
    "module": "commonjs",
    "moduleResolution": "node10",
    "lib": ["es5"]
  }
}"#
            .to_string(),
        );
        project.set_file(
            "/home/src/workspaces/project/package.json".to_string(),
            r#"{
  "dependencies": {
    "dependency": "^1.0.0"
  }
}"#
            .to_string(),
        );
        project.set_file(
            "/home/src/workspaces/project/node_modules/dependency/package.json".to_string(),
            r#"{
  "name": "dependency",
  "types": "./lib/index.d.ts",
  "exports": {
    ".": { "types": "./lib/index.d.ts" },
    "./lol": { "types": "./lib/lol.d.ts" }
  }
}"#
            .to_string(),
        );
        project.set_file(
            "/home/src/workspaces/project/node_modules/dependency/lib/index.d.ts".to_string(),
            "export declare function fooFromIndex(): void;".to_string(),
        );
        project.set_file(
            "/home/src/workspaces/project/node_modules/dependency/lib/lol.d.ts".to_string(),
            "export declare function fooFromLol(): void;".to_string(),
        );
        project.set_file(
            "/home/src/workspaces/project/src/foo.ts".to_string(),
            "fooFrom".to_string(),
        );

        let root_specs = project.auto_import_module_specifiers_from_files(
            "/home/src/workspaces/project/src/foo.ts",
            "/home/src/workspaces/project/node_modules/dependency/lib/index.d.ts",
        );
        assert!(
            root_specs.iter().any(|specifier| specifier == "dependency"),
            "expected dependency root specifier under node10 moduleResolution, got {root_specs:?}"
        );

        let subpath_specs = project.auto_import_module_specifiers_from_files(
            "/home/src/workspaces/project/src/foo.ts",
            "/home/src/workspaces/project/node_modules/dependency/lib/lol.d.ts",
        );
        assert!(
            !subpath_specs
                .iter()
                .any(|specifier| specifier == "dependency/lol"),
            "expected node10 moduleResolution to avoid exports subpath alias dependency/lol, got {subpath_specs:?}"
        );
    }

    #[test]
    fn exports_import_and_require_conditions_follow_importer_extension() {
        let mut project = Project::new();
        project.set_file(
            "/home/src/workspaces/project/tsconfig.json".to_string(),
            r#"{
  "compilerOptions": {
    "module": "nodenext",
    "lib": ["es5"]
  }
}"#
            .to_string(),
        );
        project.set_file(
            "/home/src/workspaces/project/package.json".to_string(),
            r#"{
  "dependencies": {
    "dependency": "^1.0.0"
  }
}"#
            .to_string(),
        );
        project.set_file(
            "/home/src/workspaces/project/node_modules/dependency/package.json".to_string(),
            r#"{
  "name": "dependency",
  "exports": {
    "./lol": {
      "import": "./lib/index.js",
      "require": "./lib/lol.js"
    }
  }
}"#
            .to_string(),
        );
        project.set_file(
            "/home/src/workspaces/project/node_modules/dependency/lib/index.d.ts".to_string(),
            "export declare function fooFromIndex(): void;".to_string(),
        );
        project.set_file(
            "/home/src/workspaces/project/node_modules/dependency/lib/lol.d.ts".to_string(),
            "export declare function fooFromLol(): void;".to_string(),
        );
        project.set_file(
            "/home/src/workspaces/project/src/foo.cts".to_string(),
            "fooFrom".to_string(),
        );
        project.set_file(
            "/home/src/workspaces/project/src/foo.mts".to_string(),
            "fooFrom".to_string(),
        );

        let cts_specs = project.auto_import_module_specifiers_from_files(
            "/home/src/workspaces/project/src/foo.cts",
            "/home/src/workspaces/project/node_modules/dependency/lib/lol.d.ts",
        );
        assert!(
            cts_specs
                .iter()
                .any(|specifier| specifier == "dependency/lol"),
            "expected .cts importer to follow require branch for dependency/lol, got {cts_specs:?}"
        );

        let mts_specs = project.auto_import_module_specifiers_from_files(
            "/home/src/workspaces/project/src/foo.mts",
            "/home/src/workspaces/project/node_modules/dependency/lib/index.d.ts",
        );
        assert!(
            mts_specs
                .iter()
                .any(|specifier| specifier == "dependency/lol"),
            "expected .mts importer to follow import branch for dependency/lol, got {mts_specs:?}"
        );
    }

    #[test]
    fn root_dirs_prefers_shortest_relative_specifier_across_roots() {
        let mut project = Project::new();
        project.set_file(
            "/tsconfig.json".to_string(),
            r#"{
  "compilerOptions": {
    "module": "commonjs",
    "rootDirs": [".", "./some/other/root"]
  }
}"#
            .to_string(),
        );

        assert_eq!(
            project
                .root_dirs_relative_specifier_from_files("/index.ts", "/some/other/root/types.ts"),
            Some("./types".to_string())
        );

        assert_eq!(
            project
                .auto_import_module_specifiers_from_files("/index.ts", "/some/other/root/types.ts"),
            vec!["./types".to_string(), "./some/other/root/types".to_string()]
        );
    }

    #[test]
    fn path_mapping_collapses_index_suffix_for_barrel_target() {
        let mut project = Project::new();
        project.set_file(
            "/tsconfig.json".to_string(),
            r#"{
  "compilerOptions": {
    "module": "commonjs",
    "paths": {
      "~/*": ["src/*"]
    }
  }
}"#
            .to_string(),
        );
        project.set_file("/src/dirA/thing1A.ts".to_string(), "Thing".to_string());
        project.set_file(
            "/src/dirB/index.ts".to_string(),
            "export * from \"./thing1B\";".to_string(),
        );

        assert_eq!(
            project
                .path_mapping_specifiers_from_files("/src/dirA/thing1A.ts", "/src/dirB/index.ts"),
            vec!["~/dirB".to_string()]
        );
    }

    #[test]
    fn path_mapping_uses_referenced_project_outdir_when_composite_rootdir_is_implicit() {
        let mut project = Project::new();
        project.set_import_module_specifier_preference(Some("non-relative".to_string()));
        project.set_file(
            "/home/src/workspaces/project/common/tsconfig.json".to_string(),
            r#"{
  "compilerOptions": {
    "lib": ["es5"],
    "module": "commonjs",
    "outDir": "dist",
    "composite": true
  },
  "include": ["src"]
}"#
            .to_string(),
        );
        project.set_file(
            "/home/src/workspaces/project/common/src/MyModule.ts".to_string(),
            "export function square(n: number) { return n * n; }".to_string(),
        );
        project.set_file(
            "/home/src/workspaces/project/web/tsconfig.json".to_string(),
            r#"{
  "compilerOptions": {
    "lib": ["es5"],
    "module": "esnext",
    "moduleResolution": "node",
    "noEmit": true,
    "paths": {
      "@common/*": ["../common/dist/src/*"]
    }
  },
  "include": ["src"],
  "references": [{ "path": "../common" }]
}"#
            .to_string(),
        );
        project.set_file(
            "/home/src/workspaces/project/web/src/Helper.ts".to_string(),
            "square(2);".to_string(),
        );

        let specifiers = project.auto_import_module_specifiers_from_files(
            "/home/src/workspaces/project/web/src/Helper.ts",
            "/home/src/workspaces/project/common/src/MyModule.ts",
        );
        assert!(
            specifiers.contains(&"@common/MyModule".to_string()),
            "expected @common/MyModule to be generated from dist/src path mapping, got {specifiers:?}"
        );
    }

    #[test]
    fn path_mapping_uses_outdir_source_alternatives_for_cross_project_subpaths() {
        let mut project = Project::new();
        project.set_file(
            "/home/src/workspaces/project/packages/app/tsconfig.json".to_string(),
            r#"{
  "compilerOptions": {
    "lib": ["es5"],
    "module": "commonjs",
    "outDir": "dist",
    "rootDir": "src",
    "baseUrl": ".",
    "paths": {
      "dep": ["../dep/src/main"],
      "dep/dist/*": ["../dep/src/*"]
    }
  },
  "references": [{ "path": "../dep" }]
}"#
            .to_string(),
        );
        project.set_file(
            "/home/src/workspaces/project/packages/app/src/utils.ts".to_string(),
            "dep2;".to_string(),
        );
        project.set_file(
            "/home/src/workspaces/project/packages/dep/tsconfig.json".to_string(),
            r#"{
  "compilerOptions": { "lib": ["es5"], "outDir": "dist", "rootDir": "src", "module": "commonjs" }
}"#
            .to_string(),
        );
        project.set_file(
            "/home/src/workspaces/project/packages/dep/src/sub/folder/index.ts".to_string(),
            "export const dep2 = 0;".to_string(),
        );

        let specifiers = project.auto_import_module_specifiers_from_files(
            "/home/src/workspaces/project/packages/app/src/utils.ts",
            "/home/src/workspaces/project/packages/dep/src/sub/folder/index.ts",
        );
        assert!(
            specifiers.contains(&"dep/dist/sub/folder".to_string()),
            "expected dep/dist/sub/folder path-mapped specifier, got {specifiers:?}"
        );
    }

    #[test]
    fn package_imports_from_outdir_mapping_prefer_js_even_with_allow_ts_extensions() {
        let specs = package_import_specifiers_for_target(
            r##"{
  "type": "module",
  "imports": {
    "#*": {
      "types": "./types/*",
      "default": "./dist/*"
    }
  }
}"##,
            "/",
            "/src/add.ts",
            true,
            &["/dist/add".to_string(), "/types/add".to_string()],
        );

        assert_eq!(specs, vec!["#add.js".to_string()]);
    }

    #[test]
    fn package_imports_without_allow_ts_extensions_emit_js_specifiers() {
        let specs = package_import_specifiers_for_target(
            r##"{
  "type": "module",
  "imports": {
    "#internal/*": "./dist/internal/*"
  }
}"##,
            "/home/src/workspaces/project",
            "/home/src/workspaces/project/src/internal/foo.ts",
            false,
            &["/home/src/workspaces/project/dist/internal/foo".to_string()],
        );

        assert_eq!(specs, vec!["#internal/foo.js".to_string()]);
    }

    #[test]
    fn package_imports_with_trailing_slash_mapping_emit_subpath_js_specifiers() {
        let specs = package_import_specifiers_for_target(
            r##"{
  "type": "module",
  "imports": {
    "#internal/": "./dist/internal/"
  }
}"##,
            "/home/src/workspaces/project",
            "/home/src/workspaces/project/src/internal/foo.ts",
            false,
            &["/home/src/workspaces/project/dist/internal/foo".to_string()],
        );

        assert_eq!(specs, vec!["#internal/foo.js".to_string()]);
    }

    #[test]
    fn jsconfig_paths_mapping_outranks_relative_for_shortest_preference() {
        let mut project = Project::new();
        project.set_file(
            "/package1/jsconfig.json".to_string(),
            r#"{
  "compilerOptions": {
    "checkJs": true,
    "paths": {
      "package1/*": ["./*"],
      "package2/*": ["../package2/*"]
    },
    "baseUrl": "."
  }
}"#
            .to_string(),
        );
        project.set_file("/package1/file1.js".to_string(), "bar".to_string());
        project.set_file(
            "/package2/file1.js".to_string(),
            "export const bar = 0;".to_string(),
        );

        assert_eq!(
            project.auto_import_module_specifiers_from_files(
                "/package1/file1.js",
                "/package2/file1.js"
            ),
            vec![
                "package2/file1".to_string(),
                "../package2/file1.js".to_string()
            ]
        );
    }

    #[test]
    fn jsconfig_jsonc_unquoted_keys_are_supported_for_paths_mapping() {
        let mut project = Project::new();
        project.set_file(
            "/package1/jsconfig.json".to_string(),
            r#"{
  "compilerOptions": {
    checkJs: true,
    "paths": {
      "package1/*": ["./*"],
      "package2/*": ["../package2/*"]
    },
    "baseUrl": "."
  }
}"#
            .to_string(),
        );
        project.set_file("/package1/file1.js".to_string(), "bar".to_string());
        project.set_file(
            "/package2/file1.js".to_string(),
            "export const bar = 0;".to_string(),
        );

        assert_eq!(
            project.auto_import_module_specifiers_from_files(
                "/package1/file1.js",
                "/package2/file1.js"
            ),
            vec![
                "package2/file1".to_string(),
                "../package2/file1.js".to_string()
            ]
        );
    }

    #[test]
    fn shortest_prefers_relative_over_paths_when_depth_matches() {
        let mut project = Project::new();
        project.set_file(
            "/tsconfig.json".to_string(),
            r#"{
  "compilerOptions": {
    "module": "preserve",
    "paths": {
      "@app/*": ["./src/*"]
    }
  }
}"#
            .to_string(),
        );
        project.set_file(
            "/src/utils.ts".to_string(),
            "export function add(a: number, b: number) {}".to_string(),
        );
        project.set_file("/src/index.ts".to_string(), "ad".to_string());

        assert_eq!(
            project.auto_import_module_specifiers_from_files("/src/index.ts", "/src/utils.ts"),
            vec!["./utils".to_string(), "@app/utils".to_string()]
        );
    }

    #[test]
    fn shortest_keeps_path_mapping_ahead_of_parent_relative_specifier() {
        let mut project = Project::new();
        project.set_file(
            "/tsconfig.json".to_string(),
            r#"{
  "compilerOptions": {
    "paths": {
      "@root/*": ["${configDir}/src/*"]
    }
  }
}"#
            .to_string(),
        );
        project.set_file(
            "/src/one.ts".to_string(),
            "export const one = 1;".to_string(),
        );
        project.set_file("/src/foo/two.ts".to_string(), "one".to_string());

        assert_eq!(
            project.auto_import_module_specifiers_from_files("/src/foo/two.ts", "/src/one.ts"),
            vec!["@root/one".to_string(), "../one".to_string()]
        );
    }

    #[test]
    fn node_modules_paths_mapping_beats_package_specifier_for_shortest() {
        let mut project = Project::new();
        project.set_file(
            "/tsconfig.json".to_string(),
            r#"{
  "compilerOptions": {
    "module": "amd",
    "moduleResolution": "node",
    "rootDir": "ts",
    "baseUrl": ".",
    "paths": {
      "*": ["node_modules/@woltlab/wcf/ts/*"]
    }
  }
}"#
            .to_string(),
        );
        project.set_file(
            "/node_modules/@woltlab/wcf/ts/WoltLabSuite/Core/Component/Dialog.ts".to_string(),
            "export class Dialog {}".to_string(),
        );
        project.set_file("/ts/main.ts".to_string(), "Dialog".to_string());

        assert_eq!(
            project.auto_import_module_specifiers_from_files(
                "/ts/main.ts",
                "/node_modules/@woltlab/wcf/ts/WoltLabSuite/Core/Component/Dialog.ts"
            ),
            vec![
                "WoltLabSuite/Core/Component/Dialog".to_string(),
                "@woltlab/wcf/ts/WoltLabSuite/Core/Component/Dialog".to_string()
            ]
        );
    }

    #[test]
    fn auto_imports_disabled_for_module_none_es5() {
        let mut project = Project::new();
        project.set_file(
            "/tsconfig.json".to_string(),
            r#"{
  "compilerOptions": {
    "module": "none",
    "target": "es5"
  }
}"#
            .to_string(),
        );

        assert!(!project.auto_imports_allowed_for_file("/index.ts"));
    }

    #[test]
    fn auto_imports_enabled_for_module_none_es2015() {
        let mut project = Project::new();
        project.set_file(
            "/tsconfig.json".to_string(),
            r#"{
  "compilerOptions": {
    "module": "none",
    "target": "es2015"
  }
}"#
            .to_string(),
        );

        assert!(project.auto_imports_allowed_for_file("/index.ts"));
    }

    #[test]
    fn auto_imports_disabled_from_fourslash_directives_for_module_none_es5() {
        let mut project = Project::new();
        project.set_file(
            "/index.ts".to_string(),
            "// @module: none\n// @target: es5\nx".to_string(),
        );

        assert!(!project.auto_imports_allowed_for_file("/index.ts"));
    }

    #[test]
    fn auto_imports_enabled_from_fourslash_directives_for_module_none_es2015() {
        let mut project = Project::new();
        project.set_file(
            "/index.ts".to_string(),
            "// @module: none\n// @target: es2015\nx".to_string(),
        );

        assert!(project.auto_imports_allowed_for_file("/index.ts"));
    }

    #[test]
    fn auto_imports_disabled_from_fourslash_directives_in_sibling_file() {
        let mut project = Project::new();
        project.set_file(
            "/fourslash.ts".to_string(),
            "// @module: none\n// @target: es5\n".to_string(),
        );
        project.set_file("/index.ts".to_string(), "x".to_string());

        assert!(!project.auto_imports_allowed_for_file("/index.ts"));
    }

    #[test]
    fn mts_auto_import_sources_stay_extensionless_even_with_js_imports() {
        let mut project = Project::new();
        project.set_file(
            "/mod.ts".to_string(),
            "export interface I {}\nexport class C {}\n".to_string(),
        );
        project.set_file(
            "/a.mts".to_string(),
            "import type { I } from \"./mod.js\";\nconst x: I = new C();\n".to_string(),
        );

        let specifiers = project.auto_import_module_specifiers_from_files("/a.mts", "/mod.ts");
        assert_eq!(specifiers, vec!["./mod".to_string()]);
    }

    #[test]
    fn node_modules_types_entry_uses_package_root_for_declaration_target() {
        let mut project = Project::new();
        project.set_file(
            "/home/src/workspaces/project/package.json".to_string(),
            r#"{
  "dependencies": {
    "@angular/forms": "*"
  }
}"#
            .to_string(),
        );
        project.set_file(
            "/home/src/workspaces/project/node_modules/@angular/forms/package.json".to_string(),
            r#"{
  "name": "@angular/forms",
  "typings": "./forms.d.ts"
}"#
            .to_string(),
        );
        project.set_file(
            "/home/src/workspaces/project/index.ts".to_string(),
            "PatternValidator".to_string(),
        );
        project.set_file(
            "/home/src/workspaces/project/node_modules/@angular/forms/forms.d.ts".to_string(),
            "export class PatternValidator {}\n".to_string(),
        );

        let specifiers = project.auto_import_module_specifiers_from_files(
            "/home/src/workspaces/project/index.ts",
            "/home/src/workspaces/project/node_modules/@angular/forms/forms.d.ts",
        );

        assert!(
            specifiers
                .first()
                .is_some_and(|specifier| specifier == "@angular/forms"),
            "expected @angular/forms to be preferred for typings entrypoint declarations, got {specifiers:?}"
        );
    }

    #[test]
    fn pnpm_store_package_without_name_uses_linked_dependency_alias() {
        let mut project = Project::new();
        project.set_file(
            "/home/src/workspaces/project/package.json".to_string(),
            r#"{
  "dependencies": {
    "mobx": "*"
  }
}"#
            .to_string(),
        );
        project.set_file(
            "/home/src/workspaces/project/index.ts".to_string(),
            "autorun".to_string(),
        );
        project.set_file(
            "/home/src/workspaces/project/node_modules/.pnpm/mobx@6.0.4/node_modules/mobx/package.json"
                .to_string(),
            r#"{
  "types": "dist/mobx.d.ts"
}"#
            .to_string(),
        );
        project.set_file(
            "/home/src/workspaces/project/node_modules/.pnpm/mobx@6.0.4/node_modules/mobx/dist/mobx.d.ts"
                .to_string(),
            "export declare function autorun(): void;\n".to_string(),
        );

        let specifiers = project.auto_import_module_specifiers_from_files(
            "/home/src/workspaces/project/index.ts",
            "/home/src/workspaces/project/node_modules/.pnpm/mobx@6.0.4/node_modules/mobx/dist/mobx.d.ts",
        );

        assert!(
            specifiers.iter().any(|specifier| specifier == "mobx"),
            "expected pnpm store package target to resolve to dependency alias `mobx`, got {specifiers:?}"
        );
    }
}

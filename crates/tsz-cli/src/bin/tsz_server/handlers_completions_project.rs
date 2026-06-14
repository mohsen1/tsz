//! Project-wide completion item collection helpers for tsz-server completions.

use super::{CompletionProjectCache, CompletionProjectCacheKey, Server};
use rustc_hash::{FxHashMap, FxHashSet};
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use tsz::lsp::Project;

impl Server {
    pub(super) fn project_completion_items(
        &mut self,
        file_name: &str,
        position: tsz::lsp::position::Position,
        preferences: Option<&serde_json::Value>,
    ) -> Vec<tsz::lsp::completions::CompletionItem> {
        let include_module_exports =
            Self::bool_pref_or_default(preferences, "includeCompletionsForModuleExports", false);
        let mut tracked_paths = FxHashSet::default();
        tracked_paths.extend(self.open_files.keys().cloned());
        for project_files in self.external_project_files.values() {
            tracked_paths.extend(project_files.iter().cloned());
        }
        tracked_paths.insert(file_name.to_string());

        let allowed_packages =
            include_module_exports.then(|| self.dependency_package_names_for_file(file_name));
        let workspace_prefix = Self::path_workspace_prefix(file_name);
        let mut files = FxHashMap::default();
        for path in tracked_paths {
            let allowed_packages_ref = allowed_packages
                .as_ref()
                .and_then(std::option::Option::as_ref);
            if !Self::should_include_completion_project_path(
                &path,
                file_name,
                workspace_prefix.as_deref(),
                allowed_packages_ref,
            ) {
                continue;
            }
            if let Some(text) = self
                .open_files
                .get(&path)
                .cloned()
                .or_else(|| std::fs::read_to_string(&path).ok())
            {
                files.insert(path, text);
            }
        }

        if !files.contains_key(file_name)
            && let Ok(content) = std::fs::read_to_string(file_name)
        {
            files.insert(file_name.to_string(), content);
        }
        Self::add_project_config_files(&mut files, file_name);
        if include_module_exports {
            let has_node_modules_file = files
                .keys()
                .any(|path| Self::path_is_under_node_modules(path));
            if !has_node_modules_file {
                self.add_dependency_package_files_for_completion(
                    file_name,
                    allowed_packages
                        .as_ref()
                        .and_then(std::option::Option::as_ref),
                    &mut files,
                );
            }
        }
        if files.is_empty() {
            return Vec::new();
        }

        let import_module_specifier_ending =
            Self::string_pref(preferences, "importModuleSpecifierEnding")
                .or_else(|| self.completion_import_module_specifier_ending.clone());
        let import_module_specifier_preference =
            Self::string_pref(preferences, "importModuleSpecifierPreference")
                .or_else(|| self.import_module_specifier_preference.clone());
        let auto_import_file_exclude_patterns =
            Self::string_array_pref(preferences, "autoImportFileExcludePatterns")
                .unwrap_or_else(|| self.auto_import_file_exclude_patterns.clone());
        let auto_import_specifier_exclude_regexes =
            Self::string_array_pref(preferences, "autoImportSpecifierExcludeRegexes")
                .unwrap_or_default();
        let key = CompletionProjectCacheKey {
            files: Self::completion_project_file_fingerprints(&files),
            allow_importing_ts_extensions: self.allow_importing_ts_extensions,
            auto_imports_allowed_without_tsconfig: self.auto_imports_allowed_for_inferred_projects,
            import_module_specifier_ending: import_module_specifier_ending.clone(),
            import_module_specifier_preference: import_module_specifier_preference.clone(),
            auto_import_file_exclude_patterns: auto_import_file_exclude_patterns.clone(),
            auto_import_specifier_exclude_regexes: auto_import_specifier_exclude_regexes.clone(),
        };

        let cache_matches = self
            .completion_project_cache
            .as_ref()
            .is_some_and(|cache| cache.key == key);
        if !cache_matches {
            let mut project = Project::new();
            project.set_allow_importing_ts_extensions(self.allow_importing_ts_extensions);
            project.set_auto_imports_allowed_without_tsconfig(
                self.auto_imports_allowed_for_inferred_projects,
            );
            project.set_import_module_specifier_ending(import_module_specifier_ending);
            project.set_import_module_specifier_preference(import_module_specifier_preference);
            project.set_auto_import_file_exclude_patterns(auto_import_file_exclude_patterns);
            project
                .set_auto_import_specifier_exclude_regexes(auto_import_specifier_exclude_regexes);
            for (path, text) in files {
                project.set_file(path, text);
            }
            self.completion_project_cache = Some(CompletionProjectCache { key, project });
        }

        self.completion_project_cache
            .as_mut()
            .and_then(|cache| cache.project.get_completions(file_name, position))
            .unwrap_or_default()
    }

    pub(super) fn completion_project_file_fingerprints(
        files: &FxHashMap<String, String>,
    ) -> Vec<(String, u64)> {
        let mut fingerprints: Vec<_> = files
            .iter()
            .map(|(path, text)| (path.clone(), Self::hash_completion_project_text(text)))
            .collect();
        fingerprints.sort_by(|a, b| a.0.cmp(&b.0));
        fingerprints
    }

    fn hash_completion_project_text(text: &str) -> u64 {
        let mut hasher = DefaultHasher::new();
        text.hash(&mut hasher);
        hasher.finish()
    }

    pub(super) fn dependency_package_names_for_file(
        &self,
        file_name: &str,
    ) -> Option<FxHashSet<String>> {
        let mut allowed = FxHashSet::default();
        let mut saw_package_json = false;
        let mut current = Path::new(file_name).parent();

        while let Some(dir) = current {
            let package_json_path = dir.join("package.json");
            let package_json_key = package_json_path.to_string_lossy().replace('\\', "/");
            let package_json_text = self.open_files.get(&package_json_key).cloned();

            if let Some(text) = package_json_text {
                saw_package_json = true;
                let Ok(json) = serde_json::from_str::<serde_json::Value>(&text) else {
                    // Match tsserver behavior: invalid package.json should not
                    // suppress auto-import candidates.
                    return None;
                };
                for field in [
                    "dependencies",
                    "devDependencies",
                    "peerDependencies",
                    "optionalDependencies",
                ] {
                    if let Some(deps) = json.get(field).and_then(serde_json::Value::as_object) {
                        allowed.extend(deps.keys().cloned());
                    }
                }
            }
            current = dir.parent();
        }

        saw_package_json.then_some(allowed)
    }

    pub(super) fn should_include_completion_project_path(
        path: &str,
        current_file: &str,
        workspace_prefix: Option<&str>,
        allowed_packages: Option<&FxHashSet<String>>,
    ) -> bool {
        let path = path.replace('\\', "/");
        let current_file = current_file.replace('\\', "/");
        if path == current_file {
            return true;
        }

        if Self::path_is_under_node_modules(&path) {
            return Self::node_modules_path_matches_allowed_packages(&path, allowed_packages);
        }

        if Self::is_project_config_file(&path) {
            // Always include package.json files outside node_modules: they
            // carry workspace-package metadata (`name`, `exports`, …) that
            // the auto-import specifier resolver needs even for sibling
            // packages that aren't ancestors of the currently-edited file.
            // `tsconfig.json` / `jsconfig.json` stay gated by ancestry.
            if path.ends_with("/package.json") {
                return true;
            }
            return Self::is_config_related_to_file(&path, &current_file);
        }

        workspace_prefix
            .map(|prefix| {
                if prefix == "/" {
                    // Root workspace: include any absolute path that is not under
                    // node_modules (handled above). Using `format!("{prefix}/")`
                    // would yield "//", which never matches real paths like
                    // "/Component.tsx".
                    path.starts_with('/')
                } else {
                    path == prefix || path.starts_with(&format!("{prefix}/"))
                }
            })
            .unwrap_or(true)
    }

    pub(super) fn is_project_config_file(path: &str) -> bool {
        path.ends_with("/package.json")
            || path.ends_with("/tsconfig.json")
            || path.ends_with("/jsconfig.json")
    }

    pub(super) fn is_config_related_to_file(config_path: &str, file_name: &str) -> bool {
        let Some(dir) = Path::new(config_path).parent() else {
            return false;
        };
        let dir = dir.to_string_lossy().replace('\\', "/");
        if dir == "/" {
            return file_name.starts_with('/');
        }
        file_name == dir || file_name.starts_with(&format!("{dir}/"))
    }

    pub(super) fn path_is_under_node_modules(path: &str) -> bool {
        path.contains("/node_modules/")
    }

    pub(super) fn path_workspace_prefix(file_name: &str) -> Option<String> {
        let normalized = file_name.replace('\\', "/");
        if normalized.starts_with('/') && normalized.matches('/').count() == 1 {
            return Some("/".to_string());
        }
        let segments: Vec<&str> = normalized
            .split('/')
            .filter(|segment| !segment.is_empty())
            .collect();
        if segments.is_empty() {
            return None;
        }
        if segments.len() == 1 {
            return normalized.starts_with('/').then_some("/".to_string());
        }
        if segments.len() <= 3 {
            return Some(format!("/{}", segments[0]));
        }
        Some(format!("/{}/{}/{}", segments[0], segments[1], segments[2]))
    }

    pub(super) fn node_modules_path_matches_allowed_packages(
        path: &str,
        allowed_packages: Option<&FxHashSet<String>>,
    ) -> bool {
        let Some(allowed_packages) = allowed_packages else {
            return true;
        };
        if allowed_packages.is_empty() {
            return true;
        }
        let Some(package_name) = Self::package_name_from_node_modules_path(path) else {
            return false;
        };
        if allowed_packages.contains(&package_name) {
            return true;
        }
        Self::types_package_runtime_name(&package_name)
            .is_some_and(|runtime_name| allowed_packages.contains(&runtime_name))
    }

    pub(super) fn package_name_from_node_modules_path(path: &str) -> Option<String> {
        let normalized = path.replace('\\', "/");
        let idx = normalized.rfind("/node_modules/")?;
        let mut tail = &normalized[idx + "/node_modules/".len()..];
        if tail.starts_with(".pnpm/")
            && let Some(inner_idx) = tail.find("/node_modules/")
        {
            tail = &tail[inner_idx + "/node_modules/".len()..];
        }
        let mut segments = tail.split('/').filter(|segment| !segment.is_empty());
        let first = segments.next()?;
        if first.starts_with('@') {
            let second = segments.next()?;
            Some(format!("{first}/{second}"))
        } else {
            Some(first.to_string())
        }
    }

    pub(super) fn types_package_runtime_name(package_name: &str) -> Option<String> {
        let rest = package_name.strip_prefix("@types/")?;
        if let Some((scope, name)) = rest.split_once("__") {
            return Some(format!("@{scope}/{name}"));
        }
        Some(rest.to_string())
    }

    pub(super) fn types_package_name_for(runtime_package_name: &str) -> String {
        if let Some(stripped) = runtime_package_name.strip_prefix('@')
            && let Some((scope, name)) = stripped.split_once('/')
        {
            return format!("@types/{scope}__{name}");
        }
        format!("@types/{runtime_package_name}")
    }

    pub(super) fn node_modules_roots_for_file(file_name: &str) -> Vec<String> {
        let mut roots = Vec::new();
        let mut current = Path::new(file_name).parent();
        while let Some(dir) = current {
            roots.push(
                dir.join("node_modules")
                    .to_string_lossy()
                    .replace('\\', "/"),
            );
            current = dir.parent();
        }
        roots.sort();
        roots.dedup();
        roots
    }

    pub(super) fn add_dependency_package_files_for_completion(
        &self,
        file_name: &str,
        allowed_packages: Option<&FxHashSet<String>>,
        files: &mut FxHashMap<String, String>,
    ) {
        let Some(allowed_packages) = allowed_packages else {
            return;
        };
        if allowed_packages.is_empty() {
            return;
        }

        let node_modules_roots = Self::node_modules_roots_for_file(file_name);
        if node_modules_roots.is_empty() {
            return;
        }

        let mut dependency_names: Vec<String> = allowed_packages.iter().cloned().collect();
        dependency_names.sort();
        dependency_names.dedup();

        let mut scanned_dirs = FxHashSet::default();
        for node_modules_root in node_modules_roots {
            for dependency_name in &dependency_names {
                if Self::files_already_include_dependency(files, dependency_name) {
                    continue;
                }
                let dependency_dir = format!("{node_modules_root}/{dependency_name}");
                let dependency_dir_path = Path::new(&dependency_dir);
                if !dependency_dir_path.is_dir() {
                    continue;
                }
                if !scanned_dirs.insert(dependency_dir.clone()) {
                    continue;
                }

                if !Self::files_contain_path_prefix(files, &dependency_dir) {
                    Self::add_supported_files_under_dir(dependency_dir_path, files, 256);
                }

                if Self::files_contain_declaration_under_prefix(files, &dependency_dir) {
                    continue;
                }

                let types_package_name = Self::types_package_name_for(dependency_name);
                let types_dir = format!("{node_modules_root}/{types_package_name}");
                let types_dir_path = Path::new(&types_dir);
                if !types_dir_path.is_dir() {
                    continue;
                }
                if !scanned_dirs.insert(types_dir.clone()) {
                    continue;
                }
                if !Self::files_contain_path_prefix(files, &types_dir) {
                    Self::add_supported_files_under_dir(types_dir_path, files, 256);
                }
            }
        }
    }

    pub(super) fn files_already_include_dependency(
        files: &FxHashMap<String, String>,
        dependency_name: &str,
    ) -> bool {
        files.keys().any(|path| {
            Self::package_name_from_node_modules_path(path).is_some_and(|package_name| {
                package_name == dependency_name
                    || Self::types_package_runtime_name(&package_name)
                        .is_some_and(|runtime| runtime == dependency_name)
            })
        })
    }

    pub(super) fn files_contain_path_prefix(
        files: &FxHashMap<String, String>,
        prefix: &str,
    ) -> bool {
        let normalized_prefix = prefix.replace('\\', "/");
        files.keys().any(|path| {
            path == &normalized_prefix || path.starts_with(&format!("{normalized_prefix}/"))
        })
    }

    pub(super) fn files_contain_declaration_under_prefix(
        files: &FxHashMap<String, String>,
        prefix: &str,
    ) -> bool {
        let normalized_prefix = prefix.replace('\\', "/");
        files.keys().any(|path| {
            (path == &normalized_prefix || path.starts_with(&format!("{normalized_prefix}/")))
                && (path.ends_with(".d.ts") || path.ends_with(".d.mts") || path.ends_with(".d.cts"))
        })
    }

    pub(super) fn add_supported_files_under_dir(
        root: &Path,
        files: &mut FxHashMap<String, String>,
        max_files: usize,
    ) {
        let mut added = 0usize;
        let mut stack = vec![PathBuf::from(root)];
        while let Some(dir) = stack.pop() {
            let Ok(entries) = std::fs::read_dir(&dir) else {
                continue;
            };
            for entry in entries.flatten() {
                let Ok(file_type) = entry.file_type() else {
                    continue;
                };
                let path = entry.path();
                if file_type.is_dir() {
                    let name = entry.file_name();
                    if name.to_string_lossy().as_ref() != "node_modules" {
                        stack.push(path);
                    }
                    continue;
                }
                if !file_type.is_file() {
                    continue;
                }

                let path_str = path.to_string_lossy().replace('\\', "/");
                if files.contains_key(&path_str)
                    || !Self::is_supported_completion_project_file(&path_str)
                {
                    continue;
                }
                if let Ok(text) = std::fs::read_to_string(&path) {
                    files.insert(path_str, text);
                    added += 1;
                    if added >= max_files {
                        return;
                    }
                }
            }
        }
    }

    pub(super) fn is_supported_completion_project_file(path: &str) -> bool {
        path.ends_with(".ts")
            || path.ends_with(".tsx")
            || path.ends_with(".d.ts")
            || path.ends_with(".mts")
            || path.ends_with(".cts")
            || path.ends_with(".d.mts")
            || path.ends_with(".d.cts")
            || path.ends_with(".js")
            || path.ends_with(".jsx")
            || path.ends_with(".mjs")
            || path.ends_with(".cjs")
            || path.ends_with("/package.json")
            || path.ends_with("/tsconfig.json")
            || path.ends_with("/jsconfig.json")
    }
}

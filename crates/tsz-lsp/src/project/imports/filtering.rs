//! Auto-import candidate filtering and exclusion predicates.
//!
//! Determines whether a given module specifier or file path should be offered
//! as an auto-import candidate based on project configuration (exclude patterns,
//! specifier regexes, `package.json` dependency lists, and subpackage manifests).

use std::path::Path;

use rustc_hash::{FxHashMap, FxHashSet};

use super::super::{Project, ProjectFile};
use tsz_parser::parser::node::NodeAccess;
use tsz_parser::syntax_kind_ext;

// ── Source-text cache ────────────────────────────────────────────────────────

/// Cache for bare-specifier source-text lookups so the same package-name
/// string is not rescanned repeatedly across candidates in a single request.
#[derive(Default)]
pub(in super::super) struct BareSpecifierSourceCache {
    pub(in super::super) quoted_literal_match: FxHashMap<String, bool>,
    pub(in super::super) import_like_match: FxHashMap<String, bool>,
}

// ── Exclusion predicates on `Project` ────────────────────────────────────────

impl Project {
    pub(in super::super) fn auto_import_path_is_excluded(&self, path: &str) -> bool {
        if self.auto_import_file_exclude_matchers.is_empty() {
            return false;
        }

        let normalized = path.trim().replace('\\', "/");
        if normalized.is_empty() {
            return false;
        }

        let trimmed = normalized.trim_start_matches('/');
        self.auto_import_file_exclude_matchers
            .iter()
            .any(|matcher| {
                matcher.is_match(&normalized)
                    || (!trimmed.is_empty() && matcher.is_match(trimmed))
                    || normalized
                        .strip_prefix('/')
                        .is_some_and(|stripped| matcher.is_match(stripped))
            })
    }

    fn auto_import_specifier_is_excluded(&self, module_specifier: &str) -> bool {
        self.auto_import_specifier_exclude_matchers
            .iter()
            .any(|matcher| matcher.is_match(module_specifier))
    }

    /// Returns the set of package names allowed for bare-specifier auto-imports
    /// from `from_file`, derived from the nearest `package.json` on the
    /// ancestor path. Returns `None` when no `package.json` is found (meaning
    /// all bare specifiers are allowed).
    pub(in super::super) fn allowed_dependency_package_names(
        &self,
        from_file: &str,
    ) -> Option<FxHashSet<String>> {
        let mut allowed = FxHashSet::default();
        let mut saw_package_json = false;
        let mut current = Path::new(from_file).parent();
        while let Some(dir) = current {
            let package_json_path = dir.join("package.json");
            let package_json_key = package_json_path.to_string_lossy().replace('\\', "/");
            let package_json_text = self
                .files
                .get(&package_json_key)
                .map(|f| f.source_text().to_string());

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

    /// Extracts the bare package name from a module specifier.
    ///
    /// Returns `None` for relative (`.`/`/`), internal (`#`), or empty specifiers.
    /// For scoped packages (`@scope/name/sub`) returns `@scope/name`.
    pub(in super::super) fn module_specifier_package_name(module_specifier: &str) -> Option<&str> {
        if module_specifier.is_empty()
            || module_specifier.starts_with('.')
            || module_specifier.starts_with('/')
            || module_specifier.starts_with('#')
        {
            return None;
        }

        if let Some(scoped) = module_specifier.strip_prefix('@') {
            let mut parts = scoped.split('/');
            let scope = parts.next()?;
            let pkg = parts.next()?;
            if scope.is_empty() || pkg.is_empty() {
                return None;
            }
            let len = 1 + scope.len() + 1 + pkg.len();
            return module_specifier.get(..len);
        }

        module_specifier.split('/').next()
    }

    /// Returns the set of package names that are already imported in `file`.
    pub(in super::super) fn imported_package_names(file: &ProjectFile) -> FxHashSet<String> {
        let arena = file.arena();
        let Some(source_file) = arena.get_source_file_at(file.root()) else {
            return FxHashSet::default();
        };
        let mut imported = FxHashSet::default();

        for &stmt_idx in &source_file.statements.nodes {
            let Some(stmt_node) = arena.get(stmt_idx) else {
                continue;
            };
            let module_specifier = if stmt_node.kind == syntax_kind_ext::IMPORT_DECLARATION {
                arena
                    .get_import_decl(stmt_node)
                    .and_then(|import| arena.get_literal_text(import.module_specifier))
            } else if stmt_node.kind == syntax_kind_ext::EXPORT_DECLARATION {
                arena
                    .get_export_decl(stmt_node)
                    .and_then(|export| export.module_specifier.into_option())
                    .and_then(|specifier| arena.get_literal_text(specifier))
            } else {
                None
            };

            if let Some(package_name) = module_specifier
                .and_then(Self::module_specifier_package_name)
                .map(str::to_string)
            {
                imported.insert(package_name);
            }
        }

        imported
    }

    fn source_contains_quoted_package_literal(source_text: &str, package_name: &str) -> bool {
        source_text.contains(&format!("\"{package_name}\""))
            || source_text.contains(&format!("'{package_name}'"))
    }

    fn source_contains_import_like_package_usage(source_text: &str, package_name: &str) -> bool {
        source_text.contains(&format!("from \"{package_name}\""))
            || source_text.contains(&format!("from '{package_name}'"))
            || source_text.contains(&format!("require(\"{package_name}\")"))
            || source_text.contains(&format!("require('{package_name}')"))
            || source_text.contains(&format!("import(\"{package_name}\")"))
            || source_text.contains(&format!("import('{package_name}')"))
            || source_text.contains(&format!("types=\"{package_name}\""))
            || source_text.contains(&format!("types='{package_name}'"))
    }

    fn bare_specifier_allowed_for_file(
        &self,
        module_specifier: &str,
        from_source_text: &str,
        allowed_packages: Option<&FxHashSet<String>>,
        existing_imported_packages: &FxHashSet<String>,
        source_cache: &mut BareSpecifierSourceCache,
    ) -> bool {
        let Some(package_name) = Self::module_specifier_package_name(module_specifier) else {
            return true;
        };

        let node_prefixed =
            (!package_name.starts_with("node:")).then(|| format!("node:{package_name}"));
        let node_stripped = package_name.strip_prefix("node:");

        let mut cached_quoted_literal_match = |candidate: &str| {
            if let Some(cached) = source_cache.quoted_literal_match.get(candidate) {
                return *cached;
            }
            let matched = Self::source_contains_quoted_package_literal(from_source_text, candidate);
            source_cache
                .quoted_literal_match
                .insert(candidate.to_string(), matched);
            matched
        };
        let mut cached_import_like_match = |candidate: &str| {
            if let Some(cached) = source_cache.import_like_match.get(candidate) {
                return *cached;
            }
            let matched =
                Self::source_contains_import_like_package_usage(from_source_text, candidate);
            source_cache
                .import_like_match
                .insert(candidate.to_string(), matched);
            matched
        };

        let has_existing_import = existing_imported_packages.contains(package_name)
            || node_prefixed
                .as_deref()
                .is_some_and(|candidate| existing_imported_packages.contains(candidate))
            || node_stripped
                .is_some_and(|candidate| existing_imported_packages.contains(candidate));

        // Guard against stale parser snapshots after incremental edits:
        // only trust existing-import evidence when the package literal is
        // still present in the current source text.
        let quoted_in_source = cached_quoted_literal_match(package_name)
            || node_prefixed
                .as_deref()
                .is_some_and(&mut cached_quoted_literal_match)
            || node_stripped.is_some_and(&mut cached_quoted_literal_match);
        if has_existing_import && quoted_in_source {
            return true;
        }

        let import_like_in_source = cached_import_like_match(package_name)
            || node_prefixed
                .as_deref()
                .is_some_and(&mut cached_import_like_match)
            || node_stripped.is_some_and(&mut cached_import_like_match);
        if import_like_in_source {
            return true;
        }

        allowed_packages
            .map(|allowed| allowed.contains(package_name))
            .unwrap_or(true)
    }

    /// Returns `true` when a bare specifier with a subpath (e.g. `preact/hooks`)
    /// has its own `package.json` in any loaded `node_modules` directory. This
    /// lets tsz suggest `preact/hooks` even when the parent `preact` package is
    /// not listed in the project's dependencies — the subpackage is directly
    /// installed and addressable.
    fn specifier_subpackage_has_own_manifest(&self, module_specifier: &str) -> bool {
        // Only applies to non-scoped specifiers with at least one slash.
        // Scoped packages (@scope/name) are handled normally.
        if module_specifier.starts_with('@') || !module_specifier.contains('/') {
            return false;
        }
        let needle = format!("/node_modules/{module_specifier}/package.json");
        self.files.keys().any(|k| k.ends_with(&needle))
    }

    pub(in super::super) fn is_auto_import_candidate_excluded(
        &self,
        target_file: &str,
        module_specifier: &str,
        from_source_text: &str,
        allowed_packages: Option<&FxHashSet<String>>,
        existing_imported_packages: &FxHashSet<String>,
        source_cache: &mut BareSpecifierSourceCache,
    ) -> bool {
        if self.auto_import_specifier_is_excluded(module_specifier) {
            return true;
        }

        if self.auto_import_path_is_excluded(target_file) {
            return true;
        }

        if module_specifier.starts_with('.') {
            return false;
        }

        if !self.bare_specifier_allowed_for_file(
            module_specifier,
            from_source_text,
            allowed_packages,
            existing_imported_packages,
            source_cache,
        ) && !self.specifier_subpackage_has_own_manifest(module_specifier)
        {
            return true;
        }

        if self.auto_import_path_is_excluded(module_specifier) {
            return true;
        }

        let synthetic_node_modules_path = format!("/node_modules/{module_specifier}");
        self.auto_import_path_is_excluded(&synthetic_node_modules_path)
            || self
                .auto_import_path_is_excluded(synthetic_node_modules_path.trim_start_matches('/'))
    }

    pub(in super::super) fn is_ambient_module_candidate_excluded(
        &self,
        module_specifier: &str,
        from_source_text: &str,
        allowed_packages: Option<&FxHashSet<String>>,
        existing_imported_packages: &FxHashSet<String>,
        source_cache: &mut BareSpecifierSourceCache,
    ) -> bool {
        if self.auto_import_specifier_is_excluded(module_specifier) {
            return true;
        }

        if module_specifier.starts_with('.') {
            return false;
        }

        if !self.bare_specifier_allowed_for_file(
            module_specifier,
            from_source_text,
            allowed_packages,
            existing_imported_packages,
            source_cache,
        ) && !self.specifier_subpackage_has_own_manifest(module_specifier)
        {
            return true;
        }

        if self.auto_import_path_is_excluded(module_specifier) {
            return true;
        }

        let synthetic_node_modules_path = format!("/node_modules/{module_specifier}");
        if self.auto_import_path_is_excluded(&synthetic_node_modules_path)
            || self
                .auto_import_path_is_excluded(synthetic_node_modules_path.trim_start_matches('/'))
        {
            return true;
        }

        self.ambient_module_declarations_all_excluded(module_specifier)
    }

    fn ambient_module_declarations_all_excluded(&self, module_specifier: &str) -> bool {
        let mut found_declaration = false;

        for (file_name, file) in &self.files {
            if !Self::file_declares_ambient_module(file, module_specifier) {
                continue;
            }
            found_declaration = true;
            if !self.auto_import_path_is_excluded(file_name) {
                return false;
            }
        }

        found_declaration
    }

    fn file_declares_ambient_module(file: &ProjectFile, module_specifier: &str) -> bool {
        let arena = file.arena();
        let Some(source_file) = arena.get_source_file_at(file.root()) else {
            return false;
        };

        for &stmt_idx in &source_file.statements.nodes {
            let Some(stmt_node) = arena.get(stmt_idx) else {
                continue;
            };
            if stmt_node.kind != syntax_kind_ext::MODULE_DECLARATION {
                continue;
            }
            let Some(module_decl) = arena.get_module(stmt_node) else {
                continue;
            };
            let Some(declared_name) = arena.get_literal_text(module_decl.name) else {
                continue;
            };
            if declared_name == module_specifier {
                return true;
            }
        }

        false
    }
}

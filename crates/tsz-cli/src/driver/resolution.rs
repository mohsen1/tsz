use rustc_hash::FxHashMap;
use std::path::{Component, Path, PathBuf};

// Imports kept in scope so the in-file test module can use them via `super::*`.
#[allow(unused_imports)]
use crate::config::{ModuleResolutionKind, PathMapping, ResolvedCompilerOptions};
use tsz::module_resolver::{ImportKind, ImportingModuleKind, PackageType};
use tsz::parser::NodeIndex;

mod discovery;
mod exports_imports;
mod package_resolution;
mod path_resolution;
mod probe_counts;
mod program_file_index;
mod type_packages;

#[cfg(test)]
pub(crate) use discovery::collect_module_specifiers;
#[allow(unused_imports)]
pub(crate) use discovery::{
    collect_export_binding_nodes, collect_import_bindings, collect_module_requests_from_text,
    collect_module_specifiers_for_check, collect_module_specifiers_from_text,
    collect_star_export_specifiers, json_type_attribute_enables_json_module,
    module_specifier_has_type_json_import_attribute,
};
pub(crate) use path_resolution::{
    build_duplicate_package_redirects, normalize_path, normalize_resolved_path,
    resolve_module_specifier,
};
use probe_counts::*;
pub(crate) use program_file_index::ProgramFileIndex;
pub(crate) use type_packages::{
    collect_type_packages_from_root, default_type_roots, resolve_type_package_entry_with_cache,
    resolve_type_package_entry_with_mode_and_cache, resolve_type_package_from_roots_with_cache,
    resolve_type_reference_from_node_modules_with_cache, type_package_candidates_pub,
};
#[cfg(test)]
pub(crate) use type_packages::{resolve_type_package_entry, resolve_type_package_entry_with_mode};

pub(super) use type_packages::{
    implied_resolution_mode_for_file, implied_resolution_mode_for_file_with_cache,
};

#[allow(unused_imports)]
pub(super) use discovery::*;
#[allow(unused_imports)]
pub(super) use exports_imports::*;
#[allow(unused_imports)]
pub(super) use package_resolution::*;
#[allow(unused_imports)]
pub(super) use path_resolution::*;
#[allow(unused_imports)]
pub(super) use type_packages::*;

type CollectedModuleSpecifier = (String, NodeIndex, ImportKind, Option<ImportingModuleKind>);
type SourceDiscoveryModuleRequest = (String, ImportKind, Option<ImportingModuleKind>, bool);

#[derive(Clone, Copy)]
pub(crate) enum AmbientModuleDeclarationSpecifierPolicy {
    #[cfg(test)]
    All,
    SourceDiscovery,
    Check {
        is_external_module: bool,
    },
}

#[derive(Default)]
pub(crate) struct ModuleResolutionCache {
    package_type_by_dir: FxHashMap<PathBuf, Option<PackageType>>,
    package_json_by_path: FxHashMap<PathBuf, Option<PackageJson>>,
    file_exists_by_path: FxHashMap<PathBuf, bool>,
    node_modules_dir_by_path: FxHashMap<PathBuf, bool>,
    package_root_dir_by_path: FxHashMap<PathBuf, bool>,
    // Per-compiler-options cache. A compile uses one resolved `paths` table, so
    // the specifier alone is enough to memoize the best matching mapping.
    path_mapping_by_specifier: FxHashMap<String, Option<(usize, String)>>,
}

pub(super) fn package_relative_target_path(package_root: &Path, target: &str) -> Option<PathBuf> {
    let rest = target.strip_prefix("./")?;
    let path = Path::new(target);
    if path.components().any(|component| match component {
        Component::ParentDir | Component::RootDir | Component::Prefix(_) => true,
        Component::Normal(segment) => segment == "node_modules",
        _ => false,
    }) {
        return None;
    }
    Some(package_root.join(rest))
}

impl ModuleResolutionCache {
    fn file_exists(&mut self, path: &Path) -> bool {
        if let Some(&exists) = self.file_exists_by_path.get(path) {
            return exists;
        }

        let exists = count_is_file(path);
        self.file_exists_by_path.insert(path.to_path_buf(), exists);
        exists
    }

    fn read_package_json(&mut self, path: &Path) -> Option<PackageJson> {
        if let Some(cached) = self.package_json_by_path.get(path) {
            return cached.clone();
        }

        let parsed = read_package_json_uncached(path);
        self.package_json_by_path
            .insert(path.to_path_buf(), parsed.clone());
        parsed
    }

    fn node_modules_dir_exists(&mut self, path: &Path) -> bool {
        if let Some(&exists) = self.node_modules_dir_by_path.get(path) {
            return exists;
        }

        let exists = count_is_dir(path);
        self.node_modules_dir_by_path
            .insert(path.to_path_buf(), exists);
        exists
    }

    pub(crate) fn package_root_dir_exists(&mut self, path: &Path) -> bool {
        if let Some(&exists) = self.package_root_dir_by_path.get(path) {
            return exists;
        }

        let exists = count_is_dir(path);
        self.package_root_dir_by_path
            .insert(path.to_path_buf(), exists);
        exists
    }

    fn select_path_mapping<'a>(
        &mut self,
        mappings: &'a [PathMapping],
        specifier: &str,
    ) -> Option<(&'a PathMapping, String)> {
        if let Some(cached) = self.path_mapping_by_specifier.get(specifier) {
            return cached.as_ref().and_then(|(idx, wildcard)| {
                mappings
                    .get(*idx)
                    .map(|mapping| (mapping, wildcard.clone()))
            });
        }

        let selected = select_path_mapping(mappings, specifier);
        self.path_mapping_by_specifier
            .insert(specifier.to_string(), selected.clone());
        selected.and_then(|(idx, wildcard)| mappings.get(idx).map(|mapping| (mapping, wildcard)))
    }

    fn package_type_for_dir(&mut self, dir: &Path, base_dir: &Path) -> Option<PackageType> {
        let mut current = dir;
        let mut visited = Vec::new();

        loop {
            if let Some(value) = self.package_type_by_dir.get(current).copied() {
                for path in visited {
                    self.package_type_by_dir.insert(path, value);
                }
                return value;
            }

            visited.push(current.to_path_buf());

            let package_json_path = current.join("package.json");
            if self.file_exists(&package_json_path)
                && let Some(package_json) = self.read_package_json(&package_json_path)
            {
                let value = package_type_from_json(Some(&package_json));
                for path in visited {
                    self.package_type_by_dir.insert(path, value);
                }
                return value;
            }

            if current == base_dir {
                for path in visited {
                    self.package_type_by_dir.insert(path, None);
                }
                return None;
            }

            let Some(parent) = current.parent() else {
                for path in visited {
                    self.package_type_by_dir.insert(path, None);
                }
                return None;
            };
            current = parent;
        }
    }
}

pub(crate) fn is_declaration_file(path: &Path) -> bool {
    tsz::module_resolver::ModuleExtension::from_path(path).is_declaration()
}

pub(crate) fn canonicalize_with_missing_tail(path: &Path) -> PathBuf {
    if let Ok(canonical) = std::fs::canonicalize(path) {
        return canonical;
    }

    let mut tail = Vec::new();
    let mut current = path;
    while !current.exists() {
        let Some(name) = current.file_name() else {
            return path.to_path_buf();
        };
        tail.push(name.to_os_string());
        let Some(parent) = current.parent() else {
            return path.to_path_buf();
        };
        current = parent;
    }

    let Ok(mut canonical) = std::fs::canonicalize(current) else {
        return path.to_path_buf();
    };
    for component in tail.iter().rev() {
        canonical.push(component);
    }
    canonical
}

/// Canonicalize to a real on-disk path, falling back to the *lexically
/// normalized* path when the file cannot be canonicalized (missing or
/// transiently unreadable file, relative anchor).
///
/// The fallback is normalized rather than the raw input so callers that key
/// identity on the result — program-file caches, dedup sets, redirect maps —
/// stay deterministic: `./a/b.ts`, `a/b.ts`, and `a/b.ts/` collapse to one key
/// instead of three. Display-only callers are unaffected, as a normalized path
/// renders identically to (and more cleanly than) the raw input.
pub(crate) fn canonicalize_or_owned(path: &Path) -> PathBuf {
    std::fs::canonicalize(path).unwrap_or_else(|_| normalize_path(path))
}

pub(crate) fn env_flag(name: &str) -> bool {
    let Ok(value) = std::env::var(name) else {
        return false;
    };
    let normalized = value.trim().to_ascii_lowercase();
    matches!(normalized.as_str(), "1" | "true" | "yes" | "on")
}

#[cfg(test)]
#[path = "resolution_tests.rs"]
mod resolution_tests;

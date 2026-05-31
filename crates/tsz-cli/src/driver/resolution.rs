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
mod type_packages;

// Public API re-exports for `crate::driver::resolution::<item>` callers.
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
pub(crate) use type_packages::{
    collect_type_packages_from_root, default_type_roots, resolve_type_package_entry_with_cache,
    resolve_type_package_entry_with_mode_and_cache, resolve_type_package_from_roots_with_cache,
    resolve_type_reference_from_node_modules_with_cache, type_package_candidates_pub,
};
#[cfg(test)]
pub(crate) use type_packages::{resolve_type_package_entry, resolve_type_package_entry_with_mode};

// `implied_resolution_mode_for_file*` are used by `super::core` etc.
pub(super) use type_packages::{
    implied_resolution_mode_for_file, implied_resolution_mode_for_file_with_cache,
};

// Internal sharing: bring sibling-submodule items into the resolution
// namespace so the in-file test module finds them via `super::*` and so
// siblings can call `super::<item>` for items already promoted to pub(crate).
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

// ─────────────────────────────────────────────────────────────────────────
// Counting filesystem probes (`PERFORMANCE_PLAN.md` §4.T0.3 follow-up)
//
// The plan calls for `resolver.is_file/is_dir/read_dir` counters so the
// 2026-05-10 attribution decision matrix can prove (or refute) that the
// resolver is on the hot path. Rather than sprinkle inline `inc()` calls
// before every `Path::is_file()`, we wrap the three probe primitives in
// thin counting helpers and route resolver code through them. Call sites
// in this file now use `count_is_file(p)` instead of `p.is_file()`, which
// keeps the diff one token per call and makes future swaps to a real
// `CountingFs` trait (the eventual T2.0 wrapper) a one-place change.
//
// The counter bumps themselves now live in `tsz_common::perf_counters`
// as `record_resolver_*` helpers (sibling to the cross-arena / interner
// helpers consolidated in #5097 / #5103 / #5112 / #5115 / #5118). Each
// wrapper here keeps its file-local identity because it bundles the
// counter with the underlying syscall — that's intentional grouping —
// while the gate-and-deref pattern lives in one place.
#[inline]
pub(super) fn count_is_file(path: &Path) -> bool {
    tsz_common::perf_counters::record_resolver_is_file();
    path.is_file()
}

#[inline]
pub(super) fn count_is_dir(path: &Path) -> bool {
    tsz_common::perf_counters::record_resolver_is_dir();
    path.is_dir()
}

#[inline]
pub(super) fn count_read_dir(path: &Path) -> std::io::Result<std::fs::ReadDir> {
    tsz_common::perf_counters::record_resolver_read_dir();
    std::fs::read_dir(path)
}

/// Bump `resolver_candidate_paths_total` once per invocation. The
/// `tsz_common::perf_counters::record_resolver_candidate_path` helper
/// gates and dereferences once; this wrapper preserves the file-local
/// `count_candidate_path` name so the two emit sites
/// (path-mapping virtual roots and suffix-extension expansion) stay
/// stable.
#[inline]
pub(super) fn count_candidate_path() {
    tsz_common::perf_counters::record_resolver_candidate_path();
}

/// Bump `resolver_read_package_json_calls` once per uncached read.
/// Sits inside `read_package_json_uncached`, which `large-ts-repo`
/// profiles flag as the dominant resolver work — keeping the gate
/// cheap matters even though the surrounding `read_to_string` is
/// several orders of magnitude more expensive.
#[inline]
pub(super) fn count_read_package_json() {
    tsz_common::perf_counters::record_resolver_read_package_json();
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

            if let Some(package_json) = self.read_package_json(&current.join("package.json")) {
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

pub(crate) fn canonicalize_or_owned(path: &Path) -> PathBuf {
    std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
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

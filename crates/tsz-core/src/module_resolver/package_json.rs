//! Package.json reading and package type resolution.
//!
//! Handles reading/parsing package.json files and determining the
//! package type (ESM vs CommonJS) for a given directory.

use super::{ImportingModuleKind, ModuleResolver, PackageType};
use crate::config::ModuleResolutionKind;
use std::path::Path;

use crate::module_resolver_helpers::{PackageJson, cached_is_file};

/// Parse `package.json#type` into a [`PackageType`], or `None` when the
/// field is missing or an unknown value. Shared by both
/// [`ModuleResolver::target_package_type_from_json`] (which defaults the
/// unknown/missing case to `CommonJs` for `Node16`/`NodeNext`) and
/// [`ModuleResolver::get_package_type_for_dir`] (which leaves the unknown
/// case as `None` so the directory walk-up can keep climbing).
fn parse_package_type_field(field: Option<&str>) -> Option<PackageType> {
    match field {
        Some("module") => Some(PackageType::Module),
        Some("commonjs") => Some(PackageType::CommonJs),
        _ => None,
    }
}

impl ModuleResolver {
    /// Map the importer's [`ImportingModuleKind`] to the
    /// `Option<PackageType>` that should drive extension-priority choices
    /// for file probing in the importer's own package context (relative
    /// paths, baseUrl/path mappings, classic walk-up). Only
    /// `Node16`/`NodeNext` distinguish ESM vs CJS extension order; every
    /// other mode treats all extensions equally and so returns `None`.
    pub(super) const fn importer_package_type(
        &self,
        importing_module_kind: ImportingModuleKind,
    ) -> Option<PackageType> {
        match self.resolution_kind {
            ModuleResolutionKind::Node16 | ModuleResolutionKind::NodeNext => {
                Some(match importing_module_kind {
                    ImportingModuleKind::Esm => PackageType::Module,
                    ImportingModuleKind::CommonJs => PackageType::CommonJs,
                })
            }
            _ => None,
        }
    }

    /// Compute the target [`PackageType`] for file probing inside a package
    /// whose `package.json` has already been read.
    ///
    /// The extension-priority axis (`.mts`/`.d.mts` first vs `.cts`/`.d.cts`
    /// first) only applies under `Node16`/`NodeNext`; other resolution kinds
    /// return `None` so the helper always agrees with
    /// [`Self::extension_candidates_for_package_type`].
    ///
    /// `inherit_from` is the caller's fallback hint when the target's
    /// `package.json` has no `"type"` field or declares an unknown value;
    /// it lets nested directories (e.g. `try_directory`) inherit their
    /// enclosing package's mode under `Node16`/`NodeNext` instead of
    /// silently snapping back to `CommonJs`.
    pub(super) fn target_package_type_from_json(
        &self,
        pj: &PackageJson,
        inherit_from: Option<PackageType>,
    ) -> Option<PackageType> {
        match self.resolution_kind {
            ModuleResolutionKind::Node16 | ModuleResolutionKind::NodeNext => Some(
                parse_package_type_field(pj.package_type.as_deref())
                    .or(inherit_from)
                    .unwrap_or(PackageType::CommonJs),
            ),
            _ => None,
        }
    }

    /// Get the package type for a directory by walking up to find package.json
    pub(super) fn get_package_type_for_dir(&self, dir: &Path) -> Option<PackageType> {
        let mut current = dir.to_path_buf();
        let mut visited = Vec::new();

        loop {
            let cached_current_type = self.package_type_cache.borrow().get(&current).copied();
            if let Some(result) = cached_current_type {
                Self::increment_counter(&self.package_type_cache_hits);
                // Cache all visited paths with this result
                let mut cache = self.package_type_cache.borrow_mut();
                for path in visited {
                    cache.insert(path, result);
                }
                return result;
            }
            Self::increment_counter(&self.package_type_cache_misses);

            visited.push(current.clone());

            // Check for package.json
            let package_json_path = current.join("package.json");
            if cached_is_file(&package_json_path)
                && let Ok(pj) = self.read_package_json(&package_json_path)
            {
                let package_type = parse_package_type_field(pj.package_type.as_deref());
                // Cache all visited paths
                let mut cache = self.package_type_cache.borrow_mut();
                for path in visited {
                    cache.insert(path, package_type);
                }
                return package_type;
            }

            // Move to parent
            match current.parent() {
                Some(parent) if parent != current => current = parent.to_path_buf(),
                _ => break,
            }
        }

        // No package.json found, cache as None
        let mut cache = self.package_type_cache.borrow_mut();
        for path in visited {
            cache.insert(path, None);
        }
        None
    }

    /// Read and parse `package.json`, with a per-resolver cache.
    ///
    /// The same `package.json` (typically in `node_modules/<pkg>/`) is read
    /// for multiple distinct purposes during one specifier's resolution
    /// (`package_type` lookup, exports map, main field, types field, self-
    /// reference). Without a cache each role re-stat'd, re-read, and
    /// re-parsed the file. The cache is populated on first read and reused
    /// for the rest of the resolver's lifetime.
    ///
    /// Both `Ok` and `Err` results are cached so missing-file / invalid-JSON
    /// failure paths also don't re-stat or re-parse on subsequent visits.
    ///
    /// Returns a `String` error for flexibility - callers can convert to `ResolutionFailure`
    /// with appropriate span/file information at the call site.
    pub(super) fn read_package_json(&self, path: &Path) -> Result<PackageJson, String> {
        if let Some(cached) = self.package_json_cache.borrow().get(path) {
            Self::increment_counter(&self.package_json_cache_hits);
            return cached.clone();
        }
        Self::increment_counter(&self.package_json_cache_misses);
        let result = std::fs::read_to_string(path)
            .map_err(|e| format!("Failed to read {}: {}", path.display(), e))
            .and_then(|content| {
                serde_json::from_str::<PackageJson>(&content)
                    .map_err(|e| format!("Failed to parse {}: {}", path.display(), e))
            });
        self.package_json_cache
            .borrow_mut()
            .insert(path.to_path_buf(), result.clone());
        result
    }
}

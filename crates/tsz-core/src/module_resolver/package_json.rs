//! Package.json reading and package type resolution.
//!
//! Handles reading/parsing package.json files and determining the
//! package type (ESM vs CommonJS) for a given directory.

use super::{ModuleResolver, PackageType};
use std::path::Path;

use crate::module_resolver_helpers::PackageJson;

impl ModuleResolver {
    /// Get the package type for a directory by walking up to find package.json
    pub(super) fn get_package_type_for_dir(&mut self, dir: &Path) -> Option<PackageType> {
        // Check cache first
        if let Some(cached) = self.package_type_cache.get(dir) {
            return *cached;
        }

        let mut current = dir.to_path_buf();
        let mut visited = Vec::new();

        loop {
            // Check cache for this path - copy the value to avoid borrow conflict
            if let Some(&cached) = self.package_type_cache.get(&current) {
                let result = cached;
                // Cache all visited paths with this result
                for path in visited {
                    self.package_type_cache.insert(path, result);
                }
                return result;
            }

            visited.push(current.clone());

            // Check for package.json
            let package_json_path = current.join("package.json");
            if package_json_path.is_file()
                && let Ok(pj) = self.read_package_json(&package_json_path)
            {
                let package_type = pj.package_type.as_deref().and_then(|t| match t {
                    "module" => Some(PackageType::Module),
                    "commonjs" => Some(PackageType::CommonJs),
                    _ => None,
                });
                // Cache all visited paths
                for path in visited {
                    self.package_type_cache.insert(path, package_type);
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
        for path in visited {
            self.package_type_cache.insert(path, None);
        }
        None
    }

    /// Read and parse package.json
    ///
    /// Returns a String error for flexibility - callers can convert to `ResolutionFailure`
    /// with appropriate span/file information at the call site.
    pub(super) fn read_package_json(&self, path: &Path) -> Result<PackageJson, String> {
        let content = std::fs::read_to_string(path)
            .map_err(|e| format!("Failed to read {}: {}", path.display(), e))?;

        serde_json::from_str(&content)
            .map_err(|e| format!("Failed to parse {}: {}", path.display(), e))
    }
}

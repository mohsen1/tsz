//! Self-reference resolution (package importing itself by name).
//!
//! In Node16/NodeNext/Bundler, a package can import itself using its own
//! name if the package.json `exports` field matches.

use super::{ModuleExtension, ModuleResolver, ResolvedModule};
use crate::config::ModuleResolutionKind;
use std::path::Path;

impl ModuleResolver {
    /// Try to resolve a self-reference (package importing itself by name)
    pub(super) fn try_self_reference(
        &self,
        package_name: &str,
        subpath: Option<&str>,
        original_specifier: &str,
        containing_dir: &Path,
        conditions: &[String],
    ) -> Option<ResolvedModule> {
        // Only available in Node16/NodeNext/Bundler
        if !matches!(
            self.resolution_kind,
            ModuleResolutionKind::Node16
                | ModuleResolutionKind::NodeNext
                | ModuleResolutionKind::Bundler
        ) {
            return None;
        }

        // Walk up to find the closest package.json
        let mut current = containing_dir.to_path_buf();

        loop {
            let package_json_path = current.join("package.json");

            if package_json_path.is_file()
                && let Ok(package_json) = self.read_package_json(&package_json_path)
            {
                // Check if the package name matches
                if package_json.name.as_deref() == Some(package_name) {
                    // This is a self-reference!
                    if self.resolve_package_json_exports
                        && let Some(exports) = &package_json.exports
                    {
                        let subpath_key = match subpath {
                            Some(sp) => format!("./{sp}"),
                            None => ".".to_string(),
                        };

                        if let Some(resolved) = self.resolve_package_exports_with_conditions(
                            &current,
                            exports,
                            &subpath_key,
                            conditions,
                        ) {
                            return Some(ResolvedModule {
                                resolved_path: resolved.clone(),
                                is_external: false,
                                package_name: Some(package_name.to_string()),
                                original_specifier: original_specifier.to_string(),
                                extension: ModuleExtension::from_path(&resolved),
                            });
                        }
                    }
                }
                // Found a package.json but it's not a match - stop searching
                return None;
            }

            // Move to parent directory
            match current.parent() {
                Some(parent) if parent != current => current = parent.to_path_buf(),
                _ => break,
            }
        }

        None
    }
}

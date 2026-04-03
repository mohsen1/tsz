//! Self-reference resolution (package importing itself by name).
//!
//! In Node16/NodeNext/Bundler, a package can import itself using its own
//! name if the package.json `exports` field matches.

use super::{ModuleExtension, ModuleResolver, ResolvedModule};
use crate::config::ModuleResolutionKind;
use crate::span::Span;
use std::path::Path;

/// Result of trying to resolve a self-reference
pub(super) enum SelfReferenceResultV2 {
    /// Successfully resolved to a module
    Resolved(ResolvedModule),
    /// Self-reference detected but exports don't resolve - should emit error
    ExportsFailed,
    /// Not a self-reference (package name doesn't match) - should continue searching
    NotSelfReference,
    /// TS2209: Project root is ambiguous for export map entry
    AmbiguousRoot {
        export_map_entry: String,
        package_json_path: String,
    },
}

impl ModuleResolver {
    /// Try to resolve a self-reference (package importing itself by name)
    /// This version properly distinguishes between:
    /// 1. Self-reference that successfully resolved
    /// 2. Self-reference detected but exports don't resolve (should error)
    /// 3. Not a self-reference (package name doesn't match) - should continue searching
    pub(super) fn try_self_reference_v2(
        &self,
        package_name: &str,
        subpath: Option<&str>,
        original_specifier: &str,
        containing_dir: &Path,
        containing_file: &str,
        specifier_span: Span,
        conditions: &[String],
    ) -> SelfReferenceResultV2 {
        // Only available in Node16/NodeNext/Bundler
        if !matches!(
            self.resolution_kind,
            ModuleResolutionKind::Node16
                | ModuleResolutionKind::NodeNext
                | ModuleResolutionKind::Bundler
        ) {
            return SelfReferenceResultV2::NotSelfReference;
        }

        // Walk up to find the closest package.json
        let mut current = containing_dir.to_path_buf();

        loop {
            let package_json_path = current.join("package.json");

            if package_json_path.is_file()
                && let Ok(package_json) = self.read_package_json(&package_json_path)
            {
                // Check if the package name matches - this is REQUIRED for a self-reference
                let name_matches = package_json.name.as_deref() == Some(package_name);

                if name_matches {
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
                            // Check if the resolved file is a TypeScript file (.ts, .mts, .cts, .d.ts, etc.)
                            // that was found via extension substitution for a .js export target.
                            // For self-references, this should count as resolution failure to emit
                            // the correct error (TS2307) instead of TS1479 (format mismatch).
                            let resolved_str = resolved.to_string_lossy();
                            let is_ts_file = resolved_str.ends_with(".ts")
                                || resolved_str.ends_with(".mts")
                                || resolved_str.ends_with(".cts");

                            if !is_ts_file {
                                // Only count as resolved if it's not a TypeScript file
                                return SelfReferenceResultV2::Resolved(ResolvedModule {
                                    resolved_path: resolved.clone(),
                                    resolved_using_ts_extension: false,
                                    is_external: false,
                                    package_name: Some(package_name.to_string()),
                                    original_specifier: original_specifier.to_string(),
                                    extension: ModuleExtension::from_path(&resolved),
                                });
                            }
                            // If it's a TypeScript file, fall through to ExportsFailed
                        }
                        // Self-reference detected but exports didn't resolve to an existing file
                        // Check if this is due to ambiguous project root (TS2209)
                        if self.root_dir.is_none() {
                            return SelfReferenceResultV2::AmbiguousRoot {
                                export_map_entry: subpath_key,
                                package_json_path: package_json_path.display().to_string(),
                            };
                        }
                        return SelfReferenceResultV2::ExportsFailed;
                    }
                    // Name matches but no exports field - not a self-reference for Node16+
                    // Fall through to NotSelfReference
                }
                // Found a package.json but either:
                // - The name doesn't match, OR
                // - The name matches but there's no exports field
                // In both cases, stop searching and continue to node_modules resolution
                return SelfReferenceResultV2::NotSelfReference;
            }

            // Move to parent directory
            match current.parent() {
                Some(parent) if parent != current => current = parent.to_path_buf(),
                _ => break,
            }
        }

        SelfReferenceResultV2::NotSelfReference
    }

    /// Try to resolve a self-reference (package importing itself by name)
    /// Legacy API for backward compatibility
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
                                resolved_using_ts_extension: false,
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

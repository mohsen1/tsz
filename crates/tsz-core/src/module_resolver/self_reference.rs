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
                            // CRITICAL: For self-references, we must ensure the resolved file
                            // is the EXACT target specified in exports, not a TypeScript source
                            // file found via extension substitution.
                            //
                            // When exports specify "./index.js" but the file doesn't exist,
                            // and extension substitution finds "./index.ts", this should NOT
                            // count as successful resolution. It would cause TS1479 (format
                            // mismatch) instead of the expected TS2209/TS2307.
                            //
                            // We verify this by checking if the resolved path has the same
                            // extension as what was specified in the exports target.
                            let export_target_path = self.resolve_export_target_to_string(
                                exports,
                                conditions,
                            ).map(|target| current.join(target.trim_start_matches("./")));
                            
                            if let Some(ref target_path) = export_target_path {
                                // Check if the resolved path has the same extension as the target
                                let target_ext = target_path.extension().and_then(|e| e.to_str());
                                let resolved_ext = resolved.extension().and_then(|e| e.to_str());
                                
                                // If extensions don't match (e.g., target is .js but resolved is .ts),
                                // this is extension substitution - treat as failed self-reference
                                if target_ext == resolved_ext && resolved.is_file() {
                                    return SelfReferenceResultV2::Resolved(ResolvedModule {
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
                        // Self-reference detected but exports didn't resolve correctly
                        // This should emit TS2209, not continue to node_modules
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

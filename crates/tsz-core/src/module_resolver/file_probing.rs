//! File probing and extension candidate logic for module resolution.
//!
//! This module contains the filesystem probing methods that try various
//! extension substitutions, suffix combinations, and directory index
//! fallbacks to find the actual file backing a module specifier.

use super::ModuleResolver;
use super::request_types::{ModuleExtension, PackageType};
use crate::config::ModuleResolutionKind;
use crate::module_resolver_helpers::*;
use std::path::{Path, PathBuf};

impl ModuleResolver {
    // =========================================================================
    // File probing methods
    // =========================================================================

    /// Try to resolve a file with various extensions
    pub(super) fn try_file(&self, path: &Path) -> Option<PathBuf> {
        let suffixes = &self.module_suffixes;
        if let Some(extension) = path.extension().and_then(|ext| ext.to_str())
            && split_path_extension(path).is_none()
        {
            if self.allow_arbitrary_extensions
                && let Some(resolved) = try_arbitrary_extension_declaration(path, extension)
            {
                return Some(resolved);
            }
            return None;
        }
        if let Some((base, extension)) = split_path_extension(path) {
            // Try extension substitution (.js -> .ts/.tsx/.d.ts) for all resolution modes.
            // TypeScript resolves `.js` imports to `.ts` sources in all modes.
            if let Some(rewritten) = node16_extension_substitution(path, extension) {
                for candidate in &rewritten {
                    if let Some(resolved) = try_file_with_suffixes(candidate, suffixes) {
                        return Some(resolved);
                    }
                }
            }

            // When rewriteRelativeImportExtensions is true, .ts/.tsx/.mts/.cts imports
            // should resolve to their declaration file equivalents (.d.ts/.d.mts/.d.cts).
            if self.rewrite_relative_import_extensions {
                let decl_ext = match extension {
                    "ts" | "tsx" => Some("d.ts"),
                    "mts" => Some("d.mts"),
                    "cts" => Some("d.cts"),
                    _ => None,
                };
                if let Some(decl_ext) = decl_ext {
                    let candidate = base.with_extension(decl_ext);
                    if let Some(resolved) = try_file_with_suffixes(&candidate, suffixes) {
                        return Some(resolved);
                    }
                }
            }

            // Fall back to the original extension (e.g., literal .js file)
            if let Some(resolved) = try_file_with_suffixes_and_extension(&base, extension, suffixes)
            {
                return Some(resolved);
            }

            return None;
        }

        let extensions = self.extension_candidates_for_resolution();
        for ext in extensions {
            if let Some(resolved) = try_file_with_suffixes_and_extension(path, ext, suffixes) {
                return Some(resolved);
            }
        }

        let index = path.join("index");
        for ext in extensions {
            if let Some(resolved) = try_file_with_suffixes_and_extension(&index, ext, suffixes) {
                return Some(resolved);
            }
        }

        None
    }

    /// Like `try_file`, but does NOT try directory index resolution (path/index.{ext}).
    /// Used for ESM packages in Node16/NodeNext where directory index resolution
    /// is not allowed by Node.js.
    pub(super) fn try_file_no_index(&self, path: &Path) -> Option<PathBuf> {
        let suffixes = &self.module_suffixes;
        if let Some(extension) = path.extension().and_then(|ext| ext.to_str())
            && split_path_extension(path).is_none()
        {
            if self.allow_arbitrary_extensions
                && let Some(resolved) = try_arbitrary_extension_declaration(path, extension)
            {
                return Some(resolved);
            }
            return None;
        }
        if let Some((base, extension)) = split_path_extension(path) {
            if let Some(rewritten) = node16_extension_substitution(path, extension) {
                for candidate in &rewritten {
                    if let Some(resolved) = try_file_with_suffixes(candidate, suffixes) {
                        return Some(resolved);
                    }
                }
            }
            if self.rewrite_relative_import_extensions {
                let decl_ext = match extension {
                    "ts" | "tsx" => Some("d.ts"),
                    "mts" => Some("d.mts"),
                    "cts" => Some("d.cts"),
                    _ => None,
                };
                if let Some(decl_ext) = decl_ext {
                    let candidate = base.with_extension(decl_ext);
                    if let Some(resolved) = try_file_with_suffixes(&candidate, suffixes) {
                        return Some(resolved);
                    }
                }
            }
            if let Some(resolved) = try_file_with_suffixes_and_extension(&base, extension, suffixes)
            {
                return Some(resolved);
            }
            return None;
        }

        let extensions = self.extension_candidates_for_resolution();
        for ext in extensions {
            if let Some(resolved) = try_file_with_suffixes_and_extension(path, ext, suffixes) {
                return Some(resolved);
            }
        }
        // No index fallback -- that's the whole point
        None
    }

    pub(super) const fn extension_candidates_for_resolution(&self) -> &'static [&'static str] {
        match self.resolution_kind {
            ModuleResolutionKind::Node16 | ModuleResolutionKind::NodeNext => {
                match self.current_package_type {
                    Some(PackageType::Module) => {
                        if self.allow_js {
                            &NODE16_MODULE_ALLOWJS_EXTENSION_CANDIDATES
                        } else {
                            &NODE16_MODULE_EXTENSION_CANDIDATES
                        }
                    }
                    Some(PackageType::CommonJs) => {
                        if self.allow_js {
                            &NODE16_COMMONJS_ALLOWJS_EXTENSION_CANDIDATES
                        } else {
                            &NODE16_COMMONJS_EXTENSION_CANDIDATES
                        }
                    }
                    None => {
                        if self.allow_js {
                            &TS_JS_EXTENSION_CANDIDATES
                        } else {
                            &TS_EXTENSION_CANDIDATES
                        }
                    }
                }
            }
            ModuleResolutionKind::Classic => {
                if self.allow_js {
                    &TS_JS_EXTENSION_CANDIDATES
                } else {
                    &CLASSIC_EXTENSION_CANDIDATES
                }
            }
            _ => {
                if self.allow_js {
                    &TS_JS_EXTENSION_CANDIDATES
                } else {
                    &TS_EXTENSION_CANDIDATES
                }
            }
        }
    }

    pub(super) fn try_directory(&self, path: &Path) -> Option<PathBuf> {
        if !path.is_dir() {
            return None;
        }

        let package_json_path = path.join("package.json");
        if package_json_path.exists()
            && let Ok(pj) = self.read_package_json(&package_json_path)
        {
            if let Some(types) = pj
                .types
                .or(pj.typings)
                .filter(|types| !types.trim().is_empty())
            {
                let types_path = path.join(&types);
                if let Some(resolved) = self.try_types_entry(&types_path) {
                    return Some(resolved);
                }
            }
            if let Some(main) = &pj.main {
                let main_path = path.join(main);
                if let Some(resolved) = self.try_file(&main_path) {
                    return Some(resolved);
                }
            }
        }

        let index = path.join("index");
        self.try_file(&index)
    }

    /// Try to resolve a path as a file or directory
    pub(super) fn try_file_or_directory(&self, path: &Path) -> Option<PathBuf> {
        // Try as file first
        if let Some(resolved) = self.try_file(path) {
            return Some(resolved);
        }

        self.try_directory(path)
    }

    /// Resolve an exports target without Node16 extension substitution.
    ///
    /// Explicit extensions must exist exactly; extensionless targets follow normal lookup.
    pub(super) fn try_export_target(&self, path: &Path) -> Option<PathBuf> {
        if let Some(extension) = path.extension().and_then(|ext| ext.to_str()) {
            if split_path_extension(path).is_some() {
                if path.is_file() {
                    return Some(path.to_path_buf());
                }
                // For JS export targets, try declaration substitution
                if let Some(rewritten) = node16_extension_substitution(path, extension) {
                    for candidate in &rewritten {
                        if candidate.is_file() {
                            return Some(candidate.clone());
                        }
                    }
                }
                return None;
            }
            if self.allow_arbitrary_extensions
                && let Some(resolved) = try_arbitrary_extension_declaration(path, extension)
            {
                return Some(resolved);
            }
            return None;
        }

        if let Some(resolved) = self.try_file(path) {
            return Some(resolved);
        }
        if path.is_dir() {
            let index = path.join("index");
            return self.try_file(&index);
        }
        None
    }

    pub(super) fn try_types_entry(&self, path: &Path) -> Option<PathBuf> {
        if let Some(resolved) = resolve_explicit_unknown_extension(path) {
            return Some(resolved);
        }

        if let Some((base, extension)) = split_path_extension(path) {
            if let Some(rewritten) = node16_extension_substitution(path, extension) {
                for candidate in &rewritten {
                    if let Some(resolved) = try_file_with_suffixes(candidate, &self.module_suffixes)
                    {
                        return Some(resolved);
                    }
                }
            }

            let explicit_extension = ModuleExtension::from_path(path);
            if matches!(
                explicit_extension,
                ModuleExtension::Ts
                    | ModuleExtension::Tsx
                    | ModuleExtension::Dts
                    | ModuleExtension::DmTs
                    | ModuleExtension::DCts
                    | ModuleExtension::Mts
                    | ModuleExtension::Cts
            ) {
                return try_file_with_suffixes_and_extension(
                    &base,
                    extension,
                    &self.module_suffixes,
                );
            }

            return None;
        }

        self.try_file_or_directory(path)
    }
}

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

    /// Try to resolve a file with various extensions.
    ///
    /// `package_type` is the type of the package CONTAINING `path` (the
    /// target's `package.json#type` for external lookups, the importer's
    /// own package type for in-tree lookups). For `Node16`/`NodeNext` this
    /// drives whether `.mts`/`.d.mts` or `.cts`/`.d.cts` are tried first; in
    /// other resolution modes it is ignored. Use `None` when no package
    /// context applies (path mapping, baseUrl, classic, bundler bare).
    pub(super) fn try_file(
        &self,
        path: &Path,
        package_type: Option<PackageType>,
    ) -> Option<PathBuf> {
        self.try_file_inner(path, package_type, true)
    }

    /// Like [`Self::try_file`], but does NOT try directory index resolution
    /// (`path/index.{ext}`). Used for ESM packages in Node16/NodeNext where
    /// directory index resolution is not allowed by Node.js.
    pub(super) fn try_file_no_index(
        &self,
        path: &Path,
        package_type: Option<PackageType>,
    ) -> Option<PathBuf> {
        self.try_file_inner(path, package_type, false)
    }

    /// Shared body of [`Self::try_file`] and [`Self::try_file_no_index`]. The
    /// two differ only in the trailing directory-index probe, gated on
    /// `try_index`: `try_file` passes `true`, `try_file_no_index` passes
    /// `false` (ESM packages in Node16/NodeNext, where Node.js forbids
    /// directory index resolution). Everything before that — normalization,
    /// arbitrary-extension declaration probing, `node16_extension_substitution`,
    /// the `rewriteRelativeImportExtensions` declaration remap, and the
    /// original-extension fallback — is identical between the two callers.
    fn try_file_inner(
        &self,
        path: &Path,
        package_type: Option<PackageType>,
        try_index: bool,
    ) -> Option<PathBuf> {
        // Canonicalize `.`/`..` before probing so the returned `PathBuf` is
        // the same across alias branches that point at the same file.
        let path = &normalize_path_segments(path);
        let suffixes = &self.module_suffixes;
        if let Some(extension) = path.extension().and_then(|ext| ext.to_str())
            && split_path_extension(path).is_none()
        {
            // Always probe for .d.*.ts declaration files regardless of allowArbitraryExtensions.
            // When the flag is off, the caller (lookup) emits TS6263 for the resolved file.
            if let Some(resolved) = try_arbitrary_extension_declaration(path, extension) {
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
            if self.rewrite_relative_import_extensions
                && let Some(decl_ext) = ts_extension_to_declaration(extension)
            {
                let candidate = base.with_extension(decl_ext);
                if let Some(resolved) = try_file_with_suffixes(&candidate, suffixes) {
                    return Some(resolved);
                }
            }

            // Fall back to the original extension (e.g., literal .js file)
            if let Some(resolved) = try_file_with_suffixes_and_extension(&base, extension, suffixes)
            {
                return Some(resolved);
            }

            return None;
        }

        let extensions = self.extension_candidates_for_package_type(package_type);
        for ext in extensions {
            if let Some(resolved) = try_file_with_suffixes_and_extension(path, ext, suffixes) {
                return Some(resolved);
            }
        }

        if try_index {
            let index = path.join("index");
            for ext in extensions {
                if let Some(resolved) = try_file_with_suffixes_and_extension(&index, ext, suffixes)
                {
                    return Some(resolved);
                }
            }
        }

        None
    }

    pub(super) const fn extension_candidates_for_package_type(
        &self,
        package_type: Option<PackageType>,
    ) -> &'static [&'static str] {
        match self.resolution_kind {
            ModuleResolutionKind::Node16 | ModuleResolutionKind::NodeNext => match package_type {
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
                        TS_JS_EXTENSION_CANDIDATES
                    } else {
                        TS_EXTENSION_CANDIDATES
                    }
                }
            },
            ModuleResolutionKind::Classic => {
                if self.allow_js {
                    TS_JS_EXTENSION_CANDIDATES
                } else {
                    CLASSIC_EXTENSION_CANDIDATES
                }
            }
            _ => {
                if self.allow_js {
                    TS_JS_EXTENSION_CANDIDATES
                } else {
                    TS_EXTENSION_CANDIDATES
                }
            }
        }
    }

    pub(super) fn try_directory(
        &self,
        path: &Path,
        package_type: Option<PackageType>,
    ) -> Option<PathBuf> {
        // Normalize before `is_dir()` so the existence probe is robust to
        // non-existent intermediates the host OS cannot walk, and so the
        // returned `PathBuf` is canonical (e.g. typesVersions `../` joins
        // produce `ts3.1/../ts3.1/..` otherwise).
        let path = &normalize_path_segments(path);
        if !cached_is_dir(path) {
            return None;
        }

        let package_json_path = path.join("package.json");
        if cached_is_file(&package_json_path)
            && let Ok(pj) = self.read_package_json(&package_json_path)
        {
            // The directory's own `package.json#type` determines the
            // extension-candidate priority for `main` / `types` resolution
            // inside this directory; the caller-supplied `package_type`
            // only acts as an inherited fallback when this nested
            // package.json has no `"type"` field.
            let dir_package_type = self.target_package_type_from_json(&pj, package_type);
            let types = pj
                .types
                .clone()
                .or_else(|| pj.typings.clone())
                .filter(|types| !types.trim().is_empty());

            // Apply typesVersions before the bare types/typings path. tsc's
            // loadNodeModuleFromDirectoryWorker consults typesVersions when
            // resolving a directory via package.json, matching the types field
            // value (defaulting to "index") as the subpath. Without this, a
            // relative import like `../` into a package with typesVersions
            // bypasses the redirect, producing a divergent resolution from tsc.
            if let Some(types_versions) = &pj.types_versions {
                let subpath = types.as_deref().unwrap_or("index");
                if let Some(resolved) =
                    self.resolve_types_versions(path, subpath, types_versions, dir_package_type)
                {
                    return Some(resolved);
                }
            }

            if let Some(types) = types {
                let types_path = path.join(&types);
                if let Some(resolved) = self.try_types_entry(&types_path, dir_package_type) {
                    return Some(resolved);
                }
            }
            if let Some(main) = &pj.main {
                let main_path = path.join(main);
                if let Some(resolved) = self.try_file(&main_path, dir_package_type) {
                    return Some(resolved);
                }
            }
            let index = path.join("index");
            return self.try_file(&index, dir_package_type);
        }

        let index = path.join("index");
        self.try_file(&index, package_type)
    }

    /// Try to resolve a path as a file or directory
    pub(super) fn try_file_or_directory(
        &self,
        path: &Path,
        package_type: Option<PackageType>,
    ) -> Option<PathBuf> {
        // Try as file first
        if let Some(resolved) = self.try_file(path, package_type) {
            return Some(resolved);
        }

        self.try_directory(path, package_type)
    }

    /// Resolve an exports target without Node16 extension substitution.
    ///
    /// Explicit extensions must exist exactly; extensionless targets follow normal lookup.
    pub(super) fn try_export_target(
        &self,
        path: &Path,
        package_type: Option<PackageType>,
    ) -> Option<PathBuf> {
        let path = &normalize_path_segments(path);
        if let Some(extension) = path.extension().and_then(|ext| ext.to_str()) {
            if split_path_extension(path).is_some() {
                // For JS export targets, try declaration substitution first.
                // tsc always prefers .d.ts/.ts/.tsx over .js when resolving
                // conditional export targets (output-to-source remapping).
                if let Some(rewritten) = node16_extension_substitution(path, extension) {
                    for candidate in &rewritten {
                        if let Some(resolved) =
                            try_file_with_suffixes(candidate, &self.module_suffixes)
                        {
                            return Some(resolved);
                        }
                    }
                }
                // Fall back to the original file if no declaration substitute exists
                if cached_is_file(path) {
                    return Some(path.to_path_buf());
                }
                return None;
            }
            // Always probe for .d.*.ts declaration files regardless of allowArbitraryExtensions.
            // When the flag is off, the caller (lookup) emits TS6263 for the resolved file.
            if let Some(resolved) = try_arbitrary_extension_declaration(path, extension) {
                return Some(resolved);
            }
            return None;
        }

        // In Node16/NodeNext mode, extensionless export targets (e.g. from
        // `"./": "./"` directory exports) must NOT probe for extensions.
        // Node.js resolves exports targets literally — `./other` must exist as a
        // file with that exact name. tsc mirrors this: it does not add .ts/.d.ts
        // etc. when the target has no extension. Without this guard,
        // `import "pkg/other"` would silently resolve to `pkg/other.d.ts`, which
        // contradicts the ESM requirement for explicit extensions in specifiers.
        let skip_extension_probing = matches!(
            self.resolution_kind,
            ModuleResolutionKind::Node16 | ModuleResolutionKind::NodeNext
        );
        if !skip_extension_probing && let Some(resolved) = self.try_file(path, package_type) {
            return Some(resolved);
        }
        if cached_is_dir(path) {
            let index = path.join("index");
            return self.try_file(&index, package_type);
        }
        None
    }

    pub(super) fn try_types_entry(
        &self,
        path: &Path,
        package_type: Option<PackageType>,
    ) -> Option<PathBuf> {
        let path = &normalize_path_segments(path);
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

        self.try_file_or_directory(path, package_type)
    }
}

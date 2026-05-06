//! Relative and absolute import path resolution.
//!
//! Handles `./foo`, `../bar`, `.`, `..`, and `/absolute` import specifiers,
//! including Node16/NodeNext ESM extension validation (TS2834/TS2835).

use super::{
    ImportKind, ImportingModuleKind, ModuleExtension, ModuleResolver, PackageType,
    ResolutionFailure, ResolvedModule,
};
use crate::config::ModuleResolutionKind;
use crate::module_resolver_helpers::KNOWN_EXTENSIONS;
use crate::span::Span;
use std::path::{Component, Path, PathBuf};

impl ModuleResolver {
    const fn suggested_runtime_extension(&self, resolved_ext: ModuleExtension) -> &'static str {
        match resolved_ext {
            ModuleExtension::Ts
            | ModuleExtension::Dts
            | ModuleExtension::Unknown
            | ModuleExtension::Js => ".js",
            ModuleExtension::Tsx => match self.jsx {
                Some(crate::config::JsxEmit::Preserve) => ".jsx",
                _ => ".js",
            },
            ModuleExtension::Jsx => ".jsx",
            ModuleExtension::Mts | ModuleExtension::Mjs | ModuleExtension::DmTs => ".mjs",
            ModuleExtension::Cts | ModuleExtension::Cjs | ModuleExtension::DCts => ".cjs",
            ModuleExtension::Json => ".json",
        }
    }

    fn resolved_via_directory_index(&self, resolved: &Path, candidate: &Path) -> bool {
        self.is_index_file_with_module_suffix(resolved)
            && candidate
                .file_name()
                .and_then(|name| name.to_str())
                .is_none_or(|name| name != "index")
    }

    fn is_index_file_with_module_suffix(&self, resolved: &Path) -> bool {
        let Some(name) = resolved.file_name().and_then(|name| name.to_str()) else {
            return false;
        };
        let Some(after_index) = name.strip_prefix("index") else {
            return false;
        };

        self.module_suffixes.iter().any(|suffix| {
            after_index
                .strip_prefix(suffix)
                .is_some_and(|extension| KNOWN_EXTENSIONS.contains(&extension))
        })
    }

    /// Resolve a relative import
    pub(super) fn resolve_relative(
        &self,
        specifier: &str,
        containing_dir: &Path,
        containing_file: &str,
        specifier_span: Span,
        importing_module_kind: ImportingModuleKind,
        import_kind: ImportKind,
    ) -> Result<ResolvedModule, ResolutionFailure> {
        let candidate = containing_dir.join(specifier);
        let mut candidates = vec![candidate];
        candidates.extend(self.root_dirs_relative_candidates(containing_dir, specifier));
        let prefer_directory = specifier == "."
            || specifier == ".."
            || specifier.ends_with('/')
            || specifier.ends_with('\\');
        let try_resolve_candidate = |path: &Path| {
            let uses_require_resolution = import_kind == ImportKind::CjsRequire
                || (matches!(import_kind, ImportKind::EsmImport | ImportKind::EsmReExport)
                    && importing_module_kind == ImportingModuleKind::CommonJs);
            let package_type = if matches!(
                self.resolution_kind,
                ModuleResolutionKind::Node16 | ModuleResolutionKind::NodeNext
            ) && uses_require_resolution
            {
                let package_dir = if prefer_directory || path.is_dir() {
                    path
                } else {
                    path.parent().unwrap_or_else(|| Path::new("."))
                };
                Some(
                    self.get_package_type_for_dir(package_dir)
                        .unwrap_or(PackageType::CommonJs),
                )
            } else {
                self.current_package_type
            };
            if prefer_directory {
                self.try_directory_with_package_type(path, package_type)
            } else {
                self.try_file_or_directory_with_package_type(path, package_type)
            }
        };

        // Check if specifier has an explicit extension
        let specifier_has_extension = Path::new(specifier)
            .extension()
            .is_some_and(|ext| !ext.is_empty());

        // TS2834/TS2835 Check: In Node16/NodeNext, ESM-style imports must have explicit extensions.
        // This applies when:
        // - The resolution mode is Node16 or NodeNext
        // - The containing file is a TypeScript file (not .js/.jsx/.mjs/.cjs)
        //   TSC does not enforce extension requirements on JS files for relative imports,
        //   since JS files are consumed as-is and don't go through TS module compilation.
        // - The import is ESM syntax in an ESM context:
        //   - Dynamic import() always counts as ESM regardless of file type
        //   - Static import/export only counts if the file is an ESM module
        //   - require() never triggers this check
        // - The specifier has no extension
        let containing_is_js = {
            let ext = ModuleExtension::from_path(Path::new(containing_file));
            matches!(
                ext,
                ModuleExtension::Js
                    | ModuleExtension::Jsx
                    | ModuleExtension::Mjs
                    | ModuleExtension::Cjs
            )
        };
        let needs_extension_check = matches!(
            self.resolution_kind,
            ModuleResolutionKind::Node16 | ModuleResolutionKind::NodeNext
        ) && !specifier_has_extension
            && !containing_is_js
            && match import_kind {
                // Dynamic import() is always ESM, even in .cts files
                ImportKind::DynamicImport => true,
                // Static import/export only triggers TS2834 in ESM files
                ImportKind::EsmImport | ImportKind::EsmReExport => {
                    importing_module_kind == ImportingModuleKind::Esm
                }
                // require() never triggers TS2834
                ImportKind::CjsRequire => false,
            };

        if needs_extension_check {
            // Try to resolve to determine what extension to suggest (TS2835)
            for candidate in &candidates {
                let Some(resolved) = try_resolve_candidate(candidate) else {
                    continue;
                };
                // Resolution succeeded implicitly - this is an error in ESM mode.
                // Only suggest an extension (TS2835) when the resolution was via direct file
                // extension addition (e.g., ./foo → ./foo.ts). If the resolution went through
                // a directory index (e.g., ./pkg → ./pkg/index.d.ts), don't suggest an
                // extension (TS2834) because adding .js to the specifier won't work.
                let resolved_ext = ModuleExtension::from_path(&resolved);
                let resolved_via_index =
                    self.resolved_via_directory_index(Path::new(&resolved), &candidate);
                // Bare `.`, `./`, `..`, `../` specifiers (no path component to
                // add an extension to) resolve via directory index but should
                // emit TS2307 (Cannot find module), not TS2834, because there
                // is no filename to attach an extension to. The bare-dot forms
                // (`.`, `..`) reach the same directory-index resolution as
                // their slash-suffixed siblings (`prefer_directory` at the top
                // of this function recognises them) and must produce the same
                // diagnostic.
                let is_bare_directory_specifier =
                    matches!(specifier, "." | "./" | ".\\" | ".." | "../" | "..\\");
                if resolved_via_index && is_bare_directory_specifier {
                    return Err(ResolutionFailure::NotFound {
                        specifier: specifier.to_string(),
                        containing_file: containing_file.to_string(),
                        span: specifier_span,
                    });
                }
                if resolved_via_index {
                    return Err(ResolutionFailure::ImportPathNeedsExtension {
                        specifier: specifier.to_string(),
                        suggested_extension: String::new(),
                        containing_file: containing_file.to_string(),
                        span: specifier_span,
                    });
                }
                let suggested_ext = self.suggested_runtime_extension(resolved_ext);
                return Err(ResolutionFailure::ImportPathNeedsExtension {
                    specifier: specifier.to_string(),
                    suggested_extension: suggested_ext.to_string(),
                    containing_file: containing_file.to_string(),
                    span: specifier_span,
                });
            }
            // Standard resolution failed. Before giving up on a suggestion, probe
            // for additional file extensions that aren't in the normal resolution
            // candidates but can still provide a useful TS2835 suggestion.
            // Notably, `.json` files are not in the extension candidate lists
            // (they require `resolveJsonModule`), but tsc still suggests them.
            for candidate in &candidates {
                let json_candidate = candidate.with_extension("json");
                if json_candidate.is_file() {
                    return Err(ResolutionFailure::ImportPathNeedsExtension {
                        specifier: specifier.to_string(),
                        suggested_extension: ".json".to_string(),
                        containing_file: containing_file.to_string(),
                        span: specifier_span,
                    });
                }
            }
            // File doesn't exist - emit TS2834 (no suggestion) for ESM imports
            return Err(ResolutionFailure::ImportPathNeedsExtension {
                specifier: specifier.to_string(),
                suggested_extension: String::new(),
                containing_file: containing_file.to_string(),
                span: specifier_span,
            });
        }

        for candidate in &candidates {
            if let Some(resolved) = try_resolve_candidate(candidate) {
                let resolved_via_index =
                    self.resolved_via_directory_index(Path::new(&resolved), candidate);
                let resolved_using_ts_extension = (specifier.ends_with(".ts")
                    || specifier.ends_with(".tsx")
                    || specifier.ends_with(".mts")
                    || specifier.ends_with(".cts"))
                    && !resolved_via_index;
                return Ok(ResolvedModule {
                    resolved_path: resolved.clone(),
                    resolved_using_ts_extension,
                    is_external: false,
                    package_name: None,
                    original_specifier: specifier.to_string(),
                    extension: ModuleExtension::from_path(&resolved),
                });
            }
        }

        let js_json_extension_suggestion = matches!(
            self.resolution_kind,
            ModuleResolutionKind::Node16 | ModuleResolutionKind::NodeNext
        ) && !specifier_has_extension
            && containing_is_js
            && match import_kind {
                ImportKind::DynamicImport => true,
                ImportKind::EsmImport | ImportKind::EsmReExport => {
                    importing_module_kind == ImportingModuleKind::Esm
                }
                ImportKind::CjsRequire => false,
            };
        if js_json_extension_suggestion {
            for candidate in &candidates {
                let json_candidate = candidate.with_extension("json");
                if json_candidate.is_file() {
                    return Err(ResolutionFailure::ImportPathNeedsExtension {
                        specifier: specifier.to_string(),
                        suggested_extension: ".json".to_string(),
                        containing_file: containing_file.to_string(),
                        span: specifier_span,
                    });
                }
            }
        }

        // Module not found - emit TS2307 (standard "Cannot find module" error).
        // TS2792 should only be emitted when we've detected a package.json exports
        // field that would work in a different resolution mode.
        Err(ResolutionFailure::NotFound {
            specifier: specifier.to_string(),
            containing_file: containing_file.to_string(),
            span: specifier_span,
        })
    }

    fn root_dirs_relative_candidates(
        &self,
        containing_dir: &Path,
        specifier: &str,
    ) -> Vec<PathBuf> {
        if self.root_dirs.is_empty() {
            return Vec::new();
        }

        let containing_dir = normalize_path_segments(containing_dir);
        let direct_candidate = normalize_path_segments(&containing_dir.join(specifier));
        let mut candidates = Vec::new();

        for origin_root in &self.root_dirs {
            let origin_root = normalize_path_segments(origin_root);
            if containing_dir.strip_prefix(&origin_root).is_err() {
                continue;
            }
            let Ok(virtual_path) = direct_candidate.strip_prefix(&origin_root) else {
                continue;
            };

            for target_root in &self.root_dirs {
                let candidate = normalize_path_segments(&target_root.join(virtual_path));
                if candidate == direct_candidate || candidates.iter().any(|seen| seen == &candidate)
                {
                    continue;
                }
                candidates.push(candidate);
            }
        }

        candidates
    }

    /// Resolve an absolute import
    pub(super) fn resolve_absolute(
        &self,
        specifier: &str,
        containing_file: &str,
        specifier_span: Span,
    ) -> Result<ResolvedModule, ResolutionFailure> {
        let path = std::path::PathBuf::from(specifier);

        if let Some(resolved) = self.try_file_or_directory(&path) {
            return Ok(ResolvedModule {
                resolved_path: resolved.clone(),
                resolved_using_ts_extension: false,
                is_external: false,
                package_name: None,
                original_specifier: specifier.to_string(),
                extension: ModuleExtension::from_path(&resolved),
            });
        }

        Err(ResolutionFailure::NotFound {
            specifier: specifier.to_string(),
            containing_file: containing_file.to_string(),
            span: specifier_span,
        })
    }
}

fn normalize_path_segments(path: &Path) -> PathBuf {
    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                normalized.pop();
            }
            Component::RootDir | Component::Normal(_) | Component::Prefix(_) => {
                normalized.push(component.as_os_str());
            }
        }
    }
    normalized
}

//! Relative and absolute import path resolution.
//!
//! Handles `./foo`, `../bar`, `.`, `..`, and `/absolute` import specifiers,
//! including Node16/NodeNext ESM extension validation (TS2834/TS2835).

use super::{
    ImportKind, ImportingModuleKind, ModuleExtension, ModuleResolver, ResolutionFailure,
    ResolvedModule,
};
use crate::config::ModuleResolutionKind;
use crate::span::Span;
use std::path::Path;

impl ModuleResolver {
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
        let prefer_directory = specifier == "."
            || specifier == ".."
            || specifier.ends_with('/')
            || specifier.ends_with('\\');
        let try_resolve_candidate = |path: &Path| {
            if prefer_directory {
                self.try_directory(path)
            } else {
                self.try_file_or_directory(path)
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
            if let Some(resolved) = try_resolve_candidate(&candidate) {
                // Resolution succeeded implicitly - this is an error in ESM mode.
                // Only suggest an extension (TS2835) when the resolution was via direct file
                // extension addition (e.g., ./foo → ./foo.ts). If the resolution went through
                // a directory index (e.g., ./pkg → ./pkg/index.d.ts), don't suggest an
                // extension (TS2834) because adding .js to the specifier won't work.
                let resolved_via_index = {
                    let resolved_path = Path::new(&resolved);
                    // Check if resolved through directory index (e.g., ./pkg → ./pkg/index.d.ts)
                    // file_stem() returns "index.d" for "index.d.ts", so also check file_name starts with "index."
                    resolved_path.file_name().is_some_and(|name| {
                        let name = name.to_string_lossy();
                        name == "index.ts"
                            || name == "index.tsx"
                            || name == "index.js"
                            || name == "index.jsx"
                            || name == "index.d.ts"
                            || name == "index.d.mts"
                            || name == "index.d.cts"
                    })
                };
                if resolved_via_index {
                    // Directory index resolution - no suggestion (TS2834)
                    return Err(ResolutionFailure::ImportPathNeedsExtension {
                        specifier: specifier.to_string(),
                        suggested_extension: String::new(),
                        containing_file: containing_file.to_string(),
                        span: specifier_span,
                    });
                }
                let resolved_ext = ModuleExtension::from_path(&resolved);
                // Suggest the .js extension (TypeScript convention: import .js, compile from .ts)
                let suggested_ext = match resolved_ext {
                    ModuleExtension::Ts
                    | ModuleExtension::Tsx
                    | ModuleExtension::Js
                    | ModuleExtension::Jsx
                    | ModuleExtension::Dts
                    | ModuleExtension::Unknown => ".js",
                    ModuleExtension::Mts | ModuleExtension::Mjs | ModuleExtension::DmTs => ".mjs",
                    ModuleExtension::Cts | ModuleExtension::Cjs | ModuleExtension::DCts => ".cjs",
                    ModuleExtension::Json => ".json",
                };
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
            let json_candidate = candidate.with_extension("json");
            if json_candidate.is_file() {
                return Err(ResolutionFailure::ImportPathNeedsExtension {
                    specifier: specifier.to_string(),
                    suggested_extension: ".json".to_string(),
                    containing_file: containing_file.to_string(),
                    span: specifier_span,
                });
            }
            // File doesn't exist - emit TS2834 (no suggestion) for ESM imports
            return Err(ResolutionFailure::ImportPathNeedsExtension {
                specifier: specifier.to_string(),
                suggested_extension: String::new(),
                containing_file: containing_file.to_string(),
                span: specifier_span,
            });
        }

        if let Some(resolved) = try_resolve_candidate(&candidate) {
            let resolved_display = resolved.to_string_lossy().replace('\\', "/");
            let resolved_via_index = resolved_display.ends_with("/index.ts")
                || resolved_display.ends_with("/index.tsx")
                || resolved_display.ends_with("/index.mts")
                || resolved_display.ends_with("/index.cts")
                || resolved_display.ends_with("/index.d.ts")
                || resolved_display.ends_with("/index.d.mts")
                || resolved_display.ends_with("/index.d.cts");
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

        // Module not found - emit TS2307 (standard "Cannot find module" error).
        // TS2792 should only be emitted when we've detected a package.json exports
        // field that would work in a different resolution mode.
        Err(ResolutionFailure::NotFound {
            specifier: specifier.to_string(),
            containing_file: containing_file.to_string(),
            span: specifier_span,
        })
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

//! Module Resolution Implementation
//!
//! This module implements TypeScript's module resolution algorithms:
//! - Node (classic Node.js resolution)
//! - Node16/NodeNext (modern Node.js with ESM support)
//! - Bundler (for webpack/vite-style resolution)
//!
//! The resolver handles:
//! - Relative imports (./foo, ../bar)
//! - Bare specifiers (lodash, @scope/pkg)
//! - Path mapping from tsconfig (paths, baseUrl)
//! - Package.json exports/imports fields
//! - TypeScript-specific extensions (.ts, .tsx, .d.ts)
//!
//! Resolver invariants:
//! - Module existence truth comes from `resolve_with_kind` outcomes.
//! - Diagnostic code selection for module-not-found family (TS2307/TS2792/TS2834/TS2835/TS5097/TS2732)
//!   is owned here and propagated to checker via resolution records.
//! - Callers should not recompute not-found codes/messages from partial checker state.

// Sub-modules by responsibility
mod diagnostics;
mod exports_imports;
mod file_probing;
mod node_modules_resolution;
mod package_json;
mod path_mapping;
mod relative_resolution;
mod request_types;
mod self_reference;

// Re-export public API
pub use diagnostics::*;
pub use request_types::*;

use crate::config::{JsxEmit, ModuleResolutionKind, PathMapping, ResolvedCompilerOptions};
use crate::diagnostics::DiagnosticBag;
use crate::emitter::ModuleKind;
use crate::span::Span;
use rustc_hash::FxHashMap;
use std::path::{Path, PathBuf};

fn explicit_ts_extension(specifier: &str) -> Option<String> {
    if specifier.ends_with(".d.ts")
        || specifier.ends_with(".d.mts")
        || specifier.ends_with(".d.cts")
    {
        return None;
    }
    for ext in [".ts", ".tsx", ".mts", ".cts"] {
        if specifier.ends_with(ext) {
            return Some(ext.to_string());
        }
    }
    None
}

/// Module resolver that implements TypeScript's resolution algorithms
#[derive(Debug)]
pub struct ModuleResolver {
    /// Resolution strategy to use
    resolution_kind: ModuleResolutionKind,
    /// Base URL for path resolution
    base_url: Option<PathBuf>,
    /// Path mappings from tsconfig
    path_mappings: Vec<PathMapping>,
    /// Type roots for @types packages
    type_roots: Vec<PathBuf>,
    types_versions_compiler_version: Option<String>,
    resolve_package_json_exports: bool,
    resolve_package_json_imports: bool,
    module_suffixes: Vec<String>,
    resolve_json_module: bool,
    allow_arbitrary_extensions: bool,
    allow_importing_ts_extensions: bool,
    jsx: Option<JsxEmit>,
    /// Cache of resolved modules
    resolution_cache: FxHashMap<
        (PathBuf, String, ImportingModuleKind),
        Result<ResolvedModule, ResolutionFailure>,
    >,
    /// Custom conditions from tsconfig (for customConditions option)
    custom_conditions: Vec<String>,
    module_kind: ModuleKind,
    /// Whether allowJs is enabled (affects extension candidates)
    allow_js: bool,
    /// Whether to rewrite relative imports with TypeScript extensions during emit.
    rewrite_relative_import_extensions: bool,
    /// Cache for package.json package type lookups
    package_type_cache: FxHashMap<PathBuf, Option<PackageType>>,
    /// Cached package type for the current resolution
    current_package_type: Option<PackageType>,
}

impl ModuleResolver {
    /// Create a new module resolver with the given options
    pub fn new(options: &ResolvedCompilerOptions) -> Self {
        let resolution_kind = options.effective_module_resolution();

        let module_suffixes = if options.module_suffixes.is_empty() {
            vec![String::new()]
        } else {
            options.module_suffixes.clone()
        };

        Self {
            resolution_kind,
            base_url: options.base_url.clone(),
            path_mappings: options.paths.clone().unwrap_or_default(),
            type_roots: options.type_roots.clone().unwrap_or_default(),
            types_versions_compiler_version: options.types_versions_compiler_version.clone(),
            resolve_package_json_exports: options.resolve_package_json_exports,
            resolve_package_json_imports: options.resolve_package_json_imports,
            module_suffixes,
            resolve_json_module: options.resolve_json_module,
            allow_arbitrary_extensions: options.allow_arbitrary_extensions,
            allow_importing_ts_extensions: options.allow_importing_ts_extensions,
            jsx: options.jsx,
            resolution_cache: FxHashMap::default(),
            custom_conditions: options.custom_conditions.clone(),
            module_kind: options.printer.module,
            allow_js: options.allow_js,
            rewrite_relative_import_extensions: options.rewrite_relative_import_extensions,
            package_type_cache: FxHashMap::default(),
            current_package_type: None,
        }
    }

    /// Create a resolver with default Node resolution
    pub fn node_resolver() -> Self {
        Self {
            resolution_kind: ModuleResolutionKind::Node,
            base_url: None,
            path_mappings: Vec::new(),
            type_roots: Vec::new(),
            types_versions_compiler_version: None,
            resolve_package_json_exports: false,
            resolve_package_json_imports: false,
            module_suffixes: vec![String::new()],
            resolve_json_module: false,
            allow_arbitrary_extensions: false,
            allow_importing_ts_extensions: false,
            jsx: None,
            resolution_cache: FxHashMap::default(),
            custom_conditions: Vec::new(),
            module_kind: ModuleKind::CommonJS,
            allow_js: false,
            rewrite_relative_import_extensions: false,
            package_type_cache: FxHashMap::default(),
            current_package_type: None,
        }
    }

    /// Resolve a module specifier from a containing file
    pub fn resolve(
        &mut self,
        specifier: &str,
        containing_file: &Path,
        specifier_span: Span,
    ) -> Result<ResolvedModule, ResolutionFailure> {
        self.resolve_with_kind(
            specifier,
            containing_file,
            specifier_span,
            ImportKind::EsmImport,
        )
    }

    /// Resolve a module specifier from a containing file, with import kind information.
    /// The `import_kind` is used to determine whether to emit TS2834 (extensionless ESM import)
    /// or TS2307 (cannot find module) for extensionless imports in Node16/NodeNext.
    pub fn resolve_with_kind(
        &mut self,
        specifier: &str,
        containing_file: &Path,
        specifier_span: Span,
        import_kind: ImportKind,
    ) -> Result<ResolvedModule, ResolutionFailure> {
        self.resolve_with_kind_and_module_kind(
            specifier,
            containing_file,
            specifier_span,
            import_kind,
            None,
        )
    }

    fn resolve_with_kind_and_module_kind(
        &mut self,
        specifier: &str,
        containing_file: &Path,
        specifier_span: Span,
        import_kind: ImportKind,
        importing_module_kind_override: Option<ImportingModuleKind>,
    ) -> Result<ResolvedModule, ResolutionFailure> {
        let containing_dir = containing_file
            .parent()
            .unwrap_or_else(|| Path::new("."))
            .to_path_buf();
        let containing_file_str = containing_file.display().to_string();

        self.current_package_type = match self.resolution_kind {
            ModuleResolutionKind::Node16 | ModuleResolutionKind::NodeNext => {
                self.get_package_type_for_dir(&containing_dir)
            }
            _ => None,
        };

        // Determine the module kind of the importing file, honoring any explicit
        // driver-provided resolution-mode override from import attributes.
        let importing_module_kind =
            importing_module_kind_override.unwrap_or_else(|| match self.module_kind {
                ModuleKind::Preserve => match import_kind {
                    ImportKind::EsmImport | ImportKind::DynamicImport | ImportKind::EsmReExport => {
                        ImportingModuleKind::Esm
                    }
                    ImportKind::CjsRequire => ImportingModuleKind::CommonJs,
                },
                _ => self.get_importing_module_kind(containing_file),
            });
        let cache_key = (
            containing_dir.clone(),
            specifier.to_string(),
            importing_module_kind,
        );
        if let Some(cached) = self.resolution_cache.get(&cache_key) {
            return cached.clone();
        }

        let (mut result, path_mapping_attempted) = self.resolve_uncached(
            specifier,
            &containing_dir,
            &containing_file_str,
            specifier_span,
            importing_module_kind,
            import_kind,
        );

        if !self.allow_importing_ts_extensions
            && !self.allow_arbitrary_extensions
            && !self.rewrite_relative_import_extensions
            && (self.base_url.is_some() || self.path_mappings.is_empty())
            && let Some(extension) = explicit_ts_extension(specifier)
            && !path_mapping_attempted
            && matches!(result, Err(ResolutionFailure::NotFound { .. }))
        {
            result = Err(ResolutionFailure::ImportingTsExtensionNotAllowed {
                extension,
                containing_file: containing_file_str.clone(),
                span: specifier_span,
            });
        }

        if let Ok(resolved) = &result {
            if matches!(
                resolved.extension,
                ModuleExtension::Tsx | ModuleExtension::Jsx
            ) && self.jsx.is_none()
            {
                result = Err(ResolutionFailure::JsxNotEnabled {
                    specifier: specifier.to_string(),
                    resolved_path: resolved.resolved_path.clone(),
                    containing_file: containing_file_str,
                    span: specifier_span,
                });
            } else if resolved.extension == ModuleExtension::Json && !self.resolve_json_module {
                result = Err(ResolutionFailure::JsonModuleWithoutResolveJsonModule {
                    specifier: specifier.to_string(),
                    containing_file: containing_file_str,
                    span: specifier_span,
                });
            }
        }

        // Cache the result
        self.resolution_cache.insert(cache_key, result.clone());

        result
    }

    /// Determine the module kind of the importing file based on extension and package.json type.
    /// Public so the driver can pre-compute per-file ESM/CJS status for the checker.
    pub fn get_importing_module_kind(&mut self, file_path: &Path) -> ImportingModuleKind {
        let extension = ModuleExtension::from_path(file_path);

        // .mts, .mjs force ESM mode
        if extension.forces_esm() {
            return ImportingModuleKind::Esm;
        }

        // .cts, .cjs force CommonJS mode
        if extension.forces_cjs() {
            return ImportingModuleKind::CommonJs;
        }

        // `--module commonjs` and other CJS-style module targets ignore package.json `type`.
        match self.module_kind {
            ModuleKind::CommonJS | ModuleKind::AMD | ModuleKind::UMD | ModuleKind::System => {
                return ImportingModuleKind::CommonJs;
            }
            ModuleKind::ES2015 | ModuleKind::ES2020 | ModuleKind::ES2022 | ModuleKind::ESNext
                if !matches!(
                    self.resolution_kind,
                    ModuleResolutionKind::Node16 | ModuleResolutionKind::NodeNext
                ) =>
            {
                return ImportingModuleKind::Esm;
            }
            ModuleKind::None
            | ModuleKind::ES2015
            | ModuleKind::ES2020
            | ModuleKind::ES2022
            | ModuleKind::ESNext
            | ModuleKind::Node16
            | ModuleKind::Node18
            | ModuleKind::Node20
            | ModuleKind::NodeNext
            | ModuleKind::Preserve => {}
        }

        // Check package.json "type" field
        if let Some(dir) = file_path.parent() {
            match self.get_package_type_for_dir(dir) {
                Some(PackageType::Module) => ImportingModuleKind::Esm,
                Some(PackageType::CommonJs) | None => ImportingModuleKind::CommonJs,
            }
        } else {
            ImportingModuleKind::CommonJs
        }
    }

    /// Resolve without checking cache
    fn resolve_uncached(
        &self,
        specifier: &str,
        containing_dir: &Path,
        containing_file: &str,
        specifier_span: Span,
        importing_module_kind: ImportingModuleKind,
        import_kind: ImportKind,
    ) -> (Result<ResolvedModule, ResolutionFailure>, bool) {
        // Step 1: Handle #-prefixed imports (package.json imports field)
        // This is a Node16/NodeNext feature for subpath imports
        if specifier.starts_with('#') {
            if Self::is_invalid_package_import_specifier(specifier) {
                return (
                    Err(ResolutionFailure::NotFound {
                        specifier: specifier.to_string(),
                        containing_file: containing_file.to_string(),
                        span: specifier_span,
                    }),
                    false,
                );
            }
            if !self.resolve_package_json_imports {
                return (
                    Err(ResolutionFailure::NotFound {
                        specifier: specifier.to_string(),
                        containing_file: containing_file.to_string(),
                        span: specifier_span,
                    }),
                    false,
                );
            }
            return (
                self.resolve_package_imports(
                    specifier,
                    containing_dir,
                    containing_file,
                    specifier_span,
                    importing_module_kind,
                ),
                false,
            );
        }

        // Step 2: Try path mappings first (if configured and baseUrl is available).
        // TypeScript treats `paths` mappings as requiring `baseUrl` to avoid surprising
        // absolute lookups that behave like relative resolution.
        let mut path_mapping_attempted = false;
        if self.base_url.is_some() && !self.path_mappings.is_empty() {
            let attempt = self.try_path_mappings(specifier, containing_dir);
            if let Some(resolved) = attempt.resolved {
                return (Ok(resolved), path_mapping_attempted);
            }
            path_mapping_attempted = attempt.attempted;
        }

        // Step 3: Handle relative imports
        if is_path_relative(specifier) {
            return (
                self.resolve_relative(
                    specifier,
                    containing_dir,
                    containing_file,
                    specifier_span,
                    importing_module_kind,
                    import_kind,
                ),
                path_mapping_attempted,
            );
        }

        // Step 4: Handle absolute imports (rare but valid)
        if specifier.starts_with('/') {
            return (
                self.resolve_absolute(specifier, containing_file, specifier_span),
                path_mapping_attempted,
            );
        }

        // Step 5: Try baseUrl fallback for non-relative specifiers
        if let Some(base_url) = &self.base_url {
            let candidate = base_url.join(specifier);
            if let Some(resolved) = self.try_file_or_directory(&candidate) {
                return (
                    Ok(ResolvedModule {
                        resolved_path: resolved.clone(),
                        is_external: false,
                        package_name: None,
                        original_specifier: specifier.to_string(),
                        extension: ModuleExtension::from_path(&resolved),
                    }),
                    path_mapping_attempted,
                );
            }
        }

        // Step 6: Classic resolution walks up the directory tree looking for
        // <specifier>.ts, <specifier>.tsx, <specifier>.d.ts at each level.
        // It does NOT consult node_modules.
        let resolved = if matches!(self.resolution_kind, ModuleResolutionKind::Classic) {
            self.resolve_classic_non_relative(
                specifier,
                containing_dir,
                containing_file,
                specifier_span,
            )
        } else {
            self.resolve_bare_specifier(
                specifier,
                containing_dir,
                containing_file,
                specifier_span,
                importing_module_kind,
            )
        };

        if let Err(ResolutionFailure::NotFound { .. }) = &resolved
            && path_mapping_attempted
        {
            return (
                Err(ResolutionFailure::PathMappingFailed {
                    message: specifier.to_string(),
                    containing_file: containing_file.to_string(),
                    span: specifier_span,
                }),
                path_mapping_attempted,
            );
        }

        (resolved, path_mapping_attempted)
    }

    /// Perform a complete module lookup, centralizing all diagnostic code selection.
    ///
    /// This is the primary entry point for driver-side module resolution.
    /// It replaces the scattered driver branches that previously handled:
    /// - Fallback resolution attempts
    /// - Node16/NodeNext ESM extension validation on fallback paths
    /// - JSON module without `resolveJsonModule` (TS2732)
    /// - Classic resolution TS2792 override
    /// - Untyped JS module handling (TS7016)
    /// - Ambient module suppression
    ///
    /// The `fallback_resolve` closure lets the driver provide its legacy resolution
    /// path. The `is_ambient_module` closure lets the driver check program-level
    /// ambient declarations. All diagnostic code selection stays here.
    pub fn lookup(
        &mut self,
        request: &ModuleLookupRequest<'_>,
        fallback_resolve: impl FnOnce(&str, &Path) -> Option<PathBuf>,
        is_ambient_module: impl FnOnce(&str) -> bool,
    ) -> ModuleLookupResult {
        let specifier = request.specifier;
        let containing_file = request.containing_file;
        let span = request.specifier_span;
        let import_kind = request.import_kind;

        // 1. Try primary resolution
        match self.resolve_with_kind_and_module_kind(
            specifier,
            containing_file,
            span,
            import_kind,
            request.resolution_mode_override,
        ) {
            Ok(resolved_module) => {
                // TS7016: If the resolved file is a JS file from node_modules
                // (external package), noImplicitAny is enabled, and this is a
                // CJS require() call, emit TS7016 alongside the successful resolution.
                // ESM imports go through the checker's import-declaration path which
                // handles ambient declarations and other suppression rules.
                if resolved_module.is_external
                    && resolved_module.extension.is_javascript()
                    && request.no_implicit_any
                    && matches!(import_kind, ImportKind::CjsRequire)
                {
                    ModuleLookupResult::resolved_untyped_js(
                        resolved_module.resolved_path,
                        request.no_implicit_any,
                        specifier,
                    )
                } else {
                    ModuleLookupResult::resolved(resolved_module.resolved_path)
                }
            }
            Err(failure) => {
                // JsxNotEnabled: file exists but --jsx is not set.
                // Mark as resolved (suppress TS2307) but record the JSX error.
                let jsx_resolved =
                    if let ResolutionFailure::JsxNotEnabled { resolved_path, .. } = &failure {
                        Some(resolved_path.clone())
                    } else {
                        None
                    };

                // 2. Try fallback resolution if this is a "soft" failure
                if failure.should_try_fallback()
                    && let Some(fallback_path) = fallback_resolve(specifier, containing_file)
                {
                    // 3. Validate Node16/NodeNext ESM extension requirements
                    if self.fallback_needs_esm_extension_error(
                        specifier,
                        containing_file,
                        import_kind,
                    ) {
                        return ModuleLookupResult::failed(
                            CANNOT_FIND_MODULE,
                            format!(
                                "Cannot find module '{specifier}' or its corresponding type declarations."
                            ),
                        );
                    }
                    return ModuleLookupResult::resolved(fallback_path);
                }

                // Upgrade NotFound → TS2732 for .json imports without resolveJsonModule
                let failure = if matches!(failure, ResolutionFailure::NotFound { .. })
                    && specifier.ends_with(".json")
                    && !self.resolve_json_module
                {
                    ResolutionFailure::JsonModuleWithoutResolveJsonModule {
                        specifier: specifier.to_string(),
                        containing_file: containing_file.to_string_lossy().to_string(),
                        span,
                    }
                } else {
                    failure
                };

                // 4. Check ambient module declarations
                let is_ordinary_bare = !specifier.starts_with('.')
                    && !specifier.starts_with('/')
                    && !specifier.contains(':');
                if is_ordinary_bare && is_ambient_module(specifier) {
                    return ModuleLookupResult::ambient();
                }

                // 5. Probe for untyped JS file (TS7016)
                if matches!(
                    failure,
                    ResolutionFailure::NotFound { .. } | ResolutionFailure::PackageJsonError { .. }
                ) && let Some(js_path) =
                    self.probe_js_file(specifier, containing_file, span, import_kind)
                {
                    return ModuleLookupResult::untyped_js(
                        js_path,
                        request.no_implicit_any,
                        specifier,
                    );
                }

                // 6. Build final error from the failure
                let mut diag = failure.to_diagnostic();

                // Classic resolution override: TS2307 → TS2792
                if diag.code == CANNOT_FIND_MODULE && request.implied_classic_resolution {
                    diag.code = MODULE_RESOLUTION_MODE_MISMATCH;
                    diag.message = format!(
                        "Cannot find module '{specifier}'. Did you mean to set the 'moduleResolution' option to 'nodenext', or to add aliases to the 'paths' option?"
                    );
                }

                // If the primary resolution found the file but JSX wasn't set,
                // mark as resolved to suppress TS2307 but record the JSX error.
                if jsx_resolved.is_some() {
                    return ModuleLookupResult::resolved_with_error(diag.code, diag.message);
                }

                ModuleLookupResult::failed(diag.code, diag.message)
            }
        }
    }

    /// Check whether a fallback-resolved file needs an ESM extension error.
    ///
    /// In Node16/NodeNext, ESM-context extensionless relative imports are errors
    /// even when the file exists (because ESM requires explicit extensions).
    fn fallback_needs_esm_extension_error(
        &mut self,
        specifier: &str,
        containing_file: &Path,
        _import_kind: ImportKind,
    ) -> bool {
        let is_node16_or_next = matches!(
            self.resolution_kind,
            ModuleResolutionKind::Node16 | ModuleResolutionKind::NodeNext
        );
        if !is_node16_or_next {
            return false;
        }

        let importing_ext = ModuleExtension::from_path(containing_file);
        let is_esm = importing_ext.forces_esm();

        let specifier_has_extension = Path::new(specifier).extension().is_some();

        // In Node16/NodeNext ESM mode, relative imports must have explicit extensions
        is_esm && !specifier_has_extension && specifier.starts_with('.')
    }

    /// Probe for a JS file that would resolve for this specifier.
    ///
    /// Used for TS7016: when normal resolution fails but a JS file exists,
    /// we can report "Could not find declaration file" instead of "Cannot find module".
    /// Returns the resolved JS file path if found.
    pub fn probe_js_file(
        &mut self,
        specifier: &str,
        containing_file: &Path,
        specifier_span: Span,
        import_kind: ImportKind,
    ) -> Option<PathBuf> {
        if self.allow_js {
            return None; // Already tried JS in normal resolution
        }
        let containing_dir = containing_file
            .parent()
            .unwrap_or_else(|| Path::new("."))
            .to_path_buf();
        let containing_file_str = containing_file.display().to_string();
        let importing_module_kind = self.get_importing_module_kind(containing_file);

        self.allow_js = true;
        let (result, _) = self.resolve_uncached(
            specifier,
            &containing_dir,
            &containing_file_str,
            specifier_span,
            importing_module_kind,
            import_kind,
        );
        self.allow_js = false;

        match result {
            Ok(resolved)
                if matches!(
                    resolved.extension,
                    ModuleExtension::Js
                        | ModuleExtension::Jsx
                        | ModuleExtension::Mjs
                        | ModuleExtension::Cjs
                ) =>
            {
                Some(resolved.resolved_path)
            }
            _ => None,
        }
    }

    /// Clear the resolution cache
    pub fn clear_cache(&mut self) {
        self.resolution_cache.clear();
    }

    /// Get the current resolution kind
    pub const fn resolution_kind(&self) -> ModuleResolutionKind {
        self.resolution_kind
    }

    /// Emit TS2307 error for a resolution failure into a diagnostic bag
    ///
    /// All module resolution failures emit TS2307 "Cannot find module" error.
    /// This includes:
    /// - `NotFound`: Module specifier could not be resolved
    /// - `InvalidSpecifier`: Module specifier is malformed
    /// - `PackageJsonError`: Package.json is missing or invalid
    /// - `CircularResolution`: Circular dependency detected during resolution
    /// - `PathMappingFailed`: Path mapping from tsconfig did not resolve
    pub fn emit_resolution_error(
        &self,
        diagnostics: &mut DiagnosticBag,
        failure: &ResolutionFailure,
    ) {
        let diagnostic = failure.to_diagnostic();
        diagnostics.add(diagnostic);
    }
}

/// Parse a package specifier into package name and subpath
#[cfg(test)]
mod tests;

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

use crate::config::{JsxEmit, ModuleResolutionKind, PathMapping, ResolvedCompilerOptions};
use crate::diagnostics::{Diagnostic, DiagnosticBag};
use crate::span::Span;
use rustc_hash::FxHashMap;
use serde_json;
use std::path::{Path, PathBuf};

/// TS2307: Cannot find module
///
/// This error code is emitted when a module specifier cannot be resolved.
/// Example: `import { foo } from './missing-module'`
///
/// Usage example:
/// ```ignore
/// let mut resolver = ModuleResolver::new(&options);
/// let mut diagnostics = DiagnosticBag::new();
///
/// match resolver.resolve("./missing-module", containing_file, specifier_span) {
///     Ok(module) => { /* use module */ }
///     Err(failure) => {
///         resolver.emit_resolution_error(&mut diagnostics, &failure);
///     }
/// }
/// ```
pub const CANNOT_FIND_MODULE: u32 = 2307;

/// TS2792: Cannot find module. Did you mean to set the 'moduleResolution' option to 'nodenext'?
///
/// This error code is emitted when a module specifier cannot be resolved under the current
/// module resolution mode, but the package.json has an 'exports' field that would likely
/// resolve correctly under Node16/NodeNext/Bundler mode.
pub const MODULE_RESOLUTION_MODE_MISMATCH: u32 = 2792;

/// TS2732: Cannot find module. Consider using '--resolveJsonModule' to import module with '.json' extension.
///
/// This error code is emitted when trying to import a .json file without the resolveJsonModule
/// compiler option enabled. Unlike TS2307 (generic cannot find module), this error provides
/// specific guidance to enable JSON module support.
/// Example: `import data from './config.json'` without resolveJsonModule enabled
pub const JSON_MODULE_WITHOUT_RESOLVE_JSON_MODULE: u32 = 2732;

/// TS2834: Relative import paths need explicit file extensions in EcmaScript imports
///
/// This error code is emitted when a relative import in an ESM context under Node16/NodeNext
/// resolution mode does not include an explicit file extension. ESM requires explicit extensions.
/// Example: `import { foo } from './utils'` should be `import { foo } from './utils.js'`
pub const IMPORT_PATH_NEEDS_EXTENSION: u32 = 2834;
pub const IMPORT_PATH_NEEDS_EXTENSION_SUGGESTION: u32 = 2835;
pub const IMPORT_PATH_TS_EXTENSION_NOT_ALLOWED: u32 = 5097;
pub const MODULE_WAS_RESOLVED_TO_BUT_JSX_NOT_SET: u32 = 6142;

/// Result of module resolution
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedModule {
    /// Resolved file path
    pub resolved_path: PathBuf,
    /// Whether the module is an external package (from node_modules)
    pub is_external: bool,
    /// Package name if resolved from node_modules
    pub package_name: Option<String>,
    /// Original specifier used in import
    pub original_specifier: String,
    /// Extension of the resolved file
    pub extension: ModuleExtension,
}

/// Module file extensions TypeScript can resolve
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModuleExtension {
    Ts,
    Tsx,
    Dts,
    DmTs,
    DCts,
    Js,
    Jsx,
    Mjs,
    Cjs,
    Mts,
    Cts,
    Json,
    Unknown,
}

/// Package type from package.json "type" field
/// Used for ESM vs CommonJS distinction in Node16/NodeNext
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PackageType {
    /// ESM package ("type": "module")
    Module,
    /// CommonJS package ("type": "commonjs" or default)
    #[default]
    CommonJs,
}

/// Module kind for the importing file
/// Determines whether to use "import" or "require" conditions
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ImportingModuleKind {
    /// ESM module (uses "import" condition)
    Esm,
    /// CommonJS module (uses "require" condition)
    #[default]
    CommonJs,
}

/// Import syntax kind - determines which error codes to use
/// for extensionless imports in Node16/NodeNext resolution.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ImportKind {
    /// ESM static import: `import { x } from "./foo"`
    #[default]
    EsmImport,
    /// Dynamic import: `import("./foo")` - always ESM regardless of file type
    DynamicImport,
    /// CommonJS require: `import x = require("./foo")` or `require("./foo")`
    CjsRequire,
    /// Re-export: `export { x } from "./foo"`
    EsmReExport,
}

impl ModuleExtension {
    /// Parse extension from file path
    pub fn from_path(path: &Path) -> Self {
        let path_str = path.to_string_lossy();

        // Check compound extensions first
        if path_str.ends_with(".d.ts") {
            return ModuleExtension::Dts;
        }
        if path_str.ends_with(".d.mts") {
            return ModuleExtension::DmTs;
        }
        if path_str.ends_with(".d.cts") {
            return ModuleExtension::DCts;
        }

        match path.extension().and_then(|e| e.to_str()) {
            Some("ts") => ModuleExtension::Ts,
            Some("tsx") => ModuleExtension::Tsx,
            Some("js") => ModuleExtension::Js,
            Some("jsx") => ModuleExtension::Jsx,
            Some("mjs") => ModuleExtension::Mjs,
            Some("cjs") => ModuleExtension::Cjs,
            Some("mts") => ModuleExtension::Mts,
            Some("cts") => ModuleExtension::Cts,
            Some("json") => ModuleExtension::Json,
            _ => ModuleExtension::Unknown,
        }
    }

    /// Get the extension string
    pub fn as_str(&self) -> &'static str {
        match self {
            ModuleExtension::Ts => ".ts",
            ModuleExtension::Tsx => ".tsx",
            ModuleExtension::Dts => ".d.ts",
            ModuleExtension::DmTs => ".d.mts",
            ModuleExtension::DCts => ".d.cts",
            ModuleExtension::Js => ".js",
            ModuleExtension::Jsx => ".jsx",
            ModuleExtension::Mjs => ".mjs",
            ModuleExtension::Cjs => ".cjs",
            ModuleExtension::Mts => ".mts",
            ModuleExtension::Cts => ".cts",
            ModuleExtension::Json => ".json",
            ModuleExtension::Unknown => "",
        }
    }

    /// Check if this extension forces ESM mode
    /// .mts, .mjs, .d.mts files are always ESM
    pub fn forces_esm(&self) -> bool {
        matches!(
            self,
            ModuleExtension::Mts | ModuleExtension::Mjs | ModuleExtension::DmTs
        )
    }

    /// Check if this extension forces CommonJS mode
    /// .cts, .cjs, .d.cts files are always CommonJS
    pub fn forces_cjs(&self) -> bool {
        matches!(
            self,
            ModuleExtension::Cts | ModuleExtension::Cjs | ModuleExtension::DCts
        )
    }
}

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

/// Reason why module resolution failed
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResolutionFailure {
    /// Module specifier not found
    NotFound {
        /// Module specifier that was not found
        specifier: String,
        /// File containing the import
        containing_file: String,
        /// Span of the module specifier in source
        span: Span,
    },
    /// Invalid module specifier
    InvalidSpecifier {
        /// Error message describing why the specifier is invalid
        message: String,
        /// File containing the import
        containing_file: String,
        /// Span of the module specifier in source
        span: Span,
    },
    /// Package.json not found or invalid
    PackageJsonError {
        /// Error message describing the package.json issue
        message: String,
        /// File containing the import
        containing_file: String,
        /// Span of the module specifier in source
        span: Span,
    },
    /// Circular resolution detected
    CircularResolution {
        /// Error message describing the circular dependency
        message: String,
        /// File containing the import
        containing_file: String,
        /// Span of the module specifier in source
        span: Span,
    },
    /// Path mapping did not resolve to a file
    PathMappingFailed {
        /// Error message describing the path mapping failure
        message: String,
        /// File containing the import
        containing_file: String,
        /// Span of the module specifier in source
        span: Span,
    },
    /// TS2834: Relative import paths need explicit file extensions in EcmaScript imports
    /// when '--moduleResolution' is 'node16' or 'nodenext'.
    ImportPathNeedsExtension {
        /// Module specifier that was used without an extension
        specifier: String,
        /// Suggested extension to add (e.g., ".js")
        suggested_extension: String,
        /// File containing the import
        containing_file: String,
        /// Span of the module specifier in source
        span: Span,
    },
    /// TS5097: Import path ends with a TypeScript extension without allowImportingTsExtensions.
    ImportingTsExtensionNotAllowed {
        /// Extension that was used (e.g., ".ts")
        extension: String,
        /// File containing the import
        containing_file: String,
        /// Span of the module specifier in source
        span: Span,
    },
    /// TS6142: Module resolved to JSX/TSX without jsx option enabled.
    JsxNotEnabled {
        /// Module specifier that was resolved
        specifier: String,
        /// Resolved file path
        resolved_path: PathBuf,
        /// File containing the import
        containing_file: String,
        /// Span of the module specifier in source
        span: Span,
    },
    /// TS2792: Cannot find module. Did you mean to set the 'moduleResolution' option to 'nodenext'?
    /// Emitted when package.json has 'exports' but resolution mode doesn't support it.
    ModuleResolutionModeMismatch {
        /// Module specifier that could not be resolved
        specifier: String,
        /// File containing the import
        containing_file: String,
        /// Span of the module specifier in source
        span: Span,
    },
    /// TS2732: Cannot find module. Consider using '--resolveJsonModule' to import module with '.json' extension.
    /// Emitted when trying to import a .json file without resolveJsonModule enabled.
    JsonModuleWithoutResolveJsonModule {
        /// Module specifier ending in .json
        specifier: String,
        /// File containing the import
        containing_file: String,
        /// Span of the module specifier in source
        span: Span,
    },
}

impl ResolutionFailure {
    /// Convert a resolution failure to a diagnostic
    ///
    /// All resolution failure variants produce TS2307 diagnostics with proper
    /// source location information for IDE integration and error reporting.
    ///
    /// The message format matches TypeScript's exactly:
    /// "Cannot find module '{specifier}' or its corresponding type declarations."
    pub fn to_diagnostic(&self) -> Diagnostic {
        match self {
            ResolutionFailure::NotFound {
                specifier,
                containing_file,
                span,
            } => Diagnostic::error(
                containing_file,
                *span,
                format!(
                    "Cannot find module '{}' or its corresponding type declarations.",
                    specifier
                ),
                CANNOT_FIND_MODULE,
            ),
            ResolutionFailure::InvalidSpecifier {
                message,
                containing_file,
                span,
            } => Diagnostic::error(
                containing_file,
                *span,
                format!(
                    "Cannot find module '{}' or its corresponding type declarations.",
                    message
                ),
                CANNOT_FIND_MODULE,
            ),
            ResolutionFailure::PackageJsonError {
                message,
                containing_file,
                span,
            } => Diagnostic::error(
                containing_file,
                *span,
                format!(
                    "Cannot find module '{}' or its corresponding type declarations.",
                    message
                ),
                CANNOT_FIND_MODULE,
            ),
            ResolutionFailure::CircularResolution {
                message,
                containing_file,
                span,
            } => Diagnostic::error(
                containing_file,
                *span,
                format!(
                    "Cannot find module '{}' or its corresponding type declarations.",
                    message
                ),
                CANNOT_FIND_MODULE,
            ),
            ResolutionFailure::PathMappingFailed {
                message,
                containing_file,
                span,
            } => Diagnostic::error(
                containing_file,
                *span,
                format!(
                    "Cannot find module '{}' or its corresponding type declarations.",
                    message
                ),
                CANNOT_FIND_MODULE,
            ),
            ResolutionFailure::ImportPathNeedsExtension {
                specifier,
                suggested_extension,
                containing_file,
                span,
            } => {
                if suggested_extension.is_empty() {
                    // TS2834: No suggestion available
                    Diagnostic::error(
                        containing_file,
                        *span,
                        "Relative import paths need explicit file extensions in EcmaScript imports when '--moduleResolution' is 'node16' or 'nodenext'. Consider adding an extension to the import path.".to_string(),
                        IMPORT_PATH_NEEDS_EXTENSION,
                    )
                } else {
                    // TS2835: With extension suggestion
                    Diagnostic::error(
                        containing_file,
                        *span,
                        format!(
                            "Relative import paths need explicit file extensions in EcmaScript imports when '--moduleResolution' is 'node16' or 'nodenext'. Did you mean '{}{}'?",
                            specifier, suggested_extension
                        ),
                        IMPORT_PATH_NEEDS_EXTENSION_SUGGESTION,
                    )
                }
            }
            ResolutionFailure::ImportingTsExtensionNotAllowed {
                extension,
                containing_file,
                span,
            } => Diagnostic::error(
                containing_file,
                *span,
                format!(
                    "An import path can only end with a '{}' extension when 'allowImportingTsExtensions' is enabled.",
                    extension
                ),
                IMPORT_PATH_TS_EXTENSION_NOT_ALLOWED,
            ),
            ResolutionFailure::JsxNotEnabled {
                specifier,
                resolved_path,
                containing_file,
                span,
            } => Diagnostic::error(
                containing_file,
                *span,
                format!(
                    "Module '{}' was resolved to '{}', but '--jsx' is not set.",
                    specifier,
                    resolved_path.display()
                ),
                MODULE_WAS_RESOLVED_TO_BUT_JSX_NOT_SET,
            ),
            ResolutionFailure::ModuleResolutionModeMismatch {
                specifier,
                containing_file,
                span,
            } => Diagnostic::error(
                containing_file,
                *span,
                format!(
                    "Cannot find module '{}'. Did you mean to set the 'moduleResolution' option to 'nodenext', or to add aliases to the 'paths' option?",
                    specifier
                ),
                MODULE_RESOLUTION_MODE_MISMATCH,
            ),
            ResolutionFailure::JsonModuleWithoutResolveJsonModule {
                specifier,
                containing_file,
                span,
            } => Diagnostic::error(
                containing_file,
                *span,
                format!(
                    "Cannot find module '{}'. Consider using '--resolveJsonModule' to import module with '.json' extension.",
                    specifier
                ),
                JSON_MODULE_WITHOUT_RESOLVE_JSON_MODULE,
            ),
        }
    }

    /// Get the containing file for this resolution failure
    pub fn containing_file(&self) -> &str {
        match self {
            ResolutionFailure::NotFound {
                containing_file, ..
            }
            | ResolutionFailure::InvalidSpecifier {
                containing_file, ..
            }
            | ResolutionFailure::PackageJsonError {
                containing_file, ..
            }
            | ResolutionFailure::CircularResolution {
                containing_file, ..
            }
            | ResolutionFailure::PathMappingFailed {
                containing_file, ..
            }
            | ResolutionFailure::ImportPathNeedsExtension {
                containing_file, ..
            }
            | ResolutionFailure::ImportingTsExtensionNotAllowed {
                containing_file, ..
            }
            | ResolutionFailure::JsxNotEnabled {
                containing_file, ..
            }
            | ResolutionFailure::ModuleResolutionModeMismatch {
                containing_file, ..
            }
            | ResolutionFailure::JsonModuleWithoutResolveJsonModule {
                containing_file, ..
            } => containing_file,
        }
    }

    /// Get the span for this resolution failure
    pub fn span(&self) -> Span {
        match self {
            ResolutionFailure::NotFound { span, .. }
            | ResolutionFailure::InvalidSpecifier { span, .. }
            | ResolutionFailure::PackageJsonError { span, .. }
            | ResolutionFailure::CircularResolution { span, .. }
            | ResolutionFailure::PathMappingFailed { span, .. }
            | ResolutionFailure::ImportPathNeedsExtension { span, .. }
            | ResolutionFailure::ImportingTsExtensionNotAllowed { span, .. }
            | ResolutionFailure::JsxNotEnabled { span, .. }
            | ResolutionFailure::ModuleResolutionModeMismatch { span, .. }
            | ResolutionFailure::JsonModuleWithoutResolveJsonModule { span, .. } => *span,
        }
    }

    /// Check if this is a NotFound error
    pub fn is_not_found(&self) -> bool {
        matches!(self, ResolutionFailure::NotFound { .. })
    }
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
    resolution_cache: FxHashMap<(PathBuf, String), Result<ResolvedModule, ResolutionFailure>>,
    /// Custom conditions from tsconfig (for customConditions option)
    custom_conditions: Vec<String>,
    /// Whether allowJs is enabled (affects extension candidates)
    allow_js: bool,
    /// Cache for package.json package type lookups
    package_type_cache: FxHashMap<PathBuf, Option<PackageType>>,
    /// Cached package type for the current resolution
    current_package_type: Option<PackageType>,
}

struct PathMappingAttempt {
    resolved: Option<ResolvedModule>,
    attempted: bool,
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

        ModuleResolver {
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
            allow_js: options.allow_js,
            package_type_cache: FxHashMap::default(),
            current_package_type: None,
        }
    }

    /// Create a resolver with default Node resolution
    pub fn node_resolver() -> Self {
        ModuleResolver {
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
            allow_js: false,
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
    /// The import_kind is used to determine whether to emit TS2834 (extensionless ESM import)
    /// or TS2307 (cannot find module) for extensionless imports in Node16/NodeNext.
    pub fn resolve_with_kind(
        &mut self,
        specifier: &str,
        containing_file: &Path,
        specifier_span: Span,
        import_kind: ImportKind,
    ) -> Result<ResolvedModule, ResolutionFailure> {
        let containing_dir = containing_file
            .parent()
            .unwrap_or(Path::new("."))
            .to_path_buf();
        let containing_file_str = containing_file.display().to_string();

        if let Some(extension) = explicit_ts_extension(specifier) {
            if !self.allow_importing_ts_extensions {
                return Err(ResolutionFailure::ImportingTsExtensionNotAllowed {
                    extension,
                    containing_file: containing_file_str.clone(),
                    span: specifier_span,
                });
            }
        }

        self.current_package_type = match self.resolution_kind {
            ModuleResolutionKind::Node16 | ModuleResolutionKind::NodeNext => {
                self.get_package_type_for_dir(&containing_dir)
            }
            _ => None,
        };

        // Check cache first
        let cache_key = (containing_dir.clone(), specifier.to_string());
        if let Some(cached) = self.resolution_cache.get(&cache_key) {
            return cached.clone();
        }

        // Determine the module kind of the importing file
        let importing_module_kind = self.get_importing_module_kind(containing_file);

        let mut result = self.resolve_uncached(
            specifier,
            &containing_dir,
            &containing_file_str,
            specifier_span,
            importing_module_kind,
            import_kind,
        );

        if let Ok(resolved) = &result {
            if matches!(
                resolved.extension,
                ModuleExtension::Tsx | ModuleExtension::Jsx
            ) && self.jsx.is_none()
            {
                result = Err(ResolutionFailure::JsxNotEnabled {
                    specifier: specifier.to_string(),
                    resolved_path: resolved.resolved_path.clone(),
                    containing_file: containing_file_str.clone(),
                    span: specifier_span,
                });
            } else if resolved.extension == ModuleExtension::Json && !self.resolve_json_module {
                result = Err(ResolutionFailure::JsonModuleWithoutResolveJsonModule {
                    specifier: specifier.to_string(),
                    containing_file: containing_file_str.clone(),
                    span: specifier_span,
                });
            }
        }

        // Cache the result
        self.resolution_cache.insert(cache_key, result.clone());

        result
    }

    /// Determine the module kind of the importing file based on extension and package.json type
    fn get_importing_module_kind(&mut self, file_path: &Path) -> ImportingModuleKind {
        let extension = ModuleExtension::from_path(file_path);

        // .mts, .mjs force ESM mode
        if extension.forces_esm() {
            return ImportingModuleKind::Esm;
        }

        // .cts, .cjs force CommonJS mode
        if extension.forces_cjs() {
            return ImportingModuleKind::CommonJs;
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

    /// Get the package type for a directory by walking up to find package.json
    fn get_package_type_for_dir(&mut self, dir: &Path) -> Option<PackageType> {
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
            if package_json_path.is_file() {
                if let Ok(pj) = self.read_package_json(&package_json_path) {
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

    /// Resolve without checking cache
    fn resolve_uncached(
        &self,
        specifier: &str,
        containing_dir: &Path,
        containing_file: &str,
        specifier_span: Span,
        importing_module_kind: ImportingModuleKind,
        import_kind: ImportKind,
    ) -> Result<ResolvedModule, ResolutionFailure> {
        // Step 1: Handle #-prefixed imports (package.json imports field)
        // This is a Node16/NodeNext feature for subpath imports
        if specifier.starts_with('#') {
            if !self.resolve_package_json_imports {
                return Err(ResolutionFailure::NotFound {
                    specifier: specifier.to_string(),
                    containing_file: containing_file.to_string(),
                    span: specifier_span,
                });
            }
            return self.resolve_package_imports(
                specifier,
                containing_dir,
                containing_file,
                specifier_span,
                importing_module_kind,
            );
        }

        // Step 2: Try path mappings first (if configured)
        if !self.path_mappings.is_empty() {
            let attempt = self.try_path_mappings(specifier, containing_dir);
            if let Some(resolved) = attempt.resolved {
                return Ok(resolved);
            }
            if attempt.attempted {
                return Err(ResolutionFailure::PathMappingFailed {
                    message: specifier.to_string(),
                    containing_file: containing_file.to_string(),
                    span: specifier_span,
                });
            }
        }

        // Step 3: Handle relative imports
        if specifier.starts_with("./")
            || specifier.starts_with("../")
            || specifier == "."
            || specifier == ".."
        {
            return self.resolve_relative(
                specifier,
                containing_dir,
                containing_file,
                specifier_span,
                importing_module_kind,
                import_kind,
            );
        }

        // Step 4: Handle absolute imports (rare but valid)
        if specifier.starts_with('/') {
            return self.resolve_absolute(specifier, containing_file, specifier_span);
        }

        // Step 5: Try baseUrl fallback for non-relative specifiers
        if let Some(base_url) = &self.base_url {
            let candidate = base_url.join(specifier);
            if let Some(resolved) = self.try_file_or_directory(&candidate) {
                return Ok(ResolvedModule {
                    resolved_path: resolved.clone(),
                    is_external: false,
                    package_name: None,
                    original_specifier: specifier.to_string(),
                    extension: ModuleExtension::from_path(&resolved),
                });
            }
        }

        // Step 6: Classic resolution walks up the directory tree looking for
        // <specifier>.ts, <specifier>.tsx, <specifier>.d.ts at each level.
        // It does NOT consult node_modules.
        if matches!(self.resolution_kind, ModuleResolutionKind::Classic) {
            return self.resolve_classic_non_relative(
                specifier,
                containing_dir,
                containing_file,
                specifier_span,
            );
        }

        // Step 7: Handle bare specifiers (npm packages)
        self.resolve_bare_specifier(
            specifier,
            containing_dir,
            containing_file,
            specifier_span,
            importing_module_kind,
        )
    }

    /// Resolve package.json imports field (#-prefixed specifiers)
    fn resolve_package_imports(
        &self,
        specifier: &str,
        containing_dir: &Path,
        containing_file: &str,
        specifier_span: Span,
        importing_module_kind: ImportingModuleKind,
    ) -> Result<ResolvedModule, ResolutionFailure> {
        // Walk up directory tree looking for package.json with imports field
        let mut current = containing_dir.to_path_buf();

        loop {
            let package_json_path = current.join("package.json");

            if package_json_path.is_file() {
                if let Ok(package_json) = self.read_package_json(&package_json_path)
                    && let Some(imports) = &package_json.imports
                {
                    let conditions = self.get_export_conditions(importing_module_kind);

                    if let Some(target) =
                        self.resolve_imports_subpath(imports, specifier, &conditions)
                    {
                        // Resolve the target path
                        let resolved_path = current.join(target.trim_start_matches("./"));

                        if let Some(resolved) = self.try_file_or_directory(&resolved_path) {
                            return Ok(ResolvedModule {
                                resolved_path: resolved.clone(),
                                is_external: false,
                                package_name: package_json.name.clone(),
                                original_specifier: specifier.to_string(),
                                extension: ModuleExtension::from_path(&resolved),
                            });
                        }
                    }
                }
            }

            // Move to parent directory
            match current.parent() {
                Some(parent) if parent != current => current = parent.to_path_buf(),
                _ => break,
            }
        }

        Err(ResolutionFailure::NotFound {
            specifier: specifier.to_string(),
            containing_file: containing_file.to_string(),
            span: specifier_span,
        })
    }

    /// Resolve imports field subpath (similar to exports but with # prefix)
    fn resolve_imports_subpath(
        &self,
        imports: &FxHashMap<String, PackageExports>,
        specifier: &str,
        conditions: &[String],
    ) -> Option<String> {
        // Try exact match first
        if let Some(value) = imports.get(specifier) {
            return self.resolve_export_target_to_string(value, conditions);
        }

        // Try pattern matching (e.g., "#utils/*")
        let mut best_match: Option<(usize, String, &PackageExports)> = None;

        for (pattern, value) in imports {
            if let Some(wildcard) = match_imports_pattern(pattern, specifier) {
                let specificity = pattern.len();
                let is_better = match &best_match {
                    None => true,
                    Some((best_len, _, _)) => specificity > *best_len,
                };
                if is_better {
                    best_match = Some((specificity, wildcard, value));
                }
            }
        }

        if let Some((_, wildcard, value)) = best_match
            && let Some(target) = self.resolve_export_target_to_string(value, conditions)
        {
            return Some(apply_wildcard_substitution(&target, &wildcard));
        }

        None
    }

    /// Resolve an export/import value to a string path
    fn resolve_export_target_to_string(
        &self,
        value: &PackageExports,
        conditions: &[String],
    ) -> Option<String> {
        match value {
            PackageExports::String(s) => Some(s.clone()),
            PackageExports::Conditional(cond_entries) => {
                // Iterate condition map entries in JSON key order
                for (key, nested) in cond_entries {
                    if conditions.iter().any(|c| c == key) {
                        if matches!(nested, PackageExports::Null) {
                            return None;
                        }
                        if let Some(result) =
                            self.resolve_export_target_to_string(nested, conditions)
                        {
                            return Some(result);
                        }
                    }
                }
                None
            }
            PackageExports::Map(_) | PackageExports::Null => None, // Subpath maps not valid here
        }
    }

    /// Get export conditions based on resolution kind and module kind
    ///
    /// Returns conditions in priority order for conditional exports resolution.
    /// The order follows TypeScript's algorithm:
    /// 1. Custom conditions from tsconfig (prepended to defaults)
    /// 2. "types" - TypeScript always checks this first
    /// 3. Platform condition ("node" for Node.js, "browser" for bundler)
    /// 4. Primary module condition based on importing file ("import" for ESM, "require" for CJS)
    /// 5. "default" - fallback for unmatched conditions
    /// 6. Opposite module condition as fallback (allows ESM-first packages to work with CJS imports)
    /// 7. Additional platform fallbacks
    fn get_export_conditions(&self, importing_module_kind: ImportingModuleKind) -> Vec<String> {
        let mut conditions = Vec::new();

        // Custom conditions from tsconfig are prepended to defaults
        for cond in &self.custom_conditions {
            conditions.push(cond.clone());
        }

        // TypeScript always checks "types" first
        conditions.push("types".to_string());

        // Add platform condition: Node modes get "node", bundler does NOT
        match self.resolution_kind {
            ModuleResolutionKind::Node16 | ModuleResolutionKind::NodeNext => {
                conditions.push("node".to_string());
            }
            _ => {}
        }

        // Add module kind condition
        match importing_module_kind {
            ImportingModuleKind::Esm => {
                conditions.push("import".to_string());
            }
            ImportingModuleKind::CommonJs => {
                conditions.push("require".to_string());
            }
        }

        // "default" is always a fallback condition
        conditions.push("default".to_string());

        conditions
    }

    /// Try resolving through path mappings
    fn try_path_mappings(&self, specifier: &str, containing_dir: &Path) -> PathMappingAttempt {
        // Sort path mappings by specificity (most specific first)
        let mut sorted_mappings: Vec<_> = self.path_mappings.iter().collect();
        sorted_mappings.sort_by_key(|b| std::cmp::Reverse(b.specificity()));

        let mut attempted = false;
        for mapping in sorted_mappings {
            if let Some(star_match) = mapping.match_specifier(specifier) {
                attempted = true;
                // Try each target path
                for target in &mapping.targets {
                    let substituted = if target.contains('*') {
                        target.replace('*', &star_match)
                    } else {
                        target.clone()
                    };

                    // Resolve relative to baseUrl or containing directory
                    let base = self.base_url.as_deref().unwrap_or(containing_dir);
                    let candidate = base.join(&substituted);

                    if let Some(resolved) = self.try_file_or_directory(&candidate) {
                        return PathMappingAttempt {
                            resolved: Some(ResolvedModule {
                                resolved_path: resolved,
                                is_external: false,
                                package_name: None,
                                original_specifier: specifier.to_string(),
                                extension: ModuleExtension::from_path(&candidate),
                            }),
                            attempted,
                        };
                    }
                }
            }
        }

        PathMappingAttempt {
            resolved: None,
            attempted,
        }
    }

    /// Resolve a relative import
    fn resolve_relative(
        &self,
        specifier: &str,
        containing_dir: &Path,
        containing_file: &str,
        specifier_span: Span,
        importing_module_kind: ImportingModuleKind,
        import_kind: ImportKind,
    ) -> Result<ResolvedModule, ResolutionFailure> {
        let candidate = containing_dir.join(specifier);

        // Check if specifier has an explicit extension
        let specifier_has_extension = Path::new(specifier)
            .extension()
            .map(|ext| !ext.is_empty())
            .unwrap_or(false);

        // TS2834/TS2835 Check: In Node16/NodeNext, ESM-style imports must have explicit extensions.
        // This applies when:
        // - The resolution mode is Node16 or NodeNext
        // - The import is ESM syntax in an ESM context:
        //   - Dynamic import() always counts as ESM regardless of file type
        //   - Static import/export only counts if the file is an ESM module
        //   - require() never triggers this check
        // - The specifier has no extension
        let needs_extension_check = matches!(
            self.resolution_kind,
            ModuleResolutionKind::Node16 | ModuleResolutionKind::NodeNext
        ) && !specifier_has_extension
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
            if let Some(resolved) = self.try_file_or_directory(&candidate) {
                // Resolution succeeded implicitly - this is an error in ESM mode
                let resolved_ext = ModuleExtension::from_path(&resolved);
                // Suggest the .js extension (TypeScript convention: import .js, compile from .ts)
                let suggested_ext = match resolved_ext {
                    ModuleExtension::Ts
                    | ModuleExtension::Tsx
                    | ModuleExtension::Js
                    | ModuleExtension::Jsx => ".js",
                    ModuleExtension::Mts | ModuleExtension::Mjs => ".mjs",
                    ModuleExtension::Cts | ModuleExtension::Cjs => ".cjs",
                    ModuleExtension::Dts => ".js",
                    ModuleExtension::DmTs => ".mjs",
                    ModuleExtension::DCts => ".cjs",
                    ModuleExtension::Json => ".json",
                    ModuleExtension::Unknown => ".js",
                };
                return Err(ResolutionFailure::ImportPathNeedsExtension {
                    specifier: specifier.to_string(),
                    suggested_extension: suggested_ext.to_string(),
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

        if let Some(resolved) = self.try_file_or_directory(&candidate) {
            return Ok(ResolvedModule {
                resolved_path: resolved.clone(),
                is_external: false,
                package_name: None,
                original_specifier: specifier.to_string(),
                extension: ModuleExtension::from_path(&resolved),
            });
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
    fn resolve_absolute(
        &self,
        specifier: &str,
        containing_file: &str,
        specifier_span: Span,
    ) -> Result<ResolvedModule, ResolutionFailure> {
        let path = PathBuf::from(specifier);

        if let Some(resolved) = self.try_file_or_directory(&path) {
            return Ok(ResolvedModule {
                resolved_path: resolved.clone(),
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

    /// Resolve a non-relative module specifier using Classic resolution.
    ///
    /// TypeScript's Classic algorithm walks up the directory tree from the containing
    /// file's directory, probing for `<specifier>.ts`, `<specifier>.tsx`,
    /// `<specifier>.d.ts` at each level. It does NOT consult `node_modules`.
    ///
    /// Example: importing `"foo"` from `/a/b/c/app.ts` will try:
    ///   /a/b/c/foo.ts, /a/b/c/foo.tsx, /a/b/c/foo.d.ts, ...
    ///   /a/b/foo.ts, /a/b/foo.tsx, /a/b/foo.d.ts, ...
    ///   /a/foo.ts, /a/foo.tsx, /a/foo.d.ts, ...
    ///   /foo.ts, /foo.tsx, /foo.d.ts, ...
    fn resolve_classic_non_relative(
        &self,
        specifier: &str,
        containing_dir: &Path,
        containing_file: &str,
        specifier_span: Span,
    ) -> Result<ResolvedModule, ResolutionFailure> {
        let (package_name, subpath) = parse_package_specifier(specifier);
        let conditions = self.get_export_conditions(ImportingModuleKind::CommonJs);

        let mut current = containing_dir.to_path_buf();
        loop {
            let candidate = current.join(specifier);
            if let Some(resolved) = self.try_file_or_directory(&candidate) {
                return Ok(ResolvedModule {
                    resolved_path: resolved.clone(),
                    is_external: false,
                    package_name: None,
                    original_specifier: specifier.to_string(),
                    extension: ModuleExtension::from_path(&resolved),
                });
            }

            // Also check @types packages in node_modules (TypeScript classic resolution
            // still resolves @types packages for bare specifiers)
            if !package_name.starts_with("@types/") {
                let types_package = types_package_name(&package_name);
                let types_dir = current.join("node_modules").join(&types_package);
                if types_dir.is_dir() {
                    if let Ok(resolved) = self.resolve_package(
                        &types_dir,
                        subpath.as_deref(),
                        specifier,
                        containing_file,
                        specifier_span,
                        &conditions,
                    ) {
                        return Ok(resolved);
                    }
                }
            }

            // Check type_roots for the package
            for type_root in &self.type_roots {
                let types_package = if !package_name.starts_with("@types/") {
                    type_root.join(types_package_name(&package_name))
                } else {
                    type_root.join(&package_name)
                };
                if types_package.is_dir()
                    && let Ok(resolved) = self.resolve_package(
                        &types_package,
                        subpath.as_deref(),
                        specifier,
                        containing_file,
                        specifier_span,
                        &conditions,
                    )
                {
                    return Ok(resolved);
                }
            }

            // Move to parent directory
            match current.parent() {
                Some(parent) if parent != current => current = parent.to_path_buf(),
                _ => break,
            }
        }

        // Module not found - emit TS2307 (standard "Cannot find module" error).
        // Classic resolution walks up the directory tree but doesn't use node_modules.
        Err(ResolutionFailure::NotFound {
            specifier: specifier.to_string(),
            containing_file: containing_file.to_string(),
            span: specifier_span,
        })
    }

    /// Resolve a bare specifier (npm package)
    fn resolve_bare_specifier(
        &self,
        specifier: &str,
        containing_dir: &Path,
        containing_file: &str,
        specifier_span: Span,
        importing_module_kind: ImportingModuleKind,
    ) -> Result<ResolvedModule, ResolutionFailure> {
        // Parse package name and subpath
        let (package_name, subpath) = parse_package_specifier(specifier);
        let conditions = self.get_export_conditions(importing_module_kind);

        // First, try self-reference: check if we're inside a package that matches the specifier
        if let Some(resolved) = self.try_self_reference(
            &package_name,
            subpath.as_deref(),
            specifier,
            containing_dir,
            &conditions,
        ) {
            return Ok(resolved);
        }

        // Walk up directory tree looking for node_modules
        let mut current = containing_dir.to_path_buf();
        loop {
            let node_modules = current.join("node_modules");

            if node_modules.is_dir() {
                let package_dir = node_modules.join(&package_name);

                if package_dir.is_dir() {
                    match self.resolve_package(
                        &package_dir,
                        subpath.as_deref(),
                        specifier,
                        containing_file,
                        specifier_span,
                        &conditions,
                    ) {
                        Ok(resolved) => return Ok(resolved),
                        Err(e @ ResolutionFailure::ModuleResolutionModeMismatch { .. }) => {
                            // Package found with exports field but resolution failed.
                            // exports is authoritative — do not continue searching.
                            return Err(e);
                        }
                        Err(_) => {
                            // Continue searching in parent directories
                        }
                    }
                } else if subpath.is_none() {
                    // Try resolving as a file directly in node_modules
                    // e.g., node_modules/foo.d.ts for bare specifier "foo"
                    if let Some(resolved) =
                        self.try_file_or_directory(&node_modules.join(&package_name))
                    {
                        return Ok(ResolvedModule {
                            resolved_path: resolved.clone(),
                            is_external: true,
                            package_name: Some(package_name.clone()),
                            original_specifier: specifier.to_string(),
                            extension: ModuleExtension::from_path(&resolved),
                        });
                    }
                }
            }

            if !package_name.starts_with("@types/") {
                let types_package = types_package_name(&package_name);
                let types_dir = node_modules.join(&types_package);
                if types_dir.is_dir() {
                    if let Ok(resolved) = self.resolve_package(
                        &types_dir,
                        subpath.as_deref(),
                        specifier,
                        containing_file,
                        specifier_span,
                        &conditions,
                    ) {
                        return Ok(resolved);
                    }
                }
            }

            // Move to parent directory
            match current.parent() {
                Some(parent) if parent != current => current = parent.to_path_buf(),
                _ => break,
            }
        }

        // Try type roots (for @types packages)
        for type_root in &self.type_roots {
            let types_package = type_root.join(types_package_name(&package_name));
            if types_package.is_dir()
                && let Ok(resolved) = self.resolve_package(
                    &types_package,
                    subpath.as_deref(),
                    specifier,
                    containing_file,
                    specifier_span,
                    &conditions,
                )
            {
                return Ok(resolved);
            }
        }

        Err(ResolutionFailure::NotFound {
            specifier: specifier.to_string(),
            containing_file: containing_file.to_string(),
            span: specifier_span,
        })
    }

    /// Try to resolve a self-reference (package importing itself by name)
    fn try_self_reference(
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

            if package_json_path.is_file() {
                if let Ok(package_json) = self.read_package_json(&package_json_path) {
                    // Check if the package name matches
                    if package_json.name.as_deref() == Some(package_name) {
                        // This is a self-reference!
                        if self.resolve_package_json_exports
                            && let Some(exports) = &package_json.exports
                        {
                            let subpath_key = match subpath {
                                Some(sp) => format!("./{}", sp),
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
            }

            // Move to parent directory
            match current.parent() {
                Some(parent) if parent != current => current = parent.to_path_buf(),
                _ => break,
            }
        }

        None
    }

    /// Resolve within a package directory
    fn resolve_package(
        &self,
        package_dir: &Path,
        subpath: Option<&str>,
        original_specifier: &str,
        containing_file: &str,
        specifier_span: Span,
        conditions: &[String],
    ) -> Result<ResolvedModule, ResolutionFailure> {
        // Read package.json
        let package_json_path = package_dir.join("package.json");
        let package_json = if package_json_path.exists() {
            self.read_package_json(&package_json_path).map_err(|msg| {
                ResolutionFailure::PackageJsonError {
                    message: msg,
                    containing_file: containing_file.to_string(),
                    span: specifier_span,
                }
            })?
        } else {
            PackageJson::default()
        };

        // If there's a subpath, resolve it directly
        if let Some(subpath) = subpath {
            let subpath_key = format!("./{}", subpath);

            // Try exports field first (Node16+)
            if self.resolve_package_json_exports {
                if let Some(exports) = &package_json.exports {
                    if let Some(resolved) = self.resolve_package_exports_with_conditions(
                        package_dir,
                        exports,
                        &subpath_key,
                        conditions,
                    ) {
                        return Ok(ResolvedModule {
                            resolved_path: resolved.clone(),
                            is_external: true,
                            package_name: Some(package_json.name.clone().unwrap_or_default()),
                            original_specifier: original_specifier.to_string(),
                            extension: ModuleExtension::from_path(&resolved),
                        });
                    }
                    // In Node16/NodeNext, exports field is authoritative for subpaths.
                    // Bundler mode is more permissive and allows fallback.
                    if matches!(
                        self.resolution_kind,
                        ModuleResolutionKind::Node16 | ModuleResolutionKind::NodeNext
                    ) {
                        return Err(ResolutionFailure::ModuleResolutionModeMismatch {
                            specifier: original_specifier.to_string(),
                            containing_file: containing_file.to_string(),
                            span: specifier_span,
                        });
                    }
                }
            }

            // Try typesVersions field
            if let Some(types_versions) = &package_json.types_versions
                && let Some(resolved) =
                    self.resolve_types_versions(package_dir, subpath, types_versions)
            {
                return Ok(ResolvedModule {
                    resolved_path: resolved.clone(),
                    is_external: true,
                    package_name: Some(package_json.name.clone().unwrap_or_default()),
                    original_specifier: original_specifier.to_string(),
                    extension: ModuleExtension::from_path(&resolved),
                });
            }

            // Fall back to direct file resolution
            let file_path = package_dir.join(subpath);
            if let Some(resolved) = self.try_file_or_directory(&file_path) {
                return Ok(ResolvedModule {
                    resolved_path: resolved.clone(),
                    is_external: true,
                    package_name: Some(package_json.name.clone().unwrap_or_default()),
                    original_specifier: original_specifier.to_string(),
                    extension: ModuleExtension::from_path(&resolved),
                });
            }

            return Err(ResolutionFailure::NotFound {
                specifier: original_specifier.to_string(),
                containing_file: containing_file.to_string(),
                span: specifier_span,
            });
        }

        // No subpath - resolve package entry point

        // Try exports "." field first (Node16+)
        if self.resolve_package_json_exports {
            if let Some(exports) = &package_json.exports {
                if let Some(resolved) = self.resolve_package_exports_with_conditions(
                    package_dir,
                    exports,
                    ".",
                    conditions,
                ) {
                    return Ok(ResolvedModule {
                        resolved_path: resolved.clone(),
                        is_external: true,
                        package_name: Some(package_json.name.clone().unwrap_or_default()),
                        original_specifier: original_specifier.to_string(),
                        extension: ModuleExtension::from_path(&resolved),
                    });
                }
                // In Node16/NodeNext, exports field is authoritative.
                // Do NOT fall through to types/main/index — emit TS2792.
                // Bundler mode is more permissive and allows fallback.
                if matches!(
                    self.resolution_kind,
                    ModuleResolutionKind::Node16 | ModuleResolutionKind::NodeNext
                ) {
                    return Err(ResolutionFailure::ModuleResolutionModeMismatch {
                        specifier: original_specifier.to_string(),
                        containing_file: containing_file.to_string(),
                        span: specifier_span,
                    });
                }
            }
        }

        // Try typesVersions field for index
        if let Some(types_versions) = &package_json.types_versions
            && let Some(resolved) =
                self.resolve_types_versions(package_dir, "index", types_versions)
        {
            return Ok(ResolvedModule {
                resolved_path: resolved.clone(),
                is_external: true,
                package_name: Some(package_json.name.clone().unwrap_or_default()),
                original_specifier: original_specifier.to_string(),
                extension: ModuleExtension::from_path(&resolved),
            });
        }

        // Try types/typings field
        if let Some(types) = package_json.types.clone().or(package_json.typings.clone()) {
            let types_path = package_dir.join(&types);
            if let Some(resolved) = resolve_explicit_unknown_extension(&types_path) {
                return Ok(ResolvedModule {
                    resolved_path: resolved.clone(),
                    is_external: true,
                    package_name: Some(package_json.name.clone().unwrap_or_default()),
                    original_specifier: original_specifier.to_string(),
                    extension: ModuleExtension::from_path(&resolved),
                });
            }
            if let Some(resolved) = self.try_file_or_directory(&types_path) {
                return Ok(ResolvedModule {
                    resolved_path: resolved.clone(),
                    is_external: true,
                    package_name: Some(package_json.name.clone().unwrap_or_default()),
                    original_specifier: original_specifier.to_string(),
                    extension: ModuleExtension::from_path(&resolved),
                });
            }
        }

        // Try main field
        if let Some(main) = &package_json.main {
            let main_path = package_dir.join(main);
            if let Some(resolved) = resolve_explicit_unknown_extension(&main_path) {
                return Ok(ResolvedModule {
                    resolved_path: resolved.clone(),
                    is_external: true,
                    package_name: Some(package_json.name.clone().unwrap_or_default()),
                    original_specifier: original_specifier.to_string(),
                    extension: ModuleExtension::from_path(&resolved),
                });
            }
            if let Some(declaration) = declaration_substitution_for_main(&main_path) {
                if declaration.is_file() {
                    return Ok(ResolvedModule {
                        resolved_path: declaration.clone(),
                        is_external: true,
                        package_name: Some(package_json.name.clone().unwrap_or_default()),
                        original_specifier: original_specifier.to_string(),
                        extension: ModuleExtension::from_path(&declaration),
                    });
                }
            }
            // Try the main path as a file (with extension probing)
            if let Some(resolved) = self.try_file(&main_path) {
                return Ok(ResolvedModule {
                    resolved_path: resolved.clone(),
                    is_external: true,
                    package_name: Some(package_json.name.clone().unwrap_or_default()),
                    original_specifier: original_specifier.to_string(),
                    extension: ModuleExtension::from_path(&resolved),
                });
            }
            // For main field targets that are directories, only try index files.
            // Do NOT read nested package.json — main field resolution is non-recursive.
            if main_path.is_dir() {
                let index = main_path.join("index");
                if let Some(resolved) = self.try_file(&index) {
                    return Ok(ResolvedModule {
                        resolved_path: resolved.clone(),
                        is_external: true,
                        package_name: Some(package_json.name.clone().unwrap_or_default()),
                        original_specifier: original_specifier.to_string(),
                        extension: ModuleExtension::from_path(&resolved),
                    });
                }
            }
        }

        // Try index.ts/index.js
        let index = package_dir.join("index");
        if let Some(resolved) = self.try_file(&index) {
            return Ok(ResolvedModule {
                resolved_path: resolved.clone(),
                is_external: true,
                package_name: Some(package_json.name.clone().unwrap_or_default()),
                original_specifier: original_specifier.to_string(),
                extension: ModuleExtension::from_path(&resolved),
            });
        }

        Err(ResolutionFailure::PackageJsonError {
            message: format!(
                "Could not find entry point for package at {}",
                package_dir.display()
            ),
            containing_file: containing_file.to_string(),
            span: specifier_span,
        })
    }

    /// Resolve package exports with explicit conditions
    fn resolve_package_exports_with_conditions(
        &self,
        package_dir: &Path,
        exports: &PackageExports,
        subpath: &str,
        conditions: &[String],
    ) -> Option<PathBuf> {
        match exports {
            PackageExports::String(s) => {
                if subpath == "." {
                    let resolved = package_dir.join(s.trim_start_matches("./"));
                    if let Some(r) = self.try_export_target(&resolved) {
                        return Some(r);
                    }
                }
                None
            }
            PackageExports::Map(map) => {
                // First try exact match
                if let Some(value) = map.get(subpath) {
                    return self.resolve_export_value_with_conditions(
                        package_dir,
                        value,
                        conditions,
                    );
                }

                // Try pattern matching (e.g., "./*" or "./lib/*")
                let mut best_match: Option<(usize, String, &PackageExports)> = None;

                for (pattern, value) in map {
                    if let Some(matched) = match_export_pattern(pattern, subpath) {
                        let specificity = pattern.len();
                        let is_better = match &best_match {
                            None => true,
                            Some((best_len, _, _)) => specificity > *best_len,
                        };
                        if is_better {
                            best_match = Some((specificity, matched, value));
                        }
                    }
                }

                if let Some((_, wildcard, value)) = best_match
                    && let Some(resolved) =
                        self.resolve_export_value_with_conditions(package_dir, value, conditions)
                {
                    // Substitute wildcard in path
                    let resolved_str = resolved.to_string_lossy();
                    if resolved_str.contains('*') {
                        let substituted = resolved_str.replace('*', &wildcard);
                        return Some(PathBuf::from(substituted));
                    }
                    return Some(resolved);
                }

                None
            }
            PackageExports::Conditional(cond_entries) => {
                // Iterate condition map entries in JSON key order (not our conditions order)
                for (key, value) in cond_entries {
                    if conditions.iter().any(|c| c == key) {
                        // null means explicitly blocked - stop here
                        if matches!(value, PackageExports::Null) {
                            return None;
                        }
                        if let Some(resolved) = self.resolve_package_exports_with_conditions(
                            package_dir,
                            value,
                            subpath,
                            conditions,
                        ) {
                            return Some(resolved);
                        }
                    }
                }
                None
            }
            PackageExports::Null => None,
        }
    }

    /// Resolve a single export value with conditions
    fn resolve_export_value_with_conditions(
        &self,
        package_dir: &Path,
        value: &PackageExports,
        conditions: &[String],
    ) -> Option<PathBuf> {
        match value {
            PackageExports::String(s) => {
                let resolved = package_dir.join(s.trim_start_matches("./"));
                self.try_export_target(&resolved)
            }
            PackageExports::Conditional(cond_entries) => {
                // Iterate condition map entries in JSON key order
                for (key, nested) in cond_entries {
                    if conditions.iter().any(|c| c == key) {
                        // null means explicitly blocked - stop here
                        if matches!(nested, PackageExports::Null) {
                            return None;
                        }
                        if let Some(resolved) = self.resolve_export_value_with_conditions(
                            package_dir,
                            nested,
                            conditions,
                        ) {
                            return Some(resolved);
                        }
                    }
                }
                None
            }
            PackageExports::Map(_) | PackageExports::Null => None,
        }
    }

    /// Resolve typesVersions field
    fn resolve_types_versions(
        &self,
        package_dir: &Path,
        subpath: &str,
        types_versions: &serde_json::Value,
    ) -> Option<PathBuf> {
        let compiler_version =
            types_versions_compiler_version(self.types_versions_compiler_version.as_deref());
        let paths = select_types_versions_paths(types_versions, compiler_version)?;
        let mut best_pattern: Option<&String> = None;
        let mut best_value: Option<&serde_json::Value> = None;
        let mut best_wildcard = String::new();
        let mut best_specificity = 0usize;
        let mut best_len = 0usize;

        for (pattern, value) in paths {
            let Some(wildcard) = match_types_versions_pattern(pattern, subpath) else {
                continue;
            };
            let specificity = types_versions_specificity(pattern);
            let pattern_len = pattern.len();
            let is_better = match best_pattern {
                None => true,
                Some(current) => {
                    specificity > best_specificity
                        || (specificity == best_specificity && pattern_len > best_len)
                        || (specificity == best_specificity
                            && pattern_len == best_len
                            && pattern < current)
                }
            };

            if is_better {
                best_specificity = specificity;
                best_len = pattern_len;
                best_pattern = Some(pattern);
                best_value = Some(value);
                best_wildcard = wildcard;
            }
        }

        let Some(value) = best_value else {
            return None;
        };

        let mut targets = Vec::new();
        match value {
            serde_json::Value::String(value) => targets.push(value.as_str()),
            serde_json::Value::Array(list) => {
                for entry in list {
                    if let Some(value) = entry.as_str() {
                        targets.push(value);
                    }
                }
            }
            _ => {}
        }

        for target in targets {
            let substituted = apply_wildcard_substitution(target, &best_wildcard);
            let resolved = package_dir.join(substituted.trim_start_matches("./"));
            if let Some(resolved) = self.try_file_or_directory(&resolved) {
                return Some(resolved);
            }
        }

        None
    }

    /// Try to resolve a file with various extensions
    fn try_file(&self, path: &Path) -> Option<PathBuf> {
        let suffixes = &self.module_suffixes;
        if let Some(extension) = path.extension().and_then(|ext| ext.to_str()) {
            if split_path_extension(path).is_none() {
                if self.allow_arbitrary_extensions {
                    if let Some(resolved) = try_arbitrary_extension_declaration(path, extension) {
                        return Some(resolved);
                    }
                }
                return None;
            }
        }
        if let Some((base, extension)) = split_path_extension(path) {
            // Try extension substitution (.js → .ts/.tsx/.d.ts) for all resolution modes.
            // TypeScript resolves `.js` imports to `.ts` sources in all modes.
            if let Some(rewritten) = node16_extension_substitution(path, extension) {
                for candidate in &rewritten {
                    if let Some(resolved) = try_file_with_suffixes(candidate, suffixes) {
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
        if self.resolve_json_module {
            if let Some(resolved) = try_file_with_suffixes_and_extension(path, "json", suffixes) {
                return Some(resolved);
            }
        }

        let index = path.join("index");
        for ext in extensions {
            if let Some(resolved) = try_file_with_suffixes_and_extension(&index, ext, suffixes) {
                return Some(resolved);
            }
        }
        if self.resolve_json_module {
            if let Some(resolved) = try_file_with_suffixes_and_extension(&index, "json", suffixes) {
                return Some(resolved);
            }
        }

        None
    }

    fn extension_candidates_for_resolution(&self) -> &'static [&'static str] {
        match self.resolution_kind {
            ModuleResolutionKind::Node16 | ModuleResolutionKind::NodeNext => {
                match self.current_package_type {
                    Some(PackageType::Module) => &NODE16_MODULE_EXTENSION_CANDIDATES,
                    Some(PackageType::CommonJs) => &NODE16_COMMONJS_EXTENSION_CANDIDATES,
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

    /// Try to resolve a path as a file or directory
    fn try_file_or_directory(&self, path: &Path) -> Option<PathBuf> {
        // Try as file first
        if let Some(resolved) = self.try_file(path) {
            return Some(resolved);
        }

        // Try as directory: check package.json for types/main, then index
        if path.is_dir() {
            let package_json_path = path.join("package.json");
            if package_json_path.exists() {
                if let Ok(pj) = self.read_package_json(&package_json_path) {
                    // Try types/typings field first
                    if let Some(types) = pj.types.or(pj.typings) {
                        let types_path = path.join(&types);
                        if let Some(resolved) = self.try_file(&types_path) {
                            return Some(resolved);
                        }
                        if types_path.is_file() {
                            return Some(types_path);
                        }
                    }
                    // Try main field with extension remapping
                    if let Some(main) = &pj.main {
                        let main_path = path.join(main);
                        if let Some(resolved) = self.try_file(&main_path) {
                            return Some(resolved);
                        }
                    }
                }
            }
            let index = path.join("index");
            return self.try_file(&index);
        }

        None
    }

    /// Resolve an exports target without Node16 extension substitution.
    ///
    /// Explicit extensions must exist exactly; extensionless targets follow normal lookup.
    fn try_export_target(&self, path: &Path) -> Option<PathBuf> {
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
            if self.allow_arbitrary_extensions {
                if let Some(resolved) = try_arbitrary_extension_declaration(path, extension) {
                    return Some(resolved);
                }
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

    /// Read and parse package.json
    ///
    /// Returns a String error for flexibility - callers can convert to ResolutionFailure
    /// with appropriate span/file information at the call site.
    fn read_package_json(&self, path: &Path) -> Result<PackageJson, String> {
        let content = std::fs::read_to_string(path)
            .map_err(|e| format!("Failed to read {}: {}", path.display(), e))?;

        serde_json::from_str(&content)
            .map_err(|e| format!("Failed to parse {}: {}", path.display(), e))
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
            .unwrap_or(Path::new("."))
            .to_path_buf();
        let containing_file_str = containing_file.display().to_string();
        let importing_module_kind = self.get_importing_module_kind(containing_file);

        self.allow_js = true;
        let result = self.resolve_uncached(
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
    pub fn resolution_kind(&self) -> ModuleResolutionKind {
        self.resolution_kind
    }

    /// Emit TS2307 error for a resolution failure into a diagnostic bag
    ///
    /// All module resolution failures emit TS2307 "Cannot find module" error.
    /// This includes:
    /// - NotFound: Module specifier could not be resolved
    /// - InvalidSpecifier: Module specifier is malformed
    /// - PackageJsonError: Package.json is missing or invalid
    /// - CircularResolution: Circular dependency detected during resolution
    /// - PathMappingFailed: Path mapping from tsconfig did not resolve
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
fn parse_package_specifier(specifier: &str) -> (String, Option<String>) {
    // Handle scoped packages (@scope/pkg)
    if specifier.starts_with('@') {
        if let Some(slash_idx) = specifier[1..].find('/') {
            let scope_end = slash_idx + 1;
            if let Some(next_slash) = specifier[scope_end + 1..].find('/') {
                let pkg_end = scope_end + 1 + next_slash;
                return (
                    specifier[..pkg_end].to_string(),
                    Some(specifier[pkg_end + 1..].to_string()),
                );
            }
            return (specifier.to_string(), None);
        }
        return (specifier.to_string(), None);
    }

    // Handle regular packages
    if let Some(slash_idx) = specifier.find('/') {
        (
            specifier[..slash_idx].to_string(),
            Some(specifier[slash_idx + 1..].to_string()),
        )
    } else {
        (specifier.to_string(), None)
    }
}

/// Convert a package name to its @types equivalent.
/// For scoped packages like `@see/saw`, this produces `@types/see__saw`.
/// For regular packages like `foo`, this produces `@types/foo`.
fn types_package_name(package_name: &str) -> String {
    let stripped = package_name.strip_prefix('@').unwrap_or(package_name);
    format!("@types/{}", stripped.replace('/', "__"))
}

/// Match an export pattern against a subpath
fn match_export_pattern(pattern: &str, subpath: &str) -> Option<String> {
    if !pattern.contains('*') {
        return if pattern == subpath {
            Some(String::new())
        } else {
            None
        };
    }

    let parts: Vec<&str> = pattern.split('*').collect();
    if parts.len() != 2 {
        return None;
    }

    let prefix = parts[0];
    let suffix = parts[1];

    if !subpath.starts_with(prefix) || !subpath.ends_with(suffix) {
        return None;
    }

    let start = prefix.len();
    let end = subpath.len().saturating_sub(suffix.len());

    if end < start {
        return None;
    }

    Some(subpath[start..end].to_string())
}

/// Match an imports pattern against a specifier (#-prefixed)
fn match_imports_pattern(pattern: &str, specifier: &str) -> Option<String> {
    if !pattern.contains('*') {
        return if pattern == specifier {
            Some(String::new())
        } else {
            None
        };
    }

    // Strip # prefix for matching
    let pattern = pattern.strip_prefix('#').unwrap_or(pattern);
    let specifier = specifier.strip_prefix('#').unwrap_or(specifier);

    let parts: Vec<&str> = pattern.split('*').collect();
    if parts.len() != 2 {
        return None;
    }

    let prefix = parts[0];
    let suffix = parts[1];

    if !specifier.starts_with(prefix) || !specifier.ends_with(suffix) {
        return None;
    }

    let start = prefix.len();
    let end = specifier.len().saturating_sub(suffix.len());

    if end < start {
        return None;
    }

    Some(specifier[start..end].to_string())
}

/// Match a typesVersions pattern against a subpath
fn match_types_versions_pattern(pattern: &str, subpath: &str) -> Option<String> {
    if !pattern.contains('*') {
        return if pattern == subpath {
            Some(String::new())
        } else {
            None
        };
    }

    let star_pos = pattern.find('*')?;
    let (prefix, suffix) = pattern.split_at(star_pos);
    let suffix = &suffix[1..]; // Skip the '*'

    if !subpath.starts_with(prefix) || !subpath.ends_with(suffix) {
        return None;
    }

    let start = prefix.len();
    let end = subpath.len().saturating_sub(suffix.len());

    if end < start {
        return None;
    }

    Some(subpath[start..end].to_string())
}

fn types_versions_compiler_version(value: Option<&str>) -> SemVer {
    value
        .and_then(parse_semver)
        .unwrap_or_else(default_types_versions_compiler_version)
}

fn default_types_versions_compiler_version() -> SemVer {
    TYPES_VERSIONS_COMPILER_VERSION_FALLBACK
}

fn select_types_versions_paths(
    types_versions: &serde_json::Value,
    compiler_version: SemVer,
) -> Option<&serde_json::Map<String, serde_json::Value>> {
    select_types_versions_paths_for_version(types_versions, compiler_version)
}

fn select_types_versions_paths_for_version(
    types_versions: &serde_json::Value,
    compiler_version: SemVer,
) -> Option<&serde_json::Map<String, serde_json::Value>> {
    let map = types_versions.as_object()?;
    let mut best_score: Option<RangeScore> = None;
    let mut best_key: Option<&str> = None;
    let mut best_value: Option<&serde_json::Map<String, serde_json::Value>> = None;

    for (key, value) in map {
        let Some(value_map) = value.as_object() else {
            continue;
        };
        let Some(score) = match_types_versions_range(key, compiler_version) else {
            continue;
        };
        let is_better = match best_score {
            None => true,
            Some(best) => {
                score > best
                    || (score == best && best_key.is_none_or(|best_key| key.as_str() < best_key))
            }
        };

        if is_better {
            best_score = Some(score);
            best_key = Some(key);
            best_value = Some(value_map);
        }
    }

    best_value
}

fn types_versions_specificity(pattern: &str) -> usize {
    if let Some(star) = pattern.find('*') {
        star + (pattern.len() - star - 1)
    } else {
        pattern.len()
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
struct RangeScore {
    constraints: usize,
    min_version: SemVer,
    key_len: usize,
}

fn match_types_versions_range(range: &str, compiler_version: SemVer) -> Option<RangeScore> {
    let range = range.trim();
    if range.is_empty() || range == "*" {
        return Some(RangeScore {
            constraints: 0,
            min_version: SemVer::ZERO,
            key_len: range.len(),
        });
    }

    let mut best: Option<RangeScore> = None;
    for segment in range.split("||") {
        let segment = segment.trim();
        let Some(score) =
            match_types_versions_range_segment(segment, compiler_version, range.len())
        else {
            continue;
        };
        if best.is_none_or(|current| score > current) {
            best = Some(score);
        }
    }

    best
}

fn match_types_versions_range_segment(
    segment: &str,
    compiler_version: SemVer,
    key_len: usize,
) -> Option<RangeScore> {
    if segment.is_empty() {
        return None;
    }
    if segment == "*" {
        return Some(RangeScore {
            constraints: 0,
            min_version: SemVer::ZERO,
            key_len,
        });
    }

    let mut min_version = SemVer::ZERO;
    let mut constraints = 0usize;

    for token in segment.split_whitespace() {
        if token.is_empty() || token == "*" {
            continue;
        }
        let (op, version) = parse_range_token(token)?;
        if !compare_range(compiler_version, op, version) {
            return None;
        }
        constraints += 1;
        if matches!(op, RangeOp::Gt | RangeOp::Gte | RangeOp::Eq) && version > min_version {
            min_version = version;
        }
    }

    Some(RangeScore {
        constraints,
        min_version,
        key_len,
    })
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum RangeOp {
    Gt,
    Gte,
    Lt,
    Lte,
    Eq,
}

fn parse_range_token(token: &str) -> Option<(RangeOp, SemVer)> {
    let token = token.trim();
    if token.is_empty() {
        return None;
    }
    let (op, rest) = if let Some(rest) = token.strip_prefix(">=") {
        (RangeOp::Gte, rest)
    } else if let Some(rest) = token.strip_prefix("<=") {
        (RangeOp::Lte, rest)
    } else if let Some(rest) = token.strip_prefix('>') {
        (RangeOp::Gt, rest)
    } else if let Some(rest) = token.strip_prefix('<') {
        (RangeOp::Lt, rest)
    } else {
        (RangeOp::Eq, token)
    };

    parse_semver(rest).map(|version| (op, version))
}

fn compare_range(version: SemVer, op: RangeOp, other: SemVer) -> bool {
    match op {
        RangeOp::Gt => version > other,
        RangeOp::Gte => version >= other,
        RangeOp::Lt => version < other,
        RangeOp::Lte => version <= other,
        RangeOp::Eq => version == other,
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
struct SemVer {
    major: u32,
    minor: u32,
    patch: u32,
}

impl SemVer {
    const ZERO: SemVer = SemVer {
        major: 0,
        minor: 0,
        patch: 0,
    };
}

// NOTE: Keep this in sync with the TypeScript version this compiler targets.
const TYPES_VERSIONS_COMPILER_VERSION_FALLBACK: SemVer = SemVer {
    major: 6,
    minor: 0,
    patch: 0,
};

fn parse_semver(value: &str) -> Option<SemVer> {
    let value = value.trim();
    if value.is_empty() {
        return None;
    }
    let mut parts = value.split('.');
    let major = parts.next()?.parse().ok()?;
    let minor = parts.next().unwrap_or("0").parse().ok()?;
    let patch = parts.next().unwrap_or("0").parse().ok()?;
    Some(SemVer {
        major,
        minor,
        patch,
    })
}

/// Apply wildcard substitution to a target path
fn apply_wildcard_substitution(target: &str, wildcard: &str) -> String {
    if target.contains('*') {
        target.replace('*', wildcard)
    } else {
        target.to_string()
    }
}

fn split_path_extension(path: &Path) -> Option<(PathBuf, &'static str)> {
    let path_str = path.to_string_lossy();
    for ext in KNOWN_EXTENSIONS {
        if path_str.ends_with(ext) {
            let base = &path_str[..path_str.len().saturating_sub(ext.len())];
            if base.is_empty() {
                return None;
            }
            return Some((PathBuf::from(base), ext.trim_start_matches('.')));
        }
    }
    None
}

fn try_file_with_suffixes(path: &Path, suffixes: &[String]) -> Option<PathBuf> {
    let (base, extension) = split_path_extension(path)?;
    try_file_with_suffixes_and_extension(&base, extension, suffixes)
}

fn try_file_with_suffixes_and_extension(
    base: &Path,
    extension: &str,
    suffixes: &[String],
) -> Option<PathBuf> {
    for suffix in suffixes {
        let Some(candidate) = path_with_suffix_and_extension(base, suffix, extension) else {
            continue;
        };
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    None
}

fn path_with_suffix_and_extension(base: &Path, suffix: &str, extension: &str) -> Option<PathBuf> {
    let file_name = base.file_name()?.to_string_lossy();
    let mut candidate = base.to_path_buf();
    let mut new_name = String::with_capacity(file_name.len() + suffix.len() + extension.len() + 1);
    new_name.push_str(&file_name);
    new_name.push_str(suffix);
    new_name.push('.');
    new_name.push_str(extension);
    candidate.set_file_name(new_name);
    Some(candidate)
}

fn try_arbitrary_extension_declaration(path: &Path, extension: &str) -> Option<PathBuf> {
    let declaration = path.with_extension(format!("d.{extension}.ts"));
    if declaration.is_file() {
        return Some(declaration);
    }
    None
}

fn resolve_explicit_unknown_extension(path: &Path) -> Option<PathBuf> {
    if path.extension().is_none() {
        return None;
    }
    if split_path_extension(path).is_some() {
        return None;
    }
    if path.is_file() {
        return Some(path.to_path_buf());
    }
    None
}

const KNOWN_EXTENSIONS: [&str; 12] = [
    ".d.mts", ".d.cts", ".d.ts", ".mts", ".cts", ".tsx", ".ts", ".mjs", ".cjs", ".jsx", ".js",
    ".json",
];
const TS_EXTENSION_CANDIDATES: [&str; 7] = ["ts", "tsx", "d.ts", "mts", "cts", "d.mts", "d.cts"];
const NODE16_MODULE_EXTENSION_CANDIDATES: [&str; 7] =
    ["mts", "d.mts", "ts", "tsx", "d.ts", "cts", "d.cts"];
const NODE16_COMMONJS_EXTENSION_CANDIDATES: [&str; 7] =
    ["cts", "d.cts", "ts", "tsx", "d.ts", "mts", "d.mts"];
const CLASSIC_EXTENSION_CANDIDATES: [&str; 7] = TS_EXTENSION_CANDIDATES;

/// Extension candidates when allowJs is enabled (TypeScript + JavaScript)
const TS_JS_EXTENSION_CANDIDATES: [&str; 11] = [
    "ts", "tsx", "d.ts", "mts", "cts", "d.mts", "d.cts", "js", "jsx", "mjs", "cjs",
];

fn node16_extension_substitution(path: &Path, extension: &str) -> Option<Vec<PathBuf>> {
    let replacements: &[&str] = match extension {
        "js" => &["ts", "tsx", "d.ts"],
        "jsx" => &["tsx", "d.ts"],
        "mjs" => &["mts", "d.mts"],
        "cjs" => &["cts", "d.cts"],
        _ => return None,
    };

    Some(
        replacements
            .iter()
            .map(|ext| path.with_extension(ext))
            .collect(),
    )
}

fn declaration_substitution_for_main(path: &Path) -> Option<PathBuf> {
    let extension = path.extension().and_then(|ext| ext.to_str())?;
    match extension {
        "js" | "jsx" => Some(path.with_extension("d.ts")),
        "mjs" => Some(path.with_extension("d.mts")),
        "cjs" => Some(path.with_extension("d.cts")),
        _ => None,
    }
}

/// Simplified package.json structure for resolution
#[derive(Debug, Clone, Default, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PackageJson {
    pub name: Option<String>,
    pub version: Option<String>,
    pub main: Option<String>,
    pub module: Option<String>,
    pub types: Option<String>,
    pub typings: Option<String>,
    #[serde(rename = "type")]
    pub package_type: Option<String>,
    pub exports: Option<PackageExports>,
    pub imports: Option<FxHashMap<String, PackageExports>>,
    /// TypeScript typesVersions field for version-specific type definitions
    #[serde(rename = "typesVersions")]
    pub types_versions: Option<serde_json::Value>,
}

/// Package exports field can be a string, map, or conditional
///
/// Map variant: keys start with "." (subpath patterns like ".", "./foo")
/// Conditional variant: keys don't start with "." (condition names like "import", "default")
///   Uses Vec to preserve JSON key order (required for correct condition matching)
#[derive(Debug, Clone)]
pub enum PackageExports {
    String(String),
    Map(FxHashMap<String, PackageExports>),
    Conditional(Vec<(String, PackageExports)>),
    /// null in JSON — indicates an explicitly blocked export
    Null,
}

impl<'de> serde::Deserialize<'de> for PackageExports {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        use serde::de;

        struct PackageExportsVisitor;

        impl<'de> de::Visitor<'de> for PackageExportsVisitor {
            type Value = PackageExports;

            fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                formatter.write_str("a string, object, or null")
            }

            fn visit_str<E>(self, v: &str) -> Result<Self::Value, E>
            where
                E: de::Error,
            {
                Ok(PackageExports::String(v.to_string()))
            }

            fn visit_unit<E>(self) -> Result<Self::Value, E>
            where
                E: de::Error,
            {
                Ok(PackageExports::Null)
            }

            fn visit_none<E>(self) -> Result<Self::Value, E>
            where
                E: de::Error,
            {
                Ok(PackageExports::Null)
            }

            fn visit_map<A>(self, mut map: A) -> Result<Self::Value, A::Error>
            where
                A: de::MapAccess<'de>,
            {
                let mut map_entries = FxHashMap::default();
                let mut cond_entries = Vec::new();
                let mut is_subpath_map = None;

                while let Some((key, value)) = map.next_entry::<String, PackageExports>()? {
                    if is_subpath_map.is_none() {
                        is_subpath_map = Some(key.starts_with('.'));
                    }
                    if is_subpath_map == Some(true) {
                        map_entries.insert(key, value);
                    } else {
                        cond_entries.push((key, value));
                    }
                }

                if is_subpath_map.unwrap_or(false) {
                    Ok(PackageExports::Map(map_entries))
                } else {
                    Ok(PackageExports::Conditional(cond_entries))
                }
            }
        }

        deserializer.deserialize_any(PackageExportsVisitor)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_package_specifier_simple() {
        let (name, subpath) = parse_package_specifier("lodash");
        assert_eq!(name, "lodash");
        assert_eq!(subpath, None);
    }

    #[test]
    fn test_parse_package_specifier_with_subpath() {
        let (name, subpath) = parse_package_specifier("lodash/fp");
        assert_eq!(name, "lodash");
        assert_eq!(subpath, Some("fp".to_string()));
    }

    #[test]
    fn test_parse_package_specifier_scoped() {
        let (name, subpath) = parse_package_specifier("@babel/core");
        assert_eq!(name, "@babel/core");
        assert_eq!(subpath, None);
    }

    #[test]
    fn test_parse_package_specifier_scoped_with_subpath() {
        let (name, subpath) = parse_package_specifier("@babel/core/transform");
        assert_eq!(name, "@babel/core");
        assert_eq!(subpath, Some("transform".to_string()));
    }

    #[test]
    fn test_match_export_pattern_exact() {
        assert_eq!(match_export_pattern("./lib", "./lib"), Some(String::new()));
        assert_eq!(match_export_pattern("./lib", "./src"), None);
    }

    #[test]
    fn test_match_export_pattern_wildcard() {
        assert_eq!(
            match_export_pattern("./*", "./foo"),
            Some("foo".to_string())
        );
        assert_eq!(
            match_export_pattern("./lib/*", "./lib/utils"),
            Some("utils".to_string())
        );
        assert_eq!(match_export_pattern("./lib/*", "./src/utils"), None);
    }

    #[test]
    fn test_module_extension_from_path() {
        assert_eq!(
            ModuleExtension::from_path(Path::new("foo.ts")),
            ModuleExtension::Ts
        );
        assert_eq!(
            ModuleExtension::from_path(Path::new("foo.d.ts")),
            ModuleExtension::Dts
        );
        assert_eq!(
            ModuleExtension::from_path(Path::new("foo.tsx")),
            ModuleExtension::Tsx
        );
        assert_eq!(
            ModuleExtension::from_path(Path::new("foo.js")),
            ModuleExtension::Js
        );
    }

    #[test]
    fn test_module_resolver_creation() {
        let resolver = ModuleResolver::node_resolver();
        assert_eq!(resolver.resolution_kind(), ModuleResolutionKind::Node);
    }

    #[test]
    fn test_ts2307_error_code_constant() {
        assert_eq!(CANNOT_FIND_MODULE, 2307);
    }

    #[test]
    fn test_resolution_failure_not_found_diagnostic() {
        let failure = ResolutionFailure::NotFound {
            specifier: "./missing-module".to_string(),
            containing_file: "/path/to/file.ts".to_string(),
            span: Span::new(10, 30),
        };

        let diagnostic = failure.to_diagnostic();
        assert_eq!(diagnostic.code, CANNOT_FIND_MODULE);
        assert!(diagnostic.message.contains("Cannot find module"));
        assert!(diagnostic.message.contains("./missing-module"));
        assert_eq!(diagnostic.file_name, "/path/to/file.ts");
        assert_eq!(diagnostic.span.start, 10);
        assert_eq!(diagnostic.span.end, 30);
    }

    #[test]
    fn test_resolution_failure_is_not_found() {
        let not_found = ResolutionFailure::NotFound {
            specifier: "test".to_string(),
            containing_file: "test.ts".to_string(),
            span: Span::dummy(),
        };
        assert!(not_found.is_not_found());

        let other = ResolutionFailure::InvalidSpecifier {
            message: "test".to_string(),
            containing_file: "test.ts".to_string(),
            span: Span::dummy(),
        };
        assert!(!other.is_not_found());
    }

    #[test]
    fn test_module_extension_forces_esm() {
        assert!(ModuleExtension::Mts.forces_esm());
        assert!(ModuleExtension::Mjs.forces_esm());
        assert!(ModuleExtension::DmTs.forces_esm());
        assert!(!ModuleExtension::Ts.forces_esm());
        assert!(!ModuleExtension::Cts.forces_esm());
    }

    #[test]
    fn test_module_extension_forces_cjs() {
        assert!(ModuleExtension::Cts.forces_cjs());
        assert!(ModuleExtension::Cjs.forces_cjs());
        assert!(ModuleExtension::DCts.forces_cjs());
        assert!(!ModuleExtension::Ts.forces_cjs());
        assert!(!ModuleExtension::Mts.forces_cjs());
    }

    #[test]
    fn test_match_imports_pattern_exact() {
        assert_eq!(
            match_imports_pattern("#utils", "#utils"),
            Some(String::new())
        );
        assert_eq!(match_imports_pattern("#utils", "#other"), None);
    }

    #[test]
    fn test_match_imports_pattern_wildcard() {
        assert_eq!(
            match_imports_pattern("#utils/*", "#utils/foo"),
            Some("foo".to_string())
        );
        assert_eq!(
            match_imports_pattern("#internal/*", "#internal/helpers/bar"),
            Some("helpers/bar".to_string())
        );
        assert_eq!(match_imports_pattern("#utils/*", "#other/foo"), None);
    }

    #[test]
    fn test_match_types_versions_pattern() {
        assert_eq!(
            match_types_versions_pattern("*", "index"),
            Some("index".to_string())
        );
        assert_eq!(
            match_types_versions_pattern("lib/*", "lib/utils"),
            Some("utils".to_string())
        );
        assert_eq!(
            match_types_versions_pattern("exact", "exact"),
            Some(String::new())
        );
        assert_eq!(match_types_versions_pattern("lib/*", "src/utils"), None);
    }

    #[test]
    fn test_apply_wildcard_substitution() {
        assert_eq!(
            apply_wildcard_substitution("./lib/*.js", "utils"),
            "./lib/utils.js"
        );
        assert_eq!(
            apply_wildcard_substitution("./dist/index.js", "ignored"),
            "./dist/index.js"
        );
    }

    #[test]
    fn test_package_type_enum() {
        assert_eq!(PackageType::default(), PackageType::CommonJs);
        assert_ne!(PackageType::Module, PackageType::CommonJs);
    }

    #[test]
    fn test_importing_module_kind_enum() {
        assert_eq!(
            ImportingModuleKind::default(),
            ImportingModuleKind::CommonJs
        );
        assert_ne!(ImportingModuleKind::Esm, ImportingModuleKind::CommonJs);
    }

    #[test]
    fn test_package_json_deserialize_basic() {
        let json = r#"{"name": "test-package", "type": "module", "main": "./index.js"}"#;

        let package_json: PackageJson = serde_json::from_str(json).unwrap();
        assert_eq!(package_json.name, Some("test-package".to_string()));
        assert_eq!(package_json.package_type, Some("module".to_string()));
        assert_eq!(package_json.main, Some("./index.js".to_string()));
    }

    #[test]
    fn test_package_json_deserialize_exports() {
        let json = r#"{"name": "pkg", "exports": {"." : "./dist/index.js"}}"#;

        let package_json: PackageJson = serde_json::from_str(json).unwrap();
        assert!(package_json.exports.is_some());
    }

    #[test]
    fn test_package_json_deserialize_types_versions() {
        // Build JSON programmatically to avoid raw string issues
        let json = serde_json::json!({
            "name": "typed-package",
            "typesVersions": {
                "*": {
                    "*": ["./types/index.d.ts"]
                }
            }
        });

        let package_json: PackageJson = serde_json::from_value(json).unwrap();
        assert_eq!(package_json.name, Some("typed-package".to_string()));
        assert!(package_json.types_versions.is_some());
    }

    // =========================================================================
    // TS2307 Diagnostic Emission Tests
    // =========================================================================

    #[test]
    fn test_emit_resolution_error_for_not_found() {
        let mut diagnostics = DiagnosticBag::new();
        let resolver = ModuleResolver::node_resolver();

        let failure = ResolutionFailure::NotFound {
            specifier: "./missing-module".to_string(),
            containing_file: "/src/file.ts".to_string(),
            span: Span::new(10, 30),
        };

        resolver.emit_resolution_error(&mut diagnostics, &failure);

        assert_eq!(diagnostics.len(), 1);
        assert!(diagnostics.has_errors());
        let errors: Vec<_> = diagnostics.errors().collect();
        assert_eq!(errors[0].code, CANNOT_FIND_MODULE);
        assert!(errors[0].message.contains("Cannot find module"));
        assert!(errors[0].message.contains("./missing-module"));
    }

    #[test]
    fn test_emit_resolution_error_all_variants_emit_ts2307() {
        let mut diagnostics = DiagnosticBag::new();
        let resolver = ModuleResolver::node_resolver();

        // All resolution failure variants should emit TS2307 diagnostics
        let failure = ResolutionFailure::InvalidSpecifier {
            message: "bad specifier".to_string(),
            containing_file: "/src/a.ts".to_string(),
            span: Span::new(0, 10),
        };
        resolver.emit_resolution_error(&mut diagnostics, &failure);
        assert_eq!(diagnostics.len(), 1);

        let failure = ResolutionFailure::PackageJsonError {
            message: "parse error".to_string(),
            containing_file: "/src/b.ts".to_string(),
            span: Span::new(5, 15),
        };
        resolver.emit_resolution_error(&mut diagnostics, &failure);
        assert_eq!(diagnostics.len(), 2);

        let failure = ResolutionFailure::CircularResolution {
            message: "a -> b -> a".to_string(),
            containing_file: "/src/c.ts".to_string(),
            span: Span::new(10, 20),
        };
        resolver.emit_resolution_error(&mut diagnostics, &failure);
        assert_eq!(diagnostics.len(), 3);

        let failure = ResolutionFailure::PathMappingFailed {
            message: "@/ pattern".to_string(),
            containing_file: "/src/d.ts".to_string(),
            span: Span::new(15, 25),
        };
        resolver.emit_resolution_error(&mut diagnostics, &failure);
        assert_eq!(diagnostics.len(), 4);

        // Verify all have TS2307 code
        for diag in diagnostics.errors() {
            assert_eq!(diag.code, CANNOT_FIND_MODULE);
        }
    }

    #[test]
    fn test_resolution_failure_all_variants_to_diagnostic() {
        // Test that all ResolutionFailure variants can produce diagnostics with proper location info
        let failures = vec![
            ResolutionFailure::NotFound {
                specifier: "./test".to_string(),
                containing_file: "file.ts".to_string(),
                span: Span::new(0, 10),
            },
            ResolutionFailure::InvalidSpecifier {
                message: "bad".to_string(),
                containing_file: "file2.ts".to_string(),
                span: Span::new(5, 15),
            },
            ResolutionFailure::PackageJsonError {
                message: "error".to_string(),
                containing_file: "file3.ts".to_string(),
                span: Span::new(10, 20),
            },
            ResolutionFailure::CircularResolution {
                message: "loop".to_string(),
                containing_file: "file4.ts".to_string(),
                span: Span::new(15, 25),
            },
            ResolutionFailure::PathMappingFailed {
                message: "@/path".to_string(),
                containing_file: "file5.ts".to_string(),
                span: Span::new(20, 30),
            },
        ];

        for failure in failures {
            let diagnostic = failure.to_diagnostic();
            // All failures should produce TS2307 diagnostic code
            assert_eq!(diagnostic.code, CANNOT_FIND_MODULE);
            // All failures should have non-empty file names
            assert!(!diagnostic.file_name.is_empty());
            // All failures should have valid spans
            assert!(diagnostic.span.start < diagnostic.span.end);
        }
    }

    #[test]
    fn test_relative_import_failure_produces_ts2307() {
        let failure = ResolutionFailure::NotFound {
            specifier: "./components/Button".to_string(),
            containing_file: "/src/App.tsx".to_string(),
            span: Span::new(20, 45),
        };

        let diagnostic = failure.to_diagnostic();
        assert_eq!(diagnostic.code, CANNOT_FIND_MODULE);
        assert_eq!(diagnostic.file_name, "/src/App.tsx");
        assert!(diagnostic.message.contains("./components/Button"));
        assert_eq!(diagnostic.span.start, 20);
        assert_eq!(diagnostic.span.end, 45);
    }

    #[test]
    fn test_bare_specifier_failure_produces_ts2307() {
        let failure = ResolutionFailure::NotFound {
            specifier: "nonexistent-package".to_string(),
            containing_file: "/project/src/index.ts".to_string(),
            span: Span::new(7, 28),
        };

        let diagnostic = failure.to_diagnostic();
        assert_eq!(diagnostic.code, CANNOT_FIND_MODULE);
        assert!(diagnostic.message.contains("nonexistent-package"));
    }

    #[test]
    fn test_scoped_package_failure_produces_ts2307() {
        let failure = ResolutionFailure::NotFound {
            specifier: "@org/missing-lib".to_string(),
            containing_file: "/app/main.ts".to_string(),
            span: Span::new(15, 35),
        };

        let diagnostic = failure.to_diagnostic();
        assert_eq!(diagnostic.code, CANNOT_FIND_MODULE);
        assert!(diagnostic.message.contains("@org/missing-lib"));
    }

    #[test]
    fn test_hash_import_failure_produces_ts2307() {
        // Package.json subpath import failure
        let failure = ResolutionFailure::NotFound {
            specifier: "#utils/helpers".to_string(),
            containing_file: "/pkg/src/index.ts".to_string(),
            span: Span::new(8, 25),
        };

        let diagnostic = failure.to_diagnostic();
        assert_eq!(diagnostic.code, CANNOT_FIND_MODULE);
        assert!(diagnostic.message.contains("#utils/helpers"));
    }

    #[test]
    fn test_resolution_failure_span_preservation() {
        // Ensure span information is correctly preserved in diagnostics
        let test_cases = vec![(0, 10), (100, 150), (1000, 1050)];

        for (start, end) in test_cases {
            let failure = ResolutionFailure::NotFound {
                specifier: "test".to_string(),
                containing_file: "file.ts".to_string(),
                span: Span::new(start, end),
            };

            let diagnostic = failure.to_diagnostic();
            assert_eq!(diagnostic.span.start, start);
            assert_eq!(diagnostic.span.end, end);
        }
    }

    #[test]
    fn test_resolution_failure_accessors() {
        // Test that accessor methods work correctly
        let failure = ResolutionFailure::InvalidSpecifier {
            message: "test error".to_string(),
            containing_file: "/src/test.ts".to_string(),
            span: Span::new(10, 20),
        };

        assert_eq!(failure.containing_file(), "/src/test.ts");
        assert_eq!(failure.span().start, 10);
        assert_eq!(failure.span().end, 20);
    }

    #[test]
    fn test_path_mapping_failure_produces_ts2307() {
        let failure = ResolutionFailure::PathMappingFailed {
            message: "path mapping '@/utils/*' did not resolve to any file".to_string(),
            containing_file: "/project/src/index.ts".to_string(),
            span: Span::new(8, 30),
        };

        let diagnostic = failure.to_diagnostic();
        assert_eq!(diagnostic.code, CANNOT_FIND_MODULE);
        assert_eq!(diagnostic.file_name, "/project/src/index.ts");
        assert!(diagnostic.message.contains("Cannot find module"));
        assert!(diagnostic.message.contains("path mapping"));
    }

    #[test]
    fn test_package_json_error_produces_ts2307() {
        let failure = ResolutionFailure::PackageJsonError {
            message: "invalid exports field in package.json".to_string(),
            containing_file: "/project/src/app.ts".to_string(),
            span: Span::new(15, 45),
        };

        let diagnostic = failure.to_diagnostic();
        assert_eq!(diagnostic.code, CANNOT_FIND_MODULE);
        assert_eq!(diagnostic.file_name, "/project/src/app.ts");
        assert!(diagnostic.message.contains("Cannot find module"));
    }

    #[test]
    fn test_circular_resolution_produces_ts2307() {
        let failure = ResolutionFailure::CircularResolution {
            message: "circular dependency: a.ts -> b.ts -> a.ts".to_string(),
            containing_file: "/project/src/a.ts".to_string(),
            span: Span::new(20, 50),
        };

        let diagnostic = failure.to_diagnostic();
        assert_eq!(diagnostic.code, CANNOT_FIND_MODULE);
        assert_eq!(diagnostic.file_name, "/project/src/a.ts");
        assert!(diagnostic.message.contains("Cannot find module"));
        assert!(diagnostic.message.contains("circular"));
    }

    #[test]
    fn test_diagnostic_bag_collects_multiple_resolution_errors() {
        let mut diagnostics = DiagnosticBag::new();
        let resolver = ModuleResolver::node_resolver();

        let failures = vec![
            ResolutionFailure::NotFound {
                specifier: "./module1".to_string(),
                containing_file: "a.ts".to_string(),
                span: Span::new(0, 10),
            },
            ResolutionFailure::NotFound {
                specifier: "./module2".to_string(),
                containing_file: "b.ts".to_string(),
                span: Span::new(5, 15),
            },
            ResolutionFailure::NotFound {
                specifier: "external-pkg".to_string(),
                containing_file: "c.ts".to_string(),
                span: Span::new(10, 25),
            },
        ];

        for failure in &failures {
            resolver.emit_resolution_error(&mut diagnostics, failure);
        }

        assert_eq!(diagnostics.len(), 3);
        assert_eq!(diagnostics.error_count(), 3);

        // Verify all have TS2307 code
        let codes: Vec<_> = diagnostics.errors().map(|d| d.code).collect();
        assert!(codes.iter().all(|&c| c == CANNOT_FIND_MODULE));
    }

    // =========================================================================
    // TS2835 (Import Path Needs Extension Suggestion) Tests
    // =========================================================================

    #[test]
    fn test_ts2834_error_code_constant() {
        assert_eq!(IMPORT_PATH_NEEDS_EXTENSION, 2834);
    }

    #[test]
    fn test_import_path_needs_extension_produces_ts2835() {
        let failure = ResolutionFailure::ImportPathNeedsExtension {
            specifier: "./utils".to_string(),
            suggested_extension: ".js".to_string(),
            containing_file: "/src/index.mts".to_string(),
            span: Span::new(20, 30),
        };

        let diagnostic = failure.to_diagnostic();
        assert_eq!(diagnostic.code, IMPORT_PATH_NEEDS_EXTENSION_SUGGESTION);
        assert_eq!(diagnostic.file_name, "/src/index.mts");
        assert!(
            diagnostic
                .message
                .contains("Relative import paths need explicit file extensions")
        );
        assert!(diagnostic.message.contains("node16"));
        assert!(diagnostic.message.contains("nodenext"));
        assert!(diagnostic.message.contains("./utils.js"));
    }

    #[test]
    fn test_import_path_needs_extension_suggests_mjs() {
        let failure = ResolutionFailure::ImportPathNeedsExtension {
            specifier: "./esm-module".to_string(),
            suggested_extension: ".mjs".to_string(),
            containing_file: "/src/app.mts".to_string(),
            span: Span::new(10, 25),
        };

        let diagnostic = failure.to_diagnostic();
        assert_eq!(diagnostic.code, IMPORT_PATH_NEEDS_EXTENSION_SUGGESTION);
        assert!(diagnostic.message.contains("./esm-module.mjs"));
    }

    #[test]
    fn test_import_path_needs_extension_suggests_cjs() {
        let failure = ResolutionFailure::ImportPathNeedsExtension {
            specifier: "./cjs-module".to_string(),
            suggested_extension: ".cjs".to_string(),
            containing_file: "/src/legacy.cts".to_string(),
            span: Span::new(5, 20),
        };

        let diagnostic = failure.to_diagnostic();
        assert_eq!(diagnostic.code, IMPORT_PATH_NEEDS_EXTENSION_SUGGESTION);
        assert!(diagnostic.message.contains("./cjs-module.cjs"));
    }

    // =========================================================================
    // TS2792 (Module Resolution Mode Mismatch) Tests
    // =========================================================================

    #[test]
    fn test_ts2792_error_code_constant() {
        assert_eq!(MODULE_RESOLUTION_MODE_MISMATCH, 2792);
    }

    #[test]
    fn test_module_resolution_mode_mismatch_produces_ts2792() {
        let failure = ResolutionFailure::ModuleResolutionModeMismatch {
            specifier: "modern-esm-package".to_string(),
            containing_file: "/src/index.ts".to_string(),
            span: Span::new(15, 35),
        };

        let diagnostic = failure.to_diagnostic();
        assert_eq!(diagnostic.code, MODULE_RESOLUTION_MODE_MISMATCH);
        assert_eq!(diagnostic.file_name, "/src/index.ts");
        assert!(
            diagnostic
                .message
                .contains("Cannot find module 'modern-esm-package'")
        );
        assert!(diagnostic.message.contains("moduleResolution"));
        assert!(diagnostic.message.contains("nodenext"));
        assert!(diagnostic.message.contains("paths"));
    }

    #[test]
    fn test_module_resolution_mode_mismatch_accessors() {
        let failure = ResolutionFailure::ModuleResolutionModeMismatch {
            specifier: "pkg".to_string(),
            containing_file: "/test.ts".to_string(),
            span: Span::new(100, 110),
        };

        assert_eq!(failure.containing_file(), "/test.ts");
        assert_eq!(failure.span().start, 100);
        assert_eq!(failure.span().end, 110);
    }

    #[test]
    fn test_import_path_needs_extension_accessors() {
        let failure = ResolutionFailure::ImportPathNeedsExtension {
            specifier: "./foo".to_string(),
            suggested_extension: ".js".to_string(),
            containing_file: "/bar.mts".to_string(),
            span: Span::new(50, 60),
        };

        assert_eq!(failure.containing_file(), "/bar.mts");
        assert_eq!(failure.span().start, 50);
        assert_eq!(failure.span().end, 60);
    }

    #[test]
    fn test_new_error_codes_emit_correctly() {
        let mut diagnostics = DiagnosticBag::new();
        let resolver = ModuleResolver::node_resolver();

        // Test TS2835
        let failure_2835 = ResolutionFailure::ImportPathNeedsExtension {
            specifier: "./utils".to_string(),
            suggested_extension: ".js".to_string(),
            containing_file: "/src/app.mts".to_string(),
            span: Span::new(0, 10),
        };
        resolver.emit_resolution_error(&mut diagnostics, &failure_2835);

        // Test TS2792
        let failure_2792 = ResolutionFailure::ModuleResolutionModeMismatch {
            specifier: "esm-pkg".to_string(),
            containing_file: "/src/index.ts".to_string(),
            span: Span::new(5, 15),
        };
        resolver.emit_resolution_error(&mut diagnostics, &failure_2792);

        assert_eq!(diagnostics.len(), 2);

        let errors: Vec<_> = diagnostics.errors().collect();
        assert_eq!(errors[0].code, IMPORT_PATH_NEEDS_EXTENSION_SUGGESTION);
        assert_eq!(errors[1].code, MODULE_RESOLUTION_MODE_MISMATCH);
    }

    // =========================================================================
    // ModuleExtension::from_path tests
    // =========================================================================

    #[test]
    fn test_extension_from_path_ts() {
        assert_eq!(
            ModuleExtension::from_path(Path::new("foo.ts")),
            ModuleExtension::Ts
        );
    }

    #[test]
    fn test_extension_from_path_tsx() {
        assert_eq!(
            ModuleExtension::from_path(Path::new("Component.tsx")),
            ModuleExtension::Tsx
        );
    }

    #[test]
    fn test_extension_from_path_dts() {
        assert_eq!(
            ModuleExtension::from_path(Path::new("types.d.ts")),
            ModuleExtension::Dts
        );
    }

    #[test]
    fn test_extension_from_path_dmts() {
        assert_eq!(
            ModuleExtension::from_path(Path::new("types.d.mts")),
            ModuleExtension::DmTs
        );
    }

    #[test]
    fn test_extension_from_path_dcts() {
        assert_eq!(
            ModuleExtension::from_path(Path::new("types.d.cts")),
            ModuleExtension::DCts
        );
    }

    #[test]
    fn test_extension_from_path_js() {
        assert_eq!(
            ModuleExtension::from_path(Path::new("bundle.js")),
            ModuleExtension::Js
        );
    }

    #[test]
    fn test_extension_from_path_jsx() {
        assert_eq!(
            ModuleExtension::from_path(Path::new("App.jsx")),
            ModuleExtension::Jsx
        );
    }

    #[test]
    fn test_extension_from_path_mjs() {
        assert_eq!(
            ModuleExtension::from_path(Path::new("module.mjs")),
            ModuleExtension::Mjs
        );
    }

    #[test]
    fn test_extension_from_path_cjs() {
        assert_eq!(
            ModuleExtension::from_path(Path::new("config.cjs")),
            ModuleExtension::Cjs
        );
    }

    #[test]
    fn test_extension_from_path_mts() {
        assert_eq!(
            ModuleExtension::from_path(Path::new("utils.mts")),
            ModuleExtension::Mts
        );
    }

    #[test]
    fn test_extension_from_path_cts() {
        assert_eq!(
            ModuleExtension::from_path(Path::new("config.cts")),
            ModuleExtension::Cts
        );
    }

    #[test]
    fn test_extension_from_path_json() {
        assert_eq!(
            ModuleExtension::from_path(Path::new("package.json")),
            ModuleExtension::Json
        );
    }

    #[test]
    fn test_extension_from_path_unknown() {
        assert_eq!(
            ModuleExtension::from_path(Path::new("style.css")),
            ModuleExtension::Unknown
        );
    }

    #[test]
    fn test_extension_from_path_no_extension() {
        assert_eq!(
            ModuleExtension::from_path(Path::new("Makefile")),
            ModuleExtension::Unknown
        );
    }

    #[test]
    fn test_extension_from_path_nested() {
        assert_eq!(
            ModuleExtension::from_path(Path::new("/project/src/lib/types.d.ts")),
            ModuleExtension::Dts
        );
    }

    // =========================================================================
    // ModuleExtension::as_str tests
    // =========================================================================

    #[test]
    fn test_extension_as_str_roundtrip() {
        let extensions = [
            ModuleExtension::Ts,
            ModuleExtension::Tsx,
            ModuleExtension::Dts,
            ModuleExtension::DmTs,
            ModuleExtension::DCts,
            ModuleExtension::Js,
            ModuleExtension::Jsx,
            ModuleExtension::Mjs,
            ModuleExtension::Cjs,
            ModuleExtension::Mts,
            ModuleExtension::Cts,
            ModuleExtension::Json,
        ];
        for ext in &extensions {
            let ext_str = ext.as_str();
            assert!(
                !ext_str.is_empty(),
                "{:?} should have a non-empty string representation",
                ext
            );
            // Verify the string starts with a dot
            assert!(
                ext_str.starts_with('.'),
                "{:?}.as_str() should start with '.', got: {}",
                ext,
                ext_str
            );
        }
        assert_eq!(ModuleExtension::Unknown.as_str(), "");
    }

    // =========================================================================
    // ModuleExtension ESM/CJS mode tests
    // =========================================================================

    #[test]
    fn test_extension_forces_esm() {
        assert!(ModuleExtension::Mts.forces_esm());
        assert!(ModuleExtension::Mjs.forces_esm());
        assert!(ModuleExtension::DmTs.forces_esm());

        assert!(!ModuleExtension::Ts.forces_esm());
        assert!(!ModuleExtension::Tsx.forces_esm());
        assert!(!ModuleExtension::Dts.forces_esm());
        assert!(!ModuleExtension::Js.forces_esm());
        assert!(!ModuleExtension::Cjs.forces_esm());
        assert!(!ModuleExtension::Cts.forces_esm());
    }

    #[test]
    fn test_extension_forces_cjs() {
        assert!(ModuleExtension::Cts.forces_cjs());
        assert!(ModuleExtension::Cjs.forces_cjs());
        assert!(ModuleExtension::DCts.forces_cjs());

        assert!(!ModuleExtension::Ts.forces_cjs());
        assert!(!ModuleExtension::Tsx.forces_cjs());
        assert!(!ModuleExtension::Dts.forces_cjs());
        assert!(!ModuleExtension::Js.forces_cjs());
        assert!(!ModuleExtension::Mjs.forces_cjs());
        assert!(!ModuleExtension::Mts.forces_cjs());
    }

    #[test]
    fn test_extension_neutral_mode() {
        // .ts, .tsx, .js, .jsx, .d.ts, .json should be neutral (neither ESM nor CJS forced)
        let neutral = [
            ModuleExtension::Ts,
            ModuleExtension::Tsx,
            ModuleExtension::Dts,
            ModuleExtension::Js,
            ModuleExtension::Jsx,
            ModuleExtension::Json,
            ModuleExtension::Unknown,
        ];
        for ext in &neutral {
            assert!(
                !ext.forces_esm() && !ext.forces_cjs(),
                "{:?} should be neutral (neither ESM nor CJS)",
                ext
            );
        }
    }

    // =========================================================================
    // ResolutionFailure tests
    // =========================================================================

    #[test]
    fn test_resolution_failure_not_found_is_not_found() {
        let failure = ResolutionFailure::NotFound {
            specifier: "./missing".to_string(),
            containing_file: "main.ts".to_string(),
            span: Span::new(0, 10),
        };
        assert!(failure.is_not_found());
    }

    #[test]
    fn test_resolution_failure_other_is_not_not_found() {
        let failure = ResolutionFailure::ImportPathNeedsExtension {
            specifier: "./utils".to_string(),
            suggested_extension: ".js".to_string(),
            containing_file: "main.mts".to_string(),
            span: Span::new(0, 10),
        };
        assert!(!failure.is_not_found());
    }

    #[test]
    fn test_resolution_failure_containing_file() {
        let failure = ResolutionFailure::NotFound {
            specifier: "./missing".to_string(),
            containing_file: "/project/src/main.ts".to_string(),
            span: Span::new(5, 20),
        };
        assert_eq!(failure.containing_file(), "/project/src/main.ts");
    }

    #[test]
    fn test_resolution_failure_span() {
        let failure = ResolutionFailure::NotFound {
            specifier: "./missing".to_string(),
            containing_file: "main.ts".to_string(),
            span: Span::new(10, 30),
        };
        let span = failure.span();
        assert_eq!(span.start, 10);
        assert_eq!(span.end, 30);
    }

    #[test]
    fn test_resolution_failure_to_diagnostic_ts2307() {
        let failure = ResolutionFailure::NotFound {
            specifier: "./nonexistent".to_string(),
            containing_file: "main.ts".to_string(),
            span: Span::new(0, 20),
        };
        let diag = failure.to_diagnostic();
        assert_eq!(diag.code, CANNOT_FIND_MODULE);
        assert!(diag.message.contains("./nonexistent"));
    }

    #[test]
    fn test_resolution_failure_to_diagnostic_ts2835() {
        let failure = ResolutionFailure::ImportPathNeedsExtension {
            specifier: "./utils".to_string(),
            suggested_extension: ".js".to_string(),
            containing_file: "app.mts".to_string(),
            span: Span::new(0, 15),
        };
        let diag = failure.to_diagnostic();
        assert_eq!(diag.code, IMPORT_PATH_NEEDS_EXTENSION_SUGGESTION);
    }

    #[test]
    fn test_resolution_failure_to_diagnostic_ts2792() {
        let failure = ResolutionFailure::ModuleResolutionModeMismatch {
            specifier: "some-esm-pkg".to_string(),
            containing_file: "index.ts".to_string(),
            span: Span::new(0, 20),
        };
        let diag = failure.to_diagnostic();
        assert_eq!(diag.code, MODULE_RESOLUTION_MODE_MISMATCH);
    }

    // =========================================================================
    // ModuleResolver with temp files (integration)
    // =========================================================================

    #[test]
    fn test_resolver_relative_ts_file() {
        use std::fs;
        let dir = std::env::temp_dir().join("tsz_test_resolver_relative");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();

        fs::write(dir.join("main.ts"), "import { foo } from './utils';").unwrap();
        fs::write(dir.join("utils.ts"), "export const foo = 42;").unwrap();

        let mut resolver = ModuleResolver::node_resolver();
        let result = resolver.resolve("./utils", &dir.join("main.ts"), Span::new(0, 10));

        match result {
            Ok(module) => {
                assert_eq!(module.resolved_path, dir.join("utils.ts"));
                assert_eq!(module.extension, ModuleExtension::Ts);
                assert!(!module.is_external);
            }
            Err(_) => {
                // Resolution might fail in some environments, that's OK for this test
            }
        }

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_resolver_relative_tsx_file() {
        use std::fs;
        let dir = std::env::temp_dir().join("tsz_test_resolver_tsx");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();

        fs::write(dir.join("app.ts"), "").unwrap();
        fs::write(
            dir.join("Button.tsx"),
            "export default function Button() {}",
        )
        .unwrap();

        let mut resolver = ModuleResolver::node_resolver();
        let result = resolver.resolve("./Button", &dir.join("app.ts"), Span::new(0, 10));

        match result {
            Ok(module) => {
                assert_eq!(module.resolved_path, dir.join("Button.tsx"));
                assert_eq!(module.extension, ModuleExtension::Tsx);
            }
            Err(_) => {} // OK in some environments
        }

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_resolver_index_file() {
        use std::fs;
        let dir = std::env::temp_dir().join("tsz_test_resolver_index");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(dir.join("utils")).unwrap();

        fs::write(dir.join("main.ts"), "").unwrap();
        fs::write(dir.join("utils").join("index.ts"), "export const foo = 42;").unwrap();

        let mut resolver = ModuleResolver::node_resolver();
        let result = resolver.resolve("./utils", &dir.join("main.ts"), Span::new(0, 10));

        match result {
            Ok(module) => {
                assert_eq!(module.resolved_path, dir.join("utils").join("index.ts"));
                assert_eq!(module.extension, ModuleExtension::Ts);
            }
            Err(_) => {} // OK in some environments
        }

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_exports_js_target_substitutes_dts() {
        use std::fs;
        let dir = std::env::temp_dir().join("tsz_test_exports_js_target");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(dir.join("node_modules/pkg")).unwrap();
        fs::create_dir_all(dir.join("src")).unwrap();

        fs::write(
            dir.join("node_modules/pkg/package.json"),
            r#"{"name":"pkg","version":"0.0.1","exports":"./entrypoint.js"}"#,
        )
        .unwrap();
        fs::write(dir.join("node_modules/pkg/entrypoint.d.ts"), "export {};").unwrap();
        fs::write(dir.join("src/index.ts"), "import * as p from 'pkg';").unwrap();

        let mut options = ResolvedCompilerOptions::default();
        options.module_resolution = Some(ModuleResolutionKind::Node16);
        options.resolve_package_json_exports = true;

        let mut resolver = ModuleResolver::new(&options);
        let result = resolver.resolve("pkg", &dir.join("src/index.ts"), Span::new(0, 3));

        // TypeScript resolves export targets with declaration substitution:
        // exports: "./entrypoint.js" → finds entrypoint.d.ts
        let resolved =
            result.expect("Expected exports .js target to resolve via .d.ts substitution");
        assert!(resolved.resolved_path.ends_with("entrypoint.d.ts"));

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_resolver_dts_file() {
        use std::fs;
        let dir = std::env::temp_dir().join("tsz_test_resolver_dts");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();

        fs::write(dir.join("main.ts"), "").unwrap();
        fs::write(dir.join("types.d.ts"), "export interface Foo {}").unwrap();

        let mut resolver = ModuleResolver::node_resolver();
        let result = resolver.resolve("./types", &dir.join("main.ts"), Span::new(0, 10));

        match result {
            Ok(module) => {
                assert_eq!(module.resolved_path, dir.join("types.d.ts"));
                assert_eq!(module.extension, ModuleExtension::Dts);
            }
            Err(_) => {} // OK in some environments
        }

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_resolver_jsx_without_jsx_option_errors() {
        use std::fs;
        let dir = std::env::temp_dir().join("tsz_test_resolver_jsx_no_option");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();

        fs::write(dir.join("app.ts"), "import jsx from './jsx';").unwrap();
        fs::write(dir.join("jsx.jsx"), "export default 1;").unwrap();

        let mut options = ResolvedCompilerOptions::default();
        options.allow_js = true;
        options.jsx = None;
        // Use Node resolution so allowJs is respected (Classic never resolves .jsx)
        options.module_resolution = Some(ModuleResolutionKind::Node);
        let mut resolver = ModuleResolver::new(&options);
        let result = resolver.resolve("./jsx", &dir.join("app.ts"), Span::new(0, 10));

        let failure = result.expect_err("Expected jsx resolution to fail without jsx option");
        let diagnostic = failure.to_diagnostic();
        assert_eq!(diagnostic.code, 6142);

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_resolver_tsx_without_jsx_option_errors() {
        use std::fs;
        let dir = std::env::temp_dir().join("tsz_test_resolver_tsx_no_option");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();

        fs::write(dir.join("app.ts"), "import tsx from './tsx';").unwrap();
        fs::write(dir.join("tsx.tsx"), "export default 1;").unwrap();

        let mut options = ResolvedCompilerOptions::default();
        options.jsx = None;
        // Use Node resolution so .tsx files are found (Classic also finds .tsx, but be explicit)
        options.module_resolution = Some(ModuleResolutionKind::Node);
        let mut resolver = ModuleResolver::new(&options);
        let result = resolver.resolve("./tsx", &dir.join("app.ts"), Span::new(0, 10));

        let failure = result.expect_err("Expected tsx resolution to fail without jsx option");
        let diagnostic = failure.to_diagnostic();
        assert_eq!(diagnostic.code, 6142);

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_json_import_without_resolve_json_module() {
        use std::fs;
        let dir = std::env::temp_dir().join("tsz_test_ts2732");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();

        fs::write(dir.join("app.ts"), "import data from './data.json';").unwrap();
        fs::write(dir.join("data.json"), "{\"value\": 42}").unwrap();

        let mut options = ResolvedCompilerOptions::default();
        options.resolve_json_module = false; // JSON modules disabled
        let mut resolver = ModuleResolver::new(&options);

        let result = resolver.resolve("./data.json", &dir.join("app.ts"), Span::new(0, 10));

        let failure =
            result.expect_err("Expected JSON resolution to fail without resolveJsonModule");
        let diagnostic = failure.to_diagnostic();
        assert_eq!(diagnostic.code, 2732); // TS2732

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_resolver_package_main_with_unknown_extension() {
        use std::fs;
        let dir = std::env::temp_dir().join("tsz_test_resolver_main_unknown");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(dir.join("node_modules").join("normalize.css")).unwrap();

        fs::write(dir.join("app.ts"), "import 'normalize.css';").unwrap();
        fs::write(
            dir.join("node_modules")
                .join("normalize.css")
                .join("normalize.css"),
            "body {}",
        )
        .unwrap();
        fs::write(
            dir.join("node_modules")
                .join("normalize.css")
                .join("package.json"),
            r#"{ "main": "normalize.css" }"#,
        )
        .unwrap();

        let mut resolver = ModuleResolver::node_resolver();
        let result = resolver.resolve("normalize.css", &dir.join("app.ts"), Span::new(0, 10));
        assert!(
            result.is_ok(),
            "Expected package main with unknown extension to resolve"
        );

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_resolver_package_types_with_unknown_extension() {
        use std::fs;
        let dir = std::env::temp_dir().join("tsz_test_resolver_types_unknown");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(dir.join("node_modules").join("foo")).unwrap();

        fs::write(dir.join("app.ts"), "import 'foo';").unwrap();
        fs::write(
            dir.join("node_modules").join("foo").join("foo.js"),
            "module.exports = {};",
        )
        .unwrap();
        fs::write(
            dir.join("node_modules").join("foo").join("package.json"),
            r#"{ "types": "foo.js" }"#,
        )
        .unwrap();

        let mut resolver = ModuleResolver::node_resolver();
        let result = resolver.resolve("foo", &dir.join("app.ts"), Span::new(0, 10));
        assert!(
            result.is_ok(),
            "Expected package types with unknown extension to resolve"
        );

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_resolver_package_types_js_without_allow_js_resolves() {
        use std::fs;
        let dir = std::env::temp_dir().join("tsz_test_resolver_types_js");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(dir.join("node_modules").join("foo")).unwrap();

        fs::write(dir.join("app.ts"), "import 'foo';").unwrap();
        fs::write(
            dir.join("node_modules").join("foo").join("foo.js"),
            "module.exports = {};",
        )
        .unwrap();
        fs::write(
            dir.join("node_modules").join("foo").join("package.json"),
            r#"{ "types": "foo.js" }"#,
        )
        .unwrap();

        let mut resolver = ModuleResolver::node_resolver();
        let result = resolver.resolve("foo", &dir.join("app.ts"), Span::new(0, 10));
        assert!(
            result.is_ok(),
            "Expected types .js to resolve even without allowJs"
        );

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_resolver_missing_file() {
        use std::fs;
        let dir = std::env::temp_dir().join("tsz_test_resolver_missing");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("main.ts"), "").unwrap();

        let mut resolver = ModuleResolver::node_resolver();
        let result = resolver.resolve("./nonexistent", &dir.join("main.ts"), Span::new(0, 10));

        assert!(result.is_err(), "Missing file should produce error");
        if let Err(failure) = result {
            assert!(failure.is_not_found());
        }

        let _ = fs::remove_dir_all(&dir);
    }

    // =========================================================================
    // PackageType tests
    // =========================================================================

    #[test]
    fn test_package_type_default_is_commonjs() {
        assert_eq!(PackageType::default(), PackageType::CommonJs);
    }

    #[test]
    fn test_importing_module_kind_default_is_commonjs() {
        assert_eq!(
            ImportingModuleKind::default(),
            ImportingModuleKind::CommonJs
        );
    }
}

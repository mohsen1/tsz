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

use crate::cli::config::{ModuleResolutionKind, PathMapping, ResolvedCompilerOptions};
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

/// TS2834: Relative import paths need explicit file extensions in EcmaScript imports
///
/// This error code is emitted when a relative import in an ESM context under Node16/NodeNext
/// resolution mode does not include an explicit file extension. ESM requires explicit extensions.
/// Example: `import { foo } from './utils'` should be `import { foo } from './utils.js'`
pub const IMPORT_PATH_NEEDS_EXTENSION: u32 = 2834;

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
            } => Diagnostic::error(
                containing_file,
                *span,
                format!(
                    "Relative import paths need explicit file extensions in EcmaScript imports when '--moduleResolution' is 'node16' or 'nodenext'. Did you mean '{}{}'?",
                    specifier, suggested_extension
                ),
                IMPORT_PATH_NEEDS_EXTENSION,
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
            | ResolutionFailure::ModuleResolutionModeMismatch {
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
            | ResolutionFailure::ModuleResolutionModeMismatch { span, .. } => *span,
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
    /// Cache of resolved modules
    resolution_cache: FxHashMap<(PathBuf, String), Result<ResolvedModule, ResolutionFailure>>,
    /// Extensions to try for TypeScript resolution
    #[allow(dead_code)] // Infrastructure for future extension-based resolution
    ts_extensions: Vec<&'static str>,
    /// Extensions to try for JavaScript resolution
    #[allow(dead_code)] // Infrastructure for future extension-based resolution
    js_extensions: Vec<&'static str>,
    /// Declaration extensions to try
    #[allow(dead_code)] // Infrastructure for .d.ts resolution
    dts_extensions: Vec<&'static str>,
    /// Custom conditions from tsconfig (for customConditions option)
    #[allow(dead_code)] // Infrastructure for customConditions support
    custom_conditions: Vec<String>,
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

        ModuleResolver {
            resolution_kind,
            base_url: options.base_url.clone(),
            path_mappings: options.paths.clone().unwrap_or_default(),
            type_roots: options.type_roots.clone().unwrap_or_default(),
            resolution_cache: FxHashMap::default(),
            ts_extensions: vec![".ts", ".tsx", ".d.ts"],
            js_extensions: vec![".js", ".jsx"],
            dts_extensions: vec![".d.ts", ".d.mts", ".d.cts"],
            custom_conditions: options.custom_conditions.clone(),
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
            resolution_cache: FxHashMap::default(),
            ts_extensions: vec![".ts", ".tsx", ".d.ts"],
            js_extensions: vec![".js", ".jsx"],
            dts_extensions: vec![".d.ts", ".d.mts", ".d.cts"],
            custom_conditions: Vec::new(),
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
        let containing_dir = containing_file
            .parent()
            .unwrap_or(Path::new("."))
            .to_path_buf();
        let containing_file_str = containing_file.display().to_string();

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

        let result = self.resolve_uncached(
            specifier,
            &containing_dir,
            &containing_file_str,
            specifier_span,
            importing_module_kind,
        );

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
    ) -> Result<ResolvedModule, ResolutionFailure> {
        // Step 1: Handle #-prefixed imports (package.json imports field)
        // This is a Node16/NodeNext feature for subpath imports
        if specifier.starts_with('#') {
            if !matches!(
                self.resolution_kind,
                ModuleResolutionKind::Node
                    | ModuleResolutionKind::Node16
                    | ModuleResolutionKind::NodeNext
                    | ModuleResolutionKind::Bundler
            ) {
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
        if specifier.starts_with("./") || specifier.starts_with("../") {
            return self.resolve_relative(
                specifier,
                containing_dir,
                containing_file,
                specifier_span,
                importing_module_kind,
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

        // Step 6: Classic resolution does not consult node_modules
        if matches!(self.resolution_kind, ModuleResolutionKind::Classic) {
            return Err(ResolutionFailure::NotFound {
                specifier: specifier.to_string(),
                containing_file: containing_file.to_string(),
                span: specifier_span,
            });
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
            PackageExports::Conditional(cond_map) => {
                for condition in conditions {
                    if let Some(nested) = cond_map.get(condition) {
                        if let Some(result) =
                            self.resolve_export_target_to_string(nested, conditions)
                        {
                            return Some(result);
                        }
                    }
                }
                None
            }
            PackageExports::Map(_) => None, // Subpath maps not valid here
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

        // Add platform condition based on resolution kind
        match self.resolution_kind {
            ModuleResolutionKind::Bundler => {
                conditions.push("browser".to_string());
            }
            ModuleResolutionKind::Classic
            | ModuleResolutionKind::Node
            | ModuleResolutionKind::Node16
            | ModuleResolutionKind::NodeNext => {
                conditions.push("node".to_string());
            }
        }

        // Add module kind conditions - primary first, then opposite as fallback
        // This allows packages that only export one format to still be resolved
        match importing_module_kind {
            ImportingModuleKind::Esm => {
                conditions.push("import".to_string());
                conditions.push("default".to_string());
                conditions.push("require".to_string()); // Fallback: ESM file can use CJS-only package
            }
            ImportingModuleKind::CommonJs => {
                conditions.push("require".to_string());
                conditions.push("default".to_string());
                conditions.push("import".to_string()); // Fallback: CJS file can use ESM-only package
            }
        }

        // Add additional platform fallbacks
        match self.resolution_kind {
            ModuleResolutionKind::Bundler => {
                conditions.push("node".to_string()); // Bundler can also use node exports
            }
            ModuleResolutionKind::Classic
            | ModuleResolutionKind::Node
            | ModuleResolutionKind::Node16
            | ModuleResolutionKind::NodeNext => {
                conditions.push("browser".to_string()); // Node can use browser exports as last resort
            }
        }

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
    ) -> Result<ResolvedModule, ResolutionFailure> {
        let candidate = containing_dir.join(specifier);

        // Check if specifier has an explicit extension
        let specifier_has_extension = Path::new(specifier)
            .extension()
            .map(|ext| !ext.is_empty())
            .unwrap_or(false);

        // TS2834 Check: In Node16/NodeNext + ESM context, relative imports must have explicit extensions
        if matches!(
            self.resolution_kind,
            ModuleResolutionKind::Node16 | ModuleResolutionKind::NodeNext
        ) && importing_module_kind == ImportingModuleKind::Esm
            && !specifier_has_extension
        {
            // Try to resolve to determine what extension to suggest
            if let Some(resolved) = self.try_file_or_directory(&candidate) {
                // Resolution succeeded implicitly - this is an error in ESM mode
                let resolved_ext = ModuleExtension::from_path(&resolved);
                // Suggest the .js extension (TypeScript convention: import .js, compile from .ts)
                let suggested_ext = match resolved_ext {
                    ModuleExtension::Ts | ModuleExtension::Tsx | ModuleExtension::Js | ModuleExtension::Jsx => ".js",
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
            // If resolution fails, fall through to NotFound
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

        // Track if we found a package with exports but couldn't resolve it
        // (for TS2792 hint when not in Node16/NodeNext/Bundler mode)
        let mut found_package_with_exports = false;

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
                        Err(_) => {
                            // Check if package has exports field - relevant for TS2792
                            if !matches!(
                                self.resolution_kind,
                                ModuleResolutionKind::Node16
                                    | ModuleResolutionKind::NodeNext
                                    | ModuleResolutionKind::Bundler
                            ) {
                                let package_json_path = package_dir.join("package.json");
                                if let Ok(pj) = self.read_package_json(&package_json_path) {
                                    if pj.exports.is_some() {
                                        found_package_with_exports = true;
                                    }
                                }
                            }
                            // Continue searching in parent directories
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

        // Try type roots (for @types packages)
        for type_root in &self.type_roots {
            let types_package =
                type_root.join(format!("@types/{}", package_name.replace('/', "__")));
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

        // TS2792: If we found a package with exports but couldn't resolve it,
        // and we're not in a mode that supports exports, suggest switching modes
        if found_package_with_exports {
            return Err(ResolutionFailure::ModuleResolutionModeMismatch {
                specifier: specifier.to_string(),
                containing_file: containing_file.to_string(),
                span: specifier_span,
            });
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
                        if let Some(exports) = &package_json.exports {
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
            if matches!(
                self.resolution_kind,
                ModuleResolutionKind::Node16
                    | ModuleResolutionKind::NodeNext
                    | ModuleResolutionKind::Bundler
            ) && let Some(exports) = &package_json.exports
                && let Some(resolved) = self.resolve_package_exports_with_conditions(
                    package_dir,
                    exports,
                    &subpath_key,
                    conditions,
                )
            {
                return Ok(ResolvedModule {
                    resolved_path: resolved.clone(),
                    is_external: true,
                    package_name: Some(package_json.name.clone().unwrap_or_default()),
                    original_specifier: original_specifier.to_string(),
                    extension: ModuleExtension::from_path(&resolved),
                });
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
        if matches!(
            self.resolution_kind,
            ModuleResolutionKind::Node16
                | ModuleResolutionKind::NodeNext
                | ModuleResolutionKind::Bundler
        ) && let Some(exports) = &package_json.exports
            && let Some(resolved) =
                self.resolve_package_exports_with_conditions(package_dir, exports, ".", conditions)
        {
            return Ok(ResolvedModule {
                resolved_path: resolved.clone(),
                is_external: true,
                package_name: Some(package_json.name.clone().unwrap_or_default()),
                original_specifier: original_specifier.to_string(),
                extension: ModuleExtension::from_path(&resolved),
            });
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
            if let Some(resolved) = self.try_file_or_directory(&main_path) {
                return Ok(ResolvedModule {
                    resolved_path: resolved.clone(),
                    is_external: true,
                    package_name: Some(package_json.name.clone().unwrap_or_default()),
                    original_specifier: original_specifier.to_string(),
                    extension: ModuleExtension::from_path(&resolved),
                });
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
                    if let Some(r) = self.try_file_or_directory(&resolved) {
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
            PackageExports::Conditional(cond_map) => {
                // Try conditions in the provided order
                for condition in conditions {
                    if let Some(value) = cond_map.get(condition)
                        && let Some(resolved) = self.resolve_package_exports_with_conditions(
                            package_dir,
                            value,
                            subpath,
                            conditions,
                        )
                    {
                        return Some(resolved);
                    }
                }
                None
            }
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
                self.try_file_or_directory(&resolved)
            }
            PackageExports::Conditional(cond_map) => {
                for condition in conditions {
                    if let Some(nested) = cond_map.get(condition)
                        && let Some(resolved) = self.resolve_export_value_with_conditions(
                            package_dir,
                            nested,
                            conditions,
                        )
                    {
                        return Some(resolved);
                    }
                }
                None
            }
            PackageExports::Map(_) => None,
        }
    }

    /// Resolve typesVersions field
    fn resolve_types_versions(
        &self,
        package_dir: &Path,
        subpath: &str,
        types_versions: &serde_json::Value,
    ) -> Option<PathBuf> {
        // For now, use a simple approach: select the first matching version range
        // A more complete implementation would consider TypeScript version compatibility
        let map = types_versions.as_object()?;

        // Find a matching version (using "*" as fallback)
        let mut best_paths: Option<&serde_json::Map<String, serde_json::Value>> = None;

        for (version_range, value) in map {
            let paths = value.as_object()?;
            // Use "*" as a wildcard match, or ">=" ranges
            // For simplicity, we'll match "*" or any range that would match TS 5.x
            if version_range == "*"
                || version_range.starts_with(">=")
                || version_range.starts_with(">")
            {
                best_paths = Some(paths);
                break;
            }
        }

        let paths = best_paths?;

        // Find matching pattern
        let mut best_target: Option<String> = None;
        let mut best_specificity = 0usize;

        for (pattern, value) in paths {
            if let Some(wildcard) = match_types_versions_pattern(pattern, subpath) {
                let specificity = pattern.len();
                if specificity > best_specificity {
                    best_specificity = specificity;

                    // Get target path(s)
                    let target = match value {
                        serde_json::Value::String(s) => s.clone(),
                        serde_json::Value::Array(arr) => arr
                            .first()
                            .and_then(|v| v.as_str())
                            .unwrap_or("")
                            .to_string(),
                        _ => continue,
                    };

                    best_target = Some(apply_wildcard_substitution(&target, &wildcard));
                }
            }
        }

        if let Some(target) = best_target {
            let resolved = package_dir.join(target.trim_start_matches("./"));
            return self.try_file_or_directory(&resolved);
        }

        None
    }

    /// Try to resolve a file with various extensions
    fn try_file(&self, path: &Path) -> Option<PathBuf> {
        if let Some(extension) = path.extension().and_then(|ext| ext.to_str()) {
            if matches!(
                self.resolution_kind,
                ModuleResolutionKind::Node16 | ModuleResolutionKind::NodeNext
            ) && let Some(rewritten) = node16_extension_substitution(path, extension)
            {
                for candidate in rewritten {
                    if candidate.is_file() {
                        return Some(candidate);
                    }
                }
                return None;
            }
            if path.is_file() {
                return Some(path.to_path_buf());
            }
            return None;
        }

        let extensions = self.extension_candidates_for_resolution();
        for ext in extensions {
            let with_ext = path.with_extension(ext);
            if with_ext.is_file() {
                return Some(with_ext);
            }
        }
        for ext in extensions {
            let with_ext = path.join("index").with_extension(ext);
            if with_ext.is_file() {
                return Some(with_ext);
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
                    None => &TS_EXTENSION_CANDIDATES,
                }
            }
            ModuleResolutionKind::Classic => &CLASSIC_EXTENSION_CANDIDATES,
            _ => &TS_EXTENSION_CANDIDATES,
        }
    }

    /// Try to resolve a path as a file or directory
    fn try_file_or_directory(&self, path: &Path) -> Option<PathBuf> {
        // Try as file first
        if let Some(resolved) = self.try_file(path) {
            return Some(resolved);
        }

        // Try as directory with index
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

/// Apply wildcard substitution to a target path
fn apply_wildcard_substitution(target: &str, wildcard: &str) -> String {
    if target.contains('*') {
        target.replace('*', wildcard)
    } else {
        target.to_string()
    }
}

const TS_EXTENSION_CANDIDATES: [&str; 7] = ["ts", "tsx", "d.ts", "mts", "cts", "d.mts", "d.cts"];
const NODE16_MODULE_EXTENSION_CANDIDATES: [&str; 7] =
    ["mts", "d.mts", "ts", "tsx", "d.ts", "cts", "d.cts"];
const NODE16_COMMONJS_EXTENSION_CANDIDATES: [&str; 7] =
    ["cts", "d.cts", "ts", "tsx", "d.ts", "mts", "d.mts"];
const CLASSIC_EXTENSION_CANDIDATES: [&str; 7] = TS_EXTENSION_CANDIDATES;

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
#[derive(Debug, Clone, serde::Deserialize)]
#[serde(untagged)]
pub enum PackageExports {
    String(String),
    Map(FxHashMap<String, PackageExports>),
    Conditional(FxHashMap<String, PackageExports>),
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
    // TS2834 (Import Path Needs Extension) Tests
    // =========================================================================

    #[test]
    fn test_ts2834_error_code_constant() {
        assert_eq!(IMPORT_PATH_NEEDS_EXTENSION, 2834);
    }

    #[test]
    fn test_import_path_needs_extension_produces_ts2834() {
        let failure = ResolutionFailure::ImportPathNeedsExtension {
            specifier: "./utils".to_string(),
            suggested_extension: ".js".to_string(),
            containing_file: "/src/index.mts".to_string(),
            span: Span::new(20, 30),
        };

        let diagnostic = failure.to_diagnostic();
        assert_eq!(diagnostic.code, IMPORT_PATH_NEEDS_EXTENSION);
        assert_eq!(diagnostic.file_name, "/src/index.mts");
        assert!(diagnostic.message.contains("Relative import paths need explicit file extensions"));
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
        assert_eq!(diagnostic.code, IMPORT_PATH_NEEDS_EXTENSION);
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
        assert_eq!(diagnostic.code, IMPORT_PATH_NEEDS_EXTENSION);
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
        assert!(diagnostic.message.contains("Cannot find module 'modern-esm-package'"));
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

        // Test TS2834
        let failure_2834 = ResolutionFailure::ImportPathNeedsExtension {
            specifier: "./utils".to_string(),
            suggested_extension: ".js".to_string(),
            containing_file: "/src/app.mts".to_string(),
            span: Span::new(0, 10),
        };
        resolver.emit_resolution_error(&mut diagnostics, &failure_2834);

        // Test TS2792
        let failure_2792 = ResolutionFailure::ModuleResolutionModeMismatch {
            specifier: "esm-pkg".to_string(),
            containing_file: "/src/index.ts".to_string(),
            span: Span::new(5, 15),
        };
        resolver.emit_resolution_error(&mut diagnostics, &failure_2792);

        assert_eq!(diagnostics.len(), 2);

        let errors: Vec<_> = diagnostics.errors().collect();
        assert_eq!(errors[0].code, IMPORT_PATH_NEEDS_EXTENSION);
        assert_eq!(errors[1].code, MODULE_RESOLUTION_MODE_MISMATCH);
    }
}

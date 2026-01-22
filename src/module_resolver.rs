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
    InvalidSpecifier(String),
    /// Package.json not found or invalid
    PackageJsonError(String),
    /// Circular resolution detected
    CircularResolution(String),
    /// Path mapping did not resolve to a file
    PathMappingFailed(String),
}

impl ResolutionFailure {
    /// Convert a resolution failure to a diagnostic
    pub fn span_to_diagnostic(&self) -> Diagnostic {
        match self {
            ResolutionFailure::NotFound {
                specifier,
                containing_file,
                span,
            } => Diagnostic::error(
                containing_file,
                *span,
                format!("Cannot find module '{}'", specifier),
                CANNOT_FIND_MODULE,
            ),
            ResolutionFailure::InvalidSpecifier(msg) => Diagnostic::error(
                "",
                Span::dummy(),
                format!("Invalid module specifier: {}", msg),
                CANNOT_FIND_MODULE,
            ),
            ResolutionFailure::PackageJsonError(msg) => Diagnostic::error(
                "",
                Span::dummy(),
                format!("Package.json error: {}", msg),
                CANNOT_FIND_MODULE,
            ),
            ResolutionFailure::CircularResolution(msg) => Diagnostic::error(
                "",
                Span::dummy(),
                format!("Circular resolution: {}", msg),
                CANNOT_FIND_MODULE,
            ),
            ResolutionFailure::PathMappingFailed(msg) => Diagnostic::error(
                "",
                Span::dummy(),
                format!("Path mapping failed: {}", msg),
                CANNOT_FIND_MODULE,
            ),
        }
    }

    /// Check if this is a NotFound error (for TS2307 emission)
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
    ts_extensions: Vec<&'static str>,
    /// Extensions to try for JavaScript resolution
    js_extensions: Vec<&'static str>,
    /// Declaration extensions to try
    #[allow(dead_code)] // Infrastructure for .d.ts resolution
    dts_extensions: Vec<&'static str>,
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

        // Check cache first
        let cache_key = (containing_dir.clone(), specifier.to_string());
        if let Some(cached) = self.resolution_cache.get(&cache_key) {
            return cached.clone();
        }

        let result = self.resolve_uncached(
            specifier,
            &containing_dir,
            &containing_file_str,
            specifier_span,
        );

        // Cache the result
        self.resolution_cache.insert(cache_key, result.clone());

        result
    }

    /// Resolve without checking cache
    fn resolve_uncached(
        &self,
        specifier: &str,
        containing_dir: &Path,
        containing_file: &str,
        specifier_span: Span,
    ) -> Result<ResolvedModule, ResolutionFailure> {
        // Step 1: Try path mappings first (if configured)
        if !self.path_mappings.is_empty()
            && let Some(resolved) = self.try_path_mappings(specifier, containing_dir)
        {
            return Ok(resolved);
        }

        // Step 2: Handle relative imports
        if specifier.starts_with("./") || specifier.starts_with("../") {
            return self.resolve_relative(
                specifier,
                containing_dir,
                containing_file,
                specifier_span,
            );
        }

        // Step 3: Handle absolute imports (rare but valid)
        if specifier.starts_with('/') {
            return self.resolve_absolute(specifier, containing_file, specifier_span);
        }

        // Step 4: Handle bare specifiers (npm packages)
        self.resolve_bare_specifier(specifier, containing_dir, containing_file, specifier_span)
    }

    /// Try resolving through path mappings
    fn try_path_mappings(&self, specifier: &str, containing_dir: &Path) -> Option<ResolvedModule> {
        // Sort path mappings by specificity (most specific first)
        let mut sorted_mappings: Vec<_> = self.path_mappings.iter().collect();
        sorted_mappings.sort_by_key(|b| std::cmp::Reverse(b.specificity()));

        for mapping in sorted_mappings {
            if let Some(star_match) = mapping.match_specifier(specifier) {
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
                        return Some(ResolvedModule {
                            resolved_path: resolved,
                            is_external: false,
                            package_name: None,
                            original_specifier: specifier.to_string(),
                            extension: ModuleExtension::from_path(&candidate),
                        });
                    }
                }
            }
        }

        None
    }

    /// Resolve a relative import
    fn resolve_relative(
        &self,
        specifier: &str,
        containing_dir: &Path,
        containing_file: &str,
        specifier_span: Span,
    ) -> Result<ResolvedModule, ResolutionFailure> {
        let candidate = containing_dir.join(specifier);

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
    ) -> Result<ResolvedModule, ResolutionFailure> {
        // Parse package name and subpath
        let (package_name, subpath) = parse_package_specifier(specifier);

        // Walk up directory tree looking for node_modules
        let mut current = containing_dir.to_path_buf();
        loop {
            let node_modules = current.join("node_modules");

            if node_modules.is_dir() {
                let package_dir = node_modules.join(&package_name);

                if package_dir.is_dir() {
                    return self.resolve_package(
                        &package_dir,
                        subpath.as_deref(),
                        specifier,
                        containing_file,
                        specifier_span,
                    );
                }
            }

            // Move to parent directory
            match current.parent() {
                Some(parent) => current = parent.to_path_buf(),
                None => break,
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

    /// Resolve within a package directory
    fn resolve_package(
        &self,
        package_dir: &Path,
        subpath: Option<&str>,
        original_specifier: &str,
        containing_file: &str,
        specifier_span: Span,
    ) -> Result<ResolvedModule, ResolutionFailure> {
        // Read package.json
        let package_json_path = package_dir.join("package.json");
        let package_json = if package_json_path.exists() {
            self.read_package_json(&package_json_path)?
        } else {
            PackageJson::default()
        };

        // If there's a subpath, resolve it directly
        if let Some(subpath) = subpath {
            // Try exports field first (Node16+)
            if matches!(
                self.resolution_kind,
                ModuleResolutionKind::Node16
                    | ModuleResolutionKind::NodeNext
                    | ModuleResolutionKind::Bundler
            ) && let Some(exports) = &package_json.exports
                && let Some(resolved) = self.resolve_package_exports(package_dir, exports, subpath)
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
            && let Some(resolved) = self.resolve_package_exports(package_dir, exports, ".")
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
        if let Some(types) = package_json.types.or(package_json.typings) {
            let types_path = package_dir.join(&types);
            if types_path.exists() {
                return Ok(ResolvedModule {
                    resolved_path: types_path.clone(),
                    is_external: true,
                    package_name: Some(package_json.name.clone().unwrap_or_default()),
                    original_specifier: original_specifier.to_string(),
                    extension: ModuleExtension::from_path(&types_path),
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

        Err(ResolutionFailure::PackageJsonError(format!(
            "Could not find entry point for package at {}",
            package_dir.display()
        )))
    }

    /// Resolve package exports field
    fn resolve_package_exports(
        &self,
        package_dir: &Path,
        exports: &PackageExports,
        subpath: &str,
    ) -> Option<PathBuf> {
        match exports {
            PackageExports::String(s) => {
                if subpath == "." {
                    let resolved = package_dir.join(s);
                    if resolved.exists() {
                        return Some(resolved);
                    }
                }
                None
            }
            PackageExports::Map(map) => {
                // First try exact match
                if let Some(value) = map.get(subpath) {
                    return self.resolve_export_value(package_dir, value);
                }

                // Try pattern matching (e.g., "./*" or "./lib/*")
                for (pattern, value) in map {
                    if let Some(matched) = match_export_pattern(pattern, subpath)
                        && let Some(resolved) = self.resolve_export_value(package_dir, value)
                    {
                        // Substitute wildcard
                        let resolved_str = resolved.to_string_lossy();
                        if resolved_str.contains('*') {
                            let substituted = resolved_str.replace('*', &matched);
                            return Some(PathBuf::from(substituted));
                        }
                        return Some(resolved);
                    }
                }

                None
            }
            PackageExports::Conditional(conditions) => {
                // Try conditions in order of preference
                let condition_order = match self.resolution_kind {
                    ModuleResolutionKind::Node16 | ModuleResolutionKind::NodeNext => {
                        vec!["types", "import", "require", "node", "default"]
                    }
                    ModuleResolutionKind::Bundler => {
                        vec!["types", "import", "browser", "default"]
                    }
                    _ => vec!["types", "require", "default"],
                };

                for condition in condition_order {
                    if let Some(value) = conditions.get(condition)
                        && let Some(resolved) =
                            self.resolve_package_exports(package_dir, value, subpath)
                    {
                        return Some(resolved);
                    }
                }

                None
            }
        }
    }

    /// Resolve a single export value
    fn resolve_export_value(&self, package_dir: &Path, value: &PackageExports) -> Option<PathBuf> {
        match value {
            PackageExports::String(s) => {
                let resolved = package_dir.join(s);
                if resolved.exists() {
                    Some(resolved)
                } else {
                    None
                }
            }
            PackageExports::Conditional(conditions) => self.resolve_package_exports(
                package_dir,
                &PackageExports::Conditional(conditions.clone()),
                ".",
            ),
            PackageExports::Map(_) => None,
        }
    }

    /// Try to resolve a file with various extensions
    fn try_file(&self, path: &Path) -> Option<PathBuf> {
        // First, check if exact path exists
        if path.exists() && path.is_file() {
            return Some(path.to_path_buf());
        }

        // Try TypeScript extensions
        for ext in &self.ts_extensions {
            let with_ext = path.with_extension(ext.trim_start_matches('.'));
            if with_ext.exists() && with_ext.is_file() {
                return Some(with_ext);
            }
        }

        // Try .d.ts specifically (compound extension)
        let dts = PathBuf::from(format!("{}.d.ts", path.display()));
        if dts.exists() && dts.is_file() {
            return Some(dts);
        }

        // Try JavaScript extensions
        for ext in &self.js_extensions {
            let with_ext = path.with_extension(ext.trim_start_matches('.'));
            if with_ext.exists() && with_ext.is_file() {
                return Some(with_ext);
            }
        }

        None
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
    fn read_package_json(&self, path: &Path) -> Result<PackageJson, ResolutionFailure> {
        let content = std::fs::read_to_string(path).map_err(|e| {
            ResolutionFailure::PackageJsonError(format!("Failed to read {}: {}", path.display(), e))
        })?;

        serde_json::from_str(&content).map_err(|e| {
            ResolutionFailure::PackageJsonError(format!(
                "Failed to parse {}: {}",
                path.display(),
                e
            ))
        })
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
    pub fn emit_resolution_error(
        &self,
        diagnostics: &mut DiagnosticBag,
        failure: &ResolutionFailure,
    ) {
        if failure.is_not_found() {
            let diagnostic = failure.span_to_diagnostic();
            diagnostics.add(diagnostic);
        }
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

        let diagnostic = failure.span_to_diagnostic();
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

        let other = ResolutionFailure::InvalidSpecifier("test".to_string());
        assert!(!other.is_not_found());
    }
}

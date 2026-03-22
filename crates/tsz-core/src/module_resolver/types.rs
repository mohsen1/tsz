//! Boundary types for module resolution.
//!
//! This module contains the public-facing types used by the module resolver:
//! diagnostic code constants, request/result/error types, resolution failure
//! variants, and supporting enums (extensions, import kinds, package types).

use crate::diagnostics::Diagnostic;
use crate::span::Span;
use std::path::{Path, PathBuf};

/// TS2307: Cannot find module
///
/// This error code is emitted when a module specifier cannot be resolved.
/// Example: `import { foo } from './missing-module'`
///
/// Usage example:
/// ```text
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

/// TS2834: Relative import paths need explicit file extensions in `ECMAScript` imports
///
/// This error code is emitted when a relative import in an ESM context under Node16/NodeNext
/// resolution mode does not include an explicit file extension. ESM requires explicit extensions.
/// Example: `import { foo } from './utils'` should be `import { foo } from './utils.js'`
pub const IMPORT_PATH_NEEDS_EXTENSION: u32 = 2834;
pub const IMPORT_PATH_NEEDS_EXTENSION_SUGGESTION: u32 = 2835;
pub const IMPORT_PATH_TS_EXTENSION_NOT_ALLOWED: u32 = 5097;
pub const MODULE_WAS_RESOLVED_TO_BUT_JSX_NOT_SET: u32 = 6142;

/// TS7016: Could not find a declaration file for module.
/// Emitted when resolution fails but a JS file exists for the specifier
/// and `noImplicitAny` is enabled.
pub const COULD_NOT_FIND_DECLARATION_FILE: u32 = 7016;

// ---------------------------------------------------------------------------
// ModuleLookupRequest / ModuleLookupResult — explicit driver-facing boundary
// ---------------------------------------------------------------------------

/// Complete request for module lookup from the driver.
///
/// Captures the full intent of a module resolution request so that
/// diagnostic code selection (TS2307/TS2732/TS2792/TS2834/TS2835/TS5097/TS7016)
/// lives in the resolver, not in scattered driver branches.
#[derive(Debug, Clone)]
pub struct ModuleLookupRequest<'a> {
    /// Module specifier string (e.g., `"./foo"`, `"lodash"`, `"#utils"`)
    pub specifier: &'a str,
    /// File containing the import statement
    pub containing_file: &'a Path,
    /// Span of the module specifier in source
    pub specifier_span: Span,
    /// Import syntax kind (ESM import, dynamic import, CJS require, re-export)
    pub import_kind: ImportKind,
    /// Whether `--noImplicitAny` is enabled (affects TS7016 emission)
    pub no_implicit_any: bool,
    /// Whether classic resolution is implied (for TS2792 vs TS2307)
    pub implied_classic_resolution: bool,
}

/// Structured outcome of a module lookup.
///
/// Captures everything the driver needs to:
/// - Map resolved paths to file indices
/// - Record resolution errors for the checker
/// - Track which specifiers are "resolved" (even without a target file)
#[derive(Debug, Clone)]
pub struct ModuleLookupResult {
    /// Resolved file path, if resolution succeeded.
    pub resolved_path: Option<PathBuf>,
    /// Whether to treat this specifier as "resolved" even without a mapped path.
    /// True for: ambient modules, untyped JS modules, `JsxNotEnabled` with valid file.
    pub treat_as_resolved: bool,
    /// Error to record for the checker, if any.
    pub error: Option<ModuleLookupError>,
}

/// Structured error from module lookup.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModuleLookupError {
    /// Diagnostic code (e.g., 2307, 2732, 2792, 2834, 2835, 5097, 7016)
    pub code: u32,
    /// Diagnostic message
    pub message: String,
}

impl ModuleLookupResult {
    /// Resolved successfully to a file.
    pub const fn resolved(path: PathBuf) -> Self {
        Self {
            resolved_path: Some(path),
            treat_as_resolved: false,
            error: None,
        }
    }

    /// Resolution failed with a specific error.
    pub const fn failed(code: u32, message: String) -> Self {
        Self {
            resolved_path: None,
            treat_as_resolved: false,
            error: Some(ModuleLookupError { code, message }),
        }
    }

    /// Module is an ambient declaration — suppress TS2307 without a file target.
    pub const fn ambient() -> Self {
        Self {
            resolved_path: None,
            treat_as_resolved: true,
            error: None,
        }
    }

    /// Resolved to a file but with an associated error (e.g., `JsxNotEnabled`).
    pub const fn resolved_with_error(code: u32, message: String) -> Self {
        Self {
            resolved_path: None,
            treat_as_resolved: true,
            error: Some(ModuleLookupError { code, message }),
        }
    }

    /// Untyped JS module found. Marks as resolved; error only if `noImplicitAny`.
    pub fn untyped_js(js_path: PathBuf, no_implicit_any: bool, specifier: &str) -> Self {
        Self {
            resolved_path: None,
            treat_as_resolved: true,
            error: if no_implicit_any {
                Some(ModuleLookupError {
                    code: COULD_NOT_FIND_DECLARATION_FILE,
                    message: format!(
                        "Could not find a declaration file for module '{}'. '{}' implicitly has an 'any' type.",
                        specifier,
                        js_path.display()
                    ),
                })
            } else {
                None
            },
        }
    }

    /// Classify this lookup result into a driver-facing outcome.
    ///
    /// Centralizes the post-processing that every driver (CLI, LSP, WASM) must
    /// perform after calling `ModuleResolver::lookup`:
    /// - Map resolved paths to file indices (or leave as path for path-based drivers)
    /// - Determine whether the specifier should be treated as "known" (suppress TS2307)
    /// - Extract any error for the checker
    ///
    /// This replaces scattered driver-side `if let Some(path) = result.resolved_path`
    /// / `if result.treat_as_resolved` / `if let Some(error) = result.error` logic.
    pub fn classify(self) -> ModuleLookupOutcome {
        let is_resolved = self.resolved_path.is_some() || self.treat_as_resolved;
        ModuleLookupOutcome {
            resolved_path: self.resolved_path,
            is_resolved,
            error: self.error,
        }
    }
}

/// Driver-facing outcome of a module lookup, produced by
/// [`ModuleLookupResult::classify`].
///
/// This is the canonical post-processing of a [`ModuleLookupResult`] that
/// every driver consumer needs. It answers three questions:
///
/// 1. **What file was resolved?** (`resolved_path`)
/// 2. **Should the specifier be treated as "known"?** (`is_resolved`)
///    True when the file resolved, or when the module is ambient/untyped-JS.
/// 3. **Is there an error to report?** (`error`)
///    Present for TS2307/TS2732/TS2792/TS2834/TS2835/TS5097/TS7016/TS6142.
///
/// # Example
///
/// ```ignore
/// let result = resolver.lookup(&request, fallback, ambient_check);
/// let outcome = result.classify();
///
/// if let Some(path) = &outcome.resolved_path {
///     file_map.insert(specifier, path.clone());
/// }
/// if outcome.is_resolved {
///     known_specifiers.insert(specifier);
/// }
/// if let Some(error) = &outcome.error {
///     errors.insert(specifier, error.clone());
/// }
/// ```
#[derive(Debug, Clone)]
pub struct ModuleLookupOutcome {
    /// Resolved file path, if resolution succeeded to a concrete file.
    pub resolved_path: Option<PathBuf>,
    /// Whether this specifier should be treated as "known" by the checker.
    /// True when resolved to a file, or when the module is ambient/untyped-JS.
    pub is_resolved: bool,
    /// Error to report to the checker, if any.
    pub error: Option<ModuleLookupError>,
}

/// Result of module resolution
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedModule {
    /// Resolved file path
    pub resolved_path: PathBuf,
    /// Whether the module is an external package (from `node_modules`)
    pub is_external: bool,
    /// Package name if resolved from `node_modules`
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
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Hash)]
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
            return Self::Dts;
        }
        if path_str.ends_with(".d.mts") {
            return Self::DmTs;
        }
        if path_str.ends_with(".d.cts") {
            return Self::DCts;
        }

        match path.extension().and_then(|e| e.to_str()) {
            Some("ts") => Self::Ts,
            Some("tsx") => Self::Tsx,
            Some("js") => Self::Js,
            Some("jsx") => Self::Jsx,
            Some("mjs") => Self::Mjs,
            Some("cjs") => Self::Cjs,
            Some("mts") => Self::Mts,
            Some("cts") => Self::Cts,
            Some("json") => Self::Json,
            _ => Self::Unknown,
        }
    }

    /// Get the extension string
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Ts => ".ts",
            Self::Tsx => ".tsx",
            Self::Dts => ".d.ts",
            Self::DmTs => ".d.mts",
            Self::DCts => ".d.cts",
            Self::Js => ".js",
            Self::Jsx => ".jsx",
            Self::Mjs => ".mjs",
            Self::Cjs => ".cjs",
            Self::Mts => ".mts",
            Self::Cts => ".cts",
            Self::Json => ".json",
            Self::Unknown => "",
        }
    }

    /// Check if this extension forces ESM mode
    /// .mts, .mjs, .d.mts files are always ESM
    pub const fn forces_esm(&self) -> bool {
        matches!(self, Self::Mts | Self::Mjs | Self::DmTs)
    }

    /// Check if this extension forces CommonJS mode
    /// .cts, .cjs, .d.cts files are always CommonJS
    pub const fn forces_cjs(&self) -> bool {
        matches!(self, Self::Cts | Self::Cjs | Self::DCts)
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
    /// TS2834: Relative import paths need explicit file extensions in `ECMAScript` imports
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
            Self::NotFound {
                specifier,
                containing_file,
                span,
            } => Diagnostic::error(
                containing_file,
                *span,
                format!("Cannot find module '{specifier}' or its corresponding type declarations.",),
                CANNOT_FIND_MODULE,
            ),
            Self::InvalidSpecifier {
                message,
                containing_file,
                span,
            }
            | Self::PackageJsonError {
                message,
                containing_file,
                span,
            }
            | Self::CircularResolution {
                message,
                containing_file,
                span,
            }
            | Self::PathMappingFailed {
                message,
                containing_file,
                span,
            } => Diagnostic::error(
                containing_file,
                *span,
                format!("Cannot find module '{message}' or its corresponding type declarations.",),
                CANNOT_FIND_MODULE,
            ),
            Self::ImportPathNeedsExtension {
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
                        "Relative import paths need explicit file extensions in ECMAScript imports when '--moduleResolution' is 'node16' or 'nodenext'. Consider adding an extension to the import path.".to_string(),
                        IMPORT_PATH_NEEDS_EXTENSION,
                    )
                } else {
                    // TS2835: With extension suggestion
                    Diagnostic::error(
                        containing_file,
                        *span,
                        format!(
                            "Relative import paths need explicit file extensions in ECMAScript imports when '--moduleResolution' is 'node16' or 'nodenext'. Did you mean '{specifier}{suggested_extension}'?",
                        ),
                        IMPORT_PATH_NEEDS_EXTENSION_SUGGESTION,
                    )
                }
            }
            Self::ImportingTsExtensionNotAllowed {
                extension,
                containing_file,
                span,
            } => Diagnostic::error(
                containing_file,
                *span,
                format!(
                    "An import path can only end with a '{extension}' extension when 'allowImportingTsExtensions' is enabled.",
                ),
                IMPORT_PATH_TS_EXTENSION_NOT_ALLOWED,
            ),
            Self::JsxNotEnabled {
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
            Self::ModuleResolutionModeMismatch {
                specifier,
                containing_file,
                span,
            } => Diagnostic::error(
                containing_file,
                *span,
                format!(
                    "Cannot find module '{specifier}'. Did you mean to set the 'moduleResolution' option to 'nodenext', or to add aliases to the 'paths' option?",
                ),
                MODULE_RESOLUTION_MODE_MISMATCH,
            ),
            Self::JsonModuleWithoutResolveJsonModule {
                specifier,
                containing_file,
                span,
            } => Diagnostic::error(
                containing_file,
                *span,
                format!(
                    "Cannot find module '{specifier}'. Consider using '--resolveJsonModule' to import module with '.json' extension.",
                ),
                JSON_MODULE_WITHOUT_RESOLVE_JSON_MODULE,
            ),
        }
    }

    /// Get the containing file for this resolution failure
    pub fn containing_file(&self) -> &str {
        match self {
            Self::NotFound {
                containing_file, ..
            }
            | Self::InvalidSpecifier {
                containing_file, ..
            }
            | Self::PackageJsonError {
                containing_file, ..
            }
            | Self::CircularResolution {
                containing_file, ..
            }
            | Self::PathMappingFailed {
                containing_file, ..
            }
            | Self::ImportPathNeedsExtension {
                containing_file, ..
            }
            | Self::ImportingTsExtensionNotAllowed {
                containing_file, ..
            }
            | Self::JsxNotEnabled {
                containing_file, ..
            }
            | Self::ModuleResolutionModeMismatch {
                containing_file, ..
            }
            | Self::JsonModuleWithoutResolveJsonModule {
                containing_file, ..
            } => containing_file,
        }
    }

    /// Get the span for this resolution failure
    pub const fn span(&self) -> Span {
        match self {
            Self::NotFound { span, .. }
            | Self::InvalidSpecifier { span, .. }
            | Self::PackageJsonError { span, .. }
            | Self::CircularResolution { span, .. }
            | Self::PathMappingFailed { span, .. }
            | Self::ImportPathNeedsExtension { span, .. }
            | Self::ImportingTsExtensionNotAllowed { span, .. }
            | Self::JsxNotEnabled { span, .. }
            | Self::ModuleResolutionModeMismatch { span, .. }
            | Self::JsonModuleWithoutResolveJsonModule { span, .. } => *span,
        }
    }

    /// Check if this is a `NotFound` error
    pub const fn is_not_found(&self) -> bool {
        matches!(self, Self::NotFound { .. })
    }

    /// Check if this failure type should trigger a fallback resolution attempt.
    ///
    /// tsc tries all resolution strategies before giving up. Our `ModuleResolver`
    /// may fail with `ModuleResolutionModeMismatch` or `PackageJsonError` even
    /// though the legacy/classic fallback resolver would succeed (e.g. for virtual
    /// test files or files that don't need package.json exports). We should try
    /// the fallback for these "soft" failures before emitting an error.
    pub const fn should_try_fallback(&self) -> bool {
        matches!(
            self,
            Self::NotFound { .. }
                | Self::ModuleResolutionModeMismatch { .. }
                | Self::PackageJsonError { .. }
                | Self::PathMappingFailed { .. }
        )
    }
}

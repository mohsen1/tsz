//! Diagnostic constants and error code selection for module resolution.
//!
//! All TS diagnostic codes emitted by module resolution (TS2307, TS2732,
//! TS2792, TS2834, TS2835, TS5097, TS6142, TS7016) are owned here.

use crate::diagnostics::Diagnostic;
use crate::span::Span;
use std::path::PathBuf;

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

/// TS2209: The project root is ambiguous, but is required to resolve export map entry.
/// Emitted when a package imports itself (self-reference) with exports field in package.json
/// but rootDir is not set.
pub const AMBIGUOUS_PROJECT_ROOT_FOR_EXPORT_MAP: u32 = 2209;

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
    /// TS2209: The project root is ambiguous, but is required to resolve export map entry.
    /// Emitted when a package imports itself (self-reference) with exports field in package.json
    /// but rootDir is not set in Node16/NodeNext/Bundler module resolution.
    AmbiguousProjectRoot {
        /// Export map entry that could not be resolved (e.g., "./cjs")
        export_map_entry: String,
        /// Package.json file path
        package_json_path: String,
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
            Self::AmbiguousProjectRoot {
                export_map_entry,
                package_json_path,
                containing_file,
                span,
            } => Diagnostic::error(
                containing_file,
                *span,
                format!(
                    "The project root is ambiguous, but is required to resolve export map entry '{export_map_entry}' in file '{package_json_path}'. Supply the `rootDir` compiler option to disambiguate.",
                ),
                AMBIGUOUS_PROJECT_ROOT_FOR_EXPORT_MAP,
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
            }
            | Self::AmbiguousProjectRoot {
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
            | Self::JsonModuleWithoutResolveJsonModule { span, .. }
            | Self::AmbiguousProjectRoot { span, .. } => *span,
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

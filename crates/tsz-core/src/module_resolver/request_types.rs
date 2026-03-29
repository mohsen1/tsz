//! Public types for the module resolution API.
//!
//! This module contains all public-facing types that form the stable
//! module resolution boundary: request/result types, enums, and the
//! `ResolvedModule` structure.

use crate::span::Span;
use std::path::{Path, PathBuf};

use super::COULD_NOT_FIND_DECLARATION_FILE;

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
    /// Optional explicit resolution mode override from import attributes.
    ///
    /// When present, this should take precedence over the importing file's
    /// implied ESM/CJS mode for conditional exports/imports resolution.
    pub resolution_mode_override: Option<ImportingModuleKind>,
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

    /// Resolved to a JS file in `node_modules` (external) with TS7016 error.
    /// Unlike `untyped_js`, this preserves the resolved path so the import still works.
    pub fn resolved_untyped_js(
        resolved_path: PathBuf,
        no_implicit_any: bool,
        specifier: &str,
    ) -> Self {
        Self {
            error: if no_implicit_any {
                Some(ModuleLookupError {
                    code: COULD_NOT_FIND_DECLARATION_FILE,
                    message: format!(
                        "Could not find a declaration file for module '{}'. '{}' implicitly has an 'any' type.",
                        specifier,
                        resolved_path.display()
                    ),
                })
            } else {
                None
            },
            resolved_path: Some(resolved_path),
            treat_as_resolved: false,
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

impl ImportingModuleKind {
    /// Return the package.json exports/imports condition string for this module kind.
    ///
    /// - `Esm` → `"import"`
    /// - `CommonJs` → `"require"`
    ///
    /// This is the condition used when resolving conditional exports/imports in
    /// `package.json`. Drivers that need a `"import"` / `"require"` string for
    /// per-file module format decisions (e.g., Node16/NodeNext emit) can use this
    /// instead of reimplementing the extension + package.json walk-up logic.
    pub const fn as_condition_str(&self) -> &'static str {
        match self {
            Self::Esm => "import",
            Self::CommonJs => "require",
        }
    }

    /// Whether this is ESM mode.
    pub const fn is_esm(&self) -> bool {
        matches!(self, Self::Esm)
    }

    /// Whether this is CommonJS mode.
    pub const fn is_cjs(&self) -> bool {
        matches!(self, Self::CommonJs)
    }
}

impl std::fmt::Display for ImportingModuleKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_condition_str())
    }
}

/// Matches TypeScript's `pathIsRelative` check: `/^\.\..?(?:$|[\\/])/`.
///
/// A specifier is relative only when it starts with `./`, `../`, `.` alone,
/// or `..` alone.  Notably, `.prisma/client` starts with `.` but is NOT a
/// relative specifier -- it is a bare module name.
pub fn is_path_relative(specifier: &str) -> bool {
    matches!(
        specifier.as_bytes(),
        [b'.'] | [b'.', b'.'] | [b'.', b'/' | b'\\', ..] | [b'.', b'.', b'/' | b'\\', ..]
    )
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

    /// Check if this is a declaration file extension (.d.ts, .d.mts, .d.cts).
    ///
    /// This replaces scattered `path.ends_with(".d.ts") || ...` checks in the driver.
    pub const fn is_declaration(&self) -> bool {
        matches!(self, Self::Dts | Self::DmTs | Self::DCts)
    }

    /// Check if this is any TypeScript source extension (.ts, .tsx, .mts, .cts).
    ///
    /// Declaration files (.d.ts, .d.mts, .d.cts) are NOT included — use
    /// `is_declaration()` for those.
    pub const fn is_typescript_source(&self) -> bool {
        matches!(self, Self::Ts | Self::Tsx | Self::Mts | Self::Cts)
    }

    /// Check if this is any JavaScript extension (.js, .jsx, .mjs, .cjs).
    pub const fn is_javascript(&self) -> bool {
        matches!(self, Self::Js | Self::Jsx | Self::Mjs | Self::Cjs)
    }
}

impl std::fmt::Display for ModuleExtension {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

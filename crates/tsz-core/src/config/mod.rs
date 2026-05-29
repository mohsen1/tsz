use anyhow::{Context, Result, anyhow, bail};
use serde::{Deserialize, Deserializer};
use std::collections::VecDeque;

use rustc_hash::{FxHashMap, FxHashSet};
use std::env;
use std::path::{Path, PathBuf};

use crate::checker::context::ScriptTarget as CheckerScriptTarget;
use crate::checker::diagnostics::Diagnostic;
use crate::emitter::{ModuleKind, NewLineKind, PrinterOptions, ScriptTarget};
use tsz_common::diagnostics::data::{diagnostic_codes, diagnostic_messages};
use tsz_common::diagnostics::format_message;

mod extends;

use extends::{
    anchor_inherited_path_options, anchor_inherited_root_selectors, merge_configs,
    resolve_extends_path,
};

/// Custom deserializer for boolean options that accepts both bool and string values.
/// This handles cases where tsconfig.json contains `"strict": "true"` instead of `"strict": true`.
fn deserialize_bool_or_string<'de, D>(deserializer: D) -> Result<Option<bool>, D::Error>
where
    D: Deserializer<'de>,
{
    use serde::de::Error;

    // Use a helper enum to deserialize either a bool or a string
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum BoolOrString {
        Bool(bool),
        String(String),
    }

    match Option::<BoolOrString>::deserialize(deserializer)? {
        None => Ok(None),
        Some(BoolOrString::Bool(b)) => Ok(Some(b)),
        Some(BoolOrString::String(s)) => {
            // Parse common string representations of boolean values
            let normalized = s.trim().to_lowercase();
            match normalized.as_str() {
                "true" | "1" | "yes" | "on" => Ok(Some(true)),
                "false" | "0" | "no" | "off" => Ok(Some(false)),
                _ => {
                    // Invalid boolean string - return error with helpful message
                    Err(Error::custom(format!(
                        "invalid boolean value: '{s}'. Expected true, false, 'true', or 'false'",
                    )))
                }
            }
        }
    }
}

/// Represents the `extends` field which can be a single string or an array of strings.
/// tsc 5.0+ supports `"extends": ["./base1.json", "./base2.json"]`.
#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub enum ExtendsValue {
    /// A single config path to extend from.
    Single(String),
    /// An array of config paths to extend from (applied in order, later overrides earlier).
    Array(Vec<String>),
}

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct TsConfig {
    #[serde(default)]
    pub extends: Option<ExtendsValue>,
    #[serde(default)]
    pub compiler_options: Option<CompilerOptions>,
    #[serde(default)]
    pub include: Option<Vec<String>>,
    #[serde(default)]
    pub exclude: Option<Vec<String>>,
    #[serde(default)]
    pub files: Option<Vec<String>>,
    /// Project references for composite project builds
    #[serde(default)]
    pub references: Option<Vec<TsConfigReference>>,
}

/// A project reference entry in tsconfig.json
#[derive(Debug, Clone, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct TsConfigReference {
    /// Path to the referenced project's tsconfig.json or directory
    pub path: String,
    /// If true, prepend the output of this project to the output of the referencing project
    #[serde(default)]
    pub prepend: bool,
}

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct CompilerOptions {
    #[serde(default)]
    pub target: Option<String>,
    #[serde(default)]
    pub module: Option<String>,
    #[serde(default)]
    pub module_resolution: Option<String>,
    /// Use the package.json 'exports' field when resolving package imports.
    #[serde(default, deserialize_with = "deserialize_bool_or_string")]
    pub resolve_package_json_exports: Option<bool>,
    /// Use the package.json 'imports' field when resolving imports.
    #[serde(default, deserialize_with = "deserialize_bool_or_string")]
    pub resolve_package_json_imports: Option<bool>,
    /// List of file name suffixes to search when resolving a module.
    #[serde(default)]
    pub module_suffixes: Option<Vec<String>>,
    /// Enable importing .json files.
    #[serde(default, deserialize_with = "deserialize_bool_or_string")]
    pub resolve_json_module: Option<bool>,
    /// Enable importing files with any extension, provided a declaration file is present.
    #[serde(default, deserialize_with = "deserialize_bool_or_string")]
    pub allow_arbitrary_extensions: Option<bool>,
    /// Allow imports to include TypeScript file extensions.
    #[serde(default, deserialize_with = "deserialize_bool_or_string")]
    pub allow_importing_ts_extensions: Option<bool>,
    /// Rewrite '.ts', '.tsx', '.mts', and '.cts' file extensions in relative import paths.
    #[serde(default, deserialize_with = "deserialize_bool_or_string")]
    pub rewrite_relative_import_extensions: Option<bool>,
    #[serde(default)]
    pub types_versions_compiler_version: Option<String>,
    #[serde(default)]
    pub types: Option<Vec<String>>,
    #[serde(default)]
    pub type_roots: Option<Vec<String>>,
    #[serde(default)]
    pub jsx: Option<String>,
    #[serde(default)]
    #[serde(rename = "jsxFactory")]
    pub jsx_factory: Option<String>,
    #[serde(default)]
    #[serde(rename = "jsxFragmentFactory")]
    pub jsx_fragment_factory: Option<String>,
    #[serde(default)]
    #[serde(rename = "jsxImportSource")]
    pub jsx_import_source: Option<String>,
    #[serde(default)]
    #[serde(rename = "reactNamespace")]
    pub react_namespace: Option<String>,

    #[serde(default)]
    pub lib: Option<Vec<String>>,
    #[serde(default, deserialize_with = "deserialize_bool_or_string")]
    pub no_lib: Option<bool>,
    #[serde(default, deserialize_with = "deserialize_bool_or_string")]
    pub lib_replacement: Option<bool>,
    #[serde(
        default,
        deserialize_with = "deserialize_bool_or_string",
        rename = "noTypesAndSymbols"
    )]
    pub no_types_and_symbols: Option<bool>,
    #[serde(default)]
    pub base_url: Option<String>,
    #[serde(default)]
    pub paths: Option<FxHashMap<String, Vec<String>>>,
    #[serde(default)]
    pub root_dir: Option<String>,
    #[serde(default)]
    pub root_dirs: Option<Vec<String>>,
    #[serde(default)]
    pub out_dir: Option<String>,
    #[serde(default)]
    pub out_file: Option<String>,
    #[serde(default, deserialize_with = "deserialize_bool_or_string")]
    pub composite: Option<bool>,
    #[serde(default, deserialize_with = "deserialize_bool_or_string")]
    pub declaration: Option<bool>,
    #[serde(default, deserialize_with = "deserialize_bool_or_string")]
    pub emit_declaration_only: Option<bool>,
    #[serde(default)]
    pub declaration_dir: Option<String>,
    #[serde(default, deserialize_with = "deserialize_bool_or_string")]
    pub source_map: Option<bool>,
    #[serde(default, deserialize_with = "deserialize_bool_or_string")]
    pub inline_source_map: Option<bool>,
    #[serde(default, deserialize_with = "deserialize_bool_or_string")]
    pub declaration_map: Option<bool>,
    #[serde(default)]
    pub ts_build_info_file: Option<String>,
    #[serde(default, deserialize_with = "deserialize_bool_or_string")]
    pub incremental: Option<bool>,
    #[serde(default, deserialize_with = "deserialize_bool_or_string")]
    pub strict: Option<bool>,
    /// Enable experimental Sound Mode checks.
    #[serde(default, deserialize_with = "deserialize_bool_or_string")]
    pub sound: Option<bool>,
    /// Opt first-party declaration files (.d.ts) into sound checking.
    #[serde(default, deserialize_with = "deserialize_bool_or_string")]
    pub sound_check_declarations: Option<bool>,
    /// Report sound diagnostics without failing the build.
    #[serde(default, deserialize_with = "deserialize_bool_or_string")]
    pub sound_report_only: Option<bool>,
    /// Enable pedantic sound heuristics beyond the core sound bundle.
    #[serde(default, deserialize_with = "deserialize_bool_or_string")]
    pub sound_pedantic: Option<bool>,
    #[serde(default, deserialize_with = "deserialize_bool_or_string")]
    pub no_emit: Option<bool>,
    /// Emit a UTF-8 Byte Order Mark (BOM) in the beginning of output files.
    #[serde(
        default,
        rename = "emitBOM",
        deserialize_with = "deserialize_bool_or_string"
    )]
    pub emit_bom: Option<bool>,
    #[serde(default, deserialize_with = "deserialize_bool_or_string")]
    pub no_check: Option<bool>,
    #[serde(default, deserialize_with = "deserialize_bool_or_string")]
    pub no_resolve: Option<bool>,
    /// Do not resolve symlinks to their real path.
    #[serde(default, deserialize_with = "deserialize_bool_or_string")]
    pub preserve_symlinks: Option<bool>,
    #[serde(default, deserialize_with = "deserialize_bool_or_string")]
    pub no_emit_on_error: Option<bool>,
    #[serde(default, deserialize_with = "deserialize_bool_or_string")]
    pub isolated_modules: Option<bool>,
    #[serde(default, deserialize_with = "deserialize_bool_or_string")]
    pub isolated_declarations: Option<bool>,
    #[serde(default, deserialize_with = "deserialize_bool_or_string")]
    pub verbatim_module_syntax: Option<bool>,
    /// Custom conditions for package.json exports resolution
    #[serde(default)]
    pub custom_conditions: Option<Vec<String>>,
    /// Emit additional JavaScript to ease support for importing CommonJS modules
    #[serde(default, deserialize_with = "deserialize_bool_or_string")]
    pub es_module_interop: Option<bool>,
    /// Allow 'import x from y' when a module doesn't have a default export
    #[serde(default, deserialize_with = "deserialize_bool_or_string")]
    pub allow_synthetic_default_imports: Option<bool>,
    /// Enable experimental support for legacy experimental decorators
    #[serde(default, deserialize_with = "deserialize_bool_or_string")]
    pub experimental_decorators: Option<bool>,
    /// Emit design-type metadata for decorated declarations in source files
    #[serde(default, deserialize_with = "deserialize_bool_or_string")]
    pub emit_decorator_metadata: Option<bool>,
    /// Import emit helpers from tslib instead of inlining them per-file
    #[serde(default, deserialize_with = "deserialize_bool_or_string")]
    pub import_helpers: Option<bool>,
    /// Disable emitting helper declarations.
    #[serde(default, deserialize_with = "deserialize_bool_or_string")]
    pub no_emit_helpers: Option<bool>,
    /// Emit more compliant iteration lowering for ES5/ES3 targets.
    #[serde(default, deserialize_with = "deserialize_bool_or_string")]
    pub downlevel_iteration: Option<bool>,
    /// Disable emitting comments.
    #[serde(default, deserialize_with = "deserialize_bool_or_string")]
    pub remove_comments: Option<bool>,
    /// Set the newline character used in emitted files.
    #[serde(default)]
    pub new_line: Option<String>,
    /// Allow JavaScript files to be a part of your program
    #[serde(default, deserialize_with = "deserialize_bool_or_string")]
    pub allow_js: Option<bool>,
    /// Enable error reporting in type-checked JavaScript files
    #[serde(default, deserialize_with = "deserialize_bool_or_string")]
    pub check_js: Option<bool>,
    /// Skip type checking of declaration files (.d.ts)
    #[serde(default, deserialize_with = "deserialize_bool_or_string")]
    pub skip_lib_check: Option<bool>,
    /// Skip type checking of default library declaration files (.d.ts)
    #[serde(default, deserialize_with = "deserialize_bool_or_string")]
    pub skip_default_lib_check: Option<bool>,
    /// Disable emitting declarations that have '@internal' in their JSDoc comments
    #[serde(default, deserialize_with = "deserialize_bool_or_string")]
    pub strip_internal: Option<bool>,
    /// Parse in strict mode and emit "use strict" for each source file
    #[serde(default, deserialize_with = "deserialize_bool_or_string")]
    pub always_strict: Option<bool>,
    /// Use `Object.defineProperty` semantics for class fields when downleveling.
    #[serde(default, deserialize_with = "deserialize_bool_or_string")]
    pub use_define_for_class_fields: Option<bool>,
    /// Raise error on expressions and declarations with an implied 'any' type
    #[serde(
        default,
        alias = "noImplicitAny",
        deserialize_with = "deserialize_bool_or_string"
    )]
    pub no_implicit_any: Option<bool>,
    /// Enable error reporting when a function doesn't explicitly return in all code paths
    #[serde(
        default,
        alias = "noImplicitReturns",
        deserialize_with = "deserialize_bool_or_string"
    )]
    pub no_implicit_returns: Option<bool>,
    /// Enable strict null checks
    #[serde(
        default,
        alias = "strictNullChecks",
        deserialize_with = "deserialize_bool_or_string"
    )]
    pub strict_null_checks: Option<bool>,
    /// Enable strict checking of function types
    #[serde(
        default,
        alias = "strictFunctionTypes",
        deserialize_with = "deserialize_bool_or_string"
    )]
    pub strict_function_types: Option<bool>,
    /// Check for class properties that are declared but not set in the constructor
    #[serde(
        default,
        alias = "strictPropertyInitialization",
        deserialize_with = "deserialize_bool_or_string"
    )]
    pub strict_property_initialization: Option<bool>,
    /// Raise error on 'this' expressions with an implied 'any' type
    #[serde(
        default,
        alias = "noImplicitThis",
        deserialize_with = "deserialize_bool_or_string"
    )]
    pub no_implicit_this: Option<bool>,
    /// Default catch clause variables as 'unknown' instead of 'any'
    #[serde(
        default,
        alias = "useUnknownInCatchVariables",
        deserialize_with = "deserialize_bool_or_string"
    )]
    pub use_unknown_in_catch_variables: Option<bool>,
    /// Interpret optional property types as written, rather than adding 'undefined'
    #[serde(
        default,
        alias = "exactOptionalPropertyTypes",
        deserialize_with = "deserialize_bool_or_string"
    )]
    pub exact_optional_property_types: Option<bool>,
    /// Add 'undefined' to a type when accessed using an index
    #[serde(
        default,
        alias = "noUncheckedIndexedAccess",
        deserialize_with = "deserialize_bool_or_string"
    )]
    pub no_unchecked_indexed_access: Option<bool>,
    /// Enforce bracket access for properties that come only from an index signature
    #[serde(
        default,
        alias = "noPropertyAccessFromIndexSignature",
        deserialize_with = "deserialize_bool_or_string"
    )]
    pub no_property_access_from_index_signature: Option<bool>,
    /// Check that the arguments for 'bind', 'call', and 'apply' methods match the original function
    #[serde(
        default,
        alias = "strictBindCallApply",
        deserialize_with = "deserialize_bool_or_string"
    )]
    pub strict_bind_call_apply: Option<bool>,
    /// Built-in iterators use `undefined` for `TReturn` instead of `any`
    #[serde(
        default,
        alias = "strictBuiltinIteratorReturn",
        deserialize_with = "deserialize_bool_or_string"
    )]
    pub strict_builtin_iterator_return: Option<bool>,
    /// Report errors on unused local variables
    #[serde(default, deserialize_with = "deserialize_bool_or_string")]
    pub no_unused_locals: Option<bool>,
    /// Report errors on unused parameters
    #[serde(default, deserialize_with = "deserialize_bool_or_string")]
    pub no_unused_parameters: Option<bool>,
    /// Do not report errors on unreachable code
    #[serde(default, deserialize_with = "deserialize_bool_or_string")]
    pub allow_unreachable_code: Option<bool>,
    /// Do not report errors on unused labels
    #[serde(default, deserialize_with = "deserialize_bool_or_string")]
    pub allow_unused_labels: Option<bool>,
    /// Report errors for fallthrough cases in switch statements
    #[serde(default, deserialize_with = "deserialize_bool_or_string")]
    pub no_fallthrough_cases_in_switch: Option<bool>,
    /// Check side-effect imports for module resolution errors
    #[serde(default, deserialize_with = "deserialize_bool_or_string")]
    pub no_unchecked_side_effect_imports: Option<bool>,
    /// Require 'override' modifier on members that override base class members
    #[serde(
        default,
        alias = "noImplicitOverride",
        deserialize_with = "deserialize_bool_or_string"
    )]
    pub no_implicit_override: Option<bool>,
    /// Control what method is used to detect module-format JS files.
    #[serde(default)]
    pub module_detection: Option<String>,
    /// Suppress deprecation warnings. Valid values: "5.0", "6.0".
    #[serde(default)]
    pub ignore_deprecations: Option<String>,
    /// Allow accessing UMD globals from modules.
    #[serde(default, deserialize_with = "deserialize_bool_or_string")]
    pub allow_umd_global_access: Option<bool>,
    /// Preserve const enum declarations in emitted code.
    #[serde(default, deserialize_with = "deserialize_bool_or_string")]
    pub preserve_const_enums: Option<bool>,
    /// Only allow syntax that can be fully erased (no runtime emit).
    #[serde(default, deserialize_with = "deserialize_bool_or_string")]
    pub erasable_syntax_only: Option<bool>,
    /// Specify the maximum folder depth used for checking JavaScript files from `node_modules`.
    /// Only applicable with 'allowJs'. Defaults to 0.
    #[serde(default)]
    pub max_node_module_js_depth: Option<u32>,
    /// Options that had TS5024 type errors — should NOT have defaults applied.
    /// This is set during tsconfig parsing and is not deserialized from JSON.
    #[serde(skip)]
    pub invalidated_options: Vec<String>,
}

// Re-export CheckerOptions from checker::context for unified API
pub use crate::checker::context::CheckerOptions;

#[derive(Debug, Clone, Default)]
pub struct ResolvedCompilerOptions {
    pub printer: PrinterOptions,
    pub checker: CheckerOptions,
    pub jsx: Option<JsxEmit>,
    pub lib_files: Vec<PathBuf>,
    pub lib_is_default: bool,
    pub lib_replacement: bool,
    pub module_resolution: Option<ModuleResolutionKind>,
    pub resolve_package_json_exports: bool,
    pub resolve_package_json_imports: bool,
    pub module_suffixes: Vec<String>,
    pub resolve_json_module: bool,
    pub allow_arbitrary_extensions: bool,
    pub allow_importing_ts_extensions: bool,
    pub rewrite_relative_import_extensions: bool,
    pub trace_resolution: bool,
    pub types_versions_compiler_version: Option<String>,
    pub types: Option<Vec<String>>,
    pub type_roots: Option<Vec<PathBuf>>,
    pub base_url: Option<PathBuf>,
    pub paths: Option<Vec<PathMapping>>,
    pub root_dir: Option<PathBuf>,
    pub root_dirs: Vec<PathBuf>,
    pub out_dir: Option<PathBuf>,
    pub out_file: Option<PathBuf>,
    pub declaration_dir: Option<PathBuf>,
    pub composite: bool,
    pub emit_declarations: bool,
    pub emit_declaration_only: bool,
    pub source_map: bool,
    pub inline_source_map: bool,
    pub declaration_map: bool,
    pub ts_build_info_file: Option<PathBuf>,
    pub incremental: bool,
    pub no_emit: bool,
    pub emit_bom: bool,
    pub no_emit_on_error: bool,
    /// Skip module graph expansion from imports/references when checking.
    pub no_resolve: bool,
    /// Preserve symlink paths instead of canonicalizing to real paths.
    pub preserve_symlinks: bool,
    pub isolated_declarations: bool,
    pub import_helpers: bool,
    /// Disable full type checking (only parse and emit errors reported).
    pub no_check: bool,
    /// Custom conditions for package.json exports resolution
    pub custom_conditions: Vec<String>,
    /// Emit additional JavaScript to ease support for importing CommonJS modules
    pub es_module_interop: bool,
    /// Allow 'import x from y' when a module doesn't have a default export
    pub allow_synthetic_default_imports: bool,
    /// Allow JavaScript files to be part of the program
    pub allow_js: bool,
    /// Enable error reporting in type-checked JavaScript files
    pub check_js: bool,
    /// Whether `checkJs` was explicitly set to `false` in compiler options.
    /// When `true`, ALL semantic errors are suppressed in JS files — even the
    /// `plainJSErrors` allowlist (TS2451, TS2492, etc.) that applies in the
    /// default (no-`checkJs`) mode. Distinct from `check_js == false` because
    /// that default-false is the same as "not configured", which still permits
    /// `plainJSErrors`.
    pub explicit_check_js_false: bool,
    /// Skip type checking of declaration files (.d.ts)
    pub skip_lib_check: bool,
    /// Skip type checking of default library declaration files (.d.ts)
    pub skip_default_lib_check: bool,
    /// Disable emitting declarations that have '@internal' in their JSDoc comments
    pub strip_internal: bool,
    /// Maximum folder depth for checking JS files from `node_modules`.
    /// Only applicable with `allowJs`. Default: 0 (don't check JS in `node_modules`).
    pub max_node_module_js_depth: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JsxEmit {
    Preserve,
    React,
    ReactJsx,
    ReactJsxDev,
    ReactNative,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModuleResolutionKind {
    Classic,
    Node,
    Node16,
    NodeNext,
    Bundler,
}

impl ModuleResolutionKind {
    /// Parse a TypeScript compiler option `moduleResolution` value.
    ///
    /// This accepts tsc spelling variants.
    #[must_use]
    pub fn from_ts_str(value: &str) -> Option<Self> {
        let normalized = normalize_option(value.trim());
        match normalized.as_str() {
            "classic" => Some(Self::Classic),
            "node" | "node10" => Some(Self::Node),
            "node16" => Some(Self::Node16),
            "nodenext" => Some(Self::NodeNext),
            "bundler" => Some(Self::Bundler),
            _ => None,
        }
    }

    /// Return a canonical TypeScript option spelling.
    #[must_use]
    pub const fn as_ts_str(self) -> &'static str {
        match self {
            Self::Classic => "classic",
            Self::Node => "node10",
            Self::Node16 => "node16",
            Self::NodeNext => "nodenext",
            Self::Bundler => "bundler",
        }
    }

    #[must_use]
    pub const fn is_modern(self) -> bool {
        matches!(self, Self::Node16 | Self::NodeNext | Self::Bundler)
    }
}

/// Default module kind used when `module` is omitted.
///
/// The default differs depending on whether `target` was explicitly supplied,
/// matching the existing tsc-parity behavior used by config resolution.
#[must_use]
pub const fn default_module_kind_for_target(
    target: ScriptTarget,
    target_explicitly_set: bool,
) -> ModuleKind {
    if !target_explicitly_set {
        return ModuleKind::ESNext;
    }

    match target {
        ScriptTarget::ES3 | ScriptTarget::ES5 => ModuleKind::CommonJS,
        ScriptTarget::ES2015
        | ScriptTarget::ES2016
        | ScriptTarget::ES2017
        | ScriptTarget::ES2018
        | ScriptTarget::ES2019 => ModuleKind::ES2015,
        ScriptTarget::ES2020 | ScriptTarget::ES2021 => ModuleKind::ES2020,
        ScriptTarget::ES2022
        | ScriptTarget::ES2023
        | ScriptTarget::ES2024
        | ScriptTarget::ES2025 => ModuleKind::ES2022,
        ScriptTarget::ESNext => ModuleKind::ESNext,
    }
}

/// Default `moduleResolution` used when it is omitted for a module kind.
#[must_use]
pub const fn default_module_resolution_for_module(module: ModuleKind) -> ModuleResolutionKind {
    match module {
        ModuleKind::None | ModuleKind::AMD | ModuleKind::UMD | ModuleKind::System => {
            ModuleResolutionKind::Classic
        }
        ModuleKind::NodeNext => ModuleResolutionKind::NodeNext,
        ModuleKind::Node16 | ModuleKind::Node18 | ModuleKind::Node20 => {
            ModuleResolutionKind::Node16
        }
        ModuleKind::CommonJS
        | ModuleKind::ES2015
        | ModuleKind::ES2020
        | ModuleKind::ES2022
        | ModuleKind::ESNext
        | ModuleKind::Preserve => ModuleResolutionKind::Bundler,
    }
}

/// Default `moduleDetection` shown by tsc-style config output for a module kind.
#[must_use]
pub const fn default_module_detection_for_module(module: ModuleKind) -> &'static str {
    match module {
        ModuleKind::Node16 | ModuleKind::Node18 | ModuleKind::Node20 | ModuleKind::NodeNext => {
            "force"
        }
        _ => "auto",
    }
}

#[derive(Debug, Clone)]
pub struct PathMapping {
    pub pattern: String,
    pub(crate) prefix: String,
    pub(crate) suffix: String,
    pub targets: Vec<String>,
}

impl PathMapping {
    pub fn match_specifier(&self, specifier: &str) -> Option<String> {
        if !self.pattern.contains('*') {
            return (self.pattern == specifier).then(String::new);
        }

        if !specifier.starts_with(&self.prefix) || !specifier.ends_with(&self.suffix) {
            return None;
        }

        let start = self.prefix.len();
        let end = specifier.len().saturating_sub(self.suffix.len());
        if end < start {
            return None;
        }

        Some(specifier[start..end].to_string())
    }

    pub const fn specificity(&self) -> usize {
        self.prefix.len()
    }
}

impl ResolvedCompilerOptions {
    pub const fn effective_module_resolution(&self) -> ModuleResolutionKind {
        if let Some(resolution) = self.module_resolution {
            return resolution;
        }

        default_module_resolution_for_module(self.printer.module)
    }
}

pub fn resolve_compiler_options(
    options: Option<&CompilerOptions>,
) -> Result<ResolvedCompilerOptions> {
    let mut resolved = ResolvedCompilerOptions::default();
    // TypeScript 6 defaults alwaysStrict emit on. An explicit
    // alwaysStrict=false below can still suppress the prologue.
    resolved.printer.always_strict = true;
    let Some(options) = options else {
        let default_module = default_module_kind_for_target(resolved.printer.target, false);
        resolved.printer.module = default_module;
        resolved.checker.module = default_module;
        resolved.checker.target = checker_target_from_emitter(resolved.printer.target);
        resolved.lib_files = resolve_default_lib_files(resolved.printer.target)?;
        resolved.lib_is_default = true;
        resolved.module_suffixes = vec![String::new()];
        let default_resolution = resolved.effective_module_resolution();
        resolved.resolve_package_json_exports = matches!(
            default_resolution,
            ModuleResolutionKind::Node16
                | ModuleResolutionKind::NodeNext
                | ModuleResolutionKind::Bundler
        );
        resolved.resolve_package_json_imports = resolved.resolve_package_json_exports;
        let resolve_json_module = matches!(default_resolution, ModuleResolutionKind::Bundler);
        resolved.resolve_json_module = resolve_json_module;
        resolved.checker.resolve_json_module = resolve_json_module;
        return Ok(resolved);
    };

    if let Some(target) = options.target.as_deref() {
        resolved.printer.target = parse_script_target(target)?;
    }
    resolved.checker.target = checker_target_from_emitter(resolved.printer.target);

    let module_explicitly_set = options.module.is_some();
    if let Some(module) = options.module.as_deref() {
        let kind = parse_module_kind(module)?;
        resolved.printer.module = kind;
        resolved.checker.module = kind;
    } else {
        let default_module =
            default_module_kind_for_target(resolved.printer.target, options.target.is_some());
        resolved.printer.module = default_module;
        resolved.checker.module = default_module;
    }
    resolved.checker.module_explicitly_set = module_explicitly_set;

    if let Some(module_resolution) = options.module_resolution.as_deref() {
        let value = module_resolution.trim();
        if !value.is_empty() {
            resolved.module_resolution = Some(parse_module_resolution(value)?);
        }
    }

    // When module is not explicitly set, infer it from moduleResolution (matches tsc behavior).
    // tsc infers module: node16 when moduleResolution: node16, etc.
    if !module_explicitly_set && let Some(mr) = resolved.module_resolution {
        let inferred = match mr {
            ModuleResolutionKind::Node16 => Some(ModuleKind::Node16),
            ModuleResolutionKind::NodeNext => Some(ModuleKind::NodeNext),
            _ => None,
        };
        if let Some(kind) = inferred {
            resolved.printer.module = kind;
            resolved.checker.module = kind;
        }
    }
    let effective_resolution = resolved.effective_module_resolution();
    // TS2792 remains tied to Classic resolution sites in conformance. Keep the
    // downstream checker/resolver flag derived from the computed effective
    // module resolution instead of hard-disabling it globally.
    resolved.checker.implied_classic_resolution =
        matches!(effective_resolution, ModuleResolutionKind::Classic);
    resolved.resolve_package_json_exports = options.resolve_package_json_exports.unwrap_or({
        matches!(
            effective_resolution,
            ModuleResolutionKind::Node16
                | ModuleResolutionKind::NodeNext
                | ModuleResolutionKind::Bundler
        )
    });
    // Per tsc 6.0, `resolvePackageJsonImports` defaults to true only for
    // Node16/NodeNext/Bundler. Legacy `node`/`node10` does NOT resolve
    // `package.json#imports` unless the option is explicitly enabled.
    resolved.resolve_package_json_imports = options.resolve_package_json_imports.unwrap_or({
        matches!(
            effective_resolution,
            ModuleResolutionKind::Node16
                | ModuleResolutionKind::NodeNext
                | ModuleResolutionKind::Bundler
        )
    });
    if let Some(module_suffixes) = options.module_suffixes.as_ref() {
        resolved.module_suffixes = module_suffixes.clone();
    } else {
        resolved.module_suffixes = vec![String::new()];
    }
    if let Some(resolve_json_module) = options.resolve_json_module {
        resolved.resolve_json_module = resolve_json_module;
        resolved.checker.resolve_json_module = resolve_json_module;
    } else {
        // tsc 6.0 only implies resolveJsonModule for bundler resolution.
        let resolve_json_module = matches!(effective_resolution, ModuleResolutionKind::Bundler);
        resolved.resolve_json_module = resolve_json_module;
        resolved.checker.resolve_json_module = resolve_json_module;
    }
    if let Some(import_helpers) = options.import_helpers {
        resolved.import_helpers = import_helpers;
        resolved.printer.import_helpers = import_helpers;
    }
    if let Some(allow_arbitrary_extensions) = options.allow_arbitrary_extensions {
        resolved.allow_arbitrary_extensions = allow_arbitrary_extensions;
    }
    if let Some(allow_importing_ts_extensions) = options.allow_importing_ts_extensions {
        resolved.allow_importing_ts_extensions = allow_importing_ts_extensions;
    }
    if let Some(rewrite_relative_import_extensions) = options.rewrite_relative_import_extensions {
        resolved.rewrite_relative_import_extensions = rewrite_relative_import_extensions;
    }

    if let Some(types_versions_compiler_version) =
        options.types_versions_compiler_version.as_deref()
    {
        let value = types_versions_compiler_version.trim();
        if !value.is_empty() {
            resolved.types_versions_compiler_version = Some(value.to_string());
        }
    }

    if let Some(types) = options.types.as_ref() {
        let list: Vec<String> = types
            .iter()
            .filter_map(|value| {
                let trimmed = value.trim();
                if trimmed.is_empty() {
                    None
                } else {
                    Some(trimmed.to_string())
                }
            })
            .collect();
        resolved.types = Some(list);
        resolved.checker.types_explicitly_set = true;
    }

    if let Some(type_roots) = options.type_roots.as_ref() {
        let roots: Vec<PathBuf> = type_roots
            .iter()
            .filter_map(|value| {
                let trimmed = value.trim();
                if trimmed.is_empty() {
                    None
                } else {
                    Some(PathBuf::from(trimmed))
                }
            })
            .collect();
        resolved.type_roots = Some(roots);
    }

    if let Some(factory) = options.jsx_factory.as_deref() {
        // tsc preserves `jsxFactory` verbatim — even when invalid. The
        // TS5067 / TS5059 diagnostics surface separately during config
        // validation; emit uses whatever was configured.
        resolved.checker.jsx_factory = factory.to_string();
        resolved.checker.jsx_factory_from_config = true;
    } else if let Some(ns) = options.react_namespace.as_deref() {
        resolved.checker.jsx_factory = format!("{ns}.createElement");
    }
    if let Some(frag) = options.jsx_fragment_factory.as_deref() {
        // tsc falls back to `React.Fragment` when `jsxFragmentFactory` is not
        // a valid identifier chain (e.g. `234`). Asymmetric with `jsxFactory`
        // by design — see the test pair `reactNamespaceInvalidInput` (factory
        // preserved) vs `jsxFactoryAndJsxFragmentFactoryErrorNotIdentifier`
        // (fragment factory falls back).
        if is_valid_identifier_or_qualified_name(frag) {
            resolved.checker.jsx_fragment_factory = frag.to_string();
            resolved.checker.jsx_fragment_factory_from_config = true;
        }
        // else: keep default `React.Fragment`
    }
    if let Some(source) = options.jsx_import_source.as_deref() {
        resolved.checker.jsx_import_source = source.to_string();
    }

    if let Some(jsx) = options.jsx.as_deref() {
        let jsx_emit = parse_jsx_emit(jsx)?;
        resolved.jsx = Some(jsx_emit);
        resolved.checker.jsx_mode = jsx_emit_to_mode(jsx_emit);
    }

    if let Some(no_lib) = options.no_lib {
        resolved.checker.no_lib = no_lib;
    }

    if let Some(lib_replacement) = options.lib_replacement {
        resolved.lib_replacement = lib_replacement;
    }

    if resolved.checker.no_lib && options.lib.is_some() {
        return Err(anyhow::anyhow!(
            "Option 'lib' cannot be specified with option 'noLib'."
        ));
    }

    if let Some(no_types_and_symbols) = options.no_types_and_symbols {
        resolved.checker.no_types_and_symbols = no_types_and_symbols;
    }

    if resolved.checker.no_lib && options.lib.is_some() {
        bail!("Option 'lib' cannot be specified with option 'noLib'.");
    }

    if let Some(lib_list) = options.lib.as_ref() {
        resolved.lib_files = resolve_lib_files(lib_list)?;
        resolved.lib_is_default = false;
    } else if !resolved.checker.no_lib {
        // noTypesAndSymbols is a test harness directive that controls baseline
        // output (type/symbol baselines), NOT lib loading. Default libs must
        // still be loaded so that globals like Symbol, Promise, etc. are available.
        resolved.lib_files = resolve_default_lib_files(resolved.printer.target)?;
        resolved.lib_is_default = true;
    }

    let base_url = options.base_url.as_deref().map(str::trim);
    if let Some(base_url) = base_url
        && !base_url.is_empty()
    {
        resolved.base_url = Some(PathBuf::from(base_url));
    }

    if let Some(paths) = options.paths.as_ref()
        && !paths.is_empty()
    {
        resolved.paths = Some(build_path_mappings(paths));
    }

    if let Some(root_dir) = options.root_dir.as_deref()
        && !root_dir.is_empty()
    {
        resolved.root_dir = Some(PathBuf::from(root_dir));
    }

    if let Some(root_dirs) = options.root_dirs.as_ref() {
        resolved.root_dirs = root_dirs
            .iter()
            .filter_map(|value| {
                let trimmed = value.trim();
                if trimmed.is_empty() {
                    None
                } else {
                    Some(PathBuf::from(trimmed))
                }
            })
            .collect();
    }

    if let Some(out_dir) = options.out_dir.as_deref()
        && !out_dir.is_empty()
    {
        resolved.out_dir = Some(PathBuf::from(out_dir));
    }

    if let Some(out_file) = options.out_file.as_deref()
        && !out_file.is_empty()
    {
        resolved.out_file = Some(PathBuf::from(out_file));
    }

    if let Some(declaration_dir) = options.declaration_dir.as_deref()
        && !declaration_dir.is_empty()
    {
        resolved.declaration_dir = Some(PathBuf::from(declaration_dir));
    }

    // composite implies declaration and incremental (matching tsc behavior)
    if let Some(composite) = options.composite {
        resolved.composite = composite;
        if composite {
            // composite: true implies declaration: true and incremental: true
            resolved.emit_declarations = true;
            resolved.checker.emit_declarations = true;
            resolved.incremental = true;
        }
    }

    if let Some(declaration) = options.declaration {
        resolved.emit_declarations = declaration;
        resolved.checker.emit_declarations = declaration;
    }

    if let Some(emit_declaration_only) = options.emit_declaration_only {
        resolved.emit_declaration_only = emit_declaration_only;
    }

    if let Some(source_map) = options.source_map {
        resolved.source_map = source_map;
    }

    if let Some(inline_source_map) = options.inline_source_map {
        resolved.inline_source_map = inline_source_map;
    }

    if let Some(declaration_map) = options.declaration_map {
        resolved.declaration_map = declaration_map;
    }

    if let Some(no_emit_helpers) = options.no_emit_helpers {
        resolved.printer.no_emit_helpers = no_emit_helpers;
    }
    if options.import_helpers == Some(true) {
        // importHelpers means "import from tslib" - suppress inline helper emission.
        resolved.printer.no_emit_helpers = true;
    }

    if let Some(downlevel_iteration) = options.downlevel_iteration {
        resolved.printer.downlevel_iteration = downlevel_iteration;
        resolved.checker.downlevel_iteration = downlevel_iteration;
    }

    if let Some(remove_comments) = options.remove_comments {
        resolved.printer.remove_comments = remove_comments;
    }

    if let Some(new_line) = options.new_line.as_deref() {
        resolved.printer.new_line = parse_new_line_kind(new_line)?;
    }

    if let Some(ts_build_info_file) = options.ts_build_info_file.as_deref()
        && !ts_build_info_file.is_empty()
    {
        resolved.ts_build_info_file = Some(PathBuf::from(ts_build_info_file));
    }

    if let Some(incremental) = options.incremental {
        resolved.incremental = incremental;
    }

    if let Some(strict) = options.strict {
        resolved.checker.strict = strict;
        if strict {
            resolved.checker.no_implicit_any = true;
            resolved.checker.strict_null_checks = true;
            resolved.checker.strict_function_types = true;
            resolved.checker.strict_bind_call_apply = true;
            resolved.checker.strict_property_initialization = true;
            resolved.checker.no_implicit_this = true;
            resolved.checker.use_unknown_in_catch_variables = true;
            resolved.checker.always_strict = true;
            resolved.checker.strict_builtin_iterator_return = true;
            resolved.printer.always_strict = true;
        } else {
            resolved.checker.no_implicit_any = false;
            resolved.checker.strict_null_checks = false;
            resolved.checker.strict_function_types = false;
            resolved.checker.strict_bind_call_apply = false;
            resolved.checker.strict_property_initialization = false;
            resolved.checker.no_implicit_this = false;
            resolved.checker.use_unknown_in_catch_variables = false;
            resolved.checker.strict_builtin_iterator_return = false;
        }
    }

    if let Some(sound) = options.sound {
        resolved.checker.sound_mode = sound;
    }
    if let Some(v) = options.sound_check_declarations {
        resolved.checker.sound_check_declarations = v;
    }
    if let Some(v) = options.sound_report_only {
        resolved.checker.sound_report_only = v;
    }
    if let Some(v) = options.sound_pedantic {
        resolved.checker.sound_pedantic = v;
    }

    // tsc 6.0 defaults: strict-family options are true when not explicitly set.
    // The tsc cache was generated with tsc 6.0-dev which has strict=true as its
    // effective default. CheckerOptions::default() already reflects this
    // (strict=true, all sub-flags=true). No override needed here.

    // Individual strict-family options (override strict if set explicitly)
    if let Some(v) = options.no_implicit_any {
        resolved.checker.no_implicit_any = v;
    }
    if let Some(v) = options.no_implicit_returns {
        resolved.checker.no_implicit_returns = v;
    }
    if let Some(v) = options.strict_null_checks {
        resolved.checker.strict_null_checks = v;
    }
    if let Some(v) = options.strict_function_types {
        resolved.checker.strict_function_types = v;
    }
    if let Some(v) = options.strict_property_initialization {
        resolved.checker.strict_property_initialization = v;
    }
    if let Some(v) = options.no_unchecked_indexed_access {
        resolved.checker.no_unchecked_indexed_access = v;
    }
    if let Some(v) = options.exact_optional_property_types {
        resolved.checker.exact_optional_property_types = v;
    }
    if let Some(v) = options.no_property_access_from_index_signature {
        resolved.checker.no_property_access_from_index_signature = v;
    }
    if let Some(v) = options.no_implicit_this {
        resolved.checker.no_implicit_this = v;
    }
    if let Some(v) = options.use_unknown_in_catch_variables {
        resolved.checker.use_unknown_in_catch_variables = v;
    }
    if let Some(v) = options.strict_bind_call_apply {
        resolved.checker.strict_bind_call_apply = v;
    }
    if let Some(v) = options.no_implicit_override {
        resolved.checker.no_implicit_override = v;
    }
    if let Some(v) = options.no_unchecked_side_effect_imports {
        resolved.checker.no_unchecked_side_effect_imports = v;
    }
    if let Some(v) = options.strict_builtin_iterator_return {
        resolved.checker.strict_builtin_iterator_return = v;
    } else if options
        .invalidated_options
        .iter()
        .any(|key| key == "strictBuiltinIteratorReturn")
        && let Some(strict) = options.strict
    {
        // tsc reports TS5024 for an invalid explicitly-provided
        // strictBuiltinIteratorReturn value, but the invalid sub-option does
        // not block the strict umbrella from selecting the effective value.
        resolved.checker.strict_builtin_iterator_return = strict;
    }

    if let Some(no_emit) = options.no_emit {
        resolved.no_emit = no_emit;
    }
    if let Some(emit_bom) = options.emit_bom {
        resolved.emit_bom = emit_bom;
    }
    if let Some(no_check) = options.no_check {
        resolved.no_check = no_check;
    }
    if let Some(no_resolve) = options.no_resolve {
        resolved.no_resolve = no_resolve;
        resolved.checker.no_resolve = no_resolve;
    }
    if let Some(preserve_symlinks) = options.preserve_symlinks {
        resolved.preserve_symlinks = preserve_symlinks;
    }

    if let Some(no_emit_on_error) = options.no_emit_on_error {
        resolved.no_emit_on_error = no_emit_on_error;
    }

    if let Some(isolated_modules) = options.isolated_modules {
        resolved.checker.isolated_modules = isolated_modules;
    }

    // verbatimModuleSyntax implies isolatedModules in tsc — const enums get
    // runtime bindings and are subject to TDZ checks.
    if options.verbatim_module_syntax == Some(true) {
        resolved.checker.isolated_modules = true;
        resolved.checker.verbatim_module_syntax = true;
    }

    if let Some(always_strict) = options.always_strict {
        resolved.checker.always_strict = always_strict;
        resolved.printer.always_strict = always_strict;
    }

    if let Some(use_define_for_class_fields) = options.use_define_for_class_fields {
        resolved.printer.use_define_for_class_fields = use_define_for_class_fields;
    }

    if let Some(no_unused_locals) = options.no_unused_locals {
        resolved.checker.no_unused_locals = no_unused_locals;
    }

    if let Some(no_unused_parameters) = options.no_unused_parameters {
        resolved.checker.no_unused_parameters = no_unused_parameters;
    }

    if let Some(allow_unreachable_code) = options.allow_unreachable_code {
        resolved.checker.allow_unreachable_code = Some(allow_unreachable_code);
    }

    if let Some(allow_unused_labels) = options.allow_unused_labels {
        resolved.checker.allow_unused_labels = Some(allow_unused_labels);
    }

    if let Some(ref id) = options.ignore_deprecations
        && (id == "5.0" || id == "6.0")
    {
        resolved.checker.ignore_deprecations = true;
    }

    if let Some(allow_umd) = options.allow_umd_global_access {
        resolved.checker.allow_umd_global_access = allow_umd;
    }

    if let Some(preserve) = options.preserve_const_enums {
        resolved.checker.preserve_const_enums = preserve;
        resolved.printer.preserve_const_enums = preserve;
    }

    if let Some(erasable) = options.erasable_syntax_only {
        resolved.checker.erasable_syntax_only = erasable;
    }

    if let Some(no_fallthrough) = options.no_fallthrough_cases_in_switch {
        resolved.checker.no_fallthrough_cases_in_switch = no_fallthrough;
    }

    if let Some(ref custom_conditions) = options.custom_conditions {
        resolved.custom_conditions = custom_conditions.clone();
    }

    let esmodule_invalidated = options
        .invalidated_options
        .iter()
        .any(|k| k == "esModuleInterop");
    if let Some(es_module_interop) = options.es_module_interop
        && !esmodule_invalidated
    {
        resolved.es_module_interop = es_module_interop;
        resolved.checker.es_module_interop = es_module_interop;
        resolved.printer.es_module_interop = es_module_interop;
        // esModuleInterop implies allowSyntheticDefaultImports
        if es_module_interop {
            resolved.allow_synthetic_default_imports = true;
            resolved.checker.allow_synthetic_default_imports = true;
        }
    } else if !esmodule_invalidated {
        // tsc 6.0 defaults esModuleInterop to true when not explicitly set.
        // But do NOT apply the default when TS5024 fired for this option —
        // tsc treats a type-mismatched value as if the option was never set
        // (no default, stays false).
        resolved.es_module_interop = true;
        resolved.checker.es_module_interop = true;
        resolved.printer.es_module_interop = true;
        resolved.allow_synthetic_default_imports = true;
        resolved.checker.allow_synthetic_default_imports = true;
    }

    if let Some(allow_synthetic_default_imports) = options.allow_synthetic_default_imports {
        resolved.allow_synthetic_default_imports = allow_synthetic_default_imports;
        resolved.checker.allow_synthetic_default_imports = allow_synthetic_default_imports;
    } else if !resolved.allow_synthetic_default_imports {
        // TSC defaults allowSyntheticDefaultImports to true when:
        // - esModuleInterop is true (already handled above)
        // - module is "system"
        // - moduleResolution is "bundler"
        // Otherwise defaults to false.
        let should_default_true = matches!(resolved.checker.module, ModuleKind::System)
            || matches!(
                resolved.module_resolution,
                Some(ModuleResolutionKind::Bundler)
            );
        if should_default_true {
            resolved.allow_synthetic_default_imports = true;
            resolved.checker.allow_synthetic_default_imports = true;
        }
    }

    if let Some(experimental_decorators) = options.experimental_decorators {
        resolved.checker.experimental_decorators = experimental_decorators;
        resolved.printer.legacy_decorators = experimental_decorators;
    }

    if let Some(emit_decorator_metadata) = options.emit_decorator_metadata {
        resolved.printer.emit_decorator_metadata = emit_decorator_metadata;
    }

    if let Some(allow_js) = options.allow_js {
        resolved.allow_js = allow_js;
        resolved.checker.allow_js = allow_js;
    }

    if let Some(max_depth) = options.max_node_module_js_depth {
        resolved.max_node_module_js_depth = max_depth;
    }

    if let Some(check_js) = options.check_js {
        resolved.check_js = check_js;
        resolved.checker.check_js = check_js;
        if check_js && options.allow_js.is_none() {
            resolved.allow_js = true;
            resolved.checker.allow_js = true;
        }
        if !check_js {
            // Record that `checkJs: false` was explicit, not just the default.
            // This suppresses even the `plainJSErrors` allowlist (TS2451, etc.).
            resolved.explicit_check_js_false = true;
        }
    }
    if let Some(skip_lib_check) = options.skip_lib_check {
        resolved.skip_lib_check = skip_lib_check;
    }
    if let Some(skip_default_lib_check) = options.skip_default_lib_check {
        resolved.skip_default_lib_check = skip_default_lib_check;
    }
    if let Some(isolated_declarations) = options.isolated_declarations {
        resolved.isolated_declarations = isolated_declarations;
        resolved.checker.isolated_declarations = isolated_declarations;
    }
    if let Some(strip_internal) = options.strip_internal {
        resolved.strip_internal = strip_internal;
    }

    // Implement tsc's getEmitModuleDetectionKind:
    // - If moduleDetection is explicitly "force", all non-declaration files are modules.
    // - If moduleDetection is explicitly "auto" or "legacy", use their respective rules.
    // - If moduleDetection is NOT set and module is Node16-NodeNext, default to "force".
    // - If moduleDetection is NOT set and module is anything else, default to "auto".
    if let Some(ref module_detection) = options.module_detection {
        if module_detection.eq_ignore_ascii_case("force") {
            resolved.printer.module_detection_force = true;
        } else if module_detection.eq_ignore_ascii_case("legacy") {
            resolved.printer.module_detection_legacy = true;
        }
        // "auto" leaves both detection flags as false
    } else if resolved.printer.module.is_node_module() {
        // tsc defaults to Force for Node16/Node18/Node20/NodeNext
        resolved.printer.module_detection_force = true;
    }

    Ok(resolved)
}

pub fn parse_tsconfig(source: &str) -> Result<TsConfig> {
    let normalized = normalize_jsonc(source);
    let config = serde_json::from_str(&normalized).context("failed to parse tsconfig JSON")?;
    Ok(config)
}

/// Result of parsing a tsconfig.json with diagnostic collection.
pub struct ParsedTsConfig {
    pub config: TsConfig,
    pub diagnostics: Vec<Diagnostic>,
    /// Captured from removed option `suppressExcessPropertyErrors` before stripping.
    /// tsc still honors its effect even after removal (TS5102).
    pub suppress_excess_property_errors: bool,
    /// Captured from removed option `suppressImplicitAnyIndexErrors` before stripping.
    /// tsc still honors its effect even after removal (TS5102).
    pub suppress_implicit_any_index_errors: bool,
    /// Captured from removed option `noImplicitUseStrict` before stripping.
    /// tsc still honors its effect even after removal (TS5102): when true,
    /// `alwaysStrict` does not enforce strict-mode checking rules.
    pub no_implicit_use_strict: bool,
}

/// Parse tsconfig.json source and collect diagnostics for unknown compiler options.
///
/// Unlike `parse_tsconfig`, this function:
/// 1. Detects unknown/miscased compiler option keys in the JSON
/// 2. Normalizes them to canonical casing so serde can deserialize them
/// 3. Returns TS5025 diagnostics for any miscased or unknown options
pub fn parse_tsconfig_with_diagnostics(source: &str, file_path: &str) -> Result<ParsedTsConfig> {
    let stripped = strip_jsonc(source);
    let normalized = remove_trailing_commas(&stripped);
    let mut raw: serde_json::Value =
        serde_json::from_str(&normalized).context("failed to parse tsconfig JSON")?;

    let mut diagnostics = Vec::new();
    let mut suppress_excess = false;
    let mut suppress_any_index = false;
    let mut no_implicit_use_strict = false;

    // Track options that had TS5024 type errors — defaults should not be applied for these.
    let mut ts5024_keys_outer: Vec<String> = Vec::new();

    // Check compiler options for unknown/miscased keys
    if let Some(obj) = raw.as_object_mut()
        && let Some(serde_json::Value::Object(compiler_opts)) = obj.get_mut("compilerOptions")
    {
        let keys: Vec<String> = compiler_opts.keys().cloned().collect();
        let mut renames: Vec<(String, String)> = Vec::new();
        let mut unknown_keys: Vec<String> = Vec::new();

        for key in &keys {
            let key_lower = key.to_lowercase();
            if let Some(canonical) = known_compiler_option(&key_lower) {
                if key.as_str() != canonical {
                    // Miscased option — emit TS5025 and schedule rename
                    let start = find_key_offset_in_source(&stripped, key);
                    let msg = format_message(
                        diagnostic_messages::UNKNOWN_COMPILER_OPTION_DID_YOU_MEAN,
                        &[key, canonical],
                    );
                    diagnostics.push(Diagnostic::error(
                        file_path,
                        start,
                        key.len() as u32 + 2, // include quotes
                        msg,
                        diagnostic_codes::UNKNOWN_COMPILER_OPTION_DID_YOU_MEAN,
                    ));
                    renames.push((key.clone(), canonical.to_string()));
                }
                // else: exact match, no diagnostic needed
            } else {
                // Truly unknown option — emit TS5023
                let start = find_key_offset_in_source(&stripped, key);
                if let Some(suggestion) = unknown_compiler_option_suggestion(&key_lower) {
                    let msg = format_message(
                        diagnostic_messages::UNKNOWN_COMPILER_OPTION_DID_YOU_MEAN,
                        &[key, suggestion],
                    );
                    diagnostics.push(Diagnostic::error(
                        file_path,
                        start,
                        key.len() as u32 + 2,
                        msg,
                        diagnostic_codes::UNKNOWN_COMPILER_OPTION_DID_YOU_MEAN,
                    ));
                } else {
                    let msg = format_message(diagnostic_messages::UNKNOWN_COMPILER_OPTION, &[key]);
                    diagnostics.push(Diagnostic::error(
                        file_path,
                        start,
                        key.len() as u32 + 2,
                        msg,
                        diagnostic_codes::UNKNOWN_COMPILER_OPTION,
                    ));
                }
                unknown_keys.push(key.clone());
            }
        }

        // Remove unknown keys before serde deserialization so tsz-only struct
        // fields cannot take effect from a tsc-incompatible tsconfig option.
        for key in unknown_keys {
            compiler_opts.remove(&key);
        }

        // Rename miscased keys to canonical casing so serde can deserialize them
        for (old_key, new_key) in renames {
            if let Some(value) = compiler_opts.remove(&old_key) {
                compiler_opts.insert(new_key, value);
            }
        }

        // Check for command-line-only options (TS6266)
        // These options are only valid when passed via the CLI, not in tsconfig.json.
        let cli_only_options: &[&str] = &["listFilesOnly"];
        let mut cli_only_keys: Vec<String> = Vec::new();
        for key in compiler_opts.keys().cloned().collect::<Vec<_>>() {
            if cli_only_options.contains(&key.as_str()) {
                let start = find_key_offset_in_source(&stripped, &key);
                let msg = format_message(
                    diagnostic_messages::OPTION_CAN_ONLY_BE_SPECIFIED_ON_COMMAND_LINE,
                    &[&key],
                );
                diagnostics.push(Diagnostic::error(
                    file_path,
                    start,
                    key.len() as u32 + 2,
                    msg,
                    diagnostic_codes::OPTION_CAN_ONLY_BE_SPECIFIED_ON_COMMAND_LINE,
                ));
                cli_only_keys.push(key);
            }
        }
        for key in &cli_only_keys {
            compiler_opts.remove(key);
        }

        // Check for removed compiler options (TS5102)
        // These options were deprecated in TS 5.0 and removed in TS 5.5.
        // In tsc 6.0, `mustBeRemoved` is always true (removedIn 5.5 <= tsc 6.0),
        // so TS5102 fires unconditionally — ignoreDeprecations cannot suppress it.
        // ignoreDeprecations only suppresses TS5101 (deprecated but not yet removed).
        let mut removed_keys: Vec<String> = Vec::new();
        for key in compiler_opts.keys().cloned().collect::<Vec<_>>() {
            if removed_compiler_option(&key).is_some() {
                let value = compiler_opts.get(&key);
                // Only emit TS5102 if the option is actually set (non-null, non-default)
                let is_set = match value {
                    Some(serde_json::Value::Bool(b)) => *b,
                    Some(serde_json::Value::String(s)) => !s.is_empty(),
                    Some(serde_json::Value::Null) | None => false,
                    Some(_) => true,
                };
                if is_set {
                    let start = find_key_offset_in_source(&stripped, &key);
                    let msg = format_message(
                        diagnostic_messages::OPTION_HAS_BEEN_REMOVED_PLEASE_REMOVE_IT_FROM_YOUR_CONFIGURATION,
                        &[&key],
                    );
                    diagnostics.push(Diagnostic::error(
                        file_path,
                        start,
                        key.len() as u32 + 2, // include quotes
                        msg,
                        diagnostic_codes::OPTION_HAS_BEEN_REMOVED_PLEASE_REMOVE_IT_FROM_YOUR_CONFIGURATION,
                    ));
                }
                removed_keys.push(key);
            }
        }
        // Capture removed-but-still-honored suppress flags before stripping.
        // tsc still honors these even after removal (TS5102 is emitted but suppression stays).
        suppress_excess = matches!(
            compiler_opts.get("suppressExcessPropertyErrors"),
            Some(serde_json::Value::Bool(true))
        );
        suppress_any_index = matches!(
            compiler_opts.get("suppressImplicitAnyIndexErrors"),
            Some(serde_json::Value::Bool(true))
        );
        // noImplicitUseStrict: when true, alwaysStrict does not enforce strict-mode
        // checking rules (e.g. TS1100). tsc still honors this even though the option
        // was removed in TS 5.5 (TS5102 is emitted but the semantic effect is kept).
        no_implicit_use_strict = matches!(
            compiler_opts.get("noImplicitUseStrict"),
            Some(serde_json::Value::Bool(true))
        );

        // Strip removed options so they don't reach serde or subsequent validation
        for key in &removed_keys {
            compiler_opts.remove(key);
        }

        // Check compiler option value types (TS5024)
        // Collect keys that have type mismatches so we can remove them after iteration.
        // Also track all keys that emitted TS5024 to suppress TS5101 for the same key
        // (tsc does not emit a deprecation warning for an option that also has a type error).
        let keys_after_rename: Vec<String> = compiler_opts.keys().cloned().collect();
        let mut bad_keys: Vec<String> = Vec::new();
        let mut ts5024_keys: Vec<String> = Vec::new();
        for key in &keys_after_rename {
            let expected_type = compiler_option_expected_type(key);
            if expected_type.is_empty() {
                continue; // Unknown option or no type constraint
            }
            let Some(value) = compiler_opts.get(key) else {
                continue;
            };
            let type_ok = match expected_type {
                "boolean" => value.is_boolean(),
                "string" => value.is_string(),
                "number" => value.is_number(),
                "Array" => value.is_array(),
                "string or Array" => value.is_string() || value.is_array(),
                "object" => value.is_object(),
                _ => true,
            };
            if !type_ok {
                let start = find_value_offset_in_source(&stripped, key);
                let value_len = estimate_json_value_len(value);
                let msg = format_message(
                    diagnostic_messages::COMPILER_OPTION_REQUIRES_A_VALUE_OF_TYPE,
                    &[key, expected_type],
                );
                diagnostics.push(Diagnostic::error(
                    file_path,
                    start,
                    value_len,
                    msg,
                    diagnostic_codes::COMPILER_OPTION_REQUIRES_A_VALUE_OF_TYPE,
                ));
                // Track all TS5024 keys so we can suppress TS5101 for the same key.
                ts5024_keys.push(key.clone());
                // tsc emits TS5024 and does NOT apply the value (convertJsonOption
                // returns undefined for type mismatches), so remove invalidly-typed
                // values from the config object before deserialization.
                bad_keys.push(key.clone());
            }
        }
        // Remove invalid values so serde defaults them to None
        for key in &bad_keys {
            compiler_opts.remove(key);
        }

        // Check ignoreDeprecations value (TS5103)
        // tsc 6.0 accepts both "5.0" and "6.0" as valid ignoreDeprecations values.
        // See TypeScript/src/compiler/program.ts getIgnoreDeprecationsVersion():
        //   "5.0" silences 5.0-wave deprecation warnings (now removals → TS5102).
        //   "6.0" silences 6.0-wave deprecation warnings (TS5107).
        if let Some(serde_json::Value::String(id_value)) = compiler_opts.get("ignoreDeprecations")
            && id_value != "5.0"
            && id_value != "6.0"
        {
            let start = find_value_offset_in_source(&stripped, "ignoreDeprecations");
            let value_len = id_value.len() as u32 + 2; // include quotes
            diagnostics.push(Diagnostic::error(
                file_path,
                start,
                value_len,
                diagnostic_messages::INVALID_VALUE_FOR_IGNOREDEPRECATIONS.to_string(),
                diagnostic_codes::INVALID_VALUE_FOR_IGNOREDEPRECATIONS,
            ));
        }

        // Check 6.0-wave deprecated compiler options (TS5107 / TS5101)
        // These options were deprecated in TS 6.0 and will be removed in TS 7.0.
        // Suppressed when ignoreDeprecations >= "6.0".
        let ignore_deprecations_silences_6_0 = matches!(
            compiler_opts.get("ignoreDeprecations"),
            Some(serde_json::Value::String(v)) if v == "6.0"
        );
        if !ignore_deprecations_silences_6_0 {
            // Value-based deprecations (TS5107): "Option '{0}={1}' is deprecated..."
            type DeprecationCheck = (
                &'static str,
                &'static dyn Fn(&serde_json::Value) -> Option<&'static str>,
            );
            let value_deprecations: &[DeprecationCheck] = &[
                ("alwaysStrict", &|v| {
                    if v == &serde_json::Value::Bool(false) {
                        Some("false")
                    } else {
                        None
                    }
                }),
                ("target", &|v| match v {
                    serde_json::Value::String(s) => {
                        let n = normalize_option(s);
                        if n == "es5" { Some("ES5") } else { None }
                    }
                    _ => None,
                }),
                ("moduleResolution", &|v| match v {
                    serde_json::Value::String(s) => {
                        let n = normalize_option(s);
                        if n == "node10" || n == "node" {
                            Some("node10")
                        } else if n == "classic" {
                            Some("classic")
                        } else {
                            None
                        }
                    }
                    _ => None,
                }),
                ("esModuleInterop", &|v| {
                    if v == &serde_json::Value::Bool(false) {
                        Some("false")
                    } else {
                        None
                    }
                }),
                ("allowSyntheticDefaultImports", &|v| {
                    if v == &serde_json::Value::Bool(false) {
                        Some("false")
                    } else {
                        None
                    }
                }),
                ("module", &|v| match v {
                    serde_json::Value::String(s) => {
                        let n = normalize_option(s);
                        match n.as_str() {
                            "none" => Some("None"),
                            "amd" => Some("AMD"),
                            "umd" => Some("UMD"),
                            "system" => Some("System"),
                            _ => None,
                        }
                    }
                    _ => None,
                }),
            ];
            for (key, check_fn) in value_deprecations {
                if let Some(value) = compiler_opts.get(*key)
                    && let Some(display_value) = check_fn(value)
                {
                    let start = find_value_offset_in_source(&stripped, key);
                    let value_len = estimate_json_value_len(value);
                    let msg = format_message(
                        diagnostic_messages::OPTION_IS_DEPRECATED_AND_WILL_STOP_FUNCTIONING_IN_TYPESCRIPT_SPECIFY_COMPILEROPT_2,
                        &[key, display_value, "7.0", "6.0"],
                    );
                    diagnostics.push(Diagnostic::error(
                        file_path,
                        start,
                        value_len,
                        msg,
                        diagnostic_codes::OPTION_IS_DEPRECATED_AND_WILL_STOP_FUNCTIONING_IN_TYPESCRIPT_SPECIFY_COMPILEROPT_2,
                    ));
                }
            }

            // No-value deprecations (TS5101): "Option '{0}' is deprecated..."
            let key_deprecations = ["baseUrl", "outFile", "downlevelIteration"];
            for key in &key_deprecations {
                // Suppress TS5101 when TS5024 already fired for the same option:
                // tsc does not emit a deprecation warning for options with type errors.
                if ts5024_keys.iter().any(|k| k == key) {
                    continue;
                }
                if compiler_opts.contains_key(*key) {
                    let search = format!("\"{key}\"");
                    let compiler_opts_pos = stripped.find("compilerOptions").unwrap_or(0);
                    let start = stripped[compiler_opts_pos..]
                        .find(&search)
                        .map(|p| (compiler_opts_pos + p) as u32)
                        .unwrap_or(0);
                    let key_len = key.len() as u32 + 2; // include quotes
                    let msg = format_message(
                        diagnostic_messages::OPTION_IS_DEPRECATED_AND_WILL_STOP_FUNCTIONING_IN_TYPESCRIPT_SPECIFY_COMPILEROPT,
                        &[key, "7.0", "6.0"],
                    );
                    diagnostics.push(Diagnostic::error(
                        file_path,
                        start,
                        key_len,
                        msg,
                        diagnostic_codes::OPTION_IS_DEPRECATED_AND_WILL_STOP_FUNCTIONING_IN_TYPESCRIPT_SPECIFY_COMPILEROPT,
                    ));
                }
            }
        }

        // Check for removed compiler option values (TS5108)
        // These are specific values for otherwise-valid options that tsc 6.0 removed entirely.
        // Unlike TS5107 deprecations, TS5108 cannot be suppressed by ignoreDeprecations.
        {
            type RemovedValueCheck = (
                &'static str,
                &'static dyn Fn(&serde_json::Value) -> Option<&'static str>,
            );
            let removed_value_checks: &[RemovedValueCheck] = &[("target", &|v| match v {
                serde_json::Value::String(s) => {
                    let n = normalize_option(s);
                    if n == "es3" { Some("ES3") } else { None }
                }
                _ => None,
            })];
            for (key, check_fn) in removed_value_checks {
                let matched = compiler_opts
                    .get(*key)
                    .and_then(|v| check_fn(v).map(|dv| (dv, estimate_json_value_len(v))));
                if let Some((display_value, value_len)) = matched {
                    let start = find_value_offset_in_source(&stripped, key);
                    let msg = format_message(
                        diagnostic_messages::OPTION_HAS_BEEN_REMOVED_PLEASE_REMOVE_IT_FROM_YOUR_CONFIGURATION_2,
                        &[key, display_value],
                    );
                    diagnostics.push(Diagnostic::error(
                        file_path,
                        start,
                        value_len,
                        msg,
                        diagnostic_codes::OPTION_HAS_BEEN_REMOVED_PLEASE_REMOVE_IT_FROM_YOUR_CONFIGURATION_2,
                    ));
                    // Null out so validate_option_value and resolve_compiler_options skip it.
                    compiler_opts.insert(key.to_string(), serde_json::Value::Null);
                }
            }
        }

        // Check command-line-only options in tsconfig (TS6266)
        // Some options like `listFilesOnly` can only be specified on the command line,
        // not in tsconfig.json. tsc emits TS6266 for these.
        let command_line_only_options = ["listFilesOnly"];
        for key in &command_line_only_options {
            if compiler_opts.contains_key(*key) {
                let start = find_key_offset_in_source(&stripped, key);
                let key_len = key.len() as u32 + 2; // include quotes
                let msg = format_message(
                    diagnostic_messages::OPTION_CAN_ONLY_BE_SPECIFIED_ON_COMMAND_LINE,
                    &[key],
                );
                diagnostics.push(Diagnostic::error(
                    file_path,
                    start,
                    key_len,
                    msg,
                    diagnostic_codes::OPTION_CAN_ONLY_BE_SPECIFIED_ON_COMMAND_LINE,
                ));
                // Remove the option so it doesn't affect compilation
                compiler_opts.remove(*key);
            }
        }

        // Check moduleResolution/module compatibility (TS5095)
        // `moduleResolution: "bundler"` requires `module` to be "preserve" or ES2015+.
        if let Some(serde_json::Value::String(mr_value)) = compiler_opts.get("moduleResolution") {
            let mr_normalized =
                normalize_enum_option_value(mr_value.split(',').next().unwrap_or(mr_value));
            if mr_normalized == "bundler" {
                let module_ok = if let Some(serde_json::Value::String(mod_value)) =
                    compiler_opts.get("module")
                {
                    let mod_normalized = normalize_enum_option_value(
                        mod_value.split(',').next().unwrap_or(mod_value),
                    );
                    // tsc message: "can only be used when 'module' is set to 'preserve',
                    // 'commonjs', or 'es2015' or later" — commonjs IS valid.
                    // AMD, UMD, System, None are the invalid values.
                    matches!(
                        mod_normalized.as_str(),
                        "preserve"
                            | "commonjs"
                            | "es2015"
                            | "es6"
                            | "es2020"
                            | "es2022"
                            | "esnext"
                            | "node16"
                            | "node18"
                            | "node20"
                            | "nodenext"
                    )
                } else {
                    // module not set — default depends on target.
                    // ES2015+ targets default to es2015 (compatible), lower targets
                    // default to commonjs which is also compatible with bundler in tsc 6.0.
                    true
                };
                if !module_ok {
                    let start = find_value_offset_in_source(&stripped, "moduleResolution");
                    let value_len = mr_value.len() as u32 + 2; // include quotes
                    let msg = "Option 'bundler' can only be used when 'module' is set to 'preserve', 'commonjs', or 'es2015' or later.".to_string();
                    diagnostics.push(Diagnostic::error(
                        file_path,
                        start,
                        value_len,
                        msg,
                        diagnostic_codes::OPTION_CAN_ONLY_BE_USED_WHEN_MODULE_IS_SET_TO_PRESERVE_COMMONJS_OR_ES2015_OR_LAT,
                    ));
                }
            }
        }

        // Check moduleResolution/module compatibility (TS5110)
        // When moduleResolution is node16/nodenext, module must also be node16/nodenext.
        if let Some(serde_json::Value::String(mr_value)) = compiler_opts.get("moduleResolution") {
            let mr_normalized =
                normalize_enum_option_value(mr_value.split(',').next().unwrap_or(mr_value));
            let is_node_mr = matches!(
                mr_normalized.as_str(),
                "node16" | "node18" | "node20" | "nodenext"
            );
            if is_node_mr {
                let module_ok = if let Some(serde_json::Value::String(mod_value)) =
                    compiler_opts.get("module")
                {
                    let mod_normalized = normalize_enum_option_value(
                        mod_value.split(',').next().unwrap_or(mod_value),
                    );
                    matches!(
                        mod_normalized.as_str(),
                        "node16" | "node18" | "node20" | "nodenext"
                    )
                } else {
                    false // module not explicitly set → tsc requires it to be set explicitly
                };
                if !module_ok {
                    // When module is explicitly set to a wrong value, point at
                    // its value; when module is not set at all, point at
                    // "compilerOptions" key (matching tsc behavior).
                    let (start, value_len) = if compiler_opts.contains_key("module") {
                        let s = find_value_offset_in_source(&stripped, "module");
                        let vl = compiler_opts
                            .get("module")
                            .and_then(|v| v.as_str())
                            .map_or(0, |sv| sv.len() as u32 + 2);
                        (s, vl)
                    } else {
                        // Point at "compilerOptions" key — search from start
                        let search = "\"compilerOptions\"";
                        let s = stripped.find(search).map_or(0, |p| p as u32);
                        let vl = search.len() as u32;
                        (s, vl)
                    };
                    // tsc uses PascalCase for the option values in the message
                    let mr_display = match mr_normalized.as_str() {
                        "node16" => "Node16",
                        "node18" => "Node18",
                        "node20" => "Node20",
                        "nodenext" => "NodeNext",
                        _ => &mr_normalized,
                    };
                    let msg = format_message(
                        diagnostic_messages::OPTION_MODULE_MUST_BE_SET_TO_WHEN_OPTION_MODULERESOLUTION_IS_SET_TO,
                        &[mr_display, mr_display],
                    );
                    diagnostics.push(Diagnostic::error(
                        file_path,
                        start,
                        value_len,
                        msg,
                        diagnostic_codes::OPTION_MODULE_MUST_BE_SET_TO_WHEN_OPTION_MODULERESOLUTION_IS_SET_TO,
                    ));
                }
            }
        }

        // TS5109: moduleResolution must match module for node16/nodenext modules.
        // When module is node16/nodenext, moduleResolution must be the same (or left unspecified).
        if let Some(serde_json::Value::String(mod_value)) = compiler_opts.get("module") {
            let mod_normalized =
                normalize_enum_option_value(mod_value.split(',').next().unwrap_or(mod_value));
            let is_node_module = matches!(
                mod_normalized.as_str(),
                "node16" | "node18" | "node20" | "nodenext"
            );
            if is_node_module
                && let Some(serde_json::Value::String(mr_value)) =
                    compiler_opts.get("moduleResolution")
            {
                let mr_normalized =
                    normalize_enum_option_value(mr_value.split(',').next().unwrap_or(mr_value));
                let mr_ok = matches!(
                    mr_normalized.as_str(),
                    "node16" | "node18" | "node20" | "nodenext"
                );
                if !mr_ok {
                    let start = find_value_offset_in_source(&stripped, "moduleResolution");
                    let value_len = mr_value.len() as u32 + 2;
                    let mod_display = match mod_normalized.as_str() {
                        "node16" => "Node16",
                        "node18" => "Node18",
                        "node20" => "Node20",
                        "nodenext" => "NodeNext",
                        _ => &mod_normalized,
                    };
                    let msg = format_message(
                        diagnostic_messages::OPTION_MODULERESOLUTION_MUST_BE_SET_TO_OR_LEFT_UNSPECIFIED_WHEN_OPTION_MODULE_IS,
                        &[mod_display, mod_display],
                    );
                    diagnostics.push(Diagnostic::error(
                        file_path,
                        start,
                        value_len,
                        msg,
                        diagnostic_codes::OPTION_MODULERESOLUTION_MUST_BE_SET_TO_OR_LEFT_UNSPECIFIED_WHEN_OPTION_MODULE_IS,
                    ));
                }
            }
        }

        // TS5095: moduleResolution: bundler can only be used when module is
        // preserve, commonjs, or es2015+.
        if let Some(serde_json::Value::String(mr_value)) = compiler_opts.get("moduleResolution") {
            let mr_normalized =
                normalize_enum_option_value(mr_value.split(',').next().unwrap_or(mr_value));
            if mr_normalized == "bundler" {
                let module_ok = if let Some(serde_json::Value::String(mod_value)) =
                    compiler_opts.get("module")
                {
                    let mod_normalized = normalize_enum_option_value(
                        mod_value.split(',').next().unwrap_or(mod_value),
                    );
                    // bundler is incompatible with node16/nodenext module kinds
                    !matches!(
                        mod_normalized.as_str(),
                        "node16" | "node18" | "node20" | "nodenext"
                    )
                } else {
                    true // module not set → tsc defaults it, which is valid
                };
                if !module_ok {
                    let start = find_value_offset_in_source(&stripped, "moduleResolution");
                    let value_len = mr_value.len() as u32 + 2;
                    let msg = format_message(
                        diagnostic_messages::OPTION_CAN_ONLY_BE_USED_WHEN_MODULE_IS_SET_TO_PRESERVE_COMMONJS_OR_ES2015_OR_LAT,
                        &["bundler"],
                    );
                    diagnostics.push(Diagnostic::error(
                        file_path,
                        start,
                        value_len,
                        msg,
                        diagnostic_codes::OPTION_CAN_ONLY_BE_USED_WHEN_MODULE_IS_SET_TO_PRESERVE_COMMONJS_OR_ES2015_OR_LAT,
                    ));
                }
            }
        }

        // TS6082: Only 'amd' and 'system' modules are supported alongside --outFile.
        // When outFile is set with a non-amd/system module, emit at both the module and outFile keys.
        if let Some(serde_json::Value::String(out_file_value)) = compiler_opts.get("outFile")
            && !out_file_value.is_empty()
            && !option_is_effectively_enabled(compiler_opts, &ts5024_keys, "emitDeclarationOnly")
            && let Some(serde_json::Value::String(mod_value)) = compiler_opts.get("module")
        {
            let mod_normalized =
                normalize_enum_option_value(mod_value.split(',').next().unwrap_or(mod_value));
            // `module=none` means no module system; tsc does not report TS6082 for it.
            if !matches!(mod_normalized.as_str(), "amd" | "system" | "none") {
                let msg = format_message(
                    diagnostic_messages::ONLY_AMD_AND_SYSTEM_MODULES_ARE_SUPPORTED_ALONGSIDE,
                    &["outFile"],
                );
                // Emit at the "module" key (matching tsc behavior)
                let start_module = find_key_offset_in_source(&stripped, "module");
                let module_key_len = "module".len() as u32 + 2; // include quotes
                diagnostics.push(Diagnostic::error(
                    file_path,
                    start_module,
                    module_key_len,
                    msg.clone(),
                    diagnostic_codes::ONLY_AMD_AND_SYSTEM_MODULES_ARE_SUPPORTED_ALONGSIDE,
                ));
                // Emit at the "outFile" key (matching tsc behavior)
                let start_outfile = find_key_offset_in_source(&stripped, "outFile");
                let outfile_key_len = "outFile".len() as u32 + 2;
                diagnostics.push(Diagnostic::error(
                    file_path,
                    start_outfile,
                    outfile_key_len,
                    msg,
                    diagnostic_codes::ONLY_AMD_AND_SYSTEM_MODULES_ARE_SUPPORTED_ALONGSIDE,
                ));
            }
        }

        // TS5105: Option 'verbatimModuleSyntax' cannot be used when 'module' is set to 'UMD', 'AMD', or 'System'.
        if option_is_effectively_enabled(compiler_opts, &ts5024_keys, "verbatimModuleSyntax") {
            let module_bad = if let Some(serde_json::Value::String(mod_value)) =
                compiler_opts.get("module")
            {
                let mod_normalized =
                    normalize_enum_option_value(mod_value.split(',').next().unwrap_or(mod_value));
                matches!(mod_normalized.as_str(), "umd" | "amd" | "system")
            } else {
                false
            };
            if module_bad {
                let start = find_key_offset_in_source(&stripped, "verbatimModuleSyntax");
                let key_len = "verbatimModuleSyntax".len() as u32 + 2;
                diagnostics.push(Diagnostic::error(
                    file_path,
                    start,
                    key_len,
                    diagnostic_messages::OPTION_VERBATIMMODULESYNTAX_CANNOT_BE_USED_WHEN_MODULE_IS_SET_TO_UMD_AMD_OR_SYST.to_string(),
                    diagnostic_codes::OPTION_VERBATIMMODULESYNTAX_CANNOT_BE_USED_WHEN_MODULE_IS_SET_TO_UMD_AMD_OR_SYST,
                ));
            }
        }

        // TS5069: Option '{0}' cannot be specified without specifying option '{1}' or option '{2}'.
        // Group 1: options that require 'declaration' or 'composite'
        let requires_decl_or_composite: &[&str] = &[
            "emitDeclarationOnly",
            "declarationMap",
            "isolatedDeclarations",
        ];
        for &opt in requires_decl_or_composite {
            let declaration_enabled =
                option_is_effectively_enabled(compiler_opts, &ts5024_keys, "declaration");
            let composite_enabled =
                option_is_effectively_enabled(compiler_opts, &ts5024_keys, "composite");
            if option_is_truthy(compiler_opts.get(opt))
                && !declaration_enabled
                && !composite_enabled
            {
                let msg = format_message(
                    diagnostic_messages::OPTION_CANNOT_BE_SPECIFIED_WITHOUT_SPECIFYING_OPTION_OR_OPTION,
                    &[opt, "declaration", "composite"],
                );
                let code =
                    diagnostic_codes::OPTION_CANNOT_BE_SPECIFIED_WITHOUT_SPECIFYING_OPTION_OR_OPTION;
                let mut related_keys = vec![opt];
                if option_key_present_or_invalidated(compiler_opts, &ts5024_keys, "declaration") {
                    related_keys.push("declaration");
                }
                if option_key_present_or_invalidated(compiler_opts, &ts5024_keys, "composite") {
                    related_keys.push("composite");
                }
                for key in related_keys {
                    let start = find_key_offset_in_source(&stripped, key);
                    let key_len = key.len() as u32 + 2; // include quotes
                    diagnostics.push(Diagnostic::error(
                        file_path,
                        start,
                        key_len,
                        msg.clone(),
                        code,
                    ));
                }
            }
        }

        // TS5096: allowImportingTsExtensions is only valid in no-emit modes
        // or when imports are rewritten before emit.
        if option_is_effectively_enabled(compiler_opts, &ts5024_keys, "allowImportingTsExtensions")
            && !option_is_effectively_enabled(compiler_opts, &ts5024_keys, "noEmit")
            && !option_is_effectively_enabled(compiler_opts, &ts5024_keys, "emitDeclarationOnly")
            && !option_is_effectively_enabled(
                compiler_opts,
                &ts5024_keys,
                "rewriteRelativeImportExtensions",
            )
        {
            let start = find_value_offset_in_source(&stripped, "allowImportingTsExtensions");
            let value_len = compiler_opts
                .get("allowImportingTsExtensions")
                .map_or(4, estimate_json_value_len);
            diagnostics.push(Diagnostic::error(
                file_path,
                start,
                value_len,
                diagnostic_messages::OPTION_ALLOWIMPORTINGTSEXTENSIONS_CAN_ONLY_BE_USED_WHEN_ONE_OF_NOEMIT_EMITDECLAR
                    .to_string(),
                diagnostic_codes::OPTION_ALLOWIMPORTINGTSEXTENSIONS_CAN_ONLY_BE_USED_WHEN_ONE_OF_NOEMIT_EMITDECLAR,
            ));
        }

        // Group 2: mapRoot requires 'sourceMap' or 'declarationMap'
        if compiler_opts.contains_key("mapRoot")
            && !option_is_effectively_enabled(compiler_opts, &ts5024_keys, "sourceMap")
            && !option_is_effectively_enabled(compiler_opts, &ts5024_keys, "declarationMap")
        {
            let start = find_key_offset_in_source(&stripped, "mapRoot");
            let key_len = "mapRoot".len() as u32 + 2;
            let msg = format_message(
                diagnostic_messages::OPTION_CANNOT_BE_SPECIFIED_WITHOUT_SPECIFYING_OPTION_OR_OPTION,
                &["mapRoot", "sourceMap", "declarationMap"],
            );
            diagnostics.push(Diagnostic::error(
                file_path,
                start,
                key_len,
                msg,
                diagnostic_codes::OPTION_CANNOT_BE_SPECIFIED_WITHOUT_SPECIFYING_OPTION_OR_OPTION,
            ));
        }

        // TS5091: preserveConstEnums cannot be disabled when isolatedModules is enabled.
        // tsc emits this at both key positions; we emit once per enabler.
        if matches!(
            compiler_opts.get("preserveConstEnums"),
            Some(serde_json::Value::Bool(false))
        ) {
            let enablers: &[&str] = &["isolatedModules", "isolatedDeclarations"];
            for enabler in enablers {
                if option_is_effectively_enabled(compiler_opts, &ts5024_keys, enabler) {
                    let start = find_key_offset_in_source(&stripped, "preserveConstEnums");
                    let key_len = "preserveConstEnums".len() as u32 + 2;
                    let msg = format_message(
                        diagnostic_messages::OPTION_PRESERVECONSTENUMS_CANNOT_BE_DISABLED_WHEN_IS_ENABLED,
                        &[enabler],
                    );
                    diagnostics.push(Diagnostic::error(
                        file_path,
                        start,
                        key_len,
                        msg.clone(),
                        diagnostic_codes::OPTION_PRESERVECONSTENUMS_CANNOT_BE_DISABLED_WHEN_IS_ENABLED,
                    ));
                    // tsc also emits at the enabler key position
                    let enabler_start = find_key_offset_in_source(&stripped, enabler);
                    let enabler_key_len = enabler.len() as u32 + 2;
                    diagnostics.push(Diagnostic::error(
                        file_path,
                        enabler_start,
                        enabler_key_len,
                        msg,
                        diagnostic_codes::OPTION_PRESERVECONSTENUMS_CANNOT_BE_DISABLED_WHEN_IS_ENABLED,
                    ));
                }
            }
        }

        // TS6304: Composite projects may not disable declaration emit.
        // When composite: true, declaration must not be explicitly false.
        if option_is_effectively_enabled(compiler_opts, &ts5024_keys, "composite")
            && matches!(
                compiler_opts.get("declaration"),
                Some(serde_json::Value::Bool(false))
            )
        {
            let start = find_key_offset_in_source(&stripped, "declaration");
            let key_len = "declaration".len() as u32 + 2;
            diagnostics.push(Diagnostic::error(
                file_path,
                start,
                key_len,
                diagnostic_messages::COMPOSITE_PROJECTS_MAY_NOT_DISABLE_DECLARATION_EMIT
                    .to_string(),
                diagnostic_codes::COMPOSITE_PROJECTS_MAY_NOT_DISABLE_DECLARATION_EMIT,
            ));
        }

        // TS6379: Composite projects may not disable incremental compilation.
        // When composite: true, incremental must not be explicitly false.
        // tsc anchors the error at the `compilerOptions` key itself (the
        // enclosing block that contains both interacting options), rather
        // than at `composite` or `incremental`.
        if option_is_effectively_enabled(compiler_opts, &ts5024_keys, "composite")
            && matches!(
                compiler_opts.get("incremental"),
                Some(serde_json::Value::Bool(false))
            )
        {
            let search = "\"compilerOptions\"";
            let start = stripped.find(search).map(|p| p as u32).unwrap_or(0);
            let key_len = search.len() as u32;
            diagnostics.push(Diagnostic::error(
                file_path,
                start,
                key_len,
                diagnostic_messages::COMPOSITE_PROJECTS_MAY_NOT_DISABLE_INCREMENTAL_COMPILATION
                    .to_string(),
                diagnostic_codes::COMPOSITE_PROJECTS_MAY_NOT_DISABLE_INCREMENTAL_COMPILATION,
            ));
        }

        // TS5052: Option '{0}' cannot be specified without specifying option '{1}'.
        // `checkJs` implies `allowJs` unless `allowJs` is explicitly disabled.
        if option_is_truthy(compiler_opts.get("checkJs"))
            && !option_is_effectively_enabled(compiler_opts, &ts5024_keys, "allowJs")
            && option_key_present_or_invalidated(compiler_opts, &ts5024_keys, "allowJs")
        {
            let msg = format_message(
                diagnostic_messages::OPTION_CANNOT_BE_SPECIFIED_WITHOUT_SPECIFYING_OPTION,
                &["checkJs", "allowJs"],
            );

            // Always emit at the checkJs key.
            let check_js_start = find_key_offset_in_source(&stripped, "checkJs");
            let check_js_len = "checkJs".len() as u32 + 2;
            diagnostics.push(Diagnostic::error(
                file_path,
                check_js_start,
                check_js_len,
                msg.clone(),
                diagnostic_codes::OPTION_CANNOT_BE_SPECIFIED_WITHOUT_SPECIFYING_OPTION,
            ));

            // If allowJs is explicitly present, emit at allowJs too (tsc parity).
            if compiler_opts.contains_key("allowJs") {
                let allow_js_start = find_key_offset_in_source(&stripped, "allowJs");
                let allow_js_len = "allowJs".len() as u32 + 2;
                diagnostics.push(Diagnostic::error(
                    file_path,
                    allow_js_start,
                    allow_js_len,
                    msg,
                    diagnostic_codes::OPTION_CANNOT_BE_SPECIFIED_WITHOUT_SPECIFYING_OPTION,
                ));
            }
        }

        // TS5052: emitDecoratorMetadata requires experimentalDecorators.
        if option_is_truthy(compiler_opts.get("emitDecoratorMetadata"))
            && !option_is_effectively_enabled(compiler_opts, &ts5024_keys, "experimentalDecorators")
        {
            let start = find_key_offset_in_source(&stripped, "emitDecoratorMetadata");
            let key_len = "emitDecoratorMetadata".len() as u32 + 2;
            let msg = format_message(
                diagnostic_messages::OPTION_CANNOT_BE_SPECIFIED_WITHOUT_SPECIFYING_OPTION,
                &["emitDecoratorMetadata", "experimentalDecorators"],
            );
            diagnostics.push(Diagnostic::error(
                file_path,
                start,
                key_len,
                msg,
                diagnostic_codes::OPTION_CANNOT_BE_SPECIFIED_WITHOUT_SPECIFYING_OPTION,
            ));
        }

        // TS5053: Option '{0}' cannot be specified with option '{1}'.
        // tsc emits for each conflicting key, pointing at the key's position.
        // The message always names the pair (A, B) regardless of which key is pointed at.
        let conflicting_pairs: &[(&str, &str)] = &[
            ("sourceMap", "inlineSourceMap"),
            ("mapRoot", "inlineSourceMap"),
            ("reactNamespace", "jsxFactory"),
            ("allowJs", "isolatedDeclarations"),
        ];
        // Issue #3732: tsc resolves `checkJs: true` (when `allowJs` is not
        // explicitly disabled) to an implied `allowJs: true` and still
        // emits TS5053 for the (allowJs, isolatedDeclarations) conflict.
        // Mirror that implication so the conflict pair fires even when
        // only `checkJs` is in the config.
        let allow_js_present = compiler_opts.contains_key("allowJs");
        let allow_js_implied_by_check_js =
            !allow_js_present && option_is_truthy(compiler_opts.get("checkJs"));
        let option_is_set_with_check_js_implication = |opt: &str| -> bool {
            if option_is_truthy(compiler_opts.get(opt)) {
                return true;
            }
            opt == "allowJs" && allow_js_implied_by_check_js
        };
        for &(opt_a, opt_b) in conflicting_pairs {
            if option_is_set_with_check_js_implication(opt_a)
                && option_is_set_with_check_js_implication(opt_b)
            {
                let resolve = |opt: &'static str| -> &'static str {
                    if opt == "allowJs" && allow_js_implied_by_check_js {
                        "checkJs"
                    } else {
                        opt
                    }
                };
                let key_a = resolve(opt_a);
                let key_b = resolve(opt_b);
                // Emit at the resolved-key position (issue #3732 anchors at
                // `checkJs` when allowJs is implied).
                let start = find_key_offset_in_source(&stripped, key_a);
                let key_len = key_a.len() as u32 + 2;
                let msg = format_message(
                    diagnostic_messages::OPTION_CANNOT_BE_SPECIFIED_WITH_OPTION,
                    &[opt_a, opt_b],
                );
                diagnostics.push(Diagnostic::error(
                    file_path,
                    start,
                    key_len,
                    msg.clone(),
                    diagnostic_codes::OPTION_CANNOT_BE_SPECIFIED_WITH_OPTION,
                ));
                // Emit at opt_b's position (same message, different location)
                let start_b = find_key_offset_in_source(&stripped, key_b);
                let key_len_b = key_b.len() as u32 + 2;
                diagnostics.push(Diagnostic::error(
                    file_path,
                    start_b,
                    key_len_b,
                    msg,
                    diagnostic_codes::OPTION_CANNOT_BE_SPECIFIED_WITH_OPTION,
                ));
            }
        }

        // TS5067: Invalid value for 'jsxFactory' — must be a valid identifier or qualified name.
        // A qualified name is one or more identifiers separated by dots (e.g. React.createElement, h).
        // Spaces, = signs, and other non-identifier characters make the value invalid.
        if let Some(serde_json::Value::String(jsx_factory_val)) = compiler_opts.get("jsxFactory")
            && !is_valid_identifier_or_qualified_name(jsx_factory_val)
        {
            let start = find_value_offset_in_source(&stripped, "jsxFactory");
            let msg = format_message(
                diagnostic_messages::INVALID_VALUE_FOR_JSXFACTORY_IS_NOT_A_VALID_IDENTIFIER_OR_QUALIFIED_NAME,
                &[jsx_factory_val.as_str()],
            );
            diagnostics.push(Diagnostic::error(
                file_path,
                start,
                jsx_factory_val.len() as u32 + 2, // include surrounding quotes
                msg,
                diagnostic_codes::INVALID_VALUE_FOR_JSXFACTORY_IS_NOT_A_VALID_IDENTIFIER_OR_QUALIFIED_NAME,
            ));
        }

        if let Some(serde_json::Value::String(react_namespace_val)) =
            compiler_opts.get("reactNamespace")
            && !is_valid_identifier(react_namespace_val)
        {
            let start = find_value_offset_in_source(&stripped, "reactNamespace");
            let msg = format_message(
                diagnostic_messages::INVALID_VALUE_FOR_REACTNAMESPACE_IS_NOT_A_VALID_IDENTIFIER,
                &[react_namespace_val.as_str()],
            );
            diagnostics.push(Diagnostic::error(
                file_path,
                start,
                react_namespace_val.len() as u32 + 2,
                msg,
                diagnostic_codes::INVALID_VALUE_FOR_REACTNAMESPACE_IS_NOT_A_VALID_IDENTIFIER,
            ));
        }

        // TS5070: Option '--resolveJsonModule' cannot be specified when 'moduleResolution' is set to 'classic'.
        // TS5071: Option '--resolveJsonModule' cannot be specified when 'module' is set to 'none', 'system', or 'umd'.
        // Note: moduleResolution: bundler implies resolveJsonModule=true even when not explicitly set.
        let resolve_json_explicit = option_is_truthy(compiler_opts.get("resolveJsonModule"));
        let resolve_json_implied_by_bundler = !resolve_json_explicit
            && compiler_opts.get("resolveJsonModule").is_none()
            && matches!(
                compiler_opts.get("moduleResolution").and_then(|v| v.as_str()).map(normalize_option),
                Some(ref mr) if mr == "bundler"
            );
        if resolve_json_explicit || resolve_json_implied_by_bundler {
            // Compute effective moduleResolution from raw JSON options
            let effective_mr = if let Some(serde_json::Value::String(mr_value)) =
                compiler_opts.get("moduleResolution")
            {
                normalize_enum_option_value(mr_value.split(',').next().unwrap_or(mr_value))
            } else {
                // Default moduleResolution based on module setting
                let effective_module = if let Some(serde_json::Value::String(mod_value)) =
                    compiler_opts.get("module")
                {
                    normalize_enum_option_value(mod_value.split(',').next().unwrap_or(mod_value))
                } else {
                    String::new() // no module set
                };
                match effective_module.as_str() {
                    // Only map EXPLICITLY-set classic-implying module values to "classic".
                    // When module is not set (""), tsc determines the default from target
                    // (typically commonjs → node resolution), so do not assume "classic".
                    "none" | "amd" | "umd" | "system" => "classic".to_string(),
                    "commonjs" => "node".to_string(),
                    "node16" => "node16".to_string(),
                    "nodenext" => "nodenext".to_string(),
                    _ => "bundler".to_string(),
                }
            };

            if resolve_json_explicit && effective_mr == "classic" {
                let start = find_key_offset_in_source(&stripped, "resolveJsonModule");
                let key_len = "resolveJsonModule".len() as u32 + 2;
                diagnostics.push(Diagnostic::error(
                    file_path,
                    start,
                    key_len,
                    diagnostic_messages::OPTION_RESOLVEJSONMODULE_CANNOT_BE_SPECIFIED_WHEN_MODULERESOLUTION_IS_SET_TO_CLA.to_string(),
                    diagnostic_codes::OPTION_RESOLVEJSONMODULE_CANNOT_BE_SPECIFIED_WHEN_MODULERESOLUTION_IS_SET_TO_CLA,
                ));
            }

            // TS5071: fires when module=none/system/umd but ONLY when effective_mr is NOT
            // "classic". When effective_mr IS "classic" (implied or explicit), TS5070 already
            // covers the resolveJsonModule restriction; tsc never emits both errors at once.
            if effective_mr != "classic"
                && let Some(serde_json::Value::String(mod_value)) = compiler_opts.get("module")
            {
                let mod_normalized =
                    normalize_enum_option_value(mod_value.split(',').next().unwrap_or(mod_value));
                if matches!(mod_normalized.as_str(), "none" | "system" | "umd") {
                    let emit_ts5071 = |diagnostics: &mut Vec<Diagnostic>,
                                       error_key: &str,
                                       key_len: u32| {
                        let start = find_key_offset_in_source(&stripped, error_key);
                        diagnostics.push(Diagnostic::error(
                            file_path,
                            start,
                            key_len,
                            diagnostic_messages::OPTION_RESOLVEJSONMODULE_CANNOT_BE_SPECIFIED_WHEN_MODULE_IS_SET_TO_NONE_SYSTEM_O.to_string(),
                            diagnostic_codes::OPTION_RESOLVEJSONMODULE_CANNOT_BE_SPECIFIED_WHEN_MODULE_IS_SET_TO_NONE_SYSTEM_O,
                        ));
                    };

                    // tsc reports the invalid pairing on both participating options when
                    // resolveJsonModule is explicitly present in the config.
                    emit_ts5071(&mut diagnostics, "module", "module".len() as u32 + 2);
                    if resolve_json_explicit {
                        emit_ts5071(
                            &mut diagnostics,
                            "resolveJsonModule",
                            "resolveJsonModule".len() as u32 + 2,
                        );
                    }
                }
            }
        }

        // TS5098: Option '{0}' can only be used when 'moduleResolution' is set to 'node16', 'nodenext', or 'bundler'.
        let requires_modern_mr: &[&str] = &[
            "resolvePackageJsonExports",
            "resolvePackageJsonImports",
            "customConditions",
        ];
        // Match the defaulting chain `resolve_compiler_options` uses so the
        // pre-resolve TS5098 gate doesn't disagree with the post-resolve
        // option state. tsz's defaults are:
        //   target unset → default ScriptTarget::ESNext
        //   module unset → default ESNext (when target unset) else
        //                  `default_module_kind_for_target(target, true)`
        //   moduleResolution unset → `default_module_resolution_for_module(module)`
        // and `Bundler` / `Node16` / `NodeNext` all count as "modern".
        // See https://github.com/mohsen1/tsz/issues/3509.
        let mr_is_modern = if let Some(serde_json::Value::String(mr_value)) =
            compiler_opts.get("moduleResolution")
        {
            let mr_normalized =
                normalize_enum_option_value(mr_value.split(',').next().unwrap_or(mr_value));
            matches!(mr_normalized.as_str(), "node16" | "nodenext" | "bundler")
        } else {
            // Resolve module from explicit value, or fall back through target
            // to the same default `resolve_compiler_options` would compute.
            let module_kind = if let Some(serde_json::Value::String(mod_value)) =
                compiler_opts.get("module")
            {
                let mod_normalized =
                    normalize_enum_option_value(mod_value.split(',').next().unwrap_or(mod_value));
                ModuleKind::from_ts_str(&mod_normalized)
            } else if let Some(serde_json::Value::String(tgt_value)) = compiler_opts.get("target") {
                let tgt_normalized =
                    normalize_enum_option_value(tgt_value.split(',').next().unwrap_or(tgt_value));
                ScriptTarget::from_ts_str(&tgt_normalized)
                    .map(|target| default_module_kind_for_target(target, true))
            } else {
                Some(default_module_kind_for_target(ScriptTarget::ESNext, false))
            };
            module_kind.is_some_and(|module| {
                matches!(
                    default_module_resolution_for_module(module),
                    ModuleResolutionKind::Node16
                        | ModuleResolutionKind::NodeNext
                        | ModuleResolutionKind::Bundler
                )
            })
        };
        if !mr_is_modern {
            for &opt in requires_modern_mr {
                if option_is_truthy(compiler_opts.get(opt)) {
                    let start = find_key_offset_in_source(&stripped, opt);
                    let key_len = opt.len() as u32 + 2;
                    let msg = format_message(
                        diagnostic_messages::OPTION_CAN_ONLY_BE_USED_WHEN_MODULERESOLUTION_IS_SET_TO_NODE16_NODENEXT_OR_BUNDL,
                        &[opt],
                    );
                    diagnostics.push(Diagnostic::error(
                        file_path,
                        start,
                        key_len,
                        msg,
                        diagnostic_codes::OPTION_CAN_ONLY_BE_USED_WHEN_MODULERESOLUTION_IS_SET_TO_NODE16_NODENEXT_OR_BUNDL,
                    ));
                }
            }
        }

        // TS6046: Validate option values for target, module, moduleResolution, jsx,
        // moduleDetection, newLine, and lib.
        // If a value is invalid, emit TS6046 and null it out so resolve_compiler_options
        // doesn't see it and bail.
        validate_option_value(
            compiler_opts,
            "target",
            &stripped,
            file_path,
            VALID_TARGET_VALUES,
            "--target",
            VALID_TARGET_DISPLAY,
            &mut diagnostics,
        );
        validate_option_value(
            compiler_opts,
            "module",
            &stripped,
            file_path,
            VALID_MODULE_VALUES,
            "--module",
            VALID_MODULE_DISPLAY,
            &mut diagnostics,
        );
        validate_option_value(
            compiler_opts,
            "moduleResolution",
            &stripped,
            file_path,
            VALID_MODULE_RESOLUTION_VALUES,
            "--moduleResolution",
            VALID_MODULE_RESOLUTION_DISPLAY,
            &mut diagnostics,
        );
        validate_option_value(
            compiler_opts,
            "jsx",
            &stripped,
            file_path,
            VALID_JSX_VALUES,
            "--jsx",
            VALID_JSX_DISPLAY,
            &mut diagnostics,
        );
        validate_option_value(
            compiler_opts,
            "moduleDetection",
            &stripped,
            file_path,
            VALID_MODULE_DETECTION_VALUES,
            "--moduleDetection",
            VALID_MODULE_DETECTION_DISPLAY,
            &mut diagnostics,
        );
        validate_option_value(
            compiler_opts,
            "newLine",
            &stripped,
            file_path,
            VALID_NEW_LINE_VALUES,
            "--newLine",
            VALID_NEW_LINE_DISPLAY,
            &mut diagnostics,
        );
        validate_lib_values(compiler_opts, &stripped, file_path, &mut diagnostics);

        // TS5063/TS5066: Validate paths substitution values.
        // TS5063: value should be an array (not string/number/etc.)
        // TS5066: array shouldn't be empty
        let has_base_url = compiler_opts.contains_key("baseUrl");
        if let Some(serde_json::Value::Object(paths_obj)) = compiler_opts.get_mut("paths") {
            let mut bad_patterns: Vec<String> = Vec::new();
            for (pattern, value) in paths_obj.iter() {
                let search = format!("\"{pattern}\"");
                let paths_start = stripped.find("\"paths\"").unwrap_or(0);
                let key_pos = stripped[paths_start..]
                    .find(&search)
                    .map_or(0, |p| paths_start + p);
                let after_key = key_pos + search.len();
                let rest = &stripped[after_key..];
                let value_start = if let Some(colon_pos) = rest.find(':') {
                    let after_colon = &rest[(colon_pos + 1)..];
                    let ws = after_colon.len() - after_colon.trim_start().len();
                    (after_key + colon_pos + 1 + ws) as u32
                } else {
                    key_pos as u32
                };

                match value {
                    serde_json::Value::Array(arr) if arr.is_empty() => {
                        let msg = format_message(
                            diagnostic_messages::SUBSTITUTIONS_FOR_PATTERN_SHOULDNT_BE_AN_EMPTY_ARRAY,
                            &[pattern],
                        );
                        diagnostics.push(Diagnostic::error(
                            file_path,
                            value_start,
                            2, // "[]"
                            msg,
                            diagnostic_codes::SUBSTITUTIONS_FOR_PATTERN_SHOULDNT_BE_AN_EMPTY_ARRAY,
                        ));
                    }
                    serde_json::Value::Array(arr) => {
                        // TS5064: Substitution elements must be strings
                        for (idx, elem) in arr.iter().enumerate() {
                            if let Some(substitution) = elem.as_str() {
                                if !has_base_url
                                    && !substitution.is_empty()
                                    && !is_relative_path_mapping_substitution(substitution)
                                {
                                    // Without baseUrl, TypeScript rejects non-relative path
                                    // substitutions up front instead of silently ignoring them.
                                    let elem_pos = {
                                        let arr_start = stripped[value_start as usize..]
                                            .find('[')
                                            .map_or(value_start as usize, |p| {
                                                value_start as usize + p + 1
                                            });
                                        let mut pos = arr_start;
                                        let mut found = 0;
                                        while found < idx && pos < stripped.len() {
                                            if stripped.as_bytes()[pos] == b',' {
                                                found += 1;
                                            }
                                            pos += 1;
                                        }
                                        while pos < stripped.len()
                                            && stripped.as_bytes()[pos].is_ascii_whitespace()
                                        {
                                            pos += 1;
                                        }
                                        pos as u32
                                    };
                                    let msg = diagnostic_messages::NON_RELATIVE_PATHS_ARE_NOT_ALLOWED_WHEN_BASEURL_IS_NOT_SET_DID_YOU_FORGET_A_LEAD.to_string();
                                    diagnostics.push(Diagnostic::error(
                                        file_path,
                                        elem_pos,
                                        estimate_json_value_len(elem),
                                        msg,
                                        diagnostic_codes::NON_RELATIVE_PATHS_ARE_NOT_ALLOWED_WHEN_BASEURL_IS_NOT_SET_DID_YOU_FORGET_A_LEAD,
                                    ));
                                }
                            } else {
                                let type_name = match elem {
                                    serde_json::Value::Number(_) => "number",
                                    serde_json::Value::Bool(_) => "boolean",
                                    serde_json::Value::Null => "null",
                                    serde_json::Value::Object(_) => "object",
                                    serde_json::Value::Array(_) => "Array",
                                    _ => "unknown",
                                };
                                let elem_display = match elem {
                                    serde_json::Value::Number(n) => n.to_string(),
                                    serde_json::Value::Bool(b) => b.to_string(),
                                    serde_json::Value::Null => "null".to_string(),
                                    _ => format!("{elem}"),
                                };
                                // Find the position of the element in the source text
                                let elem_pos = {
                                    let arr_start = stripped[value_start as usize..]
                                        .find('[')
                                        .map_or(value_start as usize, |p| {
                                            value_start as usize + p + 1
                                        });
                                    // Skip past idx elements (separated by commas)
                                    let mut pos = arr_start;
                                    let mut found = 0;
                                    while found < idx && pos < stripped.len() {
                                        if stripped.as_bytes()[pos] == b',' {
                                            found += 1;
                                        }
                                        pos += 1;
                                    }
                                    // Skip whitespace
                                    while pos < stripped.len()
                                        && stripped.as_bytes()[pos].is_ascii_whitespace()
                                    {
                                        pos += 1;
                                    }
                                    pos as u32
                                };
                                let msg = format_message(
                                    diagnostic_messages::SUBSTITUTION_FOR_PATTERN_HAS_INCORRECT_TYPE_EXPECTED_STRING_GOT,
                                    &[&elem_display, pattern, type_name],
                                );
                                diagnostics.push(Diagnostic::error(
                                    file_path,
                                    elem_pos,
                                    estimate_json_value_len(elem),
                                    msg,
                                    diagnostic_codes::SUBSTITUTION_FOR_PATTERN_HAS_INCORRECT_TYPE_EXPECTED_STRING_GOT,
                                ));
                                bad_patterns.push(pattern.clone());
                            }
                        }
                    }
                    _ => {
                        // TS5063: not an array
                        let value_len = estimate_json_value_len(value);
                        let msg = format_message(
                            diagnostic_messages::SUBSTITUTIONS_FOR_PATTERN_SHOULD_BE_AN_ARRAY,
                            &[pattern],
                        );
                        diagnostics.push(Diagnostic::error(
                            file_path,
                            value_start,
                            value_len,
                            msg,
                            diagnostic_codes::SUBSTITUTIONS_FOR_PATTERN_SHOULD_BE_AN_ARRAY,
                        ));
                        bad_patterns.push(pattern.clone());
                    }
                }
            }
            // Fix invalid values so serde can deserialize
            for pattern in &bad_patterns {
                if let Some(v) = paths_obj.get_mut(pattern) {
                    *v = serde_json::Value::Array(Vec::new());
                }
            }
        }

        // Propagate ts5024_keys out of this scope for use in resolve_compiler_options.
        ts5024_keys_outer = ts5024_keys;
    }

    // TS5024 for top-level tsconfig properties with wrong types. These
    // represented root selectors must be arrays; null invalidates the selector
    // without a diagnostic, matching serde's Option<T> representation.
    if let Some(obj) = raw.as_object_mut() {
        for key in ["include", "exclude", "files", "references"] {
            validate_top_level_array_option(obj, &mut diagnostics, &stripped, file_path, key);
        }
        // `compilerOptions` must be a JSON object; a scalar bypasses every
        // nested option validator and would otherwise surface as a generic
        // serde `invalid type` failure instead of TS5024.
        validate_top_level_object_option(
            obj,
            &mut diagnostics,
            &stripped,
            file_path,
            "compilerOptions",
        );
        // `compileOnSave` is a top-level boolean. tsc reports TS5024 when it
        // is set to a non-boolean (#3591 repro C); without this gate the
        // value is silently ignored.
        validate_top_level_boolean_option(
            obj,
            &mut diagnostics,
            &stripped,
            file_path,
            "compileOnSave",
        );
        // `typeAcquisition` keys are enumerated. Unknown keys must surface
        // as TS17010 to match tsc (#3591 repro B); an object that is not an
        // object is also flagged via the shared object validator above.
        validate_top_level_object_option(
            obj,
            &mut diagnostics,
            &stripped,
            file_path,
            "typeAcquisition",
        );
        validate_type_acquisition_known_keys(obj, &mut diagnostics, &stripped, file_path);

        // TS6046 for invalid `watchOptions.watchFile` / `watchDirectory` /
        // `fallbackPolling` enum values. tsc surfaces these as config
        // diagnostics before compiling; tsz used to skip them entirely.
        // See https://github.com/mohsen1/tsz/issues/3591 (repro A).
        if let Some(serde_json::Value::Object(watch_opts)) = obj.get_mut("watchOptions") {
            validate_option_value(
                watch_opts,
                "watchFile",
                &stripped,
                file_path,
                VALID_WATCH_FILE_VALUES,
                "--watchFile",
                VALID_WATCH_FILE_DISPLAY,
                &mut diagnostics,
            );
            validate_option_value(
                watch_opts,
                "watchDirectory",
                &stripped,
                file_path,
                VALID_WATCH_DIRECTORY_VALUES,
                "--watchDirectory",
                VALID_WATCH_DIRECTORY_DISPLAY,
                &mut diagnostics,
            );
            validate_option_value(
                watch_opts,
                "fallbackPolling",
                &stripped,
                file_path,
                VALID_FALLBACK_POLLING_VALUES,
                "--fallbackPolling",
                VALID_FALLBACK_POLLING_DISPLAY,
                &mut diagnostics,
            );
        }
    }

    let mut config: TsConfig =
        serde_json::from_value(raw).context("failed to parse tsconfig JSON")?;

    // Attach TS5024 invalidated keys so resolve_compiler_options knows not to apply defaults.
    if let Some(ref mut opts) = config.compiler_options {
        opts.invalidated_options = ts5024_keys_outer;
    }

    Ok(ParsedTsConfig {
        config,
        diagnostics,
        suppress_excess_property_errors: suppress_excess,
        suppress_implicit_any_index_errors: suppress_any_index,
        no_implicit_use_strict,
    })
}

/// Check whether a JSON value represents a truthy compiler option.
/// Returns true for `true` booleans, non-empty strings, and non-null values
/// that aren't `false`. Returns false for `None`, `null`, and `false`.
const fn option_is_truthy(value: Option<&serde_json::Value>) -> bool {
    match value {
        None | Some(serde_json::Value::Null) => false,
        Some(serde_json::Value::Bool(b)) => *b,
        // String options (like jsxFactory, reactNamespace) are truthy when present
        Some(_) => true,
    }
}

fn option_is_effectively_enabled(
    compiler_opts: &serde_json::Map<String, serde_json::Value>,
    invalidated_options: &[String],
    key: &str,
) -> bool {
    if compiler_option_expected_type(key) == "boolean"
        && invalidated_options.iter().any(|k| k == key)
    {
        return false;
    }
    option_is_truthy(compiler_opts.get(key))
}

fn option_key_present_or_invalidated(
    compiler_opts: &serde_json::Map<String, serde_json::Value>,
    invalidated_options: &[String],
    key: &str,
) -> bool {
    compiler_opts.contains_key(key) || invalidated_options.iter().any(|k| k == key)
}

/// Check if a string is a valid TypeScript identifier or qualified name.
/// A qualified name is one or more identifiers separated by dots: `A.B.C`.
/// Used to validate `jsxFactory` option values (TS5067).
fn is_valid_identifier_or_qualified_name(s: &str) -> bool {
    if s.is_empty() {
        return false;
    }
    for segment in s.split('.') {
        if !is_valid_identifier(segment) {
            return false;
        }
    }
    true
}

fn is_valid_identifier(s: &str) -> bool {
    if s.is_empty() {
        return false;
    }
    let mut chars = s.chars();
    match chars.next() {
        Some(c) if c.is_alphabetic() || c == '_' || c == '$' => {}
        _ => return false,
    }
    chars.all(|c| c.is_alphanumeric() || c == '_' || c == '$')
}

/// Find the byte offset of a JSON key within the source text.
/// Searches for `"key"` after `compilerOptions`.
fn find_key_offset_in_source(source: &str, key: &str) -> u32 {
    let search = format!("\"{key}\"");
    // Look for the key after "compilerOptions" to avoid matching in other sections
    let compiler_opts_pos = source.find("compilerOptions").unwrap_or(0);
    if let Some(pos) = source[compiler_opts_pos..].find(&search) {
        // Point at the opening quote of the key, matching tsc behavior
        (compiler_opts_pos + pos) as u32
    } else {
        0
    }
}

/// Find the byte offset of a JSON value within the source text.
/// Searches for `"key":` after `compilerOptions`, then finds the value start.
fn find_value_offset_in_source(source: &str, key: &str) -> u32 {
    let search = format!("\"{key}\"");
    let compiler_opts_pos = source.find("compilerOptions").unwrap_or(0);
    if let Some(key_pos) = source[compiler_opts_pos..].find(&search) {
        let after_key = compiler_opts_pos + key_pos + search.len();
        // Skip whitespace and colon to find value start
        let rest = &source[after_key..];
        if let Some(colon_pos) = rest.find(':') {
            let after_colon = after_key + colon_pos + 1;
            let value_rest = &source[after_colon..];
            // Skip whitespace to find value
            let trimmed_offset = value_rest.len() - value_rest.trim_start().len();
            return (after_colon + trimmed_offset) as u32;
        }
    }
    0
}

fn find_top_level_value_offset_in_source(source: &str, key: &str) -> u32 {
    let search = format!("\"{key}\"");
    let Some(key_pos) = source.find(&search) else {
        return 0;
    };

    let after_key = key_pos + search.len();
    let rest = &source[after_key..];
    if let Some(colon_pos) = rest.find(':') {
        let after_colon = after_key + colon_pos + 1;
        let value_rest = &source[after_colon..];
        let whitespace_len = value_rest.len() - value_rest.trim_start().len();
        (after_colon + whitespace_len) as u32
    } else {
        key_pos as u32
    }
}

fn validate_top_level_array_option(
    obj: &mut serde_json::Map<String, serde_json::Value>,
    diagnostics: &mut Vec<Diagnostic>,
    source: &str,
    file_path: &str,
    key: &str,
) {
    let Some(value) = obj.get(key) else {
        return;
    };
    if value.is_null() || value.is_array() {
        return;
    }

    let value_start = find_top_level_value_offset_in_source(source, key);
    let value_len = estimate_json_value_len(value);
    let msg = format_message(
        diagnostic_messages::COMPILER_OPTION_REQUIRES_A_VALUE_OF_TYPE,
        &[key, "Array"],
    );
    diagnostics.push(Diagnostic::error(
        file_path,
        value_start,
        value_len,
        msg,
        diagnostic_codes::COMPILER_OPTION_REQUIRES_A_VALUE_OF_TYPE,
    ));

    obj.insert(key.to_string(), serde_json::Value::Null);
}

fn validate_top_level_object_option(
    obj: &mut serde_json::Map<String, serde_json::Value>,
    diagnostics: &mut Vec<Diagnostic>,
    source: &str,
    file_path: &str,
    key: &str,
) {
    let Some(value) = obj.get(key) else {
        return;
    };
    if value.is_null() || value.is_object() {
        return;
    }

    let value_start = find_top_level_value_offset_in_source(source, key);
    let value_len = estimate_json_value_len(value);
    let msg = format_message(
        diagnostic_messages::COMPILER_OPTION_REQUIRES_A_VALUE_OF_TYPE,
        &[key, "object"],
    );
    diagnostics.push(Diagnostic::error(
        file_path,
        value_start,
        value_len,
        msg,
        diagnostic_codes::COMPILER_OPTION_REQUIRES_A_VALUE_OF_TYPE,
    ));

    // Replace the scalar with an empty object so serde can still deserialize
    // the rest of the config; the diagnostic above is what surfaces to users.
    obj.insert(
        key.to_string(),
        serde_json::Value::Object(serde_json::Map::new()),
    );
}

fn validate_top_level_boolean_option(
    obj: &mut serde_json::Map<String, serde_json::Value>,
    diagnostics: &mut Vec<Diagnostic>,
    source: &str,
    file_path: &str,
    key: &str,
) {
    let Some(value) = obj.get(key) else {
        return;
    };
    if value.is_null() || value.is_boolean() {
        return;
    }

    let value_start = find_top_level_value_offset_in_source(source, key);
    let value_len = estimate_json_value_len(value);
    let msg = format_message(
        diagnostic_messages::COMPILER_OPTION_REQUIRES_A_VALUE_OF_TYPE,
        &[key, "boolean"],
    );
    diagnostics.push(Diagnostic::error(
        file_path,
        value_start,
        value_len,
        msg,
        diagnostic_codes::COMPILER_OPTION_REQUIRES_A_VALUE_OF_TYPE,
    ));

    obj.insert(key.to_string(), serde_json::Value::Null);
}

fn validate_type_acquisition_known_keys(
    obj: &serde_json::Map<String, serde_json::Value>,
    diagnostics: &mut Vec<Diagnostic>,
    source: &str,
    file_path: &str,
) {
    const KNOWN: &[&str] = &[
        "enable",
        "include",
        "exclude",
        "disableFilenameBasedTypeAcquisition",
    ];
    let Some(serde_json::Value::Object(map)) = obj.get("typeAcquisition") else {
        return;
    };
    for key in map.keys() {
        if KNOWN.iter().any(|k| k.eq_ignore_ascii_case(key)) {
            continue;
        }
        let key_offset = find_nested_key_offset_in_source(source, "typeAcquisition", key);
        let key_len = key.len() as u32 + 2;
        let msg = format_message(diagnostic_messages::UNKNOWN_TYPE_ACQUISITION_OPTION, &[key]);
        diagnostics.push(Diagnostic::error(
            file_path,
            key_offset,
            key_len,
            msg,
            diagnostic_codes::UNKNOWN_TYPE_ACQUISITION_OPTION,
        ));
    }
}

fn find_nested_key_offset_in_source(source: &str, parent_key: &str, child_key: &str) -> u32 {
    let parent_pat = format!("\"{parent_key}\"");
    let Some(parent_pos) = source.find(&parent_pat) else {
        return 0;
    };
    let child_pat = format!("\"{child_key}\"");
    let after_parent = parent_pos + parent_pat.len();
    source[after_parent..]
        .find(&child_pat)
        .map(|p| (after_parent + p) as u32)
        .unwrap_or(0)
}

/// Estimate the display length of a JSON value for diagnostic span.
fn estimate_json_value_len(value: &serde_json::Value) -> u32 {
    match value {
        serde_json::Value::String(s) => s.len() as u32 + 2, // include quotes
        serde_json::Value::Bool(b) => {
            if *b {
                4
            } else {
                5
            }
        }
        serde_json::Value::Number(n) => n.to_string().len() as u32,
        serde_json::Value::Null => 4,
        serde_json::Value::Array(_) | serde_json::Value::Object(_) => serde_json::to_string(value)
            .map(|s| s.len() as u32)
            .unwrap_or(2),
    }
}

/// Matches TypeScript's `pathIsRelative` check: `/^\\.\\.?($|[\\\\/])/`.
const fn is_relative_path_mapping_substitution(specifier: &str) -> bool {
    matches!(
        specifier.as_bytes(),
        [b'.'] | [b'.', b'.'] | [b'.', b'/' | b'\\', ..] | [b'.', b'.', b'/' | b'\\', ..]
    )
}

/// Return the expected JSON value type for a compiler option.
/// Returns "" for unknown/unvalidated options.
fn compiler_option_expected_type(key: &str) -> &'static str {
    match key {
        // Boolean options
        "allowArbitraryExtensions"
        | "allowImportingTsExtensions"
        | "allowJs"
        | "allowSyntheticDefaultImports"
        | "allowUmdGlobalAccess"
        | "allowUnreachableCode"
        | "allowUnusedLabels"
        | "alwaysStrict"
        | "checkJs"
        | "composite"
        | "declaration"
        | "declarationMap"
        | "disableReferencedProjectLoad"
        | "disableSizeLimit"
        | "disableSolutionSearching"
        | "disableSourceOfProjectReferenceRedirect"
        | "downlevelIteration"
        | "emitBOM"
        | "emitDeclarationOnly"
        | "emitDecoratorMetadata"
        | "esModuleInterop"
        | "exactOptionalPropertyTypes"
        | "experimentalDecorators"
        | "forceConsistentCasingInFileNames"
        | "importHelpers"
        | "incremental"
        | "inlineSourceMap"
        | "inlineSources"
        | "isolatedDeclarations"
        | "isolatedModules"
        | "keyofStringsOnly"
        | "noEmit"
        | "noEmitHelpers"
        | "noEmitOnError"
        | "noErrorTruncation"
        | "noFallthroughCasesInSwitch"
        | "noImplicitAny"
        | "noImplicitOverride"
        | "noImplicitReturns"
        | "noImplicitThis"
        | "noImplicitUseStrict"
        | "noLib"
        | "libReplacement"
        | "noPropertyAccessFromIndexSignature"
        | "noResolve"
        | "noStrictGenericChecks"
        | "noUncheckedIndexedAccess"
        | "noUncheckedSideEffectImports"
        | "noUnusedLocals"
        | "noUnusedParameters"
        | "preserveConstEnums"
        | "preserveSymlinks"
        | "preserveValueImports"
        | "pretty"
        | "removeComments"
        | "resolveJsonModule"
        | "resolvePackageJsonExports"
        | "resolvePackageJsonImports"
        | "rewriteRelativeImportExtensions"
        | "skipDefaultLibCheck"
        | "skipLibCheck"
        | "sound"
        | "soundCheckDeclarations"
        | "soundPedantic"
        | "soundReportOnly"
        | "sourceMap"
        | "strict"
        | "strictBindCallApply"
        | "strictBuiltinIteratorReturn"
        | "strictFunctionTypes"
        | "strictNullChecks"
        | "strictPropertyInitialization"
        | "stripInternal"
        | "suppressExcessPropertyErrors"
        | "suppressImplicitAnyIndexErrors"
        | "traceResolution"
        | "useDefineForClassFields"
        | "useUnknownInCatchVariables"
        | "verbatimModuleSyntax" => "boolean",
        // String options
        "baseUrl"
        | "charset"
        | "declarationDir"
        | "jsx"
        | "jsxFactory"
        | "jsxFragmentFactory"
        | "jsxImportSource"
        | "mapRoot"
        | "module"
        | "moduleDetection"
        | "moduleResolution"
        | "newLine"
        | "out"
        | "outDir"
        | "outFile"
        | "reactNamespace"
        | "rootDir"
        | "sourceRoot"
        | "target"
        | "tsBuildInfoFile"
        | "ignoreDeprecations"
        | "typesVersionsCompilerVersion" => "string",
        // Number options
        "maxNodeModuleJsDepth" => "number",
        // List options (arrays)
        "lib" | "types" | "typeRoots" | "rootDirs" | "moduleSuffixes" | "customConditions"
        | "plugins" => "Array",
        // Object options
        "paths" => "object",
        _ => "",
    }
}

/// Check if a compiler option has been removed in TypeScript 5.5.
/// Returns `Some(use_instead)` if removed, where `use_instead` is "" or a replacement name.
/// These options were deprecated in TS 5.0 and removed in TS 5.5.
fn removed_compiler_option(key: &str) -> Option<&'static str> {
    match key {
        "noImplicitUseStrict"
        | "keyofStringsOnly"
        | "suppressExcessPropertyErrors"
        | "suppressImplicitAnyIndexErrors"
        | "noStrictGenericChecks"
        | "charset" => Some(""),
        "importsNotUsedAsValues" | "preserveValueImports" => Some("verbatimModuleSyntax"),
        "out" => Some("outFile"),
        _ => None,
    }
}

fn unknown_compiler_option_suggestion(key_lower: &str) -> Option<&'static str> {
    // Preserve the historical aliases for `disableSolution*`: those names are
    // closer to the real option semantically than they are by edit distance,
    // so spell out the mapping rather than relying on Levenshtein scoring.
    if let Some(name) = match key_lower {
        "disablesolutioncaching" | "disablesolutiontypechecking" => {
            Some("disableSolutionSearching")
        }
        _ => None,
    } {
        return Some(name);
    }

    // General nearest-option suggestion using TypeScript's `getSpellingSuggestion`
    // algorithm against the full set of canonical compiler-option names. This
    // upgrades typos like `stric` → `strict`, `noEmti` → `noEmit`, and
    // `moduleResoluton` → `moduleResolution` from a bare TS5023 to a TS5025
    // `Did you mean ...` diagnostic.
    tsz_parser::parser::spelling::get_spelling_suggestion(
        key_lower,
        KNOWN_COMPILER_OPTION_CANONICAL_NAMES,
    )
}

/// Canonical names of every compiler option recognized by `known_compiler_option`.
/// Used as the candidate set for `getSpellingSuggestion`-style typo recovery.
/// Keep this list in sync with `known_compiler_option`.
const KNOWN_COMPILER_OPTION_CANONICAL_NAMES: &[&str] = &[
    "allowArbitraryExtensions",
    "allowImportingTsExtensions",
    "allowJs",
    "allowSyntheticDefaultImports",
    "allowUmdGlobalAccess",
    "allowUnreachableCode",
    "allowUnusedLabels",
    "alwaysStrict",
    "baseUrl",
    "charset",
    "checkJs",
    "composite",
    "customConditions",
    "declaration",
    "declarationDir",
    "declarationMap",
    "diagnostics",
    "disableReferencedProjectLoad",
    "disableSizeLimit",
    "disableSolutionSearching",
    "disableSourceOfProjectReferenceRedirect",
    "disableSourceOfReferencedProjectLoad",
    "downlevelIteration",
    "emitBOM",
    "emitDeclarationOnly",
    "emitDecoratorMetadata",
    "erasableSyntaxOnly",
    "esModuleInterop",
    "exactOptionalPropertyTypes",
    "experimentalDecorators",
    "explainFiles",
    "extendedDiagnostics",
    "forceConsistentCasingInFileNames",
    "generateCpuProfile",
    "generateTrace",
    "ignoreDeprecations",
    "importHelpers",
    "importsNotUsedAsValues",
    "incremental",
    "inlineSourceMap",
    "inlineSources",
    "isolatedDeclarations",
    "isolatedModules",
    "jsx",
    "jsxFactory",
    "jsxFragmentFactory",
    "jsxImportSource",
    "keyofStringsOnly",
    "lib",
    "libReplacement",
    "listEmittedFiles",
    "listFiles",
    "listFilesOnly",
    "locale",
    "mapRoot",
    "maxNodeModuleJsDepth",
    "module",
    "moduleDetection",
    "moduleResolution",
    "moduleSuffixes",
    "newLine",
    "noCheck",
    "noEmit",
    "noEmitHelpers",
    "noEmitOnError",
    "noErrorTruncation",
    "noFallthroughCasesInSwitch",
    "noImplicitAny",
    "noImplicitOverride",
    "noImplicitReturns",
    "noImplicitThis",
    "noImplicitUseStrict",
    "noLib",
    "noTypesAndSymbols",
    "noPropertyAccessFromIndexSignature",
    "noResolve",
    "noStrictGenericChecks",
    "noUncheckedIndexedAccess",
    "noUncheckedSideEffectImports",
    "noUnusedLocals",
    "noUnusedParameters",
    "out",
    "outDir",
    "outFile",
    "paths",
    "plugins",
    "preserveConstEnums",
    "preserveSymlinks",
    "preserveValueImports",
    "preserveWatchOutput",
    "pretty",
    "reactNamespace",
    "removeComments",
    "resolveJsonModule",
    "resolvePackageJsonExports",
    "resolvePackageJsonImports",
    "rewriteRelativeImportExtensions",
    "rootDir",
    "rootDirs",
    "skipDefaultLibCheck",
    "skipLibCheck",
    "sound",
    "soundCheckDeclarations",
    "soundPedantic",
    "soundReportOnly",
    "sourceMap",
    "sourceRoot",
    "strict",
    "strictBindCallApply",
    "strictBuiltinIteratorReturn",
    "strictFunctionTypes",
    "strictNullChecks",
    "strictPropertyInitialization",
    "stripInternal",
    "stableTypeOrdering",
    "suppressExcessPropertyErrors",
    "suppressImplicitAnyIndexErrors",
    "target",
    "traceResolution",
    "tsBuildInfoFile",
    "typesVersionsCompilerVersion",
    "typeRoots",
    "types",
    "useDefineForClassFields",
    "useUnknownInCatchVariables",
    "verbatimModuleSyntax",
];

/// Comprehensive map of all known TypeScript compiler options.
/// Maps lowercase name → canonical camelCase name.
fn known_compiler_option(key_lower: &str) -> Option<&'static str> {
    match key_lower {
        "allowarbitraryextensions" => Some("allowArbitraryExtensions"),
        "allowimportingtsextensions" => Some("allowImportingTsExtensions"),
        "allowjs" => Some("allowJs"),
        "allowsyntheticdefaultimports" => Some("allowSyntheticDefaultImports"),
        "allowumdglobalaccess" => Some("allowUmdGlobalAccess"),
        "allowunreachablecode" => Some("allowUnreachableCode"),
        "allowunusedlabels" => Some("allowUnusedLabels"),
        "alwaysstrict" => Some("alwaysStrict"),
        "baseurl" => Some("baseUrl"),
        "charset" => Some("charset"),
        "checkjs" => Some("checkJs"),
        "composite" => Some("composite"),
        "customconditions" => Some("customConditions"),
        "declaration" => Some("declaration"),
        "declarationdir" => Some("declarationDir"),
        "declarationmap" => Some("declarationMap"),
        "diagnostics" => Some("diagnostics"),
        "disablereferencedprojectload" => Some("disableReferencedProjectLoad"),
        // Keep the historical typo alias for compatibility, but accept the real key too.
        "disablesizelimit" | "disablesizelimt" => Some("disableSizeLimit"),
        "disablesolutionsearching" => Some("disableSolutionSearching"),
        "disablesourceofprojectreferenceredirect" => {
            Some("disableSourceOfProjectReferenceRedirect")
        }
        "disablesourceofreferencedprojectload" => Some("disableSourceOfReferencedProjectLoad"),
        "downleveliteration" => Some("downlevelIteration"),
        "emitbom" => Some("emitBOM"),
        "emitdeclarationonly" => Some("emitDeclarationOnly"),
        "emitdecoratormetadata" => Some("emitDecoratorMetadata"),
        "erasablesyntaxonly" => Some("erasableSyntaxOnly"),
        "esmoduleinterop" => Some("esModuleInterop"),
        "exactoptionalpropertytypes" => Some("exactOptionalPropertyTypes"),
        "experimentaldecorators" => Some("experimentalDecorators"),
        "explainfiles" => Some("explainFiles"),
        "extendeddiagnostics" => Some("extendedDiagnostics"),
        "forceconsecinferfaces" | "forceconsistentcasinginfilenames" => {
            Some("forceConsistentCasingInFileNames")
        }
        "generatecputrace" | "generatecpuprofile" => Some("generateCpuProfile"),
        "generatetrace" => Some("generateTrace"),
        "ignoredeprecations" => Some("ignoreDeprecations"),
        "importhelpers" => Some("importHelpers"),
        "importsnotusedasvalues" => Some("importsNotUsedAsValues"),
        "incremental" => Some("incremental"),
        "inlinesourcemap" => Some("inlineSourceMap"),
        "inlinesources" => Some("inlineSources"),
        "isolateddeclarations" => Some("isolatedDeclarations"),
        "isolatedmodules" => Some("isolatedModules"),
        "jsx" => Some("jsx"),
        "jsxfactory" => Some("jsxFactory"),
        "jsxfragmentfactory" => Some("jsxFragmentFactory"),
        "jsximportsource" => Some("jsxImportSource"),
        "keyofstringsonly" => Some("keyofStringsOnly"),
        "lib" => Some("lib"),
        "libreplacement" => Some("libReplacement"),
        "listemittedfiles" => Some("listEmittedFiles"),
        "listfiles" => Some("listFiles"),
        "listfilesonly" => Some("listFilesOnly"),
        "locale" => Some("locale"),
        "maproot" => Some("mapRoot"),
        "maxnodemodulejsdepth" => Some("maxNodeModuleJsDepth"),
        "module" => Some("module"),
        "moduledetection" => Some("moduleDetection"),
        "moduleresolution" => Some("moduleResolution"),
        "modulesuffixes" => Some("moduleSuffixes"),
        "newline" => Some("newLine"),
        "nocheck" => Some("noCheck"),
        "noemit" => Some("noEmit"),
        "noemithelpers" => Some("noEmitHelpers"),
        "noemitonerror" => Some("noEmitOnError"),
        "noerrortruncation" => Some("noErrorTruncation"),
        "nofallthroughcasesinswitch" => Some("noFallthroughCasesInSwitch"),
        "noimplicitany" => Some("noImplicitAny"),
        "noimplicitoverride" => Some("noImplicitOverride"),
        "noimplicitreturns" => Some("noImplicitReturns"),
        "noimplicitthis" => Some("noImplicitThis"),
        "noimplicitusestrict" => Some("noImplicitUseStrict"),
        "nolib" => Some("noLib"),
        "notypesandsymbols" => Some("noTypesAndSymbols"),
        "nopropertyaccessfromindexsignature" => Some("noPropertyAccessFromIndexSignature"),
        "noresolve" => Some("noResolve"),
        "nostrictgenericchecks" => Some("noStrictGenericChecks"),
        "nouncheckedindexedaccess" => Some("noUncheckedIndexedAccess"),
        "nouncheckedsideeffectimports" => Some("noUncheckedSideEffectImports"),
        "nounusedlocals" => Some("noUnusedLocals"),
        "nounusedparameters" => Some("noUnusedParameters"),
        "out" => Some("out"),
        "outdir" => Some("outDir"),
        "outfile" => Some("outFile"),
        "paths" => Some("paths"),
        "plugins" => Some("plugins"),
        "preserveconstenums" => Some("preserveConstEnums"),
        "preservesymlinks" => Some("preserveSymlinks"),
        "preservevalueimports" => Some("preserveValueImports"),
        "preservewatchoutput" => Some("preserveWatchOutput"),
        "pretty" => Some("pretty"),
        "reactnamespace" => Some("reactNamespace"),
        "removecomments" => Some("removeComments"),
        "resolvejsonmodule" => Some("resolveJsonModule"),
        "resolvepackagejsonexports" => Some("resolvePackageJsonExports"),
        "resolvepackagejsonimports" => Some("resolvePackageJsonImports"),
        "rewriterelativeimportextensions" => Some("rewriteRelativeImportExtensions"),
        "rootdir" => Some("rootDir"),
        "rootdirs" => Some("rootDirs"),
        "skipdefaultlibcheck" => Some("skipDefaultLibCheck"),
        "skiplibcheck" => Some("skipLibCheck"),
        "sound" => Some("sound"),
        "soundcheckdeclarations" => Some("soundCheckDeclarations"),
        "soundpedantic" => Some("soundPedantic"),
        "soundreportonly" => Some("soundReportOnly"),
        "sourcemap" => Some("sourceMap"),
        "sourceroot" => Some("sourceRoot"),
        "strict" => Some("strict"),
        "strictbindcallapply" => Some("strictBindCallApply"),
        "strictbuiltiniteratorreturn" => Some("strictBuiltinIteratorReturn"),
        "strictfunctiontypes" => Some("strictFunctionTypes"),
        "strictnullchecks" => Some("strictNullChecks"),
        "strictpropertyinitialization" => Some("strictPropertyInitialization"),
        "stripinternal" => Some("stripInternal"),
        "stabletypeordering" => Some("stableTypeOrdering"),
        "suppressexcesspropertyerrors" => Some("suppressExcessPropertyErrors"),
        "suppressimplicitanyindexerrors" => Some("suppressImplicitAnyIndexErrors"),
        "target" => Some("target"),
        "traceresolution" => Some("traceResolution"),
        "tsbuildinfofile" => Some("tsBuildInfoFile"),
        "typesversionscompilerversion" => Some("typesVersionsCompilerVersion"),
        "typeroots" => Some("typeRoots"),
        "types" => Some("types"),
        "usedefineforclassfields" => Some("useDefineForClassFields"),
        "useunknownincatchvariables" => Some("useUnknownInCatchVariables"),
        "verbatimmodulesyntax" => Some("verbatimModuleSyntax"),
        _ => None,
    }
}

pub fn load_tsconfig(path: &Path) -> Result<TsConfig> {
    let mut visited = FxHashSet::default();
    load_tsconfig_inner(path, &mut visited, false)
}

/// Load tsconfig.json and collect config-level diagnostics.
pub fn load_tsconfig_with_diagnostics(path: &Path) -> Result<ParsedTsConfig> {
    let mut visited = FxHashSet::default();
    load_tsconfig_inner_with_diagnostics(path, &mut visited, false)
}

fn config_ignore_deprecations_silences_6_0(config: &TsConfig) -> bool {
    matches!(
        config
            .compiler_options
            .as_ref()
            .and_then(|options| options.ignore_deprecations.as_deref()),
        Some("6.0")
    )
}

const fn is_ts60_deprecation_diagnostic_code(code: u32) -> bool {
    code == diagnostic_codes::OPTION_IS_DEPRECATED_AND_WILL_STOP_FUNCTIONING_IN_TYPESCRIPT_SPECIFY_COMPILEROPT_2
        || code
            == diagnostic_codes::OPTION_IS_DEPRECATED_AND_WILL_STOP_FUNCTIONING_IN_TYPESCRIPT_SPECIFY_COMPILEROPT
}

fn load_tsconfig_inner(
    path: &Path,
    visited: &mut FxHashSet<PathBuf>,
    inherited: bool,
) -> Result<TsConfig> {
    let canonical = std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    if !visited.insert(canonical.clone()) {
        bail!("tsconfig extends cycle detected at {}", canonical.display());
    }

    let source = std::fs::read_to_string(path)
        .with_context(|| format!("failed to read tsconfig: {}", path.display()))?;
    let mut config = parse_tsconfig(&source)
        .with_context(|| format!("failed to parse tsconfig: {}", path.display()))?;
    anchor_inherited_path_options(&mut config, path);
    if inherited {
        anchor_inherited_root_selectors(&mut config, path);
    }

    let extends = config.extends.take();
    if let Some(extends_value) = extends {
        let extends_paths = match extends_value {
            ExtendsValue::Single(s) => vec![s],
            ExtendsValue::Array(arr) => arr,
        };
        // Apply extends in order: later entries override earlier ones.
        // Each base is merged into the accumulated config.
        let mut accumulated: Option<TsConfig> = None;
        for extends_path_str in &extends_paths {
            let base_path = resolve_extends_path(path, extends_path_str)?;
            let base_config = load_tsconfig_inner(&base_path, visited, true)?;
            accumulated = Some(match accumulated {
                Some(acc) => merge_configs(acc, base_config),
                None => base_config,
            });
        }
        if let Some(base) = accumulated {
            config = merge_configs(base, config);
        }
    }

    visited.remove(&canonical);
    Ok(config)
}

fn load_tsconfig_inner_with_diagnostics(
    path: &Path,
    visited: &mut FxHashSet<PathBuf>,
    inherited: bool,
) -> Result<ParsedTsConfig> {
    let canonical = std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    if !visited.insert(canonical.clone()) {
        bail!("tsconfig extends cycle detected at {}", canonical.display());
    }

    let source = std::fs::read_to_string(path)
        .with_context(|| format!("failed to read tsconfig: {}", path.display()))?;
    let file_display = path.display().to_string();
    let mut parsed = parse_tsconfig_with_diagnostics(&source, &file_display)
        .with_context(|| format!("failed to parse tsconfig: {}", path.display()))?;
    anchor_inherited_path_options(&mut parsed.config, path);
    if inherited {
        anchor_inherited_root_selectors(&mut parsed.config, path);
    }

    let extends = parsed.config.extends.take();
    if let Some(extends_value) = extends {
        let extends_paths = match extends_value {
            ExtendsValue::Single(s) => vec![s],
            ExtendsValue::Array(arr) => arr,
        };
        let mut accumulated: Option<TsConfig> = None;
        let mut base_removed_options: Vec<String> = Vec::new();
        for extends_path_str in &extends_paths {
            let base_path = resolve_extends_path(path, extends_path_str)?;
            // Collect removed options from base configs for TS5102 diagnostics.
            // TSC checks the merged result and emits TS5102 at the child's key position
            // when removed options come from base configs via extends.
            collect_removed_options_from_config(&base_path, &mut base_removed_options);
            // Route base configs through the diagnostic path so TS5024 / TS5025
            // fire on the *base* file (matching tsc's `base.json(L,C):` anchor)
            // instead of the child's invalid option being silently coerced through
            // the type-validating-free `load_tsconfig_inner`.
            //
            // TS5102 (removed compiler option) is filtered out of the base's
            // diagnostics because tsc only re-anchors that one at the child's
            // `compilerOptions` key (and only when the child opts into the
            // `verbatimModuleSyntax` replacement). The post-merge block below
            // owns that re-emission; letting the base's per-option TS5102
            // through would double-report and anchor at the wrong file.
            let base_parsed = load_tsconfig_inner_with_diagnostics(&base_path, visited, true)?;
            parsed
                .diagnostics
                .extend(base_parsed.diagnostics.into_iter().filter(|d| {
                    d.code
                        != diagnostic_codes::OPTION_HAS_BEEN_REMOVED_PLEASE_REMOVE_IT_FROM_YOUR_CONFIGURATION
                }));
            // Removed-but-honored flags are file-scoped semantics: once any file
            // in the chain sets them, the merged config must honor them.
            parsed.suppress_excess_property_errors |= base_parsed.suppress_excess_property_errors;
            parsed.suppress_implicit_any_index_errors |=
                base_parsed.suppress_implicit_any_index_errors;
            parsed.no_implicit_use_strict |= base_parsed.no_implicit_use_strict;
            accumulated = Some(match accumulated {
                Some(acc) => merge_configs(acc, base_parsed.config),
                None => base_parsed.config,
            });
        }
        if let Some(base) = accumulated {
            parsed.config = merge_configs(base, parsed.config);
        }

        // TS5102: When verbatimModuleSyntax is set in the child config and base configs
        // contain removed options that it replaces, TSC emits TS5102 at the child's
        // `compilerOptions` key position for each replaced option (matching tsc's
        // anchor on the property whose presence introduces the removed-option
        // surface, not on `verbatimModuleSyntax` itself).
        let stripped = strip_jsonc(&source);
        let child_has_vms = stripped.contains("\"verbatimModuleSyntax\"");
        if child_has_vms && !base_removed_options.is_empty() {
            // Anchor at the child's `compilerOptions` key, matching tsc's
            // `/tsconfig.json(L,C): error TS5102 …` baseline output.
            let key = "compilerOptions";
            let start = stripped
                .find(&format!("\"{key}\""))
                .map(|p| p as u32)
                .unwrap_or(0);
            let key_len = key.len() as u32 + 2;
            for opt_name in &base_removed_options {
                let msg = format_message(
                    diagnostic_messages::OPTION_HAS_BEEN_REMOVED_PLEASE_REMOVE_IT_FROM_YOUR_CONFIGURATION,
                    &[opt_name],
                );
                parsed.diagnostics.push(Diagnostic::error(
                    &file_display,
                    start,
                    key_len,
                    msg,
                    diagnostic_codes::OPTION_HAS_BEEN_REMOVED_PLEASE_REMOVE_IT_FROM_YOUR_CONFIGURATION,
                ));
            }
        }
    }

    if config_ignore_deprecations_silences_6_0(&parsed.config) {
        parsed
            .diagnostics
            .retain(|diag| !is_ts60_deprecation_diagnostic_code(diag.code));
    }

    visited.remove(&canonical);
    Ok(parsed)
}

/// Collect removed compiler option names from a config file (and its base configs).
/// Used to detect removed options inherited via `extends` for TS5102 diagnostics.
fn collect_removed_options_from_config(path: &Path, removed: &mut Vec<String>) {
    let Ok(source) = std::fs::read_to_string(path) else {
        return;
    };
    let normalized = normalize_jsonc(&source);
    let Ok(raw) = serde_json::from_str::<serde_json::Value>(&normalized) else {
        return;
    };
    if let Some(compiler_opts) = raw
        .as_object()
        .and_then(|o| o.get("compilerOptions"))
        .and_then(|v| v.as_object())
    {
        for key in compiler_opts.keys() {
            if removed_compiler_option(key).is_some() {
                // Only include if the value is actually set (non-null, non-false)
                let is_set = match compiler_opts.get(key) {
                    Some(serde_json::Value::Bool(b)) => *b,
                    Some(serde_json::Value::String(s)) => !s.is_empty(),
                    Some(serde_json::Value::Null) | None => false,
                    Some(_) => true,
                };
                if is_set {
                    removed.push(key.clone());
                }
            }
        }
    }
    // Also check base configs recursively
    if let Some(extends) = raw
        .as_object()
        .and_then(|o| o.get("extends"))
        .and_then(|v| v.as_str())
        && let Ok(base_path) = resolve_extends_path(path, extends)
    {
        collect_removed_options_from_config(&base_path, removed);
    }
}

fn parse_script_target(value: &str) -> Result<ScriptTarget> {
    reject_comma_separated_option(value, "target")?;
    ScriptTarget::from_ts_str(value)
        .ok_or_else(|| anyhow!("unsupported compilerOptions.target '{value}'"))
}

fn parse_new_line_kind(value: &str) -> Result<NewLineKind> {
    reject_comma_separated_option(value, "newLine")?;
    match value.to_ascii_lowercase().as_str() {
        "lf" => Ok(NewLineKind::LineFeed),
        "crlf" => Ok(NewLineKind::CarriageReturnLineFeed),
        _ => Err(anyhow!("unsupported compilerOptions.newLine '{value}'")),
    }
}

fn parse_module_kind(value: &str) -> Result<ModuleKind> {
    reject_comma_separated_option(value, "module")?;
    ModuleKind::from_ts_str(value)
        .ok_or_else(|| anyhow!("unsupported compilerOptions.module '{value}'"))
}

fn parse_module_resolution(value: &str) -> Result<ModuleResolutionKind> {
    reject_comma_separated_option(value, "moduleResolution")?;
    ModuleResolutionKind::from_ts_str(value)
        .ok_or_else(|| anyhow!("unsupported compilerOptions.moduleResolution '{value}'"))
}

fn reject_comma_separated_option(value: &str, option_name: &str) -> Result<()> {
    if value.contains(',') {
        bail!("unsupported compilerOptions.{option_name} '{value}'");
    }
    Ok(())
}

fn parse_jsx_emit(value: &str) -> Result<JsxEmit> {
    let normalized = normalize_option(value);
    let jsx = match normalized.as_str() {
        "preserve" => JsxEmit::Preserve,
        "react" => JsxEmit::React,
        "react-jsx" | "reactjsx" => JsxEmit::ReactJsx,
        "react-jsxdev" | "reactjsxdev" => JsxEmit::ReactJsxDev,
        "reactnative" | "react-native" => JsxEmit::ReactNative,
        _ => bail!("unsupported compilerOptions.jsx '{value}'"),
    };

    Ok(jsx)
}

/// Parse a raw `jsx` compiler-option string (e.g. `"react-jsx"`, `"4"`) into
/// the corresponding [`JsxMode`][tsz_common::checker_options::JsxMode].
/// Returns `None` when the string is unrecognised.
pub fn jsx_string_to_mode(value: &str) -> Option<tsz_common::checker_options::JsxMode> {
    parse_jsx_emit(value).ok().map(jsx_emit_to_mode)
}

const fn jsx_emit_to_mode(emit: JsxEmit) -> tsz_common::checker_options::JsxMode {
    use tsz_common::checker_options::JsxMode;
    match emit {
        JsxEmit::Preserve => JsxMode::Preserve,
        JsxEmit::React => JsxMode::React,
        JsxEmit::ReactJsx => JsxMode::ReactJsx,
        JsxEmit::ReactJsxDev => JsxMode::ReactJsxDev,
        JsxEmit::ReactNative => JsxMode::ReactNative,
    }
}

fn build_path_mappings(paths: &FxHashMap<String, Vec<String>>) -> Vec<PathMapping> {
    let mut mappings = Vec::new();
    for (pattern, targets) in paths {
        if targets.is_empty() {
            continue;
        }
        let pattern = normalize_path_pattern(pattern);
        let targets = targets
            .iter()
            .map(|target| normalize_path_pattern(target))
            .collect();
        let (prefix, suffix) = split_path_pattern(&pattern);
        mappings.push(PathMapping {
            pattern,
            prefix,
            suffix,
            targets,
        });
    }
    mappings.sort_by(|left, right| {
        right
            .specificity()
            .cmp(&left.specificity())
            .then_with(|| right.pattern.len().cmp(&left.pattern.len()))
            .then_with(|| left.pattern.cmp(&right.pattern))
    });
    mappings
}

fn normalize_path_pattern(value: &str) -> String {
    value.trim().replace('\\', "/")
}

fn split_path_pattern(pattern: &str) -> (String, String) {
    match pattern.find('*') {
        Some(star_idx) => {
            let (prefix, rest) = pattern.split_at(star_idx);
            (prefix.to_string(), rest[1..].to_string())
        }
        None => (pattern.to_string(), String::new()),
    }
}

/// Resolve lib files from names, optionally following `/// <reference lib="..." />` directives.
///
/// When `follow_references` is true, each lib file is scanned for reference directives
/// and those referenced libs are also loaded. When false, only the explicitly listed
/// libs are loaded without following their internal references.
///
/// TypeScript always follows `/// <reference lib="..." />` directives when loading libs.
/// For example, `lib.dom.d.ts` references `es2015` and `es2018.asynciterable`, so even
/// `--target es5` (which loads lib.d.ts -> dom) transitively loads ES2015 features.
/// Verified with `tsc 6.0.0-dev --target es5 --listFiles`.
pub fn resolve_lib_files_with_options(
    lib_list: &[String],
    follow_references: bool,
) -> Result<Vec<PathBuf>> {
    resolve_lib_files_with_options_inner(lib_list, follow_references, true)
}

/// Like `resolve_lib_files_with_options` but treats the input list as transitive:
/// unknown lib names are silently skipped instead of erroring. Use this for libs
/// pulled in from `/// <reference lib="..." />` directives in user source files,
/// where the lib catalog may have drifted across TS versions (e.g. rxjs still
/// references the long-renamed `esnext.asynciterable`).
pub fn resolve_lib_files_with_options_transitive(
    lib_list: &[String],
    follow_references: bool,
) -> Result<Vec<PathBuf>> {
    resolve_lib_files_with_options_inner(lib_list, follow_references, false)
}

fn resolve_lib_files_with_options_inner(
    lib_list: &[String],
    follow_references: bool,
    initial_is_required: bool,
) -> Result<Vec<PathBuf>> {
    if lib_list.is_empty() {
        return Ok(Vec::new());
    }

    if should_use_embedded_libs() {
        return resolve_lib_files_from_embedded_inner(
            lib_list,
            follow_references,
            initial_is_required,
        );
    }

    match default_lib_dir() {
        Ok(lib_dir) => resolve_lib_files_from_dir_inner(
            lib_list,
            follow_references,
            initial_is_required,
            &lib_dir,
        ),
        Err(_) => {
            resolve_lib_files_from_embedded_inner(lib_list, follow_references, initial_is_required)
        }
    }
}

pub fn resolve_lib_files_from_dir_with_options(
    lib_list: &[String],
    follow_references: bool,
    lib_dir: &Path,
) -> Result<Vec<PathBuf>> {
    resolve_lib_files_from_dir_inner(lib_list, follow_references, true, lib_dir)
}

fn resolve_lib_files_from_dir_inner(
    lib_list: &[String],
    follow_references: bool,
    initial_is_required: bool,
    lib_dir: &Path,
) -> Result<Vec<PathBuf>> {
    if lib_list.is_empty() {
        return Ok(Vec::new());
    }

    let lib_map = build_lib_map(lib_dir)?;
    let mut resolved = Vec::new();
    // (lib_name, is_initial) — initial entries come from user compilerOptions.lib
    // and must resolve; transitive entries come from `/// <reference lib="..." />`
    // directives inside lib files (or user sources) and are skipped silently when
    // unknown, matching `tsc` behavior. This matters in practice: rxjs and other
    // older libraries reference libs like `esnext.asynciterable` that have since
    // been renamed/folded into newer lib names.
    let mut pending: VecDeque<(String, bool)> = lib_list
        .iter()
        .map(|value| (normalize_lib_name(value), initial_is_required))
        .collect();
    let mut visited = FxHashSet::default();

    while let Some((lib_name, is_required)) = pending.pop_front() {
        if lib_name.is_empty() || !visited.insert(lib_name.clone()) {
            continue;
        }

        let path = match lib_map.get(&lib_name) {
            Some(path) => path.clone(),
            None => {
                if is_required {
                    return Err(anyhow!(
                        "unsupported compilerOptions.lib '{}' (not found in {})",
                        lib_name,
                        lib_dir.display()
                    ));
                }
                continue;
            }
        };
        resolved.push(path.clone());

        // Only follow /// <reference lib="..." /> directives if requested
        if follow_references {
            let contents = std::fs::read_to_string(&path)
                .with_context(|| format!("failed to read lib file {}", path.display()))?;
            for reference in extract_lib_references(&contents) {
                pending.push_back((reference, false));
            }
        }
    }

    Ok(resolved)
}

/// Resolve lib files from names, following `/// <reference lib="..." />` directives.
/// This is used when explicitly specifying libs via `--lib`.
///
/// Applies tsc-compatible aliases: `es6` → `es2015`, `es7` → `es2016`.
/// In tsc, `--lib es6` maps to `lib.es2015.d.ts` (NOT `lib.es6.d.ts`).
/// `lib.es6.d.ts` is a "full" umbrella that includes DOM/scripthost references,
/// while `lib.es2015.d.ts` only includes ES2015 language features.
/// This aliasing only applies to explicit `--lib` (not `--target`-derived defaults).
pub fn resolve_lib_files(lib_list: &[String]) -> Result<Vec<PathBuf>> {
    let aliased = apply_explicit_lib_aliases(lib_list);
    resolve_lib_files_with_options(&aliased, true)
}

pub fn resolve_lib_files_from_dir(lib_list: &[String], lib_dir: &Path) -> Result<Vec<PathBuf>> {
    let aliased = apply_explicit_lib_aliases(lib_list);
    resolve_lib_files_from_dir_with_options(&aliased, true, lib_dir)
}

/// Apply tsc-compatible aliases for user-supplied `--lib` names.
///
/// In tsc's `commandLineParser.ts`, the `libs` array maps:
/// - `es6` → `lib.es2015.d.ts`
/// - `es7` → `lib.es2016.d.ts`
///
/// This is NOT applied for `--target`-derived default libs, where `es6`
/// correctly refers to `lib.es6.d.ts` (which includes DOM).
fn apply_explicit_lib_aliases(lib_list: &[String]) -> Vec<String> {
    lib_list
        .iter()
        .map(|name| match name.to_ascii_lowercase().as_str() {
            "es6" => "es2015".to_string(),
            "es7" => "es2016".to_string(),
            _ => name.clone(),
        })
        .collect()
}

/// Resolve default lib files for a given target.
///
/// Matches tsc's behavior exactly:
/// 1. Get the root lib file for the target (e.g., "lib" for ES5, "es2015.full" for ES2015)
/// 2. Follow ALL `/// <reference lib="..." />` directives recursively
///
/// This means `--target es5` loads lib.d.ts -> dom -> es2015 (transitively),
/// which is exactly what tsc does (verified with `tsc --target es5 --listFiles`).
pub fn resolve_default_lib_files(target: ScriptTarget) -> Result<Vec<PathBuf>> {
    let root_lib = default_lib_name_for_target(target);
    if should_use_embedded_libs() {
        return resolve_lib_files_from_embedded(&[root_lib.to_string()], true);
    }

    match default_lib_dir() {
        Ok(lib_dir) => resolve_default_lib_files_from_dir(target, &lib_dir),
        Err(_) => resolve_lib_files_from_embedded(&[root_lib.to_string()], true),
    }
}

pub fn resolve_default_lib_files_from_dir(
    target: ScriptTarget,
    lib_dir: &Path,
) -> Result<Vec<PathBuf>> {
    let root_lib = default_lib_name_for_target(target);
    // Use the raw (un-aliased) resolver — default libs from --target should
    // use lib.es6.d.ts (which includes DOM), not lib.es2015.d.ts.
    resolve_lib_files_from_dir_with_options(&[root_lib.to_string()], true, lib_dir)
}

/// Get the default lib name for a target.
///
/// This matches tsc's default behavior exactly:
/// - Each target loads the corresponding `.full` lib which includes:
///   - The ES version libs (e.g., es5, es2015.promise, etc.)
///   - DOM types (document, window, console, fetch, etc.)
///   - `ScriptHost` types
///
/// The mapping matches TypeScript's `getDefaultLibFileName()` in utilitiesPublic.ts:
/// - ES3/ES5 → lib.d.ts (npm) / es5.full.d.ts (source tree)
/// - ES2015  → lib.es6.d.ts (npm) / es2015.full.d.ts (source tree)
/// - ES2016+ → lib.es20XX.full.d.ts
/// - `ESNext`  → lib.esnext.full.d.ts
///
/// Note: The source tree uses `es5.full.d.ts` naming, while built TypeScript uses `lib.d.ts`.
/// We use the source tree naming since that's what exists in TypeScript/src/lib.
pub const fn default_lib_name_for_target(target: ScriptTarget) -> &'static str {
    match target {
        // ES3/ES5 -> lib.d.ts (npm) or es5.full.d.ts (source tree)
        ScriptTarget::ES3 | ScriptTarget::ES5 => "lib",
        // ES2015 -> lib.es6.d.ts (npm) or es2015.full.d.ts (source tree)
        ScriptTarget::ES2015 => "es6",
        // ES2016+ use .full variants (ES + DOM + ScriptHost + others)
        ScriptTarget::ES2016 => "es2016.full",
        ScriptTarget::ES2017 => "es2017.full",
        ScriptTarget::ES2018 => "es2018.full",
        ScriptTarget::ES2019 => "es2019.full",
        ScriptTarget::ES2020 => "es2020.full",
        ScriptTarget::ES2021 => "es2021.full",
        ScriptTarget::ES2022 => "es2022.full",
        ScriptTarget::ES2023 => "es2023.full",
        ScriptTarget::ES2024 => "es2024.full",
        // ES2025 and ESNext use esnext.full which includes experimental features
        ScriptTarget::ES2025 | ScriptTarget::ESNext => "esnext.full",
    }
}

/// Get the core lib name for a target (without DOM/ScriptHost).
///
/// This is useful for conformance testing where:
/// 1. Tests don't need DOM types
/// 2. Core libs are smaller and faster to load
/// 3. Tests that need DOM should specify @lib: dom explicitly
pub const fn core_lib_name_for_target(target: ScriptTarget) -> &'static str {
    match target {
        ScriptTarget::ES3 | ScriptTarget::ES5 => "es5",
        ScriptTarget::ES2015 => "es2015",
        ScriptTarget::ES2016 => "es2016",
        ScriptTarget::ES2017 => "es2017",
        ScriptTarget::ES2018 => "es2018",
        ScriptTarget::ES2019 => "es2019",
        ScriptTarget::ES2020 => "es2020",
        ScriptTarget::ES2021 => "es2021",
        ScriptTarget::ES2022 => "es2022",
        ScriptTarget::ES2023
        | ScriptTarget::ES2024
        | ScriptTarget::ES2025
        | ScriptTarget::ESNext => "esnext",
    }
}

/// Get the default lib directory.
///
/// Searches in order:
/// 1. `TSZ_LIB_DIR` environment variable
/// 2. Relative to the executable
/// 3. Relative to current working directory
/// 4. `TypeScript/src/lib` in the source tree
///
/// Cache for `default_lib_dir()` result. The lib directory is determined by
/// environment variables and filesystem probing that don't change during a
/// process lifetime.
static DEFAULT_LIB_DIR_CACHE: std::sync::OnceLock<Result<PathBuf, String>> =
    std::sync::OnceLock::new();

fn should_use_embedded_libs() -> bool {
    if env::var_os("TSZ_LIB_DIR").is_some() {
        return false;
    }

    env::var_os("TSZ_USE_EMBEDDED_LIBS").is_some_and(|value| {
        let normalized = value.to_string_lossy().trim().to_ascii_lowercase();
        !matches!(normalized.as_str(), "" | "0" | "false" | "no" | "off")
    })
}

pub fn default_lib_dir() -> Result<PathBuf> {
    let cached =
        DEFAULT_LIB_DIR_CACHE.get_or_init(|| default_lib_dir_uncached().map_err(|e| e.to_string()));
    match cached {
        Ok(path) => Ok(path.clone()),
        Err(msg) => bail!("{msg}"),
    }
}

fn default_lib_dir_uncached() -> Result<PathBuf> {
    if let Some(dir) = env::var_os("TSZ_LIB_DIR") {
        let dir = PathBuf::from(dir);
        if !dir.is_dir() {
            bail!(
                "TSZ_LIB_DIR does not point to a directory: {}",
                dir.display()
            );
        }
        return Ok(canonicalize_or_owned(&dir));
    }

    if let Some(dir) = lib_dir_from_exe() {
        return Ok(dir);
    }

    if let Some(dir) = lib_dir_from_cwd() {
        return Ok(dir);
    }

    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    if let Some(dir) = lib_dir_from_root(manifest_dir) {
        return Ok(dir);
    }

    // If manifest dir is a crate under a workspace, also check ancestor dirs
    // (e.g., crates/tsz-core/ -> repo root where TypeScript/ lives)
    let mut ancestor = manifest_dir.parent();
    while let Some(dir) = ancestor {
        if let Some(found) = lib_dir_from_root(dir) {
            return Ok(found);
        }
        ancestor = dir.parent();
    }

    bail!("lib directory not found under {}", manifest_dir.display());
}

fn lib_dir_from_exe() -> Option<PathBuf> {
    let exe = env::current_exe().ok()?;
    let exe_dir = exe.parent()?;
    let candidate = exe_dir.join("lib");
    if candidate.is_dir() {
        return Some(canonicalize_or_owned(&candidate));
    }
    lib_dir_from_root(exe_dir)
}

fn lib_dir_from_cwd() -> Option<PathBuf> {
    let cwd = env::current_dir().ok()?;
    lib_dir_from_root(&cwd)
}

fn lib_dir_from_root(root: &Path) -> Option<PathBuf> {
    let candidates = [
        // Built/compiled libs from tsc build output (highest priority)
        root.join("TypeScript").join("built").join("local"),
        root.join("TypeScript").join("lib"),
        // npm-installed TypeScript libs (self-contained, matching tsc's shipped format).
        // Prefer these over TypeScript/src/lib which has source-format files with
        // cross-module /// <reference lib> directives that pull in ES2015+ content
        // even for ES5 targets (e.g., dom.generated.d.ts references es2015.symbol.d.ts).
        root.join("node_modules").join("typescript").join("lib"),
        root.join("scripts")
            .join("node_modules")
            .join("typescript")
            .join("lib"),
        root.join("scripts")
            .join("emit")
            .join("node_modules")
            .join("typescript")
            .join("lib"),
        // Bundled lib snapshot committed with tsz for standalone and test environments.
        root.join("crates")
            .join("tsz-website")
            .join("src")
            .join("lib"),
        root.join("TypeScript").join("src").join("lib"),
        root.join("TypeScript")
            .join("node_modules")
            .join("typescript")
            .join("lib"),
        root.join("tests").join("lib"),
    ];

    for candidate in candidates {
        if candidate.is_dir() {
            return Some(canonicalize_or_owned(&candidate));
        }
    }

    None
}

/// Sentinel directory used for embedded lib paths when no physical lib directory exists.
/// The parallel pipeline checks basenames against embedded libs, so the directory
/// component is irrelevant — it just needs to be a valid path prefix.
const EMBEDDED_LIB_DIR: &str = "/embedded-lib";

/// Resolve lib files using embedded (compiled-in) lib content.
/// Fallback when no physical TypeScript lib directory is available.
fn resolve_lib_files_from_embedded(
    lib_list: &[String],
    follow_references: bool,
) -> Result<Vec<PathBuf>> {
    resolve_lib_files_from_embedded_inner(lib_list, follow_references, true)
}

fn resolve_lib_files_from_embedded_inner(
    lib_list: &[String],
    follow_references: bool,
    initial_is_required: bool,
) -> Result<Vec<PathBuf>> {
    if lib_list.is_empty() {
        return Ok(Vec::new());
    }

    let lib_map = build_lib_map_from_embedded();
    let embedded_dir = Path::new(EMBEDDED_LIB_DIR);
    let mut resolved = Vec::new();
    // See sibling `resolve_lib_files_from_dir_with_options` for why transitive
    // references are skipped silently when unknown.
    let mut pending: VecDeque<(String, bool)> = lib_list
        .iter()
        .map(|value| (normalize_lib_name(value), initial_is_required))
        .collect();
    let mut visited = FxHashSet::default();

    while let Some((lib_name, is_required)) = pending.pop_front() {
        if lib_name.is_empty() || !visited.insert(lib_name.clone()) {
            continue;
        }

        let filename = match lib_map.get(lib_name.as_str()) {
            Some(f) => *f,
            None => {
                if is_required {
                    return Err(anyhow!(
                        "unsupported compilerOptions.lib '{lib_name}' (not found in embedded libs)",
                    ));
                }
                continue;
            }
        };
        resolved.push(embedded_dir.join(filename));

        if follow_references && let Some(content) = crate::embedded_libs::get_lib_content(filename)
        {
            for reference in extract_lib_references(content) {
                pending.push_back((reference, false));
            }
        }
    }

    Ok(resolved)
}

/// Build a lib-name → filename map from embedded libs.
/// Mirrors `build_lib_map` but uses compiled-in filenames instead of directory listing.
fn build_lib_map_from_embedded() -> FxHashMap<&'static str, &'static str> {
    let mut map = FxHashMap::default();
    for filename in crate::embedded_libs::all_lib_filenames() {
        if !filename.ends_with(".d.ts") {
            continue;
        }
        let stem = filename.trim_end_matches(".d.ts");
        let stem = stem.strip_suffix(".generated").unwrap_or(stem);
        let key = stem.strip_prefix("lib.").unwrap_or(stem);
        map.insert(key, filename);
    }
    // Add fallback aliases for source tree naming (no lib.d.ts or lib.es6.d.ts):
    //   "lib" -> es5.full.d.ts, "es6" -> es2015.full.d.ts
    if !map.contains_key("lib")
        && let Some(&es5_full) = map.get("es5.full")
    {
        map.insert("lib", es5_full);
    }
    if !map.contains_key("es6")
        && let Some(&es2015_full) = map.get("es2015.full")
    {
        map.insert("es6", es2015_full);
    }
    // Apply tsc's backward-compatibility lib aliases (see `legacy_lib_aliases`
    // and the file-based `build_lib_map_uncached` for the full rationale).
    for (alias, target) in legacy_lib_aliases() {
        if !map.contains_key(*alias)
            && let Some(&filename) = map.get(*target)
        {
            map.insert(*alias, filename);
        }
    }
    map
}

/// Cache for `build_lib_map` results. The lib directory is typically resolved
/// once per process and the same map is reused for all lib resolution calls.
/// Without caching, `build_lib_map` was called once per lib being resolved,
/// each time re-reading the directory and calling `realpath` on every `.d.ts`
/// file (~110 files). This dominated total compilation time (>90% on macOS).
///
/// This is intentionally immutable after first initialization to avoid mutable
/// process-wide cache state in config loading paths.
type LibMapEntry = (PathBuf, FxHashMap<String, PathBuf>);
static LIB_MAP_CACHE: std::sync::OnceLock<LibMapEntry> = std::sync::OnceLock::new();

fn build_lib_map(lib_dir: &Path) -> Result<FxHashMap<String, PathBuf>> {
    // Fast path: return cached map if lib_dir matches
    if let Some((cached_dir, cached_map)) = LIB_MAP_CACHE.get()
        && cached_dir == lib_dir
    {
        return Ok(cached_map.clone());
    }

    let map = build_lib_map_uncached(lib_dir)?;

    // Cache first successful result. If another directory seeded the cache
    // earlier, we still return the freshly computed map for this call.
    let _ = LIB_MAP_CACHE.set((lib_dir.to_path_buf(), map.clone()));

    Ok(map)
}

fn build_lib_map_uncached(lib_dir: &Path) -> Result<FxHashMap<String, PathBuf>> {
    let mut map = FxHashMap::default();
    for entry in std::fs::read_dir(lib_dir)
        .with_context(|| format!("failed to read lib directory {}", lib_dir.display()))?
    {
        let entry = entry?;
        let path = entry.path();
        let Some(file_name) = path.file_name().and_then(|name| name.to_str()) else {
            continue;
        };
        if !file_name.ends_with(".d.ts") {
            continue;
        }

        let stem = file_name.trim_end_matches(".d.ts");
        let stem = stem.strip_suffix(".generated").unwrap_or(stem);
        let key = normalize_lib_name(stem);
        map.insert(key, canonicalize_or_owned(&path));
    }

    // In TypeScript source tree (v6+), `lib.d.ts` and `lib.es6.d.ts` don't exist.
    // Add fallback aliases so that default target lib names resolve correctly:
    //   "lib" (ES5 default) -> es5.full.d.ts
    //   "es6" (ES2015 default) -> es2015.full.d.ts
    if !map.contains_key("lib")
        && let Some(path) = map.get("es5.full").cloned()
    {
        map.insert("lib".to_string(), path);
    }
    if !map.contains_key("es6")
        && let Some(path) = map.get("es2015.full").cloned()
    {
        map.insert("es6".to_string(), path);
    }

    // Apply tsc's backward-compatibility aliases for libs that were renamed
    // when their feature stabilized out of esnext. Source: TypeScript's
    // `libEntries` array in `compiler/commandLineParser.ts`. Old code that
    // still says `/// <reference lib="esnext.asynciterable" />` (e.g. rxjs)
    // must keep working.
    for (alias, target) in legacy_lib_aliases() {
        if !map.contains_key(*alias)
            && let Some(path) = map.get(*target).cloned()
        {
            map.insert((*alias).to_string(), path);
        }
    }

    Ok(map)
}

/// Backward-compat lib name aliases applied at lookup time.
/// Mirrors the tail of tsc's `libEntries` table (the "Fallback for backward
/// compatibility" block).
const fn legacy_lib_aliases() -> &'static [(&'static str, &'static str)] {
    &[
        ("es6", "es2015"),
        ("es7", "es2016"),
        ("esnext.asynciterable", "es2018.asynciterable"),
        ("esnext.symbol", "es2019.symbol"),
        ("esnext.bigint", "es2020.bigint"),
        ("esnext.weakref", "es2021.weakref"),
        ("esnext.object", "es2024.object"),
        ("esnext.regexp", "es2024.regexp"),
        ("esnext.string", "es2024.string"),
        ("esnext.float16", "es2025.float16"),
        ("esnext.iterator", "es2025.iterator"),
        ("esnext.promise", "es2025.promise"),
    ]
}

/// Extract /// <reference lib="..." /> directives from a source file.
/// Returns a list of normalized referenced lib names.
pub fn extract_lib_references(source: &str) -> Vec<String> {
    extract_lib_references_with_positions(source)
        .into_iter()
        .map(|reference| normalize_lib_name(&reference.raw))
        .collect()
}

/// A `/// <reference lib="..." />` directive captured from a source file,
/// with the byte position of the `lib` attribute value. The raw value is
/// returned exactly as it appeared between the quotes (including empty),
/// so callers can render `tsc`-compatible diagnostics like
/// `Cannot find lib definition for '<value>'.`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LibReference {
    /// Raw, un-normalized lib attribute value.
    pub raw: String,
    /// Byte offset within the source where the value starts (immediately
    /// after the opening quote).
    pub start: u32,
    /// Byte length of the raw value (zero for an empty `lib=""`).
    pub length: u32,
}

/// Like [`extract_lib_references`], but returns the original (un-normalized)
/// lib values together with their byte position in the source. Used by the
/// driver to report `TS2726` for invalid user-authored source-file
/// directives while still feeding the transitive lib resolver.
pub fn extract_lib_references_with_positions(source: &str) -> Vec<LibReference> {
    let mut refs = Vec::new();
    let mut in_block_comment = false;
    let bytes = source.as_bytes();
    let mut line_start: usize = 0;
    loop {
        let line_end = bytes[line_start..]
            .iter()
            .position(|&b| b == b'\n')
            .map_or(bytes.len(), |idx| line_start + idx);
        let line_with_cr = &source[line_start..line_end];
        let line = line_with_cr.strip_suffix('\r').unwrap_or(line_with_cr);
        let trimmed = line.trim_start();
        let trim_offset = line.len() - trimmed.len();
        let trimmed_abs = line_start + trim_offset;

        if in_block_comment {
            if trimmed.contains("*/") {
                in_block_comment = false;
            }
        } else if trimmed.starts_with("/*") {
            if !trimmed.contains("*/") {
                in_block_comment = true;
            }
        } else if trimmed.is_empty() {
            // skip blank line
        } else if trimmed.starts_with("///") {
            if let Some((value, value_offset_in_trimmed)) =
                parse_reference_lib_value_with_offset(trimmed)
            {
                refs.push(LibReference {
                    raw: value.to_string(),
                    start: (trimmed_abs + value_offset_in_trimmed) as u32,
                    length: value.len() as u32,
                });
            }
        } else if trimmed.starts_with("//") {
            // skip non-triple-slash comment
        } else {
            break;
        }

        if line_end >= bytes.len() {
            break;
        }
        line_start = line_end + 1;
    }
    refs
}

fn parse_reference_lib_value_with_offset(line: &str) -> Option<(&str, usize)> {
    let mut offset = 0;
    let bytes = line.as_bytes();
    while let Some(idx) = line[offset..].find("lib=") {
        let start = offset + idx;
        if start > 0 {
            let prev = bytes[start - 1];
            if !prev.is_ascii_whitespace() && prev != b'<' {
                offset = start + 4;
                continue;
            }
        }
        let quote = *bytes.get(start + 4)?;
        if quote != b'"' && quote != b'\'' {
            offset = start + 4;
            continue;
        }
        let value_start = start + 5;
        let rest = &line[value_start..];
        let end = rest.find(quote as char)?;
        return Some((&rest[..end], value_start));
    }
    None
}

/// Returns whether `lib_name` resolves to a known TypeScript library file.
///
/// Mirrors the resolution order used by `resolve_lib_files_with_options`:
/// the on-disk lib directory (when discoverable) takes precedence over the
/// embedded fallback. Empty inputs always return `false`. Only intended for
/// validation paths that need a yes/no answer without loading the file.
pub fn is_known_lib_name(lib_name: &str) -> bool {
    let normalized = normalize_lib_name(lib_name);
    if normalized.is_empty() {
        return false;
    }
    if let Ok(lib_dir) = default_lib_dir()
        && let Ok(map) = build_lib_map(&lib_dir)
    {
        return map.contains_key(&normalized);
    }
    build_lib_map_from_embedded().contains_key(normalized.as_str())
}

fn normalize_lib_name(value: &str) -> String {
    let normalized = value.trim().to_ascii_lowercase();
    normalized
        .strip_prefix("lib.")
        .unwrap_or(normalized.as_str())
        .to_string()
}

/// Convert emitter `ScriptTarget` to checker `ScriptTarget`.
/// The emitter has more variants (`ES2021`, `ES2022`) which map to `ESNext` in the checker.
pub const fn checker_target_from_emitter(target: ScriptTarget) -> CheckerScriptTarget {
    match target {
        ScriptTarget::ES3 => CheckerScriptTarget::ES3,
        ScriptTarget::ES5 => CheckerScriptTarget::ES5,
        ScriptTarget::ES2015 => CheckerScriptTarget::ES2015,
        ScriptTarget::ES2016 => CheckerScriptTarget::ES2016,
        ScriptTarget::ES2017 => CheckerScriptTarget::ES2017,
        ScriptTarget::ES2018 => CheckerScriptTarget::ES2018,
        ScriptTarget::ES2019 => CheckerScriptTarget::ES2019,
        ScriptTarget::ES2020 => CheckerScriptTarget::ES2020,
        ScriptTarget::ES2021
        | ScriptTarget::ES2022
        | ScriptTarget::ES2023
        | ScriptTarget::ES2024
        | ScriptTarget::ES2025
        | ScriptTarget::ESNext => CheckerScriptTarget::ESNext,
    }
}

fn canonicalize_or_owned(path: &Path) -> PathBuf {
    std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
}

fn normalize_option(value: &str) -> String {
    let mut normalized = String::with_capacity(value.len());
    for ch in value.chars() {
        if ch == '-' || ch == '_' || ch.is_whitespace() {
            continue;
        }
        normalized.push(ch.to_ascii_lowercase());
    }
    normalized
}

fn normalize_enum_option_value(value: &str) -> String {
    value.trim().to_ascii_lowercase()
}

pub fn strip_jsonc(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut chars = input.chars().peekable();
    let mut in_string = false;
    let mut escape = false;
    let mut in_line_comment = false;
    let mut in_block_comment = false;

    while let Some(ch) = chars.next() {
        if in_line_comment {
            if ch == '\n' {
                in_line_comment = false;
                out.push(ch);
            }
            continue;
        }

        if in_block_comment {
            if ch == '*' {
                if let Some('/') = chars.peek().copied() {
                    chars.next();
                    in_block_comment = false;
                }
            } else if ch == '\n' {
                out.push(ch);
            }
            continue;
        }

        if in_string {
            out.push(ch);
            if escape {
                escape = false;
            } else if ch == '\\' {
                escape = true;
            } else if ch == '"' {
                in_string = false;
            }
            continue;
        }

        if ch == '"' {
            in_string = true;
            out.push(ch);
            continue;
        }

        if ch == '/'
            && let Some(&next) = chars.peek()
        {
            if next == '/' {
                chars.next();
                in_line_comment = true;
                continue;
            }
            if next == '*' {
                chars.next();
                in_block_comment = true;
                continue;
            }
        }

        out.push(ch);
    }

    out
}

/// Convert tsconfig-style JSONC into strict JSON by removing comments and
/// trailing commas while preserving string contents.
pub fn normalize_jsonc(input: &str) -> String {
    let stripped = strip_jsonc(input);
    remove_trailing_commas(&stripped)
}

fn remove_trailing_commas(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut chars = input.chars().peekable();
    let mut in_string = false;
    let mut escape = false;

    while let Some(ch) = chars.next() {
        if in_string {
            out.push(ch);
            if escape {
                escape = false;
            } else if ch == '\\' {
                escape = true;
            } else if ch == '"' {
                in_string = false;
            }
            continue;
        }

        if ch == '"' {
            in_string = true;
            out.push(ch);
            continue;
        }

        if ch == ',' {
            let mut lookahead = chars.clone();
            while let Some(next) = lookahead.peek().copied() {
                if next.is_whitespace() {
                    lookahead.next();
                    continue;
                }
                if next == '}' || next == ']' {
                    break;
                }
                break;
            }

            if let Some(next) = lookahead.peek().copied()
                && (next == '}' || next == ']')
            {
                continue;
            }
        }

        out.push(ch);
    }

    out
}

// TS6046: Valid option value lists (lowercase canonical spellings, matching tsc 6.0)
// These must match the values tsc accepts and lists in its TS6046 messages.

/// Valid `--target` values. The display list uses tsc's canonical casing.
const VALID_TARGET_VALUES: &[&str] = &[
    "es3", "es5", "es6", "es2015", "es2016", "es2017", "es2018", "es2019", "es2020", "es2021",
    "es2022", "es2023", "es2024", "es2025", "esnext",
];
// TSC 7.0 no longer lists deprecated targets (es3, es5) in the error message
// and added es2025. Match TSC's display.
const VALID_TARGET_DISPLAY: &str = "'es6', 'es2015', 'es2016', 'es2017', 'es2018', 'es2019', 'es2020', 'es2021', 'es2022', 'es2023', 'es2024', 'es2025', 'esnext'";

/// Valid `--module` values.
const VALID_MODULE_VALUES: &[&str] = &[
    "none", "commonjs", "amd", "system", "umd", "es6", "es2015", "es2020", "es2022", "esnext",
    "node16", "node18", "node20", "nodenext", "preserve",
];
// TSC 7.0 no longer lists deprecated module kinds (none, amd, system, umd) in the error
// message, though they are still accepted. Match TSC's display.
const VALID_MODULE_DISPLAY: &str = "'commonjs', 'es6', 'es2015', 'es2020', 'es2022', 'esnext', 'node16', 'node18', 'node20', 'nodenext', 'preserve'";

/// Valid `--moduleResolution` values.
const VALID_MODULE_RESOLUTION_VALUES: &[&str] =
    &["classic", "node", "node10", "node16", "nodenext", "bundler"];
const VALID_MODULE_RESOLUTION_DISPLAY: &str =
    "'classic', 'node', 'node10', 'node16', 'nodenext', 'bundler'";

/// Valid `--jsx` values.
const VALID_JSX_VALUES: &[&str] = &[
    "preserve",
    "react",
    "react-native",
    "react-jsx",
    "react-jsxdev",
];
const VALID_JSX_DISPLAY: &str = "'preserve', 'react', 'react-native', 'react-jsx', 'react-jsxdev'";

/// Valid `--moduleDetection` values.
const VALID_MODULE_DETECTION_VALUES: &[&str] = &["auto", "legacy", "force"];
const VALID_MODULE_DETECTION_DISPLAY: &str = "'auto', 'legacy', 'force'";

/// Valid `--newLine` values.
const VALID_NEW_LINE_VALUES: &[&str] = &["crlf", "lf"];
const VALID_NEW_LINE_DISPLAY: &str = "'crlf', 'lf'";

/// Valid `watchOptions.watchFile` values. Mirrors tsc's
/// `WatchFileKind` enum spellings (lowercased for normalize-compare).
const VALID_WATCH_FILE_VALUES: &[&str] = &[
    "fixedpollinginterval",
    "prioritypollinginterval",
    "dynamicprioritypolling",
    "fixedchunksizepolling",
    "usefsevents",
    "usefseventsonparentdirectory",
];
const VALID_WATCH_FILE_DISPLAY: &str = "'fixedpollinginterval', 'prioritypollinginterval', 'dynamicprioritypolling', 'fixedchunksizepolling', 'usefsevents', 'usefseventsonparentdirectory'";

/// Valid `watchOptions.watchDirectory` values.
const VALID_WATCH_DIRECTORY_VALUES: &[&str] = &[
    "usefsevents",
    "fixedpollinginterval",
    "dynamicprioritypolling",
    "fixedchunksizepolling",
];
const VALID_WATCH_DIRECTORY_DISPLAY: &str =
    "'usefsevents', 'fixedpollinginterval', 'dynamicprioritypolling', 'fixedchunksizepolling'";

/// Valid `watchOptions.fallbackPolling` values.
const VALID_FALLBACK_POLLING_VALUES: &[&str] = &[
    "fixedinterval",
    "priorityinterval",
    "dynamicpriority",
    "fixedchunksize",
];
const VALID_FALLBACK_POLLING_DISPLAY: &str =
    "'fixedinterval', 'priorityinterval', 'dynamicpriority', 'fixedchunksize'";

/// Valid `--lib` values. This list matches tsc 6.0's accepted lib names.
const VALID_LIB_VALUES: &[&str] = &[
    "es5",
    "es6",
    "es2015",
    "es7",
    "es2016",
    "es2017",
    "es2018",
    "es2019",
    "es2020",
    "es2021",
    "es2022",
    "es2023",
    "es2024",
    "esnext",
    "dom",
    "dom.iterable",
    "dom.asynciterable",
    "webworker",
    "webworker.importscripts",
    "webworker.iterable",
    "webworker.asynciterable",
    "scripthost",
    "es2015.core",
    "es2015.collection",
    "es2015.generator",
    "es2015.iterable",
    "es2015.promise",
    "es2015.proxy",
    "es2015.reflect",
    "es2015.symbol",
    "es2015.symbol.wellknown",
    "es2016.array.include",
    "es2016.intl",
    "es2017.arraybuffer",
    "es2017.date",
    "es2017.object",
    "es2017.sharedmemory",
    "es2017.string",
    "es2017.intl",
    "es2017.typedarrays",
    "es2018.asyncgenerator",
    "es2018.asynciterable",
    "es2018.intl",
    "es2018.promise",
    "es2018.regexp",
    "es2019.array",
    "es2019.object",
    "es2019.string",
    "es2019.symbol",
    "es2019.intl",
    "es2020.bigint",
    "es2020.date",
    "es2020.promise",
    "es2020.sharedmemory",
    "es2020.string",
    "es2020.symbol.wellknown",
    "es2020.intl",
    "es2020.number",
    "es2021.promise",
    "es2021.string",
    "es2021.weakref",
    "es2021.intl",
    "es2022.array",
    "es2022.error",
    "es2022.intl",
    "es2022.object",
    "es2022.string",
    "es2022.regexp",
    "es2023.array",
    "es2023.collection",
    "es2023.intl",
    "es2024.arraybuffer",
    "es2024.collection",
    "es2024.object",
    "es2024.promise",
    "es2024.regexp",
    "es2024.sharedmemory",
    "es2024.string",
    "es2025",
    "es2025.collection",
    "es2025.float16",
    "es2025.intl",
    "es2025.iterator",
    "es2025.promise",
    "es2025.regexp",
    "esnext.array",
    "esnext.collection",
    "esnext.symbol",
    "esnext.asynciterable",
    "esnext.intl",
    "esnext.disposable",
    "esnext.bigint",
    "esnext.string",
    "esnext.promise",
    "esnext.weakref",
    "esnext.decorators",
    "esnext.object",
    "esnext.regexp",
    "esnext.iterator",
    "esnext.float16",
    "esnext.error",
    "esnext.sharedmemory",
    "esnext.date",
    "esnext.temporal",
    "decorators",
    "decorators.legacy",
];

const VALID_LIB_DISPLAY: &str = "'es5', 'es6', 'es2015', 'es7', 'es2016', 'es2017', 'es2018', 'es2019', 'es2020', 'es2021', 'es2022', 'es2023', 'es2024', 'esnext', 'dom', 'dom.iterable', 'dom.asynciterable', 'webworker', 'webworker.importscripts', 'webworker.iterable', 'webworker.asynciterable', 'scripthost', 'es2015.core', 'es2015.collection', 'es2015.generator', 'es2015.iterable', 'es2015.promise', 'es2015.proxy', 'es2015.reflect', 'es2015.symbol', 'es2015.symbol.wellknown', 'es2016.array.include', 'es2016.intl', 'es2017.arraybuffer', 'es2017.date', 'es2017.object', 'es2017.sharedmemory', 'es2017.string', 'es2017.intl', 'es2017.typedarrays', 'es2018.asyncgenerator', 'es2018.asynciterable', 'es2018.intl', 'es2018.promise', 'es2018.regexp', 'es2019.array', 'es2019.object', 'es2019.string', 'es2019.symbol', 'es2019.intl', 'es2020.bigint', 'es2020.date', 'es2020.promise', 'es2020.sharedmemory', 'es2020.string', 'es2020.symbol.wellknown', 'es2020.intl', 'es2020.number', 'es2021.promise', 'es2021.string', 'es2021.weakref', 'es2021.intl', 'es2022.array', 'es2022.error', 'es2022.intl', 'es2022.object', 'es2022.string', 'es2022.regexp', 'es2023.array', 'es2023.collection', 'es2023.intl', 'es2024.arraybuffer', 'es2024.collection', 'es2024.object', 'es2024.promise', 'es2024.regexp', 'es2024.sharedmemory', 'es2024.string', 'es2025', 'es2025.collection', 'es2025.float16', 'es2025.intl', 'es2025.iterator', 'es2025.promise', 'es2025.regexp', 'esnext.array', 'esnext.collection', 'esnext.symbol', 'esnext.asynciterable', 'esnext.intl', 'esnext.disposable', 'esnext.bigint', 'esnext.string', 'esnext.promise', 'esnext.weakref', 'esnext.decorators', 'esnext.object', 'esnext.regexp', 'esnext.iterator', 'esnext.float16', 'esnext.error', 'esnext.sharedmemory', 'esnext.date', 'esnext.temporal', 'decorators', 'decorators.legacy'";

/// Validate a single-value compiler option against a list of valid values.
/// If the value is invalid, emit TS6046 and null it out in the JSON object.
fn validate_option_value(
    compiler_opts: &mut serde_json::Map<String, serde_json::Value>,
    key: &str,
    source: &str,
    file_path: &str,
    valid_values: &[&str],
    option_flag: &str,
    display_list: &str,
    diagnostics: &mut Vec<Diagnostic>,
) {
    if let Some(serde_json::Value::String(value)) = compiler_opts.get(key) {
        let normalized = normalize_enum_option_value(value);
        if !normalized.is_empty() && !valid_values.contains(&normalized.as_str()) {
            let start = find_value_offset_in_source(source, key);
            let value_len = value.len() as u32 + 2; // include quotes
            let msg = format_message(
                diagnostic_messages::ARGUMENT_FOR_OPTION_MUST_BE,
                &[option_flag, display_list],
            );
            diagnostics.push(Diagnostic::error(
                file_path,
                start,
                value_len,
                msg,
                diagnostic_codes::ARGUMENT_FOR_OPTION_MUST_BE,
            ));
            // Null out the invalid value so resolve_compiler_options doesn't bail
            compiler_opts.insert(key.to_string(), serde_json::Value::Null);
        }
    }
}

/// Validate individual entries in the `lib` array option.
/// Invalid entries emit TS6046 and are removed from the array.
fn validate_lib_values(
    compiler_opts: &mut serde_json::Map<String, serde_json::Value>,
    source: &str,
    file_path: &str,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let Some(serde_json::Value::Array(lib_array)) = compiler_opts.get("lib") else {
        return;
    };

    // Collect invalid entries with their positions
    let mut invalid_indices = Vec::new();
    for (i, entry) in lib_array.iter().enumerate() {
        if let serde_json::Value::String(lib_name) = entry {
            let normalized = normalize_enum_option_value(lib_name);
            if !normalized.is_empty() && !VALID_LIB_VALUES.contains(&normalized.as_str()) {
                // Find position of this lib entry in source
                let start = find_lib_entry_offset(source, lib_name);
                let value_len = lib_name.len() as u32 + 2; // include quotes
                let msg = format_message(
                    diagnostic_messages::ARGUMENT_FOR_OPTION_MUST_BE,
                    &["--lib", VALID_LIB_DISPLAY],
                );
                diagnostics.push(Diagnostic::error(
                    file_path,
                    start,
                    value_len,
                    msg,
                    diagnostic_codes::ARGUMENT_FOR_OPTION_MUST_BE,
                ));
                invalid_indices.push(i);
            }
        }
    }

    // Remove invalid entries (in reverse order to preserve indices)
    if !invalid_indices.is_empty()
        && let Some(serde_json::Value::Array(lib_array)) = compiler_opts.get_mut("lib")
    {
        for &idx in invalid_indices.iter().rev() {
            lib_array.remove(idx);
        }
    }
}

/// Find the byte offset of a specific lib entry string within the source text.
/// Searches for `"entry"` within the lib array section.
fn find_lib_entry_offset(source: &str, entry: &str) -> u32 {
    let search = format!("\"{entry}\"");
    // Look for the lib array first
    let lib_pos = source.find("\"lib\"").unwrap_or(0);
    if let Some(pos) = source[lib_pos..].find(&search) {
        (lib_pos + pos) as u32
    } else {
        0
    }
}

#[cfg(test)]
mod tests;

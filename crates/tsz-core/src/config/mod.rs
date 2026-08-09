use anyhow::{Context, Result, anyhow, bail};
use serde::{Deserialize, Deserializer};

use rustc_hash::{FxHashMap, FxHashSet};
use std::path::{Path, PathBuf};

use crate::checker::context::ScriptTarget as CheckerScriptTarget;
use crate::checker::diagnostics::Diagnostic;
use crate::emitter::{ModuleKind, NewLineKind, ScriptTarget};
use tsz_common::diagnostics::data::{diagnostic_codes, diagnostic_messages};
use tsz_common::diagnostics::format_message;
use tsz_scanner::{is_ecmascript_identifier_part, is_ecmascript_identifier_start};
mod deprecation_helpers;
mod extends;
mod jsonc;
mod lib_offsets;
mod lib_resolution;

use extends::{
    anchor_extends_specifier, anchor_inherited_path_options, anchor_inherited_root_selectors,
    is_relative_or_absolute_extends_specifier, merge_configs, resolve_extends_path,
    substitute_config_dir_templates,
};
use jsonc::remove_trailing_commas;
use lib_offsets::find_lib_entry_offset;

pub use jsonc::{normalize_jsonc, strip_jsonc};

pub use lib_resolution::{
    LibReference, apply_explicit_lib_aliases, core_lib_name_for_target, default_lib_dir,
    default_lib_name_for_target, extract_lib_references, extract_lib_references_with_positions,
    is_known_lib_name, resolve_default_lib_files, resolve_default_lib_files_from_dir,
    resolve_lib_files, resolve_lib_files_from_dir, resolve_lib_files_from_dir_with_options,
    resolve_lib_files_from_embedded, resolve_lib_files_with_options,
    resolve_lib_files_with_options_transitive,
};
mod option_fanout;
mod parse;
mod resolved_options;

pub use option_fanout::apply_non_strict_fanout;
pub use parse::{
    ParsedTsConfig, RemovedOptionNotice, parse_tsconfig, parse_tsconfig_with_diagnostics,
    parse_tsconfig_with_diagnostics_deferred,
};
pub use resolved_options::{
    JsxEmit, ModuleResolutionKind, PathMapping, ResolvedCompilerOptions, apply_module_detection,
    default_module_detection_for_module, default_module_kind_for_target,
    default_module_resolution_for_module, derive_default_module_kind, resolve_compiler_options,
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
    /// Suppress deprecation warnings. Valid values: "5.0", "6.0", "7.0".
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

// Re-export the shared strict-family expansion table so every option-merge
// surface (CLI driver, tsconfig resolution, WASM, tsz-server) resolves the
// `--strict` umbrella through the same owner.
pub use tsz_common::options::strict_family;

// Re-export the shared checker-only non-strict fan-out so the printer-less
// surfaces (WASM, tsz-server) derive `verbatimModuleSyntax -> isolatedModules`
// and `esModuleInterop -> allowSyntheticDefaultImports` from the same owner as
// the emit-capable CLI/tsconfig lanes (which go through `apply_non_strict_fanout`).
pub use tsz_common::options::checker_fanout;

/// A string compiler-option value, but only when present and non-empty.
///
/// This is the single definition of the "empty string means unset" rule: `tsc`
/// gates every string option on `if (options.X)`, and `""` is falsy in that
/// gate, so an empty value is treated as if the option were absent while a
/// whitespace `" "` is a present value that still gets validated. Both
/// [`option_is_truthy`] (presence/dependency/conflict gates) and the identifier
/// validators that need the value itself route through here.
const fn nonempty_string_option(value: Option<&serde_json::Value>) -> Option<&String> {
    match value {
        Some(serde_json::Value::String(s)) if !s.is_empty() => Some(s),
        _ => None,
    }
}

/// Check whether a JSON value represents a truthy compiler option.
/// Returns false for `None`, `null`, `false`, and the empty string; true for
/// `true` and any other non-empty value (see [`nonempty_string_option`] for the
/// empty-string rationale).
const fn option_is_truthy(value: Option<&serde_json::Value>) -> bool {
    match value {
        None | Some(serde_json::Value::Null) => false,
        Some(serde_json::Value::Bool(b)) => *b,
        Some(serde_json::Value::String(_)) => nonempty_string_option(value).is_some(),
        Some(_) => true,
    }
}

fn option_is_effectively_enabled(
    compiler_opts: &serde_json::Map<String, serde_json::Value>,
    invalidated_options: &[String],
    key: &str,
) -> bool {
    if compiler_option_expected_type(key) == Some(CompilerOptionValueType::Boolean)
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

/// tsc's `getStrictOptionValue`: a strict-family member's effective value is
/// its own explicit value when it has one, and the `strict` umbrella's value
/// otherwise. An invalidated (TS5024) member is treated like an absent one, so
/// it still inherits `strict` — matching tsc, where a rejected option value
/// simply leaves the field `undefined`.
///
/// `strict` itself defaults to **true** in TypeScript 7 (it was false in 6.x),
/// which is what makes a lone `strictPropertyInitialization: true` legal:
/// `strictNullChecks` is absent, so it inherits an enabled `strict`, and the
/// TS5052 dependency is satisfied. `CheckerOptions`'s own defaults carry the
/// same `strict: true`.
///
/// This differs from [`option_is_effectively_enabled`], which reports the raw
/// per-key value and never consults `strict`. Option-consistency checks need
/// both: tsc tests the raw value of the *dependent* option and the strict-aware
/// value of the option it *depends on*.
fn strict_option_value(
    compiler_opts: &serde_json::Map<String, serde_json::Value>,
    invalidated_options: &[String],
    key: &str,
) -> bool {
    let explicit = |k: &str| {
        (compiler_opts.contains_key(k) && !invalidated_options.iter().any(|i| i == k))
            .then(|| option_is_truthy(compiler_opts.get(k)))
    };
    explicit(key).or_else(|| explicit("strict")).unwrap_or(true)
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
    let mut chars = s.chars();
    match chars.next() {
        Some(c) if is_ecmascript_identifier_start(c) => {}
        _ => return false,
    }
    chars.all(is_ecmascript_identifier_part)
}

/// Find the byte offset of a JSON key within the source text.
/// Searches for `"key"` after `compilerOptions`.
fn find_key_offset_in_source(source: &str, key: &str) -> u32 {
    find_key_offset_in_source_opt(source, key).unwrap_or(0)
}

/// Like [`find_key_offset_in_source`], but distinguishes "key absent" from
/// "key at offset 0" so callers can choose between several candidate keys.
fn find_key_offset_in_source_opt(source: &str, key: &str) -> Option<u32> {
    let search = format!("\"{key}\"");
    // Look for the key after "compilerOptions" to avoid matching in other sections
    let compiler_opts_pos = source.find("compilerOptions").unwrap_or(0);
    source[compiler_opts_pos..]
        .find(&search)
        // Point at the opening quote of the key, matching tsc behavior
        .map(|pos| (compiler_opts_pos + pos) as u32)
}

/// Report an option-interaction diagnostic that names two compiler options.
///
/// tsc walks the `compilerOptions` object literal looking for either name and
/// anchors on the **first one it encounters in source order**, emitting a
/// single diagnostic — regardless of which of the two the message names first.
/// A config listing `allowJs` above `checkJs` anchors at `allowJs` even though
/// TS5052 is "about" `checkJs`; a config listing `inlineSourceMap` above
/// `sourceMap` anchors at `inlineSourceMap` even though TS5053's message names
/// `sourceMap` first. If neither key is written out (both are implied), tsc
/// falls back to the enclosing `compilerOptions` key.
///
/// Shared by TS5052, TS5053 and TS5091, which previously each pushed a
/// diagnostic at *both* keys — a tsc 6.0 behavior
/// (`forEachPropertyAssignment` visiting both) that 7.0.2 no longer has.
fn push_first_key_anchored_diagnostic(
    diagnostics: &mut Vec<Diagnostic>,
    file_path: &str,
    stripped: &str,
    keys: [&str; 2],
    message: impl Into<String>,
    code: u32,
) {
    let anchor = keys
        .into_iter()
        .filter_map(|key| find_key_offset_in_source_opt(stripped, key).map(|off| (off, key)))
        .min_by_key(|(off, _)| *off);

    match anchor {
        Some((start, key)) => {
            diagnostics.push(Diagnostic::error(
                file_path,
                start,
                key.len() as u32 + 2, // include surrounding quotes
                message,
                code,
            ));
        }
        None => {
            let search = "\"compilerOptions\"";
            let start = stripped.find(search).map(|p| p as u32).unwrap_or(0);
            diagnostics.push(Diagnostic::error(
                file_path,
                start,
                search.len() as u32,
                message,
                code,
            ));
        }
    }
}

/// TS5052 "Option '{0}' cannot be specified without specifying option '{1}'".
/// The message always names the pair in dependency order; the anchor is chosen
/// by [`push_first_key_anchored_diagnostic`].
fn push_option_dependency_diagnostic(
    diagnostics: &mut Vec<Diagnostic>,
    file_path: &str,
    stripped: &str,
    dependent: &str,
    required: &str,
) {
    push_first_key_anchored_diagnostic(
        diagnostics,
        file_path,
        stripped,
        [dependent, required],
        format_message(
            diagnostic_messages::OPTION_CANNOT_BE_SPECIFIED_WITHOUT_SPECIFYING_OPTION,
            &[dependent, required],
        ),
        diagnostic_codes::OPTION_CANNOT_BE_SPECIFIED_WITHOUT_SPECIFYING_OPTION,
    );
}

/// Push an error diagnostic anchored at a tsconfig key, spanning the key text
/// plus its surrounding quotes.
///
/// Centralizes the `find_key_offset_in_source` + `key.len() + 2` quote-width
/// anchoring that config validation repeats at every key-anchored diagnostic
/// site, so an anchoring or off-by-one correction is a single-place change.
fn push_key_diagnostic(
    diagnostics: &mut Vec<Diagnostic>,
    file_path: &str,
    stripped: &str,
    key: &str,
    message: impl Into<String>,
    code: u32,
) {
    let start = find_key_offset_in_source(stripped, key);
    let length = key.len() as u32 + 2; // include surrounding quotes
    diagnostics.push(Diagnostic::error(file_path, start, length, message, code));
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

/// Locate the source span of an `extends` specifier string (including its
/// surrounding quotes) for anchoring TS6053 on an unresolved base config.
///
/// The search starts at the top-level `extends` key so an array element is
/// anchored at its own position; when the literal cannot be located (e.g. the
/// value was rewritten by JSONC stripping) it falls back to the `extends`
/// value start with the specifier's quoted width, matching `tsc`'s anchor on
/// the `extends` value.
fn find_extends_specifier_span(source: &str, specifier: &str) -> (u32, u32) {
    let quoted = format!("\"{specifier}\"");
    let extends_pos = source.find("\"extends\"").unwrap_or(0);
    if let Some(rel) = source[extends_pos..].find(&quoted) {
        ((extends_pos + rel) as u32, quoted.len() as u32)
    } else {
        (
            find_top_level_value_offset_in_source(source, "extends"),
            specifier.len() as u32 + 2,
        )
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

/// Matches TypeScript's `isRootedDiskPath` for `paths` substitution validation:
/// a POSIX root (`/...`), a UNC/backslash root (`\...`), or a drive-letter root
/// (`c:`, `c:/...`, `c:\...`).
const fn is_rooted_path_mapping_substitution(specifier: &str) -> bool {
    match specifier.as_bytes() {
        [b'/' | b'\\', ..] => true,
        [drive, b':', rest @ ..] => {
            drive.is_ascii_alphabetic() && matches!(rest, [] | [b'/' | b'\\', ..])
        }
        _ => false,
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum CompilerOptionValueType {
    Boolean,
    String,
    Enum,
    Number,
    Array,
    Object,
}

impl CompilerOptionValueType {
    const fn diagnostic_name(self) -> &'static str {
        match self {
            Self::Boolean => "boolean",
            Self::String => "string",
            Self::Enum => "enum",
            Self::Number => "number",
            Self::Array => "Array",
            Self::Object => "object",
        }
    }

    fn accepts(self, value: &serde_json::Value) -> bool {
        match self {
            Self::Boolean => value.is_boolean(),
            Self::String | Self::Enum => value.is_string(),
            Self::Number => value.is_number(),
            Self::Array => value.is_array(),
            Self::Object => value.is_object(),
        }
    }
}

/// Return the expected JSON value type for a recognized compiler option.
fn compiler_option_expected_type(key: &str) -> Option<CompilerOptionValueType> {
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
        | "noEmit"
        | "noEmitHelpers"
        | "noEmitOnError"
        | "noErrorTruncation"
        | "noFallthroughCasesInSwitch"
        | "noImplicitAny"
        | "noImplicitOverride"
        | "noImplicitReturns"
        | "noImplicitThis"
        | "noLib"
        | "libReplacement"
        | "noPropertyAccessFromIndexSignature"
        | "noResolve"
        | "noUncheckedIndexedAccess"
        | "noUncheckedSideEffectImports"
        | "noUnusedLocals"
        | "noUnusedParameters"
        | "preserveConstEnums"
        | "preserveSymlinks"
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
        | "traceResolution"
        | "useDefineForClassFields"
        | "useUnknownInCatchVariables"
        | "verbatimModuleSyntax" => Some(CompilerOptionValueType::Boolean),
        // Enum options are encoded as strings in JSON, but tsc identifies
        // their map-backed value domain as `enum` in TS5024.
        "jsx" | "module" | "moduleDetection" | "moduleResolution" | "newLine" | "target" => {
            Some(CompilerOptionValueType::Enum)
        }
        // String options
        "baseUrl"
        | "declarationDir"
        | "jsxFactory"
        | "jsxFragmentFactory"
        | "jsxImportSource"
        | "mapRoot"
        | "outDir"
        | "outFile"
        | "reactNamespace"
        | "rootDir"
        | "sourceRoot"
        | "tsBuildInfoFile"
        | "ignoreDeprecations"
        | "typesVersionsCompilerVersion" => Some(CompilerOptionValueType::String),
        // Number options
        "maxNodeModuleJsDepth" => Some(CompilerOptionValueType::Number),
        // List options (arrays)
        "lib" | "types" | "typeRoots" | "rootDirs" | "moduleSuffixes" | "customConditions"
        | "plugins" => Some(CompilerOptionValueType::Array),
        // Object options
        "paths" => Some(CompilerOptionValueType::Object),
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
    "incremental",
    "inlineSourceMap",
    "inlineSources",
    "isolatedDeclarations",
    "isolatedModules",
    "jsx",
    "jsxFactory",
    "jsxFragmentFactory",
    "jsxImportSource",
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
    "noLib",
    "noTypesAndSymbols",
    "noPropertyAccessFromIndexSignature",
    "noResolve",
    "noUncheckedIndexedAccess",
    "noUncheckedSideEffectImports",
    "noUnusedLocals",
    "noUnusedParameters",
    "outDir",
    "outFile",
    "paths",
    "plugins",
    "preserveConstEnums",
    "preserveSymlinks",
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

/// Former compiler options that TS7 dropped from its option declarations.
/// They are unknown options without spelling suggestions when non-null.
const TS7_DROPPED_COMPILER_OPTIONS: &[&str] = &[
    "charset",
    "importsnotusedasvalues",
    "keyofstringsonly",
    "noimplicitusestrict",
    "nostrictgenericchecks",
    "out",
    "preservevalueimports",
    "suppressexcesspropertyerrors",
    "suppressimplicitanyindexerrors",
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
        "incremental" => Some("incremental"),
        "inlinesourcemap" => Some("inlineSourceMap"),
        "inlinesources" => Some("inlineSources"),
        "isolateddeclarations" => Some("isolatedDeclarations"),
        "isolatedmodules" => Some("isolatedModules"),
        "jsx" => Some("jsx"),
        "jsxfactory" => Some("jsxFactory"),
        "jsxfragmentfactory" => Some("jsxFragmentFactory"),
        "jsximportsource" => Some("jsxImportSource"),
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
        "nolib" => Some("noLib"),
        "notypesandsymbols" => Some("noTypesAndSymbols"),
        "nopropertyaccessfromindexsignature" => Some("noPropertyAccessFromIndexSignature"),
        "noresolve" => Some("noResolve"),
        "nouncheckedindexedaccess" => Some("noUncheckedIndexedAccess"),
        "nouncheckedsideeffectimports" => Some("noUncheckedSideEffectImports"),
        "nounusedlocals" => Some("noUnusedLocals"),
        "nounusedparameters" => Some("noUnusedParameters"),
        "outdir" => Some("outDir"),
        "outfile" => Some("outFile"),
        "paths" => Some("paths"),
        "plugins" => Some("plugins"),
        "preserveconstenums" => Some("preserveConstEnums"),
        "preservesymlinks" => Some("preserveSymlinks"),
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
    let config_dir = root_config_dir(path);
    load_tsconfig_inner(path, &mut visited, false, &config_dir)
}

/// Load tsconfig.json and collect config-level diagnostics.
pub fn load_tsconfig_with_diagnostics(path: &Path) -> Result<ParsedTsConfig> {
    let mut parsed = load_tsconfig_with_diagnostics_deferred(path)?;
    parsed.flush_pending_removed_option_notices();
    Ok(parsed)
}

/// [`load_tsconfig_with_diagnostics`] with removed-option notices left in
/// `pending_removed_option_notices` (already entry-anchored) instead of
/// flushed into `diagnostics`. The CLI driver uses this to retract notices
/// for options the command line overrides with valid values before flushing —
/// tsc validates removals on the final CLI-merged options (#15806 adjacent
/// case).
pub fn load_tsconfig_with_diagnostics_deferred(path: &Path) -> Result<ParsedTsConfig> {
    let mut visited = FxHashSet::default();
    let config_dir = root_config_dir(path);
    load_tsconfig_inner_with_diagnostics(path, &mut visited, false, &config_dir)
}

/// Absolute directory of the root config being compiled. Every `${configDir}`
/// template in this config and in any config it `extends` resolves against this
/// single directory (the inheriting/leaf config's directory), so it is computed
/// once at the entry point and threaded through the `extends` recursion
/// unchanged. Canonicalization falls back to the lexical path off the
/// filesystem (e.g. on `wasm32`), matching the existing anchoring helpers.
fn root_config_dir(path: &Path) -> PathBuf {
    let parent = path.parent().unwrap_or_else(|| Path::new(""));
    std::fs::canonicalize(parent).unwrap_or_else(|_| parent.to_path_buf())
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
    config_dir: &Path,
) -> Result<TsConfig> {
    let canonical = std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    if !visited.insert(canonical.clone()) {
        // An `extends` cycle is recoverable in tsc: the cyclic base simply
        // contributes nothing and the rest of the chain still merges. See
        // `load_tsconfig_inner_with_diagnostics` for the TS18000 half; this
        // diagnostic-free loader only needs the recovery.
        return Ok(TsConfig::default());
    }

    let source = std::fs::read_to_string(path)
        .with_context(|| format!("failed to read tsconfig: {}", path.display()))?;
    let mut config = parse_tsconfig(&source)
        .with_context(|| format!("failed to parse tsconfig: {}", path.display()))?;
    // Resolve `${configDir}` against the root config's directory before any
    // `extends` anchoring, which only fires on the leftover relative paths.
    substitute_config_dir_templates(&mut config, config_dir);
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
            // An unresolved `extends` is a recoverable condition (tsc reports
            // TS6053 and continues with the local config). This diagnostic-free
            // loader simply degrades: skip the missing base rather than aborting
            // the whole config load.
            let Some(base_path) = resolve_extends_path(path, extends_path_str)? else {
                continue;
            };
            let base_config = load_tsconfig_inner(&base_path, visited, true, config_dir)?;
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
    config_dir: &Path,
) -> Result<ParsedTsConfig> {
    let canonical = std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    if !visited.insert(canonical.clone()) {
        // tsc's `parseConfig` checks its `resolutionStack` on entry, reports
        // TS18000 once, and returns an empty config for the cyclic base — the
        // surviving part of the chain still merges and the program still
        // loads and checks. `visited` is a path stack (popped on the way out),
        // so a diamond where two bases share one ancestor is not a cycle and
        // does not reach here.
        //
        // The message keeps its `{0}` placeholder unsubstituted because that
        // is what tsc emits: it builds this diagnostic without passing the
        // resolution stack as an argument. Reproduced against `typescript@7.0.2`
        // on the self-, 2- and 3-config cycles. Filed upstream as a tsc bug;
        // per the parity contract this matches rather than "fixes" it.
        let mut parsed = ParsedTsConfig::default();
        parsed.diagnostics.push(Diagnostic::error(
            "",
            0,
            0,
            diagnostic_messages::CIRCULARITY_DETECTED_WHILE_RESOLVING_CONFIGURATION.to_string(),
            diagnostic_codes::CIRCULARITY_DETECTED_WHILE_RESOLVING_CONFIGURATION,
        ));
        return Ok(parsed);
    }

    let source = std::fs::read_to_string(path)
        .with_context(|| format!("failed to read tsconfig: {}", path.display()))?;
    let file_display = path.display().to_string();
    let mut parsed = parse::parse_tsconfig_with_diagnostics_deferred(&source, &file_display)
        .with_context(|| format!("failed to parse tsconfig: {}", path.display()))?;
    // Resolve `${configDir}` against the root config's directory before any
    // `extends` anchoring, which only fires on the leftover relative paths.
    substitute_config_dir_templates(&mut parsed.config, config_dir);
    anchor_inherited_path_options(&mut parsed.config, path);
    if inherited {
        anchor_inherited_root_selectors(&mut parsed.config, path);
    }

    let extends = parsed.config.extends.take();
    let has_own_extends = extends.is_some();
    if let Some(extends_value) = extends {
        let extends_paths = match extends_value {
            ExtendsValue::Single(s) => vec![s],
            ExtendsValue::Array(arr) => arr,
        };
        let mut accumulated: Option<TsConfig> = None;
        // Removed-option notices (TS5102/TS5108 family) from base configs stay
        // pending until the whole chain merges: tsc runs the removal check
        // once on the merged effective options (fixes the false TS5108
        // tracked by #15806). VALUE notices are suppressed by a shallower
        // VALID override; KEY notices dedup to the shallowest literal
        // occurrence (whose message embeds the effective value).
        let mut inherited_pending: Vec<RemovedOptionNotice> = Vec::new();
        let mut inherited_explicit_keys: FxHashSet<String> = FxHashSet::default();
        let mut inherited_literal_keys: FxHashSet<String> = FxHashSet::default();
        let stripped = strip_jsonc(&source);
        for extends_path_str in &extends_paths {
            // An `extends` specifier that names no existing config file is a
            // recoverable error: tsc emits TS6053 anchored at the specifier and
            // continues with the remaining (local) options. Emit the same and
            // skip this base instead of failing the whole config load.
            //
            // A bare `""` is a distinct diagnosis (`getExtendsConfigPathOrArray`
            // in `commandLineParser.ts`): every resolution strategy (relative,
            // absolute, package) is a no-op on an empty specifier, so `tsc`
            // short-circuits to TS18051 (`Compiler option 'extends' cannot be
            // given an empty string.`) instead of TS6053's `File '' not found.`.
            if extends_path_str.is_empty() {
                let (start, length) = find_extends_specifier_span(&stripped, extends_path_str);
                let message = format_message(
                    diagnostic_messages::COMPILER_OPTION_CANNOT_BE_GIVEN_AN_EMPTY_STRING,
                    &["extends"],
                );
                parsed.diagnostics.push(Diagnostic::error(
                    &file_display,
                    start,
                    length,
                    message,
                    diagnostic_codes::COMPILER_OPTION_CANNOT_BE_GIVEN_AN_EMPTY_STRING,
                ));
                continue;
            }
            let Some(base_path) = resolve_extends_path(path, extends_path_str)? else {
                let (start, length) = find_extends_specifier_span(&stripped, extends_path_str);
                // A relative/absolute specifier that already names a `.json`
                // path skips tsc's existence probe entirely (see
                // `anchor_extends_specifier`'s doc comment): the failure only
                // surfaces when tsc tries to read the resolved file, which
                // reports TS5083 against that resolved path. Every other
                // shape (extensionless or differently-extensioned
                // relative/absolute specifiers, and any non-relative package
                // specifier regardless of extension) fails at resolution
                // time and keeps TS6053 with the raw specifier text.
                if is_relative_or_absolute_extends_specifier(extends_path_str)
                    && extends_path_str.ends_with(".json")
                {
                    let resolved = anchor_extends_specifier(path, extends_path_str)?;
                    let resolved_display = resolved.to_string_lossy();
                    let message = format_message(
                        diagnostic_messages::CANNOT_READ_FILE_2,
                        &[&resolved_display],
                    );
                    parsed.diagnostics.push(Diagnostic::error(
                        &file_display,
                        start,
                        length,
                        message,
                        diagnostic_codes::CANNOT_READ_FILE_2,
                    ));
                } else {
                    let message =
                        format_message(diagnostic_messages::FILE_NOT_FOUND, &[extends_path_str]);
                    parsed.diagnostics.push(Diagnostic::error(
                        &file_display,
                        start,
                        length,
                        message,
                        diagnostic_codes::FILE_NOT_FOUND,
                    ));
                }
                continue;
            };
            // Route base configs through the diagnostic path so TS5023 / TS5024 / TS5025
            // fire on the *base* file (matching tsc's `base.json(L,C):` anchor)
            // instead of the child's invalid option being silently coerced through
            // the type-validating-free `load_tsconfig_inner`.
            let mut base_parsed =
                load_tsconfig_inner_with_diagnostics(&base_path, visited, true, config_dir)?;
            parsed.diagnostics.extend(base_parsed.diagnostics);
            // Later `extends` entries override earlier ones: a VALUE notice is
            // dropped when this base's subtree sets the key to a valid value;
            // a KEY notice is dropped when this base writes the key at all
            // (the base generated its own, shallower notice). Then adopt the
            // base's own survivors.
            inherited_pending.retain(|notice| {
                if notice.is_value {
                    !base_parsed
                        .explicit_compiler_option_keys
                        .contains(&notice.key)
                } else {
                    !base_parsed.subtree_literal_keys.contains(&notice.key)
                }
            });
            inherited_pending.append(&mut base_parsed.pending_removed_option_notices);
            inherited_explicit_keys
                .extend(base_parsed.explicit_compiler_option_keys.iter().cloned());
            inherited_literal_keys.extend(base_parsed.subtree_literal_keys.iter().cloned());
            accumulated = Some(match accumulated {
                Some(acc) => merge_configs(acc, base_parsed.config),
                None => base_parsed.config,
            });
        }
        if let Some(base) = accumulated {
            parsed.config = merge_configs(base, parsed.config);
        }
        // This config overrides every base: value notices die to its valid
        // keys, key notices to its literal keys; survivors precede this
        // file's own pending notices so deeper (base) options report before
        // shallower ones. Union the subtree's key sets so a grandparent chain
        // can apply the same rules to us.
        inherited_pending.retain(|notice| {
            if notice.is_value {
                !parsed.explicit_compiler_option_keys.contains(&notice.key)
            } else {
                !parsed.literal_value_spans.contains_key(&notice.key)
            }
        });
        if !inherited_pending.is_empty() {
            inherited_pending.append(&mut parsed.pending_removed_option_notices);
            parsed.pending_removed_option_notices = inherited_pending;
        }
        parsed
            .explicit_compiler_option_keys
            .extend(inherited_explicit_keys);
        // `literal_value_spans` stays file-local for entry anchoring; only the
        // key SET unions across the subtree for parent-chain dedup.
        parsed.subtree_literal_keys.extend(inherited_literal_keys);
    }

    if config_ignore_deprecations_silences_6_0(&parsed.config) {
        parsed
            .diagnostics
            .retain(|diag| !is_ts60_deprecation_diagnostic_code(diag.code));
        parsed
            .pending_removed_option_notices
            .retain(|notice| !is_ts60_deprecation_diagnostic_code(notice.diagnostic.code));
    }

    if !inherited {
        // Entry config: re-anchor surviving base-file notices the way tsc's
        // `createCompilerOptionsDiagnostic` does — at the entry's own value
        // span when the entry literally writes the key (e.g. a TS5024-invalid
        // override of an inherited removed value), else at the entry's
        // `"compilerOptions"` key span, else as a global file-less diagnostic
        // (empty `file` renders with no location prefix). A notice generated
        // by the entry file itself keeps its original value-span anchor.
        for notice in &mut parsed.pending_removed_option_notices {
            if notice.diagnostic.file == file_display {
                continue;
            }
            if let Some((start, length)) = parsed.literal_value_spans.get(&notice.key) {
                notice.diagnostic.file = file_display.clone();
                notice.diagnostic.start = *start;
                notice.diagnostic.length = *length;
            } else if let Some(offset) = parsed.compiler_options_key_offset {
                notice.diagnostic.file = file_display.clone();
                notice.diagnostic.start = offset;
                notice.diagnostic.length = "\"compilerOptions\"".len() as u32;
            } else {
                notice.diagnostic.file = String::new();
                notice.diagnostic.start = 0;
                notice.diagnostic.length = 0;
            }
        }
        // The flush happens in the public load wrappers so the CLI driver's
        // deferred path can retract CLI-overridden notices first.

        // tsc's `getConfigFileSpecs` (`commandLineParser.ts`): an explicit
        // `"files": []` is only diagnosed on the entry config itself (never
        // on a base pulled in through `extends` — see below), and only when
        // there is no other way the entry could name its own inputs: no
        // `extends` (which could otherwise supply `files` from a base) and
        // no non-empty `references` (a solution-style root that legitimately
        // has an empty `files`). This is independent of `include`/`exclude`:
        // tsc reports it even when `include` would find real sources.
        if !has_own_extends
            && parsed
                .config
                .references
                .as_ref()
                .is_none_or(|r| r.is_empty())
            && let Some(files) = parsed.config.files.as_ref()
            && files.is_empty()
        {
            let value_start = find_top_level_value_offset_in_source(&source, "files");
            let value_len = estimate_json_value_len(&serde_json::Value::Array(Vec::new()));
            let message = format_message(
                diagnostic_messages::THE_FILES_LIST_IN_CONFIG_FILE_IS_EMPTY,
                &[&file_display],
            );
            parsed.diagnostics.push(Diagnostic::error(
                &file_display,
                value_start,
                value_len,
                message,
                diagnostic_codes::THE_FILES_LIST_IN_CONFIG_FILE_IS_EMPTY,
            ));
        }
    }

    visited.remove(&canonical);
    Ok(parsed)
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
        // tsc's `tryParsePattern` returns `undefined` for a key containing more
        // than one `*`, so `tryParsePatterns` drops it from path mapping
        // entirely. `split_path_pattern` only consults the first `*`, so a
        // multi-`*` key would otherwise build a bogus `prefix`/`suffix` and
        // match specifiers tsc never maps. Reject it here, at the parser, so no
        // such mapping ever reaches either resolver.
        if pattern.matches('*').count() > 1 {
            continue;
        }
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
            // tsc's `matchPatternOrExact` returns an exact, wildcard-free key
            // equal to the specifier *before* it consults any wildcard pattern
            // via `findBestPatternMatch`. An exact key always has a prefix as
            // long as the specifier, so it can only tie (never lose) on
            // `specificity` against a matching wildcard; break that tie in
            // favour of the literal key so the pre-sorted "first match wins"
            // selection mirrors tsc's exact-beats-wildcard precedence.
            .then_with(|| left.pattern.contains('*').cmp(&right.pattern.contains('*')))
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

/// Convert emitter `ScriptTarget` to checker `ScriptTarget`.
///
/// Both sides are the same `tsz_common` `ScriptTarget`, so this is an identity
/// mapping. The checker relies on the precise target to gate diagnostics the
/// way `tsc` does (`useDefineForClassFields` at `ES2022`, the `ES2024` and
/// `ES2025` feature gates, the `using`/`await using` disposable-global checks),
/// so `ES2021` through `ES2025` must be preserved rather than collapsed to
/// `ESNext`.
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
        ScriptTarget::ES2021 => CheckerScriptTarget::ES2021,
        ScriptTarget::ES2022 => CheckerScriptTarget::ES2022,
        ScriptTarget::ES2023 => CheckerScriptTarget::ES2023,
        ScriptTarget::ES2024 => CheckerScriptTarget::ES2024,
        ScriptTarget::ES2025 => CheckerScriptTarget::ES2025,
        ScriptTarget::ESNext => CheckerScriptTarget::ESNext,
    }
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

// TS6046 option metadata for TypeScript 7. Parse-accepted values may include
// legacy values that receive a later removal diagnostic; display lists contain
// only the values TS7 advertises in TS6046 messages.

/// Valid `--target` values. The display list uses tsc's canonical casing.
const VALID_TARGET_VALUES: &[&str] = &[
    "es5", "es6", "es2015", "es2016", "es2017", "es2018", "es2019", "es2020", "es2021", "es2022",
    "es2023", "es2024", "es2025", "esnext",
];
// TSC 7.0 no longer lists deprecated targets (es3, es5) in the error message
// and added es2025. Match TSC's display.
const VALID_TARGET_DISPLAY: &str = "'es6', 'es2015', 'es2016', 'es2017', 'es2018', 'es2019', 'es2020', 'es2021', 'es2022', 'es2023', 'es2024', 'es2025', 'esnext'";

/// Valid `--module` values.
const VALID_MODULE_VALUES: &[&str] = &[
    "commonjs", "amd", "system", "umd", "es6", "es2015", "es2020", "es2022", "esnext", "node16",
    "node18", "node20", "nodenext", "preserve",
];
// TSC 7.0 no longer lists deprecated module kinds (none, amd, system, umd) in the error
// message, though they are still accepted. Match TSC's display.
const VALID_MODULE_DISPLAY: &str = "'commonjs', 'es6', 'es2015', 'es2020', 'es2022', 'esnext', 'node16', 'node18', 'node20', 'nodenext', 'preserve'";

/// Valid `--moduleResolution` values.
const VALID_MODULE_RESOLUTION_VALUES: &[&str] =
    &["classic", "node", "node10", "node16", "nodenext", "bundler"];
const VALID_MODULE_RESOLUTION_DISPLAY: &str = "'node16', 'nodenext', 'bundler'";

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

/// Valid TypeScript 7 `--lib` values.
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

#[cfg(test)]
mod tests;

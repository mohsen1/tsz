use anyhow::{Result, bail};
use std::path::PathBuf;

use crate::checker::context::CheckerOptions;
use crate::emitter::{ModuleKind, PrinterOptions, ScriptTarget};

use super::{
    CompilerOptions, build_path_mappings, checker_target_from_emitter,
    is_valid_identifier_or_qualified_name, jsx_emit_to_mode, normalize_option, parse_jsx_emit,
    parse_module_kind, parse_module_resolution, parse_new_line_kind, parse_script_target,
    resolve_default_lib_files, resolve_lib_files,
};

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

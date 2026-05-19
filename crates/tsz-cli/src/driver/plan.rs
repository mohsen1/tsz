//! CLI option plan phase: override resolution, path normalization, and
//! validation for the compilation driver.
//!
//! This module owns the "plan" step of the driver pipeline: applying CLI
//! flags over resolved compiler options, validating the merged option set,
//! and computing emit-layout helpers used by later pipeline phases.

use anyhow::Result;
use std::path::{Path, PathBuf};

use crate::args::{CliArgs, Module, ModuleDetection, ModuleResolution, NewLine, Target};
use crate::config::{
    CompilerOptions, ModuleResolutionKind, ResolvedCompilerOptions, TsConfig,
    checker_target_from_emitter, parse_tsconfig_with_diagnostics, resolve_default_lib_files,
    resolve_lib_files,
};
use tsz::checker::diagnostics::{Diagnostic, diagnostic_codes};
use tsz_common::common::NewLineKind;

use super::{canonicalize_or_owned, is_declaration_file};

/// Apply CLI flags on top of already-resolved compiler options.
///
/// This is the public entry point for the plan phase. Callers that also have
/// access to the raw `CompilerOptions` from the tsconfig should call
/// `apply_cli_overrides_with_config_options` directly so that merged-option
/// interactions (e.g. `resolveJsonModule` defaults derived from
/// `moduleResolution`) can see the original config values.
pub fn apply_cli_overrides(options: &mut ResolvedCompilerOptions, args: &CliArgs) -> Result<()> {
    apply_cli_overrides_with_config_options(options, args, None)
}

pub(super) fn apply_cli_overrides_with_config_options(
    options: &mut ResolvedCompilerOptions,
    args: &CliArgs,
    config_options: Option<&CompilerOptions>,
) -> Result<()> {
    if let Some(target) = args.target {
        options.printer.target = target.to_script_target();
        options.checker.target = checker_target_from_emitter(options.printer.target);
    }
    if let Some(new_line) = args.new_line {
        options.printer.new_line = match new_line {
            NewLine::Lf => NewLineKind::LineFeed,
            NewLine::Crlf => NewLineKind::CarriageReturnLineFeed,
        };
    }
    if let Some(module) = args.module {
        options.printer.module = module.to_module_kind();
        options.checker.module = module.to_module_kind();
        options.checker.module_explicitly_set = true;
    }
    if let Some(module_resolution) = args.module_resolution {
        options.module_resolution = Some(module_resolution.to_module_resolution_kind());
    }
    apply_module_resolution_derived_options(options, args, config_options);
    if let Some(resolve_package_json_exports) = args.resolve_package_json_exports {
        options.resolve_package_json_exports = resolve_package_json_exports;
    }
    if let Some(resolve_package_json_imports) = args.resolve_package_json_imports {
        options.resolve_package_json_imports = resolve_package_json_imports;
    }
    if let Some(module_suffixes) = args.module_suffixes.as_ref() {
        options.module_suffixes = module_suffixes.clone();
    }
    if args.resolve_json_module {
        options.resolve_json_module = true;
        options.checker.resolve_json_module = true;
    }
    if args.allow_arbitrary_extensions {
        options.allow_arbitrary_extensions = true;
    }
    if args.allow_importing_ts_extensions {
        options.allow_importing_ts_extensions = true;
    }
    if let Some(use_define_for_class_fields) = args.use_define_for_class_fields {
        options.printer.use_define_for_class_fields = use_define_for_class_fields;
    } else if config_options.is_none_or(|options| options.use_define_for_class_fields.is_none()) {
        // Default: true for target >= ES2022, false otherwise (matches tsc behavior)
        options.printer.use_define_for_class_fields = options.printer.target.supports_es2022();
    }
    if args.rewrite_relative_import_extensions {
        options.rewrite_relative_import_extensions = true;
        options.printer.rewrite_relative_import_extensions = true;
    }
    if args.trace_resolution {
        options.trace_resolution = true;
    }
    if let Some(custom_conditions) = args.custom_conditions.as_ref() {
        options.custom_conditions = custom_conditions.clone();
    }
    if let Some(out_dir) = args.out_dir.as_ref() {
        options.out_dir = Some(out_dir.clone());
    }
    if let Some(root_dir) = args.root_dir.as_ref() {
        options.root_dir = Some(root_dir.clone());
    }
    if let Some(base_url) = args.base_url.as_ref() {
        options.base_url = Some(base_url.clone());
    }
    if let Some(root_dirs) = args.root_dirs.as_ref() {
        options.root_dirs = root_dirs.clone();
    }
    if let Some(declaration_dir) = args.declaration_dir.as_ref() {
        options.declaration_dir = Some(declaration_dir.clone());
    }
    if let Some(types) = args.types.as_ref() {
        options.types = Some(types.clone());
        options.checker.types_explicitly_set = true;
    }
    if let Some(type_roots) = args.type_roots.as_ref() {
        options.type_roots = Some(type_roots.clone());
    }
    if args.composite {
        options.composite = true;
        // composite implies declaration and incremental
        options.emit_declarations = true;
        options.checker.emit_declarations = true;
        options.incremental = true;
    }
    if args.declaration {
        options.emit_declarations = true;
        options.checker.emit_declarations = true;
    }
    if args.emit_declaration_only {
        options.emit_declaration_only = true;
    }
    if args.declaration_map {
        options.declaration_map = true;
    }
    if args.source_map {
        options.source_map = true;
    }
    if args.inline_source_map {
        options.inline_source_map = true;
    }
    if args.emit_bom {
        options.emit_bom = true;
    }
    if let Some(out_file) = args.out_file.as_ref() {
        options.out_file = Some(out_file.clone());
    }
    if let Some(ts_build_info_file) = args.ts_build_info_file.as_ref() {
        options.ts_build_info_file = Some(ts_build_info_file.clone());
    }
    if args.incremental {
        options.incremental = true;
    }
    if args.import_helpers {
        options.import_helpers = true;
        options.printer.import_helpers = true;
        // importHelpers means "import from tslib" — suppress inline helper emission
        options.printer.no_emit_helpers = true;
    }
    if args.strict {
        options.checker.strict = true;
        // Expand --strict to individual flags (matching TypeScript behavior).
        // NOTE: noImplicitReturns is NOT part of --strict in TypeScript.
        options.checker.no_implicit_any = true;
        options.checker.strict_null_checks = true;
        options.checker.strict_function_types = true;
        options.checker.strict_bind_call_apply = true;
        options.checker.strict_property_initialization = true;
        options.checker.no_implicit_this = true;
        options.checker.use_unknown_in_catch_variables = true;
        options.checker.strict_builtin_iterator_return = true;
        options.checker.always_strict = true;
        options.printer.always_strict = true;
    } else if args
        .explicitly_disabled_bool_flags
        .iter()
        .any(|name| name == "strict")
    {
        // Mirror config loader's strict-disable expansion: explicit
        // `--strict false` (forwarded by `preprocess_args` through the hidden
        // side-channel) flips a config `strict: true` plus its expansion to
        // `false`. Must run before the individual `Option<bool>` family
        // overrides below so that `--strict false --strictNullChecks=true`
        // still keeps `strict_null_checks = true` (issue #3861).
        options.checker.strict = false;
        options.checker.no_implicit_any = false;
        // noImplicitReturns is NOT part of the strict family; do not reset it here.
        options.checker.strict_null_checks = false;
        options.checker.strict_function_types = false;
        options.checker.strict_bind_call_apply = false;
        options.checker.strict_property_initialization = false;
        options.checker.no_implicit_this = false;
        options.checker.use_unknown_in_catch_variables = false;
        options.checker.strict_builtin_iterator_return = false;
        options.checker.always_strict = false;
        options.printer.always_strict = false;
    }
    // Individual strict flag overrides (must come after --strict expansion)
    if let Some(val) = args.strict_null_checks {
        options.checker.strict_null_checks = val;
    }
    if let Some(val) = args.strict_function_types {
        options.checker.strict_function_types = val;
    }
    if let Some(val) = args.strict_property_initialization {
        options.checker.strict_property_initialization = val;
    }
    if let Some(val) = args.strict_bind_call_apply {
        options.checker.strict_bind_call_apply = val;
    }
    if let Some(val) = args.no_implicit_this {
        options.checker.no_implicit_this = val;
    }
    if let Some(val) = args.no_implicit_any {
        options.checker.no_implicit_any = val;
    }
    if let Some(val) = args.use_unknown_in_catch_variables {
        options.checker.use_unknown_in_catch_variables = val;
    }
    if let Some(val) = args.strict_builtin_iterator_return {
        options.checker.strict_builtin_iterator_return = val;
    }
    if args.no_unchecked_indexed_access {
        options.checker.no_unchecked_indexed_access = true;
    }
    if args.no_unchecked_side_effect_imports {
        options.checker.no_unchecked_side_effect_imports = true;
    }
    if args.exact_optional_property_types {
        options.checker.exact_optional_property_types = true;
    }
    if args.no_property_access_from_index_signature {
        options.checker.no_property_access_from_index_signature = true;
    }
    if args.no_implicit_returns {
        options.checker.no_implicit_returns = true;
    }
    if let Some(val) = args.always_strict {
        options.checker.always_strict = val;
        options.printer.always_strict = val;
    }
    if let Some(ref id) = args.ignore_deprecations
        && (id == "5.0" || id == "6.0")
    {
        options.checker.ignore_deprecations = true;
    }
    if let Some(val) = args.allow_unreachable_code {
        options.checker.allow_unreachable_code = Some(val);
    }
    if let Some(val) = args.allow_unused_labels {
        options.checker.allow_unused_labels = Some(val);
    }
    if args.sound {
        options.checker.sound_mode = true;
    }
    if args.experimental_decorators {
        options.checker.experimental_decorators = true;
        options.printer.legacy_decorators = true;
    }
    if args.emit_decorator_metadata {
        options.printer.emit_decorator_metadata = true;
    }
    // Pass strictNullChecks to printer for metadata union serialization.
    // Only set to true when explicitly enabled via --strict or --strictNullChecks true.
    // The printer default is false (unlike CheckerOptions which defaults to true).
    if args.strict {
        options.printer.strict_null_checks = true;
    }
    if let Some(val) = args.strict_null_checks {
        options.printer.strict_null_checks = val;
    }
    if args.no_unused_locals {
        options.checker.no_unused_locals = true;
    }
    if args.no_unused_parameters {
        options.checker.no_unused_parameters = true;
    }
    if args.no_implicit_override {
        options.checker.no_implicit_override = true;
    }
    if args.erasable_syntax_only {
        options.checker.erasable_syntax_only = true;
    }
    if args.no_fallthrough_cases_in_switch {
        options.checker.no_fallthrough_cases_in_switch = true;
    }
    if args.no_implicit_use_strict {
        options.checker.no_implicit_use_strict = true;
    }
    if args.es_module_interop {
        options.es_module_interop = true;
        options.checker.es_module_interop = true;
        options.printer.es_module_interop = true;
        // esModuleInterop implies allowSyntheticDefaultImports
        options.allow_synthetic_default_imports = true;
        options.checker.allow_synthetic_default_imports = true;
    }
    if let Some(allow_synthetic_default_imports) = args.allow_synthetic_default_imports {
        options.allow_synthetic_default_imports = allow_synthetic_default_imports;
        options.checker.allow_synthetic_default_imports = allow_synthetic_default_imports;
    }
    if args.no_emit {
        options.no_emit = true;
    }
    if args.no_emit_on_error {
        options.no_emit_on_error = true;
    }
    if args.no_resolve {
        options.no_resolve = true;
        options.checker.no_resolve = true;
    }
    if args.allow_umd_global_access {
        options.checker.allow_umd_global_access = true;
    }
    if args.preserve_symlinks {
        options.preserve_symlinks = true;
    }
    if args.no_check {
        options.no_check = true;
    }
    if args.skip_lib_check {
        options.skip_lib_check = true;
    }
    if args.skip_default_lib_check {
        options.skip_default_lib_check = true;
    }
    if args.allow_js {
        options.allow_js = true;
        options.checker.allow_js = true;
    }
    if args.check_js {
        options.check_js = true;
        options.checker.check_js = true;
        if !args
            .explicitly_disabled_bool_flags
            .iter()
            .any(|name| name == "allowJs")
        {
            options.allow_js = true;
            options.checker.allow_js = true;
        }
    }
    if let Some(depth) = args.max_node_module_js_depth {
        options.max_node_module_js_depth = depth;
    }
    if args.isolated_declarations {
        options.isolated_declarations = true;
        options.checker.isolated_declarations = true;
    }
    if let Some(version) = args.types_versions_compiler_version.as_ref() {
        options.types_versions_compiler_version = Some(version.clone());
    } else if let Some(version) = super::types_versions_compiler_version_env() {
        let version = version.trim();
        if !version.is_empty() {
            options.types_versions_compiler_version = Some(version.to_string());
        }
    }
    if let Some(lib_list) = args.lib.as_ref() {
        options.lib_files = resolve_lib_files(lib_list)?;
        options.lib_is_default = false;
    }
    if args.lib_replacement {
        options.lib_replacement = true;
    }
    if args.no_lib {
        options.checker.no_lib = true;
        options.lib_files.clear();
        options.lib_is_default = false;
    }
    if args.downlevel_iteration {
        options.printer.downlevel_iteration = true;
        options.checker.downlevel_iteration = true;
    }
    if args.no_emit_helpers {
        options.printer.no_emit_helpers = true;
    }
    // Implement tsc's getEmitModuleDetectionKind for CLI overrides:
    // - Explicit "force" -> all non-declaration files are modules
    // - Explicit "auto"/"legacy" -> override config default (may undo Node16+ auto-force)
    // - Not set -> preserve config-level default
    match args.module_detection {
        Some(ModuleDetection::Force) => {
            options.printer.module_detection_force = true;
            options.printer.module_detection_legacy = false;
        }
        Some(ModuleDetection::Legacy) => {
            options.printer.module_detection_force = false;
            options.printer.module_detection_legacy = true;
        }
        Some(ModuleDetection::Auto) => {
            // Explicitly opting out of force mode
            options.printer.module_detection_force = false;
            options.printer.module_detection_legacy = false;
        }
        None => {
            // When module detection is not set via CLI, check if the CLI also overrides
            // the module kind. If module is now a node module, apply tsc's default (Force).
            if let Some(ref module_val) = args.module
                && matches!(
                    module_val,
                    Module::Node16 | Module::Node18 | Module::Node20 | Module::NodeNext
                )
            {
                options.printer.module_detection_force = true;
            }
        }
    }
    if args.preserve_const_enums {
        options.printer.preserve_const_enums = true;
    }
    // isolatedModules implies preserveConstEnums: const enums cannot be
    // inlined across file boundaries, so they must be emitted as regular enums.
    // Also disables const enum value inlining at usage sites.
    if args.isolated_modules {
        options.printer.preserve_const_enums = true;
        options.printer.no_const_enum_inlining = true;
        options.checker.isolated_modules = true;
    }
    // verbatimModuleSyntax implies preserveConstEnums (tsc 5.0+): import/export
    // syntax is preserved verbatim, so const enums must be emitted as regular
    // enums rather than erased+inlined.
    if args.verbatim_module_syntax {
        options.printer.preserve_const_enums = true;
        options.printer.no_const_enum_inlining = true;
        options.printer.verbatim_module_syntax = true;
        options.checker.verbatim_module_syntax = true;
    }
    if let Some(jsx) = args.jsx {
        let jsx_emit = match jsx {
            crate::args::JsxEmit::Preserve => crate::config::JsxEmit::Preserve,
            crate::args::JsxEmit::React => crate::config::JsxEmit::React,
            crate::args::JsxEmit::ReactJsx => crate::config::JsxEmit::ReactJsx,
            crate::args::JsxEmit::ReactJsxDev => crate::config::JsxEmit::ReactJsxDev,
            crate::args::JsxEmit::ReactNative => crate::config::JsxEmit::ReactNative,
        };
        options.jsx = Some(jsx_emit);
        // Propagate to the checker's `jsx_mode` so JSX-mode-sensitive checks
        // (e.g. TS2874 "JSX tag requires React in scope") see the CLI value.
        // The tsconfig-driven path mirrors this in `tsz-core/config`, but the
        // CLI override only touched `options.jsx` before — leaving
        // `checker.jsx_mode` at its `JsxMode::None` default and silently
        // skipping the scope check (#6021).
        options.checker.jsx_mode = match jsx_emit {
            crate::config::JsxEmit::Preserve => tsz_common::checker_options::JsxMode::Preserve,
            crate::config::JsxEmit::React => tsz_common::checker_options::JsxMode::React,
            crate::config::JsxEmit::ReactJsx => tsz_common::checker_options::JsxMode::ReactJsx,
            crate::config::JsxEmit::ReactJsxDev => {
                tsz_common::checker_options::JsxMode::ReactJsxDev
            }
            crate::config::JsxEmit::ReactNative => {
                tsz_common::checker_options::JsxMode::ReactNative
            }
        };
    }
    if let Some(ref factory) = args.jsx_factory {
        // tsc preserves `jsxFactory` verbatim — even when invalid (e.g.
        // `my-React-Lib.createElement` from `--reactNamespace`). The TS5067
        // / TS5059 diagnostics are surfaced separately during config
        // validation; emit uses whatever was configured.
        options.checker.jsx_factory = factory.clone();
        options.checker.jsx_factory_from_config = true;
    }
    if let Some(ref frag) = args.jsx_fragment_factory {
        // tsc validates `jsxFragmentFactory` at emit time and falls back to
        // `React.Fragment` when the value is not a dot-separated identifier
        // chain (e.g. `--jsxFragmentFactory 234`). This is asymmetric with
        // `jsxFactory`, which is preserved verbatim.
        if is_valid_jsx_factory_expression(frag) {
            options.checker.jsx_fragment_factory = frag.clone();
            options.checker.jsx_fragment_factory_from_config = true;
        }
        // else: keep default `React.Fragment`
    }
    if let Some(ref source) = args.jsx_import_source {
        options.checker.jsx_import_source = source.clone();
    }
    if args.remove_comments {
        options.printer.remove_comments = true;
    }
    if args.strip_internal {
        options.strip_internal = true;
    }
    if args.target.is_some() && options.lib_is_default && !options.checker.no_lib {
        options.lib_files = resolve_default_lib_files(options.printer.target)?;
    }

    if args.suppress_excess_property_errors {
        options.checker.suppress_excess_property_errors = true;
    }
    if args.suppress_implicit_any_index_errors {
        options.checker.suppress_implicit_any_index_errors = true;
    }

    apply_explicitly_disabled_bool_flags(options, args);

    Ok(())
}

/// Apply `--flag false` overrides for plain `bool` compiler-option flags.
///
/// `preprocess_args` collects each `--flag false` pair for plain bool flags
/// into `args.explicitly_disabled_bool_flags` (the value is the canonical
/// camelCase compiler-option name, e.g. `"strict"`, `"noEmit"`). The earlier
/// override blocks only set options to `true` when the corresponding `bool`
/// arg is `true`, so without this pass an explicit CLI `false` cannot override
/// a `true` value loaded from `tsconfig.json`. tsc treats `--flag false` as an
/// explicit disable, so each entry here flips the matching option(s) back to
/// `false` after config + CLI true-overrides have been applied.
#[allow(clippy::match_same_arms)]
fn apply_explicitly_disabled_bool_flags(options: &mut ResolvedCompilerOptions, args: &CliArgs) {
    if args.explicitly_disabled_bool_flags.is_empty() {
        return;
    }
    for name in &args.explicitly_disabled_bool_flags {
        if matches!(
            name.as_str(),
            // `strict` is handled earlier (just after the `--strict` true
            // expansion) so the strict-family `Option<bool>` overrides that
            // run between can still win over the disable. See the
            // `else if` branch on `args.strict` above.
            "strict"
                // CLI-only display flag; no compiler option to toggle.
                | "noErrorTruncation"
                // `inlineSources` has no corresponding `ResolvedCompilerOptions`
                // field (the CLI flag is parsed for parity but never applied today).
                | "inlineSources"
                // Display / build-graph / watch / diagnostic-mode flags don't
                // round-trip through compiler options; the CLI consumer reads
                // `args.<field>` directly, so no override is needed here.
                | "diagnostics"
                | "extendedDiagnostics"
                | "explainFiles"
                | "listFiles"
                | "listEmittedFiles"
                | "traceResolution"
                | "traceDependencies"
                | "preserveWatchOutput"
                | "synchronousWatchDirectory"
                | "watch"
                | "build"
                | "build-verbose"
                | "dry"
                | "force"
                | "clean"
                | "stopBuildOnErrors"
                | "assumeChangesOnlyAffectDirectDependencies"
                | "disableReferencedProjectLoad"
                | "disableSolutionSearching"
                | "disableSourceOfProjectReferenceRedirect"
                | "disableSizeLimit"
                | "init"
                | "all"
                | "showConfig"
                | "ignoreConfig"
                | "listFilesOnly"
                | "batch"
                // Removed/unsupported legacy flags; silently ignore so a leftover
                // `--foo false` doesn't break compilation.
                | "keyofStringsOnly"
                | "noStrictGenericChecks"
                | "preserveValueImports"
        ) {
            continue;
        }

        match name.as_str() {
            "noEmit" => options.no_emit = false,
            "noEmitOnError" => options.no_emit_on_error = false,
            "noEmitHelpers" => options.printer.no_emit_helpers = false,
            "noCheck" => options.no_check = false,
            "noResolve" => {
                options.no_resolve = false;
                options.checker.no_resolve = false;
            }
            "noLib" => options.checker.no_lib = false,
            "noUnusedLocals" => options.checker.no_unused_locals = false,
            "noUnusedParameters" => options.checker.no_unused_parameters = false,
            "noImplicitReturns" => options.checker.no_implicit_returns = false,
            "noFallthroughCasesInSwitch" => options.checker.no_fallthrough_cases_in_switch = false,
            "noImplicitOverride" => options.checker.no_implicit_override = false,
            "noPropertyAccessFromIndexSignature" => {
                options.checker.no_property_access_from_index_signature = false
            }
            "noUncheckedIndexedAccess" => options.checker.no_unchecked_indexed_access = false,
            "noUncheckedSideEffectImports" => {
                options.checker.no_unchecked_side_effect_imports = false
            }
            "noImplicitUseStrict" => options.checker.no_implicit_use_strict = false,
            "exactOptionalPropertyTypes" => options.checker.exact_optional_property_types = false,
            "erasableSyntaxOnly" => options.checker.erasable_syntax_only = false,
            "sound" => options.checker.sound_mode = false,
            "experimentalDecorators" => {
                options.checker.experimental_decorators = false;
                options.printer.legacy_decorators = false;
            }
            "emitDecoratorMetadata" => options.printer.emit_decorator_metadata = false,
            "esModuleInterop" => {
                options.es_module_interop = false;
                options.checker.es_module_interop = false;
                options.printer.es_module_interop = false;
            }
            "isolatedModules" => {
                options.checker.isolated_modules = false;
                options.printer.preserve_const_enums = false;
                options.printer.no_const_enum_inlining = false;
            }
            "isolatedDeclarations" => {
                options.isolated_declarations = false;
                options.checker.isolated_declarations = false;
            }
            "verbatimModuleSyntax" => {
                options.checker.verbatim_module_syntax = false;
                options.printer.verbatim_module_syntax = false;
                options.printer.preserve_const_enums = false;
                options.printer.no_const_enum_inlining = false;
            }
            "preserveSymlinks" => options.preserve_symlinks = false,
            "preserveConstEnums" => options.printer.preserve_const_enums = false,
            "stripInternal" => options.strip_internal = false,
            "removeComments" => options.printer.remove_comments = false,
            "emitBOM" => options.emit_bom = false,
            "downlevelIteration" => options.printer.downlevel_iteration = false,
            "importHelpers" => {
                options.import_helpers = false;
                options.printer.import_helpers = false;
                options.printer.no_emit_helpers = false;
            }
            "declaration" => {
                options.emit_declarations = false;
                options.checker.emit_declarations = false;
            }
            "declarationMap" => options.declaration_map = false,
            "emitDeclarationOnly" => options.emit_declaration_only = false,
            "sourceMap" => options.source_map = false,
            "inlineSourceMap" => options.inline_source_map = false,
            "composite" => options.composite = false,
            "incremental" => options.incremental = false,
            "skipLibCheck" => options.skip_lib_check = false,
            "skipDefaultLibCheck" => options.skip_default_lib_check = false,
            "allowJs" => {
                options.allow_js = false;
                options.checker.allow_js = false;
            }
            "checkJs" => {
                options.check_js = false;
                options.checker.check_js = false;
            }
            "allowUmdGlobalAccess" => options.checker.allow_umd_global_access = false,
            "allowArbitraryExtensions" => options.allow_arbitrary_extensions = false,
            "allowImportingTsExtensions" => options.allow_importing_ts_extensions = false,
            "rewriteRelativeImportExtensions" => {
                options.rewrite_relative_import_extensions = false;
                options.printer.rewrite_relative_import_extensions = false;
            }
            "resolveJsonModule" => {
                options.resolve_json_module = false;
                options.checker.resolve_json_module = false;
            }
            "libReplacement" => options.lib_replacement = false,
            "suppressExcessPropertyErrors" => {
                options.checker.suppress_excess_property_errors = false
            }
            "suppressImplicitAnyIndexErrors" => {
                options.checker.suppress_implicit_any_index_errors = false
            }
            _ => {
                // Unknown name: leave compilation unchanged. The flag is
                // already validated as a known bool flag in preprocess_args
                // before being recorded here.
            }
        }
    }
}

fn apply_module_resolution_derived_options(
    options: &mut ResolvedCompilerOptions,
    args: &CliArgs,
    config_options: Option<&CompilerOptions>,
) {
    let effective_resolution = options.effective_module_resolution();
    options.checker.implied_classic_resolution =
        matches!(effective_resolution, ModuleResolutionKind::Classic);

    let config_has_resolve_package_json_exports =
        config_options.is_some_and(|options| options.resolve_package_json_exports.is_some());
    if args.resolve_package_json_exports.is_none() && !config_has_resolve_package_json_exports {
        options.resolve_package_json_exports = matches!(
            effective_resolution,
            ModuleResolutionKind::Node16
                | ModuleResolutionKind::NodeNext
                | ModuleResolutionKind::Bundler
        );
    }

    let config_has_resolve_package_json_imports =
        config_options.is_some_and(|options| options.resolve_package_json_imports.is_some());
    if args.resolve_package_json_imports.is_none() && !config_has_resolve_package_json_imports {
        options.resolve_package_json_imports = matches!(
            effective_resolution,
            ModuleResolutionKind::Node
                | ModuleResolutionKind::Node16
                | ModuleResolutionKind::NodeNext
                | ModuleResolutionKind::Bundler
        );
    }

    let config_has_resolve_json_module =
        config_options.is_some_and(|options| options.resolve_json_module.is_some());
    if !args.resolve_json_module && !config_has_resolve_json_module {
        let resolve_json_module = matches!(effective_resolution, ModuleResolutionKind::Bundler);
        options.resolve_json_module = resolve_json_module;
        options.checker.resolve_json_module = resolve_json_module;
    }
}

pub(super) fn validate_cli_compiler_option_diagnostics(
    args: &CliArgs,
    config: Option<&TsConfig>,
) -> Result<Vec<Diagnostic>> {
    use tsz::checker::diagnostics::{diagnostic_messages, format_message};

    let mut diagnostics = Vec::new();
    for key in ["paths", "plugins"] {
        let provided = match key {
            "paths" => cli_config_only_option_has_non_null_value(args.paths.as_ref()),
            "plugins" => cli_config_only_option_has_non_null_value(args.plugins.as_ref()),
            _ => false,
        };
        if provided {
            diagnostics.push(Diagnostic::error(
                String::new(),
                0,
                0,
                format_message(
                    diagnostic_messages::OPTION_CAN_ONLY_BE_SPECIFIED_IN_TSCONFIG_JSON_FILE_OR_SET_TO_NULL_ON_COMMAND_LIN,
                    &[key],
                ),
                diagnostic_codes::OPTION_CAN_ONLY_BE_SPECIFIED_IN_TSCONFIG_JSON_FILE_OR_SET_TO_NULL_ON_COMMAND_LIN,
            ));
        }
    }

    let mut compiler_options = serde_json::Map::new();

    if let Some(target) = args.target {
        compiler_options.insert("target".to_string(), cli_target_value(target).into());
    }
    if let Some(module) = args.module {
        compiler_options.insert("module".to_string(), cli_module_value(module).into());
    }
    if let Some(module_resolution) = args.module_resolution {
        compiler_options.insert(
            "moduleResolution".to_string(),
            cli_module_resolution_value(module_resolution).into(),
        );
    }
    let config_options = config.and_then(|cfg| cfg.compiler_options.as_ref());
    let cli_package_resolution_option = args.custom_conditions.is_some()
        || args.resolve_package_json_exports == Some(true)
        || args.resolve_package_json_imports == Some(true);
    if cli_package_resolution_option {
        if args.module_resolution.is_none()
            && let Some(module_resolution) =
                config_options.and_then(|options| options.module_resolution.as_ref())
        {
            compiler_options.insert(
                "moduleResolution".to_string(),
                module_resolution.clone().into(),
            );
        }
        if args.module.is_none()
            && let Some(module) = config_options.and_then(|options| options.module.as_ref())
        {
            compiler_options.insert("module".to_string(), module.clone().into());
        }
    }
    if let Some(always_strict) = args.always_strict {
        compiler_options.insert("alwaysStrict".to_string(), always_strict.into());
    }
    if let Some(allow_synthetic_default_imports) = args.allow_synthetic_default_imports {
        compiler_options.insert(
            "allowSyntheticDefaultImports".to_string(),
            allow_synthetic_default_imports.into(),
        );
    }
    if let Some(ignore_deprecations) =
        effective_ignore_deprecations_for_cli_validation(args, config)
    {
        compiler_options.insert("ignoreDeprecations".to_string(), ignore_deprecations.into());
    }
    if let Some(base_url) = args.base_url.as_ref() {
        compiler_options.insert(
            "baseUrl".to_string(),
            base_url.to_string_lossy().into_owned().into(),
        );
    }
    if let Some(out_file) = args.out_file.as_ref() {
        compiler_options.insert(
            "outFile".to_string(),
            out_file.to_string_lossy().into_owned().into(),
        );
    }
    let config_bool = |get: fn(&CompilerOptions) -> Option<bool>| -> bool {
        config_options.and_then(get).unwrap_or(false)
    };
    // Group-1 TS5069 triggers (`emitDeclarationOnly`, `declarationMap`,
    // `isolatedDeclarations`) require `declaration` or `composite`. When any of
    // them is set on the CLI, inherit the config-level `declaration`/`composite`
    // so the validator sees the merged effective options instead of the bare
    // CLI snapshot.
    let triggers_decl_or_composite_check =
        args.emit_declaration_only || args.declaration_map || args.isolated_declarations;
    if args.declaration
        || (triggers_decl_or_composite_check && config_bool(|options| options.declaration))
    {
        compiler_options.insert("declaration".to_string(), true.into());
    }
    if args.composite
        || (triggers_decl_or_composite_check && config_bool(|options| options.composite))
    {
        compiler_options.insert("composite".to_string(), true.into());
    }
    if args.no_emit
        || (args.allow_importing_ts_extensions && config_bool(|options| options.no_emit))
    {
        compiler_options.insert("noEmit".to_string(), true.into());
    }
    if args.emit_declaration_only
        || (args.allow_importing_ts_extensions
            && config_bool(|options| options.emit_declaration_only))
    {
        compiler_options.insert("emitDeclarationOnly".to_string(), true.into());
    }
    if args.declaration_map {
        compiler_options.insert("declarationMap".to_string(), true.into());
    }
    if args.allow_js {
        compiler_options.insert("allowJs".to_string(), true.into());
    }
    if args.experimental_decorators {
        compiler_options.insert("experimentalDecorators".to_string(), true.into());
    }
    if args.emit_decorator_metadata {
        compiler_options.insert("emitDecoratorMetadata".to_string(), true.into());
    }
    if args.isolated_declarations {
        compiler_options.insert("isolatedDeclarations".to_string(), true.into());
    }
    if args.verbatim_module_syntax {
        compiler_options.insert("verbatimModuleSyntax".to_string(), true.into());
    }
    if args.allow_importing_ts_extensions {
        compiler_options.insert("allowImportingTsExtensions".to_string(), true.into());
    }
    if args.rewrite_relative_import_extensions
        || (args.allow_importing_ts_extensions
            && config_bool(|options| options.rewrite_relative_import_extensions))
    {
        compiler_options.insert("rewriteRelativeImportExtensions".to_string(), true.into());
    }
    if let Some(resolve_package_json_exports) = args.resolve_package_json_exports {
        compiler_options.insert(
            "resolvePackageJsonExports".to_string(),
            resolve_package_json_exports.into(),
        );
    }
    if let Some(resolve_package_json_imports) = args.resolve_package_json_imports {
        compiler_options.insert(
            "resolvePackageJsonImports".to_string(),
            resolve_package_json_imports.into(),
        );
    }
    if let Some(custom_conditions) = args.custom_conditions.as_ref() {
        compiler_options.insert(
            "customConditions".to_string(),
            serde_json::Value::Array(
                custom_conditions
                    .iter()
                    .map(|condition| serde_json::Value::String(condition.clone()))
                    .collect(),
            ),
        );
    }
    if args.downlevel_iteration {
        compiler_options.insert("downlevelIteration".to_string(), true.into());
    }

    // Removed compiler-option flags accepted by clap should still surface
    // TS5102 (Option has been removed) the same way they do from a tsconfig.
    // Synthesize the keys here so the shared `parse_tsconfig_with_diagnostics`
    // pass below catches them via `removed_compiler_option`. See #3558.
    if args.no_implicit_use_strict {
        compiler_options.insert("noImplicitUseStrict".to_string(), true.into());
    }
    if args.keyof_strings_only {
        compiler_options.insert("keyofStringsOnly".to_string(), true.into());
    }
    if args.suppress_excess_property_errors {
        compiler_options.insert("suppressExcessPropertyErrors".to_string(), true.into());
    }
    if args.suppress_implicit_any_index_errors {
        compiler_options.insert("suppressImplicitAnyIndexErrors".to_string(), true.into());
    }
    if args.no_strict_generic_checks {
        compiler_options.insert("noStrictGenericChecks".to_string(), true.into());
    }
    if args.preserve_value_imports {
        compiler_options.insert("preserveValueImports".to_string(), true.into());
    }
    if let Some(charset) = args.charset.as_deref() {
        compiler_options.insert("charset".to_string(), charset.to_string().into());
    }
    if let Some(imports_not_used_as_values) = args.imports_not_used_as_values {
        let value = match imports_not_used_as_values {
            crate::args::ImportsNotUsedAsValues::Remove => "remove",
            crate::args::ImportsNotUsedAsValues::Preserve => "preserve",
            crate::args::ImportsNotUsedAsValues::Error => "error",
        };
        compiler_options.insert("importsNotUsedAsValues".to_string(), value.into());
    }
    if let Some(out) = args.out.as_ref() {
        compiler_options.insert("out".to_string(), out.to_string_lossy().into_owned().into());
    }

    if compiler_options.is_empty() {
        return Ok(diagnostics);
    }

    let mut root = serde_json::Map::new();
    root.insert(
        "compilerOptions".to_string(),
        serde_json::Value::Object(compiler_options),
    );
    let source = serde_json::Value::Object(root).to_string();
    let parsed = parse_tsconfig_with_diagnostics(&source, "")?;
    diagnostics.extend(parsed.diagnostics);
    Ok(diagnostics)
}

fn cli_config_only_option_has_non_null_value(values: Option<&Vec<String>>) -> bool {
    values.is_some_and(|values| !(values.len() == 1 && values[0].eq_ignore_ascii_case("null")))
}

fn effective_ignore_deprecations_for_cli_validation<'a>(
    args: &'a CliArgs,
    config: Option<&'a TsConfig>,
) -> Option<&'a str> {
    if let Some(ignore_deprecations) = args.ignore_deprecations.as_deref() {
        return Some(ignore_deprecations);
    }

    config
        .and_then(|cfg| cfg.compiler_options.as_ref())
        .and_then(|compiler_options| compiler_options.ignore_deprecations.as_deref())
        .filter(|value| *value == "5.0" || *value == "6.0")
}

pub(super) fn cli_ignore_deprecations_silences_6_0(args: &CliArgs) -> bool {
    matches!(args.ignore_deprecations.as_deref(), Some("6.0"))
}

pub(super) const fn is_deprecation_diagnostic_code(code: u32) -> bool {
    code
        == diagnostic_codes::OPTION_IS_DEPRECATED_AND_WILL_STOP_FUNCTIONING_IN_TYPESCRIPT_SPECIFY_COMPILEROPT_2
        || code
            == diagnostic_codes::OPTION_IS_DEPRECATED_AND_WILL_STOP_FUNCTIONING_IN_TYPESCRIPT_SPECIFY_COMPILEROPT
}

pub(super) const fn is_removed_option_diagnostic_code(code: u32) -> bool {
    code == diagnostic_codes::OPTION_HAS_BEEN_REMOVED_PLEASE_REMOVE_IT_FROM_YOUR_CONFIGURATION
        || code
            == diagnostic_codes::OPTION_HAS_BEEN_REMOVED_PLEASE_REMOVE_IT_FROM_YOUR_CONFIGURATION_2
}

pub(super) const fn is_removed_option_value_diagnostic_code(code: u32) -> bool {
    code == diagnostic_codes::OPTION_HAS_BEEN_REMOVED_PLEASE_REMOVE_IT_FROM_YOUR_CONFIGURATION_2
}

const fn cli_target_value(target: Target) -> &'static str {
    match target {
        Target::Es3 => "es3",
        Target::Es5 => "es5",
        Target::Es2015 => "es2015",
        Target::Es2016 => "es2016",
        Target::Es2017 => "es2017",
        Target::Es2018 => "es2018",
        Target::Es2019 => "es2019",
        Target::Es2020 => "es2020",
        Target::Es2021 => "es2021",
        Target::Es2022 => "es2022",
        Target::Es2023 => "es2023",
        Target::Es2024 => "es2024",
        Target::Es2025 => "es2025",
        Target::EsNext => "esnext",
    }
}

const fn cli_module_value(module: Module) -> &'static str {
    match module {
        Module::None => "none",
        Module::CommonJs => "commonjs",
        Module::Amd => "amd",
        Module::Umd => "umd",
        Module::System => "system",
        Module::Es2015 => "es2015",
        Module::Es2020 => "es2020",
        Module::Es2022 => "es2022",
        Module::EsNext => "esnext",
        Module::Node16 => "node16",
        Module::Node18 => "node18",
        Module::Node20 => "node20",
        Module::NodeNext => "nodenext",
        Module::Preserve => "preserve",
    }
}

const fn cli_module_resolution_value(module_resolution: ModuleResolution) -> &'static str {
    match module_resolution {
        ModuleResolution::Classic => "classic",
        ModuleResolution::Node10 => "node10",
        ModuleResolution::Node16 => "node16",
        ModuleResolution::NodeNext => "nodenext",
        ModuleResolution::Bundler => "bundler",
    }
}

/// Selects the most recently modified `.d.ts` file among `emitted_files` and
/// returns its path relative to `base_dir`, using forward slashes. Returns
/// `None` if no `.d.ts` files exist or none have readable metadata.
pub(super) fn find_latest_dts_file(emitted_files: &[PathBuf], base_dir: &Path) -> Option<String> {
    let latest = emitted_files
        .iter()
        .filter(|p| p.extension().and_then(|s| s.to_str()) == Some("d.ts"))
        .filter_map(|p| std::fs::metadata(p).ok()?.modified().ok().map(|t| (t, p)))
        .max_by_key(|(t, _)| *t)
        .map(|(_, p)| p)?;

    let relative = latest
        .strip_prefix(base_dir)
        .unwrap_or(latest)
        .to_string_lossy()
        .replace('\\', "/");
    Some(relative)
}

/// Validate that a `jsxFactory` / `jsxFragmentFactory` value is a
/// dot-separated identifier chain (e.g. `h`, `React.createElement`).
///
/// Empty segments, leading/trailing dots, and any non-identifier character
/// (digits leading a segment, dashes, whitespace) fail validation. Mirrors
/// tsc's `EntityName` + identifier check used to drive the TS5067 diagnostic
/// and the runtime fallback.
fn is_valid_jsx_factory_expression(s: &str) -> bool {
    if s.is_empty() {
        return false;
    }
    s.split('.').all(|seg| {
        let mut chars = seg.chars();
        let Some(first) = chars.next() else {
            return false;
        };
        if !(first == '_' || first == '$' || first.is_alphabetic()) {
            return false;
        }
        chars.all(|c| c == '_' || c == '$' || c.is_alphanumeric())
    })
}

/// Compute the implicit common source directory for emit-eligible source files
/// when `rootDir` is not set.
///
/// Returns `Some(canonical_dir)` only when the inferred common directory
/// differs from the tsconfig directory; in that case TS5011 should fire
/// because `outDir` would land output in a layout the user did not anchor
/// explicitly. Returns `None` when there are no emit-eligible files or when
/// the inferred common directory equals the tsconfig directory.
pub(super) fn implicit_common_source_directory(
    file_paths: &[PathBuf],
    base_dir: &Path,
    cwd: &Path,
) -> Option<PathBuf> {
    let mut file_dirs: Vec<PathBuf> = file_paths
        .iter()
        .filter(|p| !is_declaration_file(p))
        .map(|p| {
            let abs = if p.is_absolute() {
                p.clone()
            } else {
                cwd.join(p)
            };
            canonicalize_or_owned(&abs)
        })
        .filter_map(|p: PathBuf| p.parent().map(Path::to_path_buf))
        .collect();

    if file_dirs.is_empty() {
        return None;
    }

    file_dirs.sort();
    file_dirs.dedup();
    let mut common = file_dirs[0].clone();
    for dir in &file_dirs[1..] {
        common = longest_common_directory(&common, dir);
        if common.as_os_str().is_empty() {
            return None;
        }
    }

    let canonical_base = canonicalize_or_owned(base_dir);
    if common == canonical_base {
        None
    } else {
        Some(common)
    }
}

fn longest_common_directory(a: &Path, b: &Path) -> PathBuf {
    a.components()
        .zip(b.components())
        .take_while(|(ac, bc)| ac == bc)
        .map(|(c, _)| c)
        .collect()
}

/// Format `path` for display relative to `dir`, using forward slashes and a
/// leading `./` when the result is a non-parent relative path. Falls back to
/// the path's own string representation when it cannot be expressed under
/// `dir`.
pub(super) fn display_relative_to_dir(path: &Path, dir: &Path) -> String {
    let rel = path.strip_prefix(dir).map(Path::to_path_buf).or_else(|_| {
        let cdir = canonicalize_or_owned(dir);
        let cpath = canonicalize_or_owned(path);
        cpath.strip_prefix(&cdir).map(Path::to_path_buf)
    });

    match rel {
        Ok(rel) if rel.as_os_str().is_empty() => "./".to_string(),
        Ok(rel) => {
            let s = rel.to_string_lossy().replace('\\', "/");
            if s.starts_with("./") || s.starts_with("../") {
                s
            } else {
                format!("./{s}")
            }
        }
        Err(_) => path.to_string_lossy().replace('\\', "/"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::args::CliArgs;
    use crate::config::ResolvedCompilerOptions;
    use clap::Parser;

    #[test]
    fn is_valid_jsx_factory_expression_accepts_simple_identifier() {
        assert!(is_valid_jsx_factory_expression("h"));
        assert!(is_valid_jsx_factory_expression("React"));
        assert!(is_valid_jsx_factory_expression("_factory"));
        assert!(is_valid_jsx_factory_expression("$createElement"));
    }

    #[test]
    fn is_valid_jsx_factory_expression_accepts_dotted_chain() {
        assert!(is_valid_jsx_factory_expression("React.createElement"));
        assert!(is_valid_jsx_factory_expression("a.b.c"));
    }

    #[test]
    fn is_valid_jsx_factory_expression_rejects_invalid() {
        assert!(!is_valid_jsx_factory_expression(""));
        assert!(!is_valid_jsx_factory_expression("234"));
        assert!(!is_valid_jsx_factory_expression("my-lib.create"));
        assert!(!is_valid_jsx_factory_expression(".leading"));
        assert!(!is_valid_jsx_factory_expression("trailing."));
    }

    #[test]
    fn cli_ignore_deprecations_6_0_detected() {
        let args = CliArgs::try_parse_from(["tsz", "--ignoreDeprecations", "6.0"]).unwrap();
        assert!(cli_ignore_deprecations_silences_6_0(&args));
    }

    #[test]
    fn cli_ignore_deprecations_5_0_not_6_0() {
        let args = CliArgs::try_parse_from(["tsz", "--ignoreDeprecations", "5.0"]).unwrap();
        assert!(!cli_ignore_deprecations_silences_6_0(&args));
    }

    #[test]
    fn apply_cli_overrides_no_check_sets_option() {
        let mut options = ResolvedCompilerOptions::default();
        let args = CliArgs::try_parse_from(["tsz", "--noCheck"]).unwrap();
        apply_cli_overrides(&mut options, &args).unwrap();
        assert!(options.no_check);
    }

    #[test]
    fn apply_cli_overrides_strict_expands_flags() {
        let mut options = ResolvedCompilerOptions::default();
        let args = CliArgs::try_parse_from(["tsz", "--strict"]).unwrap();
        apply_cli_overrides(&mut options, &args).unwrap();
        assert!(options.checker.strict_null_checks);
        assert!(options.checker.no_implicit_any);
        assert!(options.checker.strict_function_types);
    }

    #[test]
    fn longest_common_directory_shared_prefix() {
        use std::path::PathBuf;
        let a = PathBuf::from("/home/user/project/src");
        let b = PathBuf::from("/home/user/project/lib");
        let common = longest_common_directory(&a, &b);
        assert_eq!(common, PathBuf::from("/home/user/project"));
    }

    #[test]
    fn longest_common_directory_no_common() {
        use std::path::PathBuf;
        let a = PathBuf::from("/usr/local");
        let b = PathBuf::from("/home/user");
        let common = longest_common_directory(&a, &b);
        // On unix, "/" is the common root
        assert!(common == PathBuf::from("/") || common.as_os_str().is_empty());
    }
}

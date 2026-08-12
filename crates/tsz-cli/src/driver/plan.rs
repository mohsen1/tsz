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
    apply_module_detection, checker_target_from_emitter, derive_default_module_kind,
    parse_tsconfig_with_diagnostics, resolve_default_lib_files, resolve_lib_files, strict_family,
};
use tsz::checker::diagnostics::{Diagnostic, diagnostic_codes};
use tsz_common::common::NewLineKind;
use tsz_common::options::module_detection::ModuleDetectionKind;

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
    // Re-derive the default `module` from the effective target/moduleResolution
    // when neither the tsconfig nor a CLI `--module` set it explicitly. The
    // config-level resolution (`resolve_compiler_options`) computed the module
    // default from the *config's* target, but a CLI `--target`/`--moduleResolution`
    // override changes those inputs afterwards, so the default must be recomputed
    // here to match tsc: an unspecified module resolves to CommonJS for a target
    // below ES2015 (e.g. `--target es5`) and to the target's ES module kind
    // otherwise, with moduleResolution `node16`/`nodenext` implying the matching
    // module kind. Without this, `--target es5` alone wrongly kept the ambient ES
    // module default and emitted invalid `export` syntax at an ES5 target.
    if !options.checker.module_explicitly_set
        && (args.target.is_some() || args.module_resolution.is_some())
    {
        let target_explicitly_set =
            args.target.is_some() || config_options.is_some_and(|config| config.target.is_some());
        let default_module = derive_default_module_kind(
            options.printer.target,
            target_explicitly_set,
            options.module_resolution,
        );
        options.printer.module = default_module;
        options.checker.module = default_module;
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
        options.checker.types_has_wildcard = types.iter().any(|entry| entry == "*");
        options.types = Some(types.clone());
        options.checker.types_explicitly_set = true;
    }
    if let Some(type_roots) = args.type_roots.as_ref() {
        options.type_roots = Some(type_roots.clone());
    }
    if args.composite {
        // `composite -> declaration + incremental` is owned by the shared
        // `apply_non_strict_fanout` table below; record only the raw value.
        options.composite = true;
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
        // `importHelpers -> printer.import_helpers + no_emit_helpers` is owned
        // by the shared `apply_non_strict_fanout` table below.
        options.import_helpers = true;
    }
    // Strict-family expansion and explicit member overrides are owned by the
    // shared `strict_family` table (tsc 6.0 `getStrictOptionValue`).
    // NOTE: noImplicitReturns is NOT part of --strict in TypeScript.
    // An explicit `--strict false` (forwarded by `preprocess_args` through
    // the hidden side-channel) contracts a config `strict: true` plus its
    // expansion to `false`; the explicit `Option<bool>` member overrides are
    // applied after the umbrella inside the helper, so `--strict false
    // --strictNullChecks=true` still keeps `strict_null_checks = true`
    // (issue #3861). `alwaysStrict` is not a strict-family member in tsc 6.0
    // (`alwaysStrict !== false`, independent of `strict`), so neither
    // `--strict` nor `--strict false` touches it; only the explicit
    // `--alwaysStrict` override below does.
    let strict_umbrella = if args.strict {
        Some(true)
    } else if args
        .explicitly_disabled_bool_flags
        .iter()
        .any(|name| name == "strict")
    {
        Some(false)
    } else {
        None
    };
    strict_family::apply_strict_family(
        &mut options.checker,
        &strict_family::StrictFamilyOverrides {
            strict: strict_umbrella,
            no_implicit_any: args.no_implicit_any,
            no_implicit_this: args.no_implicit_this,
            strict_null_checks: args.strict_null_checks,
            strict_function_types: args.strict_function_types,
            strict_bind_call_apply: args.strict_bind_call_apply,
            strict_property_initialization: args.strict_property_initialization,
            strict_builtin_iterator_return: args.strict_builtin_iterator_return,
            use_unknown_in_catch_variables: args.use_unknown_in_catch_variables,
        },
    );
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
        && (id == "5.0" || id == "6.0" || id == "7.0")
    {
        options.checker.ignore_deprecations = true;
        // No accepted value silences the deprecated-`assert` diagnostic
        // (TS2880) on the pinned 7.0.2 oracle (#16217); `ignore_deprecations_6_0`
        // stays threaded for any deprecation whose grace window has not closed.
        options.checker.ignore_deprecations_6_0 = id == "6.0";
    }
    if let Some(val) = args.allow_unreachable_code {
        options.checker.allow_unreachable_code = Some(val);
    }
    if let Some(val) = args.allow_unused_labels {
        options.checker.allow_unused_labels = Some(val);
    }
    if args.sound || args.sound_report_only || args.sound_declaration_projection {
        options.checker.sound_mode = true;
    }
    if args.sound_report_only {
        options.checker.sound_report_only = true;
    }
    if args.sound_declaration_projection {
        options.checker.sound_declaration_projection = true;
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
    // tsc 6.0 defaults `esModuleInterop` to `true` unless it is explicitly set on
    // the command line or in `tsconfig.json`. The config path already applies
    // this default in `resolve_compiler_options`; mirror it here so the CLI-only
    // path (no tsconfig) matches tsc instead of falling back to the historical
    // `false`. An explicit `--esModuleInterop false` (recorded in
    // `explicitly_disabled_bool_flags`), a tsconfig value, or a TS5024-invalid
    // tsconfig value opts back out.
    let es_module_interop_disabled = args
        .explicitly_disabled_bool_flags
        .iter()
        .any(|name| name == "esModuleInterop");
    let es_module_interop_invalidated = config_options.is_some_and(|config| {
        config
            .invalidated_options
            .iter()
            .any(|name| name == "esModuleInterop")
    });
    if args.es_module_interop
        || (!es_module_interop_disabled
            && !es_module_interop_invalidated
            && config_options.is_none_or(|config| config.es_module_interop.is_none()))
    {
        options.es_module_interop = true;
        options.checker.es_module_interop = true;
        options.printer.es_module_interop = true;
        // `esModuleInterop -> allowSyntheticDefaultImports` is owned by the
        // shared `apply_non_strict_fanout` table below.
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
            apply_module_detection(options, ModuleDetectionKind::Force);
        }
        Some(ModuleDetection::Legacy) => {
            apply_module_detection(options, ModuleDetectionKind::Legacy);
        }
        Some(ModuleDetection::Auto) => {
            // Explicitly opting out of force mode
            apply_module_detection(options, ModuleDetectionKind::Auto);
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
                apply_module_detection(options, ModuleDetectionKind::Force);
            }
        }
    }
    if args.preserve_const_enums {
        options.printer.preserve_const_enums = true;
        options.checker.preserve_const_enums = true;
    }
    // `isolatedModules`/`verbatimModuleSyntax -> preserveConstEnums` (and
    // `verbatimModuleSyntax -> isolatedModules`) are owned by the shared
    // `apply_non_strict_fanout` table below; record only the raw source flags.
    if args.isolated_modules {
        options.checker.isolated_modules = true;
    }
    if args.verbatim_module_syntax {
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

    // Fan out the non-strict-family implications (composite -> declaration +
    // incremental, isolatedModules/verbatimModuleSyntax -> preserveConstEnums,
    // importHelpers -> no_emit_helpers, esModuleInterop ->
    // allowSyntheticDefaultImports) through the shared table, so the CLI and
    // tsconfig engines derive them identically. Runs before the explicit
    // `--flag false` disable pass (which resets the source flags and their
    // derived emit fields for the CLI-only `--flag false` side-channel).
    crate::config::apply_non_strict_fanout(options);
    // Re-apply an explicit `--allowSyntheticDefaultImports` so it still wins
    // over the `esModuleInterop -> allowSyntheticDefaultImports` implication
    // (tsc resolves an explicitly provided value before any default).
    if let Some(allow_synthetic_default_imports) = args.allow_synthetic_default_imports {
        options.allow_synthetic_default_imports = allow_synthetic_default_imports;
        options.checker.allow_synthetic_default_imports = allow_synthetic_default_imports;
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
fn apply_explicitly_disabled_bool_flags(options: &mut ResolvedCompilerOptions, args: &CliArgs) {
    if args.explicitly_disabled_bool_flags.is_empty() {
        return;
    }
    for name in &args.explicitly_disabled_bool_flags {
        if matches!(
            name.as_str(),
            // `strict` is handled earlier by the shared `strict_family`
            // helper, which applies the explicit `Option<bool>` member
            // overrides after the umbrella so they win over the disable
            // (issue #3861). See `apply_strict_family` above.
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
            "exactOptionalPropertyTypes" => options.checker.exact_optional_property_types = false,
            "erasableSyntaxOnly" => options.checker.erasable_syntax_only = false,
            "sound" => options.checker.sound_mode = false,
            "soundCheckDeclarations" => options.checker.sound_check_declarations = false,
            "soundReportOnly" => options.checker.sound_report_only = false,
            "soundPedantic" => options.checker.sound_pedantic = false,
            "soundDeclarationProjection" => options.checker.sound_declaration_projection = false,
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
    let mut diagnostics = Vec::new();
    for key in ["paths", "plugins"] {
        let provided = match key {
            "paths" => cli_config_only_option_has_non_null_value(args.paths.as_ref()),
            "plugins" => cli_config_only_option_has_non_null_value(args.plugins.as_ref()),
            _ => false,
        };
        if provided {
            diagnostics.push(cli_config_only_option_diagnostic(key));
        }
    }

    let compiler_options = cli_explicit_compiler_options_map(args, config);

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

/// Compiler-option keys the CLI explicitly sets to a VALID, non-removed
/// value. tsc merges CLI options over the config chain before running the
/// removed-option check, so a config-chain removal diagnostic (TS5108 family)
/// for one of these keys is retracted — `tsc -p . --moduleResolution bundler`
/// over a chain-effective `node10` compiles clean.
pub(super) fn cli_valid_override_keys(
    args: &CliArgs,
    config: Option<&TsConfig>,
) -> Result<rustc_hash::FxHashSet<String>> {
    let compiler_options = cli_explicit_compiler_options_map(args, config);
    if compiler_options.is_empty() {
        return Ok(rustc_hash::FxHashSet::default());
    }
    let mut root = serde_json::Map::new();
    root.insert(
        "compilerOptions".to_string(),
        serde_json::Value::Object(compiler_options),
    );
    let source = serde_json::Value::Object(root).to_string();
    let parsed = tsz::config::parse_tsconfig_with_diagnostics_deferred(&source, "")?;
    let mut keys = parsed.explicit_compiler_option_keys;
    // A CLI value that is itself removed (e.g. `--moduleResolution node10`)
    // parses as a valid string but carries its own pending removal notice; it
    // must not retract the config chain's diagnostic.
    for notice in &parsed.pending_removed_option_notices {
        keys.remove(&notice.key);
    }
    Ok(keys)
}

/// The `compilerOptions` JSON object equivalent to the options explicitly
/// passed on the command line (shared by CLI validation and the
/// removed-option override computation so both see the same key set).
fn cli_explicit_compiler_options_map(
    args: &CliArgs,
    config: Option<&TsConfig>,
) -> serde_json::Map<String, serde_json::Value> {
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
    if args
        .explicitly_disabled_bool_flags
        .iter()
        .any(|name| name == "esModuleInterop")
    {
        compiler_options.insert("esModuleInterop".to_string(), false.into());
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
    if args.downlevel_iteration
        || args
            .explicitly_disabled_bool_flags
            .iter()
            .any(|name| name == "downlevelIteration")
    {
        compiler_options.insert(
            "downlevelIteration".to_string(),
            args.downlevel_iteration.into(),
        );
    }

    // TS7-dropped compiler-option flags remain accepted by clap so the shared
    // config parser can produce tsc's TS5023 unknown-option diagnostic.
    let explicitly_disabled = |name: &str| {
        args.explicitly_disabled_bool_flags
            .iter()
            .any(|candidate| candidate == name)
    };
    if args.no_implicit_use_strict || explicitly_disabled("noImplicitUseStrict") {
        compiler_options.insert(
            "noImplicitUseStrict".to_string(),
            args.no_implicit_use_strict.into(),
        );
    }
    if args.keyof_strings_only || explicitly_disabled("keyofStringsOnly") {
        compiler_options.insert(
            "keyofStringsOnly".to_string(),
            args.keyof_strings_only.into(),
        );
    }
    if args.suppress_excess_property_errors || explicitly_disabled("suppressExcessPropertyErrors") {
        compiler_options.insert(
            "suppressExcessPropertyErrors".to_string(),
            args.suppress_excess_property_errors.into(),
        );
    }
    if args.suppress_implicit_any_index_errors
        || explicitly_disabled("suppressImplicitAnyIndexErrors")
    {
        compiler_options.insert(
            "suppressImplicitAnyIndexErrors".to_string(),
            args.suppress_implicit_any_index_errors.into(),
        );
    }
    if args.no_strict_generic_checks || explicitly_disabled("noStrictGenericChecks") {
        compiler_options.insert(
            "noStrictGenericChecks".to_string(),
            args.no_strict_generic_checks.into(),
        );
    }
    if args.preserve_value_imports || explicitly_disabled("preserveValueImports") {
        compiler_options.insert(
            "preserveValueImports".to_string(),
            args.preserve_value_imports.into(),
        );
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

    compiler_options
}

pub(super) const fn is_direct_cli_parse_diagnostic_code(code: u32) -> bool {
    matches!(
        code,
        diagnostic_codes::UNKNOWN_COMPILER_OPTION
            | diagnostic_codes::UNKNOWN_COMPILER_OPTION_DID_YOU_MEAN
            | diagnostic_codes::ARGUMENT_FOR_OPTION_MUST_BE
            | diagnostic_codes::OPTION_CAN_ONLY_BE_SPECIFIED_IN_TSCONFIG_JSON_FILE_OR_SET_TO_NULL_ON_COMMAND_LIN
    )
}

/// Build direct command-line parse diagnostics in final argv order.
///
/// `preprocess_args` records option identities in a hidden repeated argument.
/// Each identity is validated independently through the existing config parser,
/// preserving its exact diagnostic construction without inspecting messages.
pub(super) fn ordered_direct_cli_parse_diagnostics(args: &CliArgs) -> Result<Vec<Diagnostic>> {
    if args.direct_cli_option_order.is_empty() {
        return Ok(validate_cli_compiler_option_diagnostics(args, None)?
            .into_iter()
            .filter(|diagnostic| is_direct_cli_parse_diagnostic_code(diagnostic.code))
            .collect());
    }

    let mut diagnostics = Vec::new();
    for key in &args.direct_cli_option_order {
        if matches!(key.as_str(), "paths" | "plugins") {
            let provided = match key.as_str() {
                "paths" => cli_config_only_option_has_non_null_value(args.paths.as_ref()),
                "plugins" => cli_config_only_option_has_non_null_value(args.plugins.as_ref()),
                _ => false,
            };
            if provided {
                diagnostics.push(cli_config_only_option_diagnostic(key));
            }
            continue;
        }

        let Some(value) = direct_cli_parse_option_value(args, key) else {
            continue;
        };
        let mut compiler_options = serde_json::Map::new();
        compiler_options.insert(key.clone(), value);
        let mut root = serde_json::Map::new();
        root.insert(
            "compilerOptions".to_string(),
            serde_json::Value::Object(compiler_options),
        );
        let source = serde_json::Value::Object(root).to_string();
        let parsed = parse_tsconfig_with_diagnostics(&source, "")?;
        diagnostics.extend(
            parsed
                .diagnostics
                .into_iter()
                .filter(|diagnostic| is_direct_cli_parse_diagnostic_code(diagnostic.code)),
        );
    }
    Ok(diagnostics)
}

fn cli_config_only_option_diagnostic(key: &str) -> Diagnostic {
    use tsz::checker::diagnostics::{diagnostic_messages, format_message};

    Diagnostic::error(
        String::new(),
        0,
        0,
        format_message(
            diagnostic_messages::OPTION_CAN_ONLY_BE_SPECIFIED_IN_TSCONFIG_JSON_FILE_OR_SET_TO_NULL_ON_COMMAND_LIN,
            &[key],
        ),
        diagnostic_codes::OPTION_CAN_ONLY_BE_SPECIFIED_IN_TSCONFIG_JSON_FILE_OR_SET_TO_NULL_ON_COMMAND_LIN,
    )
}

fn direct_cli_parse_option_value(args: &CliArgs, key: &str) -> Option<serde_json::Value> {
    let explicitly_disabled = |name: &str| {
        args.explicitly_disabled_bool_flags
            .iter()
            .any(|candidate| candidate == name)
    };
    let dropped_bool = |name: &str, value: bool| {
        (value || explicitly_disabled(name)).then(|| serde_json::Value::Bool(value))
    };

    match key {
        "target" => args
            .target
            .map(|target| serde_json::Value::String(cli_target_value(target).to_string())),
        "module" => args
            .module
            .map(|module| serde_json::Value::String(cli_module_value(module).to_string())),
        "keyofStringsOnly" => dropped_bool("keyofStringsOnly", args.keyof_strings_only),
        "noImplicitUseStrict" => dropped_bool("noImplicitUseStrict", args.no_implicit_use_strict),
        "noStrictGenericChecks" => {
            dropped_bool("noStrictGenericChecks", args.no_strict_generic_checks)
        }
        "preserveValueImports" => dropped_bool("preserveValueImports", args.preserve_value_imports),
        "suppressExcessPropertyErrors" => dropped_bool(
            "suppressExcessPropertyErrors",
            args.suppress_excess_property_errors,
        ),
        "suppressImplicitAnyIndexErrors" => dropped_bool(
            "suppressImplicitAnyIndexErrors",
            args.suppress_implicit_any_index_errors,
        ),
        "charset" => args
            .charset
            .as_ref()
            .map(|value| serde_json::Value::String(value.clone())),
        "importsNotUsedAsValues" => args.imports_not_used_as_values.map(|value| {
            serde_json::Value::String(
                match value {
                    crate::args::ImportsNotUsedAsValues::Remove => "remove",
                    crate::args::ImportsNotUsedAsValues::Preserve => "preserve",
                    crate::args::ImportsNotUsedAsValues::Error => "error",
                }
                .to_string(),
            )
        }),
        "out" => args
            .out
            .as_ref()
            .map(|value| serde_json::Value::String(value.to_string_lossy().into_owned())),
        _ => None,
    }
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
        .filter(|value| *value == "5.0" || *value == "6.0" || *value == "7.0")
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

/// Selects the most recently modified declaration file among `emitted_files`
/// and returns its path relative to `base_dir`, using forward slashes.
/// Declaration outputs are matched with the same rule as tsc's
/// `isDeclarationFileName` (`.d.ts`, `.d.mts`, `.d.cts`, `.d.<ext>.ts`).
/// Returns `None` if no declaration files exist or none have readable
/// metadata.
pub(super) fn find_latest_dts_file(emitted_files: &[PathBuf], base_dir: &Path) -> Option<String> {
    let latest = emitted_files
        .iter()
        .filter(|p| tsz_common::file_extensions::is_ts_declaration_file(p))
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

/// Compute tsc's `getCommonSourceDirectory()` for the emit layout when no
/// explicit `rootDir` is in play: the longest common directory of the emittable
/// source files, with declaration files and `node_modules` sources excluded so
/// they cannot drag the inferred root upward.
///
/// This is the root tsc lays output out against for an explicit file list with
/// no tsconfig, so `tsz src/a.ts --outDir out` emits `out/a.js` like tsc rather
/// than the cwd-relative `out/src/a.js`. Returns `None` when that directory
/// coincides with `base_dir` (the emitter's `base_dir` fallback already
/// produces the right layout) or when there are no emittable files.
pub(super) fn emit_common_source_directory<I>(
    program_file_paths: I,
    base_dir: &Path,
    cwd: &Path,
) -> Option<PathBuf>
where
    I: IntoIterator<Item = PathBuf>,
{
    let emittable: Vec<PathBuf> = program_file_paths
        .into_iter()
        .filter(|path| {
            !path
                .components()
                .any(|component| component.as_os_str() == "node_modules")
        })
        .collect();
    implicit_common_source_directory(&emittable, base_dir, cwd)
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
    fn emit_common_source_directory_single_file_uses_file_directory() {
        // `tsz src/a.ts --outDir out` (no tsconfig, no rootDir): tsc lays output
        // relative to the file's directory, not the cwd, so the root is src.
        let files = vec![PathBuf::from("/proj/src/a.ts")];
        let got = emit_common_source_directory(files, Path::new("/proj"), Path::new("/proj"));
        assert_eq!(got, Some(PathBuf::from("/proj/src")));
    }

    #[test]
    fn emit_common_source_directory_multi_file_uses_longest_common_directory() {
        let files = vec![
            PathBuf::from("/proj/src/a.ts"),
            PathBuf::from("/proj/src/sub/c.ts"),
        ];
        let got = emit_common_source_directory(files, Path::new("/proj"), Path::new("/proj"));
        assert_eq!(got, Some(PathBuf::from("/proj/src")));
    }

    #[test]
    fn emit_common_source_directory_none_when_common_equals_base_dir() {
        // Common directory coincides with base_dir: the base_dir fallback already
        // produces the right layout, so there is nothing to override.
        let files = vec![PathBuf::from("/proj/a.ts"), PathBuf::from("/proj/b.ts")];
        let got = emit_common_source_directory(files, Path::new("/proj"), Path::new("/proj"));
        assert_eq!(got, None);
    }

    #[test]
    fn emit_common_source_directory_excludes_node_modules_and_declaration_sources() {
        // node_modules and `.d.ts` sources must not drag the common directory up.
        let files = vec![
            PathBuf::from("/proj/src/a.ts"),
            PathBuf::from("/proj/src/sub/c.ts"),
            PathBuf::from("/proj/node_modules/dep/index.ts"),
            PathBuf::from("/proj/types/global.d.ts"),
        ];
        let got = emit_common_source_directory(files, Path::new("/proj"), Path::new("/proj"));
        assert_eq!(got, Some(PathBuf::from("/proj/src")));
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
    fn cli_ignore_deprecations_7_0_not_6_0() {
        let args = CliArgs::try_parse_from(["tsz", "--ignoreDeprecations", "7.0"]).unwrap();
        assert!(!cli_ignore_deprecations_silences_6_0(&args));
    }

    #[test]
    fn cli_ts7_removed_options_use_shared_ts5102_ts5108_policy() {
        let cases: &[(&[&str], u32, &str)] = &[
            (&["--target", "es5"], 5108, "target=ES5"),
            (
                &["--moduleResolution", "node"],
                5108,
                "moduleResolution=node10",
            ),
            (&["--module", "amd"], 5108, "module=AMD"),
            (&["--alwaysStrict", "false"], 5108, "alwaysStrict=false"),
            (
                &["--allowSyntheticDefaultImports", "false"],
                5108,
                "allowSyntheticDefaultImports=false",
            ),
            (
                &["--__explicitly-disabled-bool-flag=esModuleInterop"],
                5108,
                "esModuleInterop=false",
            ),
            (&["--baseUrl", "."], 5102, "baseUrl"),
            (&["--outFile", "bundle.js"], 5102, "outFile"),
            (
                &["--__explicitly-disabled-bool-flag=downlevelIteration"],
                5102,
                "downlevelIteration",
            ),
        ];

        for (options, code, message_fragment) in cases {
            let args =
                CliArgs::try_parse_from(std::iter::once("tsz").chain(options.iter().copied()))
                    .unwrap();
            let diagnostics = validate_cli_compiler_option_diagnostics(&args, None).unwrap();
            assert!(
                diagnostics.iter().any(|diagnostic| {
                    diagnostic.code == *code && diagnostic.message_text.contains(message_fragment)
                }),
                "{options:?} should emit TS{code} for {message_fragment}, got {diagnostics:?}"
            );
        }
    }

    #[test]
    fn cli_ts7_unparsed_legacy_enum_values_use_ts6046() {
        for options in [["--target", "es3"], ["--module", "none"]] {
            let args =
                CliArgs::try_parse_from(std::iter::once("tsz").chain(options.iter().copied()))
                    .unwrap();
            let diagnostics = validate_cli_compiler_option_diagnostics(&args, None).unwrap();
            assert!(
                diagnostics.iter().any(|diagnostic| diagnostic.code == 6046),
                "{options:?} should emit TS6046, got {diagnostics:?}"
            );
            assert!(
                diagnostics.iter().all(|diagnostic| diagnostic.code != 5108),
                "{options:?} must not emit TS5108, got {diagnostics:?}"
            );
        }
    }

    #[test]
    fn ordered_direct_cli_parse_diagnostics_follow_side_channel() {
        let diagnostics_for = |order: [&str; 2]| {
            let mut argv = vec![
                "tsz".to_string(),
                "--target".to_string(),
                "es3".to_string(),
                "--keyofStringsOnly".to_string(),
            ];
            argv.extend(
                order
                    .into_iter()
                    .map(|name| format!("--__direct-cli-option-order={name}")),
            );
            let args = CliArgs::try_parse_from(argv).unwrap();
            ordered_direct_cli_parse_diagnostics(&args)
                .unwrap()
                .into_iter()
                .map(|diagnostic| diagnostic.code)
                .collect::<Vec<_>>()
        };

        assert_eq!(
            diagnostics_for(["target", "keyofStringsOnly"]),
            [6046, 5023]
        );
        assert_eq!(
            diagnostics_for(["keyofStringsOnly", "target"]),
            [5023, 6046]
        );
    }

    #[test]
    fn apply_cli_overrides_no_check_sets_option() {
        let mut options = ResolvedCompilerOptions::default();
        let args = CliArgs::try_parse_from(["tsz", "--noCheck"]).unwrap();
        apply_cli_overrides(&mut options, &args).unwrap();
        assert!(options.no_check);
    }

    #[test]
    fn apply_cli_overrides_types_versions_compiler_version_sets_option() {
        let mut options = ResolvedCompilerOptions::default();
        let args =
            CliArgs::try_parse_from(["tsz", "--typesVersionsCompilerVersion", "5.6.1"]).unwrap();
        apply_cli_overrides(&mut options, &args).unwrap();
        assert_eq!(
            options.types_versions_compiler_version.as_deref(),
            Some("5.6.1")
        );
    }

    #[test]
    fn apply_cli_overrides_types_versions_compiler_version_uses_env_fallback() {
        let mut options = ResolvedCompilerOptions::default();
        let args = CliArgs::try_parse_from(["tsz"]).unwrap();
        crate::driver::with_types_versions_env(Some(" 5.5.4 "), || {
            apply_cli_overrides(&mut options, &args).unwrap();
        });
        assert_eq!(
            options.types_versions_compiler_version.as_deref(),
            Some("5.5.4")
        );
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
    fn apply_cli_overrides_preserve_const_enums_sets_checker_and_printer() {
        // `--preserveConstEnums` must reach the checker's copy of the option,
        // not just the printer's: the checker consults it to decide whether an
        // unreachable `const enum` still "affects control flow" for TS7027
        // (an erased const enum does not; a preserved one does), matching
        // tsc's `preserveConstEnums`-gated `ModuleInstanceState` check. Only
        // wiring `options.printer.preserve_const_enums` left the checker
        // permanently reading the default `false`, silently erasing this
        // reachability distinction for any CLI invocation (the tsconfig.json
        // path already set both fields via `resolved_options.rs`).
        let mut options = ResolvedCompilerOptions::default();
        let args = CliArgs::try_parse_from(["tsz", "--preserveConstEnums"]).unwrap();
        apply_cli_overrides(&mut options, &args).unwrap();
        assert!(options.checker.preserve_const_enums);
        assert!(options.printer.preserve_const_enums);
    }

    #[test]
    fn apply_cli_overrides_no_preserve_const_enums_leaves_checker_default() {
        let mut options = ResolvedCompilerOptions::default();
        let args = CliArgs::try_parse_from(["tsz"]).unwrap();
        apply_cli_overrides(&mut options, &args).unwrap();
        assert!(!options.checker.preserve_const_enums);
        assert!(!options.printer.preserve_const_enums);
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
        assert!(common == Path::new("/") || common.as_os_str().is_empty());
    }
}

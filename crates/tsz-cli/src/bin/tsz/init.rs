//! `--init` command: render and write a `tsconfig.json` template.
//!
//! This module owns the entire `--init` surface — recognizing which compiler
//! options the user passed, formatting their values as JSONC literals, and
//! rendering the template tsc 6.x emits. It is pure aside from `handle_init`'s
//! single file write, so it stays decoupled from the rest of the entrypoint.

use anyhow::{Context, Result};
use std::ffi::OsString;
use std::path::{Path, PathBuf};

use tsz_cli::args::CliArgs;

pub(crate) fn handle_init(args: &CliArgs, cwd: &std::path::Path) -> Result<()> {
    let tsconfig_path = cwd.join("tsconfig.json");
    if tsconfig_path.exists() {
        println!(
            "error TS5054: A 'tsconfig.json' file is already defined at: '{}'.",
            tsconfig_path.display()
        );
        std::process::exit(0);
    }

    let raw_args: Vec<OsString> = std::env::args_os().skip(1).collect();
    let overrides = collect_init_overrides(&raw_args, args);
    let config = render_init_template(&overrides);

    std::fs::write(&tsconfig_path, config).with_context(|| {
        format!(
            "failed to write tsconfig.json to {}",
            tsconfig_path.display()
        )
    })?;

    println!("\nCreated a new tsconfig.json\n\nYou can learn more at https://aka.ms/tsconfig");

    Ok(())
}

/// Walk the original CLI args in order and collect (canonical option name,
/// JSON-formatted value) pairs for every recognized compiler option the user
/// passed. Later occurrences supersede earlier ones (matching tsc's
/// last-write-wins behavior). Order is preserved so that options not in the
/// fixed `--init` template are appended in the order they appear.
pub(crate) fn collect_init_overrides(
    raw_args: &[OsString],
    args: &CliArgs,
) -> Vec<(&'static str, String)> {
    let mut overrides: Vec<(&'static str, String)> = Vec::new();
    let mut i = 0;
    while i < raw_args.len() {
        let arg = raw_args[i].to_string_lossy().to_string();
        if !arg.starts_with("--") || arg == "--" {
            i += 1;
            continue;
        }
        let (flag, has_inline_value) = match arg.find('=') {
            Some(eq) => (arg[..eq].to_string(), true),
            None => (arg.clone(), false),
        };
        let canonical = canonicalize_init_option(&flag);
        let takes_value = canonical.is_some_and(init_option_takes_value);
        if let Some(name) = canonical
            && let Some(value) = init_option_value(name, args)
        {
            if let Some(pos) = overrides.iter().position(|(k, _)| *k == name) {
                overrides[pos] = (name, value);
            } else {
                overrides.push((name, value));
            }
        }
        if takes_value && !has_inline_value {
            i += 2;
        } else {
            i += 1;
        }
    }
    overrides
}

/// Returns the canonical (camelCase) compiler-option name for a CLI flag.
/// Matching is case-insensitive and ignores `-` characters so that
/// `--rootDir`, `--rootdir`, and `--root-dir` all map to `"rootDir"`.
fn canonicalize_init_option(flag: &str) -> Option<&'static str> {
    let key: String = flag
        .trim_start_matches('-')
        .chars()
        .filter(|c| *c != '-')
        .flat_map(char::to_lowercase)
        .collect();
    INIT_OPTION_TABLE.iter().find_map(|(canonical, _)| {
        let canonical_key: String = canonical
            .chars()
            .filter(|c| *c != '-')
            .flat_map(char::to_lowercase)
            .collect();
        (canonical_key == key).then_some(*canonical)
    })
}

#[derive(Clone, Copy)]
enum InitOptionKind {
    /// Boolean flag (`--strict`, `--strict false`, `--strict true`).
    Bool,
    /// Flag that requires a value (`--target esnext`, `--lib es2015,dom`).
    Value,
}

fn init_option_takes_value(name: &str) -> bool {
    INIT_OPTION_TABLE
        .iter()
        .find(|(n, _)| *n == name)
        .is_some_and(|(_, k)| matches!(k, InitOptionKind::Value))
}

/// Recognized compiler options for the `--init` flow.
///
/// The set is intentionally small relative to the full CLI surface: it covers
/// every option that has a slot in the default template plus the most common
/// command-line options that tsc users pass alongside `--init`. Unrecognized
/// options are silently ignored, matching `tsc`.
const INIT_OPTION_TABLE: &[(&str, InitOptionKind)] = &[
    // Language and environment
    ("target", InitOptionKind::Value),
    ("module", InitOptionKind::Value),
    ("moduleResolution", InitOptionKind::Value),
    ("moduleDetection", InitOptionKind::Value),
    ("jsx", InitOptionKind::Value),
    ("jsxFactory", InitOptionKind::Value),
    ("jsxFragmentFactory", InitOptionKind::Value),
    ("jsxImportSource", InitOptionKind::Value),
    ("lib", InitOptionKind::Value),
    ("types", InitOptionKind::Value),
    ("typeRoots", InitOptionKind::Value),
    ("rootDir", InitOptionKind::Value),
    ("outDir", InitOptionKind::Value),
    ("outFile", InitOptionKind::Value),
    ("baseUrl", InitOptionKind::Value),
    ("declarationDir", InitOptionKind::Value),
    ("newLine", InitOptionKind::Value),
    ("noLib", InitOptionKind::Bool),
    // Emit / output
    ("declaration", InitOptionKind::Bool),
    ("declarationMap", InitOptionKind::Bool),
    ("sourceMap", InitOptionKind::Bool),
    ("inlineSourceMap", InitOptionKind::Bool),
    ("inlineSources", InitOptionKind::Bool),
    ("emitDeclarationOnly", InitOptionKind::Bool),
    ("noEmit", InitOptionKind::Bool),
    ("noEmitOnError", InitOptionKind::Bool),
    ("noEmitHelpers", InitOptionKind::Bool),
    ("importHelpers", InitOptionKind::Bool),
    ("downlevelIteration", InitOptionKind::Bool),
    ("removeComments", InitOptionKind::Bool),
    ("preserveConstEnums", InitOptionKind::Bool),
    ("emitBOM", InitOptionKind::Bool),
    // Interop / modules
    ("esModuleInterop", InitOptionKind::Bool),
    ("allowSyntheticDefaultImports", InitOptionKind::Bool),
    ("isolatedModules", InitOptionKind::Bool),
    ("isolatedDeclarations", InitOptionKind::Bool),
    ("verbatimModuleSyntax", InitOptionKind::Bool),
    ("forceConsistentCasingInFileNames", InitOptionKind::Bool),
    ("preserveSymlinks", InitOptionKind::Bool),
    ("erasableSyntaxOnly", InitOptionKind::Bool),
    ("resolveJsonModule", InitOptionKind::Bool),
    ("noResolve", InitOptionKind::Bool),
    ("allowUmdGlobalAccess", InitOptionKind::Bool),
    ("noUncheckedSideEffectImports", InitOptionKind::Bool),
    ("allowImportingTsExtensions", InitOptionKind::Bool),
    ("rewriteRelativeImportExtensions", InitOptionKind::Bool),
    ("allowArbitraryExtensions", InitOptionKind::Bool),
    // JavaScript support
    ("allowJs", InitOptionKind::Bool),
    ("checkJs", InitOptionKind::Bool),
    // Decorators
    ("experimentalDecorators", InitOptionKind::Bool),
    ("emitDecoratorMetadata", InitOptionKind::Bool),
    // Type checking
    ("strict", InitOptionKind::Bool),
    ("noImplicitAny", InitOptionKind::Bool),
    ("strictNullChecks", InitOptionKind::Bool),
    ("strictFunctionTypes", InitOptionKind::Bool),
    ("strictBindCallApply", InitOptionKind::Bool),
    ("strictPropertyInitialization", InitOptionKind::Bool),
    ("strictBuiltinIteratorReturn", InitOptionKind::Bool),
    ("noImplicitThis", InitOptionKind::Bool),
    ("useUnknownInCatchVariables", InitOptionKind::Bool),
    ("alwaysStrict", InitOptionKind::Bool),
    ("noUnusedLocals", InitOptionKind::Bool),
    ("noUnusedParameters", InitOptionKind::Bool),
    ("exactOptionalPropertyTypes", InitOptionKind::Bool),
    ("noImplicitReturns", InitOptionKind::Bool),
    ("noFallthroughCasesInSwitch", InitOptionKind::Bool),
    ("noUncheckedIndexedAccess", InitOptionKind::Bool),
    ("noImplicitOverride", InitOptionKind::Bool),
    ("noPropertyAccessFromIndexSignature", InitOptionKind::Bool),
    ("allowUnreachableCode", InitOptionKind::Bool),
    ("allowUnusedLabels", InitOptionKind::Bool),
    ("useDefineForClassFields", InitOptionKind::Bool),
    // Completeness
    ("skipDefaultLibCheck", InitOptionKind::Bool),
    ("skipLibCheck", InitOptionKind::Bool),
    // Projects
    ("composite", InitOptionKind::Bool),
    ("incremental", InitOptionKind::Bool),
    // Diagnostics / output formatting
    ("diagnostics", InitOptionKind::Bool),
    ("extendedDiagnostics", InitOptionKind::Bool),
    ("explainFiles", InitOptionKind::Bool),
    ("listFiles", InitOptionKind::Bool),
    ("listEmittedFiles", InitOptionKind::Bool),
    ("traceResolution", InitOptionKind::Bool),
    ("noCheck", InitOptionKind::Bool),
    ("noErrorTruncation", InitOptionKind::Bool),
    ("preserveWatchOutput", InitOptionKind::Bool),
    ("pretty", InitOptionKind::Bool),
];

/// Format the user-supplied value for a recognized option as a JSON literal.
/// Returns `None` if the option is recognized but the parsed `args` struct
/// does not carry a meaningful value (e.g., a `bool` field is `false` because
/// the option was never on the command line — but in that case the caller
/// would not invoke this function).
fn init_option_value(name: &'static str, args: &CliArgs) -> Option<String> {
    match name {
        "target" => args.target.map(|t| json_str(target_init_str(t))),
        "module" => args.module.map(|m| json_str(module_init_str(m))),
        "moduleResolution" => args
            .module_resolution
            .map(|m| json_str(module_resolution_init_str(m))),
        "moduleDetection" => args
            .module_detection
            .map(|m| json_str(module_detection_init_str(m))),
        "jsx" => args.jsx.map(|j| json_str(jsx_init_str(j))),
        "jsxFactory" => args.jsx_factory.as_deref().map(json_str),
        "jsxFragmentFactory" => args.jsx_fragment_factory.as_deref().map(json_str),
        "jsxImportSource" => args.jsx_import_source.as_deref().map(json_str),
        "newLine" => args.new_line.map(|n| json_str(new_line_init_str(n))),
        "lib" => args.lib.as_ref().map(|v| json_str_array(v)),
        "types" => args.types.as_ref().map(|v| json_str_array(v)),
        "typeRoots" => args
            .type_roots
            .as_ref()
            .map(|v| json_path_array(v.iter().map(PathBuf::as_path))),
        "rootDir" => args.root_dir.as_deref().map(json_path),
        "outDir" => args.out_dir.as_deref().map(json_path),
        "outFile" => args.out_file.as_deref().map(json_path),
        "baseUrl" => args.base_url.as_deref().map(json_path),
        "declarationDir" => args.declaration_dir.as_deref().map(json_path),
        // Plain bool flags. The preprocessor in `preprocess_args` strips
        // `--flag false` pairs and either flips the field to `false` directly
        // or records the flag name in `explicitly_disabled_bool_flags`. By
        // the time we get here, `args.<field>` already reflects the user's
        // intent.
        "noLib" => Some(bool_str(args.no_lib)),
        "declaration" => Some(bool_str(args.declaration)),
        "declarationMap" => Some(bool_str(args.declaration_map)),
        "sourceMap" => Some(bool_str(args.source_map)),
        "inlineSourceMap" => Some(bool_str(args.inline_source_map)),
        "inlineSources" => Some(bool_str(args.inline_sources)),
        "emitDeclarationOnly" => Some(bool_str(args.emit_declaration_only)),
        "noEmit" => Some(bool_str(args.no_emit)),
        "noEmitOnError" => Some(bool_str(args.no_emit_on_error)),
        "noEmitHelpers" => Some(bool_str(args.no_emit_helpers)),
        "importHelpers" => Some(bool_str(args.import_helpers)),
        "downlevelIteration" => Some(bool_str(args.downlevel_iteration)),
        "removeComments" => Some(bool_str(args.remove_comments)),
        "preserveConstEnums" => Some(bool_str(args.preserve_const_enums)),
        "emitBOM" => Some(bool_str(args.emit_bom)),
        "esModuleInterop" => Some(bool_str(args.es_module_interop)),
        "isolatedModules" => Some(bool_str(args.isolated_modules)),
        "isolatedDeclarations" => Some(bool_str(args.isolated_declarations)),
        "verbatimModuleSyntax" => Some(bool_str(args.verbatim_module_syntax)),
        "preserveSymlinks" => Some(bool_str(args.preserve_symlinks)),
        "erasableSyntaxOnly" => Some(bool_str(args.erasable_syntax_only)),
        "resolveJsonModule" => Some(bool_str(args.resolve_json_module)),
        "noResolve" => Some(bool_str(args.no_resolve)),
        "allowUmdGlobalAccess" => Some(bool_str(args.allow_umd_global_access)),
        "noUncheckedSideEffectImports" => Some(bool_str(args.no_unchecked_side_effect_imports)),
        "allowImportingTsExtensions" => Some(bool_str(args.allow_importing_ts_extensions)),
        "rewriteRelativeImportExtensions" => {
            Some(bool_str(args.rewrite_relative_import_extensions))
        }
        "allowArbitraryExtensions" => Some(bool_str(args.allow_arbitrary_extensions)),
        "allowJs" => Some(bool_str(args.allow_js)),
        "checkJs" => Some(bool_str(args.check_js)),
        "experimentalDecorators" => Some(bool_str(args.experimental_decorators)),
        "emitDecoratorMetadata" => Some(bool_str(args.emit_decorator_metadata)),
        "strict" => Some(bool_str(args.strict)),
        "noUnusedLocals" => Some(bool_str(args.no_unused_locals)),
        "noUnusedParameters" => Some(bool_str(args.no_unused_parameters)),
        "exactOptionalPropertyTypes" => Some(bool_str(args.exact_optional_property_types)),
        "noImplicitReturns" => Some(bool_str(args.no_implicit_returns)),
        "noFallthroughCasesInSwitch" => Some(bool_str(args.no_fallthrough_cases_in_switch)),
        "noUncheckedIndexedAccess" => Some(bool_str(args.no_unchecked_indexed_access)),
        "noImplicitOverride" => Some(bool_str(args.no_implicit_override)),
        "noPropertyAccessFromIndexSignature" => {
            Some(bool_str(args.no_property_access_from_index_signature))
        }
        "skipDefaultLibCheck" => Some(bool_str(args.skip_default_lib_check)),
        "skipLibCheck" => Some(bool_str(args.skip_lib_check)),
        "composite" => Some(bool_str(args.composite)),
        "incremental" => Some(bool_str(args.incremental)),
        "diagnostics" => Some(bool_str(args.diagnostics)),
        "extendedDiagnostics" => Some(bool_str(args.extended_diagnostics)),
        "explainFiles" => Some(bool_str(args.explain_files)),
        "listFiles" => Some(bool_str(args.list_files)),
        "listEmittedFiles" => Some(bool_str(args.list_emitted_files)),
        "traceResolution" => Some(bool_str(args.trace_resolution)),
        "noCheck" => Some(bool_str(args.no_check)),
        "noErrorTruncation" => Some(bool_str(args.no_error_truncation)),
        "preserveWatchOutput" => Some(bool_str(args.preserve_watch_output)),
        // Tri-state Option<bool> flags.
        "pretty" => args.pretty.map(bool_str),
        "noImplicitAny" => args.no_implicit_any.map(bool_str),
        "strictNullChecks" => args.strict_null_checks.map(bool_str),
        "strictFunctionTypes" => args.strict_function_types.map(bool_str),
        "strictBindCallApply" => args.strict_bind_call_apply.map(bool_str),
        "strictPropertyInitialization" => args.strict_property_initialization.map(bool_str),
        "strictBuiltinIteratorReturn" => args.strict_builtin_iterator_return.map(bool_str),
        "noImplicitThis" => args.no_implicit_this.map(bool_str),
        "useUnknownInCatchVariables" => args.use_unknown_in_catch_variables.map(bool_str),
        "alwaysStrict" => args.always_strict.map(bool_str),
        "allowSyntheticDefaultImports" => args.allow_synthetic_default_imports.map(bool_str),
        "forceConsistentCasingInFileNames" => {
            args.force_consistent_casing_in_file_names.map(bool_str)
        }
        "allowUnreachableCode" => args.allow_unreachable_code.map(bool_str),
        "allowUnusedLabels" => args.allow_unused_labels.map(bool_str),
        "useDefineForClassFields" => args.use_define_for_class_fields.map(bool_str),
        _ => None,
    }
}

fn bool_str(b: bool) -> String {
    if b { "true".into() } else { "false".into() }
}

fn json_str(s: &str) -> String {
    // Escape backslashes and double quotes; tsconfig is JSONC and our values
    // are user-supplied strings or paths.
    let mut escaped = String::with_capacity(s.len() + 2);
    escaped.push('"');
    for c in s.chars() {
        match c {
            '\\' => escaped.push_str("\\\\"),
            '"' => escaped.push_str("\\\""),
            _ => escaped.push(c),
        }
    }
    escaped.push('"');
    escaped
}

fn json_path(p: &Path) -> String {
    // tsc emits paths with forward slashes; do the same so that snapshots are
    // stable on Windows-style inputs and so that the path round-trips through
    // tsconfig parsing.
    json_str(&p.to_string_lossy().replace('\\', "/"))
}

fn json_str_array(values: &[String]) -> String {
    if values.is_empty() {
        return "[]".into();
    }
    let items: Vec<String> = values.iter().map(|v| json_str(v)).collect();
    format!("[{}]", items.join(", "))
}

fn json_path_array<'a, I: Iterator<Item = &'a Path>>(paths: I) -> String {
    let items: Vec<String> = paths.map(json_path).collect();
    if items.is_empty() {
        "[]".into()
    } else {
        format!("[{}]", items.join(", "))
    }
}

const fn target_init_str(t: tsz_cli::args::Target) -> &'static str {
    use tsz_cli::args::Target;
    match t {
        Target::Es3 => "es3",
        Target::Es5 => "es5",
        // tsc canonicalizes ES2015 to "es6".
        Target::Es2015 => "es6",
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

const fn module_init_str(m: tsz_cli::args::Module) -> &'static str {
    use tsz_cli::args::Module;
    match m {
        Module::None => "none",
        Module::CommonJs => "commonjs",
        Module::Amd => "amd",
        Module::Umd => "umd",
        Module::System => "system",
        // tsc canonicalizes ES2015 to "es6" for module too.
        Module::Es2015 => "es6",
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

const fn module_resolution_init_str(m: tsz_cli::args::ModuleResolution) -> &'static str {
    use tsz_cli::args::ModuleResolution;
    match m {
        ModuleResolution::Classic => "classic",
        // tsc emits "node10" as the canonical name for Node10/node.
        ModuleResolution::Node10 => "node10",
        ModuleResolution::Node16 => "node16",
        ModuleResolution::NodeNext => "nodenext",
        ModuleResolution::Bundler => "bundler",
    }
}

const fn module_detection_init_str(m: tsz_cli::args::ModuleDetection) -> &'static str {
    use tsz_cli::args::ModuleDetection;
    match m {
        ModuleDetection::Auto => "auto",
        ModuleDetection::Force => "force",
        ModuleDetection::Legacy => "legacy",
    }
}

const fn jsx_init_str(j: tsz_cli::args::JsxEmit) -> &'static str {
    use tsz_cli::args::JsxEmit;
    match j {
        JsxEmit::Preserve => "preserve",
        JsxEmit::React => "react",
        JsxEmit::ReactJsx => "react-jsx",
        JsxEmit::ReactJsxDev => "react-jsxdev",
        JsxEmit::ReactNative => "react-native",
    }
}

const fn new_line_init_str(n: tsz_cli::args::NewLine) -> &'static str {
    use tsz_cli::args::NewLine;
    match n {
        NewLine::Crlf => "crlf",
        NewLine::Lf => "lf",
    }
}

fn emit_init_line(
    out: &mut String,
    map: &std::collections::HashMap<&'static str, &str>,
    key: &'static str,
    default_value: &str,
    comment_default: bool,
) {
    if let Some(value) = map.get(key) {
        out.push_str("    \"");
        out.push_str(key);
        out.push_str("\": ");
        out.push_str(value);
        out.push_str(",\n");
    } else if comment_default {
        out.push_str("    // \"");
        out.push_str(key);
        out.push_str("\": ");
        out.push_str(default_value);
        out.push_str(",\n");
    } else {
        out.push_str("    \"");
        out.push_str(key);
        out.push_str("\": ");
        out.push_str(default_value);
        out.push_str(",\n");
    }
}

/// Render the `tsconfig.json` body using the JSONC template that tsc 6.x
/// emits, with each templated option replaced by the user-provided value (if
/// any). Options that the user passed but that don't have a slot in the
/// template are appended after the template body in CLI order, matching tsc.
pub(crate) fn render_init_template(overrides: &[(&'static str, String)]) -> String {
    use std::collections::HashMap;
    let map: HashMap<&'static str, &str> =
        overrides.iter().map(|(k, v)| (*k, v.as_str())).collect();

    let mut out = String::new();
    out.push_str("{\n");
    out.push_str("  // Visit https://aka.ms/tsconfig to read more about this file\n");
    out.push_str("  \"compilerOptions\": {\n");

    out.push_str("    // File Layout\n");
    emit_init_line(&mut out, &map, "rootDir", "\"./src\"", true);
    emit_init_line(&mut out, &map, "outDir", "\"./dist\"", true);
    out.push('\n');
    out.push_str("    // Environment Settings\n");
    out.push_str("    // See also https://aka.ms/tsconfig/module\n");
    emit_init_line(&mut out, &map, "module", "\"nodenext\"", false);
    emit_init_line(&mut out, &map, "target", "\"esnext\"", false);
    emit_init_line(&mut out, &map, "types", "[]", false);
    out.push_str("    // For nodejs:\n");
    out.push_str("    // \"lib\": [\"esnext\"],\n");
    out.push_str("    // \"types\": [\"node\"],\n");
    out.push_str("    // and npm install -D @types/node\n");
    out.push('\n');
    out.push_str("    // Other Outputs\n");
    emit_init_line(&mut out, &map, "sourceMap", "true", false);
    emit_init_line(&mut out, &map, "declaration", "true", false);
    emit_init_line(&mut out, &map, "declarationMap", "true", false);
    out.push('\n');
    out.push_str("    // Stricter Typechecking Options\n");
    emit_init_line(&mut out, &map, "noUncheckedIndexedAccess", "true", false);
    emit_init_line(&mut out, &map, "exactOptionalPropertyTypes", "true", false);
    out.push('\n');
    out.push_str("    // Style Options\n");
    emit_init_line(&mut out, &map, "noImplicitReturns", "true", true);
    emit_init_line(&mut out, &map, "noImplicitOverride", "true", true);
    emit_init_line(&mut out, &map, "noUnusedLocals", "true", true);
    emit_init_line(&mut out, &map, "noUnusedParameters", "true", true);
    emit_init_line(&mut out, &map, "noFallthroughCasesInSwitch", "true", true);
    emit_init_line(
        &mut out,
        &map,
        "noPropertyAccessFromIndexSignature",
        "true",
        true,
    );
    out.push('\n');
    out.push_str("    // Recommended Options\n");
    emit_init_line(&mut out, &map, "strict", "true", false);
    emit_init_line(&mut out, &map, "jsx", "\"react-jsx\"", false);
    emit_init_line(&mut out, &map, "verbatimModuleSyntax", "true", false);
    emit_init_line(&mut out, &map, "isolatedModules", "true", false);
    emit_init_line(
        &mut out,
        &map,
        "noUncheckedSideEffectImports",
        "true",
        false,
    );
    emit_init_line(&mut out, &map, "moduleDetection", "\"force\"", false);
    emit_init_line(&mut out, &map, "skipLibCheck", "true", false);

    // Append any overrides that don't have a slot in the template, preserving
    // the order they appeared on the command line. tsc emits these after a
    // single blank line separating them from the recommended-options block.
    let template_keys: &[&str] = &[
        "rootDir",
        "outDir",
        "module",
        "target",
        "types",
        "sourceMap",
        "declaration",
        "declarationMap",
        "noUncheckedIndexedAccess",
        "exactOptionalPropertyTypes",
        "noImplicitReturns",
        "noImplicitOverride",
        "noUnusedLocals",
        "noUnusedParameters",
        "noFallthroughCasesInSwitch",
        "noPropertyAccessFromIndexSignature",
        "strict",
        "jsx",
        "verbatimModuleSyntax",
        "isolatedModules",
        "noUncheckedSideEffectImports",
        "moduleDetection",
        "skipLibCheck",
    ];
    let mut appended_any = false;
    for (key, value) in overrides.iter() {
        if template_keys.contains(key) {
            continue;
        }
        if !appended_any {
            out.push('\n');
            appended_any = true;
        }
        out.push_str("    \"");
        out.push_str(key);
        out.push_str("\": ");
        out.push_str(value);
        out.push_str(",\n");
    }

    out.push_str("  }\n");
    out.push_str("}\n");
    out
}

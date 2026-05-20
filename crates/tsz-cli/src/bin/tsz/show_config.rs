use std::path::{Path, PathBuf};

use tsz_cli::args::CliArgs;
use tsz_cli::config::{CompilerOptions, TsConfig};

/// Build the merged compiler-options JSON map for `--showConfig`.
///
/// Five sub-steps run in order:
/// 1. Convert the resolved `TsConfig` compiler options to JSON.
/// 2. Merge CLI flag overrides (CLI wins over tsconfig).
/// 3. Add implied options (options derived from explicitly set options).
/// 4. Relativize resolved absolute path options.
/// 5. Normalise outDir/outFile/rootDir/etc with a `./` prefix.
pub(super) fn build_compiler_options_map(
    config: Option<&TsConfig>,
    args: &CliArgs,
    base_dir: &Path,
) -> serde_json::Map<String, serde_json::Value> {
    let mut map = compiler_options_to_json(config.and_then(|c| c.compiler_options.as_ref()));
    apply_cli_overrides(&mut map, args);
    add_implied_options(&mut map);
    relativize_resolved_path_options(&mut map, base_dir);

    for path_key in &["outDir", "outFile", "rootDir", "declarationDir", "baseUrl"] {
        if let Some(serde_json::Value::String(s)) = map.get(*path_key) {
            let normalized = ensure_dot_slash_prefix(s);
            if normalized != *s {
                map.insert((*path_key).into(), serde_json::Value::String(normalized));
            }
        }
    }

    map
}

/// Return `(file_paths, effective_exclude)` for the `--showConfig` output.
///
/// `file_paths` is the resolved list of TypeScript files to display.
/// `effective_exclude` is the exclude array including any auto-added `outDir`.
pub(super) fn collect_files_and_excludes(
    args: &CliArgs,
    config: Option<&TsConfig>,
    base_dir: &Path,
    compiler_options_map: &serde_json::Map<String, serde_json::Value>,
) -> (Vec<String>, Vec<String>) {
    use tsz_cli::fs::{FileDiscoveryOptions, discover_ts_files};

    let allow_js = compiler_options_map
        .get("allowJs")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    let out_dir = compiler_options_map
        .get("outDir")
        .and_then(|v| v.as_str())
        .map(PathBuf::from);

    let explicit_files: Vec<PathBuf> = if !args.files.is_empty() {
        args.files.clone()
    } else if let Some(cfg) = config {
        cfg.files
            .as_ref()
            .map(|f| f.iter().map(PathBuf::from).collect())
            .unwrap_or_default()
    } else {
        Vec::new()
    };

    let files_explicitly_set =
        !args.files.is_empty() || config.and_then(|c| c.files.as_ref()).is_some();

    let file_paths: Vec<String> = if files_explicitly_set {
        explicit_files
            .iter()
            .map(|p| normalize_relative(base_dir, p))
            .collect()
    } else {
        let discovery = FileDiscoveryOptions {
            base_dir: base_dir.to_path_buf(),
            files: explicit_files,
            files_explicitly_set,
            include: config.and_then(|c| c.include.clone()),
            exclude: config.and_then(|c| c.exclude.clone()),
            out_dir: out_dir.clone(),
            follow_links: false,
            allow_js,
            resolve_json_module: compiler_options_map
                .get("resolveJsonModule")
                .and_then(|v| v.as_bool())
                .unwrap_or(false),
        };
        discover_ts_files(&discovery)
            .unwrap_or_default()
            .iter()
            .map(|f| {
                if let Ok(rel) = f.strip_prefix(base_dir) {
                    format!("./{}", rel.display())
                } else {
                    // File is outside base_dir (e.g. inherited via extends).
                    // Use diff_paths so the output stays relative.
                    tsz_cli::fs::diff_paths(f, base_dir)
                        .map(|rel| {
                            let s = rel.to_string_lossy().replace('\\', "/");
                            if s.starts_with("../") || s.starts_with('/') {
                                s
                            } else {
                                format!("./{s}")
                            }
                        })
                        .unwrap_or_else(|| f.display().to_string())
                }
            })
            .collect()
    };

    // Auto-add outDir to exclude (tsc behavior).
    let mut effective_exclude = config.and_then(|c| c.exclude.clone()).unwrap_or_default();
    if let Some(od) = &out_dir {
        let normalized_od = ensure_dot_slash_prefix(&od.display().to_string());
        if !effective_exclude.iter().any(|e| e == &normalized_od) {
            effective_exclude.push(normalized_od);
        }
    }

    (file_paths, effective_exclude)
}

/// Render the `--showConfig` JSON output string.
///
/// Pure function — no IO.  The caller prints the result.
pub(super) fn render_output(
    compiler_options_map: &serde_json::Map<String, serde_json::Value>,
    file_paths: &[String],
    effective_exclude: &[String],
    config: Option<&TsConfig>,
    base_dir: &Path,
) -> String {
    let mut output = String::from("{\n");

    // compilerOptions
    output.push_str("    \"compilerOptions\": {");
    if compiler_options_map.is_empty() {
        output.push('}');
    } else {
        output.push('\n');
        let map_len = compiler_options_map.len();
        for (i, (key, value)) in compiler_options_map.iter().enumerate() {
            output.push_str("        ");
            output.push_str(&format_key_value(key, value, 8));
            if i + 1 < map_len {
                output.push(',');
            }
            output.push('\n');
        }
        output.push_str("    }");
    }

    // references (tsc v6 ordering: compilerOptions, references, files, include, exclude)
    if let Some(cfg) = config
        && let Some(refs) = &cfg.references
    {
        output.push_str(",\n    \"references\": [\n");
        for (i, r) in refs.iter().enumerate() {
            if r.prepend {
                output.push_str(&format!(
                    "        {{\n            \"path\": \"{}\",\n            \"prepend\": true\n        }}",
                    r.path
                ));
            } else {
                output.push_str(&format!(
                    "        {{\n            \"path\": \"{}\"\n        }}",
                    r.path
                ));
            }
            if i + 1 < refs.len() {
                output.push(',');
            }
            output.push('\n');
        }
        output.push_str("    ]");
    }

    // files
    if !file_paths.is_empty() {
        output.push_str(",\n    \"files\": [\n");
        for (i, f) in file_paths.iter().enumerate() {
            output.push_str("        \"");
            output.push_str(f);
            output.push('"');
            if i + 1 < file_paths.len() {
                output.push(',');
            }
            output.push('\n');
        }
        output.push_str("    ]");
    }

    // include / exclude (only when a tsconfig was loaded)
    if let Some(cfg) = config {
        if let Some(include) = &cfg.include {
            output.push_str(",\n    \"include\": [\n");
            for (i, v) in include.iter().enumerate() {
                let display = display_selector(base_dir, v);
                output.push_str("        \"");
                output.push_str(&display);
                output.push('"');
                if i + 1 < include.len() {
                    output.push(',');
                }
                output.push('\n');
            }
            output.push_str("    ]");
        }
        if !effective_exclude.is_empty() {
            output.push_str(",\n    \"exclude\": [\n");
            for (i, v) in effective_exclude.iter().enumerate() {
                let display = display_selector(base_dir, v);
                output.push_str("        \"");
                output.push_str(&display);
                output.push('"');
                if i + 1 < effective_exclude.len() {
                    output.push(',');
                }
                output.push('\n');
            }
            output.push_str("    ]");
        }
    }

    output.push_str("\n}\n");
    output
}

fn compiler_options_to_json(
    opts: Option<&CompilerOptions>,
) -> serde_json::Map<String, serde_json::Value> {
    use serde_json::Value;
    let mut map = serde_json::Map::new();
    let Some(opts) = opts else { return map };

    if let Some(v) = &opts.target {
        let lowered = v.to_lowercase();
        let normalized = if lowered == "es2015" {
            "es6".to_string()
        } else {
            lowered
        };
        map.insert("target".into(), Value::String(normalized));
    }
    if let Some(v) = &opts.module {
        let lowered = v.to_lowercase();
        let normalized = if lowered == "es2015" {
            "es6".to_string()
        } else {
            lowered
        };
        map.insert("module".into(), Value::String(normalized));
    }
    if let Some(v) = &opts.module_resolution {
        map.insert("moduleResolution".into(), Value::String(v.to_lowercase()));
    }
    if let Some(v) = &opts.jsx {
        map.insert("jsx".into(), Value::String(v.to_lowercase()));
    }
    if let Some(v) = &opts.jsx_factory {
        map.insert("jsxFactory".into(), Value::String(v.clone()));
    }
    if let Some(v) = &opts.jsx_fragment_factory {
        map.insert("jsxFragmentFactory".into(), Value::String(v.clone()));
    }
    if let Some(v) = &opts.jsx_import_source {
        map.insert("jsxImportSource".into(), Value::String(v.clone()));
    }
    if let Some(v) = &opts.react_namespace {
        map.insert("reactNamespace".into(), Value::String(v.clone()));
    }
    if let Some(v) = &opts.base_url {
        map.insert("baseUrl".into(), Value::String(v.clone()));
    }
    if let Some(v) = &opts.root_dir {
        map.insert("rootDir".into(), Value::String(v.clone()));
    }
    if let Some(v) = &opts.root_dirs {
        map.insert(
            "rootDirs".into(),
            Value::Array(v.iter().map(|s| Value::String(s.clone())).collect()),
        );
    }
    if let Some(v) = &opts.out_dir {
        map.insert("outDir".into(), Value::String(v.clone()));
    }
    if let Some(v) = &opts.out_file {
        map.insert("outFile".into(), Value::String(v.clone()));
    }
    if let Some(v) = &opts.declaration_dir {
        map.insert("declarationDir".into(), Value::String(v.clone()));
    }
    if let Some(v) = &opts.ts_build_info_file {
        map.insert("tsBuildInfoFile".into(), Value::String(v.clone()));
    }
    if let Some(v) = &opts.module_detection {
        map.insert("moduleDetection".into(), Value::String(v.to_lowercase()));
    }
    if let Some(v) = &opts.ignore_deprecations {
        map.insert("ignoreDeprecations".into(), Value::String(v.clone()));
    }

    macro_rules! set_bool {
        ($f:ident, $k:expr) => {
            if let Some(v) = opts.$f {
                map.insert($k.into(), Value::Bool(v));
            }
        };
    }
    set_bool!(strict, "strict");
    set_bool!(no_emit, "noEmit");
    set_bool!(no_check, "noCheck");
    set_bool!(no_emit_on_error, "noEmitOnError");
    set_bool!(declaration, "declaration");
    set_bool!(emit_declaration_only, "emitDeclarationOnly");
    set_bool!(source_map, "sourceMap");
    set_bool!(inline_source_map, "inlineSourceMap");
    set_bool!(declaration_map, "declarationMap");
    set_bool!(composite, "composite");
    set_bool!(incremental, "incremental");
    set_bool!(isolated_modules, "isolatedModules");
    set_bool!(isolated_declarations, "isolatedDeclarations");
    set_bool!(verbatim_module_syntax, "verbatimModuleSyntax");
    set_bool!(es_module_interop, "esModuleInterop");
    set_bool!(
        allow_synthetic_default_imports,
        "allowSyntheticDefaultImports"
    );
    set_bool!(allow_js, "allowJs");
    set_bool!(check_js, "checkJs");
    set_bool!(skip_lib_check, "skipLibCheck");
    set_bool!(skip_default_lib_check, "skipDefaultLibCheck");
    set_bool!(strip_internal, "stripInternal");
    set_bool!(no_lib, "noLib");
    set_bool!(lib_replacement, "libReplacement");
    set_bool!(no_types_and_symbols, "noTypesAndSymbols");
    set_bool!(import_helpers, "importHelpers");
    set_bool!(no_emit_helpers, "noEmitHelpers");
    set_bool!(remove_comments, "removeComments");
    set_bool!(emit_bom, "emitBOM");
    set_bool!(no_implicit_any, "noImplicitAny");
    set_bool!(no_implicit_returns, "noImplicitReturns");
    set_bool!(strict_null_checks, "strictNullChecks");
    set_bool!(strict_function_types, "strictFunctionTypes");
    set_bool!(
        strict_property_initialization,
        "strictPropertyInitialization"
    );
    set_bool!(no_implicit_this, "noImplicitThis");
    set_bool!(use_unknown_in_catch_variables, "useUnknownInCatchVariables");
    set_bool!(exact_optional_property_types, "exactOptionalPropertyTypes");
    set_bool!(strict_bind_call_apply, "strictBindCallApply");
    set_bool!(
        strict_builtin_iterator_return,
        "strictBuiltinIteratorReturn"
    );
    set_bool!(no_unchecked_indexed_access, "noUncheckedIndexedAccess");
    set_bool!(
        no_property_access_from_index_signature,
        "noPropertyAccessFromIndexSignature"
    );
    set_bool!(no_unused_locals, "noUnusedLocals");
    set_bool!(no_unused_parameters, "noUnusedParameters");
    set_bool!(allow_unreachable_code, "allowUnreachableCode");
    set_bool!(allow_unused_labels, "allowUnusedLabels");
    set_bool!(no_fallthrough_cases_in_switch, "noFallthroughCasesInSwitch");
    set_bool!(no_resolve, "noResolve");
    set_bool!(
        no_unchecked_side_effect_imports,
        "noUncheckedSideEffectImports"
    );
    set_bool!(no_implicit_override, "noImplicitOverride");
    set_bool!(always_strict, "alwaysStrict");
    set_bool!(preserve_symlinks, "preserveSymlinks");
    set_bool!(use_define_for_class_fields, "useDefineForClassFields");
    set_bool!(experimental_decorators, "experimentalDecorators");
    set_bool!(emit_decorator_metadata, "emitDecoratorMetadata");
    set_bool!(allow_umd_global_access, "allowUmdGlobalAccess");
    set_bool!(resolve_package_json_exports, "resolvePackageJsonExports");
    set_bool!(resolve_package_json_imports, "resolvePackageJsonImports");
    set_bool!(resolve_json_module, "resolveJsonModule");
    set_bool!(allow_arbitrary_extensions, "allowArbitraryExtensions");
    set_bool!(allow_importing_ts_extensions, "allowImportingTsExtensions");
    set_bool!(
        rewrite_relative_import_extensions,
        "rewriteRelativeImportExtensions"
    );
    set_bool!(preserve_const_enums, "preserveConstEnums");
    set_bool!(erasable_syntax_only, "erasableSyntaxOnly");
    set_bool!(sound, "sound");

    if let Some(v) = &opts.new_line {
        map.insert("newLine".into(), Value::String(v.to_lowercase()));
    }
    if let Some(v) = opts.max_node_module_js_depth {
        map.insert(
            "maxNodeModuleJsDepth".into(),
            Value::Number(serde_json::Number::from(v)),
        );
    }
    if let Some(v) = &opts.lib {
        map.insert(
            "lib".into(),
            Value::Array(v.iter().map(|s| Value::String(s.clone())).collect()),
        );
    }
    if let Some(v) = &opts.types {
        map.insert(
            "types".into(),
            Value::Array(v.iter().map(|s| Value::String(s.clone())).collect()),
        );
    }
    if let Some(v) = &opts.type_roots {
        map.insert(
            "typeRoots".into(),
            Value::Array(v.iter().map(|s| Value::String(s.clone())).collect()),
        );
    }
    if let Some(v) = &opts.module_suffixes {
        map.insert(
            "moduleSuffixes".into(),
            Value::Array(v.iter().map(|s| Value::String(s.clone())).collect()),
        );
    }
    if let Some(v) = &opts.custom_conditions {
        map.insert(
            "customConditions".into(),
            Value::Array(v.iter().map(|s| Value::String(s.clone())).collect()),
        );
    }
    if let Some(paths) = &opts.paths {
        let mut paths_obj = serde_json::Map::new();
        for (pattern, targets) in paths {
            paths_obj.insert(
                pattern.clone(),
                Value::Array(targets.iter().map(|s| Value::String(s.clone())).collect()),
            );
        }
        map.insert("paths".into(), Value::Object(paths_obj));
    }
    map
}

fn apply_cli_overrides(map: &mut serde_json::Map<String, serde_json::Value>, args: &CliArgs) {
    use serde_json::Value;

    if let Some(target) = args.target {
        let s = match target {
            tsz_cli::args::Target::Es3 => "es3",
            tsz_cli::args::Target::Es5 => "es5",
            tsz_cli::args::Target::Es2015 => "es6",
            tsz_cli::args::Target::Es2016 => "es2016",
            tsz_cli::args::Target::Es2017 => "es2017",
            tsz_cli::args::Target::Es2018 => "es2018",
            tsz_cli::args::Target::Es2019 => "es2019",
            tsz_cli::args::Target::Es2020 => "es2020",
            tsz_cli::args::Target::Es2021 => "es2021",
            tsz_cli::args::Target::Es2022 => "es2022",
            tsz_cli::args::Target::Es2023 => "es2023",
            tsz_cli::args::Target::Es2024 => "es2024",
            tsz_cli::args::Target::Es2025 => "es2025",
            tsz_cli::args::Target::EsNext => "esnext",
        };
        map.insert("target".into(), Value::String(s.into()));
    }
    if let Some(module) = args.module {
        let s = match module {
            tsz_cli::args::Module::None => "none",
            tsz_cli::args::Module::CommonJs => "commonjs",
            tsz_cli::args::Module::Amd => "amd",
            tsz_cli::args::Module::Umd => "umd",
            tsz_cli::args::Module::System => "system",
            tsz_cli::args::Module::Es2015 => "es6",
            tsz_cli::args::Module::Es2020 => "es2020",
            tsz_cli::args::Module::Es2022 => "es2022",
            tsz_cli::args::Module::EsNext => "esnext",
            tsz_cli::args::Module::Node16 => "node16",
            tsz_cli::args::Module::Node18 => "node18",
            tsz_cli::args::Module::Node20 => "node20",
            tsz_cli::args::Module::NodeNext => "nodenext",
            tsz_cli::args::Module::Preserve => "preserve",
        };
        map.insert("module".into(), Value::String(s.into()));
    }
    if let Some(mr) = args.module_resolution {
        let s = match mr {
            tsz_cli::args::ModuleResolution::Classic => "classic",
            tsz_cli::args::ModuleResolution::Node10 => "node10",
            tsz_cli::args::ModuleResolution::Node16 => "node16",
            tsz_cli::args::ModuleResolution::NodeNext => "nodenext",
            tsz_cli::args::ModuleResolution::Bundler => "bundler",
        };
        map.insert("moduleResolution".into(), Value::String(s.into()));
    }
    if let Some(jsx) = args.jsx {
        let s = match jsx {
            tsz_cli::args::JsxEmit::Preserve => "preserve",
            tsz_cli::args::JsxEmit::React => "react",
            tsz_cli::args::JsxEmit::ReactJsx => "react-jsx",
            tsz_cli::args::JsxEmit::ReactJsxDev => "react-jsxdev",
            tsz_cli::args::JsxEmit::ReactNative => "react-native",
        };
        map.insert("jsx".into(), Value::String(s.into()));
    }
    if let Some(v) = &args.jsx_factory {
        map.insert("jsxFactory".into(), Value::String(v.clone()));
    }
    if let Some(v) = &args.jsx_fragment_factory {
        map.insert("jsxFragmentFactory".into(), Value::String(v.clone()));
    }
    if let Some(v) = &args.jsx_import_source {
        map.insert("jsxImportSource".into(), Value::String(v.clone()));
    }
    if let Some(v) = &args.out_dir {
        map.insert("outDir".into(), Value::String(v.display().to_string()));
    }
    if let Some(v) = &args.out_file {
        map.insert("outFile".into(), Value::String(v.display().to_string()));
    }
    if let Some(v) = &args.root_dir {
        map.insert("rootDir".into(), Value::String(v.display().to_string()));
    }
    if let Some(v) = &args.declaration_dir {
        map.insert(
            "declarationDir".into(),
            Value::String(v.display().to_string()),
        );
    }
    if let Some(v) = &args.base_url {
        map.insert("baseUrl".into(), Value::String(v.display().to_string()));
    }
    if let Some(v) = &args.ignore_deprecations {
        map.insert("ignoreDeprecations".into(), Value::String(v.clone()));
    }
    if args.ignore_config {
        map.insert("ignoreConfig".into(), Value::Bool(true));
    }

    // `--flag false` overrides round-trip via the hidden disabled-flags channel.
    let disabled_bool_flags: rustc_hash::FxHashSet<&str> = args
        .explicitly_disabled_bool_flags
        .iter()
        .map(String::as_str)
        .collect();

    macro_rules! set_if_true {
        ($f:ident, $k:expr) => {
            if args.$f {
                map.insert($k.into(), Value::Bool(true));
            } else if disabled_bool_flags.contains($k) {
                map.insert($k.into(), Value::Bool(false));
            }
        };
    }
    set_if_true!(strict, "strict");
    set_if_true!(no_emit, "noEmit");
    set_if_true!(no_check, "noCheck");
    set_if_true!(no_emit_on_error, "noEmitOnError");
    set_if_true!(declaration, "declaration");
    set_if_true!(source_map, "sourceMap");
    set_if_true!(declaration_map, "declarationMap");
    set_if_true!(composite, "composite");
    set_if_true!(incremental, "incremental");
    set_if_true!(isolated_modules, "isolatedModules");
    set_if_true!(verbatim_module_syntax, "verbatimModuleSyntax");
    set_if_true!(es_module_interop, "esModuleInterop");
    set_if_true!(allow_js, "allowJs");
    set_if_true!(check_js, "checkJs");
    set_if_true!(skip_lib_check, "skipLibCheck");
    set_if_true!(skip_default_lib_check, "skipDefaultLibCheck");
    set_if_true!(strip_internal, "stripInternal");
    set_if_true!(no_lib, "noLib");
    set_if_true!(import_helpers, "importHelpers");
    set_if_true!(no_emit_helpers, "noEmitHelpers");
    set_if_true!(no_unused_locals, "noUnusedLocals");
    set_if_true!(no_unused_parameters, "noUnusedParameters");
    set_if_true!(no_implicit_returns, "noImplicitReturns");
    set_if_true!(no_fallthrough_cases_in_switch, "noFallthroughCasesInSwitch");
    set_if_true!(exact_optional_property_types, "exactOptionalPropertyTypes");
    set_if_true!(no_unchecked_indexed_access, "noUncheckedIndexedAccess");
    set_if_true!(no_implicit_override, "noImplicitOverride");
    set_if_true!(
        no_property_access_from_index_signature,
        "noPropertyAccessFromIndexSignature"
    );
    set_if_true!(no_resolve, "noResolve");
    set_if_true!(
        no_unchecked_side_effect_imports,
        "noUncheckedSideEffectImports"
    );
    set_if_true!(allow_umd_global_access, "allowUmdGlobalAccess");
    set_if_true!(downlevel_iteration, "downlevelIteration");
    set_if_true!(experimental_decorators, "experimentalDecorators");
    set_if_true!(emit_decorator_metadata, "emitDecoratorMetadata");
    set_if_true!(preserve_const_enums, "preserveConstEnums");
    set_if_true!(remove_comments, "removeComments");
    set_if_true!(emit_bom, "emitBOM");
    set_if_true!(inline_source_map, "inlineSourceMap");
    set_if_true!(inline_sources, "inlineSources");
    set_if_true!(resolve_json_module, "resolveJsonModule");
    set_if_true!(allow_arbitrary_extensions, "allowArbitraryExtensions");
    set_if_true!(allow_importing_ts_extensions, "allowImportingTsExtensions");
    set_if_true!(
        rewrite_relative_import_extensions,
        "rewriteRelativeImportExtensions"
    );
    set_if_true!(preserve_symlinks, "preserveSymlinks");
    set_if_true!(isolated_declarations, "isolatedDeclarations");
    set_if_true!(erasable_syntax_only, "erasableSyntaxOnly");

    macro_rules! set_opt_bool {
        ($f:ident, $k:expr) => {
            if let Some(v) = args.$f {
                map.insert($k.into(), Value::Bool(v));
            }
        };
    }
    set_opt_bool!(no_implicit_any, "noImplicitAny");
    set_opt_bool!(strict_null_checks, "strictNullChecks");
    set_opt_bool!(strict_function_types, "strictFunctionTypes");
    set_opt_bool!(strict_bind_call_apply, "strictBindCallApply");
    set_opt_bool!(
        strict_property_initialization,
        "strictPropertyInitialization"
    );
    set_opt_bool!(
        strict_builtin_iterator_return,
        "strictBuiltinIteratorReturn"
    );
    set_opt_bool!(no_implicit_this, "noImplicitThis");
    set_opt_bool!(use_unknown_in_catch_variables, "useUnknownInCatchVariables");
    set_opt_bool!(always_strict, "alwaysStrict");
    set_opt_bool!(use_define_for_class_fields, "useDefineForClassFields");
    set_opt_bool!(allow_unreachable_code, "allowUnreachableCode");
    set_opt_bool!(allow_unused_labels, "allowUnusedLabels");
    set_opt_bool!(
        allow_synthetic_default_imports,
        "allowSyntheticDefaultImports"
    );
    set_opt_bool!(
        force_consistent_casing_in_file_names,
        "forceConsistentCasingInFileNames"
    );
    set_opt_bool!(pretty, "pretty");
    set_opt_bool!(resolve_package_json_exports, "resolvePackageJsonExports");
    set_opt_bool!(resolve_package_json_imports, "resolvePackageJsonImports");

    if let Some(v) = &args.lib {
        map.insert(
            "lib".into(),
            Value::Array(v.iter().map(|s| Value::String(s.clone())).collect()),
        );
    }
    if let Some(v) = &args.types {
        map.insert(
            "types".into(),
            Value::Array(v.iter().map(|s| Value::String(s.clone())).collect()),
        );
    }
    if let Some(v) = &args.type_roots {
        map.insert(
            "typeRoots".into(),
            Value::Array(
                v.iter()
                    .map(|s| Value::String(s.display().to_string()))
                    .collect(),
            ),
        );
    }
    if let Some(v) = &args.root_dirs {
        map.insert(
            "rootDirs".into(),
            Value::Array(
                v.iter()
                    .map(|s| Value::String(s.display().to_string()))
                    .collect(),
            ),
        );
    }
    if let Some(v) = &args.module_suffixes {
        map.insert(
            "moduleSuffixes".into(),
            Value::Array(v.iter().map(|s| Value::String(s.clone())).collect()),
        );
    }
    if let Some(v) = &args.custom_conditions {
        map.insert(
            "customConditions".into(),
            Value::Array(v.iter().map(|s| Value::String(s.clone())).collect()),
        );
    }
    if let Some(md) = args.module_detection {
        let s = match md {
            tsz_cli::args::ModuleDetection::Auto => "auto",
            tsz_cli::args::ModuleDetection::Force => "force",
            tsz_cli::args::ModuleDetection::Legacy => "legacy",
        };
        map.insert("moduleDetection".into(), Value::String(s.into()));
    }
    if let Some(nl) = args.new_line {
        let s = match nl {
            tsz_cli::args::NewLine::Crlf => "crlf",
            tsz_cli::args::NewLine::Lf => "lf",
        };
        map.insert("newLine".into(), Value::String(s.into()));
    }
    if let Some(v) = &args.map_root {
        map.insert("mapRoot".into(), Value::String(v.clone()));
    }
    if let Some(v) = &args.source_root {
        map.insert("sourceRoot".into(), Value::String(v.clone()));
    }
    if let Some(v) = args.max_node_module_js_depth {
        map.insert(
            "maxNodeModuleJsDepth".into(),
            Value::Number(serde_json::Number::from(v)),
        );
    }
}

/// Add options that tsc v6 shows in `--showConfig` output but are computed
/// from other explicitly set options.
///
/// Algorithm from tsc v6 `convertToTSConfig` (`commandLineParser.ts`):
/// - For each computed option: if NOT in providedKeys AND transitively depends
///   on any provided key, show it if computed value != default value.
fn add_implied_options(map: &mut serde_json::Map<String, serde_json::Value>) {
    use serde_json::Value;

    fn parse_target(s: &str) -> tsz::common::ScriptTarget {
        tsz::common::ScriptTarget::from_ts_str(s).unwrap_or(tsz::common::ScriptTarget::ES2025)
    }

    const fn compute_module(target: tsz::common::ScriptTarget) -> &'static str {
        match tsz_cli::config::default_module_kind_for_target(target, true) {
            tsz::common::ModuleKind::ES2015 => "es6",
            module => module.as_ts_str(),
        }
    }

    fn compute_module_resolution(module_str: &str) -> &'static str {
        tsz::common::ModuleKind::from_ts_str(module_str)
            .map(tsz_cli::config::default_module_resolution_for_module)
            .unwrap_or(tsz_cli::config::ModuleResolutionKind::Bundler)
            .as_ts_str()
    }

    fn compute_module_detection(module_str: &str) -> &'static str {
        tsz::common::ModuleKind::from_ts_str(module_str)
            .map(tsz_cli::config::default_module_detection_for_module)
            .unwrap_or("auto")
    }

    const DEFAULT_TARGET: tsz::common::ScriptTarget = tsz::common::ScriptTarget::ES2025;
    const DEFAULT_MODULE_RESOLUTION: &str = "bundler";
    const DEFAULT_MODULE_DETECTION: &str = "auto";

    // Snapshot the initially-provided keys before any implied inserts.
    // `depends_on_provided` must check the original set, not the growing map.
    let p_target = map.contains_key("target");
    let p_module = map.contains_key("module");
    let p_mr = map.contains_key("moduleResolution");
    let p_composite = map.contains_key("composite");
    let p_verbatim = map.contains_key("verbatimModuleSyntax");
    let p_check_js = map.contains_key("checkJs");
    let p_isolated = map.contains_key("isolatedModules");
    let p_rewrite = map.contains_key("rewriteRelativeImportExtensions");
    let p_resolve_pkg_exports = map.contains_key("resolvePackageJsonExports");

    let user_target_str = map
        .get("target")
        .and_then(|v| v.as_str())
        .unwrap_or("es2025");
    let user_target = parse_target(user_target_str);
    let user_composite = map
        .get("composite")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let user_verbatim = map
        .get("verbatimModuleSyntax")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let user_check_js = map
        .get("checkJs")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let user_isolated_modules = map
        .get("isolatedModules")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let user_rewrite_relative = map
        .get("rewriteRelativeImportExtensions")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    // module: deps=["target"]
    if !map.contains_key("module") && p_target {
        let computed = compute_module(user_target);
        if computed != compute_module(DEFAULT_TARGET) {
            map.insert("module".into(), Value::String(computed.into()));
        }
    }

    let eff_module = map
        .get("module")
        .and_then(|v| v.as_str())
        .unwrap_or_else(|| compute_module(user_target))
        .to_owned();

    // moduleResolution: deps=["module","target"]
    if !map.contains_key("moduleResolution") && (p_module || p_target) {
        let computed = compute_module_resolution(&eff_module);
        if computed != DEFAULT_MODULE_RESOLUTION {
            map.insert("moduleResolution".into(), Value::String(computed.into()));
        }
    }

    // moduleDetection: deps=["module","target"]
    if !map.contains_key("moduleDetection") && (p_module || p_target) {
        let computed = compute_module_detection(&eff_module);
        if computed != DEFAULT_MODULE_DETECTION {
            map.insert("moduleDetection".into(), Value::String(computed.into()));
        }
    }

    // useDefineForClassFields: deps=["target","module"]
    if !map.contains_key("useDefineForClassFields") && (p_target || p_module) {
        let computed = user_target.supports_es2022();
        if computed != DEFAULT_TARGET.supports_es2022() {
            map.insert("useDefineForClassFields".into(), Value::Bool(computed));
        }
    }

    // declaration: deps=["composite"]
    if !map.contains_key("declaration") && p_composite {
        let user_decl = map
            .get("declaration")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        if user_decl || user_composite {
            map.insert("declaration".into(), Value::Bool(true));
        }
    }

    // incremental: deps=["composite"]
    if !map.contains_key("incremental") && p_composite {
        let user_incr = map
            .get("incremental")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        if user_incr || user_composite {
            map.insert("incremental".into(), Value::Bool(true));
        }
    }

    // isolatedModules: deps=["verbatimModuleSyntax"]
    if !map.contains_key("isolatedModules")
        && p_verbatim
        && (user_isolated_modules || user_verbatim)
    {
        map.insert("isolatedModules".into(), Value::Bool(true));
    }

    // allowJs: deps=["checkJs"]
    if !map.contains_key("allowJs") && p_check_js && user_check_js {
        map.insert("allowJs".into(), Value::Bool(true));
    }

    // preserveConstEnums: deps=["isolatedModules","verbatimModuleSyntax"]
    if !map.contains_key("preserveConstEnums") && (p_isolated || p_verbatim) {
        let user_preserve = map
            .get("preserveConstEnums")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        if user_preserve || user_isolated_modules || user_verbatim {
            map.insert("preserveConstEnums".into(), Value::Bool(true));
        }
    }

    // declarationMap: deps=["declaration","composite"]
    if !map.contains_key("declarationMap") && (p_composite || map.contains_key("declaration")) {
        let user_decl_map = map
            .get("declarationMap")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        let eff_declaration = map
            .get("declaration")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        if user_decl_map && eff_declaration {
            map.insert("declarationMap".into(), Value::Bool(true));
        }
    }

    // allowImportingTsExtensions: deps=["rewriteRelativeImportExtensions"]
    if !map.contains_key("allowImportingTsExtensions") && p_rewrite {
        let user_allow = map
            .get("allowImportingTsExtensions")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        if user_allow || user_rewrite_relative {
            map.insert("allowImportingTsExtensions".into(), Value::Bool(true));
        }
    }

    // Effective module resolution — computed after moduleResolution may have been inserted.
    let eff_mr = map
        .get("moduleResolution")
        .and_then(|v| v.as_str())
        .unwrap_or(DEFAULT_MODULE_RESOLUTION)
        .to_lowercase();
    let mr_implies_pkg = matches!(eff_mr.as_str(), "node16" | "nodenext" | "bundler");

    // resolveJsonModule: deps=["moduleResolution","module","target"]
    if !map.contains_key("resolveJsonModule") && (p_mr || p_module || p_target) {
        let user_val = map.get("resolveJsonModule").and_then(|v| v.as_bool());
        let computed = user_val.unwrap_or(matches!(eff_mr.as_str(), "nodenext" | "bundler"));
        let default_val = matches!(DEFAULT_MODULE_RESOLUTION, "node16" | "nodenext" | "bundler");
        if computed != default_val {
            map.insert("resolveJsonModule".into(), Value::Bool(computed));
        }
    }

    // resolvePackageJsonExports: deps=["moduleResolution","module","target"]
    if !map.contains_key("resolvePackageJsonExports") && (p_mr || p_module || p_target) {
        let user_val = map
            .get("resolvePackageJsonExports")
            .and_then(|v| v.as_bool());
        let computed = user_val.unwrap_or(mr_implies_pkg);
        if !computed {
            map.insert("resolvePackageJsonExports".into(), Value::Bool(false));
        }
    }

    // resolvePackageJsonImports: deps=["moduleResolution","resolvePackageJsonExports","module","target"]
    if !map.contains_key("resolvePackageJsonImports")
        && (p_mr || p_resolve_pkg_exports || p_module || p_target)
    {
        let user_val = map
            .get("resolvePackageJsonImports")
            .and_then(|v| v.as_bool());
        let computed = user_val.unwrap_or(mr_implies_pkg);
        if !computed {
            map.insert("resolvePackageJsonImports".into(), Value::Bool(false));
        }
    }
}

/// Relativize all path options that `anchor_inherited_path_options` may have
/// absolutized during extends resolution.  We cover the same set of fields so
/// that every absolute path that was anchored also gets a relative counterpart
/// in the `--showConfig` output.
fn relativize_resolved_path_options(
    options: &mut serde_json::Map<String, serde_json::Value>,
    base_dir: &Path,
) {
    let canonical_base = base_dir
        .canonicalize()
        .unwrap_or_else(|_| base_dir.to_path_buf());

    let relativize = |s: &mut String| {
        let path_obj = Path::new(s.as_str());
        if !path_obj.is_absolute() {
            return;
        }
        let canonical_path = path_obj
            .canonicalize()
            .unwrap_or_else(|_| path_obj.to_path_buf());
        let relative = tsz_cli::fs::diff_paths(&canonical_path, &canonical_base)
            .unwrap_or_else(|| canonical_path.to_path_buf());
        *s = path_to_show_config_string(&relative);
    };

    for key in &[
        "baseUrl",
        "rootDir",
        "outDir",
        "outFile",
        "declarationDir",
        "tsBuildInfoFile",
    ] {
        if let Some(serde_json::Value::String(s)) = options.get_mut(*key) {
            relativize(s);
        }
    }

    for array_key in &["rootDirs", "typeRoots"] {
        if let Some(serde_json::Value::Array(arr)) = options.get_mut(*array_key) {
            for item in arr {
                if let serde_json::Value::String(s) = item {
                    relativize(s);
                }
            }
        }
    }
}

/// Normalise a path for `--showConfig` `files` array: relativize, forward-slash, `./` prefix.
pub(super) fn normalize_relative(base_dir: &Path, path: &Path) -> String {
    let rel = if path.is_absolute() {
        path.strip_prefix(base_dir)
            .map(|p| p.to_path_buf())
            .unwrap_or_else(|_| path.to_path_buf())
    } else {
        path.to_path_buf()
    };
    let s = rel.display().to_string().replace('\\', "/");
    if s.starts_with("./") || s.starts_with("../") || s.starts_with('/') {
        s
    } else {
        format!("./{s}")
    }
}

fn display_selector(base_dir: &Path, selector: &str) -> String {
    let path = Path::new(selector);
    if path.is_absolute() {
        display_path(base_dir, path)
    } else {
        selector.to_string()
    }
}

fn display_path(base_dir: &Path, path: &Path) -> String {
    let relative = if path.is_absolute() {
        tsz_cli::fs::diff_paths(path, base_dir).unwrap_or_else(|| path.to_path_buf())
    } else {
        path.to_path_buf()
    };
    path_to_show_config_string(&relative)
}

fn path_to_show_config_string(path: &Path) -> String {
    let display = path.to_string_lossy().replace('\\', "/");
    if display.is_empty() || display == "." {
        "./".to_string()
    } else if display.starts_with("../") || display.starts_with('/') {
        display
    } else {
        format!("./{display}")
    }
}

/// Add a `./` prefix to a path string that has neither a `./`, `../`, nor `/` prefix.
/// Treats `"."` and empty strings as `"./"`.
fn ensure_dot_slash_prefix(s: &str) -> String {
    if s.is_empty() || s == "." {
        "./".to_owned()
    } else if s.starts_with("./") || s.starts_with("../") || s.starts_with('/') {
        s.to_owned()
    } else {
        format!("./{s}")
    }
}

/// Format a key-value pair for `--showConfig` output.
pub(super) fn format_key_value(key: &str, value: &serde_json::Value, indent: usize) -> String {
    format!("\"{key}\": {}", format_value(value, indent))
}

/// Format a `serde_json::Value` with 4-space indentation matching `tsc --showConfig`.
pub(super) fn format_value(value: &serde_json::Value, indent: usize) -> String {
    match value {
        serde_json::Value::String(s) => format!("\"{s}\""),
        serde_json::Value::Bool(b) => b.to_string(),
        serde_json::Value::Number(n) => n.to_string(),
        serde_json::Value::Array(arr) => {
            if arr.is_empty() {
                "[]".to_string()
            } else {
                let item_indent = indent + 4;
                let item_pad = " ".repeat(item_indent);
                let close_pad = " ".repeat(indent);
                let mut result = String::from("[\n");
                for (i, item) in arr.iter().enumerate() {
                    result.push_str(&item_pad);
                    result.push_str(&format_value(item, item_indent));
                    if i + 1 < arr.len() {
                        result.push(',');
                    }
                    result.push('\n');
                }
                result.push_str(&close_pad);
                result.push(']');
                result
            }
        }
        serde_json::Value::Object(map) => {
            if map.is_empty() {
                "{}".to_string()
            } else {
                let item_indent = indent + 4;
                let item_pad = " ".repeat(item_indent);
                let close_pad = " ".repeat(indent);
                let mut result = String::from("{\n");
                let map_len = map.len();
                for (i, (k, v)) in map.iter().enumerate() {
                    result.push_str(&item_pad);
                    result.push_str(&format!("\"{}\": {}", k, format_value(v, item_indent)));
                    if i + 1 < map_len {
                        result.push(',');
                    }
                    result.push('\n');
                }
                result.push_str(&close_pad);
                result.push('}');
                result
            }
        }
        serde_json::Value::Null => "null".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser as _;
    use tsz_cli::args::CliArgs;
    use tsz_cli::config::{CompilerOptions as CoreCompilerOptions, TsConfig};

    fn empty_args() -> CliArgs {
        CliArgs::try_parse_from(["tsz"]).expect("minimal args should parse")
    }

    fn base_dir() -> PathBuf {
        PathBuf::from("/project")
    }

    // --- build_compiler_options_map ---

    #[test]
    fn compiler_options_to_json_preserves_strict() {
        let cfg = TsConfig {
            compiler_options: Some(CoreCompilerOptions {
                strict: Some(true),
                ..Default::default()
            }),
            ..Default::default()
        };
        let map = build_compiler_options_map(Some(&cfg), &empty_args(), &base_dir());
        assert_eq!(map.get("strict"), Some(&serde_json::Value::Bool(true)));
    }

    #[test]
    fn compiler_options_to_json_normalises_es2015_target_to_es6() {
        let cfg = TsConfig {
            compiler_options: Some(CoreCompilerOptions {
                target: Some("es2015".to_string()),
                ..Default::default()
            }),
            ..Default::default()
        };
        let map = build_compiler_options_map(Some(&cfg), &empty_args(), &base_dir());
        assert_eq!(
            map.get("target"),
            Some(&serde_json::Value::String("es6".to_string()))
        );
    }

    #[test]
    fn cli_override_target_wins_over_tsconfig() {
        let cfg = TsConfig {
            compiler_options: Some(CoreCompilerOptions {
                target: Some("es5".to_string()),
                ..Default::default()
            }),
            ..Default::default()
        };
        let mut args = empty_args();
        args.target = Some(tsz_cli::args::Target::Es2020);
        let map = build_compiler_options_map(Some(&cfg), &args, &base_dir());
        assert_eq!(
            map.get("target"),
            Some(&serde_json::Value::String("es2020".to_string()))
        );
    }

    #[test]
    fn cli_override_strict_true_sets_bool() {
        let mut args = empty_args();
        args.strict = true;
        let map = build_compiler_options_map(None, &args, &base_dir());
        assert_eq!(map.get("strict"), Some(&serde_json::Value::Bool(true)));
    }

    #[test]
    fn cli_override_strict_false_sets_bool_false() {
        // Parse `--strict false` via preprocess_args which injects the hidden flag
        let args = CliArgs::try_parse_from(["tsz", "--__explicitly-disabled-bool-flag", "strict"])
            .expect("hidden flag should parse");
        let map = build_compiler_options_map(None, &args, &base_dir());
        assert_eq!(map.get("strict"), Some(&serde_json::Value::Bool(false)));
    }

    #[test]
    fn implied_module_added_when_target_set() {
        // target=es5 → module=commonjs (not the default es2022)
        let mut args = empty_args();
        args.target = Some(tsz_cli::args::Target::Es5);
        let map = build_compiler_options_map(None, &args, &base_dir());
        // es5 → CommonJs → not the default es2022, so module should be implied
        assert!(
            map.contains_key("module"),
            "module should be implied for non-default target"
        );
    }

    #[test]
    fn composite_implies_declaration_and_incremental() {
        let cfg = TsConfig {
            compiler_options: Some(CoreCompilerOptions {
                composite: Some(true),
                ..Default::default()
            }),
            ..Default::default()
        };
        let map = build_compiler_options_map(Some(&cfg), &empty_args(), &base_dir());
        assert_eq!(
            map.get("declaration"),
            Some(&serde_json::Value::Bool(true)),
            "composite implies declaration"
        );
        assert_eq!(
            map.get("incremental"),
            Some(&serde_json::Value::Bool(true)),
            "composite implies incremental"
        );
    }

    #[test]
    fn verbatim_module_syntax_implies_isolated_modules() {
        let cfg = TsConfig {
            compiler_options: Some(CoreCompilerOptions {
                verbatim_module_syntax: Some(true),
                ..Default::default()
            }),
            ..Default::default()
        };
        let map = build_compiler_options_map(Some(&cfg), &empty_args(), &base_dir());
        assert_eq!(
            map.get("isolatedModules"),
            Some(&serde_json::Value::Bool(true)),
            "verbatimModuleSyntax implies isolatedModules"
        );
    }

    // --- render_output ---

    #[test]
    fn render_output_empty_config_produces_valid_json() {
        let map = serde_json::Map::new();
        let output = render_output(&map, &[], &[], None, &base_dir());
        assert!(output.starts_with('{'));
        assert!(output.trim_end().ends_with('}'));
        let parsed: serde_json::Value = serde_json::from_str(&output).expect("valid JSON");
        assert!(parsed.get("compilerOptions").is_some());
    }

    #[test]
    fn render_output_includes_files_array() {
        let map = serde_json::Map::new();
        let files = vec!["./a.ts".to_string(), "./b.ts".to_string()];
        let output = render_output(&map, &files, &[], None, &base_dir());
        let parsed: serde_json::Value = serde_json::from_str(&output).expect("valid JSON");
        let files_arr = parsed["files"].as_array().expect("files array");
        assert_eq!(files_arr.len(), 2);
        assert_eq!(files_arr[0], "./a.ts");
        assert_eq!(files_arr[1], "./b.ts");
    }

    #[test]
    fn render_output_includes_exclude_array() {
        let map = serde_json::Map::new();
        let excludes = vec!["./dist".to_string()];
        let cfg = TsConfig {
            exclude: Some(vec!["dist".to_string()]),
            ..Default::default()
        };
        let output = render_output(&map, &[], &excludes, Some(&cfg), &base_dir());
        let parsed: serde_json::Value = serde_json::from_str(&output).expect("valid JSON");
        let exc_arr = parsed["exclude"].as_array().expect("exclude array");
        assert_eq!(exc_arr[0], "./dist");
    }

    #[test]
    fn render_output_references_appear_before_files() {
        use tsz_cli::config::TsConfigReference;
        let map = serde_json::Map::new();
        let cfg = TsConfig {
            references: Some(vec![TsConfigReference {
                path: "../lib".to_string(),
                prepend: false,
            }]),
            ..Default::default()
        };
        let files = vec!["./src/index.ts".to_string()];
        let output = render_output(&map, &files, &[], Some(&cfg), &base_dir());
        let refs_pos = output.find("\"references\"").expect("references key");
        let files_pos = output.find("\"files\"").expect("files key");
        assert!(
            refs_pos < files_pos,
            "references must appear before files in output"
        );
    }

    // --- path utilities ---

    #[test]
    fn normalize_relative_prepends_dot_slash() {
        let base = PathBuf::from("/project");
        let path = PathBuf::from("src/index.ts");
        assert_eq!(normalize_relative(&base, &path), "./src/index.ts");
    }

    #[test]
    fn normalize_relative_strips_base_dir_from_absolute_path() {
        let base = PathBuf::from("/project");
        let path = PathBuf::from("/project/src/index.ts");
        assert_eq!(normalize_relative(&base, &path), "./src/index.ts");
    }

    #[test]
    fn normalize_relative_preserves_dot_slash_prefix() {
        let base = PathBuf::from("/project");
        let path = PathBuf::from("./src/index.ts");
        assert_eq!(normalize_relative(&base, &path), "./src/index.ts");
    }

    // --- format_value ---

    #[test]
    fn format_value_string() {
        assert_eq!(
            format_value(&serde_json::Value::String("es6".into()), 0),
            "\"es6\""
        );
    }

    #[test]
    fn format_value_bool_true() {
        assert_eq!(format_value(&serde_json::Value::Bool(true), 0), "true");
    }

    #[test]
    fn format_value_empty_array() {
        assert_eq!(format_value(&serde_json::Value::Array(vec![]), 0), "[]");
    }
}

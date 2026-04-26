use clap::Parser;
use tsz::emitter::{ModuleKind, ScriptTarget};

use super::args::{
    CliArgs, ImportsNotUsedAsValues, JsxEmit, Module, ModuleDetection, ModuleResolution, NewLine,
    PollingWatchKind, Target, WatchDirectoryKind, WatchFileKind,
};

#[test]
fn parses_defaults() {
    let args = CliArgs::try_parse_from(["tsz"]).expect("default args should parse");

    assert_eq!(args.target, None);
    assert_eq!(args.module, None);
    assert!(args.out_dir.is_none());
    assert!(args.project.is_none());
    assert!(!args.strict);
    assert!(!args.no_emit);
    assert!(!args.watch);
    assert!(args.files.is_empty());
    assert!(!args.init);
    assert!(!args.build);
    assert!(!args.all);
    assert!(!args.show_config);
    assert!(!args.list_files_only);
}

#[test]
fn parses_common_flags() {
    let args = CliArgs::try_parse_from([
        "tsz",
        "--target",
        "es2020",
        "--module",
        "commonjs",
        "--outDir",
        "dist",
        "--project",
        "configs/tsconfig.json",
        "--strict",
        "--noEmit",
        "--watch",
        "src/index.ts",
    ])
    .expect("flagged args should parse");

    assert_eq!(args.target, Some(Target::Es2020));
    assert_eq!(args.module, Some(Module::CommonJs));
    assert_eq!(args.out_dir.as_deref(), Some(std::path::Path::new("dist")));
    assert_eq!(
        args.project.as_deref(),
        Some(std::path::Path::new("configs/tsconfig.json"))
    );
    assert!(args.strict);
    assert!(args.no_emit);
    assert!(args.watch);
    assert_eq!(args.files, vec![std::path::PathBuf::from("src/index.ts")]);
}

#[test]
fn parses_cli_only_flags() {
    let args = CliArgs::try_parse_from(["tsz", "--init"]).expect("--init should parse");
    assert!(args.init);

    let args = CliArgs::try_parse_from(["tsz", "--showConfig"]).expect("--showConfig should parse");
    assert!(args.show_config);

    let args = CliArgs::try_parse_from(["tsz", "--show-config"])
        .expect("--show-config alias should parse");
    assert!(args.show_config);

    let args =
        CliArgs::try_parse_from(["tsz", "--listFilesOnly"]).expect("--listFilesOnly should parse");
    assert!(args.list_files_only);

    let args = CliArgs::try_parse_from(["tsz", "--all"]).expect("--all should parse");
    assert!(args.all);

    let args = CliArgs::try_parse_from(["tsz", "-b"]).expect("-b should parse");
    assert!(args.build);

    let args = CliArgs::try_parse_from(["tsz", "--build"]).expect("--build should parse");
    assert!(args.build);

    let args = CliArgs::try_parse_from(["tsz", "--locale", "en"]).expect("--locale should parse");
    assert_eq!(args.locale, Some("en".to_string()));
}

#[test]
fn parses_target_variants() {
    let targets = [
        ("es5", Target::Es5),
        ("es2015", Target::Es2015),
        ("es6", Target::Es2015), // alias
        ("es2016", Target::Es2016),
        ("es2017", Target::Es2017),
        ("es2018", Target::Es2018),
        ("es2019", Target::Es2019),
        ("es2020", Target::Es2020),
        ("es2021", Target::Es2021),
        ("es2022", Target::Es2022),
        ("es2023", Target::Es2023),
        ("es2024", Target::Es2024),
        ("es2025", Target::Es2025),
        ("esnext", Target::EsNext),
        ("es-next", Target::EsNext), // alias
    ];

    for (input, expected) in targets {
        let args = CliArgs::try_parse_from(["tsz", "--target", input])
            .unwrap_or_else(|_| panic!("--target {input} should parse"));
        assert_eq!(args.target, Some(expected), "target {input} failed");
    }
}

#[test]
fn parses_module_variants() {
    let modules = [
        ("none", Module::None),
        ("commonjs", Module::CommonJs),
        ("common-js", Module::CommonJs), // alias
        ("amd", Module::Amd),
        ("umd", Module::Umd),
        ("system", Module::System),
        ("es2015", Module::Es2015),
        ("es6", Module::Es2015), // alias
        ("es2020", Module::Es2020),
        ("es2022", Module::Es2022),
        ("esnext", Module::EsNext),
        ("es-next", Module::EsNext), // alias
        ("node16", Module::Node16),
        ("node-16", Module::Node16), // alias
        ("node18", Module::Node18),
        ("node20", Module::Node20),
        ("nodenext", Module::NodeNext),
        ("node-next", Module::NodeNext), // alias
        ("preserve", Module::Preserve),
    ];

    for (input, expected) in modules {
        let args = CliArgs::try_parse_from(["tsz", "--module", input])
            .unwrap_or_else(|_| panic!("--module {input} should parse"));
        assert_eq!(args.module, Some(expected), "module {input} failed");
    }
}

#[test]
fn parses_jsx_variants() {
    let jsx_modes = [
        ("preserve", JsxEmit::Preserve),
        ("react", JsxEmit::React),
        ("react-jsx", JsxEmit::ReactJsx),
        ("react-jsxdev", JsxEmit::ReactJsxDev),
        ("react-native", JsxEmit::ReactNative),
    ];

    for (input, expected) in jsx_modes {
        let args = CliArgs::try_parse_from(["tsz", "--jsx", input])
            .unwrap_or_else(|_| panic!("--jsx {input} should parse"));
        assert_eq!(args.jsx, Some(expected), "jsx {input} failed");
    }
}

#[test]
fn parses_module_resolution_variants() {
    let resolutions = [
        ("classic", ModuleResolution::Classic),
        ("node10", ModuleResolution::Node10),
        ("node", ModuleResolution::Node10), // alias
        ("node16", ModuleResolution::Node16),
        ("nodenext", ModuleResolution::NodeNext),
        ("node-next", ModuleResolution::NodeNext), // alias
        ("bundler", ModuleResolution::Bundler),
    ];

    for (input, expected) in resolutions {
        let args = CliArgs::try_parse_from(["tsz", "--moduleResolution", input])
            .unwrap_or_else(|_| panic!("--moduleResolution {input} should parse"));
        assert_eq!(
            args.module_resolution,
            Some(expected),
            "moduleResolution {input} failed"
        );
    }
}

#[test]
fn parses_module_detection_variants() {
    let detections = [
        ("auto", ModuleDetection::Auto),
        ("force", ModuleDetection::Force),
        ("legacy", ModuleDetection::Legacy),
    ];

    for (input, expected) in detections {
        let args = CliArgs::try_parse_from(["tsz", "--moduleDetection", input])
            .unwrap_or_else(|_| panic!("--moduleDetection {input} should parse"));
        assert_eq!(
            args.module_detection,
            Some(expected),
            "moduleDetection {input} failed"
        );
    }
}

#[test]
fn parses_newline_variants() {
    let args = CliArgs::try_parse_from(["tsz", "--newLine", "crlf"]).expect("crlf should parse");
    assert_eq!(args.new_line, Some(NewLine::Crlf));

    let args = CliArgs::try_parse_from(["tsz", "--newLine", "lf"]).expect("lf should parse");
    assert_eq!(args.new_line, Some(NewLine::Lf));
}

#[test]
fn parses_type_checking_flags() {
    let args = CliArgs::try_parse_from([
        "tsz",
        "--strict",
        "--noImplicitAny",
        "true",
        "--strictNullChecks",
        "true",
        "--strictFunctionTypes",
        "true",
        "--strictBindCallApply",
        "true",
        "--strictPropertyInitialization",
        "true",
        "--noImplicitThis",
        "true",
        "--useUnknownInCatchVariables",
        "true",
        "--alwaysStrict",
        "true",
        "--noUnusedLocals",
        "--noUnusedParameters",
        "--exactOptionalPropertyTypes",
        "--noImplicitReturns",
        "--noFallthroughCasesInSwitch",
        "--noUncheckedIndexedAccess",
        "--noImplicitOverride",
        "--noPropertyAccessFromIndexSignature",
    ])
    .expect("type checking flags should parse");

    assert!(args.strict);
    assert_eq!(args.no_implicit_any, Some(true));
    assert_eq!(args.strict_null_checks, Some(true));
    assert_eq!(args.strict_function_types, Some(true));
    assert_eq!(args.strict_bind_call_apply, Some(true));
    assert_eq!(args.strict_property_initialization, Some(true));
    assert_eq!(args.no_implicit_this, Some(true));
    assert_eq!(args.use_unknown_in_catch_variables, Some(true));
    assert_eq!(args.always_strict, Some(true));
    assert!(args.no_unused_locals);
    assert!(args.no_unused_parameters);
    assert!(args.exact_optional_property_types);
    assert!(args.no_implicit_returns);
    assert!(args.no_fallthrough_cases_in_switch);
    assert!(args.no_unchecked_indexed_access);
    assert!(args.no_implicit_override);
    assert!(args.no_property_access_from_index_signature);
}

#[test]
fn parses_emit_flags() {
    let args = CliArgs::try_parse_from([
        "tsz",
        "-d",
        "--declarationDir",
        "types",
        "--declarationMap",
        "--emitDeclarationOnly",
        "--sourceMap",
        "--inlineSourceMap",
        "--inlineSources",
        "--outDir",
        "dist",
        "--rootDir",
        "src",
        "--outFile",
        "bundle.js",
        "--noEmit",
        "--noEmitOnError",
        "--noEmitHelpers",
        "--importHelpers",
        "--downlevelIteration",
        "--mapRoot",
        "/maps",
        "--sourceRoot",
        "/sources",
        "--newLine",
        "lf",
        "--removeComments",
        "--preserveConstEnums",
        "--stripInternal",
        "--emitBOM",
    ])
    .expect("emit flags should parse");

    assert!(args.declaration);
    assert_eq!(
        args.declaration_dir.as_deref(),
        Some(std::path::Path::new("types"))
    );
    assert!(args.declaration_map);
    assert!(args.emit_declaration_only);
    assert!(args.source_map);
    assert!(args.inline_source_map);
    assert!(args.inline_sources);
    assert_eq!(args.out_dir.as_deref(), Some(std::path::Path::new("dist")));
    assert_eq!(args.root_dir.as_deref(), Some(std::path::Path::new("src")));
    assert_eq!(
        args.out_file.as_deref(),
        Some(std::path::Path::new("bundle.js"))
    );
    assert!(args.no_emit);
    assert!(args.no_emit_on_error);
    assert!(args.no_emit_helpers);
    assert!(args.import_helpers);
    assert!(args.downlevel_iteration);
    assert_eq!(args.map_root, Some("/maps".to_string()));
    assert_eq!(args.source_root, Some("/sources".to_string()));
    assert_eq!(args.new_line, Some(NewLine::Lf));
    assert!(args.remove_comments);
    assert!(args.preserve_const_enums);
    assert!(args.strip_internal);
    assert!(args.emit_bom);
}

#[test]
fn parses_module_flags() {
    let args = CliArgs::try_parse_from([
        "tsz",
        "--moduleResolution",
        "bundler",
        "--baseUrl",
        ".",
        "--typeRoots",
        "types,node_modules/@types",
        "--types",
        "node,jest",
        "--rootDirs",
        "src,generated",
        "--resolveJsonModule",
        "--resolvePackageJsonExports",
        "true",
        "--resolvePackageJsonImports",
        "true",
        "--moduleSuffixes",
        ".ios,.android",
        "--allowArbitraryExtensions",
        "--allowImportingTsExtensions",
        "--rewriteRelativeImportExtensions",
        "--customConditions",
        "development,browser",
        "--noResolve",
        "--allowUmdGlobalAccess",
        "--noUncheckedSideEffectImports",
    ])
    .expect("module flags should parse");

    assert_eq!(args.module_resolution, Some(ModuleResolution::Bundler));
    assert_eq!(args.base_url.as_deref(), Some(std::path::Path::new(".")));
    assert_eq!(args.type_roots.as_ref().map(|v| v.len()), Some(2));
    assert_eq!(
        args.types,
        Some(vec!["node".to_string(), "jest".to_string()])
    );
    assert_eq!(args.root_dirs.as_ref().map(|v| v.len()), Some(2));
    assert!(args.resolve_json_module);
    assert_eq!(args.resolve_package_json_exports, Some(true));
    assert_eq!(args.resolve_package_json_imports, Some(true));
    assert_eq!(
        args.module_suffixes,
        Some(vec![".ios".to_string(), ".android".to_string()])
    );
    assert!(args.allow_arbitrary_extensions);
    assert!(args.allow_importing_ts_extensions);
    assert!(args.rewrite_relative_import_extensions);
    assert_eq!(
        args.custom_conditions,
        Some(vec!["development".to_string(), "browser".to_string()])
    );
    assert!(args.no_resolve);
    assert!(args.allow_umd_global_access);
    assert!(args.no_unchecked_side_effect_imports);
}

#[test]
fn parses_interop_flags() {
    let args = CliArgs::try_parse_from([
        "tsz",
        "--esModuleInterop",
        "--allowSyntheticDefaultImports",
        "true",
        "--isolatedModules",
        "--isolatedDeclarations",
        "--verbatimModuleSyntax",
        "--forceConsistentCasingInFileNames",
        "true",
        "--preserveSymlinks",
        "--erasableSyntaxOnly",
    ])
    .expect("interop flags should parse");

    assert!(args.es_module_interop);
    assert_eq!(args.allow_synthetic_default_imports, Some(true));
    assert!(args.isolated_modules);
    assert!(args.isolated_declarations);
    assert!(args.verbatim_module_syntax);
    assert_eq!(args.force_consistent_casing_in_file_names, Some(true));
    assert!(args.preserve_symlinks);
    assert!(args.erasable_syntax_only);
}

#[test]
fn parses_javascript_support_flags() {
    let args = CliArgs::try_parse_from([
        "tsz",
        "--allowJs",
        "--checkJs",
        "--maxNodeModuleJsDepth",
        "2",
    ])
    .expect("js support flags should parse");

    assert!(args.allow_js);
    assert!(args.check_js);
    assert_eq!(args.max_node_module_js_depth, Some(2));
}

#[test]
fn parses_jsx_flags() {
    let args = CliArgs::try_parse_from([
        "tsz",
        "--jsx",
        "react-jsx",
        "--jsxFactory",
        "h",
        "--jsxFragmentFactory",
        "Fragment",
        "--jsxImportSource",
        "preact",
    ])
    .expect("jsx flags should parse");

    assert_eq!(args.jsx, Some(JsxEmit::ReactJsx));
    assert_eq!(args.jsx_factory, Some("h".to_string()));
    assert_eq!(args.jsx_fragment_factory, Some("Fragment".to_string()));
    assert_eq!(args.jsx_import_source, Some("preact".to_string()));
}

#[test]
fn parses_project_flags() {
    let args = CliArgs::try_parse_from([
        "tsz",
        "--composite",
        "-i",
        "--tsBuildInfoFile",
        ".tsbuildinfo",
        "--disableReferencedProjectLoad",
        "--disableSolutionSearching",
        "--disableSourceOfProjectReferenceRedirect",
    ])
    .expect("project flags should parse");

    assert!(args.composite);
    assert!(args.incremental);
    assert_eq!(
        args.ts_build_info_file.as_deref(),
        Some(std::path::Path::new(".tsbuildinfo"))
    );
    assert!(args.disable_referenced_project_load);
    assert!(args.disable_solution_searching);
    assert!(args.disable_source_of_project_reference_redirect);
}

#[test]
fn parses_completeness_flags() {
    let args =
        CliArgs::try_parse_from(["tsz", "--skipDefaultLibCheck", "--skipLibCheck", "--noLib"])
            .expect("completeness flags should parse");

    assert!(args.skip_default_lib_check);
    assert!(args.skip_lib_check);
    assert!(args.no_lib);
}

#[test]
fn parses_diagnostic_flags() {
    let args = CliArgs::try_parse_from([
        "tsz",
        "--diagnostics",
        "--extendedDiagnostics",
        "--explainFiles",
        "--listFiles",
        "--listEmittedFiles",
        "--traceResolution",
        "--traceDependencies",
        "--generateTrace",
        "/trace",
        "--generateCpuProfile",
        "profile.cpuprofile",
        "--noCheck",
    ])
    .expect("diagnostic flags should parse");

    assert!(args.diagnostics);
    assert!(args.extended_diagnostics);
    assert!(args.explain_files);
    assert!(args.list_files);
    assert!(args.list_emitted_files);
    assert!(args.trace_resolution);
    assert!(args.trace_dependencies);
    assert_eq!(
        args.generate_trace.as_deref(),
        Some(std::path::Path::new("/trace"))
    );
    assert_eq!(
        args.generate_cpu_profile.as_deref(),
        Some(std::path::Path::new("profile.cpuprofile"))
    );
    assert!(args.no_check);
}

#[test]
fn target_and_module_aliases_map_to_runtime_kinds() {
    let args = CliArgs::try_parse_from(["tsz", "--target", "es6", "--module", "common-js"])
        .expect("alias options should parse");

    assert_eq!(args.target, Some(Target::Es2015));
    assert_eq!(
        args.target
            .expect("target should be present")
            .to_script_target(),
        ScriptTarget::ES2015
    );
    assert_eq!(args.module, Some(Module::CommonJs));
    assert_eq!(
        args.module
            .expect("module should be present")
            .to_module_kind(),
        ModuleKind::CommonJS
    );
}

#[test]
fn parses_output_formatting_flags() {
    let args = CliArgs::try_parse_from([
        "tsz",
        "--pretty",
        "true",
        "--noErrorTruncation",
        "--preserveWatchOutput",
    ])
    .expect("output formatting flags should parse");

    assert_eq!(args.pretty, Some(true));
    assert!(args.no_error_truncation);
    assert!(args.preserve_watch_output);
}

#[test]
fn parses_watch_flags() {
    let args = CliArgs::try_parse_from([
        "tsz",
        "-w",
        "--watchFile",
        "usefsevents",
        "--watchDirectory",
        "usefsevents",
        "--fallbackPolling",
        "fixedinterval",
        "--synchronousWatchDirectory",
        "--excludeDirectories",
        "node_modules,dist",
        "--excludeFiles",
        "test.ts,spec.ts",
    ])
    .expect("watch flags should parse");

    assert!(args.watch);
    assert_eq!(args.watch_file, Some(WatchFileKind::UseFsEvents));
    assert_eq!(args.watch_directory, Some(WatchDirectoryKind::UseFsEvents));
    assert_eq!(args.fallback_polling, Some(PollingWatchKind::FixedInterval));
    assert!(args.synchronous_watch_directory);
    assert_eq!(args.exclude_directories.as_ref().map(|v| v.len()), Some(2));
    assert_eq!(args.exclude_files.as_ref().map(|v| v.len()), Some(2));
}

#[test]
fn parses_watch_file_variants() {
    let variants = [
        ("fixedpollinginterval", WatchFileKind::FixedPollingInterval),
        (
            "prioritypollinginterval",
            WatchFileKind::PriorityPollingInterval,
        ),
        (
            "dynamicprioritypolling",
            WatchFileKind::DynamicPriorityPolling,
        ),
        (
            "fixedchunksizepolling",
            WatchFileKind::FixedChunkSizePolling,
        ),
        ("usefsevents", WatchFileKind::UseFsEvents),
        (
            "usefseventsonparentdirectory",
            WatchFileKind::UseFsEventsOnParentDirectory,
        ),
    ];

    for (input, expected) in variants {
        let args = CliArgs::try_parse_from(["tsz", "--watchFile", input])
            .unwrap_or_else(|_| panic!("--watchFile {input} should parse"));
        assert_eq!(args.watch_file, Some(expected), "watchFile {input} failed");
    }
}

#[test]
fn parses_watch_directory_variants() {
    let variants = [
        ("usefsevents", WatchDirectoryKind::UseFsEvents),
        (
            "fixedpollinginterval",
            WatchDirectoryKind::FixedPollingInterval,
        ),
        (
            "dynamicprioritypolling",
            WatchDirectoryKind::DynamicPriorityPolling,
        ),
        (
            "fixedchunksizepolling",
            WatchDirectoryKind::FixedChunkSizePolling,
        ),
    ];

    for (input, expected) in variants {
        let args = CliArgs::try_parse_from(["tsz", "--watchDirectory", input])
            .unwrap_or_else(|_| panic!("--watchDirectory {input} should parse"));
        assert_eq!(
            args.watch_directory,
            Some(expected),
            "watchDirectory {input} failed"
        );
    }
}

#[test]
fn parses_fallback_polling_variants() {
    let variants = [
        ("fixedinterval", PollingWatchKind::FixedInterval),
        ("priorityinterval", PollingWatchKind::PriorityInterval),
        ("dynamicpriority", PollingWatchKind::DynamicPriority),
        ("fixedchunksize", PollingWatchKind::FixedChunkSize),
    ];

    for (input, expected) in variants {
        let args = CliArgs::try_parse_from(["tsz", "--fallbackPolling", input])
            .unwrap_or_else(|_| panic!("--fallbackPolling {input} should parse"));
        assert_eq!(
            args.fallback_polling,
            Some(expected),
            "fallbackPolling {input} failed"
        );
    }
}

#[test]
fn parses_build_flags() {
    let args = CliArgs::try_parse_from([
        "tsz",
        "-b",
        "--build-verbose",
        "--dry",
        "-f",
        "--clean",
        "--stopBuildOnErrors",
    ])
    .expect("build flags should parse");

    assert!(args.build);
    assert!(args.build_verbose);
    assert!(args.dry);
    assert!(args.force);
    assert!(args.clean);
    assert!(args.stop_build_on_errors);
}

#[test]
fn parses_decorator_flags() {
    let args =
        CliArgs::try_parse_from(["tsz", "--experimentalDecorators", "--emitDecoratorMetadata"])
            .expect("decorator flags should parse");

    assert!(args.experimental_decorators);
    assert!(args.emit_decorator_metadata);
}

#[test]
fn parses_lib_flag() {
    let args = CliArgs::try_parse_from(["tsz", "--lib", "es2020,dom,dom.iterable"])
        .expect("--lib should parse");

    assert_eq!(
        args.lib,
        Some(vec![
            "es2020".to_string(),
            "dom".to_string(),
            "dom.iterable".to_string()
        ])
    );
}

#[test]
fn parses_short_flags() {
    // Test short flag versions
    let args = CliArgs::try_parse_from([
        "tsz", "-t", "es2020", "-m", "commonjs", "-p", ".", "-w", "-d", "-i", "-b", "-f",
    ])
    .expect("short flags should parse");

    assert_eq!(args.target, Some(Target::Es2020));
    assert_eq!(args.module, Some(Module::CommonJs));
    assert_eq!(args.project.as_deref(), Some(std::path::Path::new(".")));
    assert!(args.watch);
    assert!(args.declaration);
    assert!(args.incremental);
    assert!(args.build);
    assert!(args.force);
}

#[test]
fn parses_kebab_case_aliases() {
    // Test that kebab-case aliases work
    let args = CliArgs::try_parse_from([
        "tsz",
        "--out-dir",
        "dist",
        "--root-dir",
        "src",
        "--out-file",
        "bundle.js",
        "--no-emit",
        "--source-map",
        "--declaration-map",
        "--no-emit-on-error",
        "--es-module-interop",
        "--isolated-modules",
        "--skip-lib-check",
    ])
    .expect("kebab-case aliases should parse");

    assert_eq!(args.out_dir.as_deref(), Some(std::path::Path::new("dist")));
    assert_eq!(args.root_dir.as_deref(), Some(std::path::Path::new("src")));
    assert_eq!(
        args.out_file.as_deref(),
        Some(std::path::Path::new("bundle.js"))
    );
    assert!(args.no_emit);
    assert!(args.source_map);
    assert!(args.declaration_map);
    assert!(args.no_emit_on_error);
    assert!(args.es_module_interop);
    assert!(args.isolated_modules);
    assert!(args.skip_lib_check);
}

#[test]
fn parses_deprecated_flags() {
    // Deprecated flags should still parse but are hidden
    let args = CliArgs::try_parse_from([
        "tsz",
        "--importsNotUsedAsValues",
        "remove",
        "--keyofStringsOnly",
        "--noImplicitUseStrict",
        "--noStrictGenericChecks",
        "--out",
        "bundle.js",
        "--preserveValueImports",
        "--suppressExcessPropertyErrors",
        "--suppressImplicitAnyIndexErrors",
    ])
    .expect("deprecated flags should parse");

    assert_eq!(
        args.imports_not_used_as_values,
        Some(ImportsNotUsedAsValues::Remove)
    );
    assert!(args.keyof_strings_only);
    assert!(args.no_implicit_use_strict);
    assert!(args.no_strict_generic_checks);
    assert_eq!(args.out.as_deref(), Some(std::path::Path::new("bundle.js")));
    assert!(args.preserve_value_imports);
    assert!(args.suppress_excess_property_errors);
    assert!(args.suppress_implicit_any_index_errors);
}

#[test]
fn parses_imports_not_used_as_values_variants() {
    let variants = [
        ("remove", ImportsNotUsedAsValues::Remove),
        ("preserve", ImportsNotUsedAsValues::Preserve),
        ("error", ImportsNotUsedAsValues::Error),
    ];

    for (input, expected) in variants {
        let args = CliArgs::try_parse_from(["tsz", "--importsNotUsedAsValues", input])
            .unwrap_or_else(|_| panic!("--importsNotUsedAsValues {input} should parse"));
        assert_eq!(
            args.imports_not_used_as_values,
            Some(expected),
            "importsNotUsedAsValues {input} failed"
        );
    }
}

#[test]
fn parses_multiple_input_files() {
    let args = CliArgs::try_parse_from(["tsz", "src/index.ts", "src/utils.ts", "src/types.ts"])
        .expect("multiple files should parse");

    assert_eq!(args.files.len(), 3);
    assert_eq!(args.files[0], std::path::PathBuf::from("src/index.ts"));
    assert_eq!(args.files[1], std::path::PathBuf::from("src/utils.ts"));
    assert_eq!(args.files[2], std::path::PathBuf::from("src/types.ts"));
}

#[test]
fn parses_assume_changes_flag() {
    let args = CliArgs::try_parse_from(["tsz", "--assumeChangesOnlyAffectDirectDependencies"])
        .expect("assumeChangesOnlyAffectDirectDependencies should parse");

    assert!(args.assume_changes_only_affect_direct_dependencies);
}

#[test]
fn parses_editor_support_flags() {
    let args = CliArgs::try_parse_from(["tsz", "--disableSizeLimit"])
        .expect("editor support flags should parse");

    assert!(args.disable_size_limit);
}

#[test]
fn parses_types_versions_flag() {
    let args = CliArgs::try_parse_from(["tsz", "--typesVersions", "4.9"])
        .expect("typesVersions should parse");

    assert_eq!(
        args.types_versions_compiler_version,
        Some("4.9".to_string())
    );
}

#[test]
fn parses_use_define_for_class_fields() {
    let args = CliArgs::try_parse_from(["tsz", "--useDefineForClassFields", "true"])
        .expect("useDefineForClassFields true should parse");
    assert_eq!(args.use_define_for_class_fields, Some(true));

    let args = CliArgs::try_parse_from(["tsz", "--useDefineForClassFields", "false"])
        .expect("useDefineForClassFields false should parse");
    assert_eq!(args.use_define_for_class_fields, Some(false));
}

// ==================== Enum-to-runtime-kind conversion tests ====================
//
// These exhaustively cover every variant of the three CLI enums that are
// converted to runtime kinds (`ScriptTarget`, `ModuleKind`,
// `ModuleResolutionKind`) before being threaded into `PrinterOptions` /
// `CheckerOptions` by `tsz-cli/src/driver/core.rs`. A regression in any one
// arm silently mismaps every flag for that target — the surface is wide and
// each mismap would be an effectively invisible CLI bug.
//
// The previous coverage (`target_and_module_aliases_map_to_runtime_kinds`)
// only verified one variant of `Target` and one of `Module`, and no variant
// of `ModuleResolution`.

#[test]
fn target_to_script_target_covers_every_variant() {
    let cases = [
        (Target::Es5, ScriptTarget::ES5),
        (Target::Es2015, ScriptTarget::ES2015),
        (Target::Es2016, ScriptTarget::ES2016),
        (Target::Es2017, ScriptTarget::ES2017),
        (Target::Es2018, ScriptTarget::ES2018),
        (Target::Es2019, ScriptTarget::ES2019),
        (Target::Es2020, ScriptTarget::ES2020),
        (Target::Es2021, ScriptTarget::ES2021),
        (Target::Es2022, ScriptTarget::ES2022),
        (Target::Es2023, ScriptTarget::ES2023),
        (Target::Es2024, ScriptTarget::ES2024),
        (Target::Es2025, ScriptTarget::ES2025),
        (Target::EsNext, ScriptTarget::ESNext),
    ];

    for (input, expected) in cases {
        assert_eq!(
            input.to_script_target(),
            expected,
            "Target::{input:?} should map to {expected:?}"
        );
    }
}

#[test]
fn target_to_script_target_alias_routes_to_same_runtime_kind() {
    // The `es6` alias is parsed as `Target::Es2015` (locked by
    // `parses_target_variants`), and Es2015 maps to ScriptTarget::ES2015.
    // This test pins the end-to-end mapping for the alias path so a
    // future drift in either the alias or the conversion arm fails CI.
    let args = CliArgs::try_parse_from(["tsz", "--target", "es6"])
        .expect("--target es6 alias should parse");
    let target = args.target.expect("target should be present");
    assert_eq!(target, Target::Es2015);
    assert_eq!(target.to_script_target(), ScriptTarget::ES2015);

    let args = CliArgs::try_parse_from(["tsz", "--target", "es-next"])
        .expect("--target es-next alias should parse");
    let target = args.target.expect("target should be present");
    assert_eq!(target, Target::EsNext);
    assert_eq!(target.to_script_target(), ScriptTarget::ESNext);
}

#[test]
fn target_to_script_target_is_total() {
    // Locks the {variant -> ScriptTarget} mapping as a total function.
    // If a new variant is added to `Target` without a matching arm in
    // `to_script_target`, this test will not compile (exhaustiveness),
    // and the assertion below catches an accidental duplicate codomain.
    let all = [
        Target::Es5,
        Target::Es2015,
        Target::Es2016,
        Target::Es2017,
        Target::Es2018,
        Target::Es2019,
        Target::Es2020,
        Target::Es2021,
        Target::Es2022,
        Target::Es2023,
        Target::Es2024,
        Target::Es2025,
        Target::EsNext,
    ];
    let mut kinds: Vec<ScriptTarget> = all.iter().map(|t| t.to_script_target()).collect();
    let count = kinds.len();
    kinds.sort_by_key(|k| format!("{k:?}"));
    kinds.dedup();
    assert_eq!(
        kinds.len(),
        count,
        "every Target variant must map to a unique ScriptTarget"
    );
}

#[test]
fn module_to_module_kind_covers_every_variant() {
    let cases = [
        (Module::None, ModuleKind::None),
        (Module::CommonJs, ModuleKind::CommonJS),
        (Module::Amd, ModuleKind::AMD),
        (Module::Umd, ModuleKind::UMD),
        (Module::System, ModuleKind::System),
        (Module::Es2015, ModuleKind::ES2015),
        (Module::Es2020, ModuleKind::ES2020),
        (Module::Es2022, ModuleKind::ES2022),
        (Module::EsNext, ModuleKind::ESNext),
        (Module::Node16, ModuleKind::Node16),
        (Module::Node18, ModuleKind::Node18),
        (Module::Node20, ModuleKind::Node20),
        (Module::NodeNext, ModuleKind::NodeNext),
        (Module::Preserve, ModuleKind::Preserve),
    ];

    for (input, expected) in cases {
        assert_eq!(
            input.to_module_kind(),
            expected,
            "Module::{input:?} should map to {expected:?}"
        );
    }
}

#[test]
fn module_to_module_kind_alias_routes_to_same_runtime_kind() {
    // `--module common-js` -> `Module::CommonJs` -> `ModuleKind::CommonJS`
    let args = CliArgs::try_parse_from(["tsz", "--module", "common-js"])
        .expect("--module common-js alias should parse");
    let module = args.module.expect("module should be present");
    assert_eq!(module, Module::CommonJs);
    assert_eq!(module.to_module_kind(), ModuleKind::CommonJS);

    // `--module es6` -> `Module::Es2015` -> `ModuleKind::ES2015`
    let args = CliArgs::try_parse_from(["tsz", "--module", "es6"])
        .expect("--module es6 alias should parse");
    let module = args.module.expect("module should be present");
    assert_eq!(module, Module::Es2015);
    assert_eq!(module.to_module_kind(), ModuleKind::ES2015);

    // `--module node-next` -> `Module::NodeNext` -> `ModuleKind::NodeNext`
    let args = CliArgs::try_parse_from(["tsz", "--module", "node-next"])
        .expect("--module node-next alias should parse");
    let module = args.module.expect("module should be present");
    assert_eq!(module, Module::NodeNext);
    assert_eq!(module.to_module_kind(), ModuleKind::NodeNext);
}

#[test]
fn module_to_module_kind_is_total() {
    let all = [
        Module::None,
        Module::CommonJs,
        Module::Amd,
        Module::Umd,
        Module::System,
        Module::Es2015,
        Module::Es2020,
        Module::Es2022,
        Module::EsNext,
        Module::Node16,
        Module::Node18,
        Module::Node20,
        Module::NodeNext,
        Module::Preserve,
    ];
    let mut kinds: Vec<ModuleKind> = all.iter().map(|m| m.to_module_kind()).collect();
    let count = kinds.len();
    kinds.sort_by_key(|k| format!("{k:?}"));
    kinds.dedup();
    assert_eq!(
        kinds.len(),
        count,
        "every Module variant must map to a unique ModuleKind"
    );
}

#[test]
fn module_resolution_to_kind_covers_every_variant() {
    use crate::config::ModuleResolutionKind;

    let cases = [
        (ModuleResolution::Classic, ModuleResolutionKind::Classic),
        // `Node10` collapses to `Node` in the runtime kind (the runtime
        // enum predates the Node10/Node renaming and only exposes `Node`).
        (ModuleResolution::Node10, ModuleResolutionKind::Node),
        (ModuleResolution::Node16, ModuleResolutionKind::Node16),
        (ModuleResolution::NodeNext, ModuleResolutionKind::NodeNext),
        (ModuleResolution::Bundler, ModuleResolutionKind::Bundler),
    ];

    for (input, expected) in cases {
        assert_eq!(
            input.to_module_resolution_kind(),
            expected,
            "ModuleResolution::{input:?} should map to {expected:?}"
        );
    }
}

#[test]
fn module_resolution_alias_routes_to_same_runtime_kind() {
    use crate::config::ModuleResolutionKind;

    // `--moduleResolution node` is an alias for `Node10` and must map to
    // `ModuleResolutionKind::Node` (the legacy runtime kind name).
    let args = CliArgs::try_parse_from(["tsz", "--moduleResolution", "node"])
        .expect("--moduleResolution node alias should parse");
    let resolution = args
        .module_resolution
        .expect("module_resolution should be present");
    assert_eq!(resolution, ModuleResolution::Node10);
    assert_eq!(
        resolution.to_module_resolution_kind(),
        ModuleResolutionKind::Node
    );

    // `--moduleResolution node-next` -> `NodeNext` -> `NodeNext`
    let args = CliArgs::try_parse_from(["tsz", "--moduleResolution", "node-next"])
        .expect("--moduleResolution node-next alias should parse");
    let resolution = args
        .module_resolution
        .expect("module_resolution should be present");
    assert_eq!(resolution, ModuleResolution::NodeNext);
    assert_eq!(
        resolution.to_module_resolution_kind(),
        ModuleResolutionKind::NodeNext
    );
}

#[test]
fn module_resolution_to_kind_is_total_modulo_node10_alias() {
    use crate::config::ModuleResolutionKind;

    // The runtime `ModuleResolutionKind` enum has 5 distinct variants
    // (Classic, Node, Node16, NodeNext, Bundler). The CLI `ModuleResolution`
    // enum also has 5 (Classic, Node10, Node16, NodeNext, Bundler) — Node10
    // maps to the legacy-named `Node`. We expect a perfect 5->5 bijection.
    let all = [
        ModuleResolution::Classic,
        ModuleResolution::Node10,
        ModuleResolution::Node16,
        ModuleResolution::NodeNext,
        ModuleResolution::Bundler,
    ];
    let mut kinds: Vec<ModuleResolutionKind> =
        all.iter().map(|r| r.to_module_resolution_kind()).collect();
    let count = kinds.len();
    kinds.sort_by_key(|k| format!("{k:?}"));
    kinds.dedup();
    assert_eq!(
        kinds.len(),
        count,
        "every ModuleResolution variant must map to a unique ModuleResolutionKind"
    );
}

#[test]
fn end_to_end_target_module_resolution_wiring() {
    use crate::config::ModuleResolutionKind;

    // Sanity-check the full CLI -> runtime-kind wiring for a representative
    // mix of options that downstream `driver/core.rs` depends on.
    let args = CliArgs::try_parse_from([
        "tsz",
        "--target",
        "es2025",
        "--module",
        "node20",
        "--moduleResolution",
        "bundler",
    ])
    .expect("flagged conversion args should parse");

    assert_eq!(
        args.target.expect("target").to_script_target(),
        ScriptTarget::ES2025
    );
    assert_eq!(
        args.module.expect("module").to_module_kind(),
        ModuleKind::Node20
    );
    assert_eq!(
        args.module_resolution
            .expect("module_resolution")
            .to_module_resolution_kind(),
        ModuleResolutionKind::Bundler
    );
}

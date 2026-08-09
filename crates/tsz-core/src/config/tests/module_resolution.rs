//! Module and module-resolution diagnostic tests
//! (TS6046 enum options, module + resolution defaults, TS5095 bundler
//! compatibility, TS5098 `package.json` resolution, TS5102 removed options,
//! the absence of TS5103 `ignoreDeprecations` value validation, TS5110,
//! inherited `extends` anchoring).
//!
//! Split from `config/mod.rs` to keep each file under the 2000-line limit
//! (§19; ratchet tracked by #8280).

use super::super::*;
use tempfile::tempdir;

#[test]
fn test_parse_module_resolution_rejects_comma_separated_value() {
    let json = r#"{"compilerOptions":{"moduleResolution":"node16,nodenext","module":"commonjs"}} "#;
    let config: TsConfig = serde_json::from_str(json).unwrap();
    let err = resolve_compiler_options(config.compiler_options.as_ref())
        .expect_err("comma-separated moduleResolution should be rejected");
    assert!(
        err.to_string().contains("compilerOptions.moduleResolution"),
        "{err}"
    );
}

#[test]
fn test_ts6046_emitted_for_comma_separated_enum_options() {
    for (option, value, flag) in [
        ("target", "es2020,esnext", "--target"),
        ("module", "commonjs,esnext", "--module"),
        ("moduleResolution", "node,bundler", "--moduleResolution"),
        ("moduleDetection", "auto,force", "--moduleDetection"),
        ("newLine", "lf,crlf", "--newLine"),
    ] {
        let source = format!(r#"{{"compilerOptions":{{"{option}":"{value}"}}}}"#);
        let parsed = parse_tsconfig_with_diagnostics(&source, "tsconfig.json").unwrap();
        let diagnostic = parsed
            .diagnostics
            .iter()
            .find(|diag| diag.code == diagnostic_codes::ARGUMENT_FOR_OPTION_MUST_BE)
            .unwrap_or_else(|| {
                panic!(
                    "Expected TS6046 for compilerOptions.{option}, got: {:?}",
                    parsed.diagnostics
                )
            });

        assert!(
            diagnostic.message_text.contains(flag),
            "Unexpected TS6046 message for compilerOptions.{option}: {}",
            diagnostic.message_text
        );
        assert_eq!(
            diagnostic.start,
            source.find(&format!(r#""{value}""#)).unwrap() as u32
        );
    }
}

#[test]
fn test_ts6046_emitted_for_separator_mutated_enum_options() {
    for (option, value, flag) in [
        ("target", "es_2020", "--target"),
        ("target", "es-2020", "--target"),
        ("target", "es 2020", "--target"),
        ("module", "node_next", "--module"),
        ("jsx", "react_jsx", "--jsx"),
        ("moduleResolution", "node_16", "--moduleResolution"),
    ] {
        let source = format!(
            r#"{{"compilerOptions":{{"{option}":"{value}","noEmit":true}},"files":["a.ts"]}}"#
        );
        let parsed = parse_tsconfig_with_diagnostics(&source, "tsconfig.json").unwrap();
        let codes: Vec<u32> = parsed.diagnostics.iter().map(|diag| diag.code).collect();
        let diagnostic = parsed
            .diagnostics
            .iter()
            .find(|diag| diag.code == diagnostic_codes::ARGUMENT_FOR_OPTION_MUST_BE)
            .unwrap_or_else(|| {
                panic!(
                    "Expected TS6046 for compilerOptions.{option}={value:?}, got: {:?}",
                    parsed.diagnostics
                )
            });

        assert!(
            diagnostic.message_text.contains(flag),
            "Unexpected TS6046 message for compilerOptions.{option}: {}",
            diagnostic.message_text
        );
        assert!(
            !codes.contains(
                &diagnostic_codes::OPTION_MODULE_MUST_BE_SET_TO_WHEN_OPTION_MODULERESOLUTION_IS_SET_TO
            ),
            "separator-mutated moduleResolution should not produce follow-on TS5110, got: {:?}",
            parsed.diagnostics
        );
        assert_eq!(
            diagnostic.start,
            source.find(&format!(r#""{value}""#)).unwrap() as u32
        );
    }
}

#[test]
fn test_ts6046_emitted_for_invalid_module_detection_and_new_line() {
    for (option, value, flag, expected_values) in [
        (
            "moduleDetection",
            "bogus",
            "--moduleDetection",
            "'auto', 'legacy', 'force'",
        ),
        ("newLine", "bogus", "--newLine", "'crlf', 'lf'"),
    ] {
        let source = format!(r#"{{"compilerOptions":{{"{option}":"{value}"}}}}"#);
        let parsed = parse_tsconfig_with_diagnostics(&source, "tsconfig.json").unwrap();
        let diagnostic = parsed
            .diagnostics
            .iter()
            .find(|diag| diag.code == diagnostic_codes::ARGUMENT_FOR_OPTION_MUST_BE)
            .unwrap_or_else(|| {
                panic!(
                    "Expected TS6046 for compilerOptions.{option}, got: {:?}",
                    parsed.diagnostics
                )
            });

        assert!(
            diagnostic.message_text.contains(flag)
                && diagnostic.message_text.contains(expected_values),
            "Unexpected TS6046 message for compilerOptions.{option}: {}",
            diagnostic.message_text
        );
        assert_eq!(
            diagnostic.start,
            source.find(&format!(r#""{value}""#)).unwrap() as u32
        );

        let resolved = resolve_compiler_options(parsed.config.compiler_options.as_ref())
            .expect("invalid enum value should be nulled before resolution");
        if option == "moduleDetection" {
            assert!(!resolved.printer.module_detection_force);
            assert!(!resolved.printer.module_detection_legacy);
        }
    }
}

#[test]
fn test_shared_module_defaults_cover_targets_and_resolution() {
    // The shared computed-module table covers every dated target tier.
    assert_eq!(
        default_module_kind_for_target(ScriptTarget::ES5, true),
        ModuleKind::CommonJS
    );
    assert_eq!(
        default_module_kind_for_target(ScriptTarget::ES2015, true),
        ModuleKind::ES2015
    );
    assert_eq!(
        default_module_kind_for_target(ScriptTarget::ES2019, true),
        ModuleKind::ES2015
    );
    assert_eq!(
        default_module_kind_for_target(ScriptTarget::ES2020, true),
        ModuleKind::ES2020
    );
    assert_eq!(
        default_module_kind_for_target(ScriptTarget::ES2021, true),
        ModuleKind::ES2020
    );
    assert_eq!(
        default_module_kind_for_target(ScriptTarget::ES2022, true),
        ModuleKind::ES2022
    );
    assert_eq!(
        default_module_kind_for_target(ScriptTarget::ES2025, true),
        ModuleKind::ES2022
    );
    assert_eq!(
        default_module_kind_for_target(ScriptTarget::ESNext, true),
        ModuleKind::ESNext
    );
    // Preserve the internal fallback for an `ES3` enum after callers have
    // already diagnosed the invalid CLI value with TS6046.
    assert_eq!(
        default_module_kind_for_target(ScriptTarget::ES3, true),
        ModuleKind::ES2022
    );
    // An omitted `target` resolves to `LatestStandard` (`ES2025`) → `ES2022`,
    // independent of the already-defaulted `ScriptTarget` the caller passes.
    assert_eq!(
        default_module_kind_for_target(ScriptTarget::ES2025, false),
        ModuleKind::ES2022
    );
    assert_eq!(
        default_module_kind_for_target(ScriptTarget::ESNext, false),
        ModuleKind::ES2022
    );
    assert_eq!(
        default_module_resolution_for_module(ModuleKind::System),
        ModuleResolutionKind::Classic
    );
    assert_eq!(
        default_module_resolution_for_module(ModuleKind::CommonJS),
        ModuleResolutionKind::Bundler
    );
    assert_eq!(
        default_module_resolution_for_module(ModuleKind::Node20),
        ModuleResolutionKind::Node16
    );
    assert_eq!(
        default_module_resolution_for_module(ModuleKind::NodeNext),
        ModuleResolutionKind::NodeNext
    );
}

#[test]
fn test_omitted_module_resolves_to_default_es2022() {
    // The default module derives from `LatestStandard` when `module` and
    // `target` are both omitted, landing on `ES2022` rather than `ESNext`.
    let resolved = resolve_compiler_options(None).unwrap();
    assert_eq!(resolved.printer.module, ModuleKind::ES2022);
    assert_eq!(resolved.checker.module, ModuleKind::ES2022);
    assert!(!resolved.checker.module_explicitly_set);

    // An explicit target with `module` omitted follows the same tiered table.
    let with_target = |target: &str| {
        let json = format!(r#"{{"compilerOptions":{{"target":"{target}"}}}}"#);
        let config: TsConfig = serde_json::from_str(&json).unwrap();
        resolve_compiler_options(config.compiler_options.as_ref())
            .unwrap()
            .printer
            .module
    };
    assert_eq!(with_target("es5"), ModuleKind::CommonJS);
    assert_eq!(with_target("es2015"), ModuleKind::ES2015);
    assert_eq!(with_target("es2020"), ModuleKind::ES2020);
    assert_eq!(with_target("es2022"), ModuleKind::ES2022);
    assert_eq!(with_target("esnext"), ModuleKind::ESNext);
}

#[test]
fn test_module_explicitly_set_when_specified() {
    let json = r#"{"compilerOptions":{"module":"es2015"}}"#;
    let config: TsConfig = serde_json::from_str(json).unwrap();
    let resolved = resolve_compiler_options(config.compiler_options.as_ref()).unwrap();
    assert!(resolved.checker.module_explicitly_set);
    assert!(resolved.checker.module.is_es_module());
}

#[test]
fn test_module_explicitly_set_commonjs() {
    let json = r#"{"compilerOptions":{"module":"commonjs"}}"#;
    let config: TsConfig = serde_json::from_str(json).unwrap();
    let resolved = resolve_compiler_options(config.compiler_options.as_ref()).unwrap();
    assert!(resolved.checker.module_explicitly_set);
    assert!(!resolved.checker.module.is_es_module());
}

#[test]
fn test_module_not_explicitly_set_defaults_from_target() {
    // When module is not specified, it's computed from target.
    // module_explicitly_set is false (module was derived, not explicit).
    let json = r#"{"compilerOptions":{"target":"es2015"}}"#;
    let config: TsConfig = serde_json::from_str(json).unwrap();
    let resolved = resolve_compiler_options(config.compiler_options.as_ref()).unwrap();
    assert!(!resolved.checker.module_explicitly_set);
    // Module defaults to ES2015 for es2015+ targets
    assert!(resolved.checker.module.is_es_module());
}

#[test]
fn test_effective_module_resolution_defaults_to_bundler_for_es_modules() {
    // ES module kinds default to Bundler resolution rather than legacy Classic.
    let json = r#"{"compilerOptions":{"module":"es2015","target":"es2015"}}"#;
    let config: TsConfig = serde_json::from_str(json).unwrap();
    let resolved = resolve_compiler_options(config.compiler_options.as_ref()).unwrap();
    assert_eq!(
        resolved.effective_module_resolution(),
        ModuleResolutionKind::Bundler
    );
}

#[test]
fn test_no_config_defaults_to_bundler_and_resolve_json_module() {
    let resolved = resolve_compiler_options(None).unwrap();

    assert_eq!(
        resolved.effective_module_resolution(),
        ModuleResolutionKind::Bundler
    );
    assert!(resolved.resolve_json_module);
    assert!(resolved.checker.resolve_json_module);
}

#[test]
fn test_effective_module_resolution_prefers_explicit_override() {
    let json =
        r#"{"compilerOptions":{"module":"es2015","moduleResolution":"bundler","target":"es2015"}}"#;
    let config: TsConfig = serde_json::from_str(json).unwrap();
    let resolved = resolve_compiler_options(config.compiler_options.as_ref()).unwrap();
    assert_eq!(
        resolved.effective_module_resolution(),
        ModuleResolutionKind::Bundler
    );
}

#[test]
fn test_module_not_explicitly_set_no_options() {
    // When no options at all, module_explicitly_set should be false.
    let resolved = resolve_compiler_options(None).unwrap();
    assert!(!resolved.checker.module_explicitly_set);
    assert!(
        resolved.printer.always_strict,
        "printer alwaysStrict should default to true with no compiler options"
    );
}

#[test]
fn test_ts5023_emitted_for_dropped_option() {
    let source = r#"{"compilerOptions":{"noImplicitUseStrict":true}}"#;
    let parsed = parse_tsconfig_with_diagnostics(source, "tsconfig.json").unwrap();
    let codes: Vec<u32> = parsed.diagnostics.iter().map(|d| d.code).collect();
    assert!(
        codes.contains(&5023),
        "Expected TS5023 for dropped option noImplicitUseStrict, got: {codes:?}"
    );
}

#[test]
fn test_ts5023_emitted_for_false_dropped_option() {
    let source = r#"{"compilerOptions":{"noImplicitUseStrict":false}}"#;
    let parsed = parse_tsconfig_with_diagnostics(source, "tsconfig.json").unwrap();
    let codes: Vec<u32> = parsed.diagnostics.iter().map(|d| d.code).collect();
    assert!(
        codes.contains(&5023),
        "Expected TS5023 for false-valued dropped option, got: {codes:?}"
    );
}

#[test]
fn test_ts7_dropped_option_null_is_unset() {
    let source = r#"{"compilerOptions":{"noImplicitUseStrict":null}}"#;
    let parsed = parse_tsconfig_with_diagnostics(source, "tsconfig.json").unwrap();
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
}

#[test]
fn test_ts5023_emitted_for_string_dropped_option() {
    let source = r#"{"compilerOptions":{"importsNotUsedAsValues":"error"}}"#;
    let parsed = parse_tsconfig_with_diagnostics(source, "tsconfig.json").unwrap();
    let codes: Vec<u32> = parsed.diagnostics.iter().map(|d| d.code).collect();
    assert!(
        codes.contains(&5023),
        "Expected TS5023 for dropped option importsNotUsedAsValues, got: {codes:?}"
    );
}

#[test]
fn test_ts5023_not_suppressed_with_ignore_deprecations() {
    // TypeScript 7 treats this dropped name as an unknown option; the
    // historical ignoreDeprecations setting does not suppress TS5023.
    let source = r#"{"compilerOptions":{"ignoreDeprecations":"5.0","noImplicitUseStrict":true}}"#;
    let parsed = parse_tsconfig_with_diagnostics(source, "tsconfig.json").unwrap();
    let codes: Vec<u32> = parsed.diagnostics.iter().map(|d| d.code).collect();
    assert!(
        codes.contains(&5023),
        "Expected TS5023 even with ignoreDeprecations '5.0', got: {codes:?}"
    );
}

#[test]
fn test_ts5023_not_suppressed_with_invalid_ignore_deprecations() {
    // An unrecognized ignoreDeprecations value neither suppresses the
    // dropped-name TS5023 nor adds a TS5103 of its own (#16228).
    let source = r#"{"compilerOptions":{"ignoreDeprecations":"8.0","noImplicitUseStrict":true}}"#;
    let parsed = parse_tsconfig_with_diagnostics(source, "tsconfig.json").unwrap();
    let codes: Vec<u32> = parsed.diagnostics.iter().map(|d| d.code).collect();
    assert!(
        codes.contains(&5023),
        "Expected TS5023 when ignoreDeprecations is invalid, got: {codes:?}"
    );
    assert!(
        !codes.contains(&5103),
        "Should NOT emit TS5103 for an unrecognized ignoreDeprecations, got: {codes:?}"
    );
}

#[test]
fn test_ts5023_fires_with_ignore_deprecations_6_0() {
    // "6.0" remains a valid historical ignoreDeprecations value in TS7,
    // but it does not suppress a dropped-name TS5023.
    let source = r#"{"compilerOptions":{"ignoreDeprecations":"6.0","noImplicitUseStrict":true}}"#;
    let parsed = parse_tsconfig_with_diagnostics(source, "tsconfig.json").unwrap();
    let codes: Vec<u32> = parsed.diagnostics.iter().map(|d| d.code).collect();
    assert!(
        codes.contains(&5023),
        "Expected TS5023 even with ignoreDeprecations '6.0', got: {codes:?}"
    );
    assert!(
        !codes.contains(&5103),
        "Should NOT emit TS5103 — '6.0' is a valid ignoreDeprecations value, got: {codes:?}"
    );
}

#[test]
fn test_ts5023_fires_for_all_ts7_dropped_options() {
    let removed_opts = [
        ("noImplicitUseStrict", "true"),
        ("keyofStringsOnly", "true"),
        ("suppressExcessPropertyErrors", "true"),
        ("suppressImplicitAnyIndexErrors", "true"),
        ("noStrictGenericChecks", "true"),
        ("charset", r#""utf8""#),
        ("importsNotUsedAsValues", r#""error""#),
        ("preserveValueImports", "true"),
        ("out", r#""out.js""#),
    ];
    for (opt, val) in &removed_opts {
        let source =
            format!(r#"{{"compilerOptions":{{"{opt}":{val},"ignoreDeprecations":"6.0"}}}}"#);
        let parsed = parse_tsconfig_with_diagnostics(&source, "tsconfig.json").unwrap();
        let codes: Vec<u32> = parsed.diagnostics.iter().map(|d| d.code).collect();
        assert!(
            codes.contains(&5023),
            "Expected TS5023 for TS7-dropped option '{opt}', got: {codes:?}"
        );
    }
}

#[test]
fn test_ts5023_inherited_from_extends_stays_anchored_at_base_option() {
    // Repro from `verbatimModuleSyntaxCompat3.ts`. When the extending
    // tsconfig.json uses `verbatimModuleSyntax` and the base tsconfig
    // contains dropped options (`preserveValueImports` and
    // `importsNotUsedAsValues`), TS7 keeps each TS5023 anchored at its
    // declaring base option. Reproducing requires real tempfiles because
    // inheritance resolution reads from disk.
    use tempfile::tempdir;
    let temp = tempdir().expect("create temp dir");
    let base_path = temp.path().join("tsconfig.base.json");
    let child_path = temp.path().join("tsconfig.json");
    std::fs::write(
        &base_path,
        r#"{
"compilerOptions": {
    "isolatedModules": true,
    "preserveValueImports": true,
    "importsNotUsedAsValues": "error"
}
}"#,
    )
    .expect("write base");
    let child_source = r#"{
"extends": "./tsconfig.base.json",
"compilerOptions": {
    "verbatimModuleSyntax": true
}
}"#;
    std::fs::write(&child_path, child_source).expect("write child");

    let parsed = load_tsconfig_with_diagnostics(&child_path).expect("load");
    let ts5023: Vec<&Diagnostic> = parsed
        .diagnostics
        .iter()
        .filter(|d| d.code == 5023)
        .collect();
    assert!(
        ts5023.len() >= 2,
        "Expected two TS5023 diagnostics from the base config, got: {ts5023:?}"
    );
    let expected_base = std::fs::canonicalize(&base_path).expect("canonicalize base config");
    for diag in &ts5023 {
        assert_eq!(
            std::fs::canonicalize(&diag.file).expect("canonicalize diagnostic path"),
            expected_base
        );
    }
}

#[test]
fn test_inherited_base_url_anchored_at_base_config_dir() {
    // tsc resolves a tsconfig's `baseUrl` relative to the config file
    // that declares it. When a child extends a base that sets
    // `baseUrl: "."`, the inherited `baseUrl` must point at the *base*
    // config's directory, not the child's. Issue #3332 reproduced the
    // child-anchored bug, which broke inherited `paths` mappings.
    let temp = tempdir().expect("create temp dir");
    let base_dir = temp.path().join("base");
    let app_dir = temp.path().join("app");
    std::fs::create_dir_all(&base_dir).expect("create base dir");
    std::fs::create_dir_all(&app_dir).expect("create app dir");

    let base_path = base_dir.join("tsconfig.base.json");
    std::fs::write(
        &base_path,
        r#"{
"compilerOptions": {
    "baseUrl": ".",
    "paths": { "@shared/*": ["shared/*"] }
}
}"#,
    )
    .expect("write base");

    let child_path = app_dir.join("tsconfig.json");
    std::fs::write(
        &child_path,
        r#"{
"extends": "../base/tsconfig.base.json",
"files": ["src/index.ts"]
}"#,
    )
    .expect("write child");

    let merged = load_tsconfig(&child_path).expect("load child");
    let opts = merged.compiler_options.expect("compiler options merged");
    let base_url = opts.base_url.expect("inherited baseUrl present");

    // Canonicalize to handle macOS `/var` → `/private/var` symlinks.
    let canonical_base_dir = std::fs::canonicalize(&base_dir).unwrap_or(base_dir);
    let canonical_app_dir = std::fs::canonicalize(&app_dir).unwrap_or(app_dir);
    let canonical_base_url = std::path::Path::new(&base_url)
        .canonicalize()
        .unwrap_or_else(|_| std::path::PathBuf::from(&base_url));
    let expected = canonical_base_dir.to_string_lossy();
    let actual = canonical_base_url.to_string_lossy();
    assert!(
        actual.starts_with(expected.as_ref()),
        "Inherited baseUrl must anchor at the base config's directory \
         (expected prefix {expected:?}, got {actual:?})"
    );
    assert!(
        !actual.starts_with(canonical_app_dir.to_string_lossy().as_ref()),
        "Inherited baseUrl must not anchor at the child's directory: {actual:?}"
    );
}

#[test]
fn test_inherited_root_dirs_anchor_at_declaring_config_dir() {
    let temp = tempdir().expect("create temp dir");
    let base_dir = temp.path().join("base");
    let app_dir = temp.path().join("app");
    std::fs::create_dir_all(&base_dir).expect("create base dir");
    std::fs::create_dir_all(&app_dir).expect("create app dir");

    let base_path = base_dir.join("tsconfig.base.json");
    std::fs::write(
        &base_path,
        r#"{
"compilerOptions": {
    "rootDirs": ["src", "generated"]
}
}"#,
    )
    .expect("write base");

    let child_path = app_dir.join("tsconfig.json");
    std::fs::write(
        &child_path,
        r#"{
"extends": "../base/tsconfig.base.json",
"files": ["src/index.ts"]
}"#,
    )
    .expect("write child");

    let merged = load_tsconfig(&child_path).expect("load child");
    let opts = merged.compiler_options.expect("compiler options merged");
    let root_dirs = opts.root_dirs.expect("inherited rootDirs present");
    let expected_base = base_dir
        .canonicalize()
        .expect("canonicalize base")
        .to_string_lossy()
        .into_owned();
    let unexpected_app = app_dir
        .canonicalize()
        .expect("canonicalize app")
        .to_string_lossy()
        .into_owned();

    assert_eq!(root_dirs.len(), 2);
    for root_dir in &root_dirs {
        assert!(
            root_dir.starts_with(&expected_base),
            "Inherited rootDirs must anchor at the base config's directory, got {root_dir:?}"
        );
        assert!(
            !root_dir.starts_with(&unexpected_app),
            "Inherited rootDirs must not anchor at the child's directory: {root_dir:?}"
        );
    }
}

#[test]
fn test_inherited_path_options_anchor_at_declaring_config_dir() {
    let temp = tempdir().expect("create temp dir");
    let base_dir = temp.path().join("base");
    let app_dir = temp.path().join("app");
    std::fs::create_dir_all(&base_dir).expect("create base dir");
    std::fs::create_dir_all(&app_dir).expect("create app dir");

    let base_path = base_dir.join("tsconfig.base.json");
    std::fs::write(
        &base_path,
        r#"{
"compilerOptions": {
    "rootDir": "src",
    "outDir": "dist",
    "declarationDir": "types",
    "tsBuildInfoFile": ".cache/project.tsbuildinfo",
    "typeRoots": ["./types"]
}
}"#,
    )
    .expect("write base");

    let child_path = app_dir.join("tsconfig.json");
    std::fs::write(
        &child_path,
        r#"{
"extends": "../base/tsconfig.base.json",
"files": ["src/index.ts"]
}"#,
    )
    .expect("write child");

    let merged = load_tsconfig(&child_path).expect("load child");
    let opts = merged.compiler_options.expect("compiler options merged");
    let type_roots = opts.type_roots.expect("inherited typeRoots present");
    let expected_base = base_dir
        .canonicalize()
        .expect("canonicalize base")
        .to_string_lossy()
        .into_owned();
    let unexpected_app = app_dir
        .canonicalize()
        .expect("canonicalize app")
        .to_string_lossy()
        .into_owned();

    for (name, value) in [
        ("rootDir", opts.root_dir.expect("rootDir present")),
        ("outDir", opts.out_dir.expect("outDir present")),
        (
            "declarationDir",
            opts.declaration_dir.expect("declarationDir present"),
        ),
        (
            "tsBuildInfoFile",
            opts.ts_build_info_file.expect("tsBuildInfoFile present"),
        ),
    ] {
        assert!(
            value.starts_with(&expected_base),
            "Inherited {name} must anchor at the base config's directory, got {value:?}"
        );
        assert!(
            !value.starts_with(&unexpected_app),
            "Inherited {name} must not anchor at the child's directory: {value:?}"
        );
    }

    assert_eq!(type_roots.len(), 1);
    let type_root = &type_roots[0];
    assert!(
        type_root.starts_with(&expected_base),
        "Inherited typeRoots must anchor at the base config's directory, got {type_root:?}"
    );
    assert!(
        !type_root.starts_with(&unexpected_app),
        "Inherited typeRoots must not anchor at the child's directory: {type_root:?}"
    );
}

#[test]
fn test_child_base_url_overrides_inherited_and_anchors_at_child_dir() {
    // When the child config also declares `baseUrl`, the child wins
    // and is resolved relative to the child's directory (matching tsc).
    let temp = tempdir().expect("create temp dir");
    let base_dir = temp.path().join("base");
    let app_dir = temp.path().join("app");
    std::fs::create_dir_all(&base_dir).expect("create base dir");
    std::fs::create_dir_all(&app_dir).expect("create app dir");

    let base_path = base_dir.join("tsconfig.base.json");
    std::fs::write(&base_path, r#"{ "compilerOptions": { "baseUrl": "." } }"#).expect("write base");

    let child_path = app_dir.join("tsconfig.json");
    std::fs::write(
        &child_path,
        r#"{
"extends": "../base/tsconfig.base.json",
"compilerOptions": { "baseUrl": "src" }
}"#,
    )
    .expect("write child");

    let merged = load_tsconfig(&child_path).expect("load child");
    let opts = merged.compiler_options.expect("compiler options merged");
    let base_url = opts.base_url.expect("baseUrl present");

    // Canonicalize both sides so symlink-bearing temp paths on macOS
    // (`/var/folders/...` → `/private/var/folders/...`) compare equal.
    let canonical_app_dir = std::fs::canonicalize(&app_dir).unwrap_or(app_dir);
    let canonical_base_url = std::path::Path::new(&base_url)
        .canonicalize()
        .unwrap_or_else(|_| std::path::PathBuf::from(&base_url));
    let expected_prefix = canonical_app_dir.to_string_lossy();
    let actual = canonical_base_url.to_string_lossy();
    assert!(
        actual.starts_with(expected_prefix.as_ref()),
        "Child-declared baseUrl must anchor at the child's directory \
         (expected prefix {expected_prefix:?}, got {actual:?})"
    );
}

#[test]
fn test_inherited_absolute_base_url_is_preserved() {
    // An absolute `baseUrl` declared in the base config must propagate
    // unchanged through `extends`.
    let temp = tempdir().expect("create temp dir");
    let base_dir = temp.path().join("base");
    let app_dir = temp.path().join("app");
    let abs_base_url = temp.path().join("shared-root");
    std::fs::create_dir_all(&base_dir).expect("create base dir");
    std::fs::create_dir_all(&app_dir).expect("create app dir");
    std::fs::create_dir_all(&abs_base_url).expect("create shared root");

    let abs_str = abs_base_url.to_string_lossy().replace('\\', "/");
    let base_path = base_dir.join("tsconfig.base.json");
    std::fs::write(
        &base_path,
        format!(r#"{{ "compilerOptions": {{ "baseUrl": "{abs_str}" }} }}"#),
    )
    .expect("write base");

    let child_path = app_dir.join("tsconfig.json");
    std::fs::write(
        &child_path,
        r#"{ "extends": "../base/tsconfig.base.json" }"#,
    )
    .expect("write child");

    let merged = load_tsconfig(&child_path).expect("load child");
    let base_url = merged
        .compiler_options
        .expect("compiler options merged")
        .base_url
        .expect("baseUrl present");
    assert_eq!(
        std::path::Path::new(&base_url),
        abs_base_url.as_path(),
        "Absolute inherited baseUrl must be preserved verbatim"
    );
}

#[test]
fn test_ts5102_not_emitted_for_valid_option() {
    let source = r#"{"compilerOptions":{"strict":true}}"#;
    let parsed = parse_tsconfig_with_diagnostics(source, "tsconfig.json").unwrap();
    let codes: Vec<u32> = parsed.diagnostics.iter().map(|d| d.code).collect();
    assert!(
        !codes.contains(&5102),
        "Should NOT emit TS5102 for valid option 'strict', got: {codes:?}"
    );
}

#[test]
fn test_ts5095_not_emitted_for_bundler_with_commonjs() {
    // TypeScript 7 allows moduleResolution: bundler with module: commonjs.
    let source = r#"{"compilerOptions":{"module":"commonjs","moduleResolution":"bundler"}}"#;
    let parsed = parse_tsconfig_with_diagnostics(source, "tsconfig.json").unwrap();
    let codes: Vec<u32> = parsed.diagnostics.iter().map(|d| d.code).collect();
    assert!(
        !codes.contains(&5095),
        "Should NOT emit TS5095 for bundler+commonjs, got: {codes:?}"
    );
}

#[test]
fn test_ts6046_bundler_with_none() {
    let source = r#"{"compilerOptions":{"module":"none","moduleResolution":"bundler"}}"#;
    let parsed = parse_tsconfig_with_diagnostics(source, "tsconfig.json").unwrap();
    let codes: Vec<u32> = parsed.diagnostics.iter().map(|d| d.code).collect();
    assert!(
        codes.contains(&6046),
        "Expected TS6046 for module=none, got: {codes:?}"
    );
}

#[test]
fn test_ts5095_bundler_with_amd() {
    let source = r#"{"compilerOptions":{"module":"amd","moduleResolution":"bundler"}}"#;
    let parsed = parse_tsconfig_with_diagnostics(source, "tsconfig.json").unwrap();
    let codes: Vec<u32> = parsed.diagnostics.iter().map(|d| d.code).collect();
    assert!(
        codes.contains(&5095),
        "Expected TS5095 for bundler+amd, got: {codes:?}"
    );
}

#[test]
fn test_ts5095_bundler_with_system() {
    let source = r#"{"compilerOptions":{"module":"system","moduleResolution":"bundler"}}"#;
    let parsed = parse_tsconfig_with_diagnostics(source, "tsconfig.json").unwrap();
    let codes: Vec<u32> = parsed.diagnostics.iter().map(|d| d.code).collect();
    assert!(
        codes.contains(&5095),
        "Expected TS5095 for bundler+system, got: {codes:?}"
    );
}

#[test]
fn test_ts5095_not_emitted_for_bundler_with_es2015() {
    let source = r#"{"compilerOptions":{"module":"es2015","moduleResolution":"bundler"}}"#;
    let parsed = parse_tsconfig_with_diagnostics(source, "tsconfig.json").unwrap();
    let codes: Vec<u32> = parsed.diagnostics.iter().map(|d| d.code).collect();
    assert!(
        !codes.contains(&5095),
        "Should NOT emit TS5095 for bundler+es2015, got: {codes:?}"
    );
}

#[test]
fn test_ts5095_not_emitted_for_bundler_with_esnext() {
    let source = r#"{"compilerOptions":{"module":"esnext","moduleResolution":"bundler"}}"#;
    let parsed = parse_tsconfig_with_diagnostics(source, "tsconfig.json").unwrap();
    let codes: Vec<u32> = parsed.diagnostics.iter().map(|d| d.code).collect();
    assert!(
        !codes.contains(&5095),
        "Should NOT emit TS5095 for bundler+esnext, got: {codes:?}"
    );
}

#[test]
fn test_ts5095_not_emitted_for_bundler_with_preserve() {
    let source = r#"{"compilerOptions":{"module":"preserve","moduleResolution":"bundler"}}"#;
    let parsed = parse_tsconfig_with_diagnostics(source, "tsconfig.json").unwrap();
    let codes: Vec<u32> = parsed.diagnostics.iter().map(|d| d.code).collect();
    assert!(
        !codes.contains(&5095),
        "Should NOT emit TS5095 for bundler+preserve, got: {codes:?}"
    );
}

#[test]
fn test_ts5095_emitted_for_bundler_with_node16() {
    let source = r#"{"compilerOptions":{"module":"node16","moduleResolution":"bundler"}}"#;
    let parsed = parse_tsconfig_with_diagnostics(source, "tsconfig.json").unwrap();
    let codes: Vec<u32> = parsed.diagnostics.iter().map(|d| d.code).collect();
    assert!(
        codes.contains(&5095),
        "Should emit TS5095 for bundler+node16 (tsc behavior), got: {codes:?}"
    );
}

#[test]
fn test_ts5095_emitted_for_bundler_with_node18() {
    let source = r#"{"compilerOptions":{"module":"node18","moduleResolution":"bundler"}}"#;
    let parsed = parse_tsconfig_with_diagnostics(source, "tsconfig.json").unwrap();
    let codes: Vec<u32> = parsed.diagnostics.iter().map(|d| d.code).collect();
    assert!(
        codes.contains(&5095),
        "Should emit TS5095 for bundler+node18 (tsc behavior), got: {codes:?}"
    );
}

#[test]
fn test_ts5095_emitted_for_bundler_with_nodenext() {
    let source = r#"{"compilerOptions":{"module":"nodenext","moduleResolution":"bundler"}}"#;
    let parsed = parse_tsconfig_with_diagnostics(source, "tsconfig.json").unwrap();
    let codes: Vec<u32> = parsed.diagnostics.iter().map(|d| d.code).collect();
    assert!(
        codes.contains(&5095),
        "Should emit TS5095 for bundler+nodenext (tsc behavior), got: {codes:?}"
    );
}

#[test]
fn test_ts5095_not_emitted_for_node16_resolution() {
    let source = r#"{"compilerOptions":{"module":"commonjs","moduleResolution":"node16"}}"#;
    let parsed = parse_tsconfig_with_diagnostics(source, "tsconfig.json").unwrap();
    let codes: Vec<u32> = parsed.diagnostics.iter().map(|d| d.code).collect();
    assert!(
        !codes.contains(&5095),
        "Should NOT emit TS5095 for node16 resolution, got: {codes:?}"
    );
}

/// TypeScript 7 performs no `ignoreDeprecations` VALUE validation at all
/// (#16228): the option is parsed and type-checked as a string, but no value
/// is rejected. Probed on the pinned 7.0.2 oracle — version-shaped values,
/// non-version words, and the empty string are all silently accepted.
#[test]
fn test_ts5103_never_emitted_for_any_value() {
    for value in [
        "8.0", "5.5", "5.1", "4.9", "banana", "", "0", "7.0.2", "next",
    ] {
        let source = format!(r#"{{"compilerOptions":{{"ignoreDeprecations":"{value}"}}}}"#);
        let parsed = parse_tsconfig_with_diagnostics(&source, "tsconfig.json").unwrap();
        let codes: Vec<u32> = parsed.diagnostics.iter().map(|d| d.code).collect();
        assert!(
            !codes.contains(&5103),
            "TS5103 has no trigger on the 7.0.2 oracle; \
             ignoreDeprecations={value:?} produced: {codes:?}"
        );
    }
}

/// The one validation TypeScript 7 DOES still apply to `ignoreDeprecations` is
/// the generic option-type check. Negative control for the test above: dropping
/// TS5103 must not take TS5024 with it.
#[test]
fn test_ts5024_still_fires_for_non_string_ignore_deprecations() {
    let source = r#"{"compilerOptions":{"ignoreDeprecations":8}}"#;
    let parsed = parse_tsconfig_with_diagnostics(source, "tsconfig.json").unwrap();
    let codes: Vec<u32> = parsed.diagnostics.iter().map(|d| d.code).collect();
    assert!(
        codes.contains(&5024),
        "Expected TS5024 for a numeric ignoreDeprecations, got: {codes:?}"
    );
    assert!(
        !codes.contains(&5103),
        "A type-invalid value must not resurrect TS5103, got: {codes:?}"
    );
}

#[test]
fn test_ts5103_not_emitted_for_valid_7_0() {
    // tsc 7.0.2 accepts its own version literal alongside the two historical
    // ones (#16217); it does not suppress TS2880 (or anything else), but it
    // is not an *invalid* value either.
    let source = r#"{"compilerOptions":{"ignoreDeprecations":"7.0"}}"#;
    let parsed = parse_tsconfig_with_diagnostics(source, "tsconfig.json").unwrap();
    let codes: Vec<u32> = parsed.diagnostics.iter().map(|d| d.code).collect();
    assert!(
        !codes.contains(&5103),
        "Should NOT emit TS5103 for valid ignoreDeprecations='7.0', got: {codes:?}"
    );
}

/// An unrecognized `ignoreDeprecations` value does not become reportable by
/// sitting next to something tsc DOES complain about. Each row below is a
/// distinct companion-diagnostic family, and on the 7.0.2 oracle each reports
/// its own code and nothing else: an unknown option (TS5023), a removed option
/// KEY (TS5102), a removed option VALUE (TS5108), and a clean non-deprecated
/// target (no companion at all).
#[test]
fn test_ts5103_never_emitted_alongside_a_companion_diagnostic() {
    for (companion, expected_companion_code) in [
        (r#""noImplicitUseStrict":true"#, Some(5023)),
        (r#""baseUrl":".""#, Some(5102)),
        (r#""moduleResolution":"node10""#, Some(5108)),
        (r#""target":"es2018""#, None),
    ] {
        let source = format!(r#"{{"compilerOptions":{{"ignoreDeprecations":"8.0",{companion}}}}}"#);
        let parsed = parse_tsconfig_with_diagnostics(&source, "tsconfig.json").unwrap();
        let codes: Vec<u32> = parsed.diagnostics.iter().map(|d| d.code).collect();
        assert!(
            !codes.contains(&5103),
            "TS5103 must stay off next to {companion}, got: {codes:?}"
        );
        if let Some(code) = expected_companion_code {
            assert!(
                codes.contains(&code),
                "Companion TS{code} must still fire for {companion}, got: {codes:?}"
            );
        }
    }
}

#[test]
fn test_ts5103_not_emitted_for_valid_value() {
    let source = r#"{"compilerOptions":{"ignoreDeprecations":"5.0"}}"#;
    let parsed = parse_tsconfig_with_diagnostics(source, "tsconfig.json").unwrap();
    let codes: Vec<u32> = parsed.diagnostics.iter().map(|d| d.code).collect();
    assert!(
        !codes.contains(&5103),
        "Should NOT emit TS5103 for valid ignoreDeprecations='5.0', got: {codes:?}"
    );
}

#[test]
fn test_ts5103_not_emitted_for_valid_6_0() {
    // TypeScript 7 accepts both historical ignoreDeprecations values.
    let source = r#"{"compilerOptions":{"ignoreDeprecations":"6.0"}}"#;
    let parsed = parse_tsconfig_with_diagnostics(source, "tsconfig.json").unwrap();
    let codes: Vec<u32> = parsed.diagnostics.iter().map(|d| d.code).collect();
    assert!(
        !codes.contains(&5103),
        "Should NOT emit TS5103 for valid ignoreDeprecations='6.0', got: {codes:?}"
    );
}

#[test]
fn test_ts5103_not_emitted_for_6_0_with_dropped_options() {
    // TypeScript 7 treats the dropped option as unknown while "6.0" remains a
    // valid ignoreDeprecations value; it does not suppress the removal.
    let source = r#"{"compilerOptions":{"ignoreDeprecations":"6.0","noImplicitUseStrict":true}}"#;
    let parsed = parse_tsconfig_with_diagnostics(source, "tsconfig.json").unwrap();
    let codes: Vec<u32> = parsed.diagnostics.iter().map(|d| d.code).collect();
    assert!(
        codes.contains(&5023),
        "Expected TS5023 for dropped option, got: {codes:?}"
    );
    assert!(
        !codes.contains(&5103),
        "Should NOT emit TS5103 — '6.0' is a valid ignoreDeprecations value, got: {codes:?}"
    );
}

/// An unrecognized value inherited through `extends` is no more reportable
/// than one written locally — the third path #16228 called out as untested.
#[test]
fn test_ts5103_never_emitted_for_inherited_value() {
    let source = r#"{"compilerOptions":{"ignoreDeprecations":"5.5"}}"#;
    let parsed = parse_tsconfig_with_diagnostics(source, "base.json").unwrap();
    let codes: Vec<u32> = parsed.diagnostics.iter().map(|d| d.code).collect();
    assert!(
        !codes.contains(&5103),
        "Should NOT emit TS5103 for ignoreDeprecations='5.5', got: {codes:?}"
    );
}

#[test]
fn test_ts5103_not_emitted_when_absent() {
    let source = r#"{"compilerOptions":{"strict":true}}"#;
    let parsed = parse_tsconfig_with_diagnostics(source, "tsconfig.json").unwrap();
    let codes: Vec<u32> = parsed.diagnostics.iter().map(|d| d.code).collect();
    assert!(
        !codes.contains(&5103),
        "Should NOT emit TS5103 when ignoreDeprecations is absent, got: {codes:?}"
    );
}

#[test]
fn test_ts5110_node16_resolution_with_commonjs_module() {
    let source = r#"{"compilerOptions":{"module":"commonjs","moduleResolution":"node16"}}"#;
    let parsed = parse_tsconfig_with_diagnostics(source, "tsconfig.json").unwrap();
    let codes: Vec<u32> = parsed.diagnostics.iter().map(|d| d.code).collect();
    assert!(
        codes.contains(&5110),
        "Should emit TS5110 for node16 resolution with commonjs module, got: {codes:?}"
    );
}

#[test]
fn test_ts5110_nodenext_resolution_with_es2022_module() {
    let source = r#"{"compilerOptions":{"module":"es2022","moduleResolution":"nodenext"}}"#;
    let parsed = parse_tsconfig_with_diagnostics(source, "tsconfig.json").unwrap();
    let codes: Vec<u32> = parsed.diagnostics.iter().map(|d| d.code).collect();
    assert!(
        codes.contains(&5110),
        "Should emit TS5110 for nodenext resolution with es2022 module, got: {codes:?}"
    );
}

#[test]
fn test_ts5110_not_emitted_for_matching_node16() {
    let source = r#"{"compilerOptions":{"module":"node16","moduleResolution":"node16"}}"#;
    let parsed = parse_tsconfig_with_diagnostics(source, "tsconfig.json").unwrap();
    let codes: Vec<u32> = parsed.diagnostics.iter().map(|d| d.code).collect();
    assert!(
        !codes.contains(&5110),
        "Should NOT emit TS5110 when module matches moduleResolution, got: {codes:?}"
    );
}

#[test]
fn test_ts5110_not_emitted_for_matching_nodenext() {
    let source = r#"{"compilerOptions":{"module":"nodenext","moduleResolution":"nodenext"}}"#;
    let parsed = parse_tsconfig_with_diagnostics(source, "tsconfig.json").unwrap();
    let codes: Vec<u32> = parsed.diagnostics.iter().map(|d| d.code).collect();
    assert!(
        !codes.contains(&5110),
        "Should NOT emit TS5110 when module matches moduleResolution, got: {codes:?}"
    );
}

#[test]
fn test_ts5069_emit_declaration_only_without_declaration() {
    let source = r#"{"compilerOptions":{"emitDeclarationOnly":true}}"#;
    let parsed = parse_tsconfig_with_diagnostics(source, "tsconfig.json").unwrap();
    let codes: Vec<u32> = parsed.diagnostics.iter().map(|d| d.code).collect();
    assert!(
        codes.contains(&5069),
        "Expected TS5069 for emitDeclarationOnly without declaration, got: {codes:?}"
    );
}

#[test]
fn test_ts5069_not_emitted_with_declaration() {
    let source = r#"{"compilerOptions":{"emitDeclarationOnly":true,"declaration":true}}"#;
    let parsed = parse_tsconfig_with_diagnostics(source, "tsconfig.json").unwrap();
    let codes: Vec<u32> = parsed.diagnostics.iter().map(|d| d.code).collect();
    assert!(
        !codes.contains(&5069),
        "Should NOT emit TS5069 when declaration is true, got: {codes:?}"
    );
}

#[test]
fn test_ts5069_not_emitted_with_composite() {
    let source = r#"{"compilerOptions":{"emitDeclarationOnly":true,"composite":true}}"#;
    let parsed = parse_tsconfig_with_diagnostics(source, "tsconfig.json").unwrap();
    let codes: Vec<u32> = parsed.diagnostics.iter().map(|d| d.code).collect();
    assert!(
        !codes.contains(&5069),
        "Should NOT emit TS5069 when composite is true, got: {codes:?}"
    );
}

#[test]
fn test_ts5069_emitted_when_declaration_has_string_boolean() {
    let source = r#"{
  "compilerOptions": {
"declaration": "true",
"emitDeclarationOnly": true
  }
}"#;
    let parsed = parse_tsconfig_with_diagnostics(source, "tsconfig.json").unwrap();
    let ts5024_count = parsed.diagnostics.iter().filter(|d| d.code == 5024).count();
    assert_eq!(
        ts5024_count, 1,
        "Expected TS5024 for string-typed declaration"
    );

    let mut ts5069_starts: Vec<u32> = parsed
        .diagnostics
        .iter()
        .filter(|d| d.code == 5069)
        .map(|d| d.start)
        .collect();
    ts5069_starts.sort_unstable();
    assert_eq!(
        ts5069_starts.len(),
        2,
        "Expected TS5069 at both declaration and emitDeclarationOnly"
    );
    assert_eq!(
        ts5069_starts,
        vec![
            find_key_offset_in_source(source, "declaration"),
            find_key_offset_in_source(source, "emitDeclarationOnly"),
        ]
    );
}

#[test]
fn test_ts5069_declaration_map_without_declaration() {
    let source = r#"{"compilerOptions":{"declarationMap":true}}"#;
    let parsed = parse_tsconfig_with_diagnostics(source, "tsconfig.json").unwrap();
    let codes: Vec<u32> = parsed.diagnostics.iter().map(|d| d.code).collect();
    assert!(
        codes.contains(&5069),
        "Expected TS5069 for declarationMap without declaration, got: {codes:?}"
    );
}

#[test]
fn test_ts5053_sourcemap_with_inline_sourcemap() {
    let source = r#"{"compilerOptions":{"sourceMap":true,"inlineSourceMap":true}}"#;
    let parsed = parse_tsconfig_with_diagnostics(source, "tsconfig.json").unwrap();
    let codes: Vec<u32> = parsed.diagnostics.iter().map(|d| d.code).collect();
    assert!(
        codes.contains(&5053),
        "Expected TS5053 for sourceMap with inlineSourceMap, got: {codes:?}"
    );
    // One diagnostic, anchored at whichever key comes first in the source —
    // here `sourceMap`, even though the harness-generated configs in the
    // conformance corpus sort `inlineSourceMap` above it and anchor there.
    let hits: Vec<&Diagnostic> = parsed
        .diagnostics
        .iter()
        .filter(|d| d.code == 5053)
        .collect();
    assert_eq!(
        hits.len(),
        1,
        "Expected exactly one TS5053 diagnostic, got: {:?}",
        parsed.diagnostics
    );
    assert_eq!(
        hits[0].start as usize,
        source.find(r#""sourceMap""#).unwrap(),
        "TS5053 should anchor at the earlier key, got: {:?}",
        hits[0]
    );
}

#[test]
fn test_ts5053_anchors_at_second_named_option_when_it_comes_first() {
    // The message names `sourceMap` first but `inlineSourceMap` is written
    // first, so that is where tsc anchors. Pins the corpus oracle for
    // compiler/optionsInlineSourceMapSourcemap.ts.
    let source = r#"{"compilerOptions":{"inlineSourceMap":true,"sourceMap":true}}"#;
    let parsed = parse_tsconfig_with_diagnostics(source, "tsconfig.json").unwrap();
    let hits: Vec<&Diagnostic> = parsed
        .diagnostics
        .iter()
        .filter(|d| d.code == 5053)
        .collect();
    assert_eq!(hits.len(), 1, "got: {:?}", parsed.diagnostics);
    assert_eq!(
        hits[0].start as usize,
        source.find(r#""inlineSourceMap""#).unwrap(),
        "TS5053 should anchor at inlineSourceMap, got: {:?}",
        hits[0]
    );
}

#[test]
fn test_ts5091_reports_once_anchored_at_first_key() {
    // preserveConstEnums: false with isolatedModules on. `isolatedModules` is
    // written first, so it takes the anchor even though the message is about
    // `preserveConstEnums`. Pins compiler/isolatedModulesRequiresPreserveConstEnum.ts.
    let source = r#"{"compilerOptions":{"isolatedModules":true,"preserveConstEnums":false}}"#;
    let parsed = parse_tsconfig_with_diagnostics(source, "tsconfig.json").unwrap();
    let hits: Vec<&Diagnostic> = parsed
        .diagnostics
        .iter()
        .filter(|d| d.code == 5091)
        .collect();
    assert_eq!(hits.len(), 1, "got: {:?}", parsed.diagnostics);
    assert_eq!(
        hits[0].start as usize,
        source.find(r#""isolatedModules""#).unwrap(),
        "TS5091 should anchor at isolatedModules, got: {:?}",
        hits[0]
    );
}

#[test]
fn test_ts5053_not_emitted_without_conflict() {
    let source = r#"{"compilerOptions":{"sourceMap":true}}"#;
    let parsed = parse_tsconfig_with_diagnostics(source, "tsconfig.json").unwrap();
    let codes: Vec<u32> = parsed.diagnostics.iter().map(|d| d.code).collect();
    assert!(
        !codes.contains(&5053),
        "Should NOT emit TS5053 for sourceMap alone, got: {codes:?}"
    );
}

#[test]
fn test_ts5053_allow_js_with_isolated_declarations() {
    let source =
        r#"{"compilerOptions":{"allowJs":true,"isolatedDeclarations":true,"declaration":true}}"#;
    let parsed = parse_tsconfig_with_diagnostics(source, "tsconfig.json").unwrap();
    let codes: Vec<u32> = parsed.diagnostics.iter().map(|d| d.code).collect();
    assert!(
        codes.contains(&5053),
        "Expected TS5053 for allowJs with isolatedDeclarations, got: {codes:?}"
    );
}

// Issue #3732: when checkJs is true and allowJs is absent, tsc treats
// allowJs as implied-true and still emits TS5053 for the
// (allowJs, isolatedDeclarations) conflict.
#[test]
fn test_ts5053_check_js_implies_allow_js_with_isolated_declarations() {
    let source =
        r#"{"compilerOptions":{"checkJs":true,"isolatedDeclarations":true,"declaration":true}}"#;
    let parsed = parse_tsconfig_with_diagnostics(source, "tsconfig.json").unwrap();
    let codes: Vec<u32> = parsed.diagnostics.iter().map(|d| d.code).collect();
    assert!(
        codes.contains(&5053),
        "Expected TS5053 when checkJs implies allowJs alongside isolatedDeclarations, got: {codes:?}"
    );
    // The conflict message should still reference allowJs (the option
    // tsc reports as conflicting), even though the diagnostic anchors
    // at checkJs.
    let ts5053: Vec<_> = parsed
        .diagnostics
        .iter()
        .filter(|d| d.code == 5053)
        .collect();
    assert!(
        ts5053.iter().any(|d| d.message_text.contains("'allowJs'")),
        "Expected TS5053 message to reference allowJs, got: {ts5053:?}"
    );
}

// Sanity: explicit `allowJs: false` must not implicitly enable allowJs
// through checkJs, so TS5053 must NOT fire.
#[test]
fn test_ts5053_check_js_with_explicit_allow_js_false_does_not_fire() {
    let source = r#"{"compilerOptions":{"checkJs":true,"allowJs":false,"isolatedDeclarations":true,"declaration":true}}"#;
    let parsed = parse_tsconfig_with_diagnostics(source, "tsconfig.json").unwrap();
    let codes: Vec<u32> = parsed.diagnostics.iter().map(|d| d.code).collect();
    assert!(
        !codes.contains(&5053),
        "Should not emit TS5053 when allowJs is explicitly false, got: {codes:?}"
    );
}

#[test]
fn test_ts5052_not_emitted_when_check_js_implies_allow_js() {
    let source = r#"{"compilerOptions":{"checkJs":true}}"#;
    let parsed = parse_tsconfig_with_diagnostics(source, "tsconfig.json").unwrap();
    let has_5052 = parsed.diagnostics.iter().any(|d| d.code == 5052);
    assert!(
        !has_5052,
        "Should not emit TS5052 when checkJs implies allowJs, got: {:?}",
        parsed.diagnostics
    );
}

#[test]
fn test_ts5052_check_js_with_allow_js_false_anchors_at_first_key_in_source_order() {
    // tsc reports one TS5052 and anchors it at whichever of the two named
    // options it reaches first while walking the compilerOptions object, so
    // `allowJs` listed above `checkJs` takes the anchor even though the rule
    // is stated about `checkJs`. The conformance oracle for
    // compiler/checkJsFiles6.ts pins exactly this (single diagnostic, anchored
    // at the `allowJs` line).
    let source = r#"{"compilerOptions":{"allowJs":false,"checkJs":true}}"#;
    let parsed = parse_tsconfig_with_diagnostics(source, "tsconfig.json").unwrap();
    let hits: Vec<&Diagnostic> = parsed
        .diagnostics
        .iter()
        .filter(|d| d.code == 5052)
        .collect();
    assert_eq!(
        hits.len(),
        1,
        "Expected exactly one TS5052 diagnostic, got: {:?}",
        parsed.diagnostics
    );
    assert_eq!(
        hits[0].start as usize,
        source.find(r#""allowJs""#).unwrap(),
        "TS5052 should anchor at the earlier key (allowJs), got: {:?}",
        hits[0]
    );
}

#[test]
fn test_ts5052_check_js_with_invalid_allow_js_reports_once() {
    // An invalid `allowJs` value is still a property assignment in the object
    // literal, so it is still eligible to take the anchor — and there is still
    // only one diagnostic.
    let source = r#"{"compilerOptions":{"allowJs":"nope","checkJs":true}}"#;
    let parsed = parse_tsconfig_with_diagnostics(source, "tsconfig.json").unwrap();
    let hits: Vec<&Diagnostic> = parsed
        .diagnostics
        .iter()
        .filter(|d| d.code == 5052)
        .collect();
    assert_eq!(
        hits.len(),
        1,
        "Expected exactly one TS5052 diagnostic, got: {:?}",
        parsed.diagnostics
    );
}

#[test]
fn test_ts5052_strict_property_initialization_requires_strict_null_checks() {
    // `strict: true` fans out to strictPropertyInitialization, so an explicit
    // `strictNullChecks: false` leaves the dependent option on with its
    // prerequisite off — tsc's verifyCompilerOptions reports TS5052.
    let source = r#"{"compilerOptions":{"strict":true,"strictNullChecks":false,"strictPropertyInitialization":true}}"#;
    let parsed = parse_tsconfig_with_diagnostics(source, "tsconfig.json").unwrap();
    let msgs: Vec<&str> = parsed
        .diagnostics
        .iter()
        .filter(|d| d.code == 5052)
        .map(|d| d.message_text.as_str())
        .collect();
    assert_eq!(
        msgs,
        vec![
            "Option 'strictPropertyInitialization' cannot be specified without specifying option 'strictNullChecks'."
        ],
        "got: {:?}",
        parsed.diagnostics
    );
}

#[test]
fn test_ts5052_exact_optional_property_types_requires_strict_null_checks() {
    let source =
        r#"{"compilerOptions":{"exactOptionalPropertyTypes":true,"strictNullChecks":false}}"#;
    let parsed = parse_tsconfig_with_diagnostics(source, "tsconfig.json").unwrap();
    let msgs: Vec<&str> = parsed
        .diagnostics
        .iter()
        .filter(|d| d.code == 5052)
        .map(|d| d.message_text.as_str())
        .collect();
    assert_eq!(
        msgs,
        vec![
            "Option 'exactOptionalPropertyTypes' cannot be specified without specifying option 'strictNullChecks'."
        ],
        "got: {:?}",
        parsed.diagnostics
    );
}

#[test]
fn test_ts5052_strict_family_pair_silent_when_strict_umbrella_is_absent() {
    // TypeScript 7 defaults `strict` to true, so an absent `strictNullChecks`
    // inherits an *enabled* prerequisite and a lone strict-family member is
    // legal. Pins compiler/optionsStrictPropertyInitializationStrictNullChecks.ts,
    // whose oracle expects no diagnostics at all.
    for source in [
        r#"{"compilerOptions":{"strictPropertyInitialization":true,"target":"es2015"}}"#,
        r#"{"compilerOptions":{"exactOptionalPropertyTypes":true}}"#,
    ] {
        let parsed = parse_tsconfig_with_diagnostics(source, "tsconfig.json").unwrap();
        let has_5052 = parsed.diagnostics.iter().any(|d| d.code == 5052);
        assert!(
            !has_5052,
            "strict defaults to true in TS7, so no TS5052 is due for {source}, got: {:?}",
            parsed.diagnostics
        );
    }
}

#[test]
fn test_ts5052_strict_family_pair_fires_when_strict_umbrella_is_explicitly_off() {
    // The converse: an explicit `strict: false` really does leave
    // strictNullChecks off, so the dependency is unmet.
    let source = r#"{"compilerOptions":{"strict":false,"strictPropertyInitialization":true}}"#;
    let parsed = parse_tsconfig_with_diagnostics(source, "tsconfig.json").unwrap();
    let count = parsed.diagnostics.iter().filter(|d| d.code == 5052).count();
    assert_eq!(
        count, 1,
        "explicit strict: false leaves strictNullChecks off, got: {:?}",
        parsed.diagnostics
    );
}

#[test]
fn test_ts5052_strict_family_pair_silent_when_prerequisite_inherited_from_strict() {
    // strictNullChecks is absent, so it inherits `strict: true` and the
    // prerequisite is satisfied. Nothing to report.
    let source = r#"{"compilerOptions":{"strict":true,"strictPropertyInitialization":true,"exactOptionalPropertyTypes":true}}"#;
    let parsed = parse_tsconfig_with_diagnostics(source, "tsconfig.json").unwrap();
    let has_5052 = parsed.diagnostics.iter().any(|d| d.code == 5052);
    assert!(
        !has_5052,
        "strictNullChecks inherits strict: true, so no TS5052 is due, got: {:?}",
        parsed.diagnostics
    );
}

#[test]
fn test_ts5052_strict_family_pair_silent_when_dependent_option_absent() {
    // A bare `strict: true` never names strictPropertyInitialization, and tsc
    // tests the raw option there rather than the strict-aware value, so an
    // umbrella-only config with strictNullChecks off stays quiet.
    let source = r#"{"compilerOptions":{"strict":true,"strictNullChecks":false}}"#;
    let parsed = parse_tsconfig_with_diagnostics(source, "tsconfig.json").unwrap();
    let has_5052 = parsed.diagnostics.iter().any(|d| d.code == 5052);
    assert!(
        !has_5052,
        "no dependent option is specified, so no TS5052 is due, got: {:?}",
        parsed.diagnostics
    );
}

#[test]
fn test_ts5052_not_emitted_when_check_js_and_allow_js_true() {
    let source = r#"{"compilerOptions":{"allowJs":true,"checkJs":true}}"#;
    let parsed = parse_tsconfig_with_diagnostics(source, "tsconfig.json").unwrap();
    let has_5052 = parsed.diagnostics.iter().any(|d| d.code == 5052);
    assert!(
        !has_5052,
        "Should not emit TS5052 when allowJs is true, got: {:?}",
        parsed.diagnostics
    );
}

#[test]
fn test_ts5052_jsx_fragment_factory_requires_jsx_factory() {
    // Oracle-confirmed (typescript@7.0.2): `jsxFragmentFactory` alone, even
    // with a syntactically valid value, still requires `jsxFactory`.
    let source = r#"{"compilerOptions":{"jsxFragmentFactory":"Fragment"}}"#;
    let parsed = parse_tsconfig_with_diagnostics(source, "tsconfig.json").unwrap();
    let hits: Vec<&Diagnostic> = parsed
        .diagnostics
        .iter()
        .filter(|d| d.code == 5052)
        .collect();
    assert_eq!(
        hits.len(),
        1,
        "Expected exactly one TS5052 diagnostic, got: {:?}",
        parsed.diagnostics
    );
    assert!(
        hits[0].message_text.contains(
            "'jsxFragmentFactory' cannot be specified without specifying option 'jsxFactory'"
        ),
        "Unexpected TS5052 message: {}",
        hits[0].message_text
    );
}

#[test]
fn test_ts5052_jsx_fragment_factory_silent_when_jsx_factory_present() {
    let source = r#"{"compilerOptions":{"jsxFactory":"h","jsxFragmentFactory":"Fragment.Nested"}}"#;
    let parsed = parse_tsconfig_with_diagnostics(source, "tsconfig.json").unwrap();
    let has_5052 = parsed.diagnostics.iter().any(|d| d.code == 5052);
    assert!(
        !has_5052,
        "jsxFactory is present, so no TS5052 is due, got: {:?}",
        parsed.diagnostics
    );
}

#[test]
fn test_ts18035_invalid_jsx_fragment_factory_value() {
    // Oracle-confirmed (typescript@7.0.2): fires independently of the TS5052
    // dependency check -- an invalid value is still invalid even paired with
    // jsxFactory.
    let source = r#"{"compilerOptions":{"jsxFactory":"h","jsxFragmentFactory":"not a name!"}}"#;
    let parsed = parse_tsconfig_with_diagnostics(source, "tsconfig.json").unwrap();
    let ts18035 = parsed
        .diagnostics
        .iter()
        .find(|d| d.code == diagnostic_codes::INVALID_VALUE_FOR_JSXFRAGMENTFACTORY_IS_NOT_A_VALID_IDENTIFIER_OR_QUALIFIED_NAME)
        .unwrap_or_else(|| panic!("Expected TS18035, got: {:?}", parsed.diagnostics));
    assert!(
        ts18035
            .message_text
            .contains("'not a name!' is not a valid identifier or qualified-name"),
        "Unexpected TS18035 message: {}",
        ts18035.message_text
    );
    assert_eq!(
        ts18035.start,
        source.find("\"not a name!\"").expect("value position") as u32,
    );
    let has_5052 = parsed.diagnostics.iter().any(|d| d.code == 5052);
    assert!(
        !has_5052,
        "jsxFactory is present, so no TS5052 is due even though the value is invalid, got: {:?}",
        parsed.diagnostics
    );
}

#[test]
fn test_ts18035_not_emitted_for_valid_jsx_fragment_factory_value() {
    let source = r#"{"compilerOptions":{"jsxFactory":"h","jsxFragmentFactory":"Fragment.Nested"}}"#;
    let parsed = parse_tsconfig_with_diagnostics(source, "tsconfig.json").unwrap();
    let has_18035 = parsed.diagnostics.iter().any(|d| {
        d.code
            == diagnostic_codes::INVALID_VALUE_FOR_JSXFRAGMENTFACTORY_IS_NOT_A_VALID_IDENTIFIER_OR_QUALIFIED_NAME
    });
    assert!(
        !has_18035,
        "'Fragment.Nested' is a valid qualified name, got: {:?}",
        parsed.diagnostics
    );
}

#[test]
fn test_resolve_compiler_options_propagates_check_js_to_checker_options() {
    let source = r#"{"compilerOptions":{"allowJs":true,"checkJs":true}}"#;
    let parsed = parse_tsconfig_with_diagnostics(source, "tsconfig.json").unwrap();
    let resolved = resolve_compiler_options(parsed.config.compiler_options.as_ref()).unwrap();

    assert!(resolved.check_js);
    assert!(resolved.checker.check_js);
}

#[test]
fn test_resolve_compiler_options_check_js_implies_allow_js() {
    let source = r#"{"compilerOptions":{"checkJs":true}}"#;
    let parsed = parse_tsconfig_with_diagnostics(source, "tsconfig.json").unwrap();
    let resolved = resolve_compiler_options(parsed.config.compiler_options.as_ref()).unwrap();

    assert!(resolved.check_js);
    assert!(resolved.checker.check_js);
    assert!(resolved.allow_js);
    assert!(resolved.checker.allow_js);
}

#[test]
fn test_classic_module_resolution_is_rejected_as_removed() {
    // TS7 removed `moduleResolution: classic`; the TS5070 resolveJsonModule
    // conflict is never layered on top of the removal.
    let source = r#"{"compilerOptions":{"resolveJsonModule":true,"moduleResolution":"classic"}}"#;
    let parsed = parse_tsconfig_with_diagnostics(source, "tsconfig.json").unwrap();
    let codes: Vec<u32> = parsed.diagnostics.iter().map(|d| d.code).collect();
    assert!(
        codes.contains(&5108),
        "Expected TS5108 for removed classic moduleResolution, got: {codes:?}"
    );
    assert!(
        !codes.contains(&5070) && !codes.contains(&5071),
        "TS5070/TS5071 are unreachable in TS7, got: {codes:?}"
    );
}

#[test]
fn test_resolve_json_module_not_implied_by_node_resolution() {
    for source in [
        r#"{"compilerOptions":{"module":"commonjs","moduleResolution":"node10"}}"#,
        r#"{"compilerOptions":{"module":"node16","moduleResolution":"node16"}}"#,
    ] {
        let parsed = parse_tsconfig_with_diagnostics(source, "tsconfig.json").unwrap();
        let resolved = resolve_compiler_options(parsed.config.compiler_options.as_ref()).unwrap();

        assert!(
            !resolved.resolve_json_module,
            "resolveJsonModule should not be implied for {source}"
        );
        assert!(
            !resolved.checker.resolve_json_module,
            "checker resolveJsonModule should not be implied for {source}"
        );
    }
}

#[test]
fn test_resolve_json_module_implied_by_bundler_resolution() {
    let source = r#"{"compilerOptions":{"moduleResolution":"bundler"}}"#;
    let parsed = parse_tsconfig_with_diagnostics(source, "tsconfig.json").unwrap();
    let resolved = resolve_compiler_options(parsed.config.compiler_options.as_ref()).unwrap();

    assert!(resolved.resolve_json_module);
    assert!(resolved.checker.resolve_json_module);
}

#[test]
fn test_amd_module_reports_removal_and_default_bundler_conflict() {
    // TS7 removed `module: amd` (TS5108) and its default
    // `moduleResolution: bundler` is incompatible with AMD (TS5095); the old
    // TS5070 classic-resolution conflict is gone.
    let source = r#"{"compilerOptions":{"resolveJsonModule":true,"module":"amd"}}"#;
    let parsed = parse_tsconfig_with_diagnostics(source, "tsconfig.json").unwrap();
    let codes: Vec<u32> = parsed.diagnostics.iter().map(|d| d.code).collect();
    assert!(
        codes.contains(&5108) && codes.contains(&5095),
        "Expected TS5108 + TS5095 for module=amd, got: {codes:?}"
    );
    assert!(
        !codes.contains(&5070) && !codes.contains(&5071),
        "TS5070/TS5071 are unreachable in TS7, got: {codes:?}"
    );
}

#[test]
fn test_system_module_reports_removal_and_default_bundler_conflict() {
    // module=system without explicit moduleResolution: TS5108 for the
    // removed module kind plus TS5095 for the defaulted bundler resolution.
    let source = r#"{"compilerOptions":{"resolveJsonModule":true,"module":"system"}}"#;
    let parsed = parse_tsconfig_with_diagnostics(source, "tsconfig.json").unwrap();
    let codes: Vec<u32> = parsed.diagnostics.iter().map(|d| d.code).collect();
    assert!(
        codes.contains(&5108) && codes.contains(&5095),
        "Expected TS5108 + TS5095 for module=system, got: {codes:?}"
    );
    assert!(
        !codes.contains(&5070) && !codes.contains(&5071),
        "TS5070/TS5071 are unreachable in TS7, got: {codes:?}"
    );
}

#[test]
fn test_system_module_with_explicit_bundler_resolution_reports_both() {
    // Explicit `moduleResolution: bundler` anchors TS5095 at the option
    // value instead of the "compilerOptions" key; the outcome is the same
    // TS5108 + TS5095 pair, never TS5071.
    let source = r#"{"compilerOptions":{"resolveJsonModule":true,"module":"system","moduleResolution":"bundler"}}"#;
    let parsed = parse_tsconfig_with_diagnostics(source, "tsconfig.json").unwrap();
    let codes: Vec<u32> = parsed.diagnostics.iter().map(|d| d.code).collect();
    assert!(
        codes.contains(&5108) && codes.contains(&5095),
        "Expected TS5108 + TS5095 for module=system + explicit bundler, got: {codes:?}"
    );
    assert!(
        !codes.contains(&5071),
        "TS5071 is unreachable in TS7, got: {codes:?}"
    );
}

#[test]
fn test_ts6046_resolve_json_module_with_none_module() {
    let source = r#"{"compilerOptions":{"resolveJsonModule":true,"module":"none"}}"#;
    let parsed = parse_tsconfig_with_diagnostics(source, "tsconfig.json").unwrap();
    let codes: Vec<u32> = parsed.diagnostics.iter().map(|d| d.code).collect();
    assert!(
        codes.contains(&6046),
        "Expected TS6046 for module=none, got: {codes:?}"
    );
    assert!(
        !codes.contains(&5070) && !codes.contains(&5071),
        "Invalid module value must stop later resolveJsonModule checks, got: {codes:?}"
    );
}

#[test]
fn test_ts5098_resolve_package_json_with_classic() {
    let source =
        r#"{"compilerOptions":{"resolvePackageJsonExports":true,"moduleResolution":"classic"}}"#;
    let parsed = parse_tsconfig_with_diagnostics(source, "tsconfig.json").unwrap();
    let codes: Vec<u32> = parsed.diagnostics.iter().map(|d| d.code).collect();
    assert!(
        codes.contains(&5098),
        "Expected TS5098 for resolvePackageJsonExports with classic moduleResolution, got: {codes:?}"
    );
}

#[test]
fn test_ts5098_not_emitted_with_bundler() {
    let source =
        r#"{"compilerOptions":{"resolvePackageJsonExports":true,"moduleResolution":"bundler"}}"#;
    let parsed = parse_tsconfig_with_diagnostics(source, "tsconfig.json").unwrap();
    let codes: Vec<u32> = parsed.diagnostics.iter().map(|d| d.code).collect();
    assert!(
        !codes.contains(&5098),
        "Should NOT emit TS5098 with bundler moduleResolution, got: {codes:?}"
    );
}

#[test]
fn test_ts5098_not_emitted_when_module_and_resolution_omitted() {
    // #3509: tsz used to emit TS5098 for `customConditions` /
    // `resolvePackageJsonExports` / `resolvePackageJsonImports` when
    // both `module` and `moduleResolution` were unset, even though
    // tsz's own defaulting chain (target=ESNext → module=ESNext →
    // moduleResolution=Bundler) would land on a "modern" mode. tsc
    // accepts the same configs.
    for opt in [
        "customConditions",
        "resolvePackageJsonExports",
        "resolvePackageJsonImports",
    ] {
        let source = if opt == "customConditions" {
            format!(r#"{{"compilerOptions":{{"{opt}":["x"]}},"files":["index.ts"]}}"#)
        } else {
            format!(r#"{{"compilerOptions":{{"{opt}":true}},"files":["index.ts"]}}"#)
        };
        let parsed = parse_tsconfig_with_diagnostics(&source, "tsconfig.json").unwrap();
        let codes: Vec<u32> = parsed.diagnostics.iter().map(|d| d.code).collect();
        assert!(
            !codes.contains(&5098),
            "must not emit TS5098 for {opt} when module/moduleResolution omitted, got {codes:?}"
        );
    }
}

#[test]
fn test_ts5098_emitted_with_explicit_classic() {
    // Explicit `moduleResolution: "classic"` must still trigger TS5098 —
    // user opted out of the modern defaulting chain.
    let source = r#"{"compilerOptions":{"customConditions":["x"],"moduleResolution":"classic"}}"#;
    let parsed = parse_tsconfig_with_diagnostics(source, "tsconfig.json").unwrap();
    let codes: Vec<u32> = parsed.diagnostics.iter().map(|d| d.code).collect();
    assert!(
        codes.contains(&5098),
        "explicit classic moduleResolution must still emit TS5098, got {codes:?}"
    );
}

#[test]
fn test_resolve_extends_path_uses_package_exports_mapping() {
    let temp = tempdir().unwrap();
    let project_dir = temp.path().join("project");
    let package_dir = project_dir.join("node_modules").join("pkg");
    let config_dir = package_dir.join("configs");

    std::fs::create_dir_all(&config_dir).unwrap();
    std::fs::write(project_dir.join("tsconfig.json"), "{}").unwrap();
    std::fs::write(
        package_dir.join("package.json"),
        r#"{
            "exports": {
                "./tsconfig.json": "./configs/tsconfig.base.json"
            }
        }"#,
    )
    .unwrap();
    let expected = config_dir.join("tsconfig.base.json");
    std::fs::write(&expected, "{}").unwrap();

    let resolved =
        resolve_extends_path(&project_dir.join("tsconfig.json"), "pkg/tsconfig.json").unwrap();

    assert_eq!(resolved, ExtendsResolution::Found(expected));
}

#[test]
fn extends_scoped_package_config_resolved_via_node_modules_from_nested_dir() {
    // A workspace-internal config (the directus / rocketchat / cal-com shape):
    // a nested app config `extends` a package-provided tsconfig that lives in
    // the repo-root `node_modules`. The base must be loaded through Node module
    // resolution (walking ancestors), not path-joined onto the config dir.
    let temp = tempdir().expect("create temp dir");
    let root = temp.path().join("repo");
    let pkg = root.join("node_modules").join("@scope").join("tsconfig");
    std::fs::create_dir_all(&pkg).expect("create package dir");
    std::fs::write(
        pkg.join("node22.json"),
        r#"{ "compilerOptions": { "strict": true, "target": "ES2022" } }"#,
    )
    .expect("write base");

    let app = root.join("apps").join("web");
    std::fs::create_dir_all(&app).expect("create app dir");
    let child_path = app.join("tsconfig.json");
    std::fs::write(
        &child_path,
        r#"{ "extends": "@scope/tsconfig/node22.json", "compilerOptions": { "strict": false } }"#,
    )
    .expect("write child");

    let parsed = load_tsconfig_with_diagnostics(&child_path).expect("load child");
    assert!(
        !parsed.diagnostics.iter().any(|d| d.code == 6053),
        "an installed package config must resolve, no TS6053: {:?}",
        parsed.diagnostics
    );
    let opts = parsed.config.compiler_options.expect("merged options");
    assert_eq!(
        opts.strict,
        Some(false),
        "child overrides the base's strict value"
    );
    assert_eq!(
        opts.target.as_deref(),
        Some("ES2022"),
        "the base config's options must be inherited"
    );
}

#[test]
fn extends_unresolved_package_emits_ts6053_and_keeps_local_options() {
    // The package providing the base config is not installed (the canary
    // clone-without-deps shape). tsc emits TS6053 anchored at the `extends`
    // specifier and keeps compiling with the local options; tsz must not abort
    // the whole config load.
    let temp = tempdir().expect("create temp dir");
    let project = temp.path().join("project");
    std::fs::create_dir_all(&project).expect("create project dir");
    let child_path = project.join("tsconfig.json");
    let child_source =
        r#"{ "extends": "@scope/pkg/file.json", "compilerOptions": { "strict": true } }"#;
    std::fs::write(&child_path, child_source).expect("write child");

    let parsed = load_tsconfig_with_diagnostics(&child_path).expect("load must succeed, not abort");

    let ts6053: Vec<&Diagnostic> = parsed
        .diagnostics
        .iter()
        .filter(|d| d.code == 6053)
        .collect();
    assert_eq!(
        ts6053.len(),
        1,
        "exactly one TS6053 for the unresolved extends: {:?}",
        parsed.diagnostics
    );
    assert!(
        ts6053[0].message_text.contains("@scope/pkg/file.json"),
        "TS6053 names the unresolved specifier: {}",
        ts6053[0].message_text
    );
    let expected_start = child_source
        .find("\"@scope/pkg/file.json\"")
        .expect("specifier present in source") as u32;
    assert_eq!(
        ts6053[0].start, expected_start,
        "TS6053 anchors at the extends specifier literal"
    );

    let opts = parsed
        .config
        .compiler_options
        .expect("local options retained");
    assert_eq!(
        opts.strict,
        Some(true),
        "local options survive an unresolved extends"
    );
}

#[test]
fn extends_array_reports_each_unresolved_entry() {
    // Array `extends` (TS 5.0): every unresolvable entry gets its own TS6053
    // (entries are extensionless; missing `.json` is TS5083, covered elsewhere).
    let temp = tempdir().expect("create temp dir");
    let project = temp.path().join("project");
    std::fs::create_dir_all(&project).expect("create project dir");
    std::fs::write(
        project.join("present.json"),
        r#"{ "compilerOptions": { "target": "ES2021" } }"#,
    )
    .expect("write present base");
    let child_path = project.join("tsconfig.json");
    std::fs::write(
        &child_path,
        r#"{ "extends": ["./present.json", "./missing-a", "./missing-b"] }"#,
    )
    .expect("write child");

    let parsed = load_tsconfig_with_diagnostics(&child_path).expect("load must succeed");
    let ts6053: Vec<&Diagnostic> = parsed
        .diagnostics
        .iter()
        .filter(|d| d.code == 6053)
        .collect();
    assert_eq!(
        ts6053.len(),
        2,
        "one TS6053 per unresolved array entry: {:?}",
        parsed.diagnostics
    );
    let opts = parsed.config.compiler_options.expect("present base merged");
    assert_eq!(
        opts.target.as_deref(),
        Some("ES2021"),
        "the resolvable array entry is still applied"
    );
}

#[test]
fn config_dir_template_expands_against_leaf_config_dir() {
    // TS 5.5 `${configDir}`: a root config's template resolves to its own
    // directory and produces absolute selectors/paths.
    let temp = tempdir().expect("create temp dir");
    let project_dir = temp.path().join("project");
    std::fs::create_dir_all(project_dir.join("src")).expect("create src dir");

    let config_path = project_dir.join("tsconfig.json");
    std::fs::write(
        &config_path,
        r#"{
"compilerOptions": { "noEmit": true, "outDir": "${configDir}/dist" },
"include": ["${configDir}/src"]
}"#,
    )
    .expect("write config");

    let merged = load_tsconfig(&config_path).expect("load config");
    let canonical_project = std::fs::canonicalize(&project_dir).unwrap_or(project_dir);
    let expected_src = canonical_project.join("src").to_string_lossy().into_owned();
    let expected_dist = canonical_project
        .join("dist")
        .to_string_lossy()
        .into_owned();

    assert_eq!(
        merged.include.as_deref(),
        Some(&[expected_src][..]),
        "${{configDir}}/src must expand to the config's own directory"
    );
    assert_eq!(
        merged
            .compiler_options
            .as_ref()
            .and_then(|o| o.out_dir.as_deref()),
        Some(expected_dist.as_str()),
    );
}

#[test]
fn config_dir_template_in_base_resolves_to_inheriting_config_dir() {
    // The defining behavior of `${configDir}`: a shared base config can write
    // `${configDir}/...` and every consumer resolves it against the consumer's
    // (leaf) directory, NOT the base config's own directory.
    let temp = tempdir().expect("create temp dir");
    let base_dir = temp.path().join("shared");
    let app_dir = temp.path().join("app");
    std::fs::create_dir_all(app_dir.join("src")).expect("create app src");
    std::fs::create_dir_all(&base_dir).expect("create base dir");

    let base_path = base_dir.join("tsconfig.base.json");
    std::fs::write(
        &base_path,
        r#"{
"compilerOptions": { "outDir": "${configDir}/dist", "baseUrl": "${configDir}" },
"include": ["${configDir}/src"]
}"#,
    )
    .expect("write base");

    let child_path = app_dir.join("tsconfig.json");
    std::fs::write(
        &child_path,
        r#"{ "extends": "../shared/tsconfig.base.json", "compilerOptions": { "noEmit": true } }"#,
    )
    .expect("write child");

    let merged = load_tsconfig(&child_path).expect("load child");
    let canonical_app = std::fs::canonicalize(&app_dir).unwrap_or(app_dir);
    let canonical_base = std::fs::canonicalize(&base_dir).unwrap_or(base_dir);

    let include = merged.include.expect("inherited include present");
    assert_eq!(
        include[0],
        canonical_app.join("src").to_string_lossy(),
        "${{configDir}} in the base must resolve to the inheriting config's dir"
    );
    assert!(
        !include[0].starts_with(canonical_base.to_string_lossy().as_ref()),
        "${{configDir}} must not anchor at the base config's own directory: {:?}",
        include[0]
    );

    let opts = merged.compiler_options.expect("compiler options merged");
    assert_eq!(
        opts.out_dir.as_deref(),
        Some(canonical_app.join("dist").to_string_lossy().as_ref()),
    );
    assert_eq!(
        opts.base_url.as_deref(),
        Some(canonical_app.to_string_lossy().as_ref()),
    );
}

//! Module and module-resolution diagnostic tests
//! (TS6046 enum options, module + resolution defaults, TS5095 / TS5070 /
//! TS5071 / TS5098 `resolveJsonModule` and `package.json` resolution, TS5102 /
//! TS5103 removed and deprecated options, TS5110, inherited `extends`
//! anchoring).
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
    assert_eq!(
        default_module_kind_for_target(ScriptTarget::ES5, true),
        ModuleKind::CommonJS
    );
    assert_eq!(
        default_module_kind_for_target(ScriptTarget::ES2019, true),
        ModuleKind::ES2015
    );
    assert_eq!(
        default_module_kind_for_target(ScriptTarget::ES2021, true),
        ModuleKind::ES2020
    );
    assert_eq!(
        default_module_kind_for_target(ScriptTarget::ES2025, true),
        ModuleKind::ES2022
    );
    assert_eq!(
        default_module_kind_for_target(ScriptTarget::ES2025, false),
        ModuleKind::ESNext
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
    // tsc 6.0: ES module kinds default to Bundler resolution (was Classic)
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
fn test_removed_compiler_option_lookup() {
    assert!(removed_compiler_option("noImplicitUseStrict").is_some());
    assert!(removed_compiler_option("keyofStringsOnly").is_some());
    assert!(removed_compiler_option("suppressExcessPropertyErrors").is_some());
    assert!(removed_compiler_option("suppressImplicitAnyIndexErrors").is_some());
    assert!(removed_compiler_option("noStrictGenericChecks").is_some());
    assert!(removed_compiler_option("charset").is_some());
    assert!(removed_compiler_option("out").is_some());
    assert_eq!(
        removed_compiler_option("importsNotUsedAsValues"),
        Some("verbatimModuleSyntax")
    );
    assert_eq!(
        removed_compiler_option("preserveValueImports"),
        Some("verbatimModuleSyntax")
    );
    // Non-removed options return None
    assert!(removed_compiler_option("strict").is_none());
    assert!(removed_compiler_option("target").is_none());
}

#[test]
fn test_ts5102_emitted_for_removed_option() {
    let source = r#"{"compilerOptions":{"noImplicitUseStrict":true}}"#;
    let parsed = parse_tsconfig_with_diagnostics(source, "tsconfig.json").unwrap();
    let codes: Vec<u32> = parsed.diagnostics.iter().map(|d| d.code).collect();
    assert!(
        codes.contains(&5102),
        "Expected TS5102 for removed option noImplicitUseStrict, got: {codes:?}"
    );
}

#[test]
fn test_ts5102_not_emitted_for_false_removed_option() {
    // When a removed boolean option is set to false, tsc doesn't emit TS5102
    let source = r#"{"compilerOptions":{"noImplicitUseStrict":false}}"#;
    let parsed = parse_tsconfig_with_diagnostics(source, "tsconfig.json").unwrap();
    let codes: Vec<u32> = parsed.diagnostics.iter().map(|d| d.code).collect();
    assert!(
        !codes.contains(&5102),
        "Should NOT emit TS5102 for false-valued removed option, got: {codes:?}"
    );
}

#[test]
fn test_ts5102_emitted_for_string_removed_option() {
    let source = r#"{"compilerOptions":{"importsNotUsedAsValues":"error"}}"#;
    let parsed = parse_tsconfig_with_diagnostics(source, "tsconfig.json").unwrap();
    let codes: Vec<u32> = parsed.diagnostics.iter().map(|d| d.code).collect();
    assert!(
        codes.contains(&5102),
        "Expected TS5102 for removed option importsNotUsedAsValues, got: {codes:?}"
    );
}

#[test]
fn test_ts5102_not_suppressed_with_ignore_deprecations() {
    // In tsc 6.0, removed options (deprecated 5.0, removed 5.5) always emit TS5102
    // because mustBeRemoved is true (removedIn 5.5 <= tsc 6.0).
    // ignoreDeprecations only suppresses TS5101 (deprecated but not yet removed).
    let source = r#"{"compilerOptions":{"ignoreDeprecations":"5.0","noImplicitUseStrict":true}}"#;
    let parsed = parse_tsconfig_with_diagnostics(source, "tsconfig.json").unwrap();
    let codes: Vec<u32> = parsed.diagnostics.iter().map(|d| d.code).collect();
    assert!(
        codes.contains(&5102),
        "Should emit TS5102 even with ignoreDeprecations '5.0' (option is past removal), got: {codes:?}"
    );
}

#[test]
fn test_ts5102_not_suppressed_with_invalid_ignore_deprecations() {
    // Invalid ignoreDeprecations value should NOT suppress TS5102
    let source = r#"{"compilerOptions":{"ignoreDeprecations":"7.0","noImplicitUseStrict":true}}"#;
    let parsed = parse_tsconfig_with_diagnostics(source, "tsconfig.json").unwrap();
    let codes: Vec<u32> = parsed.diagnostics.iter().map(|d| d.code).collect();
    assert!(
        codes.contains(&5102),
        "Should emit TS5102 when ignoreDeprecations is invalid, got: {codes:?}"
    );
    assert!(
        codes.contains(&5103),
        "Should also emit TS5103 for invalid ignoreDeprecations, got: {codes:?}"
    );
}

#[test]
fn test_ts5102_fires_with_ignore_deprecations_6_0() {
    // "6.0" IS a valid ignoreDeprecations value in tsc 6.0.
    // TS5102 still fires for removed 5.0-wave options (past removal deadline).
    // TS5103 must NOT fire because "6.0" is valid.
    let source = r#"{"compilerOptions":{"ignoreDeprecations":"6.0","noImplicitUseStrict":true}}"#;
    let parsed = parse_tsconfig_with_diagnostics(source, "tsconfig.json").unwrap();
    let codes: Vec<u32> = parsed.diagnostics.iter().map(|d| d.code).collect();
    assert!(
        codes.contains(&5102),
        "Should emit TS5102 even with ignoreDeprecations '6.0' (option is past removal), got: {codes:?}"
    );
    assert!(
        !codes.contains(&5103),
        "Should NOT emit TS5103 — '6.0' is a valid ignoreDeprecations value, got: {codes:?}"
    );
}

#[test]
fn test_ts5102_fires_for_all_removed_options() {
    // Verify all removed options trigger TS5102 unconditionally
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
            codes.contains(&5102),
            "Should emit TS5102 for removed option '{opt}' even with ignoreDeprecations '6.0', got: {codes:?}"
        );
    }
}

#[test]
fn test_ts5102_inherited_from_extends_anchors_at_child_compiler_options_key() {
    // Repro from `verbatimModuleSyntaxCompat3.ts`. When the extending
    // tsconfig.json uses `verbatimModuleSyntax` and the base tsconfig
    // contains removed options (`preserveValueImports`,
    // `importsNotUsedAsValues`), tsc anchors TS5102 at the *child's*
    // `"compilerOptions"` key — not at `"verbatimModuleSyntax"` which
    // tsz used to incorrectly anchor on. Reproducing requires real
    // tempfiles because the inheritance resolution reads from disk.
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
    let ts5102: Vec<&Diagnostic> = parsed
        .diagnostics
        .iter()
        .filter(|d| d.code == 5102)
        .collect();
    assert!(
        ts5102.len() >= 2,
        "Expected at least 2 TS5102 (preserveValueImports + importsNotUsedAsValues), got: {ts5102:?}"
    );
    // Each TS5102 must anchor at the child's `"compilerOptions"` key.
    let expected_start = child_source
        .find("\"compilerOptions\"")
        .expect("compilerOptions in child source") as u32;
    for diag in &ts5102 {
        assert_eq!(
            diag.start, expected_start,
            "Inherited TS5102 must anchor at child's `\"compilerOptions\"` key (start={expected_start}), got: {diag:?}"
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
    // tsc 6.0 allows moduleResolution: bundler with module: commonjs
    let source = r#"{"compilerOptions":{"module":"commonjs","moduleResolution":"bundler"}}"#;
    let parsed = parse_tsconfig_with_diagnostics(source, "tsconfig.json").unwrap();
    let codes: Vec<u32> = parsed.diagnostics.iter().map(|d| d.code).collect();
    assert!(
        !codes.contains(&5095),
        "Should NOT emit TS5095 for bundler+commonjs, got: {codes:?}"
    );
}

#[test]
fn test_ts5095_bundler_with_none() {
    let source = r#"{"compilerOptions":{"module":"none","moduleResolution":"bundler"}}"#;
    let parsed = parse_tsconfig_with_diagnostics(source, "tsconfig.json").unwrap();
    let codes: Vec<u32> = parsed.diagnostics.iter().map(|d| d.code).collect();
    assert!(
        codes.contains(&5095),
        "Expected TS5095 for bundler+none, got: {codes:?}"
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

#[test]
fn test_ts5103_emitted_for_invalid_ignore_deprecations() {
    // tsz conservatively emits TS5103 whenever ignoreDeprecations is set to an invalid value.
    // tsc only emits TS5103 when deprecated features are also present, but since tsz cannot
    // detect all deprecated features (e.g. deprecated source syntax like import assertions),
    // it conservatively emits TS5103 for any invalid ignoreDeprecations value.
    let source = r#"{"compilerOptions":{"ignoreDeprecations":"7.0"}}"#;
    let parsed = parse_tsconfig_with_diagnostics(source, "tsconfig.json").unwrap();
    let codes: Vec<u32> = parsed.diagnostics.iter().map(|d| d.code).collect();
    assert!(
        codes.contains(&5103),
        "Expected TS5103 for invalid ignoreDeprecations='7.0', got: {codes:?}"
    );
}

#[test]
fn test_ts5103_emitted_for_invalid_ignore_deprecations_with_deprecated_option() {
    // tsc emits TS5103 when an invalid ignoreDeprecations value is used alongside
    // a removed/deprecated option (the invalid value can't suppress the warning).
    let source = r#"{"compilerOptions":{"ignoreDeprecations":"5.1","noImplicitUseStrict":true}}"#;
    let parsed = parse_tsconfig_with_diagnostics(source, "tsconfig.json").unwrap();
    let codes: Vec<u32> = parsed.diagnostics.iter().map(|d| d.code).collect();
    assert!(
        codes.contains(&5103),
        "Expected TS5103 for ignoreDeprecations='5.1' with deprecated option, got: {codes:?}"
    );
}

#[test]
fn test_ts5103_emitted_for_invalid_ignore_deprecations_with_deprecated_target_alias() {
    // tsc emits TS5103 when an invalid ignoreDeprecations value is used alongside
    // a deprecated target alias like "es6" (deprecated in favor of "es2015").
    // This matches the arrowFunction conformance test pattern.
    let source = r#"{"compilerOptions":{"ignoreDeprecations":"7.0","target":"es6"}}"#;
    let parsed = parse_tsconfig_with_diagnostics(source, "tsconfig.json").unwrap();
    let codes: Vec<u32> = parsed.diagnostics.iter().map(|d| d.code).collect();
    assert!(
        codes.contains(&5103),
        "Expected TS5103 for ignoreDeprecations='7.0' with deprecated target='es6', got: {codes:?}"
    );
}

#[test]
fn test_ts5103_emitted_for_invalid_ignore_deprecations_with_any_target() {
    // tsz emits TS5103 conservatively for any invalid ignoreDeprecations value,
    // regardless of target. Even non-deprecated targets like "es2018" will trigger
    // TS5103 in tsz (conservative approach since we can't detect all deprecated syntax).
    let source = r#"{"compilerOptions":{"ignoreDeprecations":"7.0","target":"es2018"}}"#;
    let parsed = parse_tsconfig_with_diagnostics(source, "tsconfig.json").unwrap();
    let codes: Vec<u32> = parsed.diagnostics.iter().map(|d| d.code).collect();
    assert!(
        codes.contains(&5103),
        "Expected TS5103 (conservative) for ignoreDeprecations='7.0' with target='es2018', got: {codes:?}"
    );
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
    // tsc 6.0 accepts both "5.0" and "6.0" as valid ignoreDeprecations values.
    // See TypeScript/src/compiler/program.ts getIgnoreDeprecationsVersion():
    //   if (ignoreDeprecations === "5.0" || ignoreDeprecations === "6.0") return new Version(...)
    let source = r#"{"compilerOptions":{"ignoreDeprecations":"6.0"}}"#;
    let parsed = parse_tsconfig_with_diagnostics(source, "tsconfig.json").unwrap();
    let codes: Vec<u32> = parsed.diagnostics.iter().map(|d| d.code).collect();
    assert!(
        !codes.contains(&5103),
        "Should NOT emit TS5103 for valid ignoreDeprecations='6.0', got: {codes:?}"
    );
}

#[test]
fn test_ts5103_not_emitted_for_6_0_with_deprecated_options() {
    // ignoreDeprecations: "6.0" silences 6.0-wave deprecation warnings.
    // TS5102 still fires for removed 5.0-wave options (noImplicitUseStrict is removed),
    // but TS5103 must NOT fire because "6.0" is a valid value.
    let source = r#"{"compilerOptions":{"ignoreDeprecations":"6.0","noImplicitUseStrict":true}}"#;
    let parsed = parse_tsconfig_with_diagnostics(source, "tsconfig.json").unwrap();
    let codes: Vec<u32> = parsed.diagnostics.iter().map(|d| d.code).collect();
    assert!(
        codes.contains(&5102),
        "Should still emit TS5102 for removed option, got: {codes:?}"
    );
    assert!(
        !codes.contains(&5103),
        "Should NOT emit TS5103 — '6.0' is a valid ignoreDeprecations value, got: {codes:?}"
    );
}

#[test]
fn test_ts5103_emitted_for_invalid_5_5() {
    // tsc 6.0 only accepts "5.0" — "5.5" is not a valid ignoreDeprecations value
    let source = r#"{"compilerOptions":{"ignoreDeprecations":"5.5"}}"#;
    let parsed = parse_tsconfig_with_diagnostics(source, "tsconfig.json").unwrap();
    let codes: Vec<u32> = parsed.diagnostics.iter().map(|d| d.code).collect();
    assert!(
        codes.contains(&5103),
        "Should emit TS5103 for invalid ignoreDeprecations='5.5', got: {codes:?}"
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
    // tsc emits twice (at each key position)
    let count = codes.iter().filter(|&&c| c == 5053).count();
    assert_eq!(
        count, 2,
        "Expected 2 TS5053 diagnostics (one per key), got: {count}"
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
fn test_ts5052_check_js_with_allow_js_false_reports_both_sites() {
    let source = r#"{"compilerOptions":{"allowJs":false,"checkJs":true}}"#;
    let parsed = parse_tsconfig_with_diagnostics(source, "tsconfig.json").unwrap();
    let count = parsed.diagnostics.iter().filter(|d| d.code == 5052).count();
    assert_eq!(
        count, 2,
        "Expected two TS5052 diagnostics (allowJs/checkJs), got: {:?}",
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
fn test_ts5070_resolve_json_module_with_classic_module_resolution() {
    let source = r#"{"compilerOptions":{"resolveJsonModule":true,"moduleResolution":"classic"}}"#;
    let parsed = parse_tsconfig_with_diagnostics(source, "tsconfig.json").unwrap();
    let codes: Vec<u32> = parsed.diagnostics.iter().map(|d| d.code).collect();
    assert!(
        codes.contains(&5070),
        "Expected TS5070 for resolveJsonModule with classic moduleResolution, got: {codes:?}"
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
fn test_ts5070_resolve_json_module_with_amd_module() {
    // module=amd defaults to moduleResolution=classic
    let source = r#"{"compilerOptions":{"resolveJsonModule":true,"module":"amd"}}"#;
    let parsed = parse_tsconfig_with_diagnostics(source, "tsconfig.json").unwrap();
    let codes: Vec<u32> = parsed.diagnostics.iter().map(|d| d.code).collect();
    assert!(
        codes.contains(&5070),
        "Expected TS5070 for resolveJsonModule with module=amd (implies classic), got: {codes:?}"
    );
}

#[test]
fn test_ts5071_resolve_json_module_with_system_module() {
    // module=system without explicit moduleResolution implies classic resolution →
    // tsc emits TS5070 (not TS5071) because the moduleResolution-based check takes precedence.
    let source = r#"{"compilerOptions":{"resolveJsonModule":true,"module":"system"}}"#;
    let parsed = parse_tsconfig_with_diagnostics(source, "tsconfig.json").unwrap();
    let codes: Vec<u32> = parsed.diagnostics.iter().map(|d| d.code).collect();
    assert!(
        codes.contains(&5070),
        "Expected TS5070 (not TS5071) for resolveJsonModule with module=system (implies classic), got: {codes:?}"
    );
    assert!(
        !codes.contains(&5071),
        "Should NOT emit TS5071 when effective moduleResolution is classic (TS5070 takes precedence), got: {codes:?}"
    );
}

#[test]
fn test_ts5071_resolve_json_module_with_system_module_explicit_resolution() {
    // module=system with explicit non-classic moduleResolution → TS5071 fires
    let source = r#"{"compilerOptions":{"resolveJsonModule":true,"module":"system","moduleResolution":"bundler"}}"#;
    let parsed = parse_tsconfig_with_diagnostics(source, "tsconfig.json").unwrap();
    let codes: Vec<u32> = parsed.diagnostics.iter().map(|d| d.code).collect();
    assert!(
        codes.contains(&5071),
        "Expected TS5071 for resolveJsonModule with module=system + moduleResolution=bundler, got: {codes:?}"
    );
}

#[test]
fn test_ts5071_resolve_json_module_with_none_module() {
    // module=none without explicit moduleResolution implies classic resolution →
    // tsc emits TS5070 (not TS5071) because the moduleResolution-based check takes precedence.
    let source = r#"{"compilerOptions":{"resolveJsonModule":true,"module":"none"}}"#;
    let parsed = parse_tsconfig_with_diagnostics(source, "tsconfig.json").unwrap();
    let codes: Vec<u32> = parsed.diagnostics.iter().map(|d| d.code).collect();
    assert!(
        codes.contains(&5070),
        "Expected TS5070 (not TS5071) for resolveJsonModule with module=none (implies classic), got: {codes:?}"
    );
    assert!(
        !codes.contains(&5071),
        "Should NOT emit TS5071 when effective moduleResolution is classic (TS5070 takes precedence), got: {codes:?}"
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

    assert_eq!(resolved, expected);
}

//! Emit, strict-family, lib, and `extends` diagnostic tests
//! (TS6082 `outFile`, TS5107 / TS5108 `alwaysStrict` + target, TS5101 / TS5090
//! `baseUrl` + paths, strict-family defaults, TS6304 / TS6379 composite, lib
//! references and JSONC handling, extends-base anchoring, `watchOptions`
//! TS6046).
//!
//! Split from `config/mod.rs` to keep each file under the 2000-line limit
//! (§19; ratchet tracked by #8280).

use super::super::*;
use tempfile::tempdir;

#[test]
fn test_ts6082_outfile_with_commonjs() {
    let source = r#"{"compilerOptions":{"module":"commonjs","outFile":"all.js"}}"#;
    let parsed = parse_tsconfig_with_diagnostics(source, "tsconfig.json").unwrap();
    let codes: Vec<u32> = parsed.diagnostics.iter().map(|d| d.code).collect();
    assert!(
        codes.contains(&6082),
        "Expected TS6082 for outFile+commonjs, got: {codes:?}"
    );
    // Should emit twice — once at "module" key, once at "outFile" key
    let count = codes.iter().filter(|&&c| c == 6082).count();
    assert_eq!(
        count, 2,
        "Expected two TS6082 diagnostics (module + outFile keys), got {count}"
    );
}

#[test]
fn test_ts6082_outfile_with_umd() {
    let source = r#"{"compilerOptions":{"module":"umd","outFile":"all.js"}}"#;
    let parsed = parse_tsconfig_with_diagnostics(source, "tsconfig.json").unwrap();
    let codes: Vec<u32> = parsed.diagnostics.iter().map(|d| d.code).collect();
    assert!(
        codes.contains(&6082),
        "Expected TS6082 for outFile+umd, got: {codes:?}"
    );
}

#[test]
fn test_ts6082_outfile_with_es6() {
    let source = r#"{"compilerOptions":{"module":"es6","outFile":"all.js"}}"#;
    let parsed = parse_tsconfig_with_diagnostics(source, "tsconfig.json").unwrap();
    let codes: Vec<u32> = parsed.diagnostics.iter().map(|d| d.code).collect();
    assert!(
        codes.contains(&6082),
        "Expected TS6082 for outFile+es6, got: {codes:?}"
    );
}

#[test]
fn test_ts6082_not_emitted_for_amd() {
    let source = r#"{"compilerOptions":{"module":"amd","outFile":"all.js"}}"#;
    let parsed = parse_tsconfig_with_diagnostics(source, "tsconfig.json").unwrap();
    let codes: Vec<u32> = parsed.diagnostics.iter().map(|d| d.code).collect();
    assert!(
        !codes.contains(&6082),
        "Should NOT emit TS6082 for outFile+amd, got: {codes:?}"
    );
}

#[test]
fn test_ts6082_not_emitted_for_system() {
    let source = r#"{"compilerOptions":{"module":"system","outFile":"all.js"}}"#;
    let parsed = parse_tsconfig_with_diagnostics(source, "tsconfig.json").unwrap();
    let codes: Vec<u32> = parsed.diagnostics.iter().map(|d| d.code).collect();
    assert!(
        !codes.contains(&6082),
        "Should NOT emit TS6082 for outFile+system, got: {codes:?}"
    );
}

#[test]
fn test_ts6082_not_emitted_with_emit_declaration_only() {
    let source = r#"{"compilerOptions":{"module":"commonjs","outFile":"all.js","emitDeclarationOnly":true,"declaration":true}}"#;
    let parsed = parse_tsconfig_with_diagnostics(source, "tsconfig.json").unwrap();
    let codes: Vec<u32> = parsed.diagnostics.iter().map(|d| d.code).collect();
    assert!(
        !codes.contains(&6082),
        "Should NOT emit TS6082 when emitDeclarationOnly is true, got: {codes:?}"
    );
}

#[test]
fn test_ts6082_not_emitted_without_outfile() {
    let source = r#"{"compilerOptions":{"module":"commonjs"}}"#;
    let parsed = parse_tsconfig_with_diagnostics(source, "tsconfig.json").unwrap();
    let codes: Vec<u32> = parsed.diagnostics.iter().map(|d| d.code).collect();
    assert!(
        !codes.contains(&6082),
        "Should NOT emit TS6082 without outFile, got: {codes:?}"
    );
}

#[test]
fn test_ts5071_bundler_implied_resolve_json_module_with_umd() {
    // moduleResolution: bundler implies resolveJsonModule=true.
    // Combined with module=umd, this should emit TS5071.
    let source = r#"{"compilerOptions":{"moduleResolution":"bundler","module":"umd"}}"#;
    let parsed = parse_tsconfig_with_diagnostics(source, "tsconfig.json").unwrap();
    let codes: Vec<u32> = parsed.diagnostics.iter().map(|d| d.code).collect();
    assert!(
        codes.contains(&5071),
        "Expected TS5071 for bundler-implied resolveJsonModule with module=umd, got: {codes:?}"
    );
}

#[test]
fn test_ts5071_bundler_implied_resolve_json_module_with_system() {
    let source = r#"{"compilerOptions":{"moduleResolution":"bundler","module":"system"}}"#;
    let parsed = parse_tsconfig_with_diagnostics(source, "tsconfig.json").unwrap();
    let codes: Vec<u32> = parsed.diagnostics.iter().map(|d| d.code).collect();
    assert!(
        codes.contains(&5071),
        "Expected TS5071 for bundler-implied resolveJsonModule with module=system, got: {codes:?}"
    );
}

#[test]
fn test_ts5071_explicit_resolve_json_module_reports_both_keys() {
    let source = r#"{"compilerOptions":{"module":"none","moduleResolution":"bundler","resolveJsonModule":true}}"#;
    let parsed = parse_tsconfig_with_diagnostics(source, "tsconfig.json").unwrap();
    let ts5071: Vec<_> = parsed
        .diagnostics
        .iter()
        .filter(|d| d.code == 5071)
        .collect();
    assert_eq!(
        ts5071.len(),
        2,
        "Expected TS5071 at both 'module' and 'resolveJsonModule', got: {ts5071:?}"
    );
    let starts: Vec<u32> = ts5071.iter().map(|d| d.start).collect();
    assert!(
        starts.contains(&find_key_offset_in_source(source, "module")),
        "Expected TS5071 anchored to 'module', got starts: {starts:?}"
    );
    assert!(
        starts.contains(&find_key_offset_in_source(source, "resolveJsonModule")),
        "Expected TS5071 anchored to 'resolveJsonModule', got starts: {starts:?}"
    );
}

#[test]
fn test_ts5071_not_emitted_for_bundler_with_esnext() {
    // moduleResolution: bundler + module=esnext should NOT emit TS5071
    let source = r#"{"compilerOptions":{"moduleResolution":"bundler","module":"esnext"}}"#;
    let parsed = parse_tsconfig_with_diagnostics(source, "tsconfig.json").unwrap();
    let codes: Vec<u32> = parsed.diagnostics.iter().map(|d| d.code).collect();
    assert!(
        !codes.contains(&5071),
        "Should NOT emit TS5071 for bundler+esnext, got: {codes:?}"
    );
}

#[test]
fn test_implied_classic_resolution_es2015_module() {
    // ES module kinds now default to Bundler resolution, so Classic should
    // remain disabled here.
    let json = r#"{"compilerOptions":{"module":"es2015"}}"#;
    let config: TsConfig = serde_json::from_str(json).unwrap();
    let resolved = resolve_compiler_options(config.compiler_options.as_ref()).unwrap();
    assert!(
        !resolved.checker.implied_classic_resolution,
        "ES2015 should not imply Classic resolution"
    );
}

#[test]
fn test_implied_classic_resolution_amd_module() {
    let json = r#"{"compilerOptions":{"module":"amd"}}"#;
    let config: TsConfig = serde_json::from_str(json).unwrap();
    let resolved = resolve_compiler_options(config.compiler_options.as_ref()).unwrap();
    assert!(
        resolved.checker.implied_classic_resolution,
        "AMD should imply Classic resolution"
    );
}

#[test]
fn test_implied_classic_resolution_explicit_node_override() {
    // module: es2015 + moduleResolution: node10 → NOT Classic
    let json = r#"{"compilerOptions":{"module":"es2015","moduleResolution":"node10"}}"#;
    let config: TsConfig = serde_json::from_str(json).unwrap();
    let resolved = resolve_compiler_options(config.compiler_options.as_ref()).unwrap();
    assert!(
        !resolved.checker.implied_classic_resolution,
        "Explicit moduleResolution: node10 should override Classic inference"
    );
}

#[test]
fn test_implied_classic_resolution_commonjs_module() {
    // module: commonjs → effective resolution is Node10, NOT Classic
    let json = r#"{"compilerOptions":{"module":"commonjs"}}"#;
    let config: TsConfig = serde_json::from_str(json).unwrap();
    let resolved = resolve_compiler_options(config.compiler_options.as_ref()).unwrap();
    assert!(
        !resolved.checker.implied_classic_resolution,
        "CommonJS module should not imply Classic resolution"
    );
}

#[test]
fn test_implied_classic_resolution_nodenext_module() {
    // module: nodenext → effective resolution is NodeNext, NOT Classic
    let json = r#"{"compilerOptions":{"module":"nodenext"}}"#;
    let config: TsConfig = serde_json::from_str(json).unwrap();
    let resolved = resolve_compiler_options(config.compiler_options.as_ref()).unwrap();
    assert!(
        !resolved.checker.implied_classic_resolution,
        "NodeNext module should not imply Classic resolution"
    );
}

#[test]
fn test_implied_classic_resolution_explicit_bundler() {
    // module: esnext + moduleResolution: bundler → NOT Classic
    let json = r#"{"compilerOptions":{"module":"esnext","moduleResolution":"bundler"}}"#;
    let config: TsConfig = serde_json::from_str(json).unwrap();
    let resolved = resolve_compiler_options(config.compiler_options.as_ref()).unwrap();
    assert!(
        !resolved.checker.implied_classic_resolution,
        "Explicit moduleResolution: bundler should override Classic inference"
    );
}

#[test]
fn test_preserve_symlinks_defaults_false() {
    let resolved = resolve_compiler_options(None).unwrap();
    assert!(!resolved.preserve_symlinks);
}

#[test]
fn test_preserve_symlinks_resolved_from_tsconfig() {
    let json = r#"{"compilerOptions":{"preserveSymlinks":true}}"#;
    let config: TsConfig = serde_json::from_str(json).unwrap();
    let resolved = resolve_compiler_options(config.compiler_options.as_ref()).unwrap();
    assert!(resolved.preserve_symlinks);
}

#[test]
fn test_ts5107_always_strict_false() {
    let source = r#"{"compilerOptions":{"alwaysStrict":false}}"#;
    let parsed = parse_tsconfig_with_diagnostics(source, "tsconfig.json").unwrap();
    assert!(
        parsed.diagnostics.iter().any(|d| d.code == 5107),
        "alwaysStrict=false should trigger TS5107; got: {:?}",
        parsed
            .diagnostics
            .iter()
            .map(|d| d.code)
            .collect::<Vec<_>>()
    );
}

#[test]
fn test_ts5107_target_es5() {
    let source = r#"{"compilerOptions":{"target":"ES5"}}"#;
    let parsed = parse_tsconfig_with_diagnostics(source, "tsconfig.json").unwrap();
    assert!(
        parsed.diagnostics.iter().any(|d| d.code == 5107),
        "target=ES5 should trigger TS5107; got: {:?}",
        parsed
            .diagnostics
            .iter()
            .map(|d| d.code)
            .collect::<Vec<_>>()
    );
}

#[test]
fn test_ts5108_target_es3() {
    // target=ES3 was removed in TS 6.0 (TS5108, not suppressible by ignoreDeprecations).
    for value in &["ES3", "es3"] {
        let source = format!(r#"{{"compilerOptions":{{"target":"{value}"}}}}"#);
        let parsed = parse_tsconfig_with_diagnostics(&source, "tsconfig.json").unwrap();
        assert!(
            parsed.diagnostics.iter().any(|d| d.code == 5108),
            "target={value} should trigger TS5108; got: {:?}",
            parsed
                .diagnostics
                .iter()
                .map(|d| d.code)
                .collect::<Vec<_>>()
        );
    }
}

#[test]
fn test_ts5108_target_es3_not_suppressed_by_ignore_deprecations() {
    // TS5108 (removed value) must fire even when ignoreDeprecations="6.0" is set.
    let source = r#"{"compilerOptions":{"target":"ES3","ignoreDeprecations":"6.0"}}"#;
    let parsed = parse_tsconfig_with_diagnostics(source, "tsconfig.json").unwrap();
    assert!(
        parsed.diagnostics.iter().any(|d| d.code == 5108),
        "ignoreDeprecations=6.0 must NOT suppress TS5108 (removed value); got: {:?}",
        parsed
            .diagnostics
            .iter()
            .map(|d| d.code)
            .collect::<Vec<_>>()
    );
}

#[test]
fn test_ts5107_suppressed_by_ignore_deprecations_6_0() {
    let source = r#"{"compilerOptions":{"alwaysStrict":false,"ignoreDeprecations":"6.0"}}"#;
    let parsed = parse_tsconfig_with_diagnostics(source, "tsconfig.json").unwrap();
    assert!(
        !parsed.diagnostics.iter().any(|d| d.code == 5107),
        "ignoreDeprecations=6.0 should suppress TS5107; got: {:?}",
        parsed
            .diagnostics
            .iter()
            .map(|d| d.code)
            .collect::<Vec<_>>()
    );
}

#[test]
fn test_child_ignore_deprecations_suppresses_inherited_ts5107() {
    let temp = tempdir().expect("create temp dir");
    let base_path = temp.path().join("base.json");
    std::fs::write(
        &base_path,
        r#"{
  "compilerOptions": {
"moduleResolution": "node"
  }
}"#,
    )
    .expect("write base");

    let child_path = temp.path().join("tsconfig.json");
    std::fs::write(
        &child_path,
        r#"{
  "extends": "./base.json",
  "compilerOptions": {
"ignoreDeprecations": "6.0"
  },
  "files": ["a.ts"]
}"#,
    )
    .expect("write child");

    let parsed = load_tsconfig_with_diagnostics(&child_path).expect("load child");
    assert!(
        !parsed.diagnostics.iter().any(|d| d.code == 5107),
        "child ignoreDeprecations=6.0 should suppress inherited TS5107, got: {:?}",
        parsed
            .diagnostics
            .iter()
            .map(|d| (&d.file, d.code, &d.message_text))
            .collect::<Vec<_>>()
    );
}

#[test]
fn test_ts5101_base_url() {
    let source = r#"{"compilerOptions":{"baseUrl":"."}}"#;
    let parsed = parse_tsconfig_with_diagnostics(source, "tsconfig.json").unwrap();
    assert!(
        parsed.diagnostics.iter().any(|d| d.code == 5101),
        "baseUrl should trigger TS5101; got: {:?}",
        parsed
            .diagnostics
            .iter()
            .map(|d| d.code)
            .collect::<Vec<_>>()
    );
}

#[test]
fn test_ts5090_paths_substitutions_require_base_url() {
    let source = r#"{
  "compilerOptions": {
"paths": {
  "@app/*": ["src/*", "./ok/*", "../up/*", "*"]
}
  }
}"#;
    let parsed = parse_tsconfig_with_diagnostics(source, "tsconfig.json").unwrap();
    let ts5090: Vec<_> = parsed
        .diagnostics
        .iter()
        .filter(|diag| diag.code == 5090)
        .collect();
    assert_eq!(
        ts5090.len(),
        2,
        "Expected TS5090 only for non-relative substitutions without baseUrl, got: {:?}",
        parsed
            .diagnostics
            .iter()
            .map(|d| (d.code, &d.message_text))
            .collect::<Vec<_>>()
    );
}

#[test]
fn test_ts5090_paths_substitutions_suppressed_when_base_url_present() {
    let source = r#"{
  "compilerOptions": {
"baseUrl": ".",
"paths": {
  "@app/*": ["src/*", "*"]
}
  }
}"#;
    let parsed = parse_tsconfig_with_diagnostics(source, "tsconfig.json").unwrap();
    let codes: Vec<u32> = parsed.diagnostics.iter().map(|d| d.code).collect();
    assert!(
        !codes.contains(&5090),
        "baseUrl should suppress TS5090 for non-relative substitutions, got: {codes:?}"
    );
}

#[test]
fn test_strict_family_defaults_true_when_strict_not_set() {
    // tsc 6.0 defaults: strict-family options are true when not explicitly set.
    // The tsc cache was generated with tsc 6.0-dev which has strict=true as its
    // effective default. CheckerOptions::default() reflects this.
    let json = r#"{"compilerOptions":{"target":"es2015"}}"#;
    let config: TsConfig = serde_json::from_str(json).unwrap();
    let resolved = resolve_compiler_options(config.compiler_options.as_ref()).unwrap();
    assert!(
        resolved.checker.strict_null_checks,
        "strictNullChecks should default to true when strict is not set"
    );
    assert!(
        resolved.checker.strict_function_types,
        "strictFunctionTypes should default to true when strict is not set"
    );
    assert!(
        resolved.checker.no_implicit_any,
        "noImplicitAny should default to true when strict is not set"
    );
    assert!(
        resolved.checker.strict_property_initialization,
        "strictPropertyInitialization should default to true when strict is not set"
    );
    assert!(
        resolved.checker.no_implicit_this,
        "noImplicitThis should default to true when strict is not set"
    );
    assert!(
        resolved.checker.use_unknown_in_catch_variables,
        "useUnknownInCatchVariables should default to true when strict is not set"
    );
}

#[test]
fn test_resolve_compiler_options_propagates_no_check() {
    let json = r#"{"compilerOptions":{"noCheck":true}}"#;
    let config: TsConfig = serde_json::from_str(json).unwrap();
    let resolved = resolve_compiler_options(config.compiler_options.as_ref()).unwrap();
    assert!(resolved.no_check, "noCheck should be read from tsconfig");
}

#[test]
fn test_strict_false_keeps_always_strict_default() {
    // In TS 6.0, strict:false still leaves alwaysStrict on by default unless it
    // is explicitly set to false.
    let json = r#"{"compilerOptions":{"strict":false}}"#;
    let config: TsConfig = serde_json::from_str(json).unwrap();
    let resolved = resolve_compiler_options(config.compiler_options.as_ref()).unwrap();
    assert!(
        !resolved.checker.strict_null_checks,
        "strictNullChecks should be false when strict: false"
    );
    assert!(
        !resolved.checker.no_implicit_any,
        "noImplicitAny should be false when strict: false"
    );
    assert!(
        !resolved.checker.strict_property_initialization,
        "strictPropertyInitialization should be false when strict: false"
    );
    assert!(
        resolved.checker.always_strict,
        "alwaysStrict should remain true by default when strict: false"
    );
    assert!(
        resolved.printer.always_strict,
        "printer alwaysStrict should remain true by default when strict: false"
    );
}

#[test]
fn test_individual_strict_option_overrides_default() {
    // Individual strict-family options should override the default.
    let json = r#"{"compilerOptions":{"strictNullChecks":false}}"#;
    let config: TsConfig = serde_json::from_str(json).unwrap();
    let resolved = resolve_compiler_options(config.compiler_options.as_ref()).unwrap();
    assert!(
        !resolved.checker.strict_null_checks,
        "strictNullChecks should be false when explicitly set to false"
    );
    // Other strict-family options should still be true (from defaults)
    assert!(
        resolved.checker.no_implicit_any,
        "noImplicitAny should remain true when only strictNullChecks is overridden"
    );
}

#[test]
fn test_ts5024_boolean_string_uses_always_strict_default() {
    // tsc still enforces strict-mode syntax here: the invalid string value is
    // rejected with TS5024, then option resolution falls back to the TS 6.0
    // alwaysStrict default of true.
    let source = r#"{
  "compilerOptions": {
"strict": false,
"alwaysStrict": "true"
  }
}"#;
    let parsed = parse_tsconfig_with_diagnostics(source, "tsconfig.json").unwrap();
    // TS5024 should be emitted for the string-typed boolean
    let has_ts5024 = parsed.diagnostics.iter().any(|d| d.code == 5024);
    assert!(
        has_ts5024,
        "Should emit TS5024 for string 'true' on boolean option"
    );
    // The invalid value itself is not applied, but alwaysStrict still falls
    // back to its TS 6.0 default of true.
    let resolved = resolve_compiler_options(parsed.config.compiler_options.as_ref()).unwrap();
    assert!(
        resolved.checker.always_strict,
        "alwaysStrict should fall back to the TS 6.0 default when provided as a string-typed boolean"
    );
    assert!(
        resolved.printer.always_strict,
        "printer alwaysStrict should fall back to the TS 6.0 default when provided as a string-typed boolean"
    );
}

#[test]
fn test_explicit_always_strict_false_overrides_default_even_with_strict_false() {
    let source = r#"{
  "compilerOptions": {
"strict": false,
"alwaysStrict": false,
"ignoreDeprecations": "6.0"
  }
}"#;
    let parsed = parse_tsconfig_with_diagnostics(source, "tsconfig.json").unwrap();
    let resolved = resolve_compiler_options(parsed.config.compiler_options.as_ref()).unwrap();
    assert!(
        !resolved.checker.always_strict,
        "explicit alwaysStrict=false should still disable alwaysStrict"
    );
    assert!(
        !resolved.printer.always_strict,
        "explicit alwaysStrict=false should still disable printer alwaysStrict"
    );
}

#[test]
fn test_ts5024_isolated_modules_string_is_not_applied() {
    let source = r#"{
  "compilerOptions": {
"target": "es2015",
"module": "commonjs",
"isolatedModules": "true"
  }
}"#;
    let parsed = parse_tsconfig_with_diagnostics(source, "tsconfig.json").unwrap();
    let has_ts5024 = parsed.diagnostics.iter().any(|d| d.code == 5024);
    assert!(
        has_ts5024,
        "Should emit TS5024 for string 'true' on isolatedModules"
    );
    let resolved = resolve_compiler_options(parsed.config.compiler_options.as_ref()).unwrap();
    assert!(
        !resolved.checker.isolated_modules,
        "isolatedModules should not be applied from a string-typed boolean value"
    );
}

#[test]
fn test_ts5024_allow_importing_ts_extensions_string_is_not_applied() {
    let source = r#"{
  "compilerOptions": {
"moduleResolution": "bundler",
"module": "esnext",
"allowImportingTsExtensions": "true"
  }
}"#;
    let parsed = parse_tsconfig_with_diagnostics(source, "tsconfig.json").unwrap();
    let has_ts5024 = parsed.diagnostics.iter().any(|d| d.code == 5024);
    assert!(
        has_ts5024,
        "Should emit TS5024 for string 'true' on allowImportingTsExtensions"
    );
    let resolved = resolve_compiler_options(parsed.config.compiler_options.as_ref()).unwrap();
    assert!(
        !resolved.allow_importing_ts_extensions,
        "allowImportingTsExtensions should not be applied from a string-typed boolean value"
    );
}

#[test]
fn test_ts5096_allow_importing_ts_extensions_requires_emit_guard() {
    let invalid = r#"{"compilerOptions":{"allowImportingTsExtensions":true}}"#;
    let parsed = parse_tsconfig_with_diagnostics(invalid, "tsconfig.json").unwrap();
    assert!(
        parsed.diagnostics.iter().any(|d| d.code
            == diagnostic_codes::OPTION_ALLOWIMPORTINGTSEXTENSIONS_CAN_ONLY_BE_USED_WHEN_ONE_OF_NOEMIT_EMITDECLAR),
        "Expected TS5096 for allowImportingTsExtensions without an emit guard, got: {:?}",
        parsed.diagnostics
    );

    for valid in [
        r#"{"compilerOptions":{"allowImportingTsExtensions":true,"noEmit":true}}"#,
        r#"{"compilerOptions":{"allowImportingTsExtensions":true,"declaration":true,"emitDeclarationOnly":true}}"#,
        r#"{"compilerOptions":{"allowImportingTsExtensions":true,"rewriteRelativeImportExtensions":true}}"#,
    ] {
        let parsed = parse_tsconfig_with_diagnostics(valid, "tsconfig.json").unwrap();
        assert!(
            parsed.diagnostics.iter().all(|d| d.code
                != diagnostic_codes::OPTION_ALLOWIMPORTINGTSEXTENSIONS_CAN_ONLY_BE_USED_WHEN_ONE_OF_NOEMIT_EMITDECLAR),
            "Did not expect TS5096 for guarded allowImportingTsExtensions in {valid}, got: {:?}",
            parsed.diagnostics
        );
    }
}

#[test]
fn test_ts6304_composite_disables_declaration() {
    let source = r#"{"compilerOptions":{"composite":true,"declaration":false}}"#;
    let parsed = parse_tsconfig_with_diagnostics(source, "tsconfig.json").unwrap();
    let codes: Vec<u32> = parsed.diagnostics.iter().map(|d| d.code).collect();
    assert!(
        codes.contains(&6304),
        "Expected TS6304 when composite:true but declaration:false, got: {codes:?}"
    );
}

#[test]
fn test_ts6304_not_emitted_when_declaration_true() {
    let source = r#"{"compilerOptions":{"composite":true,"declaration":true}}"#;
    let parsed = parse_tsconfig_with_diagnostics(source, "tsconfig.json").unwrap();
    let codes: Vec<u32> = parsed.diagnostics.iter().map(|d| d.code).collect();
    assert!(
        !codes.contains(&6304),
        "Should NOT emit TS6304 when both composite and declaration are true, got: {codes:?}"
    );
}

#[test]
fn test_ts6379_composite_disables_incremental() {
    let source = r#"{"compilerOptions":{"composite":true,"incremental":false}}"#;
    let parsed = parse_tsconfig_with_diagnostics(source, "tsconfig.json").unwrap();
    let codes: Vec<u32> = parsed.diagnostics.iter().map(|d| d.code).collect();
    assert!(
        codes.contains(&6379),
        "Expected TS6379 when composite:true but incremental:false, got: {codes:?}"
    );
}

#[test]
fn test_ts6379_not_emitted_when_incremental_omitted() {
    // composite implies incremental, so omitting incremental is fine
    let source = r#"{"compilerOptions":{"composite":true}}"#;
    let parsed = parse_tsconfig_with_diagnostics(source, "tsconfig.json").unwrap();
    let codes: Vec<u32> = parsed.diagnostics.iter().map(|d| d.code).collect();
    assert!(
        !codes.contains(&6379),
        "Should NOT emit TS6379 when composite is true and incremental is omitted, got: {codes:?}"
    );
}

#[test]
fn test_composite_implies_declaration_and_incremental() {
    let source = r#"{"compilerOptions":{"composite":true,"noLib":true}}"#;
    let parsed = parse_tsconfig_with_diagnostics(source, "tsconfig.json").unwrap();
    let resolved = resolve_compiler_options(parsed.config.compiler_options.as_ref()).unwrap();
    assert!(
        resolved.composite,
        "composite should be true in resolved options"
    );
    assert!(
        resolved.emit_declarations,
        "composite should imply declaration:true"
    );
    assert!(
        resolved.incremental,
        "composite should imply incremental:true"
    );
}

#[test]
fn test_no_property_access_from_index_signature_resolves_from_tsconfig() {
    let source = r#"{
        "compilerOptions": {
            "noPropertyAccessFromIndexSignature": true
        }
    }"#;
    let parsed = parse_tsconfig_with_diagnostics(source, "tsconfig.json").unwrap();
    let resolved = resolve_compiler_options(parsed.config.compiler_options.as_ref()).unwrap();

    assert!(resolved.checker.no_property_access_from_index_signature);
}

#[test]
fn test_ts5024_top_level_selector_type_mismatches_are_recovered() {
    for (key, value) in [
        ("include", r#""*.ts""#),
        ("exclude", r#""dist""#),
        ("references", r#""./lib""#),
    ] {
        let source = format!(
            r#"{{
  "{key}": {value},
  "compilerOptions": {{ "noEmit": true }},
  "files": ["a.ts"]
}}"#
        );
        let parsed = parse_tsconfig_with_diagnostics(&source, "tsconfig.json").unwrap();
        let ts5024 = parsed
            .diagnostics
            .iter()
            .find(|d| {
                d.code == diagnostic_codes::COMPILER_OPTION_REQUIRES_A_VALUE_OF_TYPE
                    && d.message_text.contains(key)
                    && d.message_text.contains("Array")
            })
            .unwrap_or_else(|| panic!("Expected TS5024 for {key}, got: {:?}", parsed.diagnostics));

        assert_eq!(
            ts5024.start,
            source.find(value).expect("test value") as u32,
            "TS5024 for {key} should point at the invalid value"
        );

        match key {
            "include" => assert!(parsed.config.include.is_none()),
            "exclude" => assert!(parsed.config.exclude.is_none()),
            "references" => assert!(parsed.config.references.is_none()),
            _ => unreachable!(),
        }
        assert_eq!(parsed.config.files, Some(vec!["a.ts".to_string()]));
    }
}

#[test]
fn test_tsconfig_references_parsed() {
    let source = r#"{
        "compilerOptions": { "composite": true },
        "references": [
            { "path": "./packages/core" },
            { "path": "./packages/utils", "prepend": true }
        ]
    }"#;
    let parsed = parse_tsconfig_with_diagnostics(source, "tsconfig.json").unwrap();
    let refs = parsed.config.references.expect("should have references");
    assert_eq!(refs.len(), 2);
    assert_eq!(refs[0].path, "./packages/core");
    assert!(!refs[0].prepend);
    assert_eq!(refs[1].path, "./packages/utils");
    assert!(refs[1].prepend);
}

#[test]
fn test_extract_lib_references_normalizes_and_stops_at_first_code_line() {
    let source = r#"
        // regular comment
        /// <reference lib="ES2015" />
        /// <reference lib='lib.dom' />

        const x = 1;
        /// <reference lib="esnext" />
    "#;

    assert_eq!(
        extract_lib_references(source),
        vec!["es2015".to_string(), "dom".to_string()]
    );
}

#[test]
fn test_extract_lib_references_skips_block_comments_and_ignores_embedded_lib_text() {
    let source = r#"
        /*
         * /// <reference lib="es2017" />
         */
        /// <reference lib="es2020" />
        /// not really a lib directive
    "#;

    assert_eq!(extract_lib_references(source), vec!["es2020".to_string()]);
}

#[test]
fn test_extract_lib_references_with_positions_anchors_at_value_start() {
    // `/// <reference lib="notalib" />` — `notalib` starts at byte 20,
    // matching tsc's TS2726 anchor (column 21 in 1-indexed terms).
    let source = "/// <reference lib=\"notalib\" />\nlet x = 1;\n";
    let refs = extract_lib_references_with_positions(source);
    assert_eq!(refs.len(), 1, "should capture exactly one reference");
    assert_eq!(refs[0].raw, "notalib");
    assert_eq!(refs[0].start, 20);
    assert_eq!(refs[0].length, 7);
}

#[test]
fn test_extract_lib_references_with_positions_handles_empty_value() {
    let source = "/// <reference lib=\"\" />\n";
    let refs = extract_lib_references_with_positions(source);
    assert_eq!(refs.len(), 1);
    assert_eq!(refs[0].raw, "");
    assert_eq!(refs[0].start, 20);
    assert_eq!(refs[0].length, 0);
}

#[test]
fn test_extract_lib_references_with_positions_tracks_offset_across_lines() {
    let source = "// header\n\n/// <reference lib=\"dom\" />\n";
    let refs = extract_lib_references_with_positions(source);
    assert_eq!(refs.len(), 1);
    assert_eq!(refs[0].raw, "dom");
    // "// header\n" = 10 bytes, blank line "\n" = 1 byte, then 11 + 20 = 31.
    assert_eq!(refs[0].start, 31);
    assert_eq!(refs[0].length, 3);
}

#[test]
fn test_is_known_lib_name_accepts_canonical_and_normalized_forms() {
    // Canonical names from the TS lib catalog.
    assert!(is_known_lib_name("es2015"));
    assert!(is_known_lib_name("dom"));
    // Normalization: leading `lib.` prefix and case-insensitive match.
    assert!(is_known_lib_name("lib.es2015"));
    assert!(is_known_lib_name("ES2015"));
}

#[test]
fn test_is_known_lib_name_rejects_empty_and_unknown() {
    assert!(!is_known_lib_name(""));
    assert!(!is_known_lib_name("   "));
    assert!(!is_known_lib_name("notalib"));
}

#[test]
fn test_strip_jsonc_preserves_comment_like_text_inside_strings() {
    let input = r#"{
  // line comment
  "url": "https://example.test/*keep*/",
  "text": "// still text",
  /* block
 comment */
  "value": 1
}"#;

    let stripped = strip_jsonc(input);
    assert!(stripped.contains(r#""url": "https://example.test/*keep*/""#));
    assert!(stripped.contains(r#""text": "// still text""#));
    assert!(stripped.contains(r#""value": 1"#));
    assert!(!stripped.contains("line comment"));
    assert!(!stripped.contains("block"));
}

#[test]
fn test_normalize_jsonc_removes_comments_and_trailing_commas() {
    let input = r#"{
  // line comment
  "url": "https://example.test/*keep*/",
  "items": [
"a",
  ],
}"#;

    let normalized = normalize_jsonc(input);
    let value: serde_json::Value =
        serde_json::from_str(&normalized).expect("normalized JSONC should parse");
    assert_eq!(value["url"], "https://example.test/*keep*/");
    assert_eq!(value["items"][0], "a");
}

#[test]
fn test_default_and_core_lib_names_cover_newer_targets() {
    assert_eq!(default_lib_name_for_target(ScriptTarget::ES5), "lib");
    assert_eq!(default_lib_name_for_target(ScriptTarget::ES2015), "es6");
    assert_eq!(
        default_lib_name_for_target(ScriptTarget::ES2025),
        "esnext.full"
    );
    assert_eq!(
        default_lib_name_for_target(ScriptTarget::ESNext),
        "esnext.full"
    );

    assert_eq!(core_lib_name_for_target(ScriptTarget::ES3), "es5");
    assert_eq!(core_lib_name_for_target(ScriptTarget::ES2025), "esnext");
    assert_eq!(core_lib_name_for_target(ScriptTarget::ESNext), "esnext");
}

#[test]
fn parse_script_target_rejects_comma_separated_values() {
    assert!(parse_script_target("ES5, ES2015").is_err());
    assert!(parse_script_target("es2015, es2017").is_err());
    assert!(parse_script_target("esnext, es2022").is_err());
    // Single value should still work
    assert_eq!(parse_script_target("ES5").unwrap(), ScriptTarget::ES5);
    assert_eq!(parse_script_target("es2020").unwrap(), ScriptTarget::ES2020);
}

#[test]
fn resolve_lib_files_strict_errors_on_unknown_user_lib() {
    // User-supplied compilerOptions.lib must error on unknown names so users
    // catch typos. Use a name that cannot exist as an alias.
    let err = resolve_lib_files_with_options(&["definitely.not.a.real.lib".to_string()], false)
        .expect_err("expected unknown user lib to error");
    let msg = format!("{err}");
    assert!(
        msg.contains("unsupported compilerOptions.lib"),
        "unexpected error message: {msg}"
    );
}

#[test]
fn resolve_lib_files_lenient_skips_unknown_transitive_libs() {
    // Source-file `/// <reference lib="..." />` directives should be
    // tolerated when the lib name no longer exists. Mix a real lib with
    // a long-renamed one to confirm the real one still resolves and the
    // missing one is silently dropped instead of erroring.
    let paths = resolve_lib_files_with_options_transitive(
        &[
            "es2018.asynciterable".to_string(),
            "esnext.asynciterable".to_string(),
        ],
        false,
    )
    .expect("transitive resolver should not error on unknown names");
    let names: Vec<String> = paths
        .iter()
        .filter_map(|p| p.file_name().and_then(|s| s.to_str()).map(String::from))
        .collect();
    assert!(
        names.iter().any(|n| n.contains("es2018.asynciterable")),
        "expected es2018.asynciterable in resolved set, got {names:?}"
    );
}

#[test]
fn embedded_esnext_collection_follows_es2025_collection() {
    let paths = resolve_lib_files_from_embedded(&["esnext.collection".to_string()], true)
        .expect("embedded esnext.collection should resolve");
    let names: Vec<String> = paths
        .iter()
        .filter_map(|p| p.file_name().and_then(|s| s.to_str()).map(String::from))
        .collect();
    assert!(
        names.iter().any(|n| n == "es2025.collection.d.ts"),
        "expected es2025.collection in resolved set, got {names:?}"
    );
    assert!(
        names.iter().any(|n| n == "esnext.collection.d.ts"),
        "expected esnext.collection in resolved set, got {names:?}"
    );
}

/// Issue #3589 — base configs reached via `extends` must be validated by
/// the diagnostic loader so TS5024 surfaces on the base file rather than
/// being silently coerced through the non-diagnostic path.
#[test]
fn test_extends_base_invalid_boolean_string_emits_ts5024_anchored_at_base() {
    let temp = tempdir().expect("create temp dir");
    let base_path = temp.path().join("base.json");
    std::fs::write(
        &base_path,
        r#"{
  "compilerOptions": {
"noUncheckedIndexedAccess": "true"
  }
}"#,
    )
    .expect("write base");

    let child_path = temp.path().join("tsconfig.json");
    std::fs::write(
        &child_path,
        r#"{
  "extends": "./base.json",
  "files": ["a.ts"]
}"#,
    )
    .expect("write child");

    let parsed = load_tsconfig_with_diagnostics(&child_path).expect("load child");
    let ts5024: Vec<&Diagnostic> = parsed
        .diagnostics
        .iter()
        .filter(|d| d.code == diagnostic_codes::COMPILER_OPTION_REQUIRES_A_VALUE_OF_TYPE)
        .collect();
    assert!(
        !ts5024.is_empty(),
        "expected TS5024 from inherited base, got: {:?}",
        parsed.diagnostics
    );
    let base_diag = ts5024
        .iter()
        .find(|d| d.message_text.contains("noUncheckedIndexedAccess"))
        .expect("TS5024 for noUncheckedIndexedAccess");
    // Normalize both sides to the canonical filename so platform path
    // separators and intermediate `./` segments don't break the assert.
    let base_file_name = std::path::Path::new(&base_diag.file)
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("");
    assert_eq!(
        base_file_name, "base.json",
        "TS5024 must anchor at the base file, got file={:?}",
        base_diag.file
    );
    assert!(
        !base_diag.file.contains("tsconfig.json"),
        "TS5024 anchor must point at base.json, not the child tsconfig.json: {:?}",
        base_diag.file
    );
    // The invalid coerced value must NOT survive into the merged config.
    let opts = parsed
        .config
        .compiler_options
        .as_ref()
        .expect("merged compiler options");
    assert_eq!(
        opts.no_unchecked_indexed_access, None,
        "invalidly-typed base option must be removed before merge"
    );
}

/// Issue #3589 — the rule is structural, not keyed on a specific option.
/// Re-check with a different option (`allowJs`) to lock the parity for the
/// whole boolean-coercion family rather than one option name.
#[test]
fn test_extends_base_invalid_allowjs_string_emits_ts5024_anchored_at_base() {
    let temp = tempdir().expect("create temp dir");
    let base_path = temp.path().join("base.json");
    std::fs::write(&base_path, r#"{"compilerOptions":{"allowJs":"true"}}"#).expect("write base");

    let child_path = temp.path().join("tsconfig.json");
    std::fs::write(&child_path, r#"{"extends":"./base.json","files":["a.ts"]}"#)
        .expect("write child");

    let parsed = load_tsconfig_with_diagnostics(&child_path).expect("load child");
    let _ = base_path; // path is captured for inspection on failure
    assert!(
        parsed.diagnostics.iter().any(|d| {
            d.code == diagnostic_codes::COMPILER_OPTION_REQUIRES_A_VALUE_OF_TYPE
                && std::path::Path::new(&d.file)
                    .file_name()
                    .and_then(|s| s.to_str())
                    == Some("base.json")
                && d.message_text.contains("allowJs")
        }),
        "expected TS5024 anchored at base for allowJs, got: {:?}",
        parsed.diagnostics
    );
}

/// A valid base config must not produce spurious TS5024 just because the
/// child loader now recurses through the diagnostic path. Regression guard
/// for the happy path of #3589's fix.
#[test]
fn test_extends_base_valid_options_emit_no_ts5024() {
    let temp = tempdir().expect("create temp dir");
    let base_path = temp.path().join("base.json");
    std::fs::write(
        &base_path,
        r#"{"compilerOptions":{"strict":true,"noUncheckedIndexedAccess":true}}"#,
    )
    .expect("write base");

    let child_path = temp.path().join("tsconfig.json");
    std::fs::write(&child_path, r#"{"extends":"./base.json","files":["a.ts"]}"#)
        .expect("write child");

    let parsed = load_tsconfig_with_diagnostics(&child_path).expect("load child");
    assert!(
        !parsed
            .diagnostics
            .iter()
            .any(|d| d.code == diagnostic_codes::COMPILER_OPTION_REQUIRES_A_VALUE_OF_TYPE),
        "no TS5024 expected for valid base, got: {:?}",
        parsed.diagnostics
    );
    let opts = parsed
        .config
        .compiler_options
        .as_ref()
        .expect("merged compiler options");
    assert_eq!(opts.no_unchecked_indexed_access, Some(true));
    assert_eq!(opts.strict, Some(true));
}
fn ts6046_for_watch_options(source: &str, key: &str) -> Vec<String> {
    let parsed =
        parse_tsconfig_with_diagnostics(source, "tsconfig.json").expect("tsconfig must parse");
    parsed
        .diagnostics
        .iter()
        .filter(|d| d.code == 6046 && d.message_text.contains(key))
        .map(|d| d.message_text.clone())
        .collect()
}

#[test]
fn ts6046_fires_for_invalid_watch_file_value() {
    // #3591 repro A: `watchOptions.watchFile: "bad"` must surface TS6046
    // listing the valid `--watchFile` values.
    let source = r#"{"compilerOptions":{"noEmit":true},"watchOptions":{"watchFile":"bad"},"files":["a.ts"]}"#;
    let diags = ts6046_for_watch_options(source, "--watchFile");
    assert_eq!(
        diags.len(),
        1,
        "expected one TS6046 for invalid watchFile, got {diags:?}"
    );
    assert!(
        diags[0].contains("fixedpollinginterval"),
        "TS6046 message must list valid watchFile values, got {diags:?}"
    );
}

#[test]
fn ts6046_fires_for_invalid_watch_directory_value() {
    let source = r#"{"compilerOptions":{"noEmit":true},"watchOptions":{"watchDirectory":"bad"}}"#;
    let diags = ts6046_for_watch_options(source, "--watchDirectory");
    assert_eq!(
        diags.len(),
        1,
        "expected one TS6046 for invalid watchDirectory, got {diags:?}"
    );
}

#[test]
fn ts6046_fires_for_invalid_fallback_polling_value() {
    let source = r#"{"compilerOptions":{"noEmit":true},"watchOptions":{"fallbackPolling":"bad"}}"#;
    let diags = ts6046_for_watch_options(source, "--fallbackPolling");
    assert_eq!(
        diags.len(),
        1,
        "expected one TS6046 for invalid fallbackPolling, got {diags:?}"
    );
}

#[test]
fn ts6046_silent_for_valid_watch_file_value() {
    let source =
        r#"{"compilerOptions":{"noEmit":true},"watchOptions":{"watchFile":"useFsEvents"}}"#;
    let diags = ts6046_for_watch_options(source, "--watchFile");
    assert!(
        diags.is_empty(),
        "valid watchFile value must not emit TS6046, got {diags:?}"
    );
}

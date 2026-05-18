//! Lookup Integration tests for `module_resolver`.
//!
//! Integration tests that exercise `ModuleResolver::lookup()` end-to-end:
//! extension suggestions, fallback behavior across resolution modes,
//! bundler vs Node16/NodeNext condition selection, `module: preserve`
//! syntax-directed conditions, path-mapping diagnostics, and the
//! diagnostic-code selection table for the TS2307/TS2732/TS2792/TS5097
//! family.

use super::super::*;

#[test]
fn test_lookup_extension_suggestion_esm() {
    // TS2835: relative import without extension in ESM Node16 context
    // should fail with a suggestion extension
    use std::fs;
    let dir = std::env::temp_dir().join("tsz_lookup_ext_suggestion_esm");
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("src")).unwrap();
    fs::write(dir.join("src/utils.ts"), "export const x = 1;").unwrap();
    fs::write(dir.join("src/index.mts"), "import { x } from './utils';").unwrap();
    fs::write(dir.join("package.json"), r#"{"type": "module"}"#).unwrap();

    let options = ResolvedCompilerOptions {
        module_resolution: Some(ModuleResolutionKind::Node16),
        module_suffixes: vec![String::new()],
        printer: crate::emitter::PrinterOptions {
            module: crate::emitter::ModuleKind::Node16,
            ..Default::default()
        },
        ..Default::default()
    };
    let mut resolver = ModuleResolver::new(&options);

    let request = ModuleLookupRequest {
        specifier: "./utils",
        containing_file: &dir.join("src/index.mts"),
        specifier_span: Span::new(22, 30),
        import_kind: ImportKind::EsmImport,
        resolution_mode_override: None,
        no_implicit_any: false,
        implied_classic_resolution: false,
    };
    let result = resolver.lookup(&request, |_, _| None, |_| false, None);

    assert!(
        result.resolved_path.is_none(),
        "should not resolve without extension"
    );
    assert!(!result.treat_as_resolved, "should not treat as resolved");
    let error = result.error.expect("should have an error");
    assert_eq!(
        error.code, IMPORT_PATH_NEEDS_EXTENSION_SUGGESTION,
        "should suggest extension (TS2835)"
    );
    assert!(
        error.message.contains(".js'"),
        "should suggest .js extension: {}",
        error.message
    );

    let _ = fs::remove_dir_all(&dir);
}

// TODO: TSX resolution is too permissive -- resolves ./foo to ./foo.tsx without
// requiring explicit extension in NodeNext mode.
#[test]
fn test_lookup_extension_suggestion_tsx_preserve_uses_jsx() {
    use std::fs;
    let dir = std::env::temp_dir().join("tsz_lookup_ext_suggestion_tsx_preserve");
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("src")).unwrap();
    fs::write(dir.join("src/foo.tsx"), "export const foo = <div />;").unwrap();
    fs::write(dir.join("src/index.mts"), "import { foo } from './foo';").unwrap();
    fs::write(dir.join("package.json"), r#"{"type": "module"}"#).unwrap();

    let options = ResolvedCompilerOptions {
        module_resolution: Some(ModuleResolutionKind::NodeNext),
        module_suffixes: vec![String::new()],
        jsx: Some(crate::config::JsxEmit::Preserve),
        printer: crate::emitter::PrinterOptions {
            module: crate::emitter::ModuleKind::NodeNext,
            ..Default::default()
        },
        ..Default::default()
    };
    let mut resolver = ModuleResolver::new(&options);

    let request = ModuleLookupRequest {
        specifier: "./foo",
        containing_file: &dir.join("src/index.mts"),
        specifier_span: Span::new(22, 28),
        import_kind: ImportKind::EsmImport,
        resolution_mode_override: None,
        no_implicit_any: false,
        implied_classic_resolution: false,
    };
    let result = resolver.lookup(&request, |_, _| None, |_| false, None);

    assert!(
        result.resolved_path.is_none(),
        "should not resolve without extension"
    );
    let error = result.error.expect("should have an error");
    assert_eq!(error.code, IMPORT_PATH_NEEDS_EXTENSION_SUGGESTION);
    assert!(
        error.message.contains("./foo.jsx"),
        "should suggest .jsx for tsx preserve: {}",
        error.message
    );

    let _ = fs::remove_dir_all(&dir);
}

// TODO: TSX resolution is too permissive -- resolves ./foo to ./foo.tsx without
// requiring explicit extension in NodeNext mode.
#[test]
fn test_lookup_extension_suggestion_tsx_react_uses_js() {
    use std::fs;
    let dir = std::env::temp_dir().join("tsz_lookup_ext_suggestion_tsx_react");
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("src")).unwrap();
    fs::write(dir.join("src/foo.tsx"), "export const foo = <div />;").unwrap();
    fs::write(dir.join("src/index.mts"), "import { foo } from './foo';").unwrap();
    fs::write(dir.join("package.json"), r#"{"type": "module"}"#).unwrap();

    let options = ResolvedCompilerOptions {
        module_resolution: Some(ModuleResolutionKind::NodeNext),
        module_suffixes: vec![String::new()],
        jsx: Some(crate::config::JsxEmit::React),
        printer: crate::emitter::PrinterOptions {
            module: crate::emitter::ModuleKind::NodeNext,
            ..Default::default()
        },
        ..Default::default()
    };
    let mut resolver = ModuleResolver::new(&options);

    let request = ModuleLookupRequest {
        specifier: "./foo",
        containing_file: &dir.join("src/index.mts"),
        specifier_span: Span::new(22, 28),
        import_kind: ImportKind::EsmImport,
        resolution_mode_override: None,
        no_implicit_any: false,
        implied_classic_resolution: false,
    };
    let result = resolver.lookup(&request, |_, _| None, |_| false, None);

    assert!(
        result.resolved_path.is_none(),
        "should not resolve without extension"
    );
    let error = result.error.expect("should have an error");
    assert_eq!(error.code, IMPORT_PATH_NEEDS_EXTENSION_SUGGESTION);
    assert!(
        error.message.contains("./foo.js"),
        "should suggest .js for tsx react: {}",
        error.message
    );

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_lookup_cjs_esm_mismatch_classic_resolution() {
    // TS2792: classic resolution + a matching `node_modules/<pkg>` ancestor
    // should produce moduleResolution mismatch hint.
    let dir = std::env::temp_dir().join("tsz_lookup_cjs_esm_mismatch");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("src")).unwrap();
    std::fs::create_dir_all(dir.join("node_modules").join("nonexistent")).unwrap();
    std::fs::write(dir.join("src/index.ts"), "import 'nonexistent';").unwrap();

    let options = ResolvedCompilerOptions {
        module_resolution: Some(ModuleResolutionKind::Classic),
        module_suffixes: vec![String::new()],
        ..Default::default()
    };
    let mut resolver = ModuleResolver::new(&options);

    let request = ModuleLookupRequest {
        specifier: "nonexistent",
        containing_file: &dir.join("src/index.ts"),
        specifier_span: Span::new(8, 20),
        import_kind: ImportKind::EsmImport,
        resolution_mode_override: None,
        no_implicit_any: false,
        implied_classic_resolution: true,
    };
    let result = resolver.lookup(&request, |_, _| None, |_| false, None);

    let error = result.error.expect("should have an error");
    assert_eq!(
        error.code, MODULE_RESOLUTION_MODE_MISMATCH,
        "classic resolution with matching node_modules/<pkg> should produce TS2792, got TS{}",
        error.code
    );
    assert!(
        error.message.contains("'moduleResolution'"),
        "message should suggest moduleResolution: {}",
        error.message
    );

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn test_lookup_json_module_without_flag() {
    // TS2732: importing .json without resolveJsonModule
    use std::fs;
    let dir = std::env::temp_dir().join("tsz_lookup_json_module");
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("src")).unwrap();
    fs::write(dir.join("src/data.json"), r#"{"key": "value"}"#).unwrap();
    fs::write(dir.join("src/index.ts"), "import data from './data.json';").unwrap();

    let options = ResolvedCompilerOptions {
        module_resolution: Some(ModuleResolutionKind::Node),
        resolve_json_module: false,
        module_suffixes: vec![String::new()],
        ..Default::default()
    };
    let mut resolver = ModuleResolver::new(&options);

    let request = ModuleLookupRequest {
        specifier: "./data.json",
        containing_file: &dir.join("src/index.ts"),
        specifier_span: Span::new(18, 30),
        import_kind: ImportKind::EsmImport,
        resolution_mode_override: None,
        no_implicit_any: false,
        implied_classic_resolution: false,
    };
    let result = resolver.lookup(&request, |_, _| None, |_| false, None);

    let error = result.error.expect("should have an error");
    assert_eq!(
        error.code, JSON_MODULE_WITHOUT_RESOLVE_JSON_MODULE,
        "should emit TS2732 for .json without resolveJsonModule, got TS{}",
        error.code
    );

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_lookup_ambient_module_suppresses_error() {
    // Ambient module declarations should suppress resolution errors
    let dir = std::env::temp_dir().join("tsz_lookup_ambient");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("src")).unwrap();
    std::fs::write(dir.join("src/index.ts"), "import 'my-ambient';").unwrap();

    let options = ResolvedCompilerOptions {
        module_resolution: Some(ModuleResolutionKind::Node),
        module_suffixes: vec![String::new()],
        ..Default::default()
    };
    let mut resolver = ModuleResolver::new(&options);

    let request = ModuleLookupRequest {
        specifier: "my-ambient",
        containing_file: &dir.join("src/index.ts"),
        specifier_span: Span::new(8, 19),
        import_kind: ImportKind::EsmImport,
        resolution_mode_override: None,
        no_implicit_any: false,
        implied_classic_resolution: false,
    };
    let result = resolver.lookup(&request, |_, _| None, |spec| spec == "my-ambient", None);

    assert!(result.resolved_path.is_none(), "ambient has no file path");
    assert!(
        result.treat_as_resolved,
        "ambient should be treated as resolved"
    );
    assert!(result.error.is_none(), "ambient should have no error");

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn test_lookup_untyped_js_module_no_implicit_any() {
    // TS7016: untyped JS module in node_modules with noImplicitAny
    use std::fs;
    let dir = std::env::temp_dir().join("tsz_lookup_untyped_js");
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("node_modules/untyped")).unwrap();
    fs::create_dir_all(dir.join("src")).unwrap();
    fs::write(
        dir.join("node_modules/untyped/package.json"),
        r#"{"name":"untyped"}"#,
    )
    .unwrap();
    fs::write(
        dir.join("node_modules/untyped/index.js"),
        "module.exports = {};",
    )
    .unwrap();
    fs::write(dir.join("src/index.ts"), "import 'untyped';").unwrap();

    let options = ResolvedCompilerOptions {
        module_resolution: Some(ModuleResolutionKind::Node),
        module_suffixes: vec![String::new()],
        ..Default::default()
    };
    let mut resolver = ModuleResolver::new(&options);

    // With noImplicitAny: should get TS7016
    let request = ModuleLookupRequest {
        specifier: "untyped",
        containing_file: &dir.join("src/index.ts"),
        specifier_span: Span::new(8, 16),
        import_kind: ImportKind::EsmImport,
        resolution_mode_override: None,
        no_implicit_any: true,
        implied_classic_resolution: false,
    };
    let result = resolver.lookup(&request, |_, _| None, |_| false, None);
    let outcome = result.classify();
    assert!(
        outcome.is_resolved,
        "untyped JS should be treated as resolved (no TS2307)"
    );
    let error = outcome.error.expect("noImplicitAny should produce TS7016");
    assert_eq!(error.code, COULD_NOT_FIND_DECLARATION_FILE);

    // Without noImplicitAny: should be resolved with no error
    resolver.clear_cache();
    let request_no_strict = ModuleLookupRequest {
        specifier: "untyped",
        containing_file: &dir.join("src/index.ts"),
        specifier_span: Span::new(8, 16),
        import_kind: ImportKind::EsmImport,
        resolution_mode_override: None,
        no_implicit_any: false,
        implied_classic_resolution: false,
    };
    let result2 = resolver.lookup(&request_no_strict, |_, _| None, |_| false, None);
    let outcome2 = result2.classify();
    assert!(
        outcome2.is_resolved,
        "untyped JS should be resolved without error"
    );
    assert!(
        outcome2.error.is_none(),
        "without noImplicitAny, no error expected"
    );

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_lookup_fallback_success() {
    // Fallback resolver provides the file when primary fails
    let dir = std::env::temp_dir().join("tsz_lookup_fallback");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("src")).unwrap();
    std::fs::write(dir.join("src/index.ts"), "import 'virtual-mod';").unwrap();

    let options = ResolvedCompilerOptions {
        module_resolution: Some(ModuleResolutionKind::Node),
        module_suffixes: vec![String::new()],
        ..Default::default()
    };
    let mut resolver = ModuleResolver::new(&options);

    let fake_target = dir.join("src/virtual.d.ts");
    std::fs::write(&fake_target, "export {};").unwrap();
    let fake_target_clone = fake_target;

    let request = ModuleLookupRequest {
        specifier: "virtual-mod",
        containing_file: &dir.join("src/index.ts"),
        specifier_span: Span::new(8, 20),
        import_kind: ImportKind::EsmImport,
        resolution_mode_override: None,
        no_implicit_any: false,
        implied_classic_resolution: false,
    };
    let result = resolver.lookup(&request, |_, _| Some(fake_target_clone), |_| false, None);

    assert!(
        result.resolved_path.is_some(),
        "fallback should provide resolved path"
    );
    assert!(
        result.error.is_none(),
        "successful fallback should have no error"
    );

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn test_lookup_node16_esm_extensionless_fallback_error() {
    // Node16/NodeNext: extensionless relative import in ESM context
    // should fail even if fallback finds the file
    use std::fs;
    let dir = std::env::temp_dir().join("tsz_lookup_n16_ext_fallback");
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("src")).unwrap();
    fs::write(dir.join("src/utils.ts"), "export const x = 1;").unwrap();
    fs::write(dir.join("src/index.mts"), "import { x } from './utils';").unwrap();

    let options = ResolvedCompilerOptions {
        module_resolution: Some(ModuleResolutionKind::Node16),
        module_suffixes: vec![String::new()],
        printer: crate::emitter::PrinterOptions {
            module: crate::emitter::ModuleKind::Node16,
            ..Default::default()
        },
        ..Default::default()
    };
    let mut resolver = ModuleResolver::new(&options);

    let fake_target = dir.join("src/utils.ts");
    let fake_target_clone = fake_target;

    let request = ModuleLookupRequest {
        specifier: "./utils",
        containing_file: &dir.join("src/index.mts"),
        specifier_span: Span::new(22, 30),
        import_kind: ImportKind::EsmImport,
        resolution_mode_override: None,
        no_implicit_any: false,
        implied_classic_resolution: false,
    };
    // Primary resolution emits TS2835 (extension suggestion).
    // Even if fallback would find the file, the primary error takes priority.
    let result = resolver.lookup(&request, |_, _| Some(fake_target_clone), |_| false, None);

    assert!(
        result.resolved_path.is_none(),
        "ESM extensionless should not resolve"
    );
    let error = result.error.expect("should have an extension error");
    assert!(
        error.code == IMPORT_PATH_NEEDS_EXTENSION_SUGGESTION
            || error.code == IMPORT_PATH_NEEDS_EXTENSION,
        "should be TS2834 or TS2835, got TS{}",
        error.code
    );

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_lookup_package_exports_subpath() {
    // Package exports resolution via lookup
    use std::fs;
    let dir = std::env::temp_dir().join("tsz_lookup_pkg_exports");
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("node_modules/pkg")).unwrap();
    fs::create_dir_all(dir.join("src")).unwrap();

    fs::write(
        dir.join("node_modules/pkg/package.json"),
        r#"{"name":"pkg","exports":{".":"./main.js","./utils":"./utils.js"}}"#,
    )
    .unwrap();
    fs::write(dir.join("node_modules/pkg/main.d.ts"), "export {};").unwrap();
    fs::write(
        dir.join("node_modules/pkg/utils.d.ts"),
        "export const u: number;",
    )
    .unwrap();
    fs::write(dir.join("src/index.ts"), "import 'pkg/utils';").unwrap();

    let options = ResolvedCompilerOptions {
        module_resolution: Some(ModuleResolutionKind::Node16),
        resolve_package_json_exports: true,
        module_suffixes: vec![String::new()],
        printer: crate::emitter::PrinterOptions {
            module: crate::emitter::ModuleKind::Node16,
            ..Default::default()
        },
        ..Default::default()
    };
    let mut resolver = ModuleResolver::new(&options);

    let request = ModuleLookupRequest {
        specifier: "pkg/utils",
        containing_file: &dir.join("src/index.ts"),
        specifier_span: Span::new(8, 19),
        import_kind: ImportKind::EsmImport,
        resolution_mode_override: None,
        no_implicit_any: false,
        implied_classic_resolution: false,
    };
    let result = resolver.lookup(&request, |_, _| None, |_| false, None);

    assert!(
        result.resolved_path.is_some(),
        "package exports subpath should resolve: {:?}",
        result.error
    );
    assert!(result.error.is_none());

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_lookup_resolution_mode_override_selects_import_condition() {
    use std::fs;

    let dir = std::env::temp_dir().join("tsz_lookup_resolution_mode_override");
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("node_modules/pkg")).unwrap();
    fs::create_dir_all(dir.join("src")).unwrap();

    fs::write(
        dir.join("node_modules/pkg/package.json"),
        r#"{"name":"pkg","exports":{".":{"import":"./esm.d.ts","require":"./missing.d.cts"}}}"#,
    )
    .unwrap();
    fs::write(
        dir.join("node_modules/pkg/esm.d.ts"),
        "export type Foo = 1;",
    )
    .unwrap();
    fs::write(dir.join("src/index.cts"), "import type { Foo } from 'pkg';").unwrap();

    let options = ResolvedCompilerOptions {
        module_resolution: Some(ModuleResolutionKind::Node16),
        resolve_package_json_exports: true,
        module_suffixes: vec![String::new()],
        printer: crate::emitter::PrinterOptions {
            module: crate::emitter::ModuleKind::Node16,
            ..Default::default()
        },
        ..Default::default()
    };
    let mut resolver = ModuleResolver::new(&options);

    let request_without_override = ModuleLookupRequest {
        specifier: "pkg",
        containing_file: &dir.join("src/index.cts"),
        specifier_span: Span::new(24, 29),
        import_kind: ImportKind::EsmImport,
        resolution_mode_override: None,
        no_implicit_any: false,
        implied_classic_resolution: false,
    };
    let result_without_override =
        resolver.lookup(&request_without_override, |_, _| None, |_| false, None);
    assert!(
        result_without_override.error.is_some(),
        "CJS implied mode should follow the missing require condition"
    );

    resolver.clear_cache();

    let request_with_override = ModuleLookupRequest {
        specifier: "pkg",
        containing_file: &dir.join("src/index.cts"),
        specifier_span: Span::new(24, 29),
        import_kind: ImportKind::EsmImport,
        resolution_mode_override: Some(ImportingModuleKind::Esm),
        no_implicit_any: false,
        implied_classic_resolution: false,
    };
    let result_with_override =
        resolver.lookup(&request_with_override, |_, _| None, |_| false, None);

    assert!(
        result_with_override.resolved_path.is_some(),
        "resolution-mode import override should select the import condition: {:?}",
        result_with_override.error
    );
    assert!(result_with_override.error.is_none());

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_lookup_bundler_does_not_default_to_browser_condition() {
    // Per tsc 6.0, `moduleResolution: "bundler"` does NOT add `browser` to
    // the default condition set. The `browser` condition must be opted in
    // via `customConditions`. Without it, bundler should fall through the
    // conditional `exports` map to `default`.
    use std::fs;

    let dir = std::env::temp_dir().join("tsz_lookup_bundler_browser_default_off");
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("node_modules/pkg")).unwrap();
    fs::create_dir_all(dir.join("src")).unwrap();

    fs::write(
        dir.join("node_modules/pkg/package.json"),
        r#"{"name":"pkg","exports":{".":{"browser":"./browser.d.ts","default":"./default.d.ts"}}}"#,
    )
    .unwrap();
    fs::write(
        dir.join("node_modules/pkg/browser.d.ts"),
        "export const widget: \"browser\";",
    )
    .unwrap();
    fs::write(
        dir.join("node_modules/pkg/default.d.ts"),
        "export const widget: \"default\";",
    )
    .unwrap();
    fs::write(dir.join("src/index.ts"), "import { widget } from 'pkg';").unwrap();

    let options = ResolvedCompilerOptions {
        module_resolution: Some(ModuleResolutionKind::Bundler),
        resolve_package_json_exports: true,
        module_suffixes: vec![String::new()],
        ..Default::default()
    };
    let mut resolver = ModuleResolver::new(&options);

    let request = ModuleLookupRequest {
        specifier: "pkg",
        containing_file: &dir.join("src/index.ts"),
        specifier_span: Span::new(24, 29),
        import_kind: ImportKind::EsmImport,
        resolution_mode_override: None,
        no_implicit_any: false,
        implied_classic_resolution: false,
    };

    let result = resolver.lookup(&request, |_, _| None, |_| false, None);

    let resolved = result
        .resolved_path
        .expect("bundler resolution should match `default` when `browser` is not opted in");
    assert_eq!(resolved, dir.join("node_modules/pkg/default.d.ts"));
    assert!(result.error.is_none());

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_lookup_bundler_uses_browser_when_in_custom_conditions() {
    // Opting `browser` into `customConditions` re-enables it for bundler.
    use std::fs;

    let dir = std::env::temp_dir().join("tsz_lookup_bundler_browser_via_custom");
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("node_modules/pkg")).unwrap();
    fs::create_dir_all(dir.join("src")).unwrap();

    fs::write(
        dir.join("node_modules/pkg/package.json"),
        r#"{"name":"pkg","exports":{".":{"browser":"./browser.d.ts","default":"./default.d.ts"}}}"#,
    )
    .unwrap();
    fs::write(
        dir.join("node_modules/pkg/browser.d.ts"),
        "export const widget: \"browser\";",
    )
    .unwrap();
    fs::write(
        dir.join("node_modules/pkg/default.d.ts"),
        "export const widget: \"default\";",
    )
    .unwrap();
    fs::write(dir.join("src/index.ts"), "import { widget } from 'pkg';").unwrap();

    let options = ResolvedCompilerOptions {
        module_resolution: Some(ModuleResolutionKind::Bundler),
        resolve_package_json_exports: true,
        module_suffixes: vec![String::new()],
        custom_conditions: vec!["browser".to_string()],
        ..Default::default()
    };
    let mut resolver = ModuleResolver::new(&options);

    let request = ModuleLookupRequest {
        specifier: "pkg",
        containing_file: &dir.join("src/index.ts"),
        specifier_span: Span::new(24, 29),
        import_kind: ImportKind::EsmImport,
        resolution_mode_override: None,
        no_implicit_any: false,
        implied_classic_resolution: false,
    };

    let result = resolver.lookup(&request, |_, _| None, |_| false, None);

    let resolved = result
        .resolved_path
        .expect("opting `browser` into customConditions should select the browser branch");
    assert_eq!(resolved, dir.join("node_modules/pkg/browser.d.ts"));
    assert!(result.error.is_none());

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_lookup_module_preserve_uses_syntax_directed_conditions() {
    use std::fs;

    let dir = std::env::temp_dir().join("tsz_lookup_module_preserve_conditions");
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("node_modules/pkg")).unwrap();
    fs::create_dir_all(dir.join("src")).unwrap();

    fs::write(
        dir.join("node_modules/pkg/package.json"),
        r#"{"name":"pkg","exports":{".":{"import":"./esm.d.ts","require":"./cjs.d.ts"}}}"#,
    )
    .unwrap();
    fs::write(
        dir.join("node_modules/pkg/esm.d.ts"),
        r#"export const esm: "esm";"#,
    )
    .unwrap();
    fs::write(
        dir.join("node_modules/pkg/cjs.d.ts"),
        r#"declare const cjs: "cjs"; export = cjs;"#,
    )
    .unwrap();
    fs::write(dir.join("src/index.ts"), "import { esm } from 'pkg';").unwrap();

    let options = ResolvedCompilerOptions {
        module_resolution: Some(ModuleResolutionKind::Node16),
        resolve_package_json_exports: true,
        module_suffixes: vec![String::new()],
        printer: crate::emitter::PrinterOptions {
            module: crate::emitter::ModuleKind::Preserve,
            ..Default::default()
        },
        ..Default::default()
    };
    let mut resolver = ModuleResolver::new(&options);

    let esm_request = ModuleLookupRequest {
        specifier: "pkg",
        containing_file: &dir.join("src/index.ts"),
        specifier_span: Span::new(19, 24),
        import_kind: ImportKind::EsmImport,
        resolution_mode_override: None,
        no_implicit_any: false,
        implied_classic_resolution: false,
    };
    let esm_result = resolver.lookup(&esm_request, |_, _| None, |_| false, None);
    let esm_path = esm_result
        .resolved_path
        .expect("module preserve import should select the import condition");
    assert!(
        esm_path.ends_with("esm.d.ts"),
        "expected import condition path, got {}",
        esm_path.display()
    );

    resolver.clear_cache();

    let cjs_request = ModuleLookupRequest {
        specifier: "pkg",
        containing_file: &dir.join("src/index.ts"),
        specifier_span: Span::new(19, 24),
        import_kind: ImportKind::CjsRequire,
        resolution_mode_override: None,
        no_implicit_any: false,
        implied_classic_resolution: false,
    };
    let cjs_result = resolver.lookup(&cjs_request, |_, _| None, |_| false, None);
    let cjs_path = cjs_result
        .resolved_path
        .expect("module preserve require should select the require condition");
    assert!(
        cjs_path.ends_with("cjs.d.ts"),
        "expected require condition path, got {}",
        cjs_path.display()
    );

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_lookup_module_preserve_honors_forced_cts_and_mts_conditions() {
    use std::fs;

    let dir = std::env::temp_dir().join("tsz_lookup_module_preserve_forced_conditions");
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("node_modules/pkg")).unwrap();
    fs::create_dir_all(dir.join("src")).unwrap();

    fs::write(
        dir.join("node_modules/pkg/package.json"),
        r#"{"name":"pkg","exports":{".":{"import":"./esm.d.ts","require":"./cjs.d.ts"}}}"#,
    )
    .unwrap();
    fs::write(
        dir.join("node_modules/pkg/esm.d.ts"),
        r#"export const esm: "esm";"#,
    )
    .unwrap();
    fs::write(
        dir.join("node_modules/pkg/cjs.d.ts"),
        r#"export const cjs: "cjs";"#,
    )
    .unwrap();
    fs::write(dir.join("src/index.mts"), "import { esm } from 'pkg';").unwrap();
    fs::write(dir.join("src/index.cts"), "import { cjs } from 'pkg';").unwrap();

    let options = ResolvedCompilerOptions {
        module_resolution: Some(ModuleResolutionKind::Node16),
        resolve_package_json_exports: true,
        module_suffixes: vec![String::new()],
        printer: crate::emitter::PrinterOptions {
            module: crate::emitter::ModuleKind::Preserve,
            ..Default::default()
        },
        ..Default::default()
    };
    let mut resolver = ModuleResolver::new(&options);

    let mts_request = ModuleLookupRequest {
        specifier: "pkg",
        containing_file: &dir.join("src/index.mts"),
        specifier_span: Span::new(19, 24),
        import_kind: ImportKind::EsmImport,
        resolution_mode_override: None,
        no_implicit_any: false,
        implied_classic_resolution: false,
    };
    let mts_result = resolver.lookup(&mts_request, |_, _| None, |_| false, None);
    let mts_path = mts_result
        .resolved_path
        .expect(".mts import should stay on the import condition");
    assert!(
        mts_path.ends_with("esm.d.ts"),
        "expected import condition path for .mts, got {}",
        mts_path.display()
    );

    resolver.clear_cache();

    let cts_request = ModuleLookupRequest {
        specifier: "pkg",
        containing_file: &dir.join("src/index.cts"),
        specifier_span: Span::new(19, 24),
        import_kind: ImportKind::EsmImport,
        resolution_mode_override: None,
        no_implicit_any: false,
        implied_classic_resolution: false,
    };
    let cts_result = resolver.lookup(&cts_request, |_, _| None, |_| false, None);
    let cts_path = cts_result
        .resolved_path
        .expect(".cts import should stay on the require condition");
    assert!(
        cts_path.ends_with("cjs.d.ts"),
        "expected require condition path for .cts, got {}",
        cts_path.display()
    );

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_lookup_versioned_types_condition_prefers_matching_export_branch() {
    use std::fs;

    let dir = std::env::temp_dir().join("tsz_lookup_versioned_types_condition");
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("node_modules/inner")).unwrap();
    fs::create_dir_all(dir.join("src")).unwrap();

    fs::write(
        dir.join("node_modules/inner/package.json"),
        r#"{
            "name":"inner",
            "exports":{
                ".":{
                    "types@>=10000":"./future-types.d.ts",
                    "types@>=1":"./new-types.d.ts",
                    "types":"./old-types.d.ts",
                    "import":"./index.mjs",
                    "node":"./index.js"
                }
            }
        }"#,
    )
    .unwrap();
    fs::write(
        dir.join("node_modules/inner/old-types.d.ts"),
        "export const oldThing: number;",
    )
    .unwrap();
    fs::write(
        dir.join("node_modules/inner/new-types.d.ts"),
        "export const goodThing: number;",
    )
    .unwrap();
    fs::write(dir.join("src/index.ts"), "import * as mod from 'inner';").unwrap();

    let options = ResolvedCompilerOptions {
        module_resolution: Some(ModuleResolutionKind::Node16),
        resolve_package_json_exports: true,
        module_suffixes: vec![String::new()],
        printer: crate::emitter::PrinterOptions {
            module: crate::emitter::ModuleKind::Node16,
            ..Default::default()
        },
        checker: crate::checker::context::CheckerOptions {
            module: crate::emitter::ModuleKind::Node16,
            ..Default::default()
        },
        ..Default::default()
    };
    let mut resolver = ModuleResolver::new(&options);

    let request = ModuleLookupRequest {
        specifier: "inner",
        containing_file: &dir.join("src/index.ts"),
        specifier_span: Span::new(22, 29),
        import_kind: ImportKind::EsmImport,
        resolution_mode_override: None,
        no_implicit_any: false,
        implied_classic_resolution: false,
    };
    let result = resolver.lookup(&request, |_, _| None, |_| false, None);

    let resolved_path = result
        .resolved_path
        .expect("versioned types condition should resolve");
    assert!(
        resolved_path.ends_with("new-types.d.ts"),
        "expected versioned types branch, got {}",
        resolved_path.display()
    );

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_lookup_ts5097_ts_extension_not_found() {
    // TS5097: Import path ends with a TypeScript extension (.ts)
    // without allowImportingTsExtensions enabled.
    // When the specifier has an explicit .ts extension and the file is NOT found,
    // resolve_with_kind upgrades NotFound -> ImportingTsExtensionNotAllowed.
    // lookup() then propagates this as a TS5097 error via classify().
    use std::fs;
    let dir = std::env::temp_dir().join("tsz_test_lookup_ts5097_nf");
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();

    // Only create main.ts — utils.ts does NOT exist
    fs::write(dir.join("main.ts"), "import { foo } from './utils.ts';").unwrap();

    let options = ResolvedCompilerOptions {
        module_resolution: Some(ModuleResolutionKind::Node),
        allow_importing_ts_extensions: false,
        ..Default::default()
    };
    let mut resolver = ModuleResolver::new(&options);

    let request = ModuleLookupRequest {
        specifier: "./utils.ts",
        containing_file: &dir.join("main.ts"),
        specifier_span: Span::new(20, 32),
        import_kind: ImportKind::EsmImport,
        resolution_mode_override: None,
        no_implicit_any: false,
        implied_classic_resolution: false,
    };

    let result = resolver.lookup(&request, |_, _| None, |_| false, None);
    let outcome = result.classify();

    let error = outcome
        .error
        .expect("Expected TS5097 error for .ts extension import");
    assert_eq!(
        error.code, IMPORT_PATH_TS_EXTENSION_NOT_ALLOWED,
        "Expected TS5097 but got TS{}",
        error.code
    );
    assert!(
        error.message.contains("allowImportingTsExtensions"),
        "Error message should mention allowImportingTsExtensions: {}",
        error.message
    );

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_lookup_ts5097_mts_extension_not_found() {
    // TS5097 for .mts extension (not just .ts).
    // When the specifier has an explicit .mts extension and the file is NOT found,
    // we should get TS5097 instead of TS2307.
    use std::fs;
    let dir = std::env::temp_dir().join("tsz_test_lookup_ts5097_mts_nf");
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();

    fs::write(dir.join("main.ts"), "").unwrap();
    // utils.mts does NOT exist

    let options = ResolvedCompilerOptions {
        module_resolution: Some(ModuleResolutionKind::Node),
        allow_importing_ts_extensions: false,
        ..Default::default()
    };
    let mut resolver = ModuleResolver::new(&options);

    let request = ModuleLookupRequest {
        specifier: "./utils.mts",
        containing_file: &dir.join("main.ts"),
        specifier_span: Span::new(20, 33),
        import_kind: ImportKind::EsmImport,
        resolution_mode_override: None,
        no_implicit_any: false,
        implied_classic_resolution: false,
    };

    let result = resolver.lookup(&request, |_, _| None, |_| false, None);
    let outcome = result.classify();

    let error = outcome
        .error
        .expect("Expected TS5097 error for .mts extension import");
    assert_eq!(
        error.code, IMPORT_PATH_TS_EXTENSION_NOT_ALLOWED,
        "Expected TS5097 for .mts but got TS{}",
        error.code
    );

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_lookup_ts_extension_resolves_when_file_exists() {
    // When ./utils.ts is imported and the file EXISTS, the resolution succeeds
    // (no TS5097 error in this case — the current implementation only upgrades
    // NotFound to TS5097).
    use std::fs;
    let dir = std::env::temp_dir().join("tsz_test_lookup_ts_ext_exists");
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();

    fs::write(dir.join("main.ts"), "import { foo } from './utils.ts';").unwrap();
    fs::write(dir.join("utils.ts"), "export const foo = 42;").unwrap();

    let options = ResolvedCompilerOptions {
        module_resolution: Some(ModuleResolutionKind::Node),
        allow_importing_ts_extensions: false,
        ..Default::default()
    };
    let mut resolver = ModuleResolver::new(&options);

    let request = ModuleLookupRequest {
        specifier: "./utils.ts",
        containing_file: &dir.join("main.ts"),
        specifier_span: Span::new(20, 32),
        import_kind: ImportKind::EsmImport,
        resolution_mode_override: None,
        no_implicit_any: false,
        implied_classic_resolution: false,
    };

    let result = resolver.lookup(&request, |_, _| None, |_| false, None);
    let outcome = result.classify();

    // When the file exists, Node resolution resolves it successfully
    assert!(
        outcome.resolved_path.is_some(),
        "Should resolve when file exists even with .ts extension"
    );

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_lookup_ts2307_plain_no_upgrade() {
    // When a relative module is not found and no upgrade conditions are met
    // (not .json, not implied_classic_resolution, not ambient, no JS file),
    // lookup() should produce plain TS2307.
    use std::fs;
    let dir = std::env::temp_dir().join("tsz_test_lookup_ts2307_plain_v2");
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();

    fs::write(dir.join("main.ts"), "import { foo } from './nonexistent';").unwrap();

    let options = ResolvedCompilerOptions {
        module_resolution: Some(ModuleResolutionKind::Node),
        ..Default::default()
    };
    let mut resolver = ModuleResolver::new(&options);

    let request = ModuleLookupRequest {
        specifier: "./nonexistent",
        containing_file: &dir.join("main.ts"),
        specifier_span: Span::new(20, 35),
        import_kind: ImportKind::EsmImport,
        resolution_mode_override: None,
        no_implicit_any: false,
        implied_classic_resolution: false,
    };

    let result = resolver.lookup(&request, |_, _| None, |_| false, None);
    let outcome = result.classify();

    assert!(!outcome.is_resolved, "Not-found should not be resolved");
    let error = outcome.error.expect("Expected TS2307 error");
    assert_eq!(
        error.code, CANNOT_FIND_MODULE,
        "Expected TS2307 but got TS{}",
        error.code
    );

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_lookup_ts2732_nonexistent_json_without_resolve_json_module() {
    // When a .json import specifier does NOT resolve to any file on disk and
    // resolveJsonModule is false, lookup() should upgrade NotFound -> TS2732.
    // This exercises the upgrade branch in lookup() that catches NotFound for
    // .json specifiers after fallback fails.
    use std::fs;
    let dir = std::env::temp_dir().join("tsz_lookup_ts2732_nonexistent");
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("index.ts"), "import './missing.json';").unwrap();
    // Intentionally do NOT create missing.json

    let options = ResolvedCompilerOptions {
        module_resolution: Some(ModuleResolutionKind::Node),
        resolve_json_module: false,
        module_suffixes: vec![String::new()],
        ..Default::default()
    };
    let mut resolver = ModuleResolver::new(&options);

    let request = ModuleLookupRequest {
        specifier: "./missing.json",
        containing_file: &dir.join("index.ts"),
        specifier_span: Span::new(8, 22),
        import_kind: ImportKind::EsmImport,
        resolution_mode_override: None,
        no_implicit_any: false,
        implied_classic_resolution: false,
    };
    let result = resolver.lookup(&request, |_, _| None, |_| false, None);
    let outcome = result.classify();

    assert!(
        !outcome.is_resolved,
        "Nonexistent .json should not be resolved"
    );
    let error = outcome
        .error
        .expect("Expected error for nonexistent .json import");
    assert_eq!(
        error.code, JSON_MODULE_WITHOUT_RESOLVE_JSON_MODULE,
        "Expected TS2732 for nonexistent .json without resolveJsonModule, got TS{}",
        error.code
    );

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_lookup_ts2732_not_emitted_when_resolve_json_module_enabled() {
    // When resolveJsonModule is true but the .json file doesn't exist,
    // the error should be plain TS2307, not TS2732.
    use std::fs;
    let dir = std::env::temp_dir().join("tsz_lookup_ts2732_enabled");
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("index.ts"), "import './missing.json';").unwrap();

    let options = ResolvedCompilerOptions {
        module_resolution: Some(ModuleResolutionKind::Node),
        resolve_json_module: true,
        module_suffixes: vec![String::new()],
        ..Default::default()
    };
    let mut resolver = ModuleResolver::new(&options);

    let request = ModuleLookupRequest {
        specifier: "./missing.json",
        containing_file: &dir.join("index.ts"),
        specifier_span: Span::new(8, 22),
        import_kind: ImportKind::EsmImport,
        resolution_mode_override: None,
        no_implicit_any: false,
        implied_classic_resolution: false,
    };
    let result = resolver.lookup(&request, |_, _| None, |_| false, None);
    let outcome = result.classify();

    let error = outcome.error.expect("Expected error for missing .json");
    assert_eq!(
        error.code, CANNOT_FIND_MODULE,
        "Expected TS2307 (not TS2732) when resolveJsonModule is enabled, got TS{}",
        error.code
    );

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_lookup_path_mapping_failure_produces_ts2307_via_lookup() {
    // When path mappings are configured but fail to resolve, lookup() should
    // produce TS2307 (not panic or silently succeed).
    use std::fs;
    let dir = std::env::temp_dir().join("tsz_lookup_path_mapping_fail");
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("src")).unwrap();
    fs::write(dir.join("src/index.ts"), "import '@app/missing';").unwrap();

    let options = ResolvedCompilerOptions {
        module_resolution: Some(ModuleResolutionKind::Node),
        base_url: Some(dir.clone()),
        paths: Some(vec![PathMapping {
            pattern: "@app/*".to_string(),
            prefix: "@app/".to_string(),
            suffix: String::new(),
            targets: vec!["src/*".to_string()],
        }]),
        module_suffixes: vec![String::new()],
        ..Default::default()
    };
    let mut resolver = ModuleResolver::new(&options);

    let request = ModuleLookupRequest {
        specifier: "@app/missing",
        containing_file: &dir.join("src/index.ts"),
        specifier_span: Span::new(8, 22),
        import_kind: ImportKind::EsmImport,
        resolution_mode_override: None,
        no_implicit_any: false,
        implied_classic_resolution: false,
    };
    let result = resolver.lookup(&request, |_, _| None, |_| false, None);
    let outcome = result.classify();

    assert!(
        !outcome.is_resolved,
        "Failed path mapping should not be resolved"
    );
    let error = outcome
        .error
        .expect("Expected error for failed path mapping");
    assert_eq!(
        error.code, CANNOT_FIND_MODULE,
        "Expected TS2307 for failed path mapping, got TS{}",
        error.code
    );

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_path_mapping_extension_reflects_resolved_file() {
    // Regression: path-mapping previously classified the resolved module's
    // extension from the pre-resolution candidate (which always has no
    // extension at this point — targets with extensions are filtered
    // earlier). That made every path-mapping-resolved module carry
    // `ModuleExtension::Unknown` instead of the real `.ts` / `.d.ts` / etc.,
    // drifting from every other resolver exit (relative, node_modules,
    // exports, self-reference) which uses the resolved path. The fix
    // classifies on the resolved path.
    use std::fs;
    let dir = std::env::temp_dir().join("tsz_path_mapping_ext_regression");
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("src")).unwrap();
    fs::write(dir.join("src/index.ts"), "import '@app/widget';").unwrap();
    fs::write(dir.join("src/widget.ts"), "export const w = 1;").unwrap();

    let options = ResolvedCompilerOptions {
        module_resolution: Some(ModuleResolutionKind::Node),
        base_url: Some(dir.clone()),
        paths: Some(vec![PathMapping {
            pattern: "@app/*".to_string(),
            prefix: "@app/".to_string(),
            suffix: String::new(),
            targets: vec!["src/*".to_string()],
        }]),
        module_suffixes: vec![String::new()],
        ..Default::default()
    };
    let mut resolver = ModuleResolver::new(&options);
    let result = resolver.resolve("@app/widget", &dir.join("src/index.ts"), Span::new(8, 20));

    let module = result.expect("path mapping should resolve @app/widget to src/widget.ts");
    assert_eq!(module.resolved_path, dir.join("src/widget.ts"));
    assert_eq!(
        module.extension,
        ModuleExtension::Ts,
        "path-mapping must classify extension from the resolved file, not the extensionless candidate"
    );

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_lookup_fallback_rescues_not_found() {
    // When primary resolution fails but fallback succeeds, lookup() should
    // return resolved with no error.
    use std::fs;
    let dir = std::env::temp_dir().join("tsz_lookup_fallback_rescue");
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("main.ts"), "import './virtual';").unwrap();
    let virtual_path = dir.join("virtual.ts");
    fs::write(&virtual_path, "export const x = 1;").unwrap();

    let options = ResolvedCompilerOptions {
        module_resolution: Some(ModuleResolutionKind::Node),
        module_suffixes: vec![String::new()],
        ..Default::default()
    };
    let mut resolver = ModuleResolver::new(&options);

    let fallback_path = virtual_path;
    let request = ModuleLookupRequest {
        specifier: "./nonexistent-specifier",
        containing_file: &dir.join("main.ts"),
        specifier_span: Span::new(8, 18),
        import_kind: ImportKind::EsmImport,
        resolution_mode_override: None,
        no_implicit_any: false,
        implied_classic_resolution: false,
    };
    let result = resolver.lookup(&request, |_, _| Some(fallback_path), |_| false, None);
    let outcome = result.classify();

    assert!(outcome.is_resolved, "Fallback should mark as resolved");
    assert!(
        outcome.resolved_path.is_some(),
        "Fallback should provide a resolved path"
    );
    assert!(
        outcome.error.is_none(),
        "Fallback success should have no error"
    );

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_lookup_skips_fallback_for_nodenext_exports_authoritative_not_found() {
    use std::fs;
    let dir = std::env::temp_dir().join("tsz_lookup_skip_fallback_exports_not_found");
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("src")).unwrap();
    fs::create_dir_all(dir.join("node_modules/@types/dedent4")).unwrap();

    fs::write(
        dir.join("node_modules/@types/dedent4/package.json"),
        r#"{
            "name": "@types/dedent4",
            "version": "1.0.0",
            "main": "asdfasdfasdf",
            "exports": "./asdfasdfasdf"
        }"#,
    )
    .unwrap();
    let fallback_target = dir.join("node_modules/@types/dedent4/index.d.ts");
    fs::write(&fallback_target, "export {};").unwrap();
    fs::write(
        dir.join("src/index.mts"),
        "import dedent4 from 'dedent4';\ndedent4;\n",
    )
    .unwrap();

    let options = ResolvedCompilerOptions {
        module_resolution: Some(ModuleResolutionKind::NodeNext),
        resolve_package_json_exports: true,
        module_suffixes: vec![String::new()],
        ..Default::default()
    };
    let mut resolver = ModuleResolver::new(&options);

    let request = ModuleLookupRequest {
        specifier: "dedent4",
        containing_file: &dir.join("src/index.mts"),
        specifier_span: Span::new(22, 31),
        import_kind: ImportKind::EsmImport,
        resolution_mode_override: None,
        no_implicit_any: false,
        implied_classic_resolution: false,
    };

    let result = resolver.lookup(
        &request,
        |_, _| Some(fallback_target.clone()),
        |_| false,
        None,
    );
    let outcome = result.classify();

    assert!(
        !outcome.is_resolved,
        "Fallback must be skipped for exports-authoritative NotFound"
    );
    let error = outcome
        .error
        .expect("Expected TS2307 after skipping fallback");
    assert_eq!(
        error.code, CANNOT_FIND_MODULE,
        "Expected TS2307 when exports blocks resolution, got TS{}",
        error.code
    );

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_lookup_skips_fallback_for_bundler_exports_authoritative_not_found() {
    use std::fs;
    let dir = std::env::temp_dir().join("tsz_lookup_skip_fallback_bundler_exports_not_found");
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("src")).unwrap();
    fs::create_dir_all(dir.join("node_modules/pkg")).unwrap();

    fs::write(
        dir.join("node_modules/pkg/package.json"),
        r#"{
            "name": "pkg",
            "version": "1.0.0",
            "type": "commonjs",
            "exports": {
                "require": "./index.js"
            }
        }"#,
    )
    .unwrap();
    let fallback_target = dir.join("node_modules/pkg/index.d.ts");
    fs::write(&fallback_target, "export const x: number;").unwrap();
    fs::write(dir.join("src/index.mts"), "import { x } from 'pkg';\nx;\n").unwrap();

    let options = ResolvedCompilerOptions {
        module_resolution: Some(ModuleResolutionKind::Bundler),
        resolve_package_json_exports: true,
        module_suffixes: vec![String::new()],
        ..Default::default()
    };
    let mut resolver = ModuleResolver::new(&options);

    let request = ModuleLookupRequest {
        specifier: "pkg",
        containing_file: &dir.join("src/index.mts"),
        specifier_span: Span::new(18, 23),
        import_kind: ImportKind::EsmImport,
        resolution_mode_override: None,
        no_implicit_any: false,
        implied_classic_resolution: false,
    };

    let result = resolver.lookup(
        &request,
        |_, _| Some(fallback_target.clone()),
        |_| false,
        None,
    );
    let outcome = result.classify();

    assert!(
        !outcome.is_resolved,
        "Fallback must be skipped for exports-authoritative NotFound in Bundler mode"
    );
    let error = outcome
        .error
        .expect("Expected TS2307 after skipping fallback in Bundler mode");
    assert_eq!(
        error.code, CANNOT_FIND_MODULE,
        "Expected TS2307 when exports blocks resolution in Bundler mode, got TS{}",
        error.code
    );

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_lookup_skips_fallback_for_nodenext_literal_star_specifier_not_found() {
    use std::fs;
    let dir = std::env::temp_dir().join("tsz_lookup_skip_fallback_literal_star");
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("src")).unwrap();
    fs::create_dir_all(dir.join("node_modules/double-asterisk")).unwrap();

    fs::write(
        dir.join("node_modules/double-asterisk/package.json"),
        r#"{
            "name":"double-asterisk",
            "exports":{"./a/*/b/*/c/*":"./example.js"}
        }"#,
    )
    .unwrap();
    let fallback_target = dir.join("node_modules/double-asterisk/example.d.ts");
    fs::write(&fallback_target, "export {};").unwrap();
    fs::write(
        dir.join("src/index.mts"),
        "import {} from 'double-asterisk/a/*/b/*/c/*';\n",
    )
    .unwrap();

    let options = ResolvedCompilerOptions {
        module_resolution: Some(ModuleResolutionKind::NodeNext),
        resolve_package_json_exports: true,
        module_suffixes: vec![String::new()],
        ..Default::default()
    };
    let mut resolver = ModuleResolver::new(&options);

    let request = ModuleLookupRequest {
        specifier: "double-asterisk/a/*/b/*/c/*",
        containing_file: &dir.join("src/index.mts"),
        specifier_span: Span::new(16, 44),
        import_kind: ImportKind::EsmImport,
        resolution_mode_override: None,
        no_implicit_any: false,
        implied_classic_resolution: false,
    };

    let result = resolver.lookup(
        &request,
        |_, _| Some(fallback_target.clone()),
        |_| false,
        None,
    );
    let outcome = result.classify();

    assert!(
        !outcome.is_resolved,
        "Fallback must be skipped for literal '*' package specifier in NodeNext"
    );
    let error = outcome
        .error
        .expect("Expected TS2307 after skipping fallback for literal '*'");
    assert_eq!(
        error.code, CANNOT_FIND_MODULE,
        "Expected TS2307 for literal '*' package specifier, got TS{}",
        error.code
    );

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_lookup_should_try_fallback_not_for_hard_failures() {
    // Hard failures like ImportPathNeedsExtension should NOT trigger fallback.
    // Verify the should_try_fallback contract on failure variants.
    let hard_failures = vec![
        ResolutionFailure::ImportPathNeedsExtension {
            specifier: "./utils".to_string(),
            suggested_extension: ".js".to_string(),
            containing_file: "/app/index.mts".to_string(),
            span: Span::new(0, 10),
        },
        ResolutionFailure::ImportingTsExtensionNotAllowed {
            extension: ".ts".to_string(),
            containing_file: "/app/index.ts".to_string(),
            span: Span::new(0, 10),
        },
        ResolutionFailure::JsxNotEnabled {
            specifier: "./comp".to_string(),
            resolved_path: PathBuf::from("/app/comp.tsx"),
            containing_file: "/app/index.ts".to_string(),
            span: Span::new(0, 10),
        },
        ResolutionFailure::CircularResolution {
            message: "circular".to_string(),
            containing_file: "/app/index.ts".to_string(),
            span: Span::new(0, 10),
        },
        ResolutionFailure::InvalidSpecifier {
            message: "bad".to_string(),
            containing_file: "/app/index.ts".to_string(),
            span: Span::new(0, 10),
        },
    ];

    for failure in &hard_failures {
        assert!(
            !failure.should_try_fallback(),
            "Expected should_try_fallback=false for {:?}",
            std::mem::discriminant(failure)
        );
    }

    // Soft failures SHOULD trigger fallback
    let soft_failures = vec![
        ResolutionFailure::NotFound {
            specifier: "foo".to_string(),
            containing_file: "/app/index.ts".to_string(),
            span: Span::new(0, 10),
        },
        ResolutionFailure::PackageJsonError {
            message: "bad pkg".to_string(),
            containing_file: "/app/index.ts".to_string(),
            span: Span::new(0, 10),
        },
        ResolutionFailure::PathMappingFailed {
            message: "no match".to_string(),
            containing_file: "/app/index.ts".to_string(),
            span: Span::new(0, 10),
        },
        ResolutionFailure::ModuleResolutionModeMismatch {
            specifier: "pkg".to_string(),
            containing_file: "/app/index.ts".to_string(),
            span: Span::new(0, 10),
        },
    ];

    for failure in &soft_failures {
        assert!(
            failure.should_try_fallback(),
            "Expected should_try_fallback=true for {:?}",
            std::mem::discriminant(failure)
        );
    }
}

#[test]
fn test_lookup_classic_implied_resolution_upgrades_to_ts2792() {
    // Under classic-style resolution (module: amd|system|umd|none or
    // explicit moduleResolution: classic — issue #3077), bare specifiers
    // that fail to resolve always upgrade TS2307 → TS2792 to surface the
    // "Did you mean to set the 'moduleResolution' option to 'nodenext'..."
    // hint. The presence of an ancestor `node_modules/<pkg>/` directory is
    // not required: tsc emits TS2792 for missing packages regardless.
    use std::fs;
    let dir = std::env::temp_dir().join("tsz_lookup_classic_ts2792");
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    fs::create_dir_all(dir.join("node_modules").join("some-pkg")).unwrap();
    fs::write(dir.join("index.ts"), "import 'some-pkg';").unwrap();

    let options = ResolvedCompilerOptions {
        module_resolution: Some(ModuleResolutionKind::Node),
        module_suffixes: vec![String::new()],
        ..Default::default()
    };
    let mut resolver = ModuleResolver::new(&options);

    let request = ModuleLookupRequest {
        specifier: "some-pkg",
        containing_file: &dir.join("index.ts"),
        specifier_span: Span::new(8, 18),
        import_kind: ImportKind::EsmImport,
        resolution_mode_override: None,
        no_implicit_any: false,
        implied_classic_resolution: true,
    };
    let result = resolver.lookup(&request, |_, _| None, |_| false, None);
    let outcome = result.classify();

    assert!(!outcome.is_resolved);
    let error = outcome.error.expect("Expected error for missing module");
    assert_eq!(
        error.code, MODULE_RESOLUTION_MODE_MISMATCH,
        "Expected TS2792 for implied classic resolution with matching node_modules/<pkg>, got TS{}",
        error.code
    );
    assert!(
        error.message.contains("moduleResolution"),
        "TS2792 message should suggest moduleResolution option"
    );

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_lookup_classic_implied_resolution_without_node_modules_upgrades_to_ts2792() {
    // Even without a matching `node_modules/<pkg>/` ancestor, classic-style
    // resolution still upgrades TS2307 → TS2792 (issue #3077). Earlier
    // versions of this resolver gated the upgrade on node-style lookahead;
    // tsc 6.0.3 emits TS2792 unconditionally for bare specifiers under
    // classic resolution, so we no longer probe.
    //
    // Relative specifiers stay on plain TS2307 — see
    // `test_lookup_classic_implied_resolution_relative_keeps_ts2307` below.
    use std::fs;
    let dir = std::env::temp_dir().join("tsz_lookup_classic_ts2792_no_node_modules");
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("index.ts"), "import 'some-pkg';").unwrap();

    let options = ResolvedCompilerOptions {
        module_resolution: Some(ModuleResolutionKind::Node),
        module_suffixes: vec![String::new()],
        ..Default::default()
    };
    let mut resolver = ModuleResolver::new(&options);

    let request = ModuleLookupRequest {
        specifier: "some-pkg",
        containing_file: &dir.join("index.ts"),
        specifier_span: Span::new(8, 18),
        import_kind: ImportKind::EsmImport,
        resolution_mode_override: None,
        no_implicit_any: false,
        implied_classic_resolution: true,
    };
    let result = resolver.lookup(&request, |_, _| None, |_| false, None);
    let outcome = result.classify();

    assert!(!outcome.is_resolved);
    let error = outcome.error.expect("Expected error for missing module");
    assert_eq!(
        error.code, MODULE_RESOLUTION_MODE_MISMATCH,
        "Expected TS2792 for bare specifier under classic resolution even without a node_modules entry, got TS{}",
        error.code
    );

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_lookup_classic_implied_resolution_relative_keeps_ts2307() {
    // Relative specifiers stay on plain TS2307 — switching to a different
    // `moduleResolution` would not help them, so the TS2792 hint is
    // suppressed for the relative-import case (issue #3077).
    use std::fs;
    let dir = std::env::temp_dir().join("tsz_lookup_classic_relative_ts2307");
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("index.ts"), "import './missing';").unwrap();

    let options = ResolvedCompilerOptions {
        module_resolution: Some(ModuleResolutionKind::Node),
        module_suffixes: vec![String::new()],
        ..Default::default()
    };
    let mut resolver = ModuleResolver::new(&options);

    let request = ModuleLookupRequest {
        specifier: "./missing",
        containing_file: &dir.join("index.ts"),
        specifier_span: Span::new(8, 19),
        import_kind: ImportKind::EsmImport,
        resolution_mode_override: None,
        no_implicit_any: false,
        implied_classic_resolution: true,
    };
    let result = resolver.lookup(&request, |_, _| None, |_| false, None);
    let outcome = result.classify();

    assert!(!outcome.is_resolved);
    let error = outcome.error.expect("Expected error for missing module");
    assert_eq!(
        error.code, CANNOT_FIND_MODULE,
        "Relative specifier under classic resolution should stay on TS2307, got TS{}",
        error.code
    );

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_lookup_bare_json_specifier_nonexistent_upgrades_to_ts2732() {
    // Even for bare (non-relative) .json specifiers that don't exist,
    // lookup() should upgrade NotFound -> TS2732 when resolveJsonModule is false.
    use std::fs;
    let dir = std::env::temp_dir().join("tsz_lookup_bare_json_ts2732");
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("index.ts"), "import 'config.json';").unwrap();

    let options = ResolvedCompilerOptions {
        module_resolution: Some(ModuleResolutionKind::Node),
        resolve_json_module: false,
        module_suffixes: vec![String::new()],
        ..Default::default()
    };
    let mut resolver = ModuleResolver::new(&options);

    let request = ModuleLookupRequest {
        specifier: "config.json",
        containing_file: &dir.join("index.ts"),
        specifier_span: Span::new(8, 21),
        import_kind: ImportKind::EsmImport,
        resolution_mode_override: None,
        no_implicit_any: false,
        implied_classic_resolution: false,
    };
    let result = resolver.lookup(&request, |_, _| None, |_| false, None);
    let outcome = result.classify();

    let error = outcome.error.expect("Expected error for bare .json import");
    assert_eq!(
        error.code, JSON_MODULE_WITHOUT_RESOLVE_JSON_MODULE,
        "Expected TS2732 for bare .json without resolveJsonModule, got TS{}",
        error.code
    );

    let _ = fs::remove_dir_all(&dir);
}

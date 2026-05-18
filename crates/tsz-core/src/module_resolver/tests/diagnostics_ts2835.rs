//! Diagnostics Ts2835 tests for `module_resolver`.
//!
//! Tests for **TS2834/TS2835 (Import path needs an explicit
//! extension)** including `.js` / `.mjs` / `.cjs` suggestions and the
//! Node16 file-type-aware emission policy (`.ts` vs `.js` containing
//! files, `.json` direct imports, package.json suggestions).

use super::super::*;

#[test]
fn test_ts2834_error_code_constant() {
    assert_eq!(IMPORT_PATH_NEEDS_EXTENSION, 2834);
}

#[test]
fn test_import_path_needs_extension_produces_ts2835() {
    let failure = ResolutionFailure::ImportPathNeedsExtension {
        specifier: "./utils".to_string(),
        suggested_extension: ".js".to_string(),
        containing_file: "/src/index.mts".to_string(),
        span: Span::new(20, 30),
    };

    let diagnostic = failure.to_diagnostic();
    assert_eq!(diagnostic.code, IMPORT_PATH_NEEDS_EXTENSION_SUGGESTION);
    assert_eq!(diagnostic.file_name, "/src/index.mts");
    assert!(
        diagnostic
            .message
            .contains("Relative import paths need explicit file extensions")
    );
    assert!(diagnostic.message.contains("node16"));
    assert!(diagnostic.message.contains("nodenext"));
    assert!(diagnostic.message.contains("./utils.js"));
}

#[test]
fn test_import_path_needs_extension_suggests_mjs() {
    let failure = ResolutionFailure::ImportPathNeedsExtension {
        specifier: "./esm-module".to_string(),
        suggested_extension: ".mjs".to_string(),
        containing_file: "/src/app.mts".to_string(),
        span: Span::new(10, 25),
    };

    let diagnostic = failure.to_diagnostic();
    assert_eq!(diagnostic.code, IMPORT_PATH_NEEDS_EXTENSION_SUGGESTION);
    assert!(diagnostic.message.contains("./esm-module.mjs"));
}

#[test]
fn test_import_path_needs_extension_suggests_cjs() {
    let failure = ResolutionFailure::ImportPathNeedsExtension {
        specifier: "./cjs-module".to_string(),
        suggested_extension: ".cjs".to_string(),
        containing_file: "/src/legacy.cts".to_string(),
        span: Span::new(5, 20),
    };

    let diagnostic = failure.to_diagnostic();
    assert_eq!(diagnostic.code, IMPORT_PATH_NEEDS_EXTENSION_SUGGESTION);
    assert!(diagnostic.message.contains("./cjs-module.cjs"));
}

#[test]
fn test_import_path_needs_extension_accessors() {
    let failure = ResolutionFailure::ImportPathNeedsExtension {
        specifier: "./foo".to_string(),
        suggested_extension: ".js".to_string(),
        containing_file: "/bar.mts".to_string(),
        span: Span::new(50, 60),
    };

    assert_eq!(failure.containing_file(), "/bar.mts");
    assert_eq!(failure.span().start, 50);
    assert_eq!(failure.span().end, 60);
}

#[test]
fn test_node16_js_file_no_ts2834_for_relative_imports() {
    // In Node16/NodeNext, JS files should NOT get TS2834 for extensionless
    // relative imports. TSC does not enforce extension requirements on JS files.
    use std::fs;
    let dir = std::env::temp_dir().join("tsz_test_node16_js_no_ts2834");
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();

    // package.json with type=module makes the dir ESM
    fs::write(dir.join("package.json"), r#"{"type":"module"}"#).unwrap();
    fs::write(dir.join("index.js"), "import * as m from './utils';").unwrap();
    fs::write(dir.join("utils.js"), "export const x = 1;").unwrap();

    let options = ResolvedCompilerOptions {
        module_resolution: Some(ModuleResolutionKind::Node16),
        allow_js: true,
        ..Default::default()
    };
    let mut resolver = ModuleResolver::new(&options);

    // Resolve from a JS file — should succeed without TS2834
    let result = resolver.resolve_with_kind(
        "./utils",
        &dir.join("index.js"),
        Span::new(22, 30),
        ImportKind::EsmImport,
    );

    assert!(
        result.is_ok(),
        "Extensionless relative import from JS file should resolve, not emit TS2834: {result:?}"
    );

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_node16_ts_file_still_gets_ts2834_for_relative_imports() {
    // In Node16/NodeNext, TS files SHOULD still get TS2834/TS2835 for
    // extensionless relative imports even when allowJs is enabled.
    use std::fs;
    let dir = std::env::temp_dir().join("tsz_test_node16_ts_still_ts2834");
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();

    fs::write(dir.join("package.json"), r#"{"type":"module"}"#).unwrap();
    fs::write(dir.join("index.ts"), "import * as m from './utils';").unwrap();
    fs::write(dir.join("utils.js"), "export const x = 1;").unwrap();

    let options = ResolvedCompilerOptions {
        module_resolution: Some(ModuleResolutionKind::Node16),
        allow_js: true,
        ..Default::default()
    };
    let mut resolver = ModuleResolver::new(&options);

    // Resolve from a TS file — should emit TS2835 (with suggestion)
    let result = resolver.resolve_with_kind(
        "./utils",
        &dir.join("index.ts"),
        Span::new(22, 30),
        ImportKind::EsmImport,
    );

    assert!(
        result.is_err(),
        "Extensionless relative import from TS file should emit TS2834/TS2835: {result:?}"
    );
    let failure = result.unwrap_err();
    let diag = failure.to_diagnostic();
    assert!(
        diag.code == IMPORT_PATH_NEEDS_EXTENSION
            || diag.code == IMPORT_PATH_NEEDS_EXTENSION_SUGGESTION,
        "Expected TS2834 or TS2835, got TS{}: {}",
        diag.code,
        diag.message,
    );

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_node16_json_file_produces_ts2835_suggestion() {
    // When an extensionless ESM import matches a .json file on disk,
    // the resolver should suggest the .json extension (TS2835) instead
    // of emitting TS2834 with no suggestion.
    use std::fs;
    let dir = std::env::temp_dir().join("tsz_test_node16_json_ts2835");
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();

    fs::write(
        dir.join("package.json"),
        r#"{"type":"module","name":"pkg"}"#,
    )
    .unwrap();
    fs::write(dir.join("index.ts"), "import './package';").unwrap();

    let options = ResolvedCompilerOptions {
        module_resolution: Some(ModuleResolutionKind::Node16),
        resolve_package_json_exports: true,
        ..Default::default()
    };
    let mut resolver = ModuleResolver::new(&options);

    let result = resolver.resolve_with_kind(
        "./package",
        &dir.join("index.ts"),
        Span::new(8, 19),
        ImportKind::EsmImport,
    );

    assert!(
        result.is_err(),
        "Expected extension error for extensionless ESM import"
    );
    let failure = result.unwrap_err();
    let diag = failure.to_diagnostic();
    assert_eq!(
        diag.code, IMPORT_PATH_NEEDS_EXTENSION_SUGGESTION,
        "Expected TS2835 (with .json suggestion), got TS{}: {}",
        diag.code, diag.message,
    );
    assert!(
        diag.message.contains("./package.json"),
        "Expected suggestion to include './package.json', got: {}",
        diag.message,
    );

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_node16_js_file_package_json_produces_ts2835_suggestion() {
    use std::fs;
    let dir = std::env::temp_dir().join("tsz_test_node16_js_package_json_ts2835");
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();

    fs::write(
        dir.join("package.json"),
        r#"{"type":"module","name":"pkg"}"#,
    )
    .unwrap();
    fs::write(dir.join("index.js"), "import './package';").unwrap();

    let options = ResolvedCompilerOptions {
        module_resolution: Some(ModuleResolutionKind::Node16),
        allow_js: true,
        resolve_package_json_exports: true,
        ..Default::default()
    };
    let mut resolver = ModuleResolver::new(&options);

    let result = resolver.resolve_with_kind(
        "./package",
        &dir.join("index.js"),
        Span::new(8, 19),
        ImportKind::EsmImport,
    );

    assert!(
        result.is_err(),
        "Expected extension error for JS ESM import targeting package.json"
    );
    let failure = result.unwrap_err();
    let diag = failure.to_diagnostic();
    assert_eq!(
        diag.code, IMPORT_PATH_NEEDS_EXTENSION_SUGGESTION,
        "Expected TS2835 (with .json suggestion), got TS{}: {}",
        diag.code, diag.message,
    );
    assert!(
        diag.message.contains("./package.json"),
        "Expected suggestion to include './package.json', got: {}",
        diag.message,
    );

    let _ = fs::remove_dir_all(&dir);
}

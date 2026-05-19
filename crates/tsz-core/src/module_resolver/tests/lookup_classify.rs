//! Lookup Classify tests for `module_resolver`.
//!
//! Tests for `ModuleLookupResult::classify()` and the resulting
//! `ModuleLookupOutcome` (resolved, failed, ambient, untyped JS, JSX
//! not enabled).

use super::super::*;

#[test]
fn test_classify_resolved_path() {
    let result = ModuleLookupResult::resolved(PathBuf::from("/tmp/foo.ts"));
    let outcome = result.classify();

    assert_eq!(outcome.resolved_path, Some(PathBuf::from("/tmp/foo.ts")));
    assert!(!outcome.resolved_using_ts_extension);
    assert!(outcome.is_resolved);
    assert!(outcome.error.is_none());
}

#[test]
fn test_classify_failed() {
    let result =
        ModuleLookupResult::failed(CANNOT_FIND_MODULE, "Cannot find module 'foo'".to_string());
    let outcome = result.classify();

    assert!(outcome.resolved_path.is_none());
    assert!(!outcome.is_resolved);
    let error = outcome.error.expect("should have error");
    assert_eq!(error.code, CANNOT_FIND_MODULE);
}

#[test]
fn test_classify_ambient() {
    let result = ModuleLookupResult::ambient();
    let outcome = result.classify();

    assert!(outcome.resolved_path.is_none());
    assert!(
        outcome.is_resolved,
        "ambient modules should be treated as resolved"
    );
    assert!(outcome.error.is_none());
}

#[test]
fn test_classify_resolved_with_error() {
    let result = ModuleLookupResult::resolved_with_error(
        MODULE_WAS_RESOLVED_TO_BUT_JSX_NOT_SET,
        "jsx not set".to_string(),
    );
    let outcome = result.classify();

    assert!(outcome.resolved_path.is_none());
    assert!(outcome.is_resolved, "JsxNotEnabled should suppress TS2307");
    let error = outcome.error.expect("should have error");
    assert_eq!(error.code, MODULE_WAS_RESOLVED_TO_BUT_JSX_NOT_SET);
}

#[test]
fn test_classify_untyped_js_with_no_implicit_any() {
    // tsc reports TS7016 for any *imported* JS module without declarations,
    // regardless of whether the resolved path lives in `node_modules` or in
    // the project. TS6504 is reserved for explicit JS *root* files (CLI path).
    let result = ModuleLookupResult::untyped_js(PathBuf::from("/tmp/foo.js"), true, "foo");
    let outcome = result.classify();

    assert!(
        outcome.resolved_path.is_none(),
        "untyped JS modules are not added to the program (CLI keeps TS6504 for root files only)"
    );
    assert!(outcome.is_resolved, "untyped JS should suppress TS2307");
    let error = outcome.error.expect("should have TS7016 error");
    assert_eq!(error.code, COULD_NOT_FIND_DECLARATION_FILE);
    assert!(
        error
            .message
            .starts_with("Could not find a declaration file for module 'foo'."),
        "expected TS7016 message form, got: {}",
        error.message
    );
}

#[test]
fn test_classify_untyped_js_with_no_implicit_any_local_path() {
    // Identical TS7016 behavior whether the JS file is in `node_modules`
    // or in the user's project — covers the relative-import case from #3050.
    let result = ModuleLookupResult::untyped_js(PathBuf::from("/project/dep.js"), true, "./dep.js");
    let outcome = result.classify();

    let error = outcome.error.expect("should have TS7016 error");
    assert_eq!(error.code, COULD_NOT_FIND_DECLARATION_FILE);
    assert!(
        error
            .message
            .contains("Could not find a declaration file for module './dep.js'."),
        "local-path JS should still emit TS7016, got: {}",
        error.message
    );
}

#[test]
fn test_classify_untyped_js_without_no_implicit_any() {
    let result = ModuleLookupResult::untyped_js(PathBuf::from("/tmp/foo.js"), false, "foo");
    let outcome = result.classify();

    // No noImplicitAny: silent. The specifier still classifies as resolved
    // (suppressing TS2307) so the import binds as `any`.
    assert!(outcome.is_resolved);
    assert!(outcome.error.is_none(), "without noImplicitAny, no error");
}

#[test]
fn test_lookup_jsx_not_enabled_classify_outcome() {
    // When a module resolves to a .tsx file but --jsx is not set,
    // lookup() should mark it as resolved (suppress TS2307) with a TS6142 error.
    // This specifically tests the classify() outcome (is_resolved + error).
    use std::fs;
    let dir = std::env::temp_dir().join("tsz_test_lookup_jsx_classify");
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();

    fs::write(dir.join("main.ts"), "import Cmp from './Cmp';").unwrap();
    fs::write(dir.join("Cmp.tsx"), "export default function Cmp() {}").unwrap();

    let options = ResolvedCompilerOptions {
        module_resolution: Some(ModuleResolutionKind::Node),
        jsx: None,
        ..Default::default()
    };
    let mut resolver = ModuleResolver::new(&options);

    let request = ModuleLookupRequest {
        specifier: "./Cmp",
        containing_file: &dir.join("main.ts"),
        specifier_span: Span::new(16, 23),
        import_kind: ImportKind::EsmImport,
        resolution_mode_override: None,
        no_implicit_any: false,
        implied_classic_resolution: false,
    };

    let result = resolver.lookup(&request, |_, _| None, |_| false, None);
    let outcome = result.classify();

    assert!(
        outcome.is_resolved,
        "JsxNotEnabled should still mark the module as resolved"
    );
    let error = outcome
        .error
        .expect("Expected TS6142 error for JSX not enabled");
    assert_eq!(
        error.code, MODULE_WAS_RESOLVED_TO_BUT_JSX_NOT_SET,
        "Expected TS6142 but got TS{}",
        error.code
    );

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_lookup_successful_resolution_classify() {
    // Verify the full lookup() -> classify() pipeline for a successful resolution.
    use std::fs;
    let dir = std::env::temp_dir().join("tsz_test_lookup_success_classify");
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();

    fs::write(dir.join("main.ts"), "import { foo } from './utils';").unwrap();
    fs::write(dir.join("utils.ts"), "export const foo = 42;").unwrap();

    let options = ResolvedCompilerOptions {
        module_resolution: Some(ModuleResolutionKind::Node),
        ..Default::default()
    };
    let mut resolver = ModuleResolver::new(&options);

    let request = ModuleLookupRequest {
        specifier: "./utils",
        containing_file: &dir.join("main.ts"),
        specifier_span: Span::new(20, 29),
        import_kind: ImportKind::EsmImport,
        resolution_mode_override: None,
        no_implicit_any: false,
        implied_classic_resolution: false,
    };

    let result = resolver.lookup(&request, |_, _| None, |_| false, None);
    let outcome = result.classify();

    assert!(
        outcome.error.is_none(),
        "Successful resolution should have no error"
    );
    assert!(
        outcome.resolved_path.is_some(),
        "Successful resolution should have a path"
    );
    assert!(
        outcome.is_resolved,
        "Successful resolution should be resolved"
    );

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_lookup_classify_ambient_no_path_no_error() {
    // Ambient module: classify should show is_resolved=true, no path, no error
    use std::fs;
    let dir = std::env::temp_dir().join("tsz_lookup_classify_ambient");
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("index.ts"), "import 'my-ambient-mod';").unwrap();

    let options = ResolvedCompilerOptions {
        module_resolution: Some(ModuleResolutionKind::Node),
        module_suffixes: vec![String::new()],
        ..Default::default()
    };
    let mut resolver = ModuleResolver::new(&options);

    let request = ModuleLookupRequest {
        specifier: "my-ambient-mod",
        containing_file: &dir.join("index.ts"),
        specifier_span: Span::new(8, 24),
        import_kind: ImportKind::EsmImport,
        resolution_mode_override: None,
        no_implicit_any: false,
        implied_classic_resolution: false,
    };
    let result = resolver.lookup(&request, |_, _| None, |spec| spec == "my-ambient-mod", None);
    let outcome = result.classify();

    assert!(outcome.is_resolved, "Ambient module should be resolved");
    assert!(
        outcome.resolved_path.is_none(),
        "Ambient module should have no file path"
    );
    assert!(
        outcome.error.is_none(),
        "Ambient module should have no error"
    );

    let _ = fs::remove_dir_all(&dir);
}

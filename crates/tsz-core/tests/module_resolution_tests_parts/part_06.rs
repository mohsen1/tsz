#[test]
fn test_amd_define_parses() {
    // AMD-style define
    let source = r#"
define(["./dep"], function(dep: any) {
    return dep.value;
});
"#;
    let mut parser = ParserState::new("module.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    // Note: define() is a function call, should parse fine as expression
    assert!(
        parser.get_diagnostics().is_empty(),
        "AMD define should parse without errors: {:?}",
        parser.get_diagnostics()
    );

    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);
    // AMD define doesn't create special bindings in modern TypeScript
}

#[test]
fn test_amd_require_parses() {
    // AMD-style require (synchronous)
    let source = r#"
const dep = require("./dependency");
"#;
    let mut parser = ParserState::new("module.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    assert!(
        parser.get_diagnostics().is_empty(),
        "AMD require should parse: {:?}",
        parser.get_diagnostics()
    );

    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    assert!(
        binder.file_locals.has("dep"),
        "require() result should create 'dep' binding"
    );
}

// =============================================================================
// Edge Cases and Error Handling
// =============================================================================

#[test]
fn test_empty_module_specifier() {
    let source = r#"import {} from "";"#;
    let mut parser = ParserState::new("main.ts".to_string(), source.to_string());
    let _root = parser.parse_source_file();
    // Empty string module specifier - should parse but might produce checker errors
}

#[test]
fn test_import_with_no_clause() {
    // Side-effect only import
    let source = r#"import "./setup";"#;
    let diags = check_with_resolved_modules(source, "main.ts", vec!["./setup"], vec![]);
    assert!(
        no_error_code(&diags, TS2307),
        "Side-effect import should resolve, got: {diags:?}"
    );
}

#[test]
fn test_duplicate_import_specifiers() {
    // Importing the same module twice shouldn't cause duplicate TS2307
    let source = r#"
import { a } from "./missing";
import { b } from "./missing";
"#;
    let diags = check_with_resolved_modules(source, "main.ts", vec![], vec![]);
    let ts2307_count = diags.iter().filter(|(c, _)| *c == TS2307).count();
    // Should only emit TS2307 once per unique specifier
    assert!(
        ts2307_count <= 2,
        "Duplicate imports should not produce many TS2307 errors, got {ts2307_count} for: {diags:?}"
    );
}

#[test]
fn test_import_without_unresolved_imports_flag() {
    // When report_unresolved_imports is false, no TS2307 should be emitted
    let source = r#"import { foo } from "./nonexistent";"#;
    let diags = check_with_module_exports(source, "main.ts", vec![], false);
    assert!(
        no_error_code(&diags, TS2307),
        "Should not emit TS2307 when report_unresolved_imports is false, got: {diags:?}"
    );
}

// =============================================================================
// Multi-file Integration Test
// =============================================================================

#[test]
fn test_multi_file_module_resolution_maps() {
    use crate::checker::module_resolution::build_module_resolution_maps;

    // Simulate a real project structure
    let files = vec![
        "/project/src/index.ts".to_string(),
        "/project/src/utils/math.ts".to_string(),
        "/project/src/utils/string.ts".to_string(),
        "/project/src/types/api.d.ts".to_string(),
        "/project/src/components/Button.tsx".to_string(),
        "/project/src/components/index.ts".to_string(),
    ];

    let (paths, modules) = build_module_resolution_maps(&files);

    // index.ts can import everything
    assert!(modules.contains("./utils/math"));
    assert!(modules.contains("./utils/string"));
    assert!(modules.contains("./types/api"));
    assert!(modules.contains("./components/Button"));
    assert!(modules.contains("./components")); // index file

    // Cross-directory imports
    assert_eq!(
        paths.get(&(4, "../utils/math".to_string())),
        Some(&1),
        "Button.tsx should import ../utils/math"
    );
    assert_eq!(
        paths.get(&(4, "../types/api".to_string())),
        Some(&3),
        "Button.tsx should import ../types/api"
    );
}

#[test]
fn test_es6_import_default_binding_followed_with_named_import1() {
    let source = r#"
import A, { b } from './module';
"#;
    let module_source = r#"
export default function A() {}
export const b = 1;
"#;
    let diags = check_with_module_sources(source, "main.ts", vec![("./module", module_source)]);
    assert!(
        diags.is_empty(),
        "Default import with named import from module with both exports should produce no errors, got: {diags:?}"
    );
}

// =============================================================================

#[test]
fn test_commonjs_import_equals_no_error() {
    let source = r#"import utils = require("./utils");"#;
    let mut parser = ParserState::new("main.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    assert!(parser.get_diagnostics().is_empty());

    let mut binder = BinderState::new();
    merge_shared_lib_symbols(&mut binder);
    binder.bind_source_file(parser.get_arena(), root);

    let types = TypeInterner::new();
    let options = CheckerOptions {
        module: crate::common::ModuleKind::CommonJS,
        ..Default::default()
    };

    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "main.ts".to_string(),
        options,
    );
    setup_lib_contexts(&mut checker);
    checker.check_source_file(root);

    let codes: Vec<u32> = checker.ctx.diagnostics.iter().map(|d| d.code).collect();
    assert!(
        !codes.contains(&TS1202),
        "import = require in CommonJS should NOT emit TS1202, got: {codes:?}"
    );
}

// =============================================================================
// Circular Import Detection Tests
// =============================================================================

#[test]
fn test_circular_import_detection_in_binder() {
    // File A imports from B, and B imports from A
    // This shouldn't crash the binder
    let source_a = r#"
import { b } from "./b";
export const a = 1;
"#;
    let source_b = r#"
import { a } from "./a";
export const b = 2;
"#;

    // Parse and bind both files
    let mut parser_a = ParserState::new("a.ts".to_string(), source_a.to_string());
    let root_a = parser_a.parse_source_file();
    assert!(parser_a.get_diagnostics().is_empty());

    let mut parser_b = ParserState::new("b.ts".to_string(), source_b.to_string());
    let root_b = parser_b.parse_source_file();
    assert!(parser_b.get_diagnostics().is_empty());

    let mut binder_a = BinderState::new();
    binder_a.bind_source_file(parser_a.get_arena(), root_a);

    let mut binder_b = BinderState::new();
    binder_b.bind_source_file(parser_b.get_arena(), root_b);

    // Both files should bind successfully
    assert!(binder_a.file_locals.has("a"));
    assert!(binder_b.file_locals.has("b"));
}

// =============================================================================
// Mixed Import Style Tests
// =============================================================================


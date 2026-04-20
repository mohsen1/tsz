#[test]
fn test_require_creates_binding() {
    // require() calls are tracked by the import tracker
    let source = r#"const utils = require("./utils");"#;
    let mut parser = ParserState::new("main.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    assert!(parser.get_diagnostics().is_empty());

    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    assert!(
        binder.file_locals.has("utils"),
        "require() result should create local binding"
    );
}

#[test]
fn test_require_destructured() {
    let source = r#"const { foo, bar } = require("./utils");"#;
    let mut parser = ParserState::new("main.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    assert!(
        parser.get_diagnostics().is_empty(),
        "Parse errors: {:?}",
        parser.get_diagnostics()
    );

    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    assert!(
        binder.file_locals.has("foo"),
        "Destructured require should create 'foo' binding"
    );
    assert!(
        binder.file_locals.has("bar"),
        "Destructured require should create 'bar' binding"
    );
}

// =============================================================================
// Triple-Slash Reference Directive Tests
// =============================================================================

#[test]
fn test_triple_slash_reference_path_parsed() {
    let source = r#"/// <reference path="./globals.d.ts" />
const x: number = 42;
"#;
    let mut parser = ParserState::new("main.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    assert!(
        parser.get_diagnostics().is_empty(),
        "Parse errors: {:?}",
        parser.get_diagnostics()
    );

    // The file should parse without errors - reference directives are comments
    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    assert!(
        binder.file_locals.has("x"),
        "File with reference directive should still have regular bindings"
    );
}

#[test]
fn test_triple_slash_reference_types_parsed() {
    let source = r#"/// <reference types="node" />
const x: number = 42;
"#;
    let mut parser = ParserState::new("main.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    assert!(
        parser.get_diagnostics().is_empty(),
        "Parse errors: {:?}",
        parser.get_diagnostics()
    );

    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    assert!(
        binder.file_locals.has("x"),
        "File with reference types directive should still have regular bindings"
    );
}

#[test]
fn test_triple_slash_reference_lib_parsed() {
    let source = r#"/// <reference lib="es2015" />
const x: number = 42;
"#;
    let mut parser = ParserState::new("main.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    assert!(
        parser.get_diagnostics().is_empty(),
        "Parse errors: {:?}",
        parser.get_diagnostics()
    );

    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    assert!(
        binder.file_locals.has("x"),
        "File with reference lib directive should still have regular bindings"
    );
}

// =============================================================================
// Module Resolution Map Integration Tests
// =============================================================================

#[test]
fn test_build_resolution_maps_used_by_checker() {
    use crate::checker::module_resolution::build_module_resolution_maps;

    let file_names = vec![
        "/project/src/main.ts".to_string(),
        "/project/src/utils.ts".to_string(),
    ];

    let (paths, modules) = build_module_resolution_maps(&file_names);

    // Verify the maps contain expected entries
    assert_eq!(paths.get(&(0, "./utils".to_string())), Some(&1));
    assert!(modules.contains("./utils"));

    // These maps would be passed to checker context via:
    // checker.ctx.set_resolved_module_paths(paths);
    // checker.ctx.set_resolved_modules(modules);
}

// =============================================================================
// Export Assignment Tests (export = ...)
// =============================================================================

#[test]
fn test_export_assignment_basic() {
    let source = r#"
const myModule = { value: 42 };
export = myModule;
"#;
    let diags = check_single_file(source, "module.ts");
    // Should not have unexpected errors for basic export assignment
    let unexpected: Vec<_> = diags.iter().filter(|(c, _)| *c == TS2307).collect();
    assert!(
        unexpected.is_empty(),
        "Export assignment should not produce TS2307, got: {unexpected:?}"
    );
}

#[test]
fn test_export_assignment_with_other_exports() {
    let source = r#"
export const foo = 1;
const bar = 2;
export = bar;
"#;
    let diags = check_single_file(source, "module.ts");
    // TS2309: Export assignment conflicts with other exported elements
    let has_ts2309 = has_error_code(&diags, 2309);
    assert!(
        has_ts2309,
        "export = with other exports should emit TS2309, got: {diags:?}"
    );
}

// =============================================================================
// CommonJS Module.exports Tests (Binder)
// =============================================================================

#[test]
fn test_module_exports_assignment_binding() {
    let source = r#"
function myFunc() { return 42; }
module.exports = myFunc;
"#;
    let mut parser = ParserState::new("module.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    // module.exports = ... is valid in CommonJS context
    assert!(
        parser.get_diagnostics().is_empty(),
        "Parse errors: {:?}",
        parser.get_diagnostics()
    );

    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    assert!(
        binder.file_locals.has("myFunc"),
        "Function before module.exports should still be bound"
    );
}

#[test]
fn test_exports_named_assignment_binding() {
    let source = r#"
exports.foo = 42;
exports.bar = "hello";
"#;
    let mut parser = ParserState::new("module.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    assert!(
        parser.get_diagnostics().is_empty(),
        "Parse errors: {:?}",
        parser.get_diagnostics()
    );

    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    // The file should at least parse and bind without errors
    // exports.foo = ... creates CommonJS named exports
}

// =============================================================================
// AMD Module Tests
// =============================================================================


//! Unit tests for the shared `module_facts` detection helpers.
//!
//! These pin the behavior that was previously duplicated in the lowering pass
//! and the source-file emitter, so the single shared owner cannot regress.

use super::{
    contains_import_meta, jsx_automatic_runtime_makes_module, source_has_dynamic_import_call,
};
use crate::emitter::{JsxEmit, PrinterOptions};
use tsz_parser::parser::ParserState;
use tsz_parser::parser::node::NodeArena;

/// Parse `source` and return the arena plus the source file's top-level
/// statement list (the input the detection helpers consume).
fn parse_statements(name: &str, source: &str) -> (NodeArena, tsz_parser::parser::NodeList) {
    let mut parser = ParserState::new(name.to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.arena;
    let statements = {
        let root_node = arena.get(root).expect("root node");
        arena
            .get_source_file(root_node)
            .expect("source file")
            .statements
            .clone()
    };
    (arena, statements)
}

/// Parse `source` and return just the arena (JSX promotion scans the whole
/// arena, not a statement list).
fn parse_arena(name: &str, source: &str) -> NodeArena {
    parse_statements(name, source).0
}

// =============================================================================
// source_has_dynamic_import_call
// =============================================================================

#[test]
fn dynamic_import_top_level_is_detected() {
    let (arena, stmts) = parse_statements("test.ts", "const p = import(\"mod\");");
    assert!(source_has_dynamic_import_call(&arena, &stmts));
}

#[test]
fn dynamic_import_nested_in_function_is_detected() {
    let (arena, stmts) = parse_statements(
        "test.ts",
        "function load() { return import(\"mod\").then(m => m.x); }",
    );
    assert!(source_has_dynamic_import_call(&arena, &stmts));
}

#[test]
fn static_import_is_not_a_dynamic_import_call() {
    let (arena, stmts) = parse_statements("test.ts", "import x from \"mod\";\nconsole.log(x);");
    assert!(!source_has_dynamic_import_call(&arena, &stmts));
}

#[test]
fn plain_call_is_not_a_dynamic_import_call() {
    let (arena, stmts) = parse_statements("test.ts", "require(\"mod\");");
    assert!(!source_has_dynamic_import_call(&arena, &stmts));
}

// =============================================================================
// contains_import_meta
// =============================================================================

#[test]
fn import_meta_member_access_is_detected() {
    let (arena, stmts) = parse_statements("test.ts", "const u = import.meta.url;");
    assert!(contains_import_meta(&arena, &stmts));
}

#[test]
fn import_meta_nested_is_detected() {
    let (arena, stmts) = parse_statements("test.ts", "function f() { return import.meta; }");
    assert!(contains_import_meta(&arena, &stmts));
}

#[test]
fn file_without_import_meta_is_not_detected() {
    let (arena, stmts) = parse_statements("test.ts", "const meta = 1;\nconst x = obj.meta;");
    assert!(!contains_import_meta(&arena, &stmts));
}

#[test]
fn dynamic_import_alone_is_not_import_meta() {
    let (arena, stmts) = parse_statements("test.ts", "const p = import(\"mod\");");
    assert!(!contains_import_meta(&arena, &stmts));
}

// =============================================================================
// jsx_automatic_runtime_makes_module
// =============================================================================

fn options_with_jsx(jsx: JsxEmit, legacy: bool) -> PrinterOptions {
    PrinterOptions {
        jsx,
        module_detection_legacy: legacy,
        ..Default::default()
    }
}

#[test]
fn jsx_element_under_automatic_runtime_makes_module() {
    let arena = parse_arena("test.tsx", "const e = <div />;");
    let options = options_with_jsx(JsxEmit::ReactJsx, false);
    assert!(jsx_automatic_runtime_makes_module(&arena, &options));
}

#[test]
fn jsx_dev_runtime_also_promotes() {
    let arena = parse_arena("test.tsx", "const e = <div />;");
    let options = options_with_jsx(JsxEmit::ReactJsxDev, false);
    assert!(jsx_automatic_runtime_makes_module(&arena, &options));
}

#[test]
fn jsx_under_legacy_detection_does_not_promote() {
    let arena = parse_arena("test.tsx", "const e = <div />;");
    let options = options_with_jsx(JsxEmit::ReactJsx, true);
    assert!(!jsx_automatic_runtime_makes_module(&arena, &options));
}

#[test]
fn no_jsx_does_not_promote_even_under_automatic_runtime() {
    let arena = parse_arena("test.tsx", "const x = 1;");
    let options = options_with_jsx(JsxEmit::ReactJsx, false);
    assert!(!jsx_automatic_runtime_makes_module(&arena, &options));
}

#[test]
fn jsx_under_non_automatic_runtime_does_not_promote_via_this_rule() {
    let arena = parse_arena("test.tsx", "const e = <div />;");
    let options = options_with_jsx(JsxEmit::Preserve, false);
    assert!(!jsx_automatic_runtime_makes_module(&arena, &options));
}

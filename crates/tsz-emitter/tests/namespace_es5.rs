use super::*;
use tsz_parser::parser::ParserState;

fn emit_namespace(source: &str) -> String {
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    // Find the namespace declaration
    if let Some(root_node) = parser.arena.get(root)
        && let Some(source_file) = parser.arena.get_source_file(root_node)
        && let Some(&ns_idx) = source_file.statements.nodes.first()
    {
        let mut emitter = NamespaceES5Emitter::new(&parser.arena);
        emitter.set_source_text(source);
        return emitter.emit_namespace(ns_idx);
    }
    String::new()
}

#[test]
fn test_empty_namespace_skipped() {
    let output = emit_namespace("namespace M { }");
    assert!(
        output.is_empty() || output.trim().is_empty(),
        "Empty namespace should produce no output"
    );
}

#[test]
fn test_namespace_with_content() {
    let output = emit_namespace("namespace M { export var x = 1; }");
    assert!(output.contains("var M;"), "Should declare var M");
    assert!(output.contains("(function (M)"), "Should have IIFE");
    assert!(
        output.contains("(M || (M = {}))"),
        "Should have M || (M = {{}})"
    );
}

#[test]
fn test_namespace_with_function() {
    let output = emit_namespace("namespace M { export function foo() { return 1; } }");
    assert!(output.contains("var M;"), "Should declare var M");
    assert!(
        output.contains("function foo()"),
        "Should have function foo"
    );
    assert!(output.contains("M.foo = foo;"), "Should export foo");
}

// Note: test_declare_namespace_skipped is skipped because the parser
// currently doesn't attach the `declare` modifier to namespace nodes.
// This is a known parser limitation that should be fixed separately.
// The has_declare_modifier() check is still in place for when the parser is fixed.

#[test]
fn test_namespace_comment_after_erased_interface() {
    let source = r#"namespace A {
    export interface Point {
        x: number;
        y: number;
    }

    // valid since Point is exported
    export var Origin: Point = { x: 0, y: 0 };
}"#;
    let output = emit_namespace(source);
    assert!(
        output.contains("// valid since Point is exported"),
        "Comment after erased interface should be preserved. Got:\n{output}"
    );
}

#[test]
fn test_cjs_exported_namespace_iife_tail_folding() {
    // When a namespace is exported in CJS, exports.Name should be folded
    // into the IIFE tail: (N || (exports.N = N = {}))
    let source = "export namespace Models { export function test(): void {} }";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    if let Some(root_node) = parser.arena.get(root)
        && let Some(source_file) = parser.arena.get_source_file(root_node)
        && let Some(&stmt_idx) = source_file.statements.nodes.first()
    {
        // Get the MODULE_DECLARATION inside the EXPORT_DECLARATION
        let ns_idx = if let Some(stmt_node) = parser.arena.get(stmt_idx)
            && let Some(export_decl) = parser.arena.get_export_decl(stmt_node)
        {
            export_decl.export_clause
        } else {
            stmt_idx
        };

        let mut emitter = NamespaceES5Emitter::with_commonjs(&parser.arena, true);
        emitter.set_source_text(source);
        let output = emitter.emit_exported_namespace(ns_idx);
        assert!(
            output.contains("(exports.Models = Models = {})"),
            "CJS exported namespace should fold exports into IIFE tail. Got:\n{output}"
        );
        assert!(
            !output.contains("exports.Models = Models;"),
            "Should NOT have separate exports.Models = Models; line. Got:\n{output}"
        );
    }
}

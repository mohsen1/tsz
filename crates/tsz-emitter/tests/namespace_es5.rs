use super::*;
fn parse_test_source(source: &str) -> (tsz_parser::ParserState, tsz_parser::parser::NodeIndex) {
    let mut parser = tsz_parser::ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    (parser, root)
}

fn emit_namespace(source: &str) -> String {
    let (parser, root) = parse_test_source(source);

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
fn test_namespace_exported_destructuring_uses_single_temp() {
    let output = emit_namespace("namespace M { export var [a, b] = [1, 2]; }");
    assert!(
        output.contains("var _a;"),
        "Exported destructuring should hoist a temp. Got:\n{output}"
    );
    assert!(
        output.contains("_a = [1, 2], M.a = _a[0], M.b = _a[1];"),
        "Exported destructuring should read the initializer once. Got:\n{output}"
    );
    assert!(
        !output.contains("M.a = [1, 2]"),
        "Exported destructuring should not repeat the initializer. Got:\n{output}"
    );
}

#[test]
fn test_namespace_exported_object_destructuring_uses_single_temp() {
    let output = emit_namespace("namespace M { export var { a, b } = make(); }");
    assert!(
        output.contains("var _a;"),
        "Exported object destructuring should hoist a temp. Got:\n{output}"
    );
    assert!(
        output.contains("_a = make(), M.a = _a.a, M.b = _a.b;"),
        "Exported object destructuring should read the initializer once. Got:\n{output}"
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

#[test]
fn test_namespace_recovers_malformed_function_arrow_body_expression() {
    let output = emit_namespace(
        "// @target: es2015\r\nnamespace M {\r\n    export namespace N {\r\n\texport function f(x:number)=>2*x;\r\n    }\r\n}\r\n",
    );
    assert!(
        output.contains("2 * x;"),
        "Malformed function arrow body should be emitted as a recovered statement. Got:\n{output}"
    );
    assert!(
        !output.contains("function f"),
        "Declaration-only malformed function should not emit a function declaration. Got:\n{output}"
    );
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
    let (parser, root) = parse_test_source(source);

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

#[test]
fn test_system_exported_namespace_iife_tail_folding() {
    let source = "namespace Models { export function test(): void {} }";
    let (parser, root) = parse_test_source(source);

    if let Some(root_node) = parser.arena.get(root)
        && let Some(source_file) = parser.arena.get_source_file(root_node)
        && let Some(&ns_idx) = source_file.statements.nodes.first()
    {
        let mut emitter = NamespaceES5Emitter::new(&parser.arena);
        emitter.set_source_text(source);
        emitter.set_should_declare_var(false);
        emitter.set_system_export_fold("Models");
        let output = emitter.emit_namespace(ns_idx);
        assert!(
            !output.contains("var Models;"),
            "Merged System namespace output should omit the already-hoisted var declaration. Got:\n{output}"
        );
        assert!(
            output.contains(r#"(Models || (exports_1("Models", Models = {})))"#),
            "System namespace export should fold exports_1 into the IIFE tail. Got:\n{output}"
        );
    }
}

#[test]
fn test_cjs_exported_namespace_uninitialized_var_qualifies_references() {
    let source = r#"export namespace m1 {
    /** b's comment*/
    export var b: number;
    /** foo's comment*/
    function foo() {
        return b;
    }
    export namespace m2 {
        export class c {
        };
        /** i*/
        export var i = new c();
    }
}"#;
    let (parser, root) = parse_test_source(source);

    if let Some(root_node) = parser.arena.get(root)
        && let Some(source_file) = parser.arena.get_source_file(root_node)
        && let Some(&stmt_idx) = source_file.statements.nodes.first()
    {
        let ns_idx = if let Some(stmt_node) = parser.arena.get(stmt_idx)
            && let Some(export_decl) = parser.arena.get_export_decl(stmt_node)
        {
            export_decl.export_clause
        } else {
            stmt_idx
        };

        let mut emitter = NamespaceES5Emitter::with_commonjs(&parser.arena, true);
        emitter.set_source_text(source);
        emitter.set_target_es5(true);
        let output = emitter.emit_exported_namespace(ns_idx);
        assert!(
            output.contains("return m1.b;"),
            "References to uninitialized exported vars should be namespace-qualified. Got:\n{output}"
        );
        assert!(
            !output.contains("b's comment"),
            "No-op exported var comments should not be emitted. Got:\n{output}"
        );
        assert!(
            !output.lines().any(|line| line.trim() == "export"),
            "Namespace output should not contain stray export keyword lines. Got:\n{output}"
        );
        assert_eq!(
            output.matches("/** i*/").count(),
            1,
            "Initialized export comments should be emitted once. Got:\n{output}"
        );
    }
}

#[test]
fn test_nested_namespace_uses_var_at_es5_target() {
    // At ES5 target, nested namespaces inside IIFEs must use `var`, not `let`
    let source = "namespace m { namespace m2 { export class c { } } }";
    let (parser, root) = parse_test_source(source);

    if let Some(root_node) = parser.arena.get(root)
        && let Some(source_file) = parser.arena.get_source_file(root_node)
        && let Some(&ns_idx) = source_file.statements.nodes.first()
    {
        let mut emitter = NamespaceES5Emitter::new(&parser.arena);
        emitter.set_source_text(source);
        emitter.set_target_es5(true);
        let output = emitter.emit_namespace(ns_idx);
        assert!(
            !output.contains("let "),
            "ES5 target should never emit `let`. Got:\n{output}"
        );
        assert!(
            output.contains("var m2"),
            "Nested namespace should use `var` at ES5 target. Got:\n{output}"
        );
    }
}

#[test]
fn test_nested_namespace_uses_let_at_es2015_target() {
    // At ES2015+ target, nested namespaces inside IIFEs should use `let`
    let source = "namespace m { namespace m2 { export class c { } } }";
    let (parser, root) = parse_test_source(source);

    if let Some(root_node) = parser.arena.get(root)
        && let Some(source_file) = parser.arena.get_source_file(root_node)
        && let Some(&ns_idx) = source_file.statements.nodes.first()
    {
        let mut emitter = NamespaceES5Emitter::new(&parser.arena);
        emitter.set_source_text(source);
        // target_es5 defaults to false (ES2015+)
        let output = emitter.emit_namespace(ns_idx);
        assert!(
            output.contains("let m2"),
            "ES2015+ target should use `let` for nested namespace. Got:\n{output}"
        );
    }
}

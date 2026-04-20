#[test]
fn test_resolve_node_for_declaration_node_directly() {
    // When resolve_node is called on a declaration node, it should return
    // the symbol directly from node_symbols without walking
    let source = "const directDecl = 42;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let decl_symbol = binder
        .file_locals
        .get("directDecl")
        .expect("directDecl should be bound");

    // Find the declaration node for 'directDecl' (the one that has a node_symbol entry)
    let decl_node = arena
        .nodes
        .iter()
        .enumerate()
        .find_map(|(idx, _node)| {
            let node_idx = tsz_parser::NodeIndex(idx as u32);
            if binder.node_symbols.get(&node_idx.0) == Some(&decl_symbol) {
                Some(node_idx)
            } else {
                None
            }
        })
        .expect("should find the declaration node");

    let mut walker = ScopeWalker::new(arena, &binder);
    let resolved = walker.resolve_node(root, decl_node);

    assert_eq!(
        resolved,
        Some(decl_symbol),
        "resolve_node on a declaration node should return its symbol directly"
    );
}

#[test]
fn test_for_of_loop_variable_scoping() {
    let source = r#"
const items = [1, 2, 3];
for (const item of items) {
    const doubled = item * 2;
}
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    assert!(
        binder.file_locals.get("items").is_some(),
        "'items' should be in file_locals"
    );
    assert!(
        binder.file_locals.get("item").is_none(),
        "'item' should NOT be in file_locals (for-of scoped)"
    );
    assert!(
        binder.file_locals.get("doubled").is_none(),
        "'doubled' should NOT be in file_locals (for-of block scoped)"
    );
}

// ============================================================================
// Additional tests for resolver coverage (appended)
// ============================================================================

#[test]
fn test_shadowing_let_in_if_block_shadows_outer_let() {
    // Inner let in an if-block should shadow the outer let
    let source = r#"
const x = "outer";
if (true) {
    const x = "inner";
    const y = x;
}
const z = x;
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let outer_x = binder
        .file_locals
        .get("x")
        .expect("file-level x should exist");

    // Find all 'x' identifiers
    let x_nodes: Vec<_> = arena
        .nodes
        .iter()
        .enumerate()
        .filter_map(|(idx, node)| {
            if node.kind == SyntaxKind::Identifier as u16 {
                let node_idx = tsz_parser::NodeIndex(idx as u32);
                if arena.get_identifier_text(node_idx) == Some("x") {
                    return Some(node_idx);
                }
            }
            None
        })
        .collect();

    // The last 'x' is in 'const z = x;' at file level -- should resolve to outer x
    let last_x = *x_nodes.last().expect("should find x nodes");
    let mut walker = ScopeWalker::new(arena, &binder);
    let resolved = walker.resolve_node(root, last_x);
    assert_eq!(
        resolved,
        Some(outer_x),
        "'x' in file-level 'const z = x' should resolve to the outer x, not the shadowed inner x"
    );
}

#[test]
fn test_var_hoisting_across_if_boundary() {
    // var inside if should be function-scoped (hoisted to containing function)
    let source = r#"
function test() {
    if (true) {
        var a = 1;
    }
    if (false) {
        var b = 2;
    }
    return a + b;
}
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    // 'a' and 'b' should NOT be in file_locals (they're hoisted to function scope, not file)
    assert!(
        binder.file_locals.get("a").is_none(),
        "'a' should NOT be in file_locals (hoisted to function scope)"
    );
    assert!(
        binder.file_locals.get("b").is_none(),
        "'b' should NOT be in file_locals (hoisted to function scope)"
    );

    // Find the 'a' usage in 'return a + b;'
    let a_nodes: Vec<_> = arena
        .nodes
        .iter()
        .enumerate()
        .filter_map(|(idx, node)| {
            if node.kind == SyntaxKind::Identifier as u16 {
                let node_idx = tsz_parser::NodeIndex(idx as u32);
                if arena.get_identifier_text(node_idx) == Some("a") {
                    return Some(node_idx);
                }
            }
            None
        })
        .collect();

    // The last 'a' should be in 'return a + b;'
    let a_usage = *a_nodes.last().expect("should find 'a' identifiers");
    let mut walker = ScopeWalker::new(arena, &binder);
    let resolved = walker.resolve_node(root, a_usage);
    assert!(
        resolved.is_some(),
        "var 'a' should be resolvable via hoisting in the function scope"
    );
}

#[test]
fn test_catch_clause_parameter_scoping() {
    // catch parameter 'e' should be scoped to catch clause, not leak to try or outer
    let source = r#"
const e = "outer";
try {
    throw new Error();
} catch (e) {
    const msg = e;
}
const result = e;
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let outer_e = binder
        .file_locals
        .get("e")
        .expect("file-level 'e' should exist");

    // Find the last 'e' identifier (in 'const result = e;')
    let e_nodes: Vec<_> = arena
        .nodes
        .iter()
        .enumerate()
        .filter_map(|(idx, node)| {
            if node.kind == SyntaxKind::Identifier as u16 {
                let node_idx = tsz_parser::NodeIndex(idx as u32);
                if arena.get_identifier_text(node_idx) == Some("e") {
                    return Some(node_idx);
                }
            }
            None
        })
        .collect();

    let last_e = *e_nodes.last().expect("should find 'e' identifiers");
    let mut walker = ScopeWalker::new(arena, &binder);
    let resolved = walker.resolve_node(root, last_e);
    assert_eq!(
        resolved,
        Some(outer_e),
        "'e' in 'const result = e' should resolve to outer 'e', not catch parameter"
    );
}

#[test]
fn test_class_static_block_scope() {
    let source = r#"
const x = "outer";
class Foo {
    static {
        const x = "static-block";
        const y = x;
    }
}
const z = x;
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    // 'x', 'Foo', 'z' should be in file_locals; 'y' should not
    assert!(binder.file_locals.get("x").is_some());
    assert!(binder.file_locals.get("Foo").is_some());
    assert!(binder.file_locals.get("z").is_some());
    assert!(
        binder.file_locals.get("y").is_none(),
        "'y' should NOT be in file_locals (inside static block)"
    );
}

#[test]
fn test_multiple_functions_same_named_parameters() {
    // Two functions with parameters named 'x' should not interfere
    let source = r#"
function foo(x: number) { return x; }
function bar(x: string) { return x; }
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    assert!(binder.file_locals.get("foo").is_some());
    assert!(binder.file_locals.get("bar").is_some());
    assert!(
        binder.file_locals.get("x").is_none(),
        "parameter 'x' should NOT leak to file_locals from either function"
    );

    // Find the 'x' usages in 'return x;' statements
    let x_nodes: Vec<_> = arena
        .nodes
        .iter()
        .enumerate()
        .filter_map(|(idx, node)| {
            if node.kind == SyntaxKind::Identifier as u16 {
                let node_idx = tsz_parser::NodeIndex(idx as u32);
                if arena.get_identifier_text(node_idx) == Some("x") {
                    return Some(node_idx);
                }
            }
            None
        })
        .collect();

    // There should be at least 4 'x' identifiers (2 param decls + 2 return usages)
    assert!(
        x_nodes.len() >= 4,
        "should find at least 4 'x' identifiers, found {}",
        x_nodes.len()
    );

    // Each 'return x;' usage should resolve to some symbol (its own function's parameter)
    for &x_node in &x_nodes {
        let mut walker = ScopeWalker::new(arena, &binder);
        let resolved = walker.resolve_node(root, x_node);
        assert!(
            resolved.is_some(),
            "each 'x' identifier should resolve to a symbol"
        );
    }
}

#[test]
fn test_arrow_function_parameter_scope() {
    let source = r#"
const outer = 1;
const fn1 = (a: number, b: number) => a + b;
const fn2 = (a: string) => a;
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    assert!(binder.file_locals.get("outer").is_some());
    assert!(binder.file_locals.get("fn1").is_some());
    assert!(binder.file_locals.get("fn2").is_some());
    assert!(
        binder.file_locals.get("a").is_none(),
        "'a' should NOT be in file_locals (arrow function parameter)"
    );
    assert!(
        binder.file_locals.get("b").is_none(),
        "'b' should NOT be in file_locals (arrow function parameter)"
    );
}

#[test]
fn test_export_function_and_class_resolution() {
    let source = r#"
export function exported() { return 1; }
export class ExportedClass {}
export interface ExportedInterface { x: number; }
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    assert!(
        binder.file_locals.get("exported").is_some(),
        "'exported' function should be in file_locals"
    );
    assert!(
        binder.file_locals.get("ExportedClass").is_some(),
        "'ExportedClass' should be in file_locals"
    );
    assert!(
        binder.file_locals.get("ExportedInterface").is_some(),
        "'ExportedInterface' should be in file_locals"
    );
}

#[test]
fn test_namespace_nested_scoping() {
    let source = r#"
namespace Outer {
    export namespace Inner {
        export const value = 1;
    }
    const local = 2;
}
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    assert!(
        binder.file_locals.get("Outer").is_some(),
        "'Outer' namespace should be in file_locals"
    );
    assert!(
        binder.file_locals.get("Inner").is_none(),
        "'Inner' should NOT be in file_locals (inside Outer namespace)"
    );
    assert!(
        binder.file_locals.get("value").is_none(),
        "'value' should NOT be in file_locals (inside Inner namespace)"
    );
    assert!(
        binder.file_locals.get("local").is_none(),
        "'local' should NOT be in file_locals (inside Outer namespace)"
    );
}


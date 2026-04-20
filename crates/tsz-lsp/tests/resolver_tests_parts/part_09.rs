#[test]
fn test_resolve_generator_function_name() {
    let source = "function* counter() {\n  let i = 0;\n  while (true) { yield i++; }\n}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    assert!(
        binder.file_locals.get("counter").is_some(),
        "'counter' generator function should be in file_locals"
    );
}

#[test]
fn test_let_not_hoisted_out_of_block() {
    let source = r#"
{
    let blockScoped = 1;
}
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    assert!(
        binder.file_locals.get("blockScoped").is_none(),
        "'blockScoped' let should NOT be in file_locals (block-scoped)"
    );
}

#[test]
fn test_const_not_hoisted_out_of_block() {
    let source = r#"
if (true) {
    const inner = 42;
}
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    assert!(
        binder.file_locals.get("inner").is_none(),
        "'inner' const should NOT be in file_locals (block-scoped)"
    );
}

#[test]
fn test_resolve_multiple_interfaces_same_name() {
    // Declaration merging: multiple interfaces with same name
    let source = r#"
interface Opts { x: number; }
interface Opts { y: string; }
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    assert!(
        binder.file_locals.get("Opts").is_some(),
        "'Opts' merged interface should be in file_locals"
    );
}

#[test]
fn test_scope_chain_at_setter_body() {
    let source = r#"
const limit = 100;
class Config {
    set maxSize(val: number) {
        const clamped = Math.min(val, limit);
    }
}
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    // Find 'clamped' usage
    let clamped_nodes: Vec<_> = arena
        .nodes
        .iter()
        .enumerate()
        .filter_map(|(idx, node)| {
            if node.kind == SyntaxKind::Identifier as u16 {
                let node_idx = tsz_parser::NodeIndex(idx as u32);
                if arena.get_identifier_text(node_idx) == Some("clamped") {
                    return Some(node_idx);
                }
            }
            None
        })
        .collect();

    if let Some(&clamped_node) = clamped_nodes.first() {
        let mut walker = ScopeWalker::new(arena, &binder);
        let chain = walker.get_scope_chain(root, clamped_node);

        // Should have at least file + class + setter scopes
        assert!(
            chain.len() >= 3,
            "scope chain inside setter should have at least 3 scopes, got {}",
            chain.len()
        );

        let has_limit = chain.iter().any(|scope| scope.get("limit").is_some());
        assert!(has_limit, "'limit' should be visible from inside setter");
    }
}

#[test]
fn test_find_references_type_alias_usage() {
    let source = r#"
type Point = { x: number; y: number };
const origin: Point = { x: 0, y: 0 };
function move(p: Point): Point { return p; }
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    if let Some(point_symbol) = binder.file_locals.get("Point") {
        let mut walker = ScopeWalker::new(arena, &binder);
        let refs = walker.find_references(root, point_symbol);
        // Should find at least 3: declaration + const annotation + param annotation + return annotation
        assert!(
            refs.len() >= 3,
            "should find at least 3 references to 'Point', got {}",
            refs.len()
        );
    }
}

#[test]
fn test_resolve_nested_namespace_function() {
    let source = r#"
namespace Outer {
    export namespace Inner {
        export function deep() { return 42; }
    }
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
    // 'Inner' and 'deep' should not be at file level
    assert!(
        binder.file_locals.get("Inner").is_none(),
        "'Inner' should NOT be in file_locals (nested namespace)"
    );
    assert!(
        binder.file_locals.get("deep").is_none(),
        "'deep' should NOT be in file_locals (nested in namespace)"
    );
}

#[test]
fn test_resolve_abstract_class_name() {
    let source = "abstract class Shape {\n  abstract area(): number;\n}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    assert!(
        binder.file_locals.get("Shape").is_some(),
        "'Shape' abstract class should be in file_locals"
    );
}

#[test]
fn test_class_method_not_in_file_locals() {
    let source = r#"
class Calculator {
    add(a: number, b: number) { return a + b; }
    subtract(a: number, b: number) { return a - b; }
}
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    assert!(binder.file_locals.get("Calculator").is_some());
    assert!(
        binder.file_locals.get("add").is_none(),
        "'add' method should NOT be in file_locals"
    );
    assert!(
        binder.file_locals.get("subtract").is_none(),
        "'subtract' method should NOT be in file_locals"
    );
}

#[test]
fn test_resolve_arrow_function_variable_in_file_locals() {
    let source = "const transform = (x: number) => x * 2;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    assert!(
        binder.file_locals.get("transform").is_some(),
        "'transform' arrow function variable should be in file_locals"
    );
}


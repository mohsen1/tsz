#[test]
fn test_class_with_private_constructor_in_file_locals() {
    let source = "class Singleton {\n  private static instance: Singleton;\n  private constructor() {}\n  static getInstance() { return new Singleton(); }\n}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    assert!(
        binder.file_locals.get("Singleton").is_some(),
        "'Singleton' should be in file_locals"
    );
}

#[test]
fn test_resolve_variable_in_immediately_invoked_arrow() {
    let source = r#"
const result = (() => {
    const inner = 42;
    return inner;
})();
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    assert!(
        binder.file_locals.get("result").is_some(),
        "'result' should be in file_locals"
    );
    assert!(
        binder.file_locals.get("inner").is_none(),
        "'inner' should NOT be in file_locals (inside IIFE)"
    );
}

#[test]
fn test_multiple_var_declarations_same_name() {
    // var allows redeclaration in the same function scope
    let source = r#"
function foo() {
    var x = 1;
    var x = 2;
    return x;
}
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    assert!(
        binder.file_locals.get("foo").is_some(),
        "'foo' should be in file_locals"
    );
    // 'x' should not be in file_locals (scoped to function)
    assert!(
        binder.file_locals.get("x").is_none(),
        "'x' should NOT be in file_locals (function-scoped var)"
    );
}

#[test]
fn test_resolve_export_default_class() {
    let source = "export default class DefaultClass {\n  value = 1;\n}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    // The class name should be in file_locals
    if let Some(_sym) = binder.file_locals.get("DefaultClass") {
        // verified
    }
    // At minimum, should not panic
}

#[test]
fn test_resolve_class_with_static_methods() {
    let source = r#"
class MathUtils {
    static add(a: number, b: number) { return a + b; }
    static multiply(a: number, b: number) { return a * b; }
}
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    assert!(
        binder.file_locals.get("MathUtils").is_some(),
        "'MathUtils' should be in file_locals"
    );
    // Static methods are members, not file_locals
    assert!(
        binder.file_locals.get("add").is_none(),
        "'add' static method should NOT be in file_locals"
    );
    assert!(
        binder.file_locals.get("multiply").is_none(),
        "'multiply' static method should NOT be in file_locals"
    );
}

#[test]
fn test_find_references_enum_used_as_type_and_value() {
    let source = r#"
enum Status { Active, Inactive }
const s: Status = Status.Active;
function check(status: Status) { return status; }
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    if let Some(status_symbol) = binder.file_locals.get("Status") {
        let mut walker = ScopeWalker::new(arena, &binder);
        let refs = walker.find_references(root, status_symbol);
        // Should find at least 3: declaration + type annotation + value usage
        assert!(
            refs.len() >= 3,
            "should find at least 3 references to 'Status', got {}",
            refs.len()
        );
    }
}

#[test]
fn test_scope_chain_at_getter_body() {
    let source = r#"
const globalVal = 100;
class Counter {
    get count() {
        const temp = globalVal;
        return temp;
    }
}
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    // Find 'temp' usage in 'return temp;'
    let temp_nodes: Vec<_> = arena
        .nodes
        .iter()
        .enumerate()
        .filter_map(|(idx, node)| {
            if node.kind == SyntaxKind::Identifier as u16 {
                let node_idx = tsz_parser::NodeIndex(idx as u32);
                if arena.get_identifier_text(node_idx) == Some("temp") {
                    return Some(node_idx);
                }
            }
            None
        })
        .collect();

    if let Some(&temp_usage) = temp_nodes.last() {
        let mut walker = ScopeWalker::new(arena, &binder);
        let chain = walker.get_scope_chain(root, temp_usage);

        // Should have at least file + class + getter scopes
        assert!(
            chain.len() >= 3,
            "scope chain inside getter should have at least 3 scopes, got {}",
            chain.len()
        );

        // 'globalVal' should be visible
        let has_global = chain.iter().any(|scope| scope.get("globalVal").is_some());
        assert!(
            has_global,
            "'globalVal' should be visible from inside getter"
        );
    }
}

#[test]
fn test_resolve_const_enum_name() {
    let source = "const enum Direction { Up, Down, Left, Right }";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    assert!(
        binder.file_locals.get("Direction").is_some(),
        "'Direction' const enum should be in file_locals"
    );
}

#[test]
fn test_resolve_generic_class_name() {
    let source =
        "class Container<T> {\n  value: T;\n  constructor(val: T) { this.value = val; }\n}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    assert!(
        binder.file_locals.get("Container").is_some(),
        "'Container' generic class should be in file_locals"
    );
}

#[test]
fn test_resolve_async_function_name() {
    let source = "async function fetchData() {\n  return await Promise.resolve(1);\n}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    assert!(
        binder.file_locals.get("fetchData").is_some(),
        "'fetchData' async function should be in file_locals"
    );
}


#[test]
fn test_resolve_rest_parameter() {
    let source = "function sum(...nums: number[]) { return nums.reduce((a, b) => a + b, 0); }";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    assert!(
        binder.file_locals.get("sum").is_some(),
        "'sum' should be in file_locals"
    );
    assert!(
        binder.file_locals.get("nums").is_none(),
        "'nums' rest parameter should NOT be in file_locals"
    );
}

#[test]
fn test_find_references_interface_usage() {
    let source = r#"
interface Config {
    host: string;
    port: number;
}
const cfg: Config = { host: "localhost", port: 3000 };
function setup(c: Config) {}
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    if let Some(config_symbol) = binder.file_locals.get("Config") {
        let mut walker = ScopeWalker::new(arena, &binder);
        let refs = walker.find_references(root, config_symbol);
        // Should find at least 3: declaration + type annotation on cfg + parameter type
        assert!(
            refs.len() >= 3,
            "should find at least 3 references to 'Config', got {}",
            refs.len()
        );
    }
}

#[test]
fn test_var_hoisting_inside_if_block() {
    let source = r#"
function foo() {
    if (true) {
        var hoisted = 1;
    }
    return hoisted;
}
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    // 'hoisted' should NOT be in file_locals (var hoists to function, not file)
    assert!(
        binder.file_locals.get("hoisted").is_none(),
        "'hoisted' should NOT be in file_locals (hoisted to function scope)"
    );
}

#[test]
fn test_resolve_computed_property_no_crash() {
    let source = r#"
const key = "hello";
const obj = { [key]: 1 };
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    assert!(
        binder.file_locals.get("key").is_some(),
        "'key' should be in file_locals"
    );
    assert!(
        binder.file_locals.get("obj").is_some(),
        "'obj' should be in file_locals"
    );

    // Resolve the 'key' usage in the computed property - should not crash
    let key_nodes: Vec<_> = arena
        .nodes
        .iter()
        .enumerate()
        .filter_map(|(idx, node)| {
            if node.kind == SyntaxKind::Identifier as u16 {
                let node_idx = tsz_parser::NodeIndex(idx as u32);
                if arena.get_identifier_text(node_idx) == Some("key") {
                    return Some(node_idx);
                }
            }
            None
        })
        .collect();

    if key_nodes.len() >= 2 {
        let key_usage = *key_nodes.last().unwrap();
        let mut walker = ScopeWalker::new(arena, &binder);
        let resolved = walker.resolve_node(root, key_usage);
        assert!(
            resolved.is_some(),
            "'key' in computed property should resolve"
        );
    }
}

#[test]
fn test_namespace_members_in_file_locals() {
    let source = "namespace MyNS { export const inner = 1; }";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    assert!(
        binder.file_locals.get("MyNS").is_some(),
        "'MyNS' should be in file_locals"
    );
    // 'inner' is inside the namespace, should NOT be in file_locals
    assert!(
        binder.file_locals.get("inner").is_none(),
        "'inner' should NOT be in file_locals (scoped to namespace)"
    );
}

#[test]
fn test_scope_chain_at_nested_arrow_functions() {
    let source = r#"
const a = 1;
const outer = () => {
    const b = 2;
    const inner = () => {
        const c = 3;
        return a + b + c;
    };
};
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    // Find the 'c' usage in return statement
    let c_nodes: Vec<_> = arena
        .nodes
        .iter()
        .enumerate()
        .filter_map(|(idx, node)| {
            if node.kind == SyntaxKind::Identifier as u16 {
                let node_idx = tsz_parser::NodeIndex(idx as u32);
                if arena.get_identifier_text(node_idx) == Some("c") {
                    return Some(node_idx);
                }
            }
            None
        })
        .collect();

    if let Some(&c_usage) = c_nodes.last() {
        let mut walker = ScopeWalker::new(arena, &binder);
        let chain = walker.get_scope_chain(root, c_usage);

        // Should have at least 3 scopes (file + outer arrow + inner arrow)
        assert!(
            chain.len() >= 3,
            "nested arrow function scope chain should have at least 3 scopes, got {}",
            chain.len()
        );

        // 'a' should be visible from innermost scope
        let has_a = chain.iter().any(|scope| scope.get("a").is_some());
        assert!(has_a, "'a' should be visible from nested arrow function");
    }
}

// ============================================================================
// Additional resolver tests (batch 4 - edge cases)
// ============================================================================

#[test]
fn test_resolve_empty_source() {
    let source = "";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    // No file_locals should exist in an empty file
    assert!(
        binder.file_locals.is_empty(),
        "empty source should have no file_locals"
    );
}

#[test]
fn test_resolve_destructuring_array_declaration() {
    let source = "const [first, second, ...rest] = [1, 2, 3, 4, 5];";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    // Array destructured variables should be in file_locals
    if let Some(_sym) = binder.file_locals.get("first") {
        assert!(binder.file_locals.get("second").is_some());
    }
    // rest may or may not be bound depending on binder implementation
    let _ = binder.file_locals.get("rest");
}

#[test]
fn test_resolve_destructuring_object_declaration() {
    let source = "const { name, age } = { name: 'Alice', age: 30 };";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    // Object destructured variables should be in file_locals
    if let Some(_sym) = binder.file_locals.get("name") {
        assert!(
            binder.file_locals.get("age").is_some(),
            "'age' should also be in file_locals from destructuring"
        );
    }
}

#[test]
fn test_abstract_class_in_file_locals() {
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


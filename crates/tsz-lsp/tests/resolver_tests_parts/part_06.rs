#[test]
fn test_multiple_declarations_same_scope() {
    let source = "const a = 1;\nlet b = 2;\nvar c = 3;\nfunction d() {}\nclass E {}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    assert!(binder.file_locals.get("a").is_some());
    assert!(binder.file_locals.get("b").is_some());
    assert!(binder.file_locals.get("c").is_some());
    assert!(binder.file_locals.get("d").is_some());
    assert!(binder.file_locals.get("E").is_some());
}

#[test]
fn test_find_references_class_used_in_type_position() {
    let source = r#"
class MyClass {}
const a: MyClass = new MyClass();
function foo(x: MyClass) {}
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let class_symbol = binder
        .file_locals
        .get("MyClass")
        .expect("MyClass should be bound");

    let mut walker = ScopeWalker::new(arena, &binder);
    let refs = walker.find_references(root, class_symbol);

    // Should find at least 3 references (declaration + type annotation + new expression)
    assert!(
        refs.len() >= 3,
        "should find at least 3 references to 'MyClass', got {}",
        refs.len()
    );
}

#[test]
fn test_scope_chain_at_arrow_function_body() {
    let source = r#"
const outer = 1;
const fn1 = (x: number) => {
    const inner = x;
    return inner;
};
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    // Find the 'inner' usage in 'return inner;'
    let inner_nodes: Vec<_> = arena
        .nodes
        .iter()
        .enumerate()
        .filter_map(|(idx, node)| {
            if node.kind == SyntaxKind::Identifier as u16 {
                let node_idx = tsz_parser::NodeIndex(idx as u32);
                if arena.get_identifier_text(node_idx) == Some("inner") {
                    return Some(node_idx);
                }
            }
            None
        })
        .collect();

    if let Some(&inner_usage) = inner_nodes.last() {
        let mut walker = ScopeWalker::new(arena, &binder);
        let chain = walker.get_scope_chain(root, inner_usage);

        // Should have at least file + arrow function scopes
        assert!(
            chain.len() >= 2,
            "scope chain inside arrow function should have at least 2 scopes, got {}",
            chain.len()
        );

        // 'outer' should be visible
        let has_outer = chain.iter().any(|scope| scope.get("outer").is_some());
        assert!(
            has_outer,
            "'outer' should be visible from inside arrow function"
        );
    }
}

#[test]
fn test_try_finally_variable_scoping() {
    let source = r#"
try {
    const tryVar = 1;
} finally {
    const finallyVar = 2;
}
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    assert!(
        binder.file_locals.get("tryVar").is_none(),
        "'tryVar' should NOT be in file_locals (block-scoped in try)"
    );
    assert!(
        binder.file_locals.get("finallyVar").is_none(),
        "'finallyVar' should NOT be in file_locals (block-scoped in finally)"
    );
}

// ============================================================================
// Additional resolver tests (batch 3)
// ============================================================================

#[test]
fn test_resolve_interface_in_file_locals() {
    let source = "interface Foo { x: number; }\ninterface Bar extends Foo {}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    assert!(
        binder.file_locals.get("Foo").is_some(),
        "'Foo' interface should be in file_locals"
    );
    assert!(
        binder.file_locals.get("Bar").is_some(),
        "'Bar' interface should be in file_locals"
    );
}

#[test]
fn test_resolve_enum_members_not_in_file_locals() {
    let source = "enum Direction { Up, Down, Left, Right }";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    assert!(
        binder.file_locals.get("Direction").is_some(),
        "'Direction' should be in file_locals"
    );
    // Individual enum members should not leak to file scope
    assert!(
        binder.file_locals.get("Up").is_none(),
        "'Up' enum member should NOT be in file_locals"
    );
}

#[test]
fn test_resolve_class_in_file_locals() {
    let source = "class Animal { name: string = ''; }\nclass Dog extends Animal {}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    assert!(
        binder.file_locals.get("Animal").is_some(),
        "'Animal' class should be in file_locals"
    );
    assert!(
        binder.file_locals.get("Dog").is_some(),
        "'Dog' class should be in file_locals"
    );
}

#[test]
fn test_labeled_statement_scoping() {
    let source = r#"
outer: for (let i = 0; i < 10; i++) {
    inner: for (let j = 0; j < 10; j++) {
        if (i === j) break outer;
    }
}
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    // Loop variables should be block-scoped and not in file_locals
    assert!(
        binder.file_locals.get("i").is_none(),
        "'i' should NOT be in file_locals (block-scoped in for loop)"
    );
    assert!(
        binder.file_locals.get("j").is_none(),
        "'j' should NOT be in file_locals (block-scoped in for loop)"
    );
}

#[test]
fn test_resolve_variable_in_template_literal() {
    let source = r#"
const name = "world";
const greeting = `hello ${name}`;
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    assert!(
        binder.file_locals.get("name").is_some(),
        "'name' should be in file_locals"
    );
    assert!(
        binder.file_locals.get("greeting").is_some(),
        "'greeting' should be in file_locals"
    );

    // Find the 'name' usage inside the template literal
    let name_nodes: Vec<_> = arena
        .nodes
        .iter()
        .enumerate()
        .filter_map(|(idx, node)| {
            if node.kind == SyntaxKind::Identifier as u16 {
                let node_idx = tsz_parser::NodeIndex(idx as u32);
                if arena.get_identifier_text(node_idx) == Some("name") {
                    return Some(node_idx);
                }
            }
            None
        })
        .collect();

    if name_nodes.len() >= 2 {
        let name_usage = *name_nodes.last().unwrap();
        let mut walker = ScopeWalker::new(arena, &binder);
        let resolved = walker.resolve_node(root, name_usage);
        assert!(
            resolved.is_some(),
            "'name' usage in template literal should resolve"
        );
    }
}

#[test]
fn test_resolve_default_parameter_value() {
    let source = "function greet(name: string = 'world') { return name; }";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    assert!(
        binder.file_locals.get("greet").is_some(),
        "'greet' should be in file_locals"
    );

    // 'name' should not leak to file scope
    assert!(
        binder.file_locals.get("name").is_none(),
        "'name' parameter should NOT be in file_locals"
    );
}


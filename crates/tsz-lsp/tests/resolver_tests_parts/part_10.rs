#[test]
fn test_scope_chain_in_nested_arrow_callbacks() {
    let source = r#"
const outer = 1;
const fn1 = () => {
    const mid = 2;
    const fn2 = () => {
        const inner = 3;
    };
};
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    // Find 'inner' identifier
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

    if let Some(&inner_node) = inner_nodes.first() {
        let mut walker = ScopeWalker::new(arena, &binder);
        let chain = walker.get_scope_chain(root, inner_node);

        // Should have at least: file + fn1 arrow + fn2 arrow
        assert!(
            chain.len() >= 3,
            "scope chain inside nested arrows should have at least 3 scopes, got {}",
            chain.len()
        );

        let has_outer = chain.iter().any(|scope| scope.get("outer").is_some());
        assert!(has_outer, "'outer' should be visible from inner arrow");
    }
}

#[test]
fn test_resolve_enum_not_in_file_locals_members() {
    let source = "enum Color { Red, Green, Blue }";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    assert!(binder.file_locals.get("Color").is_some());
    assert!(
        binder.file_locals.get("Red").is_none(),
        "'Red' enum member should NOT be in file_locals"
    );
    assert!(
        binder.file_locals.get("Green").is_none(),
        "'Green' enum member should NOT be in file_locals"
    );
}

#[test]
fn test_resolve_export_const_in_file_locals() {
    let source = "export const API_KEY = 'abc123';";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    assert!(
        binder.file_locals.get("API_KEY").is_some(),
        "'API_KEY' exported const should be in file_locals"
    );
}

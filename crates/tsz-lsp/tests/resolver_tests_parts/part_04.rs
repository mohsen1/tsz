#[test]
fn test_getter_setter_scope_creation() {
    let source = r#"
class MyClass {
    private _val: number = 0;
    get val(): number {
        const temp = this._val;
        return temp;
    }
    set val(newVal: number) {
        const validated = newVal;
        this._val = validated;
    }
}
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    assert!(
        binder.file_locals.get("MyClass").is_some(),
        "'MyClass' should be in file_locals"
    );
    // Internal variables should not leak
    assert!(
        binder.file_locals.get("temp").is_none(),
        "'temp' should NOT be in file_locals (inside getter)"
    );
    assert!(
        binder.file_locals.get("validated").is_none(),
        "'validated' should NOT be in file_locals (inside setter)"
    );
    assert!(
        binder.file_locals.get("newVal").is_none(),
        "'newVal' should NOT be in file_locals (setter parameter)"
    );
}

#[test]
fn test_private_identifier_resolution() {
    let source = r#"
class Counter {
    #count: number = 0;
    increment() {
        this.#count++;
    }
    getCount() {
        return this.#count;
    }
}
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    assert!(
        binder.file_locals.get("Counter").is_some(),
        "'Counter' should be in file_locals"
    );

    // Find private identifier nodes (#count)
    let private_id_nodes: Vec<_> = arena
        .nodes
        .iter()
        .enumerate()
        .filter_map(|(idx, node)| {
            if node.kind == SyntaxKind::PrivateIdentifier as u16 {
                let node_idx = tsz_parser::NodeIndex(idx as u32);
                if arena.get_identifier_text(node_idx) == Some("#count") {
                    return Some(node_idx);
                }
            }
            None
        })
        .collect();

    assert!(
        !private_id_nodes.is_empty(),
        "should find at least one #count private identifier"
    );

    // Try resolving the first private identifier
    let mut walker = ScopeWalker::new(arena, &binder);
    let resolved = walker.resolve_node(root, private_id_nodes[0]);
    // Private identifiers are resolved through binder.resolve_identifier
    // The result depends on whether the binder tracks them, but we verify no panic
    let _ = resolved;
}

#[test]
fn test_for_in_loop_variable_resolution() {
    let source = r#"
const obj = { a: 1, b: 2, c: 3 };
for (const key in obj) {
    const value = obj[key];
}
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    // Find the 'key' usage inside the for-in body (in obj[key])
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

    assert!(
        key_nodes.len() >= 2,
        "should find at least 2 'key' identifiers (decl + usage)"
    );

    // The last 'key' should be the usage in obj[key]
    let key_usage = *key_nodes.last().unwrap();
    let mut walker = ScopeWalker::new(arena, &binder);
    let resolved = walker.resolve_node(root, key_usage);
    assert!(
        resolved.is_some(),
        "'key' usage inside for-in body should resolve to its declaration"
    );
}

#[test]
fn test_for_of_loop_variable_resolution() {
    let source = r#"
const items = [10, 20, 30];
for (const item of items) {
    const doubled = item * 2;
}
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    // Find the 'item' usage inside the for-of body
    let item_nodes: Vec<_> = arena
        .nodes
        .iter()
        .enumerate()
        .filter_map(|(idx, node)| {
            if node.kind == SyntaxKind::Identifier as u16 {
                let node_idx = tsz_parser::NodeIndex(idx as u32);
                if arena.get_identifier_text(node_idx) == Some("item") {
                    return Some(node_idx);
                }
            }
            None
        })
        .collect();

    assert!(
        item_nodes.len() >= 2,
        "should find at least 2 'item' identifiers (decl + usage)"
    );

    let item_usage = *item_nodes.last().unwrap();
    let mut walker = ScopeWalker::new(arena, &binder);
    let resolved = walker.resolve_node(root, item_usage);
    assert!(
        resolved.is_some(),
        "'item' usage inside for-of body should resolve to its declaration"
    );
}

#[test]
fn test_scope_chain_deeply_nested() {
    let source = r#"
const a = 1;
function outer() {
    const b = 2;
    function middle() {
        const c = 3;
        function inner() {
            const d = 4;
            return a + b + c + d;
        }
    }
}
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    // Find the 'd' identifier in the return statement
    let d_nodes: Vec<_> = arena
        .nodes
        .iter()
        .enumerate()
        .filter_map(|(idx, node)| {
            if node.kind == SyntaxKind::Identifier as u16 {
                let node_idx = tsz_parser::NodeIndex(idx as u32);
                if arena.get_identifier_text(node_idx) == Some("d") {
                    return Some(node_idx);
                }
            }
            None
        })
        .collect();

    let d_usage = *d_nodes.last().expect("should find 'd' identifiers");
    let mut walker = ScopeWalker::new(arena, &binder);
    let chain = walker.get_scope_chain(root, d_usage);

    // Should have at least 4 scopes (file + outer + middle + inner)
    assert!(
        chain.len() >= 4,
        "deeply nested scope chain should have at least 4 scopes, got {}",
        chain.len()
    );

    // 'a' should be visible from the innermost scope
    let has_a = chain.iter().any(|scope| scope.get("a").is_some());
    assert!(has_a, "'a' should be visible from the innermost scope");
}

#[test]
fn test_resolve_node_cached_with_none_stats() {
    use crate::resolver::ScopeCache;
    use rustc_hash::FxHashMap;

    let source = "const x = 10;\nconst y = x + 1;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let x_usage = arena
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
        .next_back()
        .expect("should find x usage");

    // Resolve with cache but None stats (should not panic)
    let mut walker = ScopeWalker::new(arena, &binder);
    let mut cache: ScopeCache = FxHashMap::default();
    let result = walker.resolve_node_cached(root, x_usage, &mut cache, None);
    assert!(result.is_some(), "should resolve x even with None stats");

    // Second call should also work with None stats
    let mut walker2 = ScopeWalker::new(arena, &binder);
    let result2 = walker2.resolve_node_cached(root, x_usage, &mut cache, None);
    assert_eq!(result, result2, "cached result should match first result");
}

#[test]
fn test_resolve_non_identifier_node() {
    // Resolving a node that is not an identifier should return None
    let source = "const x = 1 + 2;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    // Find a numeric literal node
    let numeric_node = arena.nodes.iter().enumerate().find_map(|(idx, node)| {
        if node.kind == SyntaxKind::NumericLiteral as u16 {
            Some(tsz_parser::NodeIndex(idx as u32))
        } else {
            None
        }
    });

    if let Some(num_idx) = numeric_node {
        let mut walker = ScopeWalker::new(arena, &binder);
        let resolved = walker.resolve_node(root, num_idx);
        assert!(
            resolved.is_none(),
            "resolving a numeric literal should return None"
        );
    }
}

#[test]
fn test_find_references_across_scopes() {
    // Variable declared at file level, used inside multiple nested scopes
    let source = r#"
const shared = 42;
function foo() { return shared; }
function bar() { return shared + 1; }
const baz = () => shared;
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let shared_symbol = binder
        .file_locals
        .get("shared")
        .expect("shared should be bound");

    let mut walker = ScopeWalker::new(arena, &binder);
    let refs = walker.find_references(root, shared_symbol);

    // Should find the declaration + 3 usages (foo, bar, baz) = at least 4
    assert!(
        refs.len() >= 4,
        "should find at least 4 references to 'shared' (decl + 3 usages), got {}",
        refs.len()
    );
}

#[test]
fn test_scope_chain_cached_second_call_hits() {
    use crate::resolver::{ScopeCache, ScopeCacheStats};
    use rustc_hash::FxHashMap;

    let source = "function f() { const a = 1; }";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let a_node = arena
        .nodes
        .iter()
        .enumerate()
        .find_map(|(idx, node)| {
            if node.kind == SyntaxKind::Identifier as u16 {
                let node_idx = tsz_parser::NodeIndex(idx as u32);
                if arena.get_identifier_text(node_idx) == Some("a") {
                    return Some(node_idx);
                }
            }
            None
        })
        .expect("should find 'a'");

    let mut cache: ScopeCache = FxHashMap::default();

    // First call - miss
    let mut walker = ScopeWalker::new(arena, &binder);
    let mut stats1 = ScopeCacheStats::default();
    let chain1 = walker.get_scope_chain_cached(root, a_node, &mut cache, Some(&mut stats1));
    let chain1_len = chain1.len();
    assert_eq!(stats1.misses, 1);
    assert_eq!(stats1.hits, 0);

    // Second call - hit
    let mut walker2 = ScopeWalker::new(arena, &binder);
    let mut stats2 = ScopeCacheStats::default();
    let chain2 = walker2.get_scope_chain_cached(root, a_node, &mut cache, Some(&mut stats2));
    assert_eq!(
        chain2.len(),
        chain1_len,
        "cached chain should have same length"
    );
    assert_eq!(stats2.hits, 1, "second call should be a cache hit");
    assert_eq!(stats2.misses, 0);
}

#[test]
fn test_var_not_hoisted_to_file_level_from_nested_function() {
    // var inside a nested function should NOT hoist to file scope
    let source = r#"
function outer() {
    function inner() {
        var deepVar = 1;
    }
    return deepVar;
}
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    assert!(
        binder.file_locals.get("deepVar").is_none(),
        "'deepVar' should NOT be in file_locals (var hoists only to its containing function)"
    );
}


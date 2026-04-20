#[test]
fn test_find_references_function_multiple_calls() {
    let source = "function doWork() {}\ndoWork();\ndoWork();\ndoWork();";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let fn_symbol = binder
        .file_locals
        .get("doWork")
        .expect("doWork should be bound");

    let mut walker = ScopeWalker::new(arena, &binder);
    let refs = walker.find_references(root, fn_symbol);

    // Should find at least the declaration + 3 call sites
    assert!(
        refs.len() >= 3,
        "Should find at least 3 references for 'doWork', got {}",
        refs.len()
    );
}

#[test]
fn test_no_resolution_for_undefined_symbol() {
    let source = "const x = 1;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    // 'nonexistent' is not declared anywhere
    assert!(
        binder.file_locals.get("nonexistent").is_none(),
        "'nonexistent' should not be in file_locals"
    );
}

#[test]
fn test_resolve_type_alias_name() {
    let source = "type StringOrNumber = string | number;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    assert!(
        binder.file_locals.get("StringOrNumber").is_some(),
        "Should bind 'StringOrNumber' type alias in file_locals"
    );
}

// ============================================================================
// New tests below (appended after existing tests)
// ============================================================================

#[test]
fn test_resolve_node_cached_returns_same_as_uncached() {
    use crate::resolver::{ScopeCache, ScopeCacheStats};
    use rustc_hash::FxHashMap;

    let source = "const x = 10;\nconst y = x + 1;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let x_symbol = binder.file_locals.get("x").expect("x should be bound");

    // Find the usage of 'x' in 'x + 1'
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

    // Resolve without cache
    let mut walker = ScopeWalker::new(arena, &binder);
    let uncached = walker.resolve_node(root, x_usage);

    // Resolve with cache (first call = miss)
    let mut walker2 = ScopeWalker::new(arena, &binder);
    let mut cache: ScopeCache = FxHashMap::default();
    let mut stats = ScopeCacheStats::default();
    let cached = walker2.resolve_node_cached(root, x_usage, &mut cache, Some(&mut stats));

    assert_eq!(uncached, cached, "cached resolution should match uncached");
    assert_eq!(uncached, Some(x_symbol));
    assert_eq!(stats.misses, 1, "first call should be a miss");
    assert_eq!(stats.hits, 0);
}

#[test]
fn test_resolve_node_cached_hits_on_second_call() {
    use crate::resolver::{ScopeCache, ScopeCacheStats};
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

    let mut walker = ScopeWalker::new(arena, &binder);
    let mut cache: ScopeCache = FxHashMap::default();
    let mut stats = ScopeCacheStats::default();

    // First call (miss)
    let _ = walker.resolve_node_cached(root, x_usage, &mut cache, Some(&mut stats));
    assert_eq!(stats.misses, 1);
    assert_eq!(stats.hits, 0);

    // Second call (hit)
    let mut walker2 = ScopeWalker::new(arena, &binder);
    let mut stats2 = ScopeCacheStats::default();
    let _ = walker2.resolve_node_cached(root, x_usage, &mut cache, Some(&mut stats2));
    assert_eq!(stats2.hits, 1, "second call should be a cache hit");
    assert_eq!(stats2.misses, 0);
}

#[test]
fn test_get_scope_chain_at_file_level() {
    let source = "const a = 1;\nconst b = 2;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    // Find the identifier 'b' in the declaration
    let b_node = arena
        .nodes
        .iter()
        .enumerate()
        .find_map(|(idx, node)| {
            if node.kind == SyntaxKind::Identifier as u16 {
                let node_idx = tsz_parser::NodeIndex(idx as u32);
                if arena.get_identifier_text(node_idx) == Some("b") {
                    return Some(node_idx);
                }
            }
            None
        })
        .expect("should find 'b' identifier");

    let mut walker = ScopeWalker::new(arena, &binder);
    let chain = walker.get_scope_chain(root, b_node);

    // At file level there should be at least one scope (file-level scope)
    assert!(
        !chain.is_empty(),
        "scope chain should have at least the file-level scope"
    );

    // The file-level scope should contain 'a' and 'b'
    let has_a = chain.iter().any(|scope| scope.get("a").is_some());
    let has_b = chain.iter().any(|scope| scope.get("b").is_some());
    assert!(has_a, "scope chain should contain 'a'");
    assert!(has_b, "scope chain should contain 'b'");
}

#[test]
fn test_get_scope_chain_inside_function() {
    let source = "const outer = 1;\nfunction foo() {\n  const inner = 2;\n  return inner;\n}";
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

    assert!(
        !inner_nodes.is_empty(),
        "should find 'inner' identifier nodes"
    );

    let inner_usage = *inner_nodes.last().unwrap();
    let mut walker = ScopeWalker::new(arena, &binder);
    let chain = walker.get_scope_chain(root, inner_usage);

    // Should have more than one scope (file + function)
    assert!(
        chain.len() >= 2,
        "scope chain inside function should have at least 2 scopes, got {}",
        chain.len()
    );

    // 'outer' should be visible somewhere in the chain
    let has_outer = chain.iter().any(|scope| scope.get("outer").is_some());
    assert!(
        has_outer,
        "'outer' should be visible from inside the function"
    );
}

#[test]
fn test_get_scope_chain_cached_returns_same_as_uncached() {
    use crate::resolver::{ScopeCache, ScopeCacheStats};
    use rustc_hash::FxHashMap;

    let source = "const x = 1;\nfunction f() { const y = x; }";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    // Find 'y' identifier
    let y_node = arena
        .nodes
        .iter()
        .enumerate()
        .find_map(|(idx, node)| {
            if node.kind == SyntaxKind::Identifier as u16 {
                let node_idx = tsz_parser::NodeIndex(idx as u32);
                if arena.get_identifier_text(node_idx) == Some("y") {
                    return Some(node_idx);
                }
            }
            None
        })
        .expect("should find 'y'");

    // Get uncached scope chain
    let mut walker = ScopeWalker::new(arena, &binder);
    let uncached_chain = walker.get_scope_chain(root, y_node);

    // Get cached scope chain
    let mut walker2 = ScopeWalker::new(arena, &binder);
    let mut cache: ScopeCache = FxHashMap::default();
    let mut stats = ScopeCacheStats::default();
    let cached_chain = walker2.get_scope_chain_cached(root, y_node, &mut cache, Some(&mut stats));

    assert_eq!(
        uncached_chain.len(),
        cached_chain.len(),
        "cached and uncached scope chains should have same length"
    );
    assert_eq!(stats.misses, 1);
    assert_eq!(stats.hits, 0);

    // Second call should hit
    let mut walker3 = ScopeWalker::new(arena, &binder);
    let mut stats2 = ScopeCacheStats::default();
    let _ = walker3.get_scope_chain_cached(root, y_node, &mut cache, Some(&mut stats2));
    assert_eq!(stats2.hits, 1, "second get_scope_chain_cached should hit");
}

#[test]
fn test_var_hoisting_function_scoped() {
    // var declarations should be hoisted to function scope
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

    // Find the 'hoisted' usage in 'return hoisted;'
    let hoisted_nodes: Vec<_> = arena
        .nodes
        .iter()
        .enumerate()
        .filter_map(|(idx, node)| {
            if node.kind == SyntaxKind::Identifier as u16 {
                let node_idx = tsz_parser::NodeIndex(idx as u32);
                if arena.get_identifier_text(node_idx) == Some("hoisted") {
                    return Some(node_idx);
                }
            }
            None
        })
        .collect();

    assert!(
        hoisted_nodes.len() >= 2,
        "should find at least 2 'hoisted' identifiers (decl + usage)"
    );

    // The last occurrence should be the usage in 'return hoisted;'
    let usage = *hoisted_nodes.last().unwrap();
    let mut walker = ScopeWalker::new(arena, &binder);
    let resolved = walker.resolve_node(root, usage);

    assert!(
        resolved.is_some(),
        "var-declared 'hoisted' should be resolvable outside its block (function-scoped hoisting)"
    );
}

#[test]
fn test_arrow_function_creates_scope() {
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

    // 'outer' and 'fn1' should be in file_locals
    assert!(
        binder.file_locals.get("outer").is_some(),
        "'outer' should be in file_locals"
    );
    assert!(
        binder.file_locals.get("fn1").is_some(),
        "'fn1' should be in file_locals"
    );

    // 'inner' and 'x' should NOT be in file_locals (scoped inside arrow)
    assert!(
        binder.file_locals.get("inner").is_none(),
        "'inner' should NOT be in file_locals (scoped inside arrow function)"
    );
    assert!(
        binder.file_locals.get("x").is_none(),
        "'x' should NOT be in file_locals (parameter of arrow function)"
    );
}


#[test]
fn test_resolve_non_identifier_node() {
    // Resolving a node that is not an identifier should return None
    let source = "const x = 1 + 2;";
    let (parser, root, binder) = bind_test_source(source);
    let arena = parser.get_arena();
    let _ = root;

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
    let (parser, root, binder) = bind_test_source(source);
    let arena = parser.get_arena();
    let _ = root;

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
    let (parser, root, binder) = bind_test_source(source);
    let arena = parser.get_arena();
    let _ = root;

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
    let (parser, root, binder) = bind_test_source(source);
    let _arena = parser.get_arena();
    let _ = root;

    assert!(
        binder.file_locals.get("deepVar").is_none(),
        "'deepVar' should NOT be in file_locals (var hoists only to its containing function)"
    );
}

#[test]
fn test_destructuring_in_catch_clause() {
    let source = r#"
try {
    throw { message: "fail", code: 42 };
} catch (err) {
    const { message, code } = err as any;
    console.log(message, code);
}
"#;
    let (parser, root, binder) = bind_test_source(source);
    let _arena = parser.get_arena();
    let _ = root;

    // None of the catch-scoped variables should leak to file_locals
    assert!(
        binder.file_locals.get("err").is_none(),
        "'err' should NOT be in file_locals (catch clause scoped)"
    );
    assert!(
        binder.file_locals.get("message").is_none(),
        "'message' should NOT be in file_locals (catch block scoped)"
    );
    assert!(
        binder.file_locals.get("code").is_none(),
        "'code' should NOT be in file_locals (catch block scoped)"
    );
}

#[test]
fn test_resolve_node_cached_for_non_identifier() {
    use crate::resolver::{ScopeCache, ScopeCacheStats};
    use rustc_hash::FxHashMap;

    let source = "const x = 123;";
    let (parser, root, binder) = bind_test_source(source);
    let arena = parser.get_arena();
    let _ = root;

    // Find a numeric literal
    let num_node = arena
        .nodes
        .iter()
        .enumerate()
        .find_map(|(idx, node)| {
            if node.kind == SyntaxKind::NumericLiteral as u16 {
                Some(tsz_parser::NodeIndex(idx as u32))
            } else {
                None
            }
        })
        .expect("should find numeric literal");

    let mut walker = ScopeWalker::new(arena, &binder);
    let mut cache: ScopeCache = FxHashMap::default();
    let mut stats = ScopeCacheStats::default();
    let result = walker.resolve_node_cached(root, num_node, &mut cache, Some(&mut stats));
    assert!(
        result.is_none(),
        "resolve_node_cached should return None for non-identifier nodes"
    );
}

#[test]
fn test_scope_chain_at_class_method_body() {
    let source = r#"
const global = 1;
class Foo {
    method() {
        const local = 2;
        return local;
    }
}
"#;
    let (parser, root, binder) = bind_test_source(source);
    let arena = parser.get_arena();
    let _ = root;

    // Find the 'local' usage in 'return local;'
    let local_nodes: Vec<_> = arena
        .nodes
        .iter()
        .enumerate()
        .filter_map(|(idx, node)| {
            if node.kind == SyntaxKind::Identifier as u16 {
                let node_idx = tsz_parser::NodeIndex(idx as u32);
                if arena.get_identifier_text(node_idx) == Some("local") {
                    return Some(node_idx);
                }
            }
            None
        })
        .collect();

    let local_usage = *local_nodes.last().expect("should find 'local' usage");
    let mut walker = ScopeWalker::new(arena, &binder);
    let chain = walker.get_scope_chain(root, local_usage);

    // Should have scopes for: file + class + method (at least 3)
    assert!(
        chain.len() >= 3,
        "scope chain inside class method should have at least 3 scopes, got {}",
        chain.len()
    );

    // 'global' should be visible from inside the method
    let has_global = chain.iter().any(|scope| scope.get("global").is_some());
    assert!(
        has_global,
        "'global' should be visible from inside a class method"
    );
}

#[test]
fn test_resolve_type_alias_in_file_locals() {
    let source = "type Callback = () => void;\ntype Result<T> = { ok: T };";
    let (parser, root, binder) = bind_test_source(source);
    let _arena = parser.get_arena();
    let _ = root;

    assert!(
        binder.file_locals.get("Callback").is_some(),
        "'Callback' type alias should be in file_locals"
    );
    assert!(
        binder.file_locals.get("Result").is_some(),
        "'Result' type alias should be in file_locals"
    );
}

#[test]
fn test_const_enum_in_file_locals() {
    let source = "const enum Direction { Up, Down, Left, Right }";
    let (parser, root, binder) = bind_test_source(source);
    let _arena = parser.get_arena();
    let _ = root;

    assert!(
        binder.file_locals.get("Direction").is_some(),
        "'Direction' const enum should be in file_locals"
    );
}

#[test]
fn test_while_loop_variable_scoping() {
    let source = r#"
while (true) {
    const loopVar = 1;
    break;
}
"#;
    let (parser, root, binder) = bind_test_source(source);
    let _arena = parser.get_arena();
    let _ = root;

    assert!(
        binder.file_locals.get("loopVar").is_none(),
        "'loopVar' should NOT be in file_locals (block-scoped inside while)"
    );
}

#[test]
fn test_do_while_loop_variable_scoping() {
    let source = r#"
do {
    const doVar = 1;
} while (false);
"#;
    let (parser, root, binder) = bind_test_source(source);
    let _arena = parser.get_arena();
    let _ = root;

    assert!(
        binder.file_locals.get("doVar").is_none(),
        "'doVar' should NOT be in file_locals (block-scoped inside do-while)"
    );
}

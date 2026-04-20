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
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

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
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

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
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

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

// ============================================================================
// Additional resolver tests (batch 2)
// ============================================================================

#[test]
fn test_resolve_type_alias_in_file_locals() {
    let source = "type Callback = () => void;\ntype Result<T> = { ok: T };";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

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
fn test_while_loop_variable_scoping() {
    let source = r#"
while (true) {
    const loopVar = 1;
    break;
}
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

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
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    assert!(
        binder.file_locals.get("doVar").is_none(),
        "'doVar' should NOT be in file_locals (block-scoped inside do-while)"
    );
}

#[test]
fn test_switch_case_variable_scoping() {
    let source = r#"
const x = 1;
switch (x) {
    case 1: {
        const caseVar = "one";
        break;
    }
    default: {
        const defaultVar = "other";
    }
}
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    assert!(
        binder.file_locals.get("x").is_some(),
        "'x' should be in file_locals"
    );
    assert!(
        binder.file_locals.get("caseVar").is_none(),
        "'caseVar' should NOT be in file_locals (block-scoped inside case)"
    );
    assert!(
        binder.file_locals.get("defaultVar").is_none(),
        "'defaultVar' should NOT be in file_locals (block-scoped inside default)"
    );
}

#[test]
fn test_if_else_block_variable_scoping() {
    let source = r#"
if (true) {
    const ifVar = 1;
} else {
    const elseVar = 2;
}
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    assert!(
        binder.file_locals.get("ifVar").is_none(),
        "'ifVar' should NOT be in file_locals (block-scoped in if)"
    );
    assert!(
        binder.file_locals.get("elseVar").is_none(),
        "'elseVar' should NOT be in file_locals (block-scoped in else)"
    );
}

#[test]
fn test_function_expression_name_scoping() {
    let source = r#"
const fn1 = function namedExpr() {
    return namedExpr;
};
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    assert!(
        binder.file_locals.get("fn1").is_some(),
        "'fn1' should be in file_locals"
    );
    // The named function expression name should NOT leak to file scope
    assert!(
        binder.file_locals.get("namedExpr").is_none(),
        "'namedExpr' should NOT be in file_locals (function expression name is local)"
    );
}


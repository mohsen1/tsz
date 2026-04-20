#[test]
fn test_for_loop_variable_scoping() {
    let source = r#"
for (let i = 0; i < 10; i++) {
    const x = i;
}
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    // 'i' and 'x' should NOT be in file_locals (scoped to the for loop)
    assert!(
        binder.file_locals.get("i").is_none(),
        "'i' should NOT be in file_locals (for-loop scoped)"
    );
    assert!(
        binder.file_locals.get("x").is_none(),
        "'x' should NOT be in file_locals (for-loop block scoped)"
    );
}

#[test]
fn test_for_in_loop_variable_scoping() {
    let source = r#"
const obj = { a: 1, b: 2 };
for (const key in obj) {
    const val = key;
}
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    assert!(
        binder.file_locals.get("obj").is_some(),
        "'obj' should be in file_locals"
    );
    assert!(
        binder.file_locals.get("key").is_none(),
        "'key' should NOT be in file_locals (for-in scoped)"
    );
}

#[test]
fn test_catch_clause_variable_scoping() {
    let source = r#"
try {
    const a = 1;
} catch (err) {
    const msg = err;
}
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    // Neither 'a', 'err', nor 'msg' should be in file_locals
    assert!(
        binder.file_locals.get("a").is_none(),
        "'a' should NOT be in file_locals (try block scoped)"
    );
    assert!(
        binder.file_locals.get("err").is_none(),
        "'err' should NOT be in file_locals (catch clause scoped)"
    );
    assert!(
        binder.file_locals.get("msg").is_none(),
        "'msg' should NOT be in file_locals (catch block scoped)"
    );
}

#[test]
fn test_class_inside_function() {
    let source = r#"
function factory() {
    class Inner {
        value: number = 0;
    }
    return new Inner();
}
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    assert!(
        binder.file_locals.get("factory").is_some(),
        "'factory' should be in file_locals"
    );
    assert!(
        binder.file_locals.get("Inner").is_none(),
        "'Inner' should NOT be in file_locals (scoped inside function)"
    );
}

#[test]
fn test_shadowing_across_nested_scopes() {
    // x is declared at file level, then shadowed inside a function
    let source = r#"
const x = "outer";
function foo() {
    const x = "inner";
    return x;
}
const y = x;
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

    // Find the 'x' in 'const y = x;' (last usage of 'x' at file level)
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

    // The last 'x' identifier should be the one in 'const y = x;' at file level
    let last_x = *x_nodes.last().expect("should find x identifiers");
    let mut walker = ScopeWalker::new(arena, &binder);
    let resolved = walker.resolve_node(root, last_x);

    assert_eq!(
        resolved,
        Some(outer_x),
        "x in 'const y = x' should resolve to the outer (file-level) x, not the inner shadow"
    );
}

#[test]
fn test_find_references_no_matching_references() {
    let source = "const x = 1;\nconst y = 2;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    // Create a symbol that doesn't appear in the source
    let fake_symbol = binder
        .symbols
        .alloc(symbol_flags::VARIABLE, "nonexistent".to_string());

    let mut walker = ScopeWalker::new(arena, &binder);
    let refs = walker.find_references(root, fake_symbol);

    assert!(
        refs.is_empty(),
        "find_references should return empty vec for a symbol with no references, got {} refs",
        refs.len()
    );
}

#[test]
fn test_multiple_scopes_same_named_variables() {
    // Two separate functions each define their own 'x'
    let source = r#"
function foo() {
    const x = 1;
    return x;
}
function bar() {
    const x = 2;
    return x;
}
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    // 'foo' and 'bar' should be in file_locals
    assert!(binder.file_locals.get("foo").is_some());
    assert!(binder.file_locals.get("bar").is_some());

    // 'x' should NOT be in file_locals (each is function-scoped)
    assert!(
        binder.file_locals.get("x").is_none(),
        "'x' should not leak to file_locals from inside functions"
    );
}

#[test]
fn test_module_declaration_scoping() {
    let source = r#"
namespace MyModule {
    export const value = 42;
    const internal = "hidden";
}
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    assert!(
        binder.file_locals.get("MyModule").is_some(),
        "'MyModule' namespace should be in file_locals"
    );
    // 'value' and 'internal' should NOT be in file_locals (scoped inside namespace)
    assert!(
        binder.file_locals.get("value").is_none(),
        "'value' should NOT be in file_locals (inside namespace)"
    );
    assert!(
        binder.file_locals.get("internal").is_none(),
        "'internal' should NOT be in file_locals (inside namespace)"
    );
}

#[test]
fn test_export_declaration_resolution() {
    let source = r#"
export const exported = 1;
export function exportedFn() {}
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    assert!(
        binder.file_locals.get("exported").is_some(),
        "'exported' should be in file_locals"
    );
    assert!(
        binder.file_locals.get("exportedFn").is_some(),
        "'exportedFn' should be in file_locals"
    );
}

#[test]
fn test_scope_cache_stats_default() {
    use crate::resolver::ScopeCacheStats;

    let stats = ScopeCacheStats::default();
    assert_eq!(stats.hits, 0);
    assert_eq!(stats.misses, 0);
}


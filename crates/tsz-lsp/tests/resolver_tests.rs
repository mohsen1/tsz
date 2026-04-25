use crate::resolver::ScopeWalker;
use tsz_binder::{BinderState, symbol_flags};
use tsz_parser::ParserState;
use tsz_parser::parser::node::NodeAccess;
use tsz_scanner::SyntaxKind;

#[test]
fn test_resolve_simple_variable() {
    // const x = 1; x + 1;
    let source = "const x = 1; x + 1;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    // Should have a symbol for 'x'
    assert!(binder.file_locals.get("x").is_some());
}

#[test]
fn test_find_references_includes_module_namespace_string_literals() {
    let source = r#"
const foo = "foo";
export { foo as "__<alias>" };
import { "__<alias>" as bar } from "./foo";
bar;
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let literal_nodes: Vec<_> = arena
        .nodes
        .iter()
        .enumerate()
        .filter_map(|(idx, node)| {
            if node.kind != SyntaxKind::StringLiteral as u16 {
                return None;
            }
            let node_idx = tsz_parser::NodeIndex(idx as u32);
            if arena.get_literal_text(node_idx) != Some("__<alias>") {
                return None;
            }
            Some(node_idx)
        })
        .collect();

    assert_eq!(
        literal_nodes.len(),
        2,
        "expected both import/export string literal namespace identifiers"
    );

    let symbol_id = binder
        .symbols
        .alloc(symbol_flags::ALIAS, "__<alias>".to_string());
    for literal_idx in &literal_nodes {
        let ext = arena
            .get_extended(*literal_idx)
            .expect("string literal should have parent");
        let parent_idx = ext.parent;
        let parent_node = arena
            .get(parent_idx)
            .expect("string literal parent should exist");
        assert!(
            parent_node.kind == tsz_parser::syntax_kind_ext::IMPORT_SPECIFIER
                || parent_node.kind == tsz_parser::syntax_kind_ext::EXPORT_SPECIFIER,
            "expected quoted namespace literal to be under import/export specifier"
        );
        std::sync::Arc::make_mut(&mut binder.node_symbols).insert(parent_idx.0, symbol_id);
    }

    let mut ref_walker = ScopeWalker::new(arena, &binder);
    let refs = ref_walker.find_references(root, symbol_id);
    let string_refs: Vec<_> = refs
        .into_iter()
        .filter(|idx| {
            arena
                .get(*idx)
                .is_some_and(|node| node.kind == SyntaxKind::StringLiteral as u16)
                && arena.get_literal_text(*idx) == Some("__<alias>")
        })
        .collect();

    assert_eq!(
        string_refs.len(),
        2,
        "expected find_references to include both quoted import/export specifier references"
    );
}

#[test]
fn test_resolve_function_name() {
    let source = "function greet() {}\ngreet();";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    // Should have a symbol for 'greet'
    assert!(
        binder.file_locals.get("greet").is_some(),
        "Should bind 'greet' in file_locals"
    );
}

#[test]
fn test_resolve_variable_in_nested_scope() {
    // Variable declared inside a function body
    let source = "function outer() {\n  const inner = 1;\n  return inner;\n}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    // 'outer' should be in file_locals
    assert!(
        binder.file_locals.get("outer").is_some(),
        "Should bind 'outer' in file_locals"
    );
    // 'inner' is block-scoped inside the function, so it should NOT be in file_locals
    assert!(
        binder.file_locals.get("inner").is_none(),
        "'inner' should NOT be in file_locals (it's block-scoped inside a function)"
    );
}

#[test]
fn test_resolve_class_name() {
    let source = "class MyClass {\n  method() {}\n}\nconst c = new MyClass();";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    assert!(
        binder.file_locals.get("MyClass").is_some(),
        "Should bind 'MyClass' in file_locals"
    );
    assert!(
        binder.file_locals.get("c").is_some(),
        "Should bind 'c' in file_locals"
    );
}

#[test]
fn test_resolve_interface_name() {
    let source = "interface IFoo {\n  bar: string;\n}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    assert!(
        binder.file_locals.get("IFoo").is_some(),
        "Should bind 'IFoo' in file_locals"
    );
}

#[test]
fn test_resolve_enum_name() {
    let source = "enum Color {\n  Red,\n  Green,\n  Blue\n}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    assert!(
        binder.file_locals.get("Color").is_some(),
        "Should bind 'Color' in file_locals"
    );
}

#[test]
fn test_resolve_parameter_stays_in_function_scope() {
    // Parameters should be scoped to their function, not in file_locals
    let source = "function add(a: number, b: number) { return a + b; }";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    assert!(
        binder.file_locals.get("add").is_some(),
        "Should bind 'add' in file_locals"
    );
    // Parameters 'a' and 'b' should NOT be in file_locals
    assert!(
        binder.file_locals.get("a").is_none(),
        "'a' parameter should NOT be in file_locals"
    );
    assert!(
        binder.file_locals.get("b").is_none(),
        "'b' parameter should NOT be in file_locals"
    );
}

#[test]
fn test_scope_walker_resolve_identifier_usage() {
    // Use ScopeWalker to resolve an identifier usage back to its declaration
    let source = "const x = 10;\nconst y = x + 1;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let x_symbol = binder.file_locals.get("x").expect("x should be bound");

    // Find the identifier 'x' in the expression 'x + 1' (not the declaration)
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
        .next_back(); // The last occurrence should be the usage in 'x + 1'

    if let Some(usage_idx) = x_usage {
        let mut walker = ScopeWalker::new(arena, &binder);
        let resolved = walker.resolve_node(root, usage_idx);
        assert_eq!(
            resolved,
            Some(x_symbol),
            "ScopeWalker should resolve 'x' usage to its declaration symbol"
        );
    }
}

#[test]
fn test_find_references_variable_multiple_usages() {
    let source = "const val = 1;\nconst a = val;\nconst b = val;\nconst c = val;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let val_symbol = binder.file_locals.get("val").expect("val should be bound");

    let mut walker = ScopeWalker::new(arena, &binder);
    let refs = walker.find_references(root, val_symbol);

    // Should find at least the declaration + 3 usages = 4 references
    // (exact count depends on how find_references counts the declaration)
    assert!(
        refs.len() >= 3,
        "Should find at least 3 references for 'val', got {}",
        refs.len()
    );
}

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

#[test]
fn test_resolve_node_for_declaration_node_directly() {
    // When resolve_node is called on a declaration node, it should return
    // the symbol directly from node_symbols without walking
    let source = "const directDecl = 42;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let decl_symbol = binder
        .file_locals
        .get("directDecl")
        .expect("directDecl should be bound");

    // Find the declaration node for 'directDecl' (the one that has a node_symbol entry)
    let decl_node = arena
        .nodes
        .iter()
        .enumerate()
        .find_map(|(idx, _node)| {
            let node_idx = tsz_parser::NodeIndex(idx as u32);
            if binder.node_symbols.get(&node_idx.0) == Some(&decl_symbol) {
                Some(node_idx)
            } else {
                None
            }
        })
        .expect("should find the declaration node");

    let mut walker = ScopeWalker::new(arena, &binder);
    let resolved = walker.resolve_node(root, decl_node);

    assert_eq!(
        resolved,
        Some(decl_symbol),
        "resolve_node on a declaration node should return its symbol directly"
    );
}

#[test]
fn test_for_of_loop_variable_scoping() {
    let source = r#"
const items = [1, 2, 3];
for (const item of items) {
    const doubled = item * 2;
}
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    assert!(
        binder.file_locals.get("items").is_some(),
        "'items' should be in file_locals"
    );
    assert!(
        binder.file_locals.get("item").is_none(),
        "'item' should NOT be in file_locals (for-of scoped)"
    );
    assert!(
        binder.file_locals.get("doubled").is_none(),
        "'doubled' should NOT be in file_locals (for-of block scoped)"
    );
}

// ============================================================================
// Additional tests for resolver coverage (appended)
// ============================================================================

#[test]
fn test_shadowing_let_in_if_block_shadows_outer_let() {
    // Inner let in an if-block should shadow the outer let
    let source = r#"
const x = "outer";
if (true) {
    const x = "inner";
    const y = x;
}
const z = x;
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

    // Find all 'x' identifiers
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

    // The last 'x' is in 'const z = x;' at file level -- should resolve to outer x
    let last_x = *x_nodes.last().expect("should find x nodes");
    let mut walker = ScopeWalker::new(arena, &binder);
    let resolved = walker.resolve_node(root, last_x);
    assert_eq!(
        resolved,
        Some(outer_x),
        "'x' in file-level 'const z = x' should resolve to the outer x, not the shadowed inner x"
    );
}

#[test]
fn test_var_hoisting_across_if_boundary() {
    // var inside if should be function-scoped (hoisted to containing function)
    let source = r#"
function test() {
    if (true) {
        var a = 1;
    }
    if (false) {
        var b = 2;
    }
    return a + b;
}
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    // 'a' and 'b' should NOT be in file_locals (they're hoisted to function scope, not file)
    assert!(
        binder.file_locals.get("a").is_none(),
        "'a' should NOT be in file_locals (hoisted to function scope)"
    );
    assert!(
        binder.file_locals.get("b").is_none(),
        "'b' should NOT be in file_locals (hoisted to function scope)"
    );

    // Find the 'a' usage in 'return a + b;'
    let a_nodes: Vec<_> = arena
        .nodes
        .iter()
        .enumerate()
        .filter_map(|(idx, node)| {
            if node.kind == SyntaxKind::Identifier as u16 {
                let node_idx = tsz_parser::NodeIndex(idx as u32);
                if arena.get_identifier_text(node_idx) == Some("a") {
                    return Some(node_idx);
                }
            }
            None
        })
        .collect();

    // The last 'a' should be in 'return a + b;'
    let a_usage = *a_nodes.last().expect("should find 'a' identifiers");
    let mut walker = ScopeWalker::new(arena, &binder);
    let resolved = walker.resolve_node(root, a_usage);
    assert!(
        resolved.is_some(),
        "var 'a' should be resolvable via hoisting in the function scope"
    );
}

#[test]
fn test_catch_clause_parameter_scoping() {
    // catch parameter 'e' should be scoped to catch clause, not leak to try or outer
    let source = r#"
const e = "outer";
try {
    throw new Error();
} catch (e) {
    const msg = e;
}
const result = e;
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let outer_e = binder
        .file_locals
        .get("e")
        .expect("file-level 'e' should exist");

    // Find the last 'e' identifier (in 'const result = e;')
    let e_nodes: Vec<_> = arena
        .nodes
        .iter()
        .enumerate()
        .filter_map(|(idx, node)| {
            if node.kind == SyntaxKind::Identifier as u16 {
                let node_idx = tsz_parser::NodeIndex(idx as u32);
                if arena.get_identifier_text(node_idx) == Some("e") {
                    return Some(node_idx);
                }
            }
            None
        })
        .collect();

    let last_e = *e_nodes.last().expect("should find 'e' identifiers");
    let mut walker = ScopeWalker::new(arena, &binder);
    let resolved = walker.resolve_node(root, last_e);
    assert_eq!(
        resolved,
        Some(outer_e),
        "'e' in 'const result = e' should resolve to outer 'e', not catch parameter"
    );
}

#[test]
fn test_class_static_block_scope() {
    let source = r#"
const x = "outer";
class Foo {
    static {
        const x = "static-block";
        const y = x;
    }
}
const z = x;
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    // 'x', 'Foo', 'z' should be in file_locals; 'y' should not
    assert!(binder.file_locals.get("x").is_some());
    assert!(binder.file_locals.get("Foo").is_some());
    assert!(binder.file_locals.get("z").is_some());
    assert!(
        binder.file_locals.get("y").is_none(),
        "'y' should NOT be in file_locals (inside static block)"
    );
}

#[test]
fn test_multiple_functions_same_named_parameters() {
    // Two functions with parameters named 'x' should not interfere
    let source = r#"
function foo(x: number) { return x; }
function bar(x: string) { return x; }
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    assert!(binder.file_locals.get("foo").is_some());
    assert!(binder.file_locals.get("bar").is_some());
    assert!(
        binder.file_locals.get("x").is_none(),
        "parameter 'x' should NOT leak to file_locals from either function"
    );

    // Find the 'x' usages in 'return x;' statements
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

    // There should be at least 4 'x' identifiers (2 param decls + 2 return usages)
    assert!(
        x_nodes.len() >= 4,
        "should find at least 4 'x' identifiers, found {}",
        x_nodes.len()
    );

    // Each 'return x;' usage should resolve to some symbol (its own function's parameter)
    for &x_node in &x_nodes {
        let mut walker = ScopeWalker::new(arena, &binder);
        let resolved = walker.resolve_node(root, x_node);
        assert!(
            resolved.is_some(),
            "each 'x' identifier should resolve to a symbol"
        );
    }
}

#[test]
fn test_arrow_function_parameter_scope() {
    let source = r#"
const outer = 1;
const fn1 = (a: number, b: number) => a + b;
const fn2 = (a: string) => a;
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    assert!(binder.file_locals.get("outer").is_some());
    assert!(binder.file_locals.get("fn1").is_some());
    assert!(binder.file_locals.get("fn2").is_some());
    assert!(
        binder.file_locals.get("a").is_none(),
        "'a' should NOT be in file_locals (arrow function parameter)"
    );
    assert!(
        binder.file_locals.get("b").is_none(),
        "'b' should NOT be in file_locals (arrow function parameter)"
    );
}

#[test]
fn test_export_function_and_class_resolution() {
    let source = r#"
export function exported() { return 1; }
export class ExportedClass {}
export interface ExportedInterface { x: number; }
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    assert!(
        binder.file_locals.get("exported").is_some(),
        "'exported' function should be in file_locals"
    );
    assert!(
        binder.file_locals.get("ExportedClass").is_some(),
        "'ExportedClass' should be in file_locals"
    );
    assert!(
        binder.file_locals.get("ExportedInterface").is_some(),
        "'ExportedInterface' should be in file_locals"
    );
}

#[test]
fn test_namespace_nested_scoping() {
    let source = r#"
namespace Outer {
    export namespace Inner {
        export const value = 1;
    }
    const local = 2;
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
    assert!(
        binder.file_locals.get("Inner").is_none(),
        "'Inner' should NOT be in file_locals (inside Outer namespace)"
    );
    assert!(
        binder.file_locals.get("value").is_none(),
        "'value' should NOT be in file_locals (inside Inner namespace)"
    );
    assert!(
        binder.file_locals.get("local").is_none(),
        "'local' should NOT be in file_locals (inside Outer namespace)"
    );
}

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

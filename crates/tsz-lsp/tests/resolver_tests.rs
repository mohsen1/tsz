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
        binder.node_symbols.insert(parent_idx.0, symbol_id);
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
fn test_resolve_export_module_namespace_string_literal_uses_property_symbol_fallback() {
    let source = r#"
const foo = "foo";
export { foo as "__<alias>" };
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let export_literal = arena
        .nodes
        .iter()
        .enumerate()
        .find_map(|(idx, node)| {
            if node.kind == SyntaxKind::StringLiteral as u16 {
                let node_idx = tsz_parser::NodeIndex(idx as u32);
                if arena.get_literal_text(node_idx) == Some("__<alias>") {
                    return Some(node_idx);
                }
            }
            None
        })
        .expect("expected export string-literal alias node");

    let mut walker = ScopeWalker::new(arena, &binder);
    let resolved = walker
        .resolve_node(root, export_literal)
        .expect("quoted export alias should resolve to an existing symbol");
    let foo_symbol = binder
        .file_locals
        .get("foo")
        .expect("foo symbol should be bound in file locals");
    assert_eq!(
        resolved, foo_symbol,
        "export-side quoted alias should resolve via the specifier property symbol when alias literal has no direct symbol"
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
        .last(); // The last occurrence should be the usage in 'x + 1'

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
        .last()
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
        .last()
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

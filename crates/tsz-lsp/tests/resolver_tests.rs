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

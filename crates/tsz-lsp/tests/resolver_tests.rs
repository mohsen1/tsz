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

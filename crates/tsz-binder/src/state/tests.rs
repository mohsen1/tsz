use super::BinderState;
use crate::{SymbolTable, symbol_flags};
use tsz_parser::parser::ParserState;

#[test]
fn test_namespace_exports_exclude_non_exported_members() {
    let source = r"
namespace M {
    export class A {}
    class B {}
}
";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    let m_sym_id = binder
        .file_locals
        .get("M")
        .expect("expected namespace symbol for M");
    let symbol = binder
        .symbols
        .get(m_sym_id)
        .expect("expected namespace symbol data");
    let exports = symbol.exports.as_ref().expect("expected exports table");

    assert!(exports.has("A"), "expected A to be exported");
    assert!(!exports.has("B"), "expected B to be non-exported");
}

#[test]
fn records_import_metadata_for_exported_reexports() {
    let source = r"
export { A, B as C } from './a';
export type { D as E } from './b';
";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    let a_sym_id = binder
        .file_locals
        .get("A")
        .expect("expected re-exported symbol A");
    let a_symbol = binder
        .symbols
        .get(a_sym_id)
        .expect("expected symbol data for A");
    assert_eq!(a_symbol.import_module.as_deref(), Some("./a"));
    assert_eq!(a_symbol.import_name.as_deref(), Some("A"));
    assert!(!a_symbol.is_type_only);

    let c_sym_id = binder
        .file_locals
        .get("C")
        .expect("expected re-exported symbol C");
    let c_symbol = binder
        .symbols
        .get(c_sym_id)
        .expect("expected symbol data for C");
    assert_eq!(c_symbol.import_module.as_deref(), Some("./a"));
    assert_eq!(c_symbol.import_name.as_deref(), Some("B"));
    assert!(!c_symbol.is_type_only);

    let e_sym_id = binder
        .file_locals
        .get("E")
        .expect("expected type-only re-exported symbol E");
    let e_symbol = binder
        .symbols
        .get(e_sym_id)
        .expect("expected symbol data for E");
    assert_eq!(e_symbol.import_module.as_deref(), Some("./b"));
    assert_eq!(e_symbol.import_name.as_deref(), Some("D"));
    assert!(e_symbol.is_type_only);
}

#[test]
fn resolves_wildcard_type_only_reexports_with_provenance() {
    let mut binder = BinderState::new();

    let a_sym = binder.symbols.alloc(symbol_flags::CLASS, "A".to_string());
    let b_sym = binder.symbols.alloc(symbol_flags::CLASS, "B".to_string());

    let mut a_exports = SymbolTable::new();
    a_exports.set("A".to_string(), a_sym);
    a_exports.set("B".to_string(), b_sym);
    binder.module_exports.insert("./a".to_string(), a_exports);

    binder
        .wildcard_reexports
        .entry("./b".to_string())
        .or_default()
        .push("./a".to_string());
    binder
        .wildcard_reexports_type_only
        .entry("./b".to_string())
        .or_default()
        .push(("./a".to_string(), true));

    binder
        .wildcard_reexports
        .entry("./c".to_string())
        .or_default()
        .push("./b".to_string());
    binder
        .wildcard_reexports_type_only
        .entry("./c".to_string())
        .or_default()
        .push(("./b".to_string(), false));

    binder
        .wildcard_reexports
        .entry("./d".to_string())
        .or_default()
        .push("./a".to_string());
    binder
        .wildcard_reexports_type_only
        .entry("./d".to_string())
        .or_default()
        .push(("./a".to_string(), false));

    let (resolved_a, is_type_only_a) = binder
        .resolve_import_with_reexports_type_only("./c", "A")
        .expect("expected type-only wildcard chain from './c' -> './b' -> './a'");
    assert_eq!(resolved_a, a_sym);
    assert!(is_type_only_a);

    let (resolved_b, is_type_only_b) = binder
        .resolve_import_with_reexports_type_only("./c", "B")
        .expect("expected type-only wildcard chain from './c' -> './b' -> './a'");
    assert_eq!(resolved_b, b_sym);
    assert!(is_type_only_b);

    let (resolved_a_value, is_type_only_value) = binder
        .resolve_import_with_reexports_type_only("./d", "A")
        .expect("expected value wildcard chain from './d' -> './a'");
    assert_eq!(resolved_a_value, a_sym);
    assert!(!is_type_only_value);
}

#[test]
fn global_augmentation_namespace_appears_in_file_locals() {
    // `declare global { namespace JSX { ... } }` inside a module declaration
    // should make the JSX namespace visible at the file level (in file_locals),
    // since `global` escapes the module scope.
    let source = r#"
declare module "react" {
    global {
        namespace JSX {
            interface IntrinsicElements {
                div: any;
                span: any;
            }
        }
    }
}
"#;
    let mut parser = ParserState::new("react.d.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    // JSX namespace should be in file_locals because it's inside `declare global`
    let jsx_sym_id = binder
        .file_locals
        .get("JSX")
        .expect("expected JSX namespace in file_locals from global augmentation");
    let jsx_symbol = binder
        .symbols
        .get(jsx_sym_id)
        .expect("expected symbol data for JSX");

    // JSX should be a namespace/module
    assert!(
        jsx_symbol.flags & symbol_flags::NAMESPACE_MODULE != 0,
        "JSX should have NAMESPACE_MODULE flag"
    );

    // JSX should have IntrinsicElements in its exports
    let exports = jsx_symbol
        .exports
        .as_ref()
        .expect("expected JSX to have exports");
    assert!(
        exports.has("IntrinsicElements"),
        "expected IntrinsicElements in JSX exports"
    );

    // JSX should also be tracked as a global augmentation
    assert!(
        binder.global_augmentations.contains_key("JSX"),
        "expected JSX in global_augmentations"
    );
}

#[test]
fn iife_no_flow_start_node() {
    // For a non-async, non-generator IIFE, the binder should NOT create a
    // FlowStart node for the function body. This means the IIFE body runs
    // in the outer flow context.
    use crate::flow::flow_flags;

    let source = r"
let x: number | undefined;
(function() {
    x = 1;
})();
";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    // Count START nodes. There should be exactly 1 (the file-level start),
    // NOT 2 (file + IIFE body).
    let start_count = (0..binder.flow_nodes.len())
        .filter(|&i| {
            binder
                .flow_nodes
                .get(crate::flow::FlowNodeId(i as u32))
                .is_some_and(|n| n.has_any_flags(flow_flags::START))
        })
        .count();
    assert_eq!(
        start_count, 1,
        "IIFE body should not create a FlowStart node"
    );
}

#[test]
fn non_iife_function_gets_flow_start_node() {
    // A regular (non-IIFE) function expression SHOULD get a FlowStart node.
    use crate::flow::flow_flags;

    let source = r"
let x: number | undefined;
let f = function() {
    x = 1;
};
";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    // Count START nodes. Should be 2: one for the file, one for the function body.
    let start_count = (0..binder.flow_nodes.len())
        .filter(|&i| {
            binder
                .flow_nodes
                .get(crate::flow::FlowNodeId(i as u32))
                .is_some_and(|n| n.has_any_flags(flow_flags::START))
        })
        .count();
    assert_eq!(
        start_count, 2,
        "non-IIFE function should create a FlowStart node"
    );
}

#[test]
fn async_iife_gets_flow_start_node() {
    // An async IIFE should still get a FlowStart node (not treated as inline).
    use crate::flow::flow_flags;

    let source = r"
let x: number | undefined;
(async function() {
    x = 1;
})();
";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    let start_count = (0..binder.flow_nodes.len())
        .filter(|&i| {
            binder
                .flow_nodes
                .get(crate::flow::FlowNodeId(i as u32))
                .is_some_and(|n| n.has_any_flags(flow_flags::START))
        })
        .count();
    assert_eq!(
        start_count, 2,
        "async IIFE should still create a FlowStart node"
    );
}

#[test]
fn generator_iife_gets_flow_start_node() {
    // A generator IIFE should still get a FlowStart node (not treated as inline).
    use crate::flow::flow_flags;

    let source = r"
let x: number | undefined;
(function*() {
    x = 1;
})();
";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    let start_count = (0..binder.flow_nodes.len())
        .filter(|&i| {
            binder
                .flow_nodes
                .get(crate::flow::FlowNodeId(i as u32))
                .is_some_and(|n| n.has_any_flags(flow_flags::START))
        })
        .count();
    assert_eq!(
        start_count, 2,
        "generator IIFE should still create a FlowStart node"
    );
}

#[test]
fn arrow_iife_no_flow_start_node() {
    // Arrow function IIFE should also be treated as inline (no FlowStart).
    use crate::flow::flow_flags;

    let source = r"
let x: number | undefined;
(() => {
    x = 1;
})();
";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    let start_count = (0..binder.flow_nodes.len())
        .filter(|&i| {
            binder
                .flow_nodes
                .get(crate::flow::FlowNodeId(i as u32))
                .is_some_and(|n| n.has_any_flags(flow_flags::START))
        })
        .count();
    assert_eq!(
        start_count, 1,
        "arrow IIFE should not create a FlowStart node"
    );
}

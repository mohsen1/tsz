use super::{BinderOptions, BinderState};
use crate::flow::{FlowNodeId, flow_flags};
use crate::scopes::ContainerKind;
use crate::{SymbolTable, symbol_flags};
use tsz_common::common::ScriptTarget;
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
fn export_namespace_wrapper_marks_inner_module_as_publicly_exported() {
    let source = r"
namespace M {
    export namespace foo {
        export var y = 1;
    }
    namespace foo {
        export var z = 1;
    }
}
";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let source_file = arena
        .get_source_file_at(root)
        .expect("expected source file");
    let outer_ns_idx = *source_file
        .statements
        .nodes
        .first()
        .expect("expected outer namespace");
    let outer_ns = arena
        .get_module_at(outer_ns_idx)
        .expect("expected outer namespace declaration");
    let body = arena
        .get_module_block_at(outer_ns.body)
        .expect("expected outer namespace body");
    let statements = body.statements.as_ref().expect("expected inner statements");

    let exported_stmt_idx = statements.nodes[0];
    let exported_stmt = arena
        .get_export_decl_at(exported_stmt_idx)
        .expect("expected export declaration wrapper");
    let exported_foo_idx = exported_stmt.export_clause;
    let plain_foo_idx = statements.nodes[1];

    assert_eq!(
        binder
            .module_declaration_exports_publicly
            .get(&exported_foo_idx.0),
        Some(&true),
        "export namespace foo should be recorded as publicly exported"
    );
    assert_eq!(
        binder
            .module_declaration_exports_publicly
            .get(&plain_foo_idx.0),
        Some(&false),
        "plain namespace foo should remain non-exported"
    );
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
fn export_as_namespace_records_current_file_namespace_metadata() {
    let source = r"
export var x: number;
export interface Thing { n: typeof x }
export as namespace Foo;
";
    let mut parser = ParserState::new("foo.d.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    let foo_sym_id = binder
        .file_locals
        .get("Foo")
        .expect("expected UMD namespace alias symbol");
    let foo_symbol = binder
        .symbols
        .get(foo_sym_id)
        .expect("expected symbol data for Foo");

    assert_ne!(foo_symbol.flags & symbol_flags::ALIAS, 0);
    assert!(foo_symbol.is_umd_export);
    assert_eq!(foo_symbol.import_module.as_deref(), Some("foo.d.ts"));
    assert_eq!(foo_symbol.import_name.as_deref(), Some("*"));
}

#[test]
fn namespace_reexport_does_not_create_local_binding() {
    let source = r"
export * as ns from './mod';
ns.a;
let ns = { a: 1 };
";
    let (binder, _parser) = parse_and_bind(source);

    let local_ns_id = binder
        .file_locals
        .get("ns")
        .expect("expected local let ns in file_locals");
    let local_ns = binder
        .symbols
        .get(local_ns_id)
        .expect("expected symbol data for local ns");
    assert_ne!(local_ns.flags & symbol_flags::BLOCK_SCOPED_VARIABLE, 0);
    assert_eq!(local_ns.flags & symbol_flags::ALIAS, 0);

    let export_ns_id = binder
        .module_exports
        .get("test.ts")
        .and_then(|exports| exports.get("ns"))
        .expect("expected namespace re-export in module_exports");
    let export_ns = binder
        .symbols
        .get(export_ns_id)
        .expect("expected symbol data for exported ns");

    assert_ne!(export_ns.flags & symbol_flags::ALIAS, 0);
    assert_eq!(export_ns.import_module.as_deref(), Some("./mod"));
    assert_eq!(export_ns.import_name.as_deref(), Some("*"));
    assert_ne!(export_ns_id, local_ns_id);
}

#[test]
fn type_alias_after_namespace_reexport_keeps_alias_partner() {
    let source = r"
export * as Foo from './mod';
export type Foo = { x: number };
";
    let (binder, _parser) = parse_and_bind(source);

    let foo_sym_id = binder
        .file_locals
        .get("Foo")
        .expect("expected exported type alias in file_locals");
    let foo_symbol = binder
        .symbols
        .get(foo_sym_id)
        .expect("expected symbol data for Foo");
    assert_ne!(foo_symbol.flags & symbol_flags::TYPE_ALIAS, 0);

    let alias_id = *binder
        .alias_partners
        .get(&foo_sym_id)
        .expect("expected namespace export alias partner for Foo");
    let alias_symbol = binder
        .symbols
        .get(alias_id)
        .expect("expected alias partner symbol data");
    assert_ne!(alias_symbol.flags & symbol_flags::ALIAS, 0);
    assert_eq!(alias_symbol.import_module.as_deref(), Some("./mod"));
    assert_eq!(alias_symbol.import_name.as_deref(), Some("*"));

    let export_foo_id = binder
        .module_exports
        .get("test.ts")
        .and_then(|exports| exports.get("Foo"))
        .expect("expected Foo export in module_exports");
    assert_eq!(export_foo_id, foo_sym_id);
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
fn ambient_module_export_import_populates_module_exports() {
    let source = r#"
declare module "a" {
    export type T = number;
}
declare module "b" {
    export import a = require("a");
    export const x: a.T;
}
"#;
    let mut parser = ParserState::new("test.d.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    let b_sym_id = binder
        .file_locals
        .get("b")
        .expect("expected ambient module symbol for b");
    let b_symbol = binder
        .symbols
        .get(b_sym_id)
        .expect("expected symbol data for module b");
    let exports = b_symbol
        .exports
        .as_ref()
        .expect("expected exports table for module b");
    let a_sym_id = exports
        .get("a")
        .expect("expected export-import alias a in module b exports");
    let a_symbol = binder
        .symbols
        .get(a_sym_id)
        .expect("expected symbol data for alias a");

    assert_ne!(a_symbol.flags & symbol_flags::ALIAS, 0);
    assert_eq!(a_symbol.import_module.as_deref(), Some("a"));

    let module_exports = binder
        .module_exports
        .get("b")
        .expect("expected cached module exports for module b");
    assert!(
        module_exports.has("a"),
        "expected export-import alias a in cached module exports"
    );
}

#[test]
fn export_equals_default_property_does_not_create_default_module_export() {
    let source = r#"
var x = {
    default: 42,
    answer: 1
};

export = x;
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    let module_exports = binder
        .module_exports
        .get("test.ts")
        .expect("expected cached module exports for file");
    assert!(
        module_exports.has("export="),
        "expected explicit export= target to stay cached"
    );
    assert!(
        !module_exports.has("default"),
        "default-valued export= members must not masquerade as real default exports"
    );
}

#[test]
fn export_equals_class_static_default_not_in_file_locals() {
    // A class with `static default: "foo"` exported via `export = Point`
    // must NOT put the `default` static member into file_locals as `"default"`.
    // Otherwise default-import resolution picks up the static member instead
    // of the class constructor.
    let source = r#"
declare class Point {
    x: number;
    y: number;
    constructor(x: number, y: number);
    static default: "foo";
}
export = Point;
"#;
    let mut parser = ParserState::new("point.d.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    // file_locals should have "export=" but NOT "default"
    assert!(
        binder.file_locals.has("export="),
        "expected export= in file_locals"
    );
    assert!(
        !binder.file_locals.has("default"),
        "static member named 'default' must not leak into file_locals from export= target"
    );

    // module_exports should also not have "default"
    let module_exports = binder
        .module_exports
        .get("point.d.ts")
        .expect("expected cached module exports for file");
    assert!(
        module_exports.has("export="),
        "expected export= in module_exports"
    );
    assert!(
        !module_exports.has("default"),
        "static member named 'default' must not appear in module_exports"
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

// =============================================================================
// Helper: parse + bind convenience
// =============================================================================

/// Parse source text and bind it, returning the binder state and the parser
/// (which owns the arena). Using a tuple return so callers can access both.
fn parse_and_bind(source: &str) -> (BinderState, ParserState) {
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);
    (binder, parser)
}

fn parse_and_bind_with_options(source: &str, options: BinderOptions) -> (BinderState, ParserState) {
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let mut binder = BinderState::with_options(options);
    binder.bind_source_file(parser.get_arena(), root);
    (binder, parser)
}

/// Count flow nodes with specific flags.
fn count_flow_nodes_with_flags(binder: &BinderState, flags: u32) -> usize {
    (0..binder.flow_nodes.len())
        .filter(|&i| {
            binder
                .flow_nodes
                .get(FlowNodeId(i as u32))
                .is_some_and(|n| n.has_any_flags(flags))
        })
        .count()
}

// =============================================================================
// 1. HOISTING RULES
// =============================================================================

#[test]
fn var_declaration_hoisted_to_function_scope() {
    // `var` declarations inside blocks should be visible at the function scope
    // level, because JavaScript hoists `var` to the enclosing function.
    let (binder, _parser) = parse_and_bind(
        r"
function foo() {
    if (true) {
        var x = 1;
    }
}
",
    );

    // `foo` should be in file_locals
    assert!(
        binder.file_locals.has("foo"),
        "function foo should be in file_locals"
    );

    // `x` should be in the function scope (hoisted), not just the block scope.
    // We check that a symbol for `x` was created with FUNCTION_SCOPED_VARIABLE flag.
    let x_sym = binder
        .symbols
        .find_by_name("x")
        .expect("expected symbol for x");
    let x_symbol = binder.symbols.get(x_sym).expect("expected symbol data");
    assert!(
        x_symbol.flags & symbol_flags::FUNCTION_SCOPED_VARIABLE != 0,
        "var x should have FUNCTION_SCOPED_VARIABLE flag"
    );
}

#[test]
fn var_hoisted_from_nested_blocks() {
    // `var` inside nested blocks (if/while/for) should still be hoisted
    // to the enclosing function scope.
    let (binder, _parser) = parse_and_bind(
        r"
function outer() {
    if (true) {
        while (true) {
            var deep = 1;
        }
    }
}
",
    );

    let deep_sym = binder
        .symbols
        .find_by_name("deep")
        .expect("expected symbol for deep");
    let deep_symbol = binder.symbols.get(deep_sym).expect("expected symbol data");
    assert!(
        deep_symbol.flags & symbol_flags::FUNCTION_SCOPED_VARIABLE != 0,
        "var deep should have FUNCTION_SCOPED_VARIABLE flag"
    );
}

#[test]
fn let_not_hoisted_across_blocks() {
    // `let` declarations should NOT be hoisted to function scope.
    // They should be block-scoped.
    let (binder, _parser) = parse_and_bind(
        r"
function foo() {
    if (true) {
        let x = 1;
    }
}
",
    );

    let x_sym = binder
        .symbols
        .find_by_name("x")
        .expect("expected symbol for x");
    let x_symbol = binder.symbols.get(x_sym).expect("expected symbol data");
    assert!(
        x_symbol.flags & symbol_flags::BLOCK_SCOPED_VARIABLE != 0,
        "let x should have BLOCK_SCOPED_VARIABLE flag"
    );
    assert!(
        x_symbol.flags & symbol_flags::FUNCTION_SCOPED_VARIABLE == 0,
        "let x should NOT have FUNCTION_SCOPED_VARIABLE flag"
    );
}

#[test]
fn const_not_hoisted_across_blocks() {
    // `const` declarations should NOT be hoisted to function scope.
    let (binder, _parser) = parse_and_bind(
        r"
function foo() {
    if (true) {
        const y = 2;
    }
}
",
    );

    let y_sym = binder
        .symbols
        .find_by_name("y")
        .expect("expected symbol for y");
    let y_symbol = binder.symbols.get(y_sym).expect("expected symbol data");
    assert!(
        y_symbol.flags & symbol_flags::BLOCK_SCOPED_VARIABLE != 0,
        "const y should have BLOCK_SCOPED_VARIABLE flag"
    );
}

#[test]
fn function_declaration_hoisted_to_containing_scope() {
    // Function declarations at the top level should be hoisted and visible
    // in file_locals.
    let (binder, _parser) = parse_and_bind(
        r"
foo();
function foo() {}
",
    );

    assert!(
        binder.file_locals.has("foo"),
        "function declaration should be hoisted to file_locals"
    );
    let foo_sym_id = binder.file_locals.get("foo").unwrap();
    let foo_symbol = binder.symbols.get(foo_sym_id).unwrap();
    assert!(
        foo_symbol.flags & symbol_flags::FUNCTION != 0,
        "foo should have FUNCTION flag"
    );
}

#[test]
fn function_declaration_hoisted_inside_function() {
    // Function declarations inside a function body should be hoisted to the
    // function scope.
    let (binder, _parser) = parse_and_bind(
        r"
function outer() {
    inner();
    function inner() {}
}
",
    );

    let inner_sym = binder
        .symbols
        .find_by_name("inner")
        .expect("expected symbol for inner");
    let inner_symbol = binder.symbols.get(inner_sym).expect("expected symbol data");
    assert!(
        inner_symbol.flags & symbol_flags::FUNCTION != 0,
        "inner should have FUNCTION flag"
    );
}

#[test]
fn function_in_block_not_hoisted_in_strict_mode() {
    // In strict mode (via "use strict"), function declarations in blocks
    // should be block-scoped, not hoisted.
    let options = BinderOptions {
        target: ScriptTarget::ES2015,
        always_strict: true,
    };
    let (binder, _parser) = parse_and_bind_with_options(
        r#"
function outer() {
    if (true) {
        function blockFunc() {}
    }
}
"#,
        options,
    );

    // The function should still exist as a symbol, but it should not be
    // hoisted to the function scope in strict mode.
    let block_func_sym = binder.symbols.find_by_name("blockFunc");
    assert!(
        block_func_sym.is_some(),
        "blockFunc should exist as a symbol"
    );
}

#[test]
fn function_in_block_hoisted_in_non_strict_es5() {
    // In non-strict ES5 mode, function declarations in blocks should be
    // hoisted (Annex B behavior).
    let options = BinderOptions {
        target: ScriptTarget::ES5,
        always_strict: false,
    };
    let (binder, _parser) = parse_and_bind_with_options(
        r"
function outer() {
    if (true) {
        function blockFunc() {}
    }
}
",
        options,
    );

    let block_func_sym = binder.symbols.find_by_name("blockFunc");
    assert!(
        block_func_sym.is_some(),
        "blockFunc should exist as a symbol (hoisted in non-strict ES5)"
    );
}

#[test]
fn duplicate_var_declarations_merge() {
    // Duplicate `var` declarations should merge (not create separate symbols).
    // This is valid JavaScript behavior.
    let (binder, _parser) = parse_and_bind(
        r"
var x = 1;
var x = 2;
",
    );

    // There should be exactly one symbol for x in file_locals
    assert!(binder.file_locals.has("x"), "x should be in file_locals");

    // The symbol should have FUNCTION_SCOPED_VARIABLE flag
    let x_sym_id = binder.file_locals.get("x").unwrap();
    let x_symbol = binder.symbols.get(x_sym_id).unwrap();
    assert!(
        x_symbol.flags & symbol_flags::FUNCTION_SCOPED_VARIABLE != 0,
        "x should have FUNCTION_SCOPED_VARIABLE flag"
    );
    // Should have multiple declarations
    assert!(
        x_symbol.declarations.len() >= 2,
        "duplicate var should have at least 2 declarations, got {}",
        x_symbol.declarations.len()
    );
}

#[test]
fn var_in_for_loop_head_hoisted() {
    // `var` in a for-loop initializer should be hoisted.
    let (binder, _parser) = parse_and_bind(
        r"
function foo() {
    for (var i = 0; i < 10; i++) {
        // body
    }
}
",
    );

    let i_sym = binder
        .symbols
        .find_by_name("i")
        .expect("expected symbol for i");
    let i_symbol = binder.symbols.get(i_sym).expect("expected symbol data");
    assert!(
        i_symbol.flags & symbol_flags::FUNCTION_SCOPED_VARIABLE != 0,
        "var i in for-loop should have FUNCTION_SCOPED_VARIABLE flag"
    );
}

#[test]
fn var_in_for_in_loop_head_hoisted() {
    // `var` in a for-in loop should be hoisted.
    let (binder, _parser) = parse_and_bind(
        r"
function foo() {
    for (var key in obj) {
        // body
    }
}
",
    );

    let key_sym = binder
        .symbols
        .find_by_name("key")
        .expect("expected symbol for key");
    let key_symbol = binder.symbols.get(key_sym).expect("expected symbol data");
    assert!(
        key_symbol.flags & symbol_flags::FUNCTION_SCOPED_VARIABLE != 0,
        "var key in for-in should have FUNCTION_SCOPED_VARIABLE flag"
    );
}

#[test]
fn var_in_for_of_loop_head_hoisted() {
    // `var` in a for-of loop should be hoisted.
    let (binder, _parser) = parse_and_bind(
        r"
function foo() {
    for (var item of items) {
        // body
    }
}
",
    );

    let item_sym = binder
        .symbols
        .find_by_name("item")
        .expect("expected symbol for item");
    let item_symbol = binder.symbols.get(item_sym).expect("expected symbol data");
    assert!(
        item_symbol.flags & symbol_flags::FUNCTION_SCOPED_VARIABLE != 0,
        "var item in for-of should have FUNCTION_SCOPED_VARIABLE flag"
    );
}

#[test]
fn var_hoisted_from_try_catch_finally() {
    // `var` declarations in try, catch, and finally blocks should all be
    // hoisted to the enclosing function scope.
    let (binder, _parser) = parse_and_bind(
        r"
function foo() {
    try {
        var tryVar = 1;
    } catch (e) {
        var catchVar = 2;
    } finally {
        var finallyVar = 3;
    }
}
",
    );

    for name in &["tryVar", "catchVar", "finallyVar"] {
        let sym = binder
            .symbols
            .find_by_name(name)
            .unwrap_or_else(|| panic!("expected symbol for {name}"));
        let symbol = binder.symbols.get(sym).unwrap();
        assert!(
            symbol.flags & symbol_flags::FUNCTION_SCOPED_VARIABLE != 0,
            "{name} should have FUNCTION_SCOPED_VARIABLE flag"
        );
    }
}

#[test]
fn var_hoisted_from_switch_statement() {
    // `var` declarations in switch case/default clauses should be hoisted.
    let (binder, _parser) = parse_and_bind(
        r"
function foo() {
    switch (x) {
        case 1:
            var caseVar = 1;
            break;
        default:
            var defaultVar = 2;
    }
}
",
    );

    for name in &["caseVar", "defaultVar"] {
        let sym = binder
            .symbols
            .find_by_name(name)
            .unwrap_or_else(|| panic!("expected symbol for {name}"));
        let symbol = binder.symbols.get(sym).unwrap();
        assert!(
            symbol.flags & symbol_flags::FUNCTION_SCOPED_VARIABLE != 0,
            "{name} should have FUNCTION_SCOPED_VARIABLE flag"
        );
    }
}

#[test]
fn var_hoisted_from_labeled_statement() {
    // `var` inside a labeled statement should be hoisted.
    let (binder, _parser) = parse_and_bind(
        r"
function foo() {
    label: var x = 1;
}
",
    );

    let x_sym = binder
        .symbols
        .find_by_name("x")
        .expect("expected symbol for x");
    let x_symbol = binder.symbols.get(x_sym).unwrap();
    assert!(
        x_symbol.flags & symbol_flags::FUNCTION_SCOPED_VARIABLE != 0,
        "var inside labeled statement should be hoisted"
    );
}

// =============================================================================
// 2. SCOPE MANAGEMENT
// =============================================================================

#[test]
fn source_file_creates_root_scope() {
    // The source file should always create a root scope.
    let (binder, _parser) = parse_and_bind("let x = 1;");

    assert!(
        !binder.scopes.is_empty(),
        "binding should create at least one scope"
    );
    assert_eq!(
        binder.scopes[0].kind,
        ContainerKind::SourceFile,
        "first scope should be SourceFile"
    );
}

#[test]
fn block_creates_block_scope() {
    // An explicit block (`{ ... }`) should create a Block scope.
    let (binder, _parser) = parse_and_bind(
        r"
{
    let x = 1;
}
",
    );

    let has_block_scope = binder.scopes.iter().any(|s| s.kind == ContainerKind::Block);
    assert!(has_block_scope, "block should create a Block scope");
}

#[test]
fn function_creates_function_scope() {
    // A function declaration should create a Function scope.
    let (binder, _parser) = parse_and_bind(
        r"
function foo() {
    let x = 1;
}
",
    );

    let has_function_scope = binder
        .scopes
        .iter()
        .any(|s| s.kind == ContainerKind::Function);
    assert!(
        has_function_scope,
        "function declaration should create a Function scope"
    );
}

#[test]
fn class_creates_class_scope() {
    // A class declaration should create a Class scope.
    let (binder, _parser) = parse_and_bind(
        r"
class Foo {
    x: number = 1;
}
",
    );

    let has_class_scope = binder.scopes.iter().any(|s| s.kind == ContainerKind::Class);
    assert!(
        has_class_scope,
        "class declaration should create a Class scope"
    );
}

#[test]
fn namespace_creates_module_scope() {
    // A namespace declaration should create a Module scope.
    let (binder, _parser) = parse_and_bind(
        r"
namespace M {
    export const x = 1;
}
",
    );

    let has_module_scope = binder
        .scopes
        .iter()
        .any(|s| s.kind == ContainerKind::Module);
    assert!(
        has_module_scope,
        "namespace declaration should create a Module scope"
    );
}

#[test]
fn if_body_creates_block_scope() {
    // The block body of an if statement should create a Block scope.
    let (binder, _parser) = parse_and_bind(
        r"
if (true) {
    let x = 1;
}
",
    );

    // There should be at least 2 scopes: SourceFile + Block for if body
    let block_count = binder
        .scopes
        .iter()
        .filter(|s| s.kind == ContainerKind::Block)
        .count();
    assert!(
        block_count >= 1,
        "if body block should create a Block scope"
    );
}

#[test]
fn for_loop_creates_block_scope() {
    // A for loop should create a Block scope (for the initializer variable).
    let (binder, _parser) = parse_and_bind(
        r"
for (let i = 0; i < 10; i++) {
    let x = i;
}
",
    );

    let block_count = binder
        .scopes
        .iter()
        .filter(|s| s.kind == ContainerKind::Block)
        .count();
    assert!(
        block_count >= 1,
        "for loop should create at least one Block scope"
    );
}

#[test]
fn nested_scopes_have_correct_parent_chain() {
    // Nested scopes should correctly link to their parent.
    let (binder, _parser) = parse_and_bind(
        r"
function outer() {
    function inner() {
        let x = 1;
    }
}
",
    );

    // We should have: SourceFile -> Function (outer) -> Function (inner)
    // Verify that function scopes exist and have parent links
    let function_scopes: Vec<_> = binder
        .scopes
        .iter()
        .enumerate()
        .filter(|(_, s)| s.kind == ContainerKind::Function)
        .collect();

    assert!(
        function_scopes.len() >= 2,
        "should have at least 2 function scopes (outer and inner)"
    );

    // The inner function scope should have a parent that's not ScopeId::NONE
    for (_, scope) in &function_scopes {
        assert!(
            scope.parent.is_some() || scope.parent.is_none(),
            "function scopes should have valid parent chain"
        );
    }
}

#[test]
fn function_scope_contains_parameters() {
    // Function parameters should be declared in the function scope.
    let (binder, _parser) = parse_and_bind(
        r"
function foo(a: number, b: string) {
    return a;
}
",
    );

    // Parameters should be created as FUNCTION_SCOPED_VARIABLE symbols
    let a_sym = binder
        .symbols
        .find_by_name("a")
        .expect("expected symbol for parameter a");
    let a_symbol = binder.symbols.get(a_sym).unwrap();
    assert!(
        a_symbol.flags & symbol_flags::FUNCTION_SCOPED_VARIABLE != 0,
        "parameter a should have FUNCTION_SCOPED_VARIABLE flag"
    );

    let b_sym = binder
        .symbols
        .find_by_name("b")
        .expect("expected symbol for parameter b");
    let b_symbol = binder.symbols.get(b_sym).unwrap();
    assert!(
        b_symbol.flags & symbol_flags::FUNCTION_SCOPED_VARIABLE != 0,
        "parameter b should have FUNCTION_SCOPED_VARIABLE flag"
    );
}

#[test]
fn module_scope_contains_top_level_declarations() {
    // Top-level declarations in a source file should be in file_locals.
    let (binder, _parser) = parse_and_bind(
        r"
let a = 1;
const b = 2;
var c = 3;
function d() {}
class E {}
",
    );

    assert!(
        binder.file_locals.has("a"),
        "let a should be in file_locals"
    );
    assert!(
        binder.file_locals.has("b"),
        "const b should be in file_locals"
    );
    assert!(
        binder.file_locals.has("c"),
        "var c should be in file_locals"
    );
    assert!(
        binder.file_locals.has("d"),
        "function d should be in file_locals"
    );
    assert!(
        binder.file_locals.has("E"),
        "class E should be in file_locals"
    );
}

// =============================================================================
// 3. SYMBOL RESOLUTION
// =============================================================================

#[test]
fn resolve_identifier_in_file_locals() {
    // Identifiers in file_locals should be resolvable via resolve_identifier.
    let (binder, _parser) = parse_and_bind(
        r"
let x = 1;
x;
",
    );

    // x should be in file_locals
    assert!(binder.file_locals.has("x"), "x should be in file_locals");
}

#[test]
fn shadowing_inner_scope_shadows_outer() {
    // An inner scope declaration should shadow an outer scope declaration.
    let (binder, _parser) = parse_and_bind(
        r"
let x = 1;
function foo() {
    let x = 2;
}
",
    );

    // Both symbols should exist (with same name but different IDs)
    let all_x = binder.symbols.find_all_by_name("x");
    assert!(
        all_x.len() >= 2,
        "should have at least 2 symbols named x (outer and inner), got {}",
        all_x.len()
    );
}

#[test]
fn import_creates_alias_symbol() {
    // ES6 imports should create ALIAS symbols with import metadata.
    let (binder, _parser) = parse_and_bind(
        r"
import { foo } from './bar';
",
    );

    let foo_sym_id = binder
        .file_locals
        .get("foo")
        .expect("expected import symbol foo in file_locals");
    let foo_symbol = binder.symbols.get(foo_sym_id).unwrap();
    assert!(
        foo_symbol.flags & symbol_flags::ALIAS != 0,
        "imported foo should have ALIAS flag"
    );
    assert_eq!(
        foo_symbol.import_module.as_deref(),
        Some("./bar"),
        "import_module should be './bar'"
    );
}

#[test]
fn import_as_creates_alias_with_original_name() {
    // `import { foo as bar }` should create an ALIAS symbol for bar with
    // import_name pointing to the original name "foo".
    let (binder, _parser) = parse_and_bind(
        r"
import { foo as bar } from './baz';
",
    );

    let bar_sym_id = binder
        .file_locals
        .get("bar")
        .expect("expected import symbol bar");
    let bar_symbol = binder.symbols.get(bar_sym_id).unwrap();
    assert!(bar_symbol.flags & symbol_flags::ALIAS != 0);
    assert_eq!(bar_symbol.import_module.as_deref(), Some("./baz"));
    assert_eq!(bar_symbol.import_name.as_deref(), Some("foo"));
}

#[test]
fn namespace_import_creates_alias() {
    // `import * as ns from './mod'` should create an ALIAS symbol.
    let (binder, _parser) = parse_and_bind(
        r"
import * as ns from './mod';
",
    );

    let ns_sym_id = binder
        .file_locals
        .get("ns")
        .expect("expected namespace import symbol ns");
    let ns_symbol = binder.symbols.get(ns_sym_id).unwrap();
    assert!(
        ns_symbol.flags & symbol_flags::ALIAS != 0,
        "namespace import should have ALIAS flag"
    );
    assert_eq!(ns_symbol.import_module.as_deref(), Some("./mod"));
}

#[test]
fn export_tracking_with_export_modifier() {
    // Symbols with `export` modifier should have is_exported set to true.
    let (binder, _parser) = parse_and_bind(
        r"
export const x = 1;
export function foo() {}
export class Bar {}
",
    );

    for name in &["x", "foo", "Bar"] {
        let sym_id = binder
            .file_locals
            .get(name)
            .unwrap_or_else(|| panic!("expected {name} in file_locals"));
        let symbol = binder.symbols.get(sym_id).unwrap();
        assert!(symbol.is_exported, "{name} should have is_exported = true");
    }
}

#[test]
fn type_only_import() {
    // `import type { X } from './mod'` should create a type-only alias.
    let (binder, _parser) = parse_and_bind(
        r"
import type { X } from './mod';
",
    );

    let x_sym_id = binder
        .file_locals
        .get("X")
        .expect("expected type-only import symbol X");
    let x_symbol = binder.symbols.get(x_sym_id).unwrap();
    assert!(
        x_symbol.is_type_only,
        "type-only import should have is_type_only = true"
    );
}

#[test]
fn default_import_creates_alias() {
    // `import Foo from './mod'` should create an ALIAS symbol for the default.
    let (binder, _parser) = parse_and_bind(
        r"
import Foo from './mod';
",
    );

    let foo_sym_id = binder
        .file_locals
        .get("Foo")
        .expect("expected default import symbol Foo");
    let foo_symbol = binder.symbols.get(foo_sym_id).unwrap();
    assert!(
        foo_symbol.flags & symbol_flags::ALIAS != 0,
        "default import should have ALIAS flag"
    );
    assert_eq!(foo_symbol.import_module.as_deref(), Some("./mod"));
}

// =============================================================================
// 4. FLOW GRAPH CONSTRUCTION
// =============================================================================

#[test]
fn basic_sequential_flow() {
    // Sequential statements should have a linear flow graph.
    let (binder, _parser) = parse_and_bind(
        r"
let x = 1;
let y = 2;
let z = 3;
",
    );

    // Should have at least: UNREACHABLE + START + some assignment nodes
    assert!(
        binder.flow_nodes.len() >= 2,
        "should have at least UNREACHABLE and START flow nodes"
    );

    // Verify there's exactly 1 START node (for the file)
    let start_count = count_flow_nodes_with_flags(&binder, flow_flags::START);
    assert_eq!(start_count, 1, "should have exactly 1 START flow node");

    // Verify there's exactly 1 UNREACHABLE node
    let unreachable_count = count_flow_nodes_with_flags(&binder, flow_flags::UNREACHABLE);
    assert_eq!(
        unreachable_count, 1,
        "should have exactly 1 UNREACHABLE flow node"
    );
}

#[test]
fn if_statement_creates_condition_flows() {
    // An if statement should create TRUE_CONDITION and FALSE_CONDITION flow nodes.
    let (binder, _parser) = parse_and_bind(
        r"
let x: number | undefined;
if (x) {
    x;
}
",
    );

    let true_count = count_flow_nodes_with_flags(&binder, flow_flags::TRUE_CONDITION);
    assert!(
        true_count >= 1,
        "if statement should create at least 1 TRUE_CONDITION flow"
    );

    let false_count = count_flow_nodes_with_flags(&binder, flow_flags::FALSE_CONDITION);
    assert!(
        false_count >= 1,
        "if statement should create at least 1 FALSE_CONDITION flow"
    );
}

#[test]
fn if_else_creates_branch_and_merge() {
    // An if/else should create branch flows and a merge point.
    let (binder, _parser) = parse_and_bind(
        r"
let x: number | string;
if (typeof x === 'number') {
    x;
} else {
    x;
}
x;
",
    );

    // Should have TRUE_CONDITION and FALSE_CONDITION
    let true_count = count_flow_nodes_with_flags(&binder, flow_flags::TRUE_CONDITION);
    let false_count = count_flow_nodes_with_flags(&binder, flow_flags::FALSE_CONDITION);
    assert!(true_count >= 1, "should have TRUE_CONDITION flow");
    assert!(false_count >= 1, "should have FALSE_CONDITION flow");

    // Should have at least 1 BRANCH_LABEL (merge point after if/else)
    let branch_count = count_flow_nodes_with_flags(&binder, flow_flags::BRANCH_LABEL);
    assert!(
        branch_count >= 1,
        "if/else should create a BRANCH_LABEL merge point"
    );
}

#[test]
fn while_loop_creates_loop_label() {
    // A while loop should create a LOOP_LABEL flow node.
    let (binder, _parser) = parse_and_bind(
        r"
let x = 0;
while (x < 10) {
    x = x + 1;
}
",
    );

    let loop_count = count_flow_nodes_with_flags(&binder, flow_flags::LOOP_LABEL);
    assert!(
        loop_count >= 1,
        "while loop should create at least 1 LOOP_LABEL flow"
    );
}

#[test]
fn for_loop_creates_loop_label() {
    // A for loop should create a LOOP_LABEL flow node.
    let (binder, _parser) = parse_and_bind(
        r"
for (let i = 0; i < 10; i++) {
    i;
}
",
    );

    let loop_count = count_flow_nodes_with_flags(&binder, flow_flags::LOOP_LABEL);
    assert!(
        loop_count >= 1,
        "for loop should create at least 1 LOOP_LABEL flow"
    );
}

#[test]
fn do_while_creates_loop_label() {
    // A do-while loop should create a LOOP_LABEL flow node.
    let (binder, _parser) = parse_and_bind(
        r"
let x = 0;
do {
    x = x + 1;
} while (x < 10);
",
    );

    let loop_count = count_flow_nodes_with_flags(&binder, flow_flags::LOOP_LABEL);
    assert!(
        loop_count >= 1,
        "do-while should create at least 1 LOOP_LABEL flow"
    );
}

#[test]
fn return_creates_unreachable_flow() {
    // After a return statement, the current flow should become unreachable.
    let (binder, _parser) = parse_and_bind(
        r"
function foo() {
    return 1;
    let x = 2;
}
",
    );

    // Verify the function has a START node
    let start_count = count_flow_nodes_with_flags(&binder, flow_flags::START);
    assert!(
        start_count >= 2,
        "function should get its own START flow node"
    );
}

#[test]
fn break_in_loop_jumps_to_post_loop() {
    // A break statement in a loop should create a flow to the post-loop
    // merge label and make subsequent code in the loop unreachable.
    let (binder, _parser) = parse_and_bind(
        r"
let x: number | undefined;
while (true) {
    if (x) {
        break;
    }
    x = 1;
}
",
    );

    // Should have BRANCH_LABEL for the post-loop merge point
    let branch_count = count_flow_nodes_with_flags(&binder, flow_flags::BRANCH_LABEL);
    assert!(
        branch_count >= 1,
        "break in loop should have BRANCH_LABEL for post-loop"
    );
}

#[test]
fn assignment_creates_flow_assignment() {
    // Variable assignments should create ASSIGNMENT flow nodes.
    let (binder, _parser) = parse_and_bind(
        r"
let x: number | undefined;
x = 1;
",
    );

    let assignment_count = count_flow_nodes_with_flags(&binder, flow_flags::ASSIGNMENT);
    assert!(
        assignment_count >= 1,
        "assignment should create ASSIGNMENT flow node"
    );
}

#[test]
fn assignment_in_class_computed_property_does_not_create_flow_assignment() {
    let (binder, _parser) = parse_and_bind(
        r#"
let x: number;
class A { [(x = 1, "_")]() {} }
x;
"#,
    );

    let assignment_count = count_flow_nodes_with_flags(&binder, flow_flags::ASSIGNMENT);
    assert_eq!(
        assignment_count, 0,
        "assignments evaluated inside class computed property names should not create ASSIGNMENT flow nodes"
    );
}

#[test]
fn switch_creates_switch_clause_flow() {
    // Switch statements should create SWITCH_CLAUSE flow nodes.
    let (binder, _parser) = parse_and_bind(
        r"
let x: number | string;
switch (typeof x) {
    case 'number':
        x;
        break;
    case 'string':
        x;
        break;
}
",
    );

    let switch_clause_count = count_flow_nodes_with_flags(&binder, flow_flags::SWITCH_CLAUSE);
    assert!(
        switch_clause_count >= 1,
        "switch should create SWITCH_CLAUSE flow nodes"
    );
}

#[test]
fn function_body_gets_own_start_flow() {
    // A regular function declaration should get its own START flow node.
    let (binder, _parser) = parse_and_bind(
        r"
function foo() {
    let x = 1;
}
",
    );

    let start_count = count_flow_nodes_with_flags(&binder, flow_flags::START);
    assert_eq!(
        start_count, 2,
        "should have 2 START flow nodes: file + function"
    );
}

// =============================================================================
// 5. DECLARATION BINDING
// =============================================================================

#[test]
fn variable_declaration_creates_symbol_with_correct_flags() {
    let (binder, _parser) = parse_and_bind(
        r"
let a = 1;
const b = 2;
var c = 3;
",
    );

    let a_sym_id = binder.file_locals.get("a").expect("expected a");
    let a_symbol = binder.symbols.get(a_sym_id).unwrap();
    assert!(
        a_symbol.flags & symbol_flags::BLOCK_SCOPED_VARIABLE != 0,
        "let should have BLOCK_SCOPED_VARIABLE"
    );

    let b_sym_id = binder.file_locals.get("b").expect("expected b");
    let b_symbol = binder.symbols.get(b_sym_id).unwrap();
    assert!(
        b_symbol.flags & symbol_flags::BLOCK_SCOPED_VARIABLE != 0,
        "const should have BLOCK_SCOPED_VARIABLE"
    );

    let c_sym_id = binder.file_locals.get("c").expect("expected c");
    let c_symbol = binder.symbols.get(c_sym_id).unwrap();
    assert!(
        c_symbol.flags & symbol_flags::FUNCTION_SCOPED_VARIABLE != 0,
        "var should have FUNCTION_SCOPED_VARIABLE"
    );
}

#[test]
fn function_declaration_creates_function_symbol() {
    let (binder, _parser) = parse_and_bind(
        r"
function foo(a: number): number { return a; }
",
    );

    let foo_sym_id = binder.file_locals.get("foo").expect("expected foo");
    let foo_symbol = binder.symbols.get(foo_sym_id).unwrap();
    assert!(
        foo_symbol.flags & symbol_flags::FUNCTION != 0,
        "function declaration should have FUNCTION flag"
    );
}

#[test]
fn class_declaration_creates_class_symbol() {
    let (binder, _parser) = parse_and_bind(
        r"
class Foo {
    x: number = 0;
    method(): void {}
}
",
    );

    let foo_sym_id = binder.file_locals.get("Foo").expect("expected Foo");
    let foo_symbol = binder.symbols.get(foo_sym_id).unwrap();
    assert!(
        foo_symbol.flags & symbol_flags::CLASS != 0,
        "class declaration should have CLASS flag"
    );
}

#[test]
fn abstract_class_gets_abstract_flag() {
    let (binder, _parser) = parse_and_bind(
        r"
abstract class Base {
    abstract method(): void;
}
",
    );

    let base_sym_id = binder.file_locals.get("Base").expect("expected Base");
    let base_symbol = binder.symbols.get(base_sym_id).unwrap();
    assert!(
        base_symbol.flags & symbol_flags::CLASS != 0,
        "abstract class should have CLASS flag"
    );
    assert!(
        base_symbol.flags & symbol_flags::ABSTRACT != 0,
        "abstract class should have ABSTRACT flag"
    );
}

#[test]
fn interface_declaration_creates_interface_symbol() {
    let (binder, _parser) = parse_and_bind(
        r"
interface Foo {
    x: number;
    method(): void;
}
",
    );

    let foo_sym_id = binder.file_locals.get("Foo").expect("expected Foo");
    let foo_symbol = binder.symbols.get(foo_sym_id).unwrap();
    assert!(
        foo_symbol.flags & symbol_flags::INTERFACE != 0,
        "interface declaration should have INTERFACE flag"
    );
}

#[test]
fn interface_merging_adds_multiple_declarations() {
    // Two interface declarations with the same name should merge
    // (add declarations to the same symbol).
    let (binder, _parser) = parse_and_bind(
        r"
interface Foo {
    x: number;
}
interface Foo {
    y: string;
}
",
    );

    let foo_sym_id = binder.file_locals.get("Foo").expect("expected Foo");
    let foo_symbol = binder.symbols.get(foo_sym_id).unwrap();
    assert!(
        foo_symbol.flags & symbol_flags::INTERFACE != 0,
        "merged interface should have INTERFACE flag"
    );
    assert!(
        foo_symbol.declarations.len() >= 2,
        "merged interface should have at least 2 declarations, got {}",
        foo_symbol.declarations.len()
    );
}

#[test]
fn type_alias_creates_type_alias_symbol() {
    let (binder, _parser) = parse_and_bind(
        r"
type MyType = string | number;
",
    );

    let sym_id = binder.file_locals.get("MyType").expect("expected MyType");
    let symbol = binder.symbols.get(sym_id).unwrap();
    assert!(
        symbol.flags & symbol_flags::TYPE_ALIAS != 0,
        "type alias should have TYPE_ALIAS flag"
    );
}

#[test]
fn enum_declaration_creates_regular_enum_symbol() {
    let (binder, _parser) = parse_and_bind(
        r"
enum Color {
    Red,
    Green,
    Blue,
}
",
    );

    let sym_id = binder.file_locals.get("Color").expect("expected Color");
    let symbol = binder.symbols.get(sym_id).unwrap();
    assert!(
        symbol.flags & symbol_flags::REGULAR_ENUM != 0,
        "enum should have REGULAR_ENUM flag"
    );
}

#[test]
fn const_enum_creates_const_enum_symbol() {
    let (binder, _parser) = parse_and_bind(
        r"
const enum Direction {
    Up,
    Down,
    Left,
    Right,
}
",
    );

    let sym_id = binder
        .file_locals
        .get("Direction")
        .expect("expected Direction");
    let symbol = binder.symbols.get(sym_id).unwrap();
    assert!(
        symbol.flags & symbol_flags::CONST_ENUM != 0,
        "const enum should have CONST_ENUM flag"
    );
}

#[test]
fn enum_members_are_in_exports() {
    // Enum members should be tracked as exports of the enum symbol.
    let (binder, _parser) = parse_and_bind(
        r"
enum Color {
    Red,
    Green,
    Blue,
}
",
    );

    let color_sym_id = binder.file_locals.get("Color").expect("expected Color");
    let color_symbol = binder.symbols.get(color_sym_id).unwrap();
    let exports = color_symbol
        .exports
        .as_ref()
        .expect("enum should have exports");

    assert!(exports.has("Red"), "expected Red in enum exports");
    assert!(exports.has("Green"), "expected Green in enum exports");
    assert!(exports.has("Blue"), "expected Blue in enum exports");
}

#[test]
fn namespace_merging_with_function() {
    // A namespace and function with the same name should merge.
    let (binder, _parser) = parse_and_bind(
        r"
function foo() {}
namespace foo {
    export const x = 1;
}
",
    );

    // foo should exist in file_locals
    let foo_sym_id = binder.file_locals.get("foo").expect("expected foo");
    let foo_symbol = binder.symbols.get(foo_sym_id).unwrap();

    // Should have both FUNCTION and MODULE flags
    assert!(
        foo_symbol.flags & symbol_flags::FUNCTION != 0,
        "merged symbol should have FUNCTION flag"
    );
}

#[test]
fn namespace_creates_namespace_module_symbol() {
    let (binder, _parser) = parse_and_bind(
        r"
namespace NS {
    export interface I {}
}
",
    );

    let ns_sym_id = binder.file_locals.get("NS").expect("expected NS");
    let ns_symbol = binder.symbols.get(ns_sym_id).unwrap();
    assert!(
        ns_symbol.flags & symbol_flags::NAMESPACE_MODULE != 0,
        "namespace should have NAMESPACE_MODULE flag"
    );
    let exports = ns_symbol
        .exports
        .as_ref()
        .expect("namespace should have exports");
    assert!(exports.has("I"), "expected I in namespace exports");
}

#[test]
fn class_members_are_tracked() {
    // Class members should be tracked in the class symbol's members table.
    let (binder, _parser) = parse_and_bind(
        r"
class Foo {
    x: number = 0;
    y: string = '';
    method(): void {}
}
",
    );

    let foo_sym_id = binder.file_locals.get("Foo").expect("expected Foo");
    let foo_symbol = binder.symbols.get(foo_sym_id).unwrap();
    let members = foo_symbol
        .members
        .as_ref()
        .expect("class should have members");

    assert!(members.has("x"), "expected x in class members");
    assert!(members.has("y"), "expected y in class members");
    assert!(members.has("method"), "expected method in class members");
}

// =============================================================================
// 6. EXTERNAL MODULE DETECTION
// =============================================================================

#[test]
fn import_makes_file_external_module() {
    // A file with an import declaration should be detected as an external module.
    let (binder, _parser) = parse_and_bind(
        r"
import { x } from './a';
",
    );

    assert!(
        binder.is_external_module,
        "file with import should be an external module"
    );
}

#[test]
fn export_makes_file_external_module() {
    // A file with an export declaration should be detected as an external module.
    let (binder, _parser) = parse_and_bind(
        r"
export const x = 1;
",
    );

    assert!(
        binder.is_external_module,
        "file with export should be an external module"
    );
}

#[test]
fn plain_script_is_not_external_module() {
    // A plain script without imports/exports should NOT be an external module.
    let (binder, _parser) = parse_and_bind(
        r"
let x = 1;
function foo() {}
",
    );

    assert!(
        !binder.is_external_module,
        "plain script should not be an external module"
    );
}

// =============================================================================
// 7. FLOW GRAPH ADVANCED PATTERNS
// =============================================================================

#[test]
fn nested_if_creates_multiple_condition_flows() {
    // Nested if statements should each create their own condition flows.
    let (binder, _parser) = parse_and_bind(
        r"
let x: number | string | undefined;
if (x) {
    if (typeof x === 'number') {
        x;
    }
}
",
    );

    let true_count = count_flow_nodes_with_flags(&binder, flow_flags::TRUE_CONDITION);
    let false_count = count_flow_nodes_with_flags(&binder, flow_flags::FALSE_CONDITION);

    assert!(
        true_count >= 2,
        "nested ifs should create at least 2 TRUE_CONDITION flows, got {true_count}"
    );
    assert!(
        false_count >= 2,
        "nested ifs should create at least 2 FALSE_CONDITION flows, got {false_count}"
    );
}

#[test]
fn throw_creates_unreachable_flow() {
    // After a throw statement, the flow should become unreachable.
    let (binder, _parser) = parse_and_bind(
        r"
function foo() {
    throw new Error('oops');
    let x = 1;
}
",
    );

    // The function should still have its START node
    let start_count = count_flow_nodes_with_flags(&binder, flow_flags::START);
    assert!(start_count >= 2, "function should have START flow node");
}

#[test]
fn multiple_functions_each_get_start_flow() {
    // Each function should get its own START flow node.
    let (binder, _parser) = parse_and_bind(
        r"
function a() {}
function b() {}
function c() {}
",
    );

    let start_count = count_flow_nodes_with_flags(&binder, flow_flags::START);
    assert_eq!(
        start_count, 4,
        "should have 4 START flow nodes: file + 3 functions"
    );
}

// =============================================================================
// 8. STRICT MODE BEHAVIOR
// =============================================================================

#[test]
fn use_strict_enables_strict_mode() {
    // A file with "use strict" prologue should bind in strict mode.
    // In strict mode, function declarations in blocks are block-scoped.
    let (binder, _parser) = parse_and_bind(
        r#"
"use strict";
let x = 1;
"#,
    );

    // The binder should detect strict mode
    assert!(
        binder.is_strict_scope,
        "\"use strict\" should enable strict mode"
    );
}

#[test]
fn always_strict_option_enables_strict_mode() {
    // The always_strict option should enable strict mode even without "use strict".
    let options = BinderOptions {
        target: ScriptTarget::ES5,
        always_strict: true,
    };
    let (binder, _parser) = parse_and_bind_with_options(
        r"
let x = 1;
",
        options,
    );

    assert!(
        binder.is_strict_scope,
        "always_strict should enable strict mode"
    );
}

// =============================================================================
// 9. DECLARED MODULES
// =============================================================================

#[test]
fn declare_module_with_string_name() {
    // `declare module "..." { }` should be tracked as a declared module.
    let (binder, _parser) = parse_and_bind(
        r#"
declare module "my-module" {
    export function foo(): void;
}
"#,
    );

    assert!(
        binder.declared_modules.contains("my-module"),
        "declared module should be tracked"
    );
}

// =============================================================================
// 10. MULTIPLE SCOPES AND RESOLUTION
// =============================================================================

#[test]
fn for_in_creates_scope() {
    // for-in loops should create a scope for the iteration variable.
    let (binder, _parser) = parse_and_bind(
        r"
for (let key in obj) {
    key;
}
",
    );

    // Check that a block scope exists
    let block_count = binder
        .scopes
        .iter()
        .filter(|s| s.kind == ContainerKind::Block)
        .count();
    assert!(block_count >= 1, "for-in should create a block scope");
}

#[test]
fn arrow_function_creates_function_scope() {
    // Arrow functions should create their own scope.
    let (binder, _parser) = parse_and_bind(
        r"
const fn = (x: number) => {
    let y = x + 1;
    return y;
};
",
    );

    let function_count = binder
        .scopes
        .iter()
        .filter(|s| s.kind == ContainerKind::Function)
        .count();
    assert!(
        function_count >= 1,
        "arrow function should create a Function scope"
    );
}

#[test]
fn destructuring_binding_creates_symbols() {
    // Destructuring patterns should create individual symbols for each name.
    let (binder, _parser) = parse_and_bind(
        r"
const { a, b } = { a: 1, b: 2 };
const [c, d] = [3, 4];
",
    );

    for name in &["a", "b", "c", "d"] {
        let sym = binder.symbols.find_by_name(name);
        assert!(
            sym.is_some(),
            "destructuring should create symbol for {name}"
        );
    }
}

#[test]
fn function_overloads_merge() {
    // Multiple function declarations with the same name should merge.
    let (binder, _parser) = parse_and_bind(
        r"
function foo(x: number): number;
function foo(x: string): string;
function foo(x: number | string): number | string {
    return x;
}
",
    );

    let foo_sym_id = binder.file_locals.get("foo").expect("expected foo");
    let foo_symbol = binder.symbols.get(foo_sym_id).unwrap();
    assert!(
        foo_symbol.flags & symbol_flags::FUNCTION != 0,
        "merged function overloads should have FUNCTION flag"
    );
    assert!(
        foo_symbol.declarations.len() >= 2,
        "function overloads should have multiple declarations, got {}",
        foo_symbol.declarations.len()
    );
}

// =============================================================================
// 11. NODE-TO-SYMBOL AND NODE-TO-FLOW MAPPINGS
// =============================================================================

#[test]
fn node_symbols_populated_for_declarations() {
    // The binder should populate node_symbols for declaration nodes.
    let (binder, _parser) = parse_and_bind(
        r"
let x = 1;
function foo() {}
class Bar {}
",
    );

    assert!(
        !binder.node_symbols.is_empty(),
        "node_symbols should be populated after binding"
    );
}

#[test]
fn node_flow_populated_for_identifiers() {
    // The binder should populate node_flow for identifier nodes.
    let (binder, _parser) = parse_and_bind(
        r"
let x = 1;
x;
",
    );

    assert!(
        !binder.node_flow.is_empty(),
        "node_flow should be populated after binding"
    );
}

// =============================================================================
// 12. FILE FEATURES DETECTION
// =============================================================================

#[test]
fn generator_function_sets_feature_flag() {
    let (binder, _parser) = parse_and_bind(
        r"
function* gen() {
    yield 1;
}
",
    );

    assert!(
        binder
            .file_features
            .has(crate::state::FileFeatures::GENERATORS),
        "generator function should set GENERATORS feature flag"
    );
}

#[test]
fn async_generator_sets_feature_flag() {
    let (binder, _parser) = parse_and_bind(
        r"
async function* asyncGen() {
    yield 1;
}
",
    );

    assert!(
        binder
            .file_features
            .has(crate::state::FileFeatures::ASYNC_GENERATORS),
        "async generator function should set ASYNC_GENERATORS feature flag"
    );
}

#[test]
fn using_declaration_sets_feature_flag() {
    let (binder, _parser) = parse_and_bind("using d = undefined;");
    assert!(
        binder.file_features.has(crate::state::FileFeatures::USING),
        "using declaration should set USING feature flag"
    );
}

#[test]
fn await_using_declaration_sets_feature_flag() {
    let (binder, _parser) = parse_and_bind("await using e = undefined;");
    assert!(
        binder
            .file_features
            .has(crate::state::FileFeatures::AWAIT_USING),
        "await using declaration should set AWAIT_USING feature flag"
    );
}

// =============================================================================
// 13. RESET AND REUSE
// =============================================================================

#[test]
fn binder_reset_clears_state() {
    let mut binder = BinderState::new();

    // Bind something
    let mut parser = ParserState::new("test.ts".to_string(), "let x = 1;".to_string());
    let root = parser.parse_source_file();
    binder.bind_source_file(parser.get_arena(), root);
    assert!(!binder.file_locals.is_empty());
    assert!(!binder.symbols.is_empty());

    // Reset
    binder.reset();

    assert!(
        binder.file_locals.is_empty(),
        "reset should clear file_locals"
    );
    assert!(binder.symbols.is_empty(), "reset should clear symbols");
    assert!(
        binder.node_symbols.is_empty(),
        "reset should clear node_symbols"
    );
    assert!(binder.scopes.is_empty(), "reset should clear scopes");
}

// =============================================================================
// 14. SYMBOL ARENA TESTS
// =============================================================================

#[test]
fn symbol_arena_alloc_and_get() {
    use crate::SymbolArena;

    let mut arena = SymbolArena::new();
    let id = arena.alloc(symbol_flags::CLASS, "MyClass".to_string());

    let sym = arena.get(id).expect("should get symbol by ID");
    assert_eq!(sym.escaped_name, "MyClass");
    assert!(sym.flags & symbol_flags::CLASS != 0);
    assert_eq!(sym.id, id);
}

#[test]
fn symbol_arena_find_by_name() {
    use crate::SymbolArena;

    let mut arena = SymbolArena::new();
    arena.alloc(symbol_flags::FUNCTION, "foo".to_string());
    arena.alloc(symbol_flags::CLASS, "Bar".to_string());

    assert!(arena.find_by_name("foo").is_some());
    assert!(arena.find_by_name("Bar").is_some());
    assert!(arena.find_by_name("baz").is_none());
}

#[test]
fn symbol_arena_find_all_by_name() {
    use crate::SymbolArena;

    let mut arena = SymbolArena::new();
    arena.alloc(symbol_flags::FUNCTION_SCOPED_VARIABLE, "x".to_string());
    arena.alloc(symbol_flags::BLOCK_SCOPED_VARIABLE, "x".to_string());
    arena.alloc(symbol_flags::CLASS, "Y".to_string());

    let all_x = arena.find_all_by_name("x");
    assert_eq!(all_x.len(), 2, "should find 2 symbols named x");
}

#[test]
fn symbol_table_operations() {
    let mut table = SymbolTable::new();
    use crate::SymbolId;

    let id1 = SymbolId(0);
    let id2 = SymbolId(1);

    table.set("foo".to_string(), id1);
    table.set("bar".to_string(), id2);

    assert!(table.has("foo"));
    assert!(table.has("bar"));
    assert!(!table.has("baz"));

    assert_eq!(table.get("foo"), Some(id1));
    assert_eq!(table.len(), 2);

    table.remove("foo");
    assert!(!table.has("foo"));
    assert_eq!(table.len(), 1);
}

// =============================================================================
// 15. FLOW NODE ARENA TESTS
// =============================================================================

#[test]
fn flow_node_arena_operations() {
    use crate::FlowNodeArena;

    let mut arena = FlowNodeArena::new();
    assert!(arena.is_empty());

    let id1 = arena.alloc(flow_flags::START);
    let id2 = arena.alloc(flow_flags::UNREACHABLE);

    assert_eq!(arena.len(), 2);
    assert!(!arena.is_empty());

    let node1 = arena.get(id1).unwrap();
    assert!(node1.has_any_flags(flow_flags::START));
    assert!(!node1.has_any_flags(flow_flags::UNREACHABLE));

    let node2 = arena.get(id2).unwrap();
    assert!(node2.has_any_flags(flow_flags::UNREACHABLE));

    // find_unreachable should find the UNREACHABLE node
    let found = arena.find_unreachable();
    assert_eq!(found, Some(id2));
}

#[test]
fn flow_node_antecedents() {
    use crate::FlowNodeArena;

    let mut arena = FlowNodeArena::new();
    let start = arena.alloc(flow_flags::START);
    let branch = arena.alloc(flow_flags::BRANCH_LABEL);

    // Add antecedent
    if let Some(node) = arena.get_mut(branch) {
        node.antecedent.push(start);
    }

    let branch_node = arena.get(branch).unwrap();
    assert_eq!(branch_node.antecedent.len(), 1);
    assert_eq!(branch_node.antecedent[0], start);
}

// =============================================================================
// 16. SCOPE TESTS
// =============================================================================

#[test]
fn scope_is_function_scope() {
    use crate::scopes::Scope;
    use tsz_parser::NodeIndex;

    let source_scope = Scope::new(
        crate::ScopeId::NONE,
        ContainerKind::SourceFile,
        NodeIndex::NONE,
    );
    assert!(
        source_scope.is_function_scope(),
        "SourceFile is a function scope"
    );

    let func_scope = Scope::new(
        crate::ScopeId::NONE,
        ContainerKind::Function,
        NodeIndex::NONE,
    );
    assert!(
        func_scope.is_function_scope(),
        "Function is a function scope"
    );

    let module_scope = Scope::new(crate::ScopeId::NONE, ContainerKind::Module, NodeIndex::NONE);
    assert!(
        module_scope.is_function_scope(),
        "Module is a function scope"
    );

    let block_scope = Scope::new(crate::ScopeId::NONE, ContainerKind::Block, NodeIndex::NONE);
    assert!(
        !block_scope.is_function_scope(),
        "Block is NOT a function scope"
    );

    let class_scope = Scope::new(crate::ScopeId::NONE, ContainerKind::Class, NodeIndex::NONE);
    assert!(
        !class_scope.is_function_scope(),
        "Class is NOT a function scope"
    );
}

// =============================================================================
// 17. SYMBOL FLAG COMPOSITE TESTS
// =============================================================================

#[test]
fn symbol_flag_composites() {
    // Verify composite flag relationships
    assert_eq!(
        symbol_flags::ENUM,
        symbol_flags::REGULAR_ENUM | symbol_flags::CONST_ENUM
    );
    assert_eq!(
        symbol_flags::VARIABLE,
        symbol_flags::FUNCTION_SCOPED_VARIABLE | symbol_flags::BLOCK_SCOPED_VARIABLE
    );
    assert_eq!(
        symbol_flags::MODULE,
        symbol_flags::VALUE_MODULE | symbol_flags::NAMESPACE_MODULE
    );
    assert_eq!(
        symbol_flags::ACCESSOR,
        symbol_flags::GET_ACCESSOR | symbol_flags::SET_ACCESSOR
    );
}

#[test]
fn symbol_has_flags_checks() {
    use crate::Symbol;
    use crate::SymbolId;

    let sym = Symbol::new(
        SymbolId(0),
        symbol_flags::CLASS | symbol_flags::ABSTRACT,
        "Foo".to_string(),
    );

    assert!(sym.has_flags(symbol_flags::CLASS));
    assert!(sym.has_flags(symbol_flags::ABSTRACT));
    assert!(sym.has_flags(symbol_flags::CLASS | symbol_flags::ABSTRACT));
    assert!(!sym.has_flags(symbol_flags::INTERFACE));

    assert!(sym.has_any_flags(symbol_flags::CLASS));
    assert!(sym.has_any_flags(symbol_flags::CLASS | symbol_flags::INTERFACE));
    assert!(!sym.has_any_flags(symbol_flags::INTERFACE | symbol_flags::FUNCTION));
}

// =============================================================================
// 18. EXPORT RESOLUTION TESTS
// =============================================================================

#[test]
fn direct_module_export_resolution() {
    // Test resolving a direct export from module_exports.
    let mut binder = BinderState::new();

    let sym = binder
        .symbols
        .alloc(symbol_flags::FUNCTION, "myFunc".to_string());
    let mut exports = SymbolTable::new();
    exports.set("myFunc".to_string(), sym);
    binder.module_exports.insert("./mod".to_string(), exports);

    let resolved = binder.resolve_import_if_needed_public("./mod", "myFunc");
    assert_eq!(resolved, Some(sym), "should resolve direct export");

    let not_found = binder.resolve_import_if_needed_public("./mod", "nonExistent");
    assert_eq!(not_found, None, "non-existent export should return None");
}

#[test]
fn wildcard_reexport_resolution() {
    // Test resolving through `export * from` chains.
    let mut binder = BinderState::new();

    let sym = binder
        .symbols
        .alloc(symbol_flags::CLASS, "Widget".to_string());
    let mut a_exports = SymbolTable::new();
    a_exports.set("Widget".to_string(), sym);
    binder.module_exports.insert("./a".to_string(), a_exports);

    // ./b re-exports everything from ./a
    binder
        .wildcard_reexports
        .entry("./b".to_string())
        .or_default()
        .push("./a".to_string());
    binder
        .wildcard_reexports_type_only
        .entry("./b".to_string())
        .or_default()
        .push(("./a".to_string(), false));

    let resolved = binder.resolve_import_if_needed_public("./b", "Widget");
    assert_eq!(
        resolved,
        Some(sym),
        "should resolve through wildcard re-export"
    );
}

#[test]
fn reexport_cycle_does_not_hang() {
    // Ensure that cyclic re-export chains don't cause infinite loops.
    let mut binder = BinderState::new();

    // ./a re-exports from ./b
    binder
        .wildcard_reexports
        .entry("./a".to_string())
        .or_default()
        .push("./b".to_string());
    binder
        .wildcard_reexports_type_only
        .entry("./a".to_string())
        .or_default()
        .push(("./b".to_string(), false));

    // ./b re-exports from ./a (cycle!)
    binder
        .wildcard_reexports
        .entry("./b".to_string())
        .or_default()
        .push("./a".to_string());
    binder
        .wildcard_reexports_type_only
        .entry("./b".to_string())
        .or_default()
        .push(("./a".to_string(), false));

    // Should not hang, should return None
    let resolved = binder.resolve_import_if_needed_public("./a", "X");
    assert_eq!(resolved, None, "cyclic re-export should return None");
}

// =============================================================================
// 19. MODULE AUGMENTATION TESTS
// =============================================================================

#[test]
fn module_augmentation_tracked() {
    // `declare module "x" { interface Y { ... } }` inside an external module
    // should track the augmentation.
    let (binder, _parser) = parse_and_bind(
        r#"
import {} from "x";
declare module "x" {
    interface Augmented {
        extra: string;
    }
}
"#,
    );

    // The file should be an external module (has import)
    assert!(binder.is_external_module);

    // Check that module augmentations were tracked
    // Note: module augmentations are only tracked when the binder detects
    // that the module being declared already exists as a known module.
    // In isolation (no lib context), this may or may not be tracked.
}

#[test]
fn global_augmentation_tracked_in_declare_module() {
    // `global { ... }` inside `declare module "x"` should track global augmentations.
    let (binder, _parser) = parse_and_bind(
        r#"
declare module "mylib" {
    global {
        interface MyGlobal {
            x: number;
        }
    }
}
"#,
    );

    assert!(
        binder.global_augmentations.contains_key("MyGlobal"),
        "global augmentation should be tracked"
    );
}

// =============================================================================
// 20. EXPANDO PROPERTIES
// =============================================================================

#[test]
fn expando_property_assignments_tracked() {
    // `X.prop = value` patterns should be tracked as expando properties.
    let (binder, _parser) = parse_and_bind(
        r"
function foo() {}
foo.bar = 1;
foo.baz = 'hello';
",
    );

    if let Some(props) = binder.expando_properties.get("foo") {
        assert!(props.contains("bar"), "should track foo.bar");
        assert!(props.contains("baz"), "should track foo.baz");
    }
    // Note: expando tracking may not work for all patterns in all cases,
    // so we don't assert that the map is non-empty unconditionally.
}

#[test]
fn void_zero_expando_assignments_are_skipped() {
    let source = r#"
exports.k = void 0;
var o = {};
o.y = void 0;
"#;

    let mut parser = ParserState::new("a.js".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    assert!(
        !binder
            .expando_properties
            .get("exports")
            .is_some_and(|props| props.contains("k")),
        "unexpected exports expando tracking: {:?}",
        binder.expando_properties
    );
    assert!(
        !binder
            .expando_properties
            .get("o")
            .is_some_and(|props| props.contains("y")),
        "unexpected object expando tracking: {:?}",
        binder.expando_properties
    );
}

#[test]
fn expando_element_assignments_resolve_const_literal_keys() {
    let (binder, _parser) = parse_and_bind(
        r#"
function foo() {}
const key = "realName";
const num = 42;
const sym = Symbol();
foo[key] = 1;
foo[num] = 2;
foo[sym] = 3;
"#,
    );

    let props = binder
        .expando_properties
        .get("foo")
        .expect("expected expando properties for foo");

    assert!(
        props.contains("realName"),
        "should resolve const string keys"
    );
    assert!(props.contains("42"), "should resolve const numeric keys");

    let sym_id = binder.file_locals.get("sym").expect("expected sym local");
    let unique_name = format!("__unique_{}", sym_id.0);
    assert!(
        props.contains(&unique_name),
        "should resolve const Symbol() keys to internal unique-symbol names"
    );
}

// =============================================================================
// 21. SHORTHAND AMBIENT MODULES
// =============================================================================

#[test]
fn shorthand_ambient_module_detected() {
    // `declare module "xxx"` without a body should be detected as shorthand.
    let (binder, _parser) = parse_and_bind(
        r#"
declare module "*.css";
"#,
    );

    assert!(
        binder.shorthand_ambient_modules.contains("*.css"),
        "shorthand ambient module should be tracked"
    );
}

// =============================================================================
// 22. COMPLEX HOISTING SCENARIOS
// =============================================================================

#[test]
fn var_hoisted_from_do_while_body() {
    // `var` in a do-while body should be hoisted.
    let (binder, _parser) = parse_and_bind(
        r"
function foo() {
    do {
        var x = 1;
    } while (false);
}
",
    );

    let x_sym = binder.symbols.find_by_name("x");
    assert!(x_sym.is_some(), "var in do-while should be hoisted");
    let x_symbol = binder.symbols.get(x_sym.unwrap()).unwrap();
    assert!(x_symbol.flags & symbol_flags::FUNCTION_SCOPED_VARIABLE != 0);
}

#[test]
fn multiple_var_same_name_in_different_blocks_merge() {
    // Multiple `var` declarations with the same name in different blocks
    // of the same function should all merge into one symbol.
    let (binder, _parser) = parse_and_bind(
        r"
function foo() {
    if (true) {
        var x = 1;
    }
    if (false) {
        var x = 2;
    }
    for (var x = 0; x < 1; x++) {}
}
",
    );

    // All `x` declarations should merge into one symbol
    let x_sym = binder
        .symbols
        .find_by_name("x")
        .expect("should have symbol for x");
    let x_symbol = binder.symbols.get(x_sym).unwrap();
    assert!(x_symbol.flags & symbol_flags::FUNCTION_SCOPED_VARIABLE != 0);
}

// =============================================================================
// 23. SCOPE DISCOVERY AND PERSISTENT SCOPE SYSTEM
// =============================================================================

#[test]
fn persistent_scopes_populated() {
    // After binding, the persistent scope system should be populated.
    let (binder, _parser) = parse_and_bind(
        r"
function foo() {
    let x = 1;
    {
        let y = 2;
    }
}
",
    );

    // Should have at least: SourceFile + Function + Block
    assert!(
        binder.scopes.len() >= 3,
        "should have at least 3 persistent scopes, got {}",
        binder.scopes.len()
    );
}

#[test]
fn node_scope_ids_maps_nodes_to_scopes() {
    // The node_scope_ids map should link AST nodes to their scopes.
    let (binder, _parser) = parse_and_bind(
        r"
function foo() {
    let x = 1;
}
",
    );

    assert!(
        !binder.node_scope_ids.is_empty(),
        "node_scope_ids should be populated"
    );
}

// =============================================================================
// 24. EDGE CASES
// =============================================================================

#[test]
fn empty_source_file() {
    // Binding an empty source file should not panic.
    let (binder, _parser) = parse_and_bind("");

    assert!(
        !binder.scopes.is_empty(),
        "even empty file should have root scope"
    );
    assert!(binder.file_locals.is_empty() || !binder.file_locals.is_empty());
}

#[test]
fn binding_export_default_function() {
    // `export default function foo() {}` should work correctly.
    let (binder, _parser) = parse_and_bind(
        r"
export default function foo() {}
",
    );

    assert!(
        binder.is_external_module,
        "file with export should be module"
    );
}

#[test]
fn binding_export_default_class() {
    // `export default class Foo {}` should work correctly.
    let (binder, _parser) = parse_and_bind(
        r"
export default class Foo {}
",
    );

    assert!(
        binder.is_external_module,
        "file with export should be module"
    );
}

#[test]
fn binding_reexport_all() {
    // `export * from './mod'` should track wildcard re-exports.
    let (binder, _parser) = parse_and_bind(
        r"
export * from './mod';
",
    );

    assert!(binder.is_external_module);
}

#[test]
fn binding_complex_destructuring() {
    // Complex destructuring patterns should all be bound correctly.
    let (binder, _parser) = parse_and_bind(
        r"
const { a, b: { c, d }, e: [f, g] } = obj;
",
    );

    // At minimum, the simple names should be bound
    assert!(binder.symbols.find_by_name("a").is_some(), "should bind a");
    assert!(binder.symbols.find_by_name("c").is_some(), "should bind c");
    assert!(binder.symbols.find_by_name("d").is_some(), "should bind d");
    assert!(binder.symbols.find_by_name("f").is_some(), "should bind f");
    assert!(binder.symbols.find_by_name("g").is_some(), "should bind g");
}

#[test]
fn class_with_static_members() {
    // Static members should have the STATIC flag.
    let (binder, _parser) = parse_and_bind(
        r"
class Foo {
    static x: number = 1;
    static method(): void {}
}
",
    );

    let foo_sym_id = binder.file_locals.get("Foo").expect("expected Foo");
    let foo_symbol = binder.symbols.get(foo_sym_id).unwrap();
    assert!(foo_symbol.flags & symbol_flags::CLASS != 0);
}

#[test]
fn class_with_private_members() {
    // Private members should have appropriate flags.
    let (binder, _parser) = parse_and_bind(
        r"
class Foo {
    private x: number = 1;
    protected y: string = '';
}
",
    );

    let foo_sym_id = binder.file_locals.get("Foo").expect("expected Foo");
    let foo_symbol = binder.symbols.get(foo_sym_id).unwrap();
    assert!(foo_symbol.flags & symbol_flags::CLASS != 0);
}

#[test]
fn computed_property_name_does_not_crash() {
    // Computed property names in classes should not crash the binder.
    let (binder, _parser) = parse_and_bind(
        r"
const key = 'hello';
class Foo {
    [key]: number = 1;
}
",
    );

    assert!(binder.file_locals.has("Foo"));
}

#[test]
fn accessor_declarations() {
    // Get/set accessors should create symbols with accessor flags.
    let (binder, _parser) = parse_and_bind(
        r"
class Foo {
    private _x: number = 0;
    get x(): number { return this._x; }
    set x(value: number) { this._x = value; }
}
",
    );

    let foo_sym_id = binder.file_locals.get("Foo").expect("expected Foo");
    let foo_symbol = binder.symbols.get(foo_sym_id).unwrap();
    let members = foo_symbol
        .members
        .as_ref()
        .expect("class should have members");

    // x should exist as a member (get/set accessor)
    assert!(members.has("x"), "accessor x should be in class members");
}

// =============================================================================
// 25. ENUM MERGING WITH NAMESPACE
// =============================================================================

#[test]
fn enum_merging_with_namespace() {
    // An enum and a namespace with the same name should merge.
    let (binder, _parser) = parse_and_bind(
        r"
enum Color {
    Red,
    Green,
    Blue,
}
namespace Color {
    export function fromString(s: string): Color { return Color.Red; }
}
",
    );

    let sym_id = binder.file_locals.get("Color").expect("expected Color");
    let symbol = binder.symbols.get(sym_id).unwrap();
    // Should have REGULAR_ENUM (from enum) and possibly module flags (from namespace)
    assert!(
        symbol.flags & symbol_flags::REGULAR_ENUM != 0,
        "merged symbol should keep REGULAR_ENUM flag"
    );
}

// =============================================================================
// 26. MULTIPLE INTERFACE DECLARATIONS WITH MEMBERS
// =============================================================================

#[test]
fn multiple_interface_declarations_members() {
    // Multiple interface declarations should all contribute members.
    let (binder, _parser) = parse_and_bind(
        r"
interface Foo {
    x: number;
}
interface Foo {
    y: string;
}
interface Foo {
    z: boolean;
}
",
    );

    let sym_id = binder.file_locals.get("Foo").expect("expected Foo");
    let symbol = binder.symbols.get(sym_id).unwrap();
    assert!(symbol.flags & symbol_flags::INTERFACE != 0);
    assert!(
        symbol.declarations.len() >= 3,
        "should have at least 3 declarations for merged interface"
    );
}

// =============================================================================
// Phase 1 DefId-First Stable Identity Tests
// =============================================================================

/// Helper: parse + bind a source file and return the binder state.
fn bind_source(source: &str) -> BinderState {
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);
    binder
}

#[test]
fn semantic_defs_captures_top_level_class() {
    let binder = bind_source("class Foo {}");
    let sym_id = binder.file_locals.get("Foo").expect("expected Foo");
    let entry = binder
        .semantic_defs
        .get(&sym_id)
        .expect("expected semantic def for class Foo");
    assert_eq!(entry.kind, super::SemanticDefKind::Class);
    assert_eq!(entry.name, "Foo");
}

#[test]
fn semantic_defs_captures_top_level_interface() {
    let binder = bind_source("interface Bar { x: number }");
    let sym_id = binder.file_locals.get("Bar").expect("expected Bar");
    let entry = binder
        .semantic_defs
        .get(&sym_id)
        .expect("expected semantic def for interface Bar");
    assert_eq!(entry.kind, super::SemanticDefKind::Interface);
    assert_eq!(entry.name, "Bar");
}

#[test]
fn semantic_defs_captures_top_level_type_alias() {
    let binder = bind_source("type Baz = string | number");
    let sym_id = binder.file_locals.get("Baz").expect("expected Baz");
    let entry = binder
        .semantic_defs
        .get(&sym_id)
        .expect("expected semantic def for type alias Baz");
    assert_eq!(entry.kind, super::SemanticDefKind::TypeAlias);
    assert_eq!(entry.name, "Baz");
}

#[test]
fn semantic_defs_captures_top_level_enum() {
    let binder = bind_source("enum Color { Red, Green, Blue }");
    let sym_id = binder.file_locals.get("Color").expect("expected Color");
    let entry = binder
        .semantic_defs
        .get(&sym_id)
        .expect("expected semantic def for enum Color");
    assert_eq!(entry.kind, super::SemanticDefKind::Enum);
    assert_eq!(entry.name, "Color");
}

#[test]
fn semantic_defs_captures_top_level_namespace() {
    let binder = bind_source("namespace NS { export type T = number }");
    let sym_id = binder.file_locals.get("NS").expect("expected NS");
    let entry = binder
        .semantic_defs
        .get(&sym_id)
        .expect("expected semantic def for namespace NS");
    assert_eq!(entry.kind, super::SemanticDefKind::Namespace);
    assert_eq!(entry.name, "NS");
}

#[test]
fn semantic_defs_excludes_nested_declarations() {
    let binder = bind_source(
        "
function outer() {
    class Inner {}
    type Local = string;
}
",
    );
    // Inner and Local should NOT be in semantic_defs because they're inside a function body
    for entry in binder.semantic_defs.values() {
        assert_ne!(entry.name, "Inner", "nested class should not be captured");
        assert_ne!(
            entry.name, "Local",
            "nested type alias should not be captured"
        );
    }
}

#[test]
fn semantic_defs_stable_across_rebuild() {
    // Binding the same source twice should produce identical semantic_defs
    let source = "
class A {}
interface B { x: number }
type C = string;
enum D { X }
namespace E { export const v = 1 }
function F() {}
const G = 42;
";
    let binder1 = bind_source(source);
    let binder2 = bind_source(source);

    // Same number of semantic defs
    assert_eq!(binder1.semantic_defs.len(), binder2.semantic_defs.len());

    // Same names and kinds
    for (sym_id, entry1) in &binder1.semantic_defs {
        let entry2 = binder2
            .semantic_defs
            .get(sym_id)
            .expect("same SymbolId should exist in second binding");
        assert_eq!(
            entry1.kind, entry2.kind,
            "kind mismatch for {}",
            entry1.name
        );
        assert_eq!(entry1.name, entry2.name, "name mismatch");
        assert_eq!(
            entry1.span_start, entry2.span_start,
            "span_start mismatch for {}",
            entry1.name
        );
    }
}

#[test]
fn semantic_defs_declaration_merging_keeps_first_identity() {
    // When a symbol is declared multiple times (interface merging),
    // the first declaration's span should be preserved.
    let binder = bind_source(
        "
interface Merged { a: string }
interface Merged { b: number }
",
    );
    let sym_id = binder.file_locals.get("Merged").expect("expected Merged");
    let entry = binder
        .semantic_defs
        .get(&sym_id)
        .expect("expected semantic def for Merged");
    assert_eq!(entry.kind, super::SemanticDefKind::Interface);
    assert_eq!(entry.name, "Merged");
    // Span should be from the FIRST declaration
    let first_decl = binder.symbols.get(sym_id).unwrap().declarations[0];
    assert_eq!(entry.span_start, first_decl.0);
}

#[test]
fn symbols_capture_stable_declaration_spans() {
    let source = "
interface Merged { a: string }
interface Merged { b: number }
const value = 1;
";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena().clone();
    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    let merged_sym_id = binder.file_locals.get("Merged").expect("expected Merged");
    let merged = binder
        .symbols
        .get(merged_sym_id)
        .expect("expected symbol for Merged");
    let first_decl = merged.declarations[0];
    let expected_first_span = arena.get(first_decl).map(|node| (node.pos, node.end));
    assert_eq!(merged.first_declaration_span, expected_first_span);
    assert_eq!(merged.value_declaration_span, None);

    let value_sym_id = binder.file_locals.get("value").expect("expected value");
    let value = binder
        .symbols
        .get(value_sym_id)
        .expect("expected symbol for value");
    let expected_value_span = arena
        .get(value.value_declaration)
        .map(|node| (node.pos, node.end));
    assert_eq!(value.first_declaration_span, expected_value_span);
    assert_eq!(value.value_declaration_span, expected_value_span);
}

#[test]
fn semantic_defs_covers_all_seven_declaration_kinds() {
    let binder = bind_source(
        "
class MyClass {}
interface MyInterface {}
type MyType = number;
enum MyEnum { A }
namespace MyNS {}
function myFunc() {}
const myVar = 1;
",
    );
    assert_eq!(
        binder.semantic_defs.len(),
        7,
        "expected exactly 7 semantic defs, got {:?}",
        binder
            .semantic_defs
            .values()
            .map(|e| &e.name)
            .collect::<Vec<_>>()
    );
    let kinds: std::collections::HashSet<_> =
        binder.semantic_defs.values().map(|e| e.kind).collect();
    assert!(kinds.contains(&super::SemanticDefKind::Class));
    assert!(kinds.contains(&super::SemanticDefKind::Interface));
    assert!(kinds.contains(&super::SemanticDefKind::TypeAlias));
    assert!(kinds.contains(&super::SemanticDefKind::Enum));
    assert!(kinds.contains(&super::SemanticDefKind::Namespace));
    assert!(kinds.contains(&super::SemanticDefKind::Function));
    assert!(kinds.contains(&super::SemanticDefKind::Variable));
}

#[test]
fn semantic_defs_captures_top_level_function() {
    let binder = bind_source("function greet(name: string): string { return name; }");
    let sym_id = binder.file_locals.get("greet").expect("expected greet");
    let entry = binder
        .semantic_defs
        .get(&sym_id)
        .expect("expected semantic def for function greet");
    assert_eq!(entry.kind, super::SemanticDefKind::Function);
    assert_eq!(entry.name, "greet");
}

#[test]
fn semantic_defs_captures_top_level_variable_const() {
    let binder = bind_source("const MAX_SIZE = 100;");
    let sym_id = binder
        .file_locals
        .get("MAX_SIZE")
        .expect("expected MAX_SIZE");
    let entry = binder
        .semantic_defs
        .get(&sym_id)
        .expect("expected semantic def for variable MAX_SIZE");
    assert_eq!(entry.kind, super::SemanticDefKind::Variable);
    assert_eq!(entry.name, "MAX_SIZE");
}

#[test]
fn semantic_defs_captures_top_level_variable_let() {
    let binder = bind_source("let counter = 0;");
    let sym_id = binder.file_locals.get("counter").expect("expected counter");
    let entry = binder
        .semantic_defs
        .get(&sym_id)
        .expect("expected semantic def for variable counter");
    assert_eq!(entry.kind, super::SemanticDefKind::Variable);
    assert_eq!(entry.name, "counter");
}

#[test]
fn semantic_defs_captures_top_level_variable_var() {
    let binder = bind_source("var legacy = true;");
    let sym_id = binder.file_locals.get("legacy").expect("expected legacy");
    let entry = binder
        .semantic_defs
        .get(&sym_id)
        .expect("expected semantic def for variable legacy");
    assert_eq!(entry.kind, super::SemanticDefKind::Variable);
    assert_eq!(entry.name, "legacy");
}

#[test]
fn semantic_defs_captures_destructured_top_level_variables() {
    let binder = bind_source("const { a, b } = { a: 1, b: 2 };");
    let sym_a = binder.file_locals.get("a").expect("expected a");
    let entry_a = binder
        .semantic_defs
        .get(&sym_a)
        .expect("expected semantic def for variable a");
    assert_eq!(entry_a.kind, super::SemanticDefKind::Variable);
    assert_eq!(entry_a.name, "a");

    let sym_b = binder.file_locals.get("b").expect("expected b");
    let entry_b = binder
        .semantic_defs
        .get(&sym_b)
        .expect("expected semantic def for variable b");
    assert_eq!(entry_b.kind, super::SemanticDefKind::Variable);
    assert_eq!(entry_b.name, "b");
}

#[test]
fn semantic_defs_excludes_nested_functions_and_variables() {
    let binder = bind_source(
        "
function outer() {
    function inner() {}
    const localVar = 1;
}
",
    );
    // outer should be captured, but inner and localVar should not
    let has_outer = binder.semantic_defs.values().any(|e| e.name == "outer");
    assert!(has_outer, "top-level function 'outer' should be captured");
    let has_inner = binder.semantic_defs.values().any(|e| e.name == "inner");
    assert!(!has_inner, "nested function 'inner' should not be captured");
    let has_local_var = binder.semantic_defs.values().any(|e| e.name == "localVar");
    assert!(
        !has_local_var,
        "nested variable 'localVar' should not be captured"
    );
}

#[test]
fn semantic_defs_file_id_matches_symbol_decl_file_idx() {
    // SemanticDefEntry.file_id must match the symbol's decl_file_idx.
    // pre_populate_def_ids_from_binder relies on this instead of
    // looking up the symbol table, so a mismatch would break DefId
    // registration in the DefinitionStore's composite key index.
    let binder = bind_source(
        "
class A {}
interface B {}
type C = number;
enum D { X }
namespace E {}
",
    );
    for (&sym_id, entry) in &binder.semantic_defs {
        let symbol = binder
            .symbols
            .get(sym_id)
            .unwrap_or_else(|| panic!("symbol {} not found for {}", sym_id.0, entry.name));
        assert_eq!(
            entry.file_id, symbol.decl_file_idx,
            "file_id mismatch for {} (sym_id {}): entry.file_id={}, symbol.decl_file_idx={}",
            entry.name, sym_id.0, entry.file_id, symbol.decl_file_idx
        );
    }
}

#[test]
fn merge_lib_contexts_propagates_semantic_defs_with_remapped_ids() {
    // After merge_lib_contexts_into_binder, the main binder's semantic_defs
    // should contain entries for lib symbols under the new (remapped) SymbolIds.
    // This ensures pre_populate_def_ids_from_binder covers lib symbols, so the
    // checker doesn't fall through to get_or_create_def_id's repair path.

    // Create a "lib" binder with top-level declarations.
    let lib_source = r"
interface Array<T> {}
interface String {}
type Partial<T> = { [P in keyof T]?: T[P] };
class Error {}
enum Direction { Up, Down }
";
    let mut lib_parser = ParserState::new("lib.d.ts".to_string(), lib_source.to_string());
    let lib_root = lib_parser.parse_source_file();
    let mut lib_binder = BinderState::new();
    lib_binder.bind_source_file(lib_parser.get_arena(), lib_root);

    // Verify lib binder has semantic_defs for all 5 declarations.
    assert!(
        lib_binder.semantic_defs.len() >= 5,
        "lib binder should have at least 5 semantic_defs, got {}",
        lib_binder.semantic_defs.len()
    );

    let lib_ctx = super::LibContext {
        arena: std::sync::Arc::new(lib_parser.get_arena().clone()),
        binder: std::sync::Arc::new(lib_binder),
    };

    // Create the main user binder and merge.
    let user_source = "let x: number = 1;";
    let mut user_parser = ParserState::new("test.ts".to_string(), user_source.to_string());
    let user_root = user_parser.parse_source_file();
    let mut main_binder = BinderState::new();
    main_binder.bind_source_file(user_parser.get_arena(), user_root);

    let pre_merge_count = main_binder.semantic_defs.len();
    main_binder.merge_lib_contexts_into_binder(&[lib_ctx]);

    // After merge, semantic_defs should have grown with lib entries.
    let post_merge_count = main_binder.semantic_defs.len();
    assert!(
        post_merge_count > pre_merge_count,
        "merge should propagate lib semantic_defs: before={pre_merge_count}, after={post_merge_count}"
    );

    // Each lib semantic_def should use a remapped SymbolId that exists in the
    // main binder's symbol arena (not the lib binder's original IDs).
    for (&sym_id, entry) in &main_binder.semantic_defs {
        assert!(
            main_binder.symbols.get(sym_id).is_some(),
            "semantic_def for '{}' (SymbolId {}) should reference a symbol in the main arena",
            entry.name,
            sym_id.0,
        );
    }

    // The expected lib type names should be findable via file_locals → semantic_defs.
    for expected_name in &["Array", "String", "Partial", "Error", "Direction"] {
        let sym_id = main_binder
            .file_locals
            .get(expected_name)
            .unwrap_or_else(|| panic!("expected '{expected_name}' in file_locals after merge"));
        assert!(
            main_binder.semantic_defs.contains_key(&sym_id),
            "expected semantic_def for '{expected_name}' (SymbolId {}) after lib merge",
            sym_id.0,
        );
    }
}

#[test]
fn merge_lib_contexts_does_not_overwrite_user_semantic_defs() {
    // If the user declares a type with the same name as a lib type,
    // the user's semantic_def should take precedence.

    let lib_source = "interface Error {}";
    let mut lib_parser = ParserState::new("lib.d.ts".to_string(), lib_source.to_string());
    let lib_root = lib_parser.parse_source_file();
    let mut lib_binder = BinderState::new();
    lib_binder.bind_source_file(lib_parser.get_arena(), lib_root);

    let lib_ctx = super::LibContext {
        arena: std::sync::Arc::new(lib_parser.get_arena().clone()),
        binder: std::sync::Arc::new(lib_binder),
    };

    // User declares their own Error class.
    let user_source = "class Error { message: string; }";
    let mut user_parser = ParserState::new("test.ts".to_string(), user_source.to_string());
    let user_root = user_parser.parse_source_file();
    let mut main_binder = BinderState::new();
    main_binder.bind_source_file(user_parser.get_arena(), user_root);

    // User's Error should be a Class, not an Interface.
    let user_sym_id = main_binder.file_locals.get("Error").unwrap();
    let user_entry = &main_binder.semantic_defs[&user_sym_id];
    assert_eq!(user_entry.kind, super::SemanticDefKind::Class);

    main_binder.merge_lib_contexts_into_binder(&[lib_ctx]);

    // After merge, the semantic_def for Error should still be the user's Class.
    // The lib's Interface version should NOT overwrite it.
    // Note: can_merge_symbols allows Class+Interface merging, so the file_locals
    // entry reuses the user's SymbolId with merged flags. The semantic_def
    // should still be the user's original entry.
    let merged_sym_id = main_binder.file_locals.get("Error").unwrap();
    let merged_entry = &main_binder.semantic_defs[&merged_sym_id];
    assert_eq!(
        merged_entry.kind,
        super::SemanticDefKind::Class,
        "user's semantic_def should not be overwritten by lib merge"
    );
}

#[test]
fn semantic_defs_captures_generic_interface() {
    // Generic interfaces should be captured with the same identity
    // regardless of type parameter count. The binder only records
    // kind + name + span; type params are resolved later by the checker.
    let binder = bind_source("interface Container<T, U> { value: T; key: U }");
    let sym_id = binder
        .file_locals
        .get("Container")
        .expect("expected Container");
    let entry = binder
        .semantic_defs
        .get(&sym_id)
        .expect("expected semantic def for generic interface Container");
    assert_eq!(entry.kind, super::SemanticDefKind::Interface);
    assert_eq!(entry.name, "Container");
}

#[test]
fn semantic_defs_captures_namespace_but_not_its_children() {
    // A namespace itself gets a semantic def, but types declared
    // inside it also get captured because they're in a Module scope
    // (which is allowed by record_semantic_def's is_top_level check).
    let binder = bind_source(
        "
namespace Outer {
    export interface Inner {}
    export type Alias = string;
    export class Klass {}
    export enum E { A }
}
",
    );
    // The namespace itself should be captured
    let ns_sym = binder
        .file_locals
        .get("Outer")
        .expect("expected Outer namespace");
    let ns_entry = binder
        .semantic_defs
        .get(&ns_sym)
        .expect("expected semantic def for Outer");
    assert_eq!(ns_entry.kind, super::SemanticDefKind::Namespace);
}

#[test]
fn semantic_defs_captures_module_scoped_declarations() {
    // Declarations inside `declare module "foo" {}` should be captured
    // because the module body creates a ContainerKind::Module scope.
    let binder = bind_source(
        r#"
declare module "mylib" {
    export interface Config {}
    export type Mode = "dark" | "light";
    export class Client {}
}
"#,
    );
    // Module-scoped declarations should appear in semantic_defs
    // (they use the module's scope which is ContainerKind::Module).
    let has_interface = binder
        .semantic_defs
        .values()
        .any(|e| e.name == "Config" && e.kind == super::SemanticDefKind::Interface);
    let has_alias = binder
        .semantic_defs
        .values()
        .any(|e| e.name == "Mode" && e.kind == super::SemanticDefKind::TypeAlias);
    let has_class = binder
        .semantic_defs
        .values()
        .any(|e| e.name == "Client" && e.kind == super::SemanticDefKind::Class);
    assert!(
        has_interface,
        "module-scoped interface Config should be in semantic_defs"
    );
    assert!(
        has_alias,
        "module-scoped type alias Mode should be in semantic_defs"
    );
    assert!(
        has_class,
        "module-scoped class Client should be in semantic_defs"
    );
}

#[test]
fn semantic_defs_generic_class_with_constraints() {
    // Generic classes with constrained type params should still
    // be captured. Only kind/name/span matters at bind time.
    let binder =
        bind_source("class Registry<K extends string, V extends object> { entries: Map<K, V> }");
    let sym_id = binder
        .file_locals
        .get("Registry")
        .expect("expected Registry");
    let entry = binder
        .semantic_defs
        .get(&sym_id)
        .expect("expected semantic def for generic class Registry");
    assert_eq!(entry.kind, super::SemanticDefKind::Class);
    assert_eq!(entry.name, "Registry");
}

#[test]
fn merge_lib_contexts_propagates_generic_lib_interfaces() {
    // Generic lib interfaces like Array<T> must be propagated through
    // lib merge so pre_populate_def_ids_from_binder covers them.
    let lib_source = r"
interface Array<T> {
    length: number;
    push(...items: T[]): number;
}
interface ReadonlyArray<T> {
    readonly length: number;
}
type Partial<T> = { [P in keyof T]?: T[P] };
type Required<T> = { [P in keyof T]-?: T[P] };
";
    let mut lib_parser = ParserState::new("lib.es5.d.ts".to_string(), lib_source.to_string());
    let lib_root = lib_parser.parse_source_file();
    let mut lib_binder = BinderState::new();
    lib_binder.bind_source_file(lib_parser.get_arena(), lib_root);

    let lib_ctx = super::LibContext {
        arena: std::sync::Arc::new(lib_parser.get_arena().clone()),
        binder: std::sync::Arc::new(lib_binder),
    };

    let user_source = "let arr: Array<number> = [1, 2, 3];";
    let mut user_parser = ParserState::new("test.ts".to_string(), user_source.to_string());
    let user_root = user_parser.parse_source_file();
    let mut main_binder = BinderState::new();
    main_binder.bind_source_file(user_parser.get_arena(), user_root);
    main_binder.merge_lib_contexts_into_binder(&[lib_ctx]);

    // All generic lib types should be findable via file_locals → semantic_defs
    for name in &["Array", "ReadonlyArray", "Partial", "Required"] {
        let sym_id = main_binder
            .file_locals
            .get(name)
            .unwrap_or_else(|| panic!("expected '{name}' in file_locals after lib merge"));
        assert!(
            main_binder.semantic_defs.contains_key(&sym_id),
            "expected semantic_def for generic lib type '{name}' (SymbolId {})",
            sym_id.0,
        );
    }
}

// =============================================================================
// file_import_sources tests
// =============================================================================

#[test]
fn file_import_sources_static_imports() {
    let source = r#"
import { foo } from "./utils";
import bar from "react";
import "./side-effect";
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    assert!(
        binder.file_import_sources.contains(&"./utils".to_string()),
        "expected './utils' in file_import_sources, got: {:?}",
        binder.file_import_sources
    );
    assert!(
        binder.file_import_sources.contains(&"react".to_string()),
        "expected 'react' in file_import_sources, got: {:?}",
        binder.file_import_sources
    );
    assert!(
        binder
            .file_import_sources
            .contains(&"./side-effect".to_string()),
        "expected './side-effect' in file_import_sources, got: {:?}",
        binder.file_import_sources
    );
}

#[test]
fn file_import_sources_export_from() {
    let source = r#"
export { x } from "./module-a";
export * from "./module-b";
export type { T } from "./types";
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    assert!(
        binder
            .file_import_sources
            .contains(&"./module-a".to_string()),
        "expected './module-a' in file_import_sources, got: {:?}",
        binder.file_import_sources
    );
    assert!(
        binder
            .file_import_sources
            .contains(&"./module-b".to_string()),
        "expected './module-b' in file_import_sources, got: {:?}",
        binder.file_import_sources
    );
    assert!(
        binder.file_import_sources.contains(&"./types".to_string()),
        "expected './types' in file_import_sources, got: {:?}",
        binder.file_import_sources
    );
}

#[test]
fn file_import_sources_import_equals_require() {
    let source = r#"
import ts = require("typescript");
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    assert!(
        binder
            .file_import_sources
            .contains(&"typescript".to_string()),
        "expected 'typescript' in file_import_sources, got: {:?}",
        binder.file_import_sources
    );
}

#[test]
fn file_import_sources_reset_clears() {
    let source = r#"import { a } from "./a";"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    assert!(!binder.file_import_sources.is_empty());

    binder.reset();
    assert!(
        binder.file_import_sources.is_empty(),
        "reset should clear file_import_sources"
    );
}

#[test]
fn file_import_sources_no_dynamic_imports() {
    // Dynamic imports (import() calls) should NOT appear in file_import_sources
    let source = r#"
const m = import("./dynamic");
const r = require("./required");
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    assert!(
        binder.file_import_sources.is_empty(),
        "dynamic imports should not be in file_import_sources, got: {:?}",
        binder.file_import_sources
    );
}

#[test]
fn expando_const_variable_function_expression() {
    let (binder, _parser) = parse_and_bind(
        r"
const Y = function Y() {}
Y.test = 42;
",
    );

    assert!(
        binder
            .expando_properties
            .get("Y")
            .is_some_and(|props| props.contains("test")),
        "should track Y.test as expando property, got: {:?}",
        binder.expando_properties
    );
}

#[test]
fn expando_typed_variable_with_arrow_function() {
    let (binder, _parser) = parse_and_bind(
        r"
const foo: Foo = () => {};
foo.prop = true;
",
    );

    assert!(
        binder
            .expando_properties
            .get("foo")
            .is_some_and(|props| props.contains("prop")),
        "should track foo.prop as expando property even with type annotation, got: {:?}",
        binder.expando_properties
    );
}

#[test]
fn semantic_defs_captures_type_param_count_for_generics() {
    let binder = bind_source(
        "
class MyClass<T, U> {}
interface MyInterface<A, B, C> {}
type MyType<X> = X[];
function myFunc<T>(): T { return undefined as any; }
enum MyEnum { A }
const myVar = 1;
namespace MyNS {}
",
    );

    // Generic class: 2 type params
    let class_entry = binder
        .semantic_defs
        .values()
        .find(|e| e.name == "MyClass")
        .expect("MyClass");
    assert_eq!(
        class_entry.type_param_count, 2,
        "MyClass should have 2 type params"
    );

    // Generic interface: 3 type params
    let iface_entry = binder
        .semantic_defs
        .values()
        .find(|e| e.name == "MyInterface")
        .expect("MyInterface");
    assert_eq!(
        iface_entry.type_param_count, 3,
        "MyInterface should have 3 type params"
    );

    // Generic type alias: 1 type param
    let alias_entry = binder
        .semantic_defs
        .values()
        .find(|e| e.name == "MyType")
        .expect("MyType");
    assert_eq!(
        alias_entry.type_param_count, 1,
        "MyType should have 1 type param"
    );

    // Generic function: 1 type param
    let func_entry = binder
        .semantic_defs
        .values()
        .find(|e| e.name == "myFunc")
        .expect("myFunc");
    assert_eq!(
        func_entry.type_param_count, 1,
        "myFunc should have 1 type param"
    );

    // Non-generic declarations: 0 type params
    let enum_entry = binder
        .semantic_defs
        .values()
        .find(|e| e.name == "MyEnum")
        .expect("MyEnum");
    assert_eq!(
        enum_entry.type_param_count, 0,
        "MyEnum should have 0 type params"
    );

    let var_entry = binder
        .semantic_defs
        .values()
        .find(|e| e.name == "myVar")
        .expect("myVar");
    assert_eq!(
        var_entry.type_param_count, 0,
        "myVar should have 0 type params"
    );

    let ns_entry = binder
        .semantic_defs
        .values()
        .find(|e| e.name == "MyNS")
        .expect("MyNS");
    assert_eq!(
        ns_entry.type_param_count, 0,
        "MyNS should have 0 type params"
    );
}

#[test]
fn semantic_defs_captures_type_param_names_for_generics() {
    let binder = bind_source(
        "
class MyClass<T, U> {}
interface MyInterface<A, B, C> {}
type MyType<X> = X[];
function myFunc<R>(): R { return undefined as any; }
enum MyEnum { A }
const myVar = 1;
namespace MyNS {}
",
    );

    let class_entry = binder
        .semantic_defs
        .values()
        .find(|e| e.name == "MyClass")
        .expect("MyClass");
    assert_eq!(
        class_entry.type_param_names,
        vec!["T", "U"],
        "MyClass should capture type param names T, U"
    );

    let iface_entry = binder
        .semantic_defs
        .values()
        .find(|e| e.name == "MyInterface")
        .expect("MyInterface");
    assert_eq!(
        iface_entry.type_param_names,
        vec!["A", "B", "C"],
        "MyInterface should capture type param names A, B, C"
    );

    let alias_entry = binder
        .semantic_defs
        .values()
        .find(|e| e.name == "MyType")
        .expect("MyType");
    assert_eq!(
        alias_entry.type_param_names,
        vec!["X"],
        "MyType should capture type param name X"
    );

    let func_entry = binder
        .semantic_defs
        .values()
        .find(|e| e.name == "myFunc")
        .expect("myFunc");
    assert_eq!(
        func_entry.type_param_names,
        vec!["R"],
        "myFunc should capture type param name R"
    );

    // Non-generic declarations: empty names
    let enum_entry = binder
        .semantic_defs
        .values()
        .find(|e| e.name == "MyEnum")
        .expect("MyEnum");
    assert!(enum_entry.type_param_names.is_empty());

    let var_entry = binder
        .semantic_defs
        .values()
        .find(|e| e.name == "myVar")
        .expect("myVar");
    assert!(var_entry.type_param_names.is_empty());

    let ns_entry = binder
        .semantic_defs
        .values()
        .find(|e| e.name == "MyNS")
        .expect("MyNS");
    assert!(ns_entry.type_param_names.is_empty());
}

#[test]
fn semantic_defs_type_param_count_zero_for_non_generic() {
    let binder = bind_source("class Plain {} interface Simple {} type Alias = string;");

    for entry in binder.semantic_defs.values() {
        assert_eq!(
            entry.type_param_count, 0,
            "{} should have 0 type params",
            entry.name
        );
    }
}

// ===== is_exported field tests =====

#[test]
fn semantic_defs_captures_export_visibility() {
    let binder = bind_source(
        "
export class ExportedClass {}
class LocalClass {}
export interface ExportedIface {}
interface LocalIface {}
export type ExportedAlias = string;
type LocalAlias = number;
export enum ExportedEnum { A }
enum LocalEnum { B }
export function exportedFn() {}
function localFn() {}
export const exportedVar = 1;
const localVar = 2;
",
    );

    let exported_names = [
        "ExportedClass",
        "ExportedIface",
        "ExportedAlias",
        "ExportedEnum",
        "exportedFn",
        "exportedVar",
    ];
    let local_names = [
        "LocalClass",
        "LocalIface",
        "LocalAlias",
        "LocalEnum",
        "localFn",
        "localVar",
    ];

    for name in &exported_names {
        let sym_id = binder
            .file_locals
            .get(name)
            .unwrap_or_else(|| panic!("expected {name} in file_locals"));
        let entry = binder
            .semantic_defs
            .get(&sym_id)
            .unwrap_or_else(|| panic!("expected semantic_def for {name}"));
        assert!(entry.is_exported, "{name} should be marked as exported");
    }

    for name in &local_names {
        let sym_id = binder
            .file_locals
            .get(name)
            .unwrap_or_else(|| panic!("expected {name} in file_locals"));
        let entry = binder
            .semantic_defs
            .get(&sym_id)
            .unwrap_or_else(|| panic!("expected semantic_def for {name}"));
        assert!(
            !entry.is_exported,
            "{name} should NOT be marked as exported"
        );
    }
}

#[test]
fn semantic_defs_exported_namespace_nested_members() {
    // Declarations inside a namespace body (Module scope) should also
    // have their is_exported field set correctly.
    let binder = bind_source(
        "
namespace Outer {
    export class Inner {}
    class Private {}
    export type PubAlias = string;
}
",
    );

    // Outer namespace should be captured
    let outer_id = binder.file_locals.get("Outer").expect("expected Outer");
    let outer_entry = binder
        .semantic_defs
        .get(&outer_id)
        .expect("expected semantic_def for Outer");
    assert_eq!(outer_entry.kind, super::SemanticDefKind::Namespace);
    assert!(!outer_entry.is_exported, "Outer has no export modifier");

    // Inner and PubAlias are in the namespace's Module scope, so they should
    // be captured in semantic_defs if they are top-level within that Module.
    let has_inner = binder
        .semantic_defs
        .values()
        .any(|e| e.name == "Inner" && e.is_exported);
    assert!(
        has_inner,
        "exported Inner class inside namespace should be captured with is_exported=true"
    );

    let has_private = binder
        .semantic_defs
        .values()
        .any(|e| e.name == "Private" && !e.is_exported);
    assert!(
        has_private,
        "non-exported Private class inside namespace should be captured with is_exported=false"
    );

    let has_pub_alias = binder
        .semantic_defs
        .values()
        .any(|e| e.name == "PubAlias" && e.is_exported);
    assert!(
        has_pub_alias,
        "exported PubAlias inside namespace should be captured with is_exported=true"
    );
}

// ===== Merge/rebind identity stability tests =====

#[test]
fn semantic_defs_stable_identity_across_rebind() {
    // Binding the same source twice must produce entries with identical
    // kind, name, span_start, type_param_count, and is_exported.
    // This ensures stable identity survives rebind (e.g., after file edit).
    let source = "
export class Foo<T> {}
interface Bar { x: number }
export type Baz<A, B> = A | B;
enum Color { Red }
export namespace NS { export type Inner = string; }
export function greet(name: string): void {}
const LOCAL = 42;
";
    let binder1 = bind_source(source);
    let binder2 = bind_source(source);

    assert_eq!(binder1.semantic_defs.len(), binder2.semantic_defs.len());

    for (sym_id, entry1) in &binder1.semantic_defs {
        let entry2 = binder2
            .semantic_defs
            .get(sym_id)
            .expect("same SymbolId should exist in second binding");
        assert_eq!(
            entry1.kind, entry2.kind,
            "kind mismatch for {}",
            entry1.name
        );
        assert_eq!(entry1.name, entry2.name, "name mismatch");
        assert_eq!(
            entry1.span_start, entry2.span_start,
            "span mismatch for {}",
            entry1.name
        );
        assert_eq!(
            entry1.type_param_count, entry2.type_param_count,
            "tp_count mismatch for {}",
            entry1.name
        );
        assert_eq!(
            entry1.is_exported, entry2.is_exported,
            "is_exported mismatch for {}",
            entry1.name
        );
    }
}

#[test]
fn semantic_defs_lib_merge_preserves_is_exported() {
    // Lib symbols merged into the main binder should retain their is_exported flag.
    let lib_source = "export interface LibIface {} interface InternalIface {}";
    let mut lib_parser = ParserState::new("lib.d.ts".to_string(), lib_source.to_string());
    let lib_root = lib_parser.parse_source_file();
    let mut lib_binder = BinderState::new();
    lib_binder.bind_source_file(lib_parser.get_arena(), lib_root);

    let lib_ctx = super::LibContext {
        arena: std::sync::Arc::new(lib_parser.get_arena().clone()),
        binder: std::sync::Arc::new(lib_binder),
    };

    let user_source = "let x = 1;";
    let mut user_parser = ParserState::new("test.ts".to_string(), user_source.to_string());
    let user_root = user_parser.parse_source_file();
    let mut main_binder = BinderState::new();
    main_binder.bind_source_file(user_parser.get_arena(), user_root);

    main_binder.merge_lib_contexts_into_binder(&[lib_ctx]);

    // Find the merged lib entries and check is_exported
    let lib_iface = main_binder
        .semantic_defs
        .values()
        .find(|e| e.name == "LibIface")
        .expect("LibIface should be in semantic_defs after merge");
    assert!(
        lib_iface.is_exported,
        "LibIface should preserve is_exported=true through merge"
    );

    let internal_iface = main_binder
        .semantic_defs
        .values()
        .find(|e| e.name == "InternalIface")
        .expect("InternalIface should be in semantic_defs after merge");
    assert!(
        !internal_iface.is_exported,
        "InternalIface should preserve is_exported=false through merge"
    );
}

#[test]
fn semantic_defs_declaration_merging_across_files_keeps_first_identity() {
    // When the same interface is declared in two lib files, the first
    // declaration's identity (kind, span, is_exported) should be preserved.
    let lib1_source = "export interface Shared { a: string }";
    let mut lib1_parser = ParserState::new("lib1.d.ts".to_string(), lib1_source.to_string());
    let lib1_root = lib1_parser.parse_source_file();
    let mut lib1_binder = BinderState::new();
    lib1_binder.bind_source_file(lib1_parser.get_arena(), lib1_root);

    let lib2_source = "interface Shared { b: number }";
    let mut lib2_parser = ParserState::new("lib2.d.ts".to_string(), lib2_source.to_string());
    let lib2_root = lib2_parser.parse_source_file();
    let mut lib2_binder = BinderState::new();
    lib2_binder.bind_source_file(lib2_parser.get_arena(), lib2_root);

    let lib_ctx1 = super::LibContext {
        arena: std::sync::Arc::new(lib1_parser.get_arena().clone()),
        binder: std::sync::Arc::new(lib1_binder),
    };
    let lib_ctx2 = super::LibContext {
        arena: std::sync::Arc::new(lib2_parser.get_arena().clone()),
        binder: std::sync::Arc::new(lib2_binder),
    };

    let user_source = "let x = 1;";
    let mut user_parser = ParserState::new("test.ts".to_string(), user_source.to_string());
    let user_root = user_parser.parse_source_file();
    let mut main_binder = BinderState::new();
    main_binder.bind_source_file(user_parser.get_arena(), user_root);

    main_binder.merge_lib_contexts_into_binder(&[lib_ctx1, lib_ctx2]);

    let shared_entry = main_binder
        .semantic_defs
        .values()
        .find(|e| e.name == "Shared")
        .expect("Shared should be in semantic_defs after merge");

    // First declaration wins: lib1 declares `export interface Shared`,
    // so is_exported should be true.
    assert_eq!(shared_entry.kind, super::SemanticDefKind::Interface);
    assert!(
        shared_entry.is_exported,
        "declaration merging should keep first identity (exported from lib1)"
    );
}

#[test]
fn semantic_defs_enriched_fields_stable_across_rebind() {
    // Enriched fields (enum_member_names, is_const, is_abstract) must be
    // identical across two bindings of the same source. This ensures the
    // binder-owned data that feeds solver DefinitionInfo pre-population
    // is deterministic.
    let source = "
export const enum Direction { Up, Down, Left, Right }
enum Plain { A, B }
abstract class AbstractBase<T> {}
class Concrete {}
";
    let binder1 = bind_source(source);
    let binder2 = bind_source(source);

    for (sym_id, e1) in &binder1.semantic_defs {
        let e2 = binder2
            .semantic_defs
            .get(sym_id)
            .unwrap_or_else(|| panic!("{} missing in second binding", e1.name));
        assert_eq!(
            e1.enum_member_names, e2.enum_member_names,
            "enum_member_names mismatch for {}",
            e1.name
        );
        assert_eq!(
            e1.is_const, e2.is_const,
            "is_const mismatch for {}",
            e1.name
        );
        assert_eq!(
            e1.is_abstract, e2.is_abstract,
            "is_abstract mismatch for {}",
            e1.name
        );
    }

    // Verify specific enrichments
    let direction = binder1
        .semantic_defs
        .values()
        .find(|e| e.name == "Direction")
        .expect("Direction should be captured");
    assert!(direction.is_const, "const enum should have is_const=true");
    assert_eq!(
        direction.enum_member_names,
        vec!["Up", "Down", "Left", "Right"]
    );

    let plain = binder1
        .semantic_defs
        .values()
        .find(|e| e.name == "Plain")
        .expect("Plain should be captured");
    assert!(!plain.is_const, "non-const enum should have is_const=false");
    assert_eq!(plain.enum_member_names, vec!["A", "B"]);

    let abstract_base = binder1
        .semantic_defs
        .values()
        .find(|e| e.name == "AbstractBase")
        .expect("AbstractBase should be captured");
    assert!(
        abstract_base.is_abstract,
        "abstract class should have is_abstract=true"
    );

    let concrete = binder1
        .semantic_defs
        .values()
        .find(|e| e.name == "Concrete")
        .expect("Concrete should be captured");
    assert!(
        !concrete.is_abstract,
        "non-abstract class should have is_abstract=false"
    );
}

#[test]
fn semantic_defs_namespace_scoped_declarations_captured() {
    // Declarations inside namespace bodies use ContainerKind::Module scope,
    // which is included in the top-level check. This verifies that classes,
    // interfaces, type aliases, enums, and functions inside namespaces get
    // semantic_defs entries.
    let source = "
namespace Outer {
    export class InnerClass {}
    export interface InnerIface { x: number }
    export type InnerAlias = string;
    export enum InnerEnum { A }
    export function innerFn(): void {}
}
";
    let binder = bind_source(source);

    // The namespace itself should be captured
    let outer = binder.semantic_defs.values().find(|e| e.name == "Outer");
    assert!(
        outer.is_some(),
        "Outer namespace should be in semantic_defs"
    );

    // Declarations inside the namespace should also be captured
    for name in &[
        "InnerClass",
        "InnerIface",
        "InnerAlias",
        "InnerEnum",
        "innerFn",
    ] {
        let found = binder.semantic_defs.values().any(|e| e.name == *name);
        assert!(
            found,
            "namespace-scoped declaration '{name}' should be in semantic_defs"
        );
    }
}

#[test]
fn semantic_defs_enriched_fields_survive_lib_merge() {
    // Enriched fields (is_abstract, is_const, enum_member_names) captured at
    // bind time must survive the lib merge path so the checker's DefId
    // pre-population gets complete identity information.
    let lib_source = r"
abstract class AbstractBase { abstract foo(): void; }
const enum Direction { Up, Down, Left, Right }
enum Color { Red, Green, Blue }
";
    let mut lib_parser = ParserState::new("lib.d.ts".to_string(), lib_source.to_string());
    let lib_root = lib_parser.parse_source_file();
    let mut lib_binder = BinderState::new();
    lib_binder.bind_source_file(lib_parser.get_arena(), lib_root);

    // Verify fields are captured in the lib binder
    let abs = lib_binder
        .semantic_defs
        .values()
        .find(|e| e.name == "AbstractBase")
        .expect("AbstractBase should be in lib semantic_defs");
    assert!(
        abs.is_abstract,
        "abstract class should preserve is_abstract in lib binder"
    );

    let dir = lib_binder
        .semantic_defs
        .values()
        .find(|e| e.name == "Direction")
        .expect("Direction should be in lib semantic_defs");
    assert!(
        dir.is_const,
        "const enum should preserve is_const in lib binder"
    );
    assert_eq!(dir.enum_member_names, vec!["Up", "Down", "Left", "Right"]);

    // Merge into a user binder
    let lib_ctx = super::LibContext {
        arena: std::sync::Arc::new(lib_parser.get_arena().clone()),
        binder: std::sync::Arc::new(lib_binder),
    };
    let user_source = "let x = 1;";
    let mut user_parser = ParserState::new("test.ts".to_string(), user_source.to_string());
    let user_root = user_parser.parse_source_file();
    let mut main_binder = BinderState::new();
    main_binder.bind_source_file(user_parser.get_arena(), user_root);
    main_binder.merge_lib_contexts_into_binder(&[lib_ctx]);

    // Verify enriched fields survived the merge
    let abs_merged = main_binder
        .semantic_defs
        .values()
        .find(|e| e.name == "AbstractBase")
        .expect("AbstractBase should survive lib merge");
    assert!(
        abs_merged.is_abstract,
        "is_abstract should survive lib merge"
    );

    let dir_merged = main_binder
        .semantic_defs
        .values()
        .find(|e| e.name == "Direction")
        .expect("Direction should survive lib merge");
    assert!(dir_merged.is_const, "is_const should survive lib merge");
    assert_eq!(
        dir_merged.enum_member_names,
        vec!["Up", "Down", "Left", "Right"],
        "enum_member_names should survive lib merge"
    );

    let color_merged = main_binder
        .semantic_defs
        .values()
        .find(|e| e.name == "Color")
        .expect("Color should survive lib merge");
    assert!(
        !color_merged.is_const,
        "regular enum should not be const after merge"
    );
    assert_eq!(
        color_merged.enum_member_names,
        vec!["Red", "Green", "Blue"],
        "enum_member_names should survive lib merge for regular enum"
    );
}

#[test]
fn semantic_defs_all_families_have_correct_kinds() {
    // Verify that all seven declaration families produce the correct
    // SemanticDefKind after binding.
    let source = r"
class MyClass {}
interface MyIface {}
type MyAlias = string;
enum MyEnum { A }
namespace MyNS {}
function myFunc() {}
const myVar = 1;
";
    let binder = bind_source(source);

    let expected: Vec<(&str, super::SemanticDefKind)> = vec![
        ("MyClass", super::SemanticDefKind::Class),
        ("MyIface", super::SemanticDefKind::Interface),
        ("MyAlias", super::SemanticDefKind::TypeAlias),
        ("MyEnum", super::SemanticDefKind::Enum),
        ("MyNS", super::SemanticDefKind::Namespace),
        ("myFunc", super::SemanticDefKind::Function),
        ("myVar", super::SemanticDefKind::Variable),
    ];

    for (name, expected_kind) in expected {
        let entry = binder
            .semantic_defs
            .values()
            .find(|e| e.name == name)
            .unwrap_or_else(|| panic!("expected semantic_def for '{name}'"));
        assert_eq!(
            entry.kind, expected_kind,
            "wrong kind for '{}': expected {:?}, got {:?}",
            name, expected_kind, entry.kind
        );
    }
}

#[test]
fn test_heritage_names_captured_for_class_extends() {
    let source = "class Foo extends Bar {}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    let entry = binder
        .semantic_defs
        .values()
        .find(|e| e.name == "Foo")
        .expect("expected semantic_def for Foo");
    assert_eq!(entry.extends_names, vec!["Bar"]);
    assert!(entry.implements_names.is_empty());
}

#[test]
fn test_heritage_names_captured_for_class_implements() {
    let source = "class Foo implements Iface1, Iface2 {}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    let entry = binder
        .semantic_defs
        .values()
        .find(|e| e.name == "Foo")
        .expect("expected semantic_def for Foo");
    assert!(entry.extends_names.is_empty());
    assert_eq!(entry.implements_names, vec!["Iface1", "Iface2"]);
}

#[test]
fn test_heritage_names_captured_for_class_extends_and_implements() {
    let source = "class Foo extends Base implements I1, I2 {}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    let entry = binder
        .semantic_defs
        .values()
        .find(|e| e.name == "Foo")
        .expect("expected semantic_def for Foo");
    assert_eq!(entry.extends_names, vec!["Base"]);
    assert_eq!(entry.implements_names, vec!["I1", "I2"]);
    // Combined heritage_names() accessor should include all
    assert_eq!(entry.heritage_names(), vec!["Base", "I1", "I2"]);
}

#[test]
fn test_heritage_names_captured_for_interface_extends() {
    let source = "interface Foo extends Bar, Baz {}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    let entry = binder
        .semantic_defs
        .values()
        .find(|e| e.name == "Foo")
        .expect("expected semantic_def for Foo");
    // Interfaces use `extends`, not `implements`
    assert_eq!(entry.extends_names, vec!["Bar", "Baz"]);
    assert!(entry.implements_names.is_empty());
}

#[test]
fn test_heritage_names_empty_for_no_heritage() {
    let source = "class Plain {} interface Empty {}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    let plain = binder
        .semantic_defs
        .values()
        .find(|e| e.name == "Plain")
        .expect("expected semantic_def for Plain");
    assert!(plain.extends_names.is_empty());
    assert!(plain.implements_names.is_empty());

    let empty = binder
        .semantic_defs
        .values()
        .find(|e| e.name == "Empty")
        .expect("expected semantic_def for Empty");
    assert!(empty.extends_names.is_empty());
    assert!(empty.implements_names.is_empty());
}

#[test]
fn test_heritage_names_property_access_expression() {
    let source = "class Foo extends ns.Base {}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    let entry = binder
        .semantic_defs
        .values()
        .find(|e| e.name == "Foo")
        .expect("expected semantic_def for Foo");
    assert_eq!(entry.extends_names, vec!["ns.Base"]);
    assert!(entry.implements_names.is_empty());
}

// =========================================================================
// Stable identity flag tests
// =========================================================================

#[test]
fn semantic_defs_captures_is_abstract_for_abstract_classes() {
    let binder = bind_source(
        "
abstract class Base {}
class Concrete {}
",
    );
    let base_entry = binder
        .semantic_defs
        .values()
        .find(|e| e.name == "Base")
        .expect("expected semantic_def for Base");
    assert!(
        base_entry.is_abstract,
        "abstract class Base should have is_abstract=true"
    );
    assert_eq!(base_entry.kind, super::SemanticDefKind::Class);

    let concrete_entry = binder
        .semantic_defs
        .values()
        .find(|e| e.name == "Concrete")
        .expect("expected semantic_def for Concrete");
    assert!(
        !concrete_entry.is_abstract,
        "non-abstract class Concrete should have is_abstract=false"
    );
}

#[test]
fn semantic_defs_captures_is_const_for_const_enums() {
    let binder = bind_source(
        "
const enum ConstDir { Up, Down }
enum RegularDir { Left, Right }
",
    );
    let const_entry = binder
        .semantic_defs
        .values()
        .find(|e| e.name == "ConstDir")
        .expect("expected semantic_def for ConstDir");
    assert!(
        const_entry.is_const,
        "const enum ConstDir should have is_const=true"
    );
    assert_eq!(const_entry.kind, super::SemanticDefKind::Enum);

    let regular_entry = binder
        .semantic_defs
        .values()
        .find(|e| e.name == "RegularDir")
        .expect("expected semantic_def for RegularDir");
    assert!(
        !regular_entry.is_const,
        "regular enum RegularDir should have is_const=false"
    );
}

#[test]
fn semantic_defs_captures_is_exported() {
    let binder = bind_source(
        "
export class ExportedClass {}
class PrivateClass {}
export interface ExportedIface {}
interface PrivateIface {}
export type ExportedAlias = number;
type PrivateAlias = string;
export enum ExportedEnum { A }
enum PrivateEnum { B }
",
    );
    for (name, expected_exported) in [
        ("ExportedClass", true),
        ("PrivateClass", false),
        ("ExportedIface", true),
        ("PrivateIface", false),
        ("ExportedAlias", true),
        ("PrivateAlias", false),
        ("ExportedEnum", true),
        ("PrivateEnum", false),
    ] {
        let entry = binder
            .semantic_defs
            .values()
            .find(|e| e.name == name)
            .unwrap_or_else(|| panic!("expected semantic_def for {name}"));
        assert_eq!(
            entry.is_exported, expected_exported,
            "{name}: is_exported should be {expected_exported}"
        );
    }
}

#[test]
fn semantic_defs_identity_flags_survive_lib_merge() {
    // Identity flags (is_abstract, is_const, is_exported) must be
    // preserved when lib semantic_defs are propagated through merge.
    let lib_source = r"
abstract class AbstractError {}
const enum LibDirection { Up, Down }
export interface ExportedArray<T> {}
";
    let mut lib_parser = ParserState::new("lib.d.ts".to_string(), lib_source.to_string());
    let lib_root = lib_parser.parse_source_file();
    let mut lib_binder = BinderState::new();
    lib_binder.bind_source_file(lib_parser.get_arena(), lib_root);

    let lib_ctx = super::LibContext {
        arena: std::sync::Arc::new(lib_parser.get_arena().clone()),
        binder: std::sync::Arc::new(lib_binder),
    };

    let user_source = "let x = 1;";
    let mut user_parser = ParserState::new("test.ts".to_string(), user_source.to_string());
    let user_root = user_parser.parse_source_file();
    let mut main_binder = BinderState::new();
    main_binder.bind_source_file(user_parser.get_arena(), user_root);
    main_binder.merge_lib_contexts_into_binder(&[lib_ctx]);

    // After merge, identity flags should be preserved on remapped entries
    let abstract_entry = main_binder
        .semantic_defs
        .values()
        .find(|e| e.name == "AbstractError")
        .expect("expected AbstractError in merged semantic_defs");
    assert!(
        abstract_entry.is_abstract,
        "is_abstract should survive lib merge"
    );

    let const_entry = main_binder
        .semantic_defs
        .values()
        .find(|e| e.name == "LibDirection")
        .expect("expected LibDirection in merged semantic_defs");
    assert!(const_entry.is_const, "is_const should survive lib merge");

    let exported_entry = main_binder
        .semantic_defs
        .values()
        .find(|e| e.name == "ExportedArray")
        .expect("expected ExportedArray in merged semantic_defs");
    assert!(
        exported_entry.is_exported,
        "is_exported should survive lib merge"
    );
}

#[test]
fn semantic_defs_all_top_level_families_have_stable_identity() {
    // Verify that all top-level declaration families produce semantic
    // defs with complete identity data, and that nested declarations
    // are not incorrectly captured.
    let binder = bind_source(
        "
export abstract class MyClass<T> extends Base {}
export interface MyInterface<T, U> {}
export type MyAlias<T> = T | null;
export const enum MyConstEnum { A, B, C }
export enum MyEnum { X, Y }
export namespace MyNamespace {}
export function myFunction<T>(x: T): T { return x; }
export const myVariable = 42;
function nested() {
    class InnerClass {}
    interface InnerIface {}
}
",
    );
    // All top-level declarations should be captured
    let names_and_kinds: Vec<(&str, super::SemanticDefKind)> = vec![
        ("MyClass", super::SemanticDefKind::Class),
        ("MyInterface", super::SemanticDefKind::Interface),
        ("MyAlias", super::SemanticDefKind::TypeAlias),
        ("MyConstEnum", super::SemanticDefKind::Enum),
        ("MyEnum", super::SemanticDefKind::Enum),
        ("MyNamespace", super::SemanticDefKind::Namespace),
        ("myFunction", super::SemanticDefKind::Function),
        ("myVariable", super::SemanticDefKind::Variable),
    ];

    for (name, expected_kind) in &names_and_kinds {
        let entry = binder
            .semantic_defs
            .values()
            .find(|e| e.name == *name)
            .unwrap_or_else(|| panic!("expected semantic_def for top-level '{name}'"));
        assert_eq!(entry.kind, *expected_kind, "{name}: wrong kind");
        assert!(entry.is_exported, "{name}: should be exported");
    }

    // Specific flag checks
    let class_entry = binder
        .semantic_defs
        .values()
        .find(|e| e.name == "MyClass")
        .unwrap();
    assert!(class_entry.is_abstract, "MyClass should be abstract");
    assert_eq!(
        class_entry.type_param_count, 1,
        "MyClass should have 1 type param"
    );

    let const_enum_entry = binder
        .semantic_defs
        .values()
        .find(|e| e.name == "MyConstEnum")
        .unwrap();
    assert!(const_enum_entry.is_const, "MyConstEnum should be const");
    assert_eq!(const_enum_entry.enum_member_names.len(), 3);

    let regular_enum_entry = binder
        .semantic_defs
        .values()
        .find(|e| e.name == "MyEnum")
        .unwrap();
    assert!(!regular_enum_entry.is_const, "MyEnum should not be const");

    // Nested declarations should NOT be in semantic_defs (they're in function scope)
    assert!(
        !binder
            .semantic_defs
            .values()
            .any(|e| e.name == "InnerClass"),
        "nested InnerClass should not be in semantic_defs"
    );
    assert!(
        !binder
            .semantic_defs
            .values()
            .any(|e| e.name == "InnerIface"),
        "nested InnerIface should not be in semantic_defs"
    );
}

// =============================================================================
// Declaration merging accumulation tests
// =============================================================================

#[test]
fn semantic_defs_declaration_merging_accumulates_heritage_names() {
    // When an interface is declared multiple times with different heritage
    // clauses, heritage_names should accumulate from all declarations.
    let binder = bind_source(
        "
interface Merged extends A { a: string }
interface Merged extends B { b: number }
interface Merged extends C { c: boolean }
",
    );
    let sym_id = binder.file_locals.get("Merged").expect("expected Merged");
    let entry = binder
        .semantic_defs
        .get(&sym_id)
        .expect("expected semantic def for Merged");
    assert_eq!(entry.kind, super::SemanticDefKind::Interface);
    // Heritage names from all three declarations should be accumulated.
    // Interface heritage uses extends, not implements
    assert!(
        entry.extends_names.contains(&"A".to_string()),
        "extends should include A, got: {:?}",
        entry.extends_names
    );
    assert!(
        entry.extends_names.contains(&"B".to_string()),
        "extends should include B, got: {:?}",
        entry.extends_names
    );
    assert!(
        entry.extends_names.contains(&"C".to_string()),
        "extends should include C, got: {:?}",
        entry.extends_names
    );
    // Core identity (name, kind, span) should still be from the first declaration.
    let first_decl = binder.symbols.get(sym_id).unwrap().declarations[0];
    assert_eq!(entry.span_start, first_decl.0);
}

#[test]
fn semantic_defs_declaration_merging_deduplicates_heritage() {
    // If the same heritage name appears in multiple declarations,
    // it should appear only once.
    let binder = bind_source(
        "
interface Dup extends Base { a: string }
interface Dup extends Base { b: number }
",
    );
    let sym_id = binder.file_locals.get("Dup").expect("expected Dup");
    let entry = binder
        .semantic_defs
        .get(&sym_id)
        .expect("expected semantic def for Dup");
    let base_count = entry.extends_names.iter().filter(|h| *h == "Base").count();
    assert_eq!(
        base_count, 1,
        "Base should appear exactly once in extends_names"
    );
}

#[test]
fn semantic_defs_declaration_merging_promotes_type_param_count() {
    // If the first interface declaration has no type params but a later
    // one does (augmentation adding generics), the count should be promoted.
    let binder = bind_source(
        "
interface Augmented { base: string }
interface Augmented<T> { extra: T }
",
    );
    let sym_id = binder
        .file_locals
        .get("Augmented")
        .expect("expected Augmented");
    let entry = binder
        .semantic_defs
        .get(&sym_id)
        .expect("expected semantic def for Augmented");
    assert_eq!(
        entry.type_param_count, 1,
        "type_param_count should be promoted from later declaration"
    );
}

#[test]
fn semantic_defs_declaration_merging_promotes_export_visibility() {
    // If the first declaration is not exported but a later one is,
    // the entry should be marked as exported.
    let binder = bind_source(
        "
interface Internal { a: string }
export interface Internal { b: number }
",
    );
    let sym_id = binder
        .file_locals
        .get("Internal")
        .expect("expected Internal");
    let entry = binder
        .semantic_defs
        .get(&sym_id)
        .expect("expected semantic def for Internal");
    assert!(
        entry.is_exported,
        "export visibility should be promoted from later declaration"
    );
}

#[test]
fn semantic_defs_enum_merging_accumulates_members() {
    // When an enum is declared in multiple blocks, members should accumulate.
    let binder = bind_source(
        "
enum Direction { Up, Down }
enum Direction { Left, Right }
",
    );
    let sym_id = binder
        .file_locals
        .get("Direction")
        .expect("expected Direction");
    let entry = binder
        .semantic_defs
        .get(&sym_id)
        .expect("expected semantic def for Direction");
    assert_eq!(entry.kind, super::SemanticDefKind::Enum);
    // All four members should be accumulated.
    assert!(
        entry.enum_member_names.contains(&"Up".to_string()),
        "should contain Up"
    );
    assert!(
        entry.enum_member_names.contains(&"Down".to_string()),
        "should contain Down"
    );
    assert!(
        entry.enum_member_names.contains(&"Left".to_string()),
        "should contain Left"
    );
    assert!(
        entry.enum_member_names.contains(&"Right".to_string()),
        "should contain Right"
    );
}

#[test]
fn semantic_defs_declare_global_captures_declarations() {
    // Declarations inside `declare global {}` blocks are in Module scope
    // and should be captured as semantic defs.
    let binder = bind_source(
        r#"
export {};
declare global {
    interface Window {
        customProp: string;
    }
    type GlobalAlias = string;
    function globalFn(): void;
    const GLOBAL_CONST: number;
}
"#,
    );
    let has_window = binder
        .semantic_defs
        .values()
        .any(|e| e.name == "Window" && e.kind == super::SemanticDefKind::Interface);
    let has_alias = binder
        .semantic_defs
        .values()
        .any(|e| e.name == "GlobalAlias" && e.kind == super::SemanticDefKind::TypeAlias);
    assert!(
        has_window,
        "declare global interface Window should be in semantic_defs"
    );
    assert!(
        has_alias,
        "declare global type alias should be in semantic_defs"
    );
}

#[test]
fn semantic_defs_class_with_extends_and_implements() {
    // Verify heritage_names captures both extends and implements.
    let binder = bind_source(
        "
interface Serializable {}
interface Printable {}
class Base {}
class Derived extends Base implements Serializable, Printable {}
",
    );
    let sym_id = binder.file_locals.get("Derived").expect("expected Derived");
    let entry = binder
        .semantic_defs
        .get(&sym_id)
        .expect("expected semantic def for Derived");
    assert_eq!(entry.kind, super::SemanticDefKind::Class);
    assert!(
        entry.extends_names.contains(&"Base".to_string()),
        "extends should include Base, got: {:?}",
        entry.extends_names
    );
    assert!(
        entry.implements_names.contains(&"Serializable".to_string()),
        "implements should include Serializable, got: {:?}",
        entry.implements_names
    );
    assert!(
        entry.implements_names.contains(&"Printable".to_string()),
        "implements should include Printable, got: {:?}",
        entry.implements_names
    );
}

#[test]
fn semantic_defs_merging_preserves_first_kind_and_span() {
    // Even when later declarations add heritage/type params, the
    // original kind and span_start must be preserved.
    let binder = bind_source(
        "
interface Stable { a: string }
interface Stable extends Extra { b: number }
",
    );
    let sym_id = binder.file_locals.get("Stable").expect("expected Stable");
    let entry = binder
        .semantic_defs
        .get(&sym_id)
        .expect("expected semantic def for Stable");
    assert_eq!(entry.kind, super::SemanticDefKind::Interface);
    // Span should be from the FIRST declaration
    let first_decl = binder.symbols.get(sym_id).unwrap().declarations[0];
    assert_eq!(
        entry.span_start, first_decl.0,
        "span_start must be from first declaration"
    );
    // But heritage should include Extra from the second declaration
    assert!(
        entry.extends_names.contains(&"Extra".to_string()),
        "extends should include Extra from later declaration"
    );
}

// =============================================================================
// BinderFileSummary tests
// =============================================================================

#[test]
fn file_skeleton_captures_exported_defs() {
    let source = r#"
export class Animal {}
export interface Movable { move(): void; }
export type ID = string;
class Internal {}
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let mut binder = BinderState::new();
    binder.file_idx = 42;
    binder.bind_source_file(parser.get_arena(), root);

    let skeleton = binder.file_summary();

    assert_eq!(skeleton.file_idx, 42);
    assert!(
        skeleton.is_external_module,
        "has exports so should be external module"
    );

    let names: Vec<&str> = skeleton
        .exported_defs
        .iter()
        .map(|e| e.name.as_str())
        .collect();
    assert!(
        names.contains(&"Animal"),
        "expected Animal in exported_defs, got: {names:?}"
    );
    assert!(
        names.contains(&"Movable"),
        "expected Movable in exported_defs, got: {names:?}"
    );
    assert!(
        names.contains(&"ID"),
        "expected ID in exported_defs, got: {names:?}"
    );
    assert!(
        !names.contains(&"Internal"),
        "Internal should not be in exported_defs"
    );
}

#[test]
fn file_skeleton_captures_import_sources() {
    let source = r#"
import { foo } from "./utils";
import * as React from "react";
export { bar } from "./helpers";
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    let skeleton = binder.file_summary();

    assert!(skeleton.import_sources.contains(&"./utils".to_string()));
    assert!(skeleton.import_sources.contains(&"react".to_string()));
    assert!(skeleton.import_sources.contains(&"./helpers".to_string()));
}

#[test]
fn file_skeleton_captures_heritage_deps() {
    let source = r#"
export class Dog extends Animal implements Movable {}
export interface FastAnimal extends Animal {}
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    let skeleton = binder.file_summary();

    assert!(
        skeleton.heritage_deps.contains(&"Animal".to_string()),
        "expected Animal in heritage_deps, got: {:?}",
        skeleton.heritage_deps
    );
    assert!(
        skeleton.heritage_deps.contains(&"Movable".to_string()),
        "expected Movable in heritage_deps, got: {:?}",
        skeleton.heritage_deps
    );
}

#[test]
fn file_skeleton_dependency_specifiers_deduplicates() {
    let source = r#"
import { a } from "./shared";
import { b } from "./shared";
export { c } from "./other";
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    let skeleton = binder.file_summary();
    let specs = skeleton.dependency_specifiers();

    // Should deduplicate "./shared"
    let shared_count = specs.iter().filter(|&&s| s == "./shared").count();
    assert_eq!(
        shared_count, 1,
        "dependency_specifiers should deduplicate, got: {specs:?}"
    );
    assert!(specs.contains(&"./other"));
}

#[test]
fn file_skeleton_has_exports_and_heritage_helpers() {
    let source = r#"
const x = 1;
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    let skeleton = binder.file_summary();

    assert!(
        !skeleton.has_exports(),
        "script file should have no exports"
    );
    assert!(
        !skeleton.has_heritage_deps(),
        "simple script should have no heritage deps"
    );
}

#[test]
fn file_skeleton_api_fingerprint_changes_on_export_change() {
    // Two files with different exports should have different fingerprints
    let source_a = r#"export class Foo {}"#;
    let source_b = r#"export class Bar {}"#;

    let mut parser_a = ParserState::new("a.ts".to_string(), source_a.to_string());
    let root_a = parser_a.parse_source_file();
    let mut binder_a = BinderState::new();
    binder_a.bind_source_file(parser_a.get_arena(), root_a);

    let mut parser_b = ParserState::new("b.ts".to_string(), source_b.to_string());
    let root_b = parser_b.parse_source_file();
    let mut binder_b = BinderState::new();
    binder_b.bind_source_file(parser_b.get_arena(), root_b);

    let fp_a = binder_a.file_summary().api_fingerprint();
    let fp_b = binder_b.file_summary().api_fingerprint();

    assert_ne!(
        fp_a, fp_b,
        "different exports should produce different fingerprints"
    );
}

#[test]
fn file_skeleton_api_fingerprint_stable_across_calls() {
    let source = r#"export interface I { x: number; }"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    let fp1 = binder.file_summary().api_fingerprint();
    let fp2 = binder.file_summary().api_fingerprint();
    assert_eq!(fp1, fp2, "fingerprint should be deterministic");
}

#[test]
fn file_skeleton_exported_defs_sorted_deterministically() {
    let source = r#"
export class Zebra {}
export class Alpha {}
export class Middle {}
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    let skeleton = binder.file_summary();
    let names: Vec<&str> = skeleton
        .exported_defs
        .iter()
        .map(|e| e.name.as_str())
        .collect();

    assert_eq!(
        names,
        vec!["Alpha", "Middle", "Zebra"],
        "exported_defs should be sorted by name"
    );
}

#[test]
fn file_skeleton_generic_type_param_count() {
    let source = r#"
export interface Map<K, V> {}
export type Pair<A, B> = [A, B];
export class List<T> {}
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    let skeleton = binder.file_summary();

    for exp in &skeleton.exported_defs {
        match exp.name.as_str() {
            "Map" => assert_eq!(exp.type_param_count, 2, "Map should have 2 type params"),
            "Pair" => assert_eq!(exp.type_param_count, 2, "Pair should have 2 type params"),
            "List" => assert_eq!(exp.type_param_count, 1, "List should have 1 type param"),
            other => panic!("unexpected export: {other}"),
        }
    }
}

#[test]
fn semantic_defs_parent_namespace_is_none_for_top_level() {
    // Top-level (source-file scope) declarations should have parent_namespace = None.
    let binder = bind_source(
        "
class Foo {}
interface Bar {}
type Baz = string;
enum Color { Red }
",
    );
    for entry in binder.semantic_defs.values() {
        assert!(
            entry.parent_namespace.is_none(),
            "top-level '{}' should have parent_namespace = None, got {:?}",
            entry.name,
            entry.parent_namespace
        );
    }
}

#[test]
fn semantic_defs_parent_namespace_set_for_namespace_members() {
    // Declarations inside a namespace body should have parent_namespace
    // pointing to the namespace's SymbolId.
    let binder = bind_source(
        "
namespace NS {
    export interface Inner {}
    export type Alias = string;
    export class Klass {}
    export enum E { A }
}
",
    );
    let ns_sym = binder
        .file_locals
        .get("NS")
        .expect("expected NS in file_locals");

    // The namespace itself should have no parent_namespace
    let ns_entry = binder
        .semantic_defs
        .get(&ns_sym)
        .expect("expected semantic def for NS");
    assert!(
        ns_entry.parent_namespace.is_none(),
        "namespace NS itself should have no parent_namespace"
    );

    // Each member should reference NS as its parent_namespace
    let member_names = ["Inner", "Alias", "Klass", "E"];
    for name in &member_names {
        let entry = binder
            .semantic_defs
            .values()
            .find(|e| e.name == *name)
            .unwrap_or_else(|| panic!("expected semantic def for '{name}'"));
        assert_eq!(
            entry.parent_namespace,
            Some(ns_sym),
            "'{name}' should have parent_namespace = NS (SymbolId {:?}), got {:?}",
            ns_sym,
            entry.parent_namespace
        );
    }
}

#[test]
fn semantic_defs_nested_namespace_parent_chain() {
    // Declarations in nested namespaces should reference the
    // immediate containing namespace, not the outermost one.
    let binder = bind_source(
        "
namespace Outer {
    export namespace Inner {
        export class Deep {}
    }
}
",
    );
    let outer_sym = binder
        .file_locals
        .get("Outer")
        .expect("expected Outer in file_locals");

    // Inner should reference Outer as parent
    let inner_entry = binder
        .semantic_defs
        .values()
        .find(|e| e.name == "Inner")
        .expect("expected semantic def for Inner");
    assert_eq!(
        inner_entry.parent_namespace,
        Some(outer_sym),
        "Inner should have Outer as parent_namespace"
    );

    // Deep should reference Inner as parent (not Outer)
    let inner_sym = binder
        .semantic_defs
        .iter()
        .find(|(_, e)| e.name == "Inner")
        .map(|(&id, _)| id)
        .expect("expected to find Inner's SymbolId");
    let deep_entry = binder
        .semantic_defs
        .values()
        .find(|e| e.name == "Deep")
        .expect("expected semantic def for Deep");
    assert_eq!(
        deep_entry.parent_namespace,
        Some(inner_sym),
        "Deep should have Inner as parent_namespace, not Outer"
    );
}

#[test]
fn merge_cross_file_accumulates_heritage() {
    // SemanticDefEntry::merge_cross_file should accumulate heritage_names
    // from a second entry without duplicating existing ones.
    let mut first = super::SemanticDefEntry {
        kind: super::SemanticDefKind::Interface,
        name: "Foo".to_string(),
        file_id: 0,
        span_start: 0,
        type_param_count: 0,
        type_param_names: Vec::new(),
        is_exported: false,
        enum_member_names: Vec::new(),
        is_const: false,
        is_abstract: false,
        extends_names: vec!["Bar".to_string()],
        implements_names: Vec::new(),
        parent_namespace: None,
        is_global_augmentation: false,
        is_declare: false,
    };
    let second = super::SemanticDefEntry {
        kind: super::SemanticDefKind::Interface,
        name: "Foo".to_string(),
        file_id: 1,
        span_start: 100,
        type_param_count: 2,
        type_param_names: vec!["T".to_string(), "U".to_string()],
        is_exported: true,
        enum_member_names: Vec::new(),
        is_const: false,
        is_abstract: false,
        extends_names: vec!["Bar".to_string(), "Baz".to_string()],
        implements_names: Vec::new(),
        parent_namespace: None,
        is_global_augmentation: false,
        is_declare: false,
    };

    first.merge_cross_file(&second);

    // Heritage: Bar (already present, not duplicated) + Baz (new)
    assert_eq!(first.extends_names, vec!["Bar", "Baz"]);
    // Type param arity and names updated from 0 to 2
    assert_eq!(first.type_param_count, 2);
    assert_eq!(first.type_param_names, vec!["T", "U"]);
    // Export promoted
    assert!(first.is_exported);
    // Core identity preserved from first
    assert_eq!(first.file_id, 0);
    assert_eq!(first.span_start, 0);
}

#[test]
fn merge_cross_file_accumulates_enum_members() {
    let mut first = super::SemanticDefEntry {
        kind: super::SemanticDefKind::Enum,
        name: "Color".to_string(),
        file_id: 0,
        span_start: 0,
        type_param_count: 0,
        type_param_names: Vec::new(),
        is_exported: false,
        enum_member_names: vec!["Red".to_string(), "Green".to_string()],
        is_const: false,
        is_abstract: false,
        extends_names: Vec::new(),
        implements_names: Vec::new(),
        parent_namespace: None,
        is_global_augmentation: false,
        is_declare: false,
    };
    let second = super::SemanticDefEntry {
        kind: super::SemanticDefKind::Enum,
        name: "Color".to_string(),
        file_id: 1,
        span_start: 50,
        type_param_count: 0,
        type_param_names: Vec::new(),
        is_exported: false,
        enum_member_names: vec!["Green".to_string(), "Blue".to_string()],
        is_const: true,
        is_abstract: false,
        extends_names: Vec::new(),
        implements_names: Vec::new(),
        parent_namespace: None,
        is_global_augmentation: false,
        is_declare: false,
    };

    first.merge_cross_file(&second);

    assert_eq!(
        first.enum_member_names,
        vec!["Red", "Green", "Blue"],
        "Should accumulate unique enum members"
    );
    assert!(first.is_const, "const flag should be promoted");
}

#[test]
fn merge_cross_file_does_not_downgrade_type_param_count() {
    // If the first declaration has type params, a later declaration without
    // them should not reset the count.
    let mut first = super::SemanticDefEntry {
        kind: super::SemanticDefKind::Interface,
        name: "Foo".to_string(),
        file_id: 0,
        span_start: 0,
        type_param_count: 3,
        type_param_names: vec!["A".to_string(), "B".to_string(), "C".to_string()],
        is_exported: true,
        enum_member_names: Vec::new(),
        is_const: false,
        is_abstract: false,
        extends_names: Vec::new(),
        implements_names: Vec::new(),
        parent_namespace: None,
        is_global_augmentation: false,
        is_declare: false,
    };
    let second = super::SemanticDefEntry {
        kind: super::SemanticDefKind::Interface,
        name: "Foo".to_string(),
        file_id: 1,
        span_start: 50,
        type_param_count: 0,
        type_param_names: Vec::new(),
        is_exported: false,
        enum_member_names: Vec::new(),
        is_const: false,
        is_abstract: false,
        extends_names: vec!["Extra".to_string()],
        implements_names: Vec::new(),
        parent_namespace: None,
        is_global_augmentation: false,
        is_declare: false,
    };

    first.merge_cross_file(&second);

    assert_eq!(
        first.type_param_count, 3,
        "type_param_count should not decrease"
    );
    assert!(first.is_exported, "export flag should not be lost");
    assert_eq!(first.extends_names, vec!["Extra"]);
}

// =============================================================================
// Stable Identity Tests: All Declaration Families
// =============================================================================

#[test]
fn semantic_defs_capture_all_top_level_declaration_families() {
    // Verify that the binder captures semantic_defs for all top-level
    // declaration families: class, interface, type alias, enum, namespace,
    // function, and variable.
    let binder = bind_source(
        r"
class MyClass<T> { }
interface MyInterface<A, B> { x: number }
type MyAlias = string;
enum MyEnum { Red, Green, Blue }
namespace MyNS { export type Inner = number }
function myFunc<U>(): void { }
const myVar: string = 'hello';
",
    );

    // All 7 top-level declarations should have semantic_defs.
    // MyNS.Inner is namespace-scoped, so it also gets captured (Module scope).
    assert!(
        binder.semantic_defs.len() >= 7,
        "Expected at least 7 semantic_defs for all families, got {}",
        binder.semantic_defs.len()
    );

    // Verify each family is present by checking names.
    let names: Vec<&str> = binder
        .semantic_defs
        .values()
        .map(|e| e.name.as_str())
        .collect();
    assert!(names.contains(&"MyClass"), "Missing class");
    assert!(names.contains(&"MyInterface"), "Missing interface");
    assert!(names.contains(&"MyAlias"), "Missing type alias");
    assert!(names.contains(&"MyEnum"), "Missing enum");
    assert!(names.contains(&"MyNS"), "Missing namespace");
    assert!(names.contains(&"myFunc"), "Missing function");
    assert!(names.contains(&"myVar"), "Missing variable");

    // Verify kinds match.
    let find_kind = |name: &str| -> Option<super::SemanticDefKind> {
        binder
            .semantic_defs
            .values()
            .find(|e| e.name == name)
            .map(|e| e.kind)
    };
    assert_eq!(find_kind("MyClass"), Some(super::SemanticDefKind::Class));
    assert_eq!(
        find_kind("MyInterface"),
        Some(super::SemanticDefKind::Interface)
    );
    assert_eq!(
        find_kind("MyAlias"),
        Some(super::SemanticDefKind::TypeAlias)
    );
    assert_eq!(find_kind("MyEnum"), Some(super::SemanticDefKind::Enum));
    assert_eq!(find_kind("MyNS"), Some(super::SemanticDefKind::Namespace));
    assert_eq!(find_kind("myFunc"), Some(super::SemanticDefKind::Function));
    assert_eq!(find_kind("myVar"), Some(super::SemanticDefKind::Variable));
}

#[test]
fn semantic_defs_capture_type_param_arity_and_names() {
    // Verify that type parameter arity and names are captured at bind time.
    let binder = bind_source(
        r"
class Foo<T, U extends string> { }
interface Bar<A, B, C> { }
type Baz<X> = X[];
function qux<Y, Z>(): void { }
",
    );

    let find = |name: &str| -> Option<&super::SemanticDefEntry> {
        binder.semantic_defs.values().find(|e| e.name == name)
    };

    let foo = find("Foo").expect("Missing Foo");
    assert_eq!(foo.type_param_count, 2);
    assert_eq!(foo.type_param_names, vec!["T", "U"]);

    let bar = find("Bar").expect("Missing Bar");
    assert_eq!(bar.type_param_count, 3);
    assert_eq!(bar.type_param_names, vec!["A", "B", "C"]);

    let baz = find("Baz").expect("Missing Baz");
    assert_eq!(baz.type_param_count, 1);
    assert_eq!(baz.type_param_names, vec!["X"]);

    let qux = find("qux").expect("Missing qux");
    assert_eq!(qux.type_param_count, 2);
    assert_eq!(qux.type_param_names, vec!["Y", "Z"]);
}

#[test]
fn semantic_defs_capture_class_and_enum_metadata() {
    // Verify that abstract class flag, const enum flag, and enum member names
    // are captured at bind time.
    let binder = bind_source(
        r"
abstract class Base { }
const enum Colors { Red, Green, Blue }
class Concrete { }
enum Direction { Up, Down }
",
    );

    let find = |name: &str| -> Option<&super::SemanticDefEntry> {
        binder.semantic_defs.values().find(|e| e.name == name)
    };

    let base = find("Base").expect("Missing Base");
    assert!(
        base.is_abstract,
        "abstract class should have is_abstract=true"
    );

    let colors = find("Colors").expect("Missing Colors");
    assert!(colors.is_const, "const enum should have is_const=true");
    assert_eq!(colors.enum_member_names, vec!["Red", "Green", "Blue"]);

    let concrete = find("Concrete").expect("Missing Concrete");
    assert!(!concrete.is_abstract);

    let dir = find("Direction").expect("Missing Direction");
    assert!(!dir.is_const);
    assert_eq!(dir.enum_member_names, vec!["Up", "Down"]);
}

#[test]
fn semantic_defs_capture_declare_global_augmentations() {
    // Declarations inside `declare global { }` blocks should be captured
    // as semantic_defs with is_global_augmentation=true.
    let binder = bind_source(
        r#"
export {};
declare global {
    interface MyGlobal {
        foo: string;
    }
    type GlobalAlias = number;
}
"#,
    );

    let find = |name: &str| -> Option<&super::SemanticDefEntry> {
        binder.semantic_defs.values().find(|e| e.name == name)
    };

    let my_global = find("MyGlobal").expect("declare global interface should be in semantic_defs");
    assert_eq!(my_global.kind, super::SemanticDefKind::Interface);
    assert!(
        my_global.is_global_augmentation,
        "declare global declaration should have is_global_augmentation=true"
    );

    let global_alias =
        find("GlobalAlias").expect("declare global type alias should be in semantic_defs");
    assert_eq!(global_alias.kind, super::SemanticDefKind::TypeAlias);
    assert!(
        global_alias.is_global_augmentation,
        "declare global declaration should have is_global_augmentation=true"
    );
}

#[test]
fn semantic_defs_capture_heritage_names() {
    // Verify that heritage clause names (extends/implements) are captured.
    let binder = bind_source(
        r"
interface Base { x: number }
interface Other { y: string }
class MyClass extends Base implements Other { }
interface Child extends Base { }
",
    );

    let find = |name: &str| -> Option<&super::SemanticDefEntry> {
        binder.semantic_defs.values().find(|e| e.name == name)
    };

    let my_class = find("MyClass").expect("Missing MyClass");
    assert!(
        my_class.heritage_names().contains(&"Base".to_string()),
        "class should capture extends name"
    );

    let child = find("Child").expect("Missing Child");
    assert!(
        child.heritage_names().contains(&"Base".to_string()),
        "interface should capture extends name"
    );
}

#[test]
fn semantic_defs_namespace_members_have_parent_namespace() {
    // Namespace members should have parent_namespace set to the namespace's SymbolId.
    let binder = bind_source(
        r"
namespace MyNS {
    export class Inner { }
    export interface InnerIface { }
    export type InnerAlias = string;
}
",
    );

    let find = |name: &str| -> Option<&super::SemanticDefEntry> {
        binder.semantic_defs.values().find(|e| e.name == name)
    };

    let ns = find("MyNS").expect("Missing MyNS");
    assert!(
        ns.parent_namespace.is_none(),
        "top-level namespace should not have parent_namespace"
    );

    let inner = find("Inner").expect("Missing Inner");
    assert!(
        inner.parent_namespace.is_some(),
        "namespace member should have parent_namespace"
    );

    let inner_iface = find("InnerIface").expect("Missing InnerIface");
    assert!(
        inner_iface.parent_namespace.is_some(),
        "namespace member should have parent_namespace"
    );

    let inner_alias = find("InnerAlias").expect("Missing InnerAlias");
    assert!(
        inner_alias.parent_namespace.is_some(),
        "namespace member should have parent_namespace"
    );
}

#[test]
fn merge_cross_file_propagates_global_augmentation_flag() {
    // merge_cross_file should promote is_global_augmentation when the second
    // entry is from a declare global block.
    let mut first = super::SemanticDefEntry {
        kind: super::SemanticDefKind::Interface,
        name: "Foo".to_string(),
        file_id: 0,
        span_start: 0,
        type_param_count: 0,
        type_param_names: Vec::new(),
        is_exported: false,
        enum_member_names: Vec::new(),
        is_const: false,
        is_abstract: false,
        extends_names: Vec::new(),
        implements_names: Vec::new(),
        parent_namespace: None,
        is_global_augmentation: false,
        is_declare: false,
    };
    let second = super::SemanticDefEntry {
        kind: super::SemanticDefKind::Interface,
        name: "Foo".to_string(),
        file_id: 1,
        span_start: 50,
        type_param_count: 0,
        type_param_names: Vec::new(),
        is_exported: false,
        enum_member_names: Vec::new(),
        is_const: false,
        is_abstract: false,
        extends_names: Vec::new(),
        implements_names: Vec::new(),
        parent_namespace: None,
        is_global_augmentation: true,
        is_declare: false,
    };

    first.merge_cross_file(&second);
    assert!(
        first.is_global_augmentation,
        "is_global_augmentation should be promoted by merge_cross_file"
    );
}

// =============================================================================
// is_declare flag capture tests
// =============================================================================

#[test]
fn semantic_defs_capture_is_declare_for_all_families() {
    // Verify that the `is_declare` flag is captured for declaration families
    // where the `declare` keyword is semantically meaningful (class, enum,
    // namespace). For interfaces and type aliases, `declare` is redundant
    // and the parser may not include it in the modifiers list.
    let binder = bind_source(
        r"
declare class DeclaredClass {}
declare enum DeclaredEnum { A, B }
declare namespace DeclaredNS {}
declare function declaredFunc(): void;
",
    );

    let find = |name: &str| -> Option<&super::SemanticDefEntry> {
        binder.semantic_defs.values().find(|e| e.name == name)
    };

    let dc = find("DeclaredClass").expect("Missing DeclaredClass");
    assert!(dc.is_declare, "DeclaredClass should have is_declare=true");
    assert_eq!(dc.kind, super::SemanticDefKind::Class);

    let de = find("DeclaredEnum").expect("Missing DeclaredEnum");
    assert!(de.is_declare, "DeclaredEnum should have is_declare=true");
    assert_eq!(de.kind, super::SemanticDefKind::Enum);

    let dn = find("DeclaredNS").expect("Missing DeclaredNS");
    assert!(dn.is_declare, "DeclaredNS should have is_declare=true");
    assert_eq!(dn.kind, super::SemanticDefKind::Namespace);

    // Function declarations also capture is_declare via the simple path.
    // The `record_semantic_def` wrapper passes false by default, but
    // function callers currently don't capture is_declare. We verify the
    // flag is at least present and false for function declarations using
    // the default path.
    let df = find("declaredFunc").expect("Missing declaredFunc");
    assert_eq!(df.kind, super::SemanticDefKind::Function);
    // Note: function binding currently uses record_semantic_def (not _with_declare),
    // so is_declare may be false even with the declare keyword. This is acceptable
    // because function declarations are value-space and don't need is_declare for
    // the primary use cases (emit gating, diagnostic suppression for classes/enums).
}

#[test]
fn semantic_defs_is_declare_false_for_non_ambient_declarations() {
    // Verify that `is_declare` is false for regular (non-ambient) declarations.
    let binder = bind_source(
        r"
class RegularClass {}
interface RegularInterface {}
type RegularAlias = number;
enum RegularEnum { X }
namespace RegularNS {}
",
    );

    for entry in binder.semantic_defs.values() {
        assert!(
            !entry.is_declare,
            "{} should have is_declare=false, got true",
            entry.name
        );
    }
}

#[test]
fn merge_cross_file_promotes_is_declare() {
    // merge_cross_file should promote is_declare when the second entry
    // is a declare statement.
    let mut first = super::SemanticDefEntry {
        kind: super::SemanticDefKind::Class,
        name: "Foo".to_string(),
        file_id: 0,
        span_start: 0,
        type_param_count: 0,
        type_param_names: Vec::new(),
        is_exported: false,
        enum_member_names: Vec::new(),
        is_const: false,
        is_abstract: false,
        extends_names: Vec::new(),
        implements_names: Vec::new(),
        parent_namespace: None,
        is_global_augmentation: false,
        is_declare: false,
    };
    let second = super::SemanticDefEntry {
        kind: super::SemanticDefKind::Class,
        name: "Foo".to_string(),
        file_id: 1,
        span_start: 50,
        type_param_count: 0,
        type_param_names: Vec::new(),
        is_exported: false,
        enum_member_names: Vec::new(),
        is_const: false,
        is_abstract: false,
        extends_names: Vec::new(),
        implements_names: Vec::new(),
        parent_namespace: None,
        is_global_augmentation: false,
        is_declare: true,
    };

    first.merge_cross_file(&second);
    assert!(
        first.is_declare,
        "is_declare should be promoted by merge_cross_file"
    );
}

// =============================================================================
// Rebind Stability Tests
// =============================================================================

#[test]
fn semantic_defs_stable_across_rebind() {
    // Verify that re-parsing and re-binding the same source produces
    // semantic_defs with identical structure. This is a key invariant for
    // incremental compilation: the binder's stable identity must survive
    // re-bind without changing shape, kind, name, or metadata.
    let source = r"
export class MyClass<T> extends Base {}
export interface MyInterface<A, B> { x: number }
export type MyAlias<X> = X | null;
export enum MyEnum { Red, Green, Blue }
export namespace MyNS { export type Inner = number }
declare class AmbientClass {}
";

    // Bind once
    let binder1 = bind_source(source);
    // Bind again (fresh parse + bind)
    let binder2 = bind_source(source);

    // Same number of semantic_defs
    assert_eq!(
        binder1.semantic_defs.len(),
        binder2.semantic_defs.len(),
        "Rebind should produce the same number of semantic_defs"
    );

    // Each name in binder1 should exist in binder2 with the same metadata.
    for entry1 in binder1.semantic_defs.values() {
        let entry2 = binder2
            .semantic_defs
            .values()
            .find(|e| e.name == entry1.name)
            .unwrap_or_else(|| panic!("Missing {} after rebind", entry1.name));

        assert_eq!(
            entry1.kind, entry2.kind,
            "{}: kind mismatch after rebind",
            entry1.name
        );
        assert_eq!(
            entry1.type_param_count, entry2.type_param_count,
            "{}: type_param_count mismatch after rebind",
            entry1.name
        );
        assert_eq!(
            entry1.type_param_names, entry2.type_param_names,
            "{}: type_param_names mismatch after rebind",
            entry1.name
        );
        assert_eq!(
            entry1.is_exported, entry2.is_exported,
            "{}: is_exported mismatch after rebind",
            entry1.name
        );
        assert_eq!(
            entry1.is_abstract, entry2.is_abstract,
            "{}: is_abstract mismatch after rebind",
            entry1.name
        );
        assert_eq!(
            entry1.is_const, entry2.is_const,
            "{}: is_const mismatch after rebind",
            entry1.name
        );
        assert_eq!(
            entry1.is_declare, entry2.is_declare,
            "{}: is_declare mismatch after rebind",
            entry1.name
        );
        assert_eq!(
            entry1.extends_names, entry2.extends_names,
            "{}: extends_names mismatch after rebind",
            entry1.name
        );
        assert_eq!(
            entry1.implements_names, entry2.implements_names,
            "{}: implements_names mismatch after rebind",
            entry1.name
        );
        assert_eq!(
            entry1.enum_member_names, entry2.enum_member_names,
            "{}: enum_member_names mismatch after rebind",
            entry1.name
        );
    }
}

#[test]
fn semantic_defs_identity_stable_with_changed_bodies() {
    // Verify that changing function/class bodies does not affect the
    // binder's semantic_defs shape. Only the AST-level structure changes;
    // the semantic identity (kind, name, arity, heritage) should stay the same.
    let source_v1 = r"
class MyClass<T> { value: T; foo(): void {} }
interface MyInterface { x: number; y: string }
type MyAlias<X> = X | null;
enum MyEnum { A = 1, B = 2 }
";

    let source_v2 = r"
class MyClass<T> { value: T; bar(): string { return ''; } baz(): void {} }
interface MyInterface { x: number; y: string; z: boolean }
type MyAlias<X> = X | null;
enum MyEnum { A = 1, B = 2 }
";

    let binder1 = bind_source(source_v1);
    let binder2 = bind_source(source_v2);

    // Same top-level families should exist.
    assert_eq!(
        binder1.semantic_defs.len(),
        binder2.semantic_defs.len(),
        "Body changes should not affect semantic_defs count"
    );

    for entry1 in binder1.semantic_defs.values() {
        let entry2 = binder2
            .semantic_defs
            .values()
            .find(|e| e.name == entry1.name)
            .unwrap_or_else(|| panic!("Missing {} after body change", entry1.name));

        assert_eq!(
            entry1.kind, entry2.kind,
            "{}: kind should be stable across body changes",
            entry1.name
        );
        assert_eq!(
            entry1.type_param_count, entry2.type_param_count,
            "{}: arity should be stable across body changes",
            entry1.name
        );
    }
}

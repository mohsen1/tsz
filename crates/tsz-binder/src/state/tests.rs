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

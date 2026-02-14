//! Tests for symbol resolution behavior in the checker.

use crate::checker::context::CheckerOptions;
use crate::checker::state::CheckerState;
use tsz_binder::BinderState;
use tsz_parser::parser::ParserState;
use tsz_solver::TypeInterner;

use crate::test_fixtures::{merge_shared_lib_symbols, setup_lib_contexts};

fn collect_diagnostics(source: &str) -> Vec<crate::checker::diagnostics::Diagnostic> {
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.ts".to_string(),
        CheckerOptions::default(),
    );

    // Enable TS2304 emission for unresolved names
    checker.ctx.report_unresolved_imports = true;

    checker.check_source_file(root);
    checker.ctx.diagnostics.clone()
}

fn collect_diagnostics_with_libs(source: &str) -> Vec<crate::checker::diagnostics::Diagnostic> {
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut binder = BinderState::new();
    merge_shared_lib_symbols(&mut binder);
    binder.bind_source_file(parser.get_arena(), root);

    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.ts".to_string(),
        CheckerOptions::default(),
    );
    setup_lib_contexts(&mut checker);

    // Enable TS2304 emission for unresolved names
    checker.ctx.report_unresolved_imports = true;

    checker.check_source_file(root);
    checker.ctx.diagnostics.clone()
}

#[test]
fn test_symbol_resolution_value_shadow_does_not_block_type_lookup() {
    let source = r#"
type Foo = { a: number };
function f() {
    const Foo = 123;
    let x: Foo;
    return x;
}
"#;

    let diagnostics = collect_diagnostics_with_libs(source);
    let value_as_type = diagnostics.iter().filter(|d| d.code == 2749).count();
    let cannot_find = diagnostics.iter().filter(|d| d.code == 2304).count();

    assert_eq!(
        value_as_type, 0,
        "Expected no TS2749 for type lookup through value shadowing, got: {:?}",
        diagnostics
    );
    assert_eq!(
        cannot_find, 0,
        "Expected no TS2304 for Foo in type position, got: {:?}",
        diagnostics
    );
}

#[test]
fn test_symbol_resolution_class_member_not_resolved_as_value() {
    let source = r#"
class C {
    foo: number;
    method() {
        foo;
    }
}
"#;

    let diagnostics = collect_diagnostics(source);
    // Filter out TS2318 (Cannot find global type) which is expected when no lib files are loaded
    let filtered: Vec<_> = diagnostics.iter().filter(|d| d.code != 2318).collect();
    let ts2304_count = filtered.iter().filter(|d| d.code == 2304).count();

    assert!(
        ts2304_count >= 1,
        "Expected TS2304 for unqualified class member reference, got: {:?}",
        filtered
    );
}

#[test]
fn test_symbol_resolution_type_params_in_nested_scopes() {
    let source = r#"
function outer<T>() {
    function inner<U>() {
        let a: T;
        let b: U;
        return [a, b];
    }
}
"#;

    let diagnostics = collect_diagnostics(source);
    let type_param_errors = diagnostics.iter().filter(|d| d.code == 2749).count();
    let cannot_find = diagnostics.iter().filter(|d| d.code == 2304).count();

    assert_eq!(
        type_param_errors, 0,
        "Expected no TS2749 for nested type parameters, got: {:?}",
        diagnostics
    );
    assert_eq!(
        cannot_find, 0,
        "Expected no TS2304 for nested type parameters, got: {:?}",
        diagnostics
    );
}

#[test]
fn test_symbol_resolution_global_console_with_libs() {
    let diagnostics = collect_diagnostics_with_libs(r#"console.log("ok");"#);
    let ts2304_count = diagnostics.iter().filter(|d| d.code == 2304).count();
    let ts2584_count = diagnostics.iter().filter(|d| d.code == 2584).count();

    // console is a DOM global, not an ES5 global, so TS2584 is expected
    // when only ES5 lib is loaded
    assert_eq!(
        ts2304_count, 0,
        "Expected no TS2304 for console with lib files, got: {:?}",
        diagnostics
    );
    assert_eq!(
        ts2584_count, 1,
        "Expected TS2584 for console (DOM global) with ES5 lib only, got: {:?}",
        diagnostics
    );
}

#[test]
fn test_symbol_resolution_parameter_in_nested_function() {
    let source = r#"
function outer(x: number) {
    function inner() {
        const y = x + 1;
        return y;
    }
    return inner();
}
"#;

    let diagnostics = collect_diagnostics(source);
    let ts2304_count = diagnostics.iter().filter(|d| d.code == 2304).count();

    assert_eq!(
        ts2304_count, 0,
        "Expected no TS2304 for outer parameter in nested function, got: {:?}",
        diagnostics
    );
}

#[test]
fn test_symbol_resolution_block_shadowing_is_scoped() {
    let source = r#"
let x = 1;
{
    let x = 2;
    x;
}
x;
"#;

    let diagnostics = collect_diagnostics(source);
    let ts2304_count = diagnostics.iter().filter(|d| d.code == 2304).count();

    assert_eq!(
        ts2304_count, 0,
        "Expected no TS2304 for block-scoped shadowing, got: {:?}",
        diagnostics
    );
}

#[test]
fn test_symbol_resolution_namespace_type_and_value() {
    let source = r#"
namespace N {
    export interface I { a: number; }
    export const value = 1;
}
let x: N.I;
let y = N.value;
"#;

    let diagnostics = collect_diagnostics(source);
    let ts2304_count = diagnostics.iter().filter(|d| d.code == 2304).count();
    let ts2749_count = diagnostics.iter().filter(|d| d.code == 2749).count();

    assert_eq!(
        ts2304_count, 0,
        "Expected no TS2304 for namespace members, got: {:?}",
        diagnostics
    );
    assert_eq!(
        ts2749_count, 0,
        "Expected no TS2749 for namespace type usage, got: {:?}",
        diagnostics
    );
}

#[test]
fn test_symbol_resolution_namespace_used_as_type_errors() {
    let source = r#"
namespace A {
    export const x = 1;
}
let a: A;
"#;

    let diagnostics = collect_diagnostics(source);
    let ts2709_count = diagnostics.iter().filter(|d| d.code == 2709).count();

    assert_eq!(
        ts2709_count, 1,
        "Expected TS2709 for namespace used as type, got: {:?}",
        diagnostics
    );
}

#[test]
fn test_symbol_resolution_namespace_interface_type_args_error() {
    let source = r#"
namespace X {
    export namespace Y {
        export interface Z { }
    }
    export interface Y { }
}
let z2: X.Y<string>;
"#;

    let diagnostics = collect_diagnostics(source);
    let ts2315_count = diagnostics.iter().filter(|d| d.code == 2315).count();

    assert_eq!(
        ts2315_count, 1,
        "Expected TS2315 for non-generic namespace interface, got: {:?}",
        diagnostics
    );
}

#[test]
fn test_symbol_resolution_namespace_interface_generic_merge() {
    let source = r#"
namespace X {
    export namespace Y {
        export interface Z { }
    }
    export interface Y<T> { }
}
var z: X.Y.Z = null;
var z2: X.Y<string>;
"#;

    let diagnostics = collect_diagnostics(source);
    let ts2315_count = diagnostics.iter().filter(|d| d.code == 2315).count();
    let ts2304_count = diagnostics.iter().filter(|d| d.code == 2304).count();

    assert_eq!(
        ts2315_count, 0,
        "Expected no TS2315 for merged namespace/interface, got: {:?}",
        diagnostics
    );
    assert_eq!(
        ts2304_count, 0,
        "Expected no TS2304 for merged namespace/interface, got: {:?}",
        diagnostics
    );
}

#[test]
fn test_namespace_exports_table_excludes_non_exported_members() {
    let source = r#"
namespace M {
    export class A {}
    class B {}
}
"#;
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
fn test_namespace_exports_include_interface_with_same_name_as_namespace() {
    let source = r#"
namespace X {
    export namespace Y {
        export interface Z { }
    }
    export interface Y { }
}
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    let x_sym_id = binder
        .file_locals
        .get("X")
        .expect("expected namespace symbol for X");
    let x_symbol = binder
        .symbols
        .get(x_sym_id)
        .expect("expected namespace symbol data");
    let exports = x_symbol.exports.as_ref().expect("expected exports table");

    assert!(exports.has("Y"), "expected Y to be exported");
}

#[test]
fn test_namespace_non_exported_class_has_no_export_modifier() {
    use tsz_scanner::SyntaxKind;

    let source = r#"
namespace M {
    export class A {}
    class B {}
}
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let _root = parser.parse_source_file();

    let arena = parser.get_arena();
    let mut found_b = false;
    let mut b_has_export = false;

    for node in arena.nodes.iter() {
        if node.kind == tsz_parser::parser::syntax_kind_ext::CLASS_DECLARATION {
            if let Some(class) = arena.get_class(node)
                && let Some(name_node) = arena.get(class.name)
                && let Some(ident) = arena.get_identifier(name_node)
                && ident.escaped_text == "B"
            {
                found_b = true;
                if let Some(mods) = &class.modifiers {
                    for &mod_idx in &mods.nodes {
                        if let Some(mod_node) = arena.get(mod_idx)
                            && mod_node.kind == SyntaxKind::ExportKeyword as u16
                        {
                            b_has_export = true;
                            break;
                        }
                    }
                }
            }
        }
    }

    assert!(found_b, "expected to find class B");
    assert!(!b_has_export, "class B should not be exported");
}

#[test]
fn test_array_type_uses_qualified_name_for_namespace_member() {
    let source = r#"
namespace M {
    export class A {}
    class B {}
}
var t2: M.B[] = [];
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let _root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut found = false;
    for node in arena.nodes.iter() {
        if let Some(type_ref) = arena.get_type_ref(node) {
            if let Some(name_node) = arena.get(type_ref.type_name)
                && name_node.kind == tsz_parser::parser::syntax_kind_ext::QUALIFIED_NAME
                && let Some(qn) = arena.get_qualified_name(name_node)
                && let Some(left_node) = arena.get(qn.left)
                && let Some(right_node) = arena.get(qn.right)
                && let Some(left_ident) = arena.get_identifier(left_node)
                && let Some(right_ident) = arena.get_identifier(right_node)
                && left_ident.escaped_text == "M"
                && right_ident.escaped_text == "B"
            {
                found = true;
                break;
            }
        }
    }

    assert!(
        found,
        "expected type reference for M.B[] to use qualified name"
    );
}

#[test]
fn test_symbol_resolution_interface_value_property_access_errors() {
    let source = r#"
namespace Foo2 {
    namespace Bar {
        export var x = 42;
    }

    export interface Bar {
        y: string;
    }
}

var z2 = Foo2.Bar.y;
"#;

    let diagnostics = collect_diagnostics(source);
    let ts2339_count = diagnostics.iter().filter(|d| d.code == 2339).count();

    assert_eq!(
        ts2339_count, 1,
        "Expected TS2339 for interface used as value property access, got: {:?}",
        diagnostics
    );
}

#[test]
fn test_symbol_resolution_namespace_as_base_type_errors() {
    let source = r#"
namespace M {}
class C extends M {}
interface I extends M { }
class C2 implements M { }
"#;

    let diagnostics = collect_diagnostics(source);
    let ts2708_count = diagnostics.iter().filter(|d| d.code == 2708).count();
    let ts2709_count = diagnostics.iter().filter(|d| d.code == 2709).count();

    assert!(
        ts2708_count >= 1 && ts2709_count >= 1,
        "Expected TS2708 and TS2709 for namespace as base type, got: {:?}",
        diagnostics
    );
}

#[test]
fn test_symbol_resolution_nested_namespace_qualified_type() {
    let source = r#"
namespace Outer {
    export namespace Inner {
        export interface I { a: number; }
    }
}
let x: Outer.Inner.I;
"#;

    let diagnostics = collect_diagnostics(source);
    let ts2304_count = diagnostics.iter().filter(|d| d.code == 2304).count();
    let ts2749_count = diagnostics.iter().filter(|d| d.code == 2749).count();

    assert_eq!(
        ts2304_count, 0,
        "Expected no TS2304 for nested namespace type, got: {:?}",
        diagnostics
    );
    assert_eq!(
        ts2749_count, 0,
        "Expected no TS2749 for nested namespace type, got: {:?}",
        diagnostics
    );
}

#[test]
fn test_symbol_resolution_type_alias_in_block_scope() {
    let source = r#"
function f() {
    {
        type T = { a: number };
        let x: T;
        return x;
    }
}
"#;

    let diagnostics = collect_diagnostics(source);
    let ts2304_count = diagnostics.iter().filter(|d| d.code == 2304).count();
    let ts2749_count = diagnostics.iter().filter(|d| d.code == 2749).count();

    assert_eq!(
        ts2304_count, 0,
        "Expected no TS2304 for block-scoped type alias, got: {:?}",
        diagnostics
    );
    assert_eq!(
        ts2749_count, 0,
        "Expected no TS2749 for block-scoped type alias, got: {:?}",
        diagnostics
    );
}

#[test]
fn test_symbol_resolution_global_array_with_libs() {
    let diagnostics = collect_diagnostics_with_libs("let xs: Array<string> = [];");
    let ts2304_count = diagnostics.iter().filter(|d| d.code == 2304).count();
    let ts2318_count = diagnostics.iter().filter(|d| d.code == 2318).count();

    assert_eq!(
        ts2304_count, 0,
        "Expected no TS2304 for Array with lib files, got: {:?}",
        diagnostics
    );
    assert_eq!(
        ts2318_count, 0,
        "Expected no TS2318 for Array with lib files, got: {:?}",
        diagnostics
    );
}

#[test]
fn test_symbol_resolution_type_param_shadowing() {
    let source = r#"
function outer<T>() {
    function inner<T>() {
        let x: T;
        return x;
    }
}
"#;

    let diagnostics = collect_diagnostics(source);
    let ts2304_count = diagnostics.iter().filter(|d| d.code == 2304).count();
    let ts2749_count = diagnostics.iter().filter(|d| d.code == 2749).count();

    assert_eq!(
        ts2304_count, 0,
        "Expected no TS2304 for shadowed type parameters, got: {:?}",
        diagnostics
    );
    assert_eq!(
        ts2749_count, 0,
        "Expected no TS2749 for shadowed type parameters, got: {:?}",
        diagnostics
    );
}

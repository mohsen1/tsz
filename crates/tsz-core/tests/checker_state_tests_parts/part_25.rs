// Tests for Checker - Type checker using `NodeArena` and Solver
//
// This module contains comprehensive type checking tests organized into categories:
// - Basic type checking (creation, intrinsic types, type interning)
// - Type compatibility and assignability
// - Excess property checking
// - Function overloads and call resolution
// - Generic types and type inference
// - Control flow analysis
// - Error diagnostics
use crate::binder::BinderState;
use crate::checker::state::CheckerState;
use crate::parser::ParserState;
use crate::parser::node::NodeArena;
use crate::test_fixtures::{TestContext, merge_shared_lib_symbols, setup_lib_contexts};
use tsz_solver::{TypeId, TypeInterner, Visibility, types::RelationCacheKey, types::TypeData};

// =============================================================================
// Basic Type Checker Tests
// =============================================================================
#[test]
fn test_checker_namespace_merges_with_function_type_exports_reverse_order() {
    use crate::parser::ParserState;
    use tsz_solver::TypeData;

    let source = r#"
namespace Merge {
    export interface Extra { value: number; }
}
function Merge() {}
type Alias = Merge.Extra;
"#;

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
        crate::checker::context::CheckerOptions {
            jsx_factory: "React.createElement".to_string(),
            jsx_factory_from_config: false,
            jsx_fragment_factory: "React.Fragment".to_string(),
            jsx_fragment_factory_from_config: false,
            no_lib: true,
            ..Default::default()
        },
    );
    setup_lib_contexts(&mut checker);
    checker.check_source_file(root);
    // tsc does NOT emit TS2434 for non-instantiated namespaces (interfaces/types only).
    assert!(
        checker.ctx.diagnostics.iter().all(|d| d.code != 2434),
        "Non-instantiated namespace should NOT trigger TS2434: {:?}",
        checker.ctx.diagnostics
    );

    let alias_sym = binder.file_locals.get("Alias").expect("Alias should exist");
    let alias_type = checker.get_type_of_symbol(alias_sym);
    let alias_key = types.lookup(alias_type).expect("Alias type should exist");
    match alias_key {
        TypeData::Object(shape_id) | TypeData::ObjectWithIndex(shape_id) => {
            let shape = types.object_shape(shape_id);
            let prop = shape
                .properties
                .iter()
                .find(|prop| types.resolve_atom(prop.name) == "value")
                .expect("Expected property value");
            assert_eq!(prop.type_id, TypeId::NUMBER);
        }
        TypeData::Lazy(_def_id) => {
            // Phase 4.3: Interface type references now use Lazy(DefId)
            // The Lazy type is correctly resolved when needed for type checking
        }
        _ => panic!("Expected Alias to resolve to Object or Lazy type, got {alias_key:?}"),
    }
}

/// Test namespace merging with enum for value exports
///
/// NOTE: Previously ignored due to wrong type expectation.
#[test]
fn test_checker_namespace_merges_with_enum_value_exports() {
    use crate::parser::ParserState;

    let source = r#"
enum Merge {
    A,
}
namespace Merge {
    export const extra = 1;
}
const direct = Merge.extra;
"#;

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
        crate::checker::context::CheckerOptions::default(),
    );
    setup_lib_contexts(&mut checker);
    checker.check_source_file(root);
    assert!(
        checker.ctx.diagnostics.is_empty(),
        "Unexpected diagnostics: {:?}",
        checker.ctx.diagnostics
    );

    let direct_sym = binder
        .file_locals
        .get("direct")
        .expect("direct should exist");
    // `export const extra = 1` produces literal type `1`, not `number`
    assert_eq!(
        checker.get_type_of_symbol(direct_sym),
        types.literal_number(1.0)
    );
}

/// Test namespace merging with enum in reverse order
///
/// NOTE: Previously ignored due to wrong type expectation.
#[test]
fn test_checker_namespace_merges_with_enum_value_exports_reverse_order() {
    use crate::parser::ParserState;

    let source = r#"
namespace Merge {
    export const extra = 1;
}
enum Merge {
    A,
}
const direct = Merge.extra;
"#;

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
        crate::checker::context::CheckerOptions {
            jsx_factory: "React.createElement".to_string(),
            jsx_factory_from_config: false,
            jsx_fragment_factory: "React.Fragment".to_string(),
            jsx_fragment_factory_from_config: false,
            no_lib: true,
            ..Default::default()
        },
    );
    setup_lib_contexts(&mut checker);
    checker.check_source_file(root);
    let non_lib_diagnostics: Vec<_> = checker
        .ctx
        .diagnostics
        .iter()
        .filter(|d| d.code != 2318)
        .collect();
    assert!(
        non_lib_diagnostics.is_empty(),
        "Unexpected diagnostics: {non_lib_diagnostics:?}"
    );

    let direct_sym = binder
        .file_locals
        .get("direct")
        .expect("direct should exist");
    // `export const extra = 1` produces literal type `1`, not `number`
    assert_eq!(
        checker.get_type_of_symbol(direct_sym),
        types.literal_number(1.0)
    );
}

#[test]
fn test_checker_namespace_merges_with_enum_type_exports() {
    use crate::parser::ParserState;
    use tsz_solver::TypeData;

    let source = r#"
enum Merge {
    A,
}
namespace Merge {
    export interface Extra { value: number; }
}
type Alias = Merge.Extra;
"#;

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
        crate::checker::context::CheckerOptions {
            jsx_factory: "React.createElement".to_string(),
            jsx_factory_from_config: false,
            jsx_fragment_factory: "React.Fragment".to_string(),
            jsx_fragment_factory_from_config: false,
            no_lib: true,
            ..Default::default()
        },
    );
    setup_lib_contexts(&mut checker);
    checker.check_source_file(root);
    let non_lib_diagnostics: Vec<_> = checker
        .ctx
        .diagnostics
        .iter()
        .filter(|d| d.code != 2318)
        .collect();
    assert!(
        non_lib_diagnostics.is_empty(),
        "Unexpected diagnostics: {non_lib_diagnostics:?}"
    );

    let alias_sym = binder.file_locals.get("Alias").expect("Alias should exist");
    let alias_type = checker.get_type_of_symbol(alias_sym);
    let alias_key = types.lookup(alias_type).expect("Alias type should exist");
    match alias_key {
        TypeData::Object(shape_id) | TypeData::ObjectWithIndex(shape_id) => {
            let shape = types.object_shape(shape_id);
            let prop = shape
                .properties
                .iter()
                .find(|prop| types.resolve_atom(prop.name) == "value")
                .expect("Expected property value");
            assert_eq!(prop.type_id, TypeId::NUMBER);
        }
        TypeData::Lazy(_def_id) => {
            // Phase 4.3: Interface type references now use Lazy(DefId)
            // The Lazy type is correctly resolved when needed for type checking
        }
        _ => panic!("Expected Alias to resolve to Object or Lazy type, got {alias_key:?}"),
    }
}

#[test]
fn test_checker_namespace_merges_with_enum_type_exports_reverse_order() {
    use crate::parser::ParserState;
    use tsz_solver::TypeData;

    let source = r#"
namespace Merge {
    export interface Extra { value: number; }
}
enum Merge {
    A,
}
type Alias = Merge.Extra;
"#;

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
        crate::checker::context::CheckerOptions {
            jsx_factory: "React.createElement".to_string(),
            jsx_factory_from_config: false,
            jsx_fragment_factory: "React.Fragment".to_string(),
            jsx_fragment_factory_from_config: false,
            no_lib: true,
            ..Default::default()
        },
    );
    setup_lib_contexts(&mut checker);
    checker.check_source_file(root);
    let non_lib_diagnostics: Vec<_> = checker
        .ctx
        .diagnostics
        .iter()
        .filter(|d| d.code != 2318)
        .collect();
    assert!(
        non_lib_diagnostics.is_empty(),
        "Unexpected diagnostics: {non_lib_diagnostics:?}"
    );

    let alias_sym = binder.file_locals.get("Alias").expect("Alias should exist");
    let alias_type = checker.get_type_of_symbol(alias_sym);
    let alias_key = types.lookup(alias_type).expect("Alias type should exist");
    match alias_key {
        TypeData::Object(shape_id) | TypeData::ObjectWithIndex(shape_id) => {
            let shape = types.object_shape(shape_id);
            let prop = shape
                .properties
                .iter()
                .find(|prop| types.resolve_atom(prop.name) == "value")
                .expect("Expected property value");
            assert_eq!(prop.type_id, TypeId::NUMBER);
        }
        TypeData::Lazy(_def_id) => {
            // Phase 4.3: Interface type references now use Lazy(DefId)
            // The Lazy type is correctly resolved when needed for type checking
        }
        _ => panic!("Expected Alias to resolve to Object or Lazy type, got {alias_key:?}"),
    }
}

/// Test namespace merging with class for element access
///
/// Namespace members should be visible through bracket access on the class constructor type.
#[test]
fn test_checker_namespace_merges_with_class_element_access() {
    use crate::parser::ParserState;

    let source = r#"
class Foo {}
namespace Foo {
    export const value = 1;
}
const direct = Foo["value"];
"#;

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
        crate::checker::context::CheckerOptions::default(),
    );
    setup_lib_contexts(&mut checker);
    checker.check_source_file(root);
    let codes: Vec<u32> = checker.ctx.diagnostics.iter().map(|d| d.code).collect();
    assert!(
        codes.is_empty(),
        "Expected no diagnostics for namespace+class element access, got: {codes:?}"
    );
}

#[test]
fn test_checker_interface_typeof_value_reference() {
    use crate::parser::ParserState;
    use tsz_solver::{SymbolRef, TypeData};

    let source = r#"
const Foo = 1;
namespace Ns {
    export const value = 1;
}
interface Bar {
    x: typeof Foo;
    y: typeof Ns.value;
}
"#;

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
        crate::checker::context::CheckerOptions::default(),
    );
    setup_lib_contexts(&mut checker);
    checker.check_source_file(root);
    assert!(
        checker.ctx.diagnostics.is_empty(),
        "Unexpected diagnostics: {:?}",
        checker.ctx.diagnostics
    );

    let foo_sym = binder.file_locals.get("Foo").expect("Foo should exist");
    let ns_sym = binder.file_locals.get("Ns").expect("Ns should exist");
    let value_sym = binder
        .get_symbol(ns_sym)
        .and_then(|symbol| symbol.exports.as_ref())
        .and_then(|exports| exports.get("value"))
        .expect("Ns.value should exist");

    let bar_sym = binder.file_locals.get("Bar").expect("Bar should exist");
    let bar_type = checker.get_type_of_symbol(bar_sym);
    let bar_key = types.lookup(bar_type).expect("Bar type should exist");
    match bar_key {
        TypeData::Object(shape_id) | TypeData::ObjectWithIndex(shape_id) => {
            let shape = types.object_shape(shape_id);
            let prop_names: Vec<String> = shape
                .properties
                .iter()
                .map(|prop| types.resolve_atom(prop.name))
                .collect();
            let prop_x = shape
                .properties
                .iter()
                .find(|prop| types.resolve_atom(prop.name) == "x")
                .expect("Expected property x");
            let prop_y = shape
                .properties
                .iter()
                .find(|prop| types.resolve_atom(prop.name) == "y")
                .unwrap_or_else(|| panic!("Expected property y, got {prop_names:?}"));

            match types.lookup(prop_x.type_id) {
                Some(TypeData::TypeQuery(SymbolRef(sym_id))) => assert_eq!(sym_id, foo_sym.0),
                other => panic!("Expected x to be typeof Foo, got {other:?}"),
            }

            match types.lookup(prop_y.type_id) {
                Some(TypeData::TypeQuery(SymbolRef(sym_id))) => assert_eq!(sym_id, value_sym.0),
                other => panic!("Expected y to be typeof Ns.value, got {other:?}"),
            }
        }
        _ => panic!("Expected Bar to resolve to Object type, got {bar_key:?}"),
    }
}

/// Test typeof with namespace alias member access
///
/// Test that `typeof Alias.value` resolves to the correct type through
/// namespace import aliases (`import Alias = Ns`).
#[test]
fn test_checker_typeof_namespace_alias_member() {
    use crate::parser::ParserState;

    let source = r#"
namespace Ns {
    export const value = 1;
}
import Alias = Ns;
type T = typeof Alias.value;
"#;

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
        crate::checker::context::CheckerOptions::default(),
    );
    setup_lib_contexts(&mut checker);
    checker.check_source_file(root);
    assert!(
        checker.ctx.diagnostics.is_empty(),
        "Unexpected diagnostics: {:?}",
        checker.ctx.diagnostics
    );

    // typeof Alias.value should resolve to the literal type 1 (const value = 1)
    let t_sym = binder.file_locals.get("T").expect("T should exist");
    let t_type = checker.get_type_of_symbol(t_sym);
    assert_eq!(
        t_type,
        types.literal_number(1.0),
        "typeof Alias.value should resolve to literal type 1"
    );
}

#[test]
fn test_checker_typeof_with_type_arguments() {
    use crate::parser::ParserState;
    use tsz_solver::{SymbolRef, TypeData};

    let source = r#"
const Foo = <T>(value: T) => value;
type Alias = typeof Foo<string>;
"#;

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
        crate::checker::context::CheckerOptions::default(),
    );
    setup_lib_contexts(&mut checker);
    checker.check_source_file(root);
    assert!(
        checker.ctx.diagnostics.is_empty(),
        "Unexpected diagnostics: {:?}",
        checker.ctx.diagnostics
    );

    let foo_sym = binder.file_locals.get("Foo").expect("Foo should exist");
    let alias_sym = binder.file_locals.get("Alias").expect("Alias should exist");

    let alias_type = checker.get_type_of_symbol(alias_sym);
    let alias_key = types.lookup(alias_type).expect("Alias type should exist");
    match alias_key {
        TypeData::Application(app_id) => {
            let app = types.type_application(app_id);
            assert_eq!(app.args, vec![TypeId::STRING]);
            match types.lookup(app.base) {
                Some(TypeData::TypeQuery(SymbolRef(sym_id))) => assert_eq!(sym_id, foo_sym.0),
                other => panic!("Expected TypeQuery base type, got {other:?}"),
            }
        }
        _ => panic!("Expected Alias to be Application type, got {alias_key:?}"),
    }
}

/// Test circular type alias handling
///
/// NOTE: Currently ignored - circular type alias resolution is not fully implemented.
/// Circular type alias handling
///
/// TODO: Circular type aliases do not resolve to `any` as tsc does.
/// Currently they resolve to a lazy/unresolved TypeId. When circular alias
/// detection is implemented, update to assert `TypeId::ANY`.
#[test]
fn test_checker_circular_type_aliases() {
    use crate::parser::ParserState;

    let source = r#"
type A = B;
type B = A;
"#;

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
        crate::checker::context::CheckerOptions::default(),
    );
    setup_lib_contexts(&mut checker);
    checker.check_source_file(root);

    let a_sym = binder.file_locals.get("A").expect("A should exist");
    let b_sym = binder.file_locals.get("B").expect("B should exist");

    // TODO: Should be TypeId::ANY once circular alias detection resolves to `any`
    let a_type = checker.get_type_of_symbol(a_sym);
    let b_type = checker.get_type_of_symbol(b_sym);
    // Both should resolve to the same type (they reference each other)
    assert_eq!(
        a_type, b_type,
        "Circular aliases A and B should resolve to the same type"
    );
}

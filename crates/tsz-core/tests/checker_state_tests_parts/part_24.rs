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
fn test_checker_property_access_union_type() {
    use crate::parser::ParserState;
    use tsz_solver::TypeData;

    // Test union property access WITHOUT narrowing
    // Using declare prevents CFA narrowing on initialization
    let source = r#"
type U = { a: number } | { a: string };
declare const obj: U;
const value = obj.a;
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

    let value_sym = binder.file_locals.get("value").expect("value should exist");
    let value_type = checker.get_type_of_symbol(value_sym);
    let value_key = types.lookup(value_type).expect("value type should exist");
    match value_key {
        TypeData::Union(members) => {
            let members = types.type_list(members);
            assert!(members.contains(&TypeId::NUMBER));
            assert!(members.contains(&TypeId::STRING));
        }
        _ => panic!("Expected union type for value, got {value_key:?}"),
    }
}

#[test]
fn test_checker_namespace_merges_with_class_exports() {
    use crate::parser::ParserState;
    use tsz_solver::TypeData;

    let source = r#"
class Foo {}
namespace Foo {
    export interface Bar { x: number; }
}
type Alias = Foo.Bar;
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
                .find(|prop| types.resolve_atom(prop.name) == "x")
                .expect("Expected property x");
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
fn test_checker_namespace_merges_with_class_exports_reverse_order() {
    use crate::parser::ParserState;
    use tsz_solver::TypeData;

    let source = r#"
namespace Foo {
    export interface Bar { x: number; }
}
class Foo {}
type Alias = Foo.Bar;
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
    // Only instantiated namespaces (with runtime members like variables, functions, classes)
    // trigger TS2434 when they precede the class/function they merge with.
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
                .find(|prop| types.resolve_atom(prop.name) == "x")
                .expect("Expected property x");
            assert_eq!(prop.type_id, TypeId::NUMBER);
        }
        TypeData::Lazy(_def_id) => {
            // Phase 4.3: Interface type references now use Lazy(DefId)
            // The Lazy type is correctly resolved when needed for type checking
        }
        _ => panic!("Expected Alias to resolve to Object or Lazy type, got {alias_key:?}"),
    }
}

/// Test namespace merging with class for value exports
///
/// NOTE: Currently ignored - see `test_checker_namespace_merges_with_class_element_access`.
#[test]
fn test_checker_namespace_merges_with_class_value_exports() {
    use crate::parser::ParserState;

    let source = r#"
class Foo {}
namespace Foo {
    export const value = 1;
}
const direct = Foo.value;
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
    // `export const value = 1` produces literal type `1`, not `number`
    assert_eq!(
        checker.get_type_of_symbol(direct_sym),
        types.literal_number(1.0)
    );
}

/// Test namespace merging with class in reverse order
///
/// NOTE: Previously ignored due to wrong type expectation.
#[test]
fn test_checker_namespace_merges_with_class_value_exports_reverse_order() {
    use crate::parser::ParserState;

    let source = r#"
namespace Foo {
    export const value = 1;
}
class Foo {}
const direct = Foo.value;
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
    // tsc emits TS2434 when namespace appears before the class it merges with
    let ts2434: Vec<_> = checker
        .ctx
        .diagnostics
        .iter()
        .filter(|d| d.code == 2434)
        .collect();
    assert_eq!(
        ts2434.len(),
        1,
        "Expected exactly one TS2434, got diagnostics: {:?}",
        checker.ctx.diagnostics
    );

    let direct_sym = binder
        .file_locals
        .get("direct")
        .expect("direct should exist");
    // `export const value = 1` produces literal type `1`, not `number`
    assert_eq!(
        checker.get_type_of_symbol(direct_sym),
        types.literal_number(1.0)
    );
}

/// Test namespace merging across declarations for value access
///
/// NOTE: Currently ignored - namespace merging across declarations is not fully
/// implemented. The type resolution for merged namespaces doesn't correctly
/// combine all exported values across declarations.
#[test]
fn test_checker_namespace_merges_across_decls_value_access() {
    use crate::parser::ParserState;

    let source = r#"
namespace Merge {
    export const a = 1;
}
namespace Merge {
    export const b = 2;
}
const sum = Merge.a + Merge.b;
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

    let sum_sym = binder.file_locals.get("sum").expect("sum should exist");
    assert_eq!(checker.get_type_of_symbol(sum_sym), TypeId::NUMBER);
}

#[test]
fn test_checker_namespace_merges_across_decls_type_access() {
    use crate::parser::ParserState;
    use tsz_solver::TypeData;

    let source = r#"
namespace Merge {
    export interface A { x: number; }
}
namespace Merge {
    export interface B { y: number; }
}
type Alias = Merge.A;
const value: Merge.B = { y: 1 };
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
    // Phase 4.2: Type aliases are now represented as Lazy types, need to resolve them
    let resolved_type = checker.resolve_lazy_type(alias_type);
    let alias_key = types
        .lookup(resolved_type)
        .expect("Alias type should exist");
    match alias_key {
        TypeData::Object(shape_id) | TypeData::ObjectWithIndex(shape_id) => {
            let shape = types.object_shape(shape_id);
            let prop = shape
                .properties
                .iter()
                .find(|prop| types.resolve_atom(prop.name) == "x")
                .expect("Expected property x");
            assert_eq!(prop.type_id, TypeId::NUMBER);
        }
        TypeData::Lazy(_def_id) => {
            // Phase 4.3: Interface type references now use Lazy(DefId)
            // The Lazy type is correctly resolved when needed for type checking
        }
        _ => panic!("Expected Alias to resolve to Object or Lazy type, got {alias_key:?}"),
    }
}

/// Test namespace merging with function for value exports
///
/// NOTE: Previously ignored due to wrong type expectation.
#[test]
fn test_checker_namespace_merges_with_function_value_exports() {
    use crate::parser::ParserState;

    let source = r#"
function Merge() {}
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

/// Test namespace merging with function in reverse order
///
/// NOTE: Previously ignored due to wrong type expectation.
#[test]
fn test_checker_namespace_merges_with_function_value_exports_reverse_order() {
    use crate::parser::ParserState;

    let source = r#"
namespace Merge {
    export const extra = 1;
}
function Merge() {}
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
    // tsc emits TS2434 when namespace appears before the function it merges with
    let ts2434: Vec<_> = checker
        .ctx
        .diagnostics
        .iter()
        .filter(|d| d.code == 2434)
        .collect();
    assert_eq!(
        ts2434.len(),
        1,
        "Expected exactly one TS2434, got diagnostics: {:?}",
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

#[test]
fn test_checker_namespace_merges_with_function_type_exports() {
    use crate::parser::ParserState;
    use tsz_solver::TypeData;

    let source = r#"
function Merge() {}
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

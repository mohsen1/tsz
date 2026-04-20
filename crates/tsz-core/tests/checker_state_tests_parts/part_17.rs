//! Tests for Checker - Type checker using `NodeArena` and Solver
//!
//! This module contains comprehensive type checking tests organized into categories:
//! - Basic type checking (creation, intrinsic types, type interning)
//! - Type compatibility and assignability
//! - Excess property checking
//! - Function overloads and call resolution
//! - Generic types and type inference
//! - Control flow analysis
//! - Error diagnostics
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
fn test_explicit_property_no_error_4111() {
    use crate::parser::ParserState;

    let source = r#"
interface MixedType {
    explicitProp: string;
    [key: string]: string | number;
}
const obj: MixedType = {} as any;
const val = obj.explicitProp;
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut binder = BinderState::new();
    merge_shared_lib_symbols(&mut binder);
    binder.bind_source_file(parser.get_arena(), root);

    let types = TypeInterner::new();
    let opts = crate::checker::context::CheckerOptions {
        jsx_factory: "React.createElement".to_string(),
        jsx_fragment_factory: "React.Fragment".to_string(),
        no_property_access_from_index_signature: true,
        ..Default::default()
    };
    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.ts".to_string(),
        opts,
    );
    setup_lib_contexts(&mut checker);
    checker.check_source_file(root);

    let codes: Vec<u32> = checker.ctx.diagnostics.iter().map(|d| d.code).collect();
    assert!(
        !codes.contains(&4111),
        "Should not have error 4111 for explicit property"
    );
}

/// TODO: Property access from index signature on mixed unions incorrectly emits TS4111.
/// When a union has one member with an explicit property and another with an index
/// signature, tsc does NOT emit TS4111 for the explicit property. Currently we do emit it.
/// When this is fixed, update to assert !codes.contains(&4111).
#[test]
fn test_union_with_index_signature_4111() {
    use crate::parser::ParserState;

    let source = r#"
type Mixed = { x: number } | { [key: string]: number };
const obj: Mixed = {} as any;
const val = obj.x;
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut binder = BinderState::new();
    merge_shared_lib_symbols(&mut binder);
    binder.bind_source_file(parser.get_arena(), root);

    let types = TypeInterner::new();
    let opts = crate::checker::context::CheckerOptions {
        jsx_factory: "React.createElement".to_string(),
        jsx_fragment_factory: "React.Fragment".to_string(),
        no_property_access_from_index_signature: true,
        ..Default::default()
    };
    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.ts".to_string(),
        opts,
    );
    setup_lib_contexts(&mut checker);
    checker.check_source_file(root);

    let codes: Vec<u32> = checker.ctx.diagnostics.iter().map(|d| d.code).collect();
    // Should NOT emit 4111 when union member has explicit property 'x'.
    // Previously incorrectly emitted TS4111 for mixed union with index signature;
    // fixed by preserving union index access diagnostics.
    assert!(
        !codes.contains(&4111),
        "Expected no TS4111 when union member has explicit property 'x', got: {codes:?}"
    );
}

#[test]
fn test_checker_lowers_full_source_file() {
    use crate::parser::ParserState;
    use tsz_solver::TypeData;

    let source = r#"
interface Foo { x: number; }
type Bar = Foo | string;
type Baz = [string, number];
type Qux = { [key: string]: Foo };
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
    let bar_sym = binder.file_locals.get("Bar").expect("Bar should exist");
    let baz_sym = binder.file_locals.get("Baz").expect("Baz should exist");
    let qux_sym = binder.file_locals.get("Qux").expect("Qux should exist");

    let foo_type = checker.get_type_of_symbol(foo_sym);
    let foo_key = types.lookup(foo_type).expect("Foo type should exist");
    match foo_key {
        TypeData::Object(shape_id) | TypeData::ObjectWithIndex(shape_id) => {
            let shape = types.object_shape(shape_id);
            let prop = shape
                .properties
                .iter()
                .find(|prop| types.resolve_atom(prop.name) == "x")
                .expect("Expected property x");
            assert_eq!(prop.type_id, TypeId::NUMBER);
        }
        _ => panic!("Expected Foo to be Object type, got {foo_key:?}"),
    }

    let bar_type = checker.get_type_of_symbol(bar_sym);
    let bar_key = types.lookup(bar_type).expect("Bar type should exist");
    match bar_key {
        TypeData::Union(members) => {
            let members = types.type_list(members);
            assert_eq!(members.len(), 2);
            assert!(members.contains(&TypeId::STRING));
            // The non-string member may be a lazy type reference to Foo
            // (TypeData::Lazy) or the resolved Object type. Either is valid.
            let non_string_member = *members.iter().find(|&&m| m != TypeId::STRING).unwrap();
            if non_string_member != foo_type {
                // If not the same TypeId, verify it's a lazy reference (unevaluated Foo)
                let member_key = types
                    .lookup(non_string_member)
                    .expect("member type should exist");
                assert!(
                    matches!(member_key, TypeData::Lazy(_)),
                    "Non-string member should be foo_type or a Lazy reference, got {member_key:?}"
                );
            }
        }
        _ => panic!("Expected Bar to be Union type, got {bar_key:?}"),
    }

    let baz_type = checker.get_type_of_symbol(baz_sym);
    let baz_key = types.lookup(baz_type).expect("Baz type should exist");
    match baz_key {
        TypeData::Tuple(elements) => {
            let elements = types.tuple_list(elements);
            assert_eq!(elements.len(), 2);
            assert_eq!(elements[0].type_id, TypeId::STRING);
            assert_eq!(elements[1].type_id, TypeId::NUMBER);
        }
        _ => panic!("Expected Baz to be Tuple type, got {baz_key:?}"),
    }

    let qux_type = checker.get_type_of_symbol(qux_sym);
    let qux_key = types.lookup(qux_type).expect("Qux type should exist");
    match qux_key {
        TypeData::ObjectWithIndex(shape_id) => {
            let shape = types.object_shape(shape_id);
            let string_index = shape
                .string_index
                .as_ref()
                .expect("Expected string index signature");
            assert_eq!(string_index.key_type, TypeId::STRING);
            let value_key = types
                .lookup(string_index.value_type)
                .expect("Index value type should exist");
            match value_key {
                TypeData::Lazy(_def_id) => {} // Phase 4.2: Now uses Lazy(DefId) instead of Ref(SymbolRef)
                _ => panic!("Expected Foo lazy type, got {value_key:?}"),
            }
        }
        _ => panic!("Expected Qux to be ObjectWithIndex type, got {qux_key:?}"),
    }
}

/// Test that interface extends correctly inherits properties
///
/// NOTE: Currently ignored - interface extends is not fully implemented.
/// Properties from parent interfaces are not correctly inherited.
#[test]
fn test_interface_extends_inherits_properties() {
    use crate::parser::ParserState;

    let source = r#"
interface Base {
    base: string;
}
interface Derived extends Base {
    derived: number;
}
const obj: Derived = { base: "x", derived: 1 };
const base_value = obj.base;
const derived_value = obj.derived;
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

    let base_sym = binder
        .file_locals
        .get("base_value")
        .expect("base_value should exist");
    let base_type = checker.get_type_of_symbol(base_sym);
    assert_eq!(base_type, TypeId::STRING);

    let derived_sym = binder
        .file_locals
        .get("derived_value")
        .expect("derived_value should exist");
    let derived_type = checker.get_type_of_symbol(derived_sym);
    assert_eq!(derived_type, TypeId::NUMBER);
}

/// Test that interface extends correctly applies type arguments
///
/// NOTE: Currently ignored - interface extension with type arguments is not fully
/// implemented. Generic type parameters in interface extends clauses are not
/// correctly resolved.
#[test]
fn test_interface_extends_applies_type_arguments() {
    use crate::parser::ParserState;

    let source = r#"
interface Box<T> {
    value: T;
}
interface Derived extends Box<string> {
    count: number;
}
const obj: Derived = { value: "x", count: 1 };
const value = obj.value;
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

    let value_sym = binder.file_locals.get("value").expect("value should exist");
    let value_type = checker.get_type_of_symbol(value_sym);
    assert_eq!(value_type, TypeId::STRING);
}

/// Test that interface extends with type alias applies type arguments
///
/// NOTE: Currently ignored - see `test_interface_extends_applies_type_arguments`.
#[test]
fn test_interface_extends_type_alias_applies_type_arguments() {
    use crate::parser::ParserState;

    let source = r#"
type Box<T> = { value: T };
interface Derived extends Box<string> {
    count: number;
}
const obj: Derived = { value: "x", count: 1 };
const value = obj.value;
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
    assert_eq!(value_type, TypeId::STRING);
}

#[test]
fn test_interface_extends_class_applies_type_arguments() {
    use crate::parser::ParserState;

    let source = r#"
class Box<T> {
    value!: T;
}
interface Derived extends Box<string> {
    count: number;
}
const obj: Derived = { value: "x", count: 1 };
const value = obj.value;
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
    assert_eq!(value_type, TypeId::STRING);
}

#[test]
fn test_interface_extends_readonly_property_mismatch_2430() {
    use crate::parser::ParserState;

    let source = r#"
interface Base {
    x: number;
}
interface Derived extends Base {
    readonly x: number;
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

    let codes: Vec<u32> = checker.ctx.diagnostics.iter().map(|d| d.code).collect();
    // TypeScript allows `readonly x: number` to satisfy `x: number` in interface extends
    assert!(
        codes.is_empty(),
        "Expected no errors for readonly property in interface extends, got: {codes:?}"
    );
}

#[test]
fn test_interface_extends_optional_property_mismatch_2430() {
    use crate::parser::ParserState;

    let source = r#"
interface Base {
    x: number;
}
interface Derived extends Base {
    x?: number;
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

    let codes: Vec<u32> = checker.ctx.diagnostics.iter().map(|d| d.code).collect();
    assert!(
        codes.contains(&2430),
        "Expected error 2430 for optional property mismatch, got: {codes:?}"
    );
}

#[test]
fn test_optional_property_allows_undefined_assignment() {
    use crate::parser::ParserState;

    let source = r#"
interface Foo {
    x?: number;
}
const ok: Foo = {};
const ok2: Foo = { x: 1 };
const ok3: Foo = { x: undefined };
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
}


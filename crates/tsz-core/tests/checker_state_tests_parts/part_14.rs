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
fn test_contextual_typing_for_object_properties() {
    use tsz_solver::ContextualTypeContext;

    // Test that ContextualTypeContext can extract property types from object types
    let types = TypeInterner::new();

    // Create an object type: { name: string, age: number }
    use tsz_solver::PropertyInfo;

    let obj_type = types.object(vec![
        PropertyInfo {
            name: types.intern_string("name"),
            type_id: TypeId::STRING,
            write_type: TypeId::STRING,
            optional: false,
            readonly: false,
            is_method: false,
            is_class_prototype: false,
            visibility: Visibility::Public,
            parent_id: None,
            declaration_order: 0,
            is_string_named: false,
        },
        PropertyInfo {
            name: types.intern_string("age"),
            type_id: TypeId::NUMBER,
            write_type: TypeId::NUMBER,
            optional: false,
            readonly: false,
            is_method: false,
            is_class_prototype: false,
            visibility: Visibility::Public,
            parent_id: None,
            declaration_order: 0,
            is_string_named: false,
        },
    ]);

    // Create contextual context
    let ctx = ContextualTypeContext::with_expected(&types, obj_type);

    // Test property type extraction
    assert_eq!(ctx.get_property_type("name"), Some(TypeId::STRING));
    assert_eq!(ctx.get_property_type("age"), Some(TypeId::NUMBER));
    assert_eq!(ctx.get_property_type("unknown"), None);
}

#[test]
fn test_contextual_property_type_infers_callback_param() {
    use crate::parser::ParserState;

    let source = r#"
type Handler = { cb: (x: number) => void };
const h: Handler = { cb: x => x.toUpperCase() };
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
        codes.contains(&2339),
        "Expected error 2339 for contextual property param mismatch, got: {codes:?}"
    );
}

#[test]
fn test_ts2339_any_property_access_no_error() {
    use crate::parser::ParserState;

    let source = r#"
let value: any;
value.foo;
value.bar();
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    assert!(
        parser.get_diagnostics().is_empty(),
        "Parse errors: {:?}",
        parser.get_diagnostics()
    );

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
        !codes.contains(&2339),
        "Did not expect 2339 for property access on any, got: {codes:?}"
    );
}

#[test]
fn test_ts2339_unknown_property_access_after_narrowing() {
    use crate::parser::ParserState;

    let source = r#"
let value: unknown = {};
value.foo;
const obj: object = value as object;
obj.foo;
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    assert!(
        parser.get_diagnostics().is_empty(),
        "Parse errors: {:?}",
        parser.get_diagnostics()
    );

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

    // value.foo → TS18046 ("'value' is of type 'unknown'.")
    let ts18046_count = checker
        .ctx
        .diagnostics
        .iter()
        .filter(|d| d.code == 18046)
        .count();
    assert_eq!(
        ts18046_count, 1,
        "Expected one TS18046 error for unknown.foo, got: {:?}",
        checker.ctx.diagnostics
    );
    // obj.foo → TS2339 ("Property 'foo' does not exist on type 'object'.")
    let ts2339_count = checker
        .ctx
        .diagnostics
        .iter()
        .filter(|d| d.code == 2339)
        .count();
    assert_eq!(
        ts2339_count, 1,
        "Expected one TS2339 error for object.foo, got: {:?}",
        checker.ctx.diagnostics
    );
}

#[test]
fn test_ts2339_catch_binding_unknown() {
    use crate::parser::ParserState;

    let source = r#"
// @strict: true
function f() {
    try {
    } catch ({ x }) {
    }
}
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    assert!(
        parser.get_diagnostics().is_empty(),
        "Parse errors: {:?}",
        parser.get_diagnostics()
    );

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

    let count = checker
        .ctx
        .diagnostics
        .iter()
        .filter(|d| d.code == 2339)
        .count();
    assert!(
        count >= 1,
        "Expected at least one 2339 for catch destructuring from unknown, got: {:?}",
        checker.ctx.diagnostics
    );
}

#[test]
fn test_ts2339_union_optional_property_access() {
    use crate::parser::ParserState;

    let source = r#"
type A = { foo?: string };
type B = { foo: string };

function read(value: A | B) {
    return value.foo;
}
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    assert!(
        parser.get_diagnostics().is_empty(),
        "Parse errors: {:?}",
        parser.get_diagnostics()
    );

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
        !codes.contains(&2339),
        "Did not expect 2339 for optional property on union, got: {codes:?}"
    );
}

#[test]
fn test_ts2339_class_static_inheritance() {
    use crate::parser::ParserState;

    let source = r#"
class Base {
    static foo: number;
}

class Derived extends Base {}

Derived.foo;
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    assert!(
        parser.get_diagnostics().is_empty(),
        "Parse errors: {:?}",
        parser.get_diagnostics()
    );

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
        !codes.contains(&2339),
        "Did not expect 2339 for inherited static property access, got: {codes:?}"
    );
}

#[test]
fn test_ts2339_class_instance_object_members() {
    use crate::parser::ParserState;

    let source = r#"
class C {
    x: number = 1;
}

const c = new C();
c.toString();
c.hasOwnProperty("x");
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    assert!(
        parser.get_diagnostics().is_empty(),
        "Parse errors: {:?}",
        parser.get_diagnostics()
    );

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
        !codes.contains(&2339),
        "Did not expect 2339 for Object prototype member access, got: {codes:?}"
    );
}

#[test]
fn test_ts2339_this_missing_property_in_class() {
    use crate::parser::ParserState;

    let source = r#"
class C {
    constructor() {
        this.missing;
    }
}
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    assert!(
        parser.get_diagnostics().is_empty(),
        "Parse errors: {:?}",
        parser.get_diagnostics()
    );

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
        codes.contains(&2339),
        "Expected 2339 for missing property on this, got: {codes:?}"
    );
}

#[test]
fn test_ts2339_static_property_access_from_instance() {
    use crate::parser::ParserState;

    let source = r#"
class C {
    static foo: number;
    static get bar() { return 1; }
    value = 1;
}

const c = new C();
c.foo;
c.bar;
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    assert!(
        parser.get_diagnostics().is_empty(),
        "Parse errors: {:?}",
        parser.get_diagnostics()
    );

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
        codes.contains(&2339) || codes.contains(&2576),
        "Expected TS2339/TS2576 for static property access on instance, got: {codes:?}"
    );
}


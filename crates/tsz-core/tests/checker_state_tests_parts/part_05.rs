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
fn test_overload_call_handles_tuple_spread_params() {
    use crate::parser::ParserState;

    let source = r#"
declare function foo1(a: number, b: string, c: boolean, ...d: number[]): void;

function foo2<T extends [number, string]>(t1: T, t2: [boolean], a1: number[]) {
    foo1(...t1, true, 42, 43, 44);
    foo1(...t1, ...t2, 42, 43, 44);
    foo1(...t1, ...t2, ...a1);
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
}

#[test]
fn test_overload_call_handles_variadic_tuple_param() {
    use crate::parser::ParserState;

    let source = r#"
declare function ft3<T extends unknown[]>(t: [...T]): T;
declare function ft4<T extends unknown[]>(t: [...T]): readonly [...T];

ft3(["hello", 42]);
ft4(["hello", 42]);
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

#[test]
fn test_overload_call_handles_generic_signatures() {
    use crate::parser::ParserState;

    let source = r#"
function id<T>(x: T): T;
function id(x: any): any;
function id(x: any) { return x; }
id("test");
id(123);
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

/// Test that overload calls work with array methods
///
/// NOTE: Currently ignored - overload resolution for array methods is not fully
/// implemented. The checker doesn't correctly match array method overloads for
/// generic callback functions (map and filter work, reduce has overload issues).
#[test]
fn test_overload_call_array_methods() {
    use crate::parser::ParserState;

    let source = r#"
const arr = [1, 2, 3];
arr.map(x => x * 2);
arr.filter(x => x > 1);
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

/// TODO: Array.reduce overload resolution picks wrong overload for callback type inference.
/// The callback should be contextually typed from the correct `Array.reduce` overload.
#[test]
fn test_overload_call_array_reduce() {
    use crate::parser::ParserState;

    let source = r#"
const arr = [1, 2, 3];
arr.reduce((a, b) => a + b, 0);
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
    assert_eq!(
        codes.len(),
        0,
        "Expected no diagnostics once overload resolution picks the right Array.reduce overload, got: {codes:?}"
    );
}

/// Block-body callbacks with explicit param types should match generic overloads.
/// Regression test: `raw_block_body_callback_mismatch` was incorrectly checking
/// `is_assignable_to(actual_return, expected_return)` where `expected_return` was
/// a type parameter (e.g., `U`), causing a false TS2769 for block-body arrows
/// like `(acc: number[], a: number) => { return [a]; }` against `reduce<U>`.
#[test]
fn test_generic_overload_block_body_callback_no_false_ts2769() {
    use crate::parser::ParserState;

    let source = r#"
const arr = [1, 2, 3];
arr.reduce((acc: number[], a: number, index: number) => { return [a] }, []);
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
        !codes.contains(&2769),
        "Should not emit TS2769 for block-body callback matching generic overload, got: {codes:?}"
    );
}

#[test]
fn test_class_method_overload_reports_no_overload_matches() {
    use crate::checker::diagnostics::diagnostic_codes;
    use crate::parser::ParserState;

    let source = r#"
class C {
    foo(x: string): void;
    foo(x: number): void;
    foo(x: any) {}
}
const c = new C();
c.foo(true);
c.foo("ok");
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
    let count_2769 = codes
        .iter()
        .filter(|&&c| c == diagnostic_codes::NO_OVERLOAD_MATCHES_THIS_CALL)
        .count();
    assert_eq!(
        count_2769, 1,
        "Expected exactly one overload mismatch (2769), got: {codes:?}"
    );
}

#[test]
fn test_new_expression_infers_class_instance_type() {
    use crate::parser::ParserState;
    use tsz_solver::TypeData;

    let source = r#"
class Foo {
    name = "";
    count = 1;
    readonly tag: string = "x";
    greet(msg: string): number { return 1; }
}
const f = new Foo();
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

    println!("=== debug Box ===");
    if let Some(box_sym) = binder.file_locals.get("Box") {
        let box_type = checker.get_type_of_symbol(box_sym);
        println!("Box type id: {box_type:?}");
        println!("Box type key: {:?}", types.lookup(box_type));
    } else {
        println!("Box symbol missing");
    }

    let f_sym = binder.file_locals.get("f").expect("f should exist");
    let f_type = checker.get_type_of_symbol(f_sym);
    let f_key = types.lookup(f_type).expect("f type should exist");
    match f_key {
        TypeData::Object(shape_id) | TypeData::ObjectWithIndex(shape_id) => {
            let shape = types.object_shape(shape_id);
            let props = shape.properties.as_slice();
            let name_atom = types.intern_string("name");
            let count_atom = types.intern_string("count");
            let tag_atom = types.intern_string("tag");
            let greet_atom = types.intern_string("greet");

            assert!(
                props
                    .iter()
                    .any(|p| p.name == name_atom && p.type_id == TypeId::STRING),
                "Expected name: string in class instance properties, got: {props:?}"
            );
            assert!(
                props
                    .iter()
                    .any(|p| p.name == count_atom && p.type_id == TypeId::NUMBER),
                "Expected count: number in class instance properties, got: {props:?}"
            );
            let tag_prop = props
                .iter()
                .find(|p| p.name == tag_atom)
                .expect("tag property should exist");
            assert!(tag_prop.readonly, "Expected tag to be readonly");
            assert!(
                props.iter().any(|p| p.name == greet_atom && p.is_method),
                "Expected greet method in class instance properties, got: {props:?}"
            );
        }
        _ => panic!("Expected f to be Object or ObjectWithIndex type, got {f_key:?}"),
    }
}

#[test]
fn test_new_expression_infers_parameter_properties() {
    use crate::parser::ParserState;
    use tsz_solver::TypeData;

    let source = r#"
class Foo {
    constructor(public id: number, readonly tag: string, count: number) {}
}
const f = new Foo(1, "x", 2);
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

    let f_sym = binder.file_locals.get("f").expect("f should exist");
    let f_type = checker.get_type_of_symbol(f_sym);
    let f_key = types.lookup(f_type).expect("f type should exist");
    match f_key {
        TypeData::Object(shape_id) | TypeData::ObjectWithIndex(shape_id) => {
            let shape = types.object_shape(shape_id);
            let props = shape.properties.as_slice();
            let id_atom = types.intern_string("id");
            let tag_atom = types.intern_string("tag");
            let count_atom = types.intern_string("count");

            assert!(
                props
                    .iter()
                    .any(|p| p.name == id_atom && p.type_id == TypeId::NUMBER),
                "Expected id: number in class instance properties, got: {props:?}"
            );
            let tag_prop = props
                .iter()
                .find(|p| p.name == tag_atom)
                .expect("tag property should exist");
            assert_eq!(tag_prop.type_id, TypeId::STRING);
            assert!(tag_prop.readonly, "Expected tag to be readonly");
            assert!(
                !props.iter().any(|p| p.name == count_atom),
                "Expected count to be absent from class instance properties, got: {props:?}"
            );
        }
        _ => panic!("Expected f to be Object or ObjectWithIndex type, got {f_key:?}"),
    }
}

#[test]
fn test_new_expression_infers_base_class_properties() {
    use crate::parser::ParserState;
    use tsz_solver::TypeData;

    let source = r#"
class Base<T> {
    value: T;
    constructor(value: T) {
        this.value = value;
    }
}
class Derived extends Base<string> {
    count = 1;
    constructor() {
        super("default");
    }
}
const d = new Derived();
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

    let d_sym = binder.file_locals.get("d").expect("d should exist");
    let d_type = checker.get_type_of_symbol(d_sym);
    let d_key = types.lookup(d_type).expect("d type should exist");
    match d_key {
        TypeData::Object(shape_id) | TypeData::ObjectWithIndex(shape_id) => {
            let shape = types.object_shape(shape_id);
            let props = shape.properties.as_slice();
            let value_atom = types.intern_string("value");
            let count_atom = types.intern_string("count");
            let value_prop = props
                .iter()
                .find(|p| p.name == value_atom)
                .expect("value property should exist");
            assert_eq!(value_prop.type_id, TypeId::STRING);
            assert!(
                props
                    .iter()
                    .any(|p| p.name == count_atom && p.type_id == TypeId::NUMBER),
                "Expected count: number in class instance properties, got: {props:?}"
            );
        }
        _ => panic!("Expected d to be Object or ObjectWithIndex type, got {d_key:?}"),
    }
}

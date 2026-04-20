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
/// Test TS7053: Element access with union string index requires index signature
///
/// When noImplicitAny is enabled, accessing an object with a union string index
/// that includes non-literal types should emit TS7053.
#[test]
fn test_checker_element_access_union_string_index_requires_signature() {
    use crate::parser::ParserState;

    let source = r#"
interface Foo { x: number; }
const obj: Foo = { x: 1 };
let key: "x" | string;
const value = obj[key];
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
        no_implicit_any: true,
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
        codes.contains(&7053),
        "Expected error 7053 for union string index, got: {codes:?}"
    );
}

/// Test TS7053: Element access with union string/number index requires index signature
///
/// When noImplicitAny is enabled, accessing an object with a union string/number index
/// should emit TS7053. Related to `test_checker_element_access_union_string_index_requires_signature`.
#[test]
fn test_checker_element_access_union_string_number_index_requires_signature() {
    use crate::parser::ParserState;

    let source = r#"
interface Foo { x: number; }
const obj: Foo = { x: 1 };
let key: string | number;
const value = obj[key];
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
        no_implicit_any: true,
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
        codes.contains(&7053),
        "Expected error 7053 for union string/number index, got: {codes:?}"
    );
}

#[test]
fn test_checker_lowers_element_access_literal_key_union() {
    use crate::parser::ParserState;
    use tsz_solver::TypeData;

    let source = r#"
interface Foo { a: number; b: string; }
const obj: Foo = { a: 1, b: "hi" };
declare let key: "a" | "b";
const value = obj[key];
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
fn test_checker_element_access_union_key_cross_product() {
    use crate::parser::ParserState;
    use tsz_solver::TypeData;

    let source = r#"
type A = { kind: "a"; val: 1 } | { kind: "b"; val: 2 };
declare const obj: A;
declare const key: "kind" | "val";
const value = obj[key];
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
            let lit_a = types.literal_string("a");
            let lit_b = types.literal_string("b");
            let lit_one = types.literal_number(1.0);
            let lit_two = types.literal_number(2.0);
            assert!(members.contains(&lit_a));
            assert!(members.contains(&lit_b));
            assert!(members.contains(&lit_one));
            assert!(members.contains(&lit_two));
        }
        other => panic!("Expected union type for value, got {other:?}"),
    }
}

#[test]
fn test_checker_lowers_element_access_literal_key_type() {
    use crate::parser::ParserState;

    let source = r#"
interface Foo { a: number; b: string; }
const obj: Foo = { a: 1, b: "hi" };
declare let key: "a";
const value = obj[key];
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
    assert_eq!(value_type, TypeId::NUMBER);
}

#[test]
fn test_checker_lowers_element_access_numeric_literal_union() {
    use crate::parser::ParserState;
    use tsz_solver::TypeData;

    let source = r#"
const tup: [string, number, boolean] = ["a", 1, true];
declare let idx: 0 | 2;
const value = tup[idx];
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
            assert!(members.contains(&TypeId::STRING));
            assert!(members.contains(&TypeId::BOOLEAN));
            assert_eq!(members.len(), 2);
        }
        _ => panic!("Expected union type for value, got {value_key:?}"),
    }
}

#[test]
fn test_checker_lowers_element_access_mixed_literal_key_union() {
    use crate::parser::ParserState;
    use tsz_solver::TypeData;

    let source = r#"
const arr: string[] = ["a"];
declare let key: "length" | 0;
const value = arr[key];
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
            assert!(members.contains(&TypeId::STRING));
            assert!(members.contains(&TypeId::NUMBER));
            assert_eq!(members.len(), 2);
        }
        _ => panic!("Expected union type for value, got {value_key:?}"),
    }
}

#[test]
fn test_checker_element_access_reports_nullable_object() {
    use crate::parser::ParserState;

    let source = r#"
type Foo = { a: number };
let obj: Foo | undefined;
const value = obj["a"];
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
        strict_null_checks: true,
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
    // tsc emits TS18048 "'obj' is possibly 'undefined'." with strictNullChecks
    assert!(
        codes.contains(&18048),
        "Expected error 18048 for possibly undefined object, got: {codes:?}"
    );

    let value_sym = binder.file_locals.get("value").expect("value should exist");
    let value_type = checker.get_type_of_symbol(value_sym);
    assert_eq!(value_type, TypeId::NUMBER);
}

#[test]
fn test_checker_element_access_optional_chain_nullable_object() {
    use crate::parser::ParserState;
    use tsz_solver::TypeData;

    let source = r#"
type Foo = { a: number };
let obj: Foo | undefined;
const value = obj?.["a"];
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
            assert!(members.contains(&TypeId::UNDEFINED));
        }
        _ => panic!("Expected union type for value, got {value_key:?}"),
    }
}

#[test]
fn test_checker_property_access_optional_chain_nullable_object() {
    use crate::parser::ParserState;
    use tsz_solver::TypeData;

    let source = r#"
type Foo = { a: number };
let obj: Foo | undefined;
const value = obj?.a;
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
            assert!(members.contains(&TypeId::UNDEFINED));
        }
        _ => panic!("Expected union type for value, got {value_key:?}"),
    }
}

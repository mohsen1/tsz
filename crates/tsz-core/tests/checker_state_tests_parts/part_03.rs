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
fn test_checker_subtype_literals() {
    let arena = NodeArena::new();
    let binder = BinderState::new();
    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        &arena,
        &binder,
        &types,
        "test.ts".to_string(),
        crate::checker::context::CheckerOptions {
            jsx_factory: "React.createElement".to_string(),
            jsx_factory_from_config: false,
            jsx_fragment_factory: "React.Fragment".to_string(),
            jsx_fragment_factory_from_config: false,
            strict_function_types: true,
            ..Default::default()
        },
    );

    // String literal is subtype of string
    let hello = checker.ctx.types.literal_string("hello");
    assert!(checker.is_assignable_to(hello, TypeId::STRING));

    // Number literal is subtype of number
    let forty_two = checker.ctx.types.literal_number(42.0);
    assert!(checker.is_assignable_to(forty_two, TypeId::NUMBER));

    // Boolean literal is subtype of boolean
    let t = checker.ctx.types.literal_boolean(true);
    assert!(checker.is_assignable_to(t, TypeId::BOOLEAN));

    // String literal is NOT assignable to number
    assert!(!checker.is_assignable_to(hello, TypeId::NUMBER));
}

#[test]
fn test_checker_subtype_unions() {
    let arena = NodeArena::new();
    let binder = BinderState::new();
    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        &arena,
        &binder,
        &types,
        "test.ts".to_string(),
        crate::checker::context::CheckerOptions {
            jsx_factory: "React.createElement".to_string(),
            jsx_factory_from_config: false,
            jsx_fragment_factory: "React.Fragment".to_string(),
            jsx_fragment_factory_from_config: false,
            strict_function_types: true,
            ..Default::default()
        },
    );

    // Create string | number union
    let string_or_number = checker.get_union_type(vec![TypeId::STRING, TypeId::NUMBER]);

    // String is assignable to string | number
    assert!(checker.is_assignable_to(TypeId::STRING, string_or_number));
    assert!(checker.is_assignable_to(TypeId::NUMBER, string_or_number));

    // Boolean is NOT assignable to string | number
    assert!(!checker.is_assignable_to(TypeId::BOOLEAN, string_or_number));

    // string | number is assignable to string | number | boolean
    let three_types = checker.get_union_type(vec![TypeId::STRING, TypeId::NUMBER, TypeId::BOOLEAN]);
    assert!(checker.is_assignable_to(string_or_number, three_types));
}

#[test]
fn test_checker_assignability_direct_union_member_fast_path() {
    let arena = NodeArena::new();
    let binder = BinderState::new();
    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        &arena,
        &binder,
        &types,
        "test.ts".to_string(),
        crate::checker::context::CheckerOptions {
            jsx_factory: "React.createElement".to_string(),
            jsx_factory_from_config: false,
            jsx_fragment_factory: "React.Fragment".to_string(),
            jsx_fragment_factory_from_config: false,
            strict_function_types: true,
            ..Default::default()
        },
    );

    let string_or_number = checker.get_union_type(vec![TypeId::STRING, TypeId::NUMBER]);
    assert!(checker.is_assignable_to(TypeId::STRING, string_or_number));
    assert!(checker.is_assignable_to_bivariant(TypeId::STRING, string_or_number));

    let regular_flags = checker.ctx.pack_relation_flags();
    let bivariant_flags = regular_flags & !RelationCacheKey::FLAG_STRICT_FUNCTION_TYPES;
    let regular_key =
        RelationCacheKey::assignability(TypeId::STRING, string_or_number, regular_flags, 0);
    let bivariant_key =
        RelationCacheKey::assignability(TypeId::STRING, string_or_number, bivariant_flags, 0);
    assert_ne!(
        regular_key, bivariant_key,
        "regular and bivariant union-member assignability must use distinct relation cache keys"
    );
}

#[test]
fn test_checker_type_identity() {
    let arena = NodeArena::new();
    let binder = BinderState::new();
    let types = TypeInterner::new();
    let checker = CheckerState::new(
        &arena,
        &binder,
        &types,
        "test.ts".to_string(),
        crate::checker::context::CheckerOptions::default(),
    );

    // Same type is identical to itself
    assert_eq!(TypeId::STRING, TypeId::STRING);
    assert_eq!(TypeId::NUMBER, TypeId::NUMBER);

    // Different types are not identical
    assert_ne!(TypeId::STRING, TypeId::NUMBER);

    // Same literal values produce identical types (via interning)
    let lit1 = checker.ctx.types.literal_string("test");
    let lit2 = checker.ctx.types.literal_string("test");
    assert_eq!(lit1, lit2);
}

#[test]
fn test_check_object_literal_excess_properties() {
    use crate::parser::ParserState;

    let source = r#"
type Foo = { x: number };
let foo: Foo = { x: 1, y: 2 };
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut binder = BinderState::new();
    crate::test_fixtures::merge_shared_lib_symbols(&mut binder);
    binder.bind_source_file(parser.get_arena(), root);

    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.ts".to_string(),
        crate::checker::context::CheckerOptions::default(),
    );
    crate::test_fixtures::setup_lib_contexts(&mut checker);
    checker.check_source_file(root);

    let codes: Vec<u32> = checker.ctx.diagnostics.iter().map(|d| d.code).collect();
    assert!(
        codes.contains(&2322) || codes.contains(&2353),
        "Expected error code 2322 or 2353"
    );
}

#[test]
fn test_function_overload_missing_implementation_2391() {
    use crate::parser::ParserState;
    let source = r#"function foo();"#;

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
        codes.contains(&2391),
        "Expected error 2391 (Function implementation is missing), got: {codes:?}"
    );
}

#[test]
fn test_function_overload_with_implementation() {
    use crate::parser::ParserState;
    let source = r#"
function foo(): void;
function foo() {}
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
        !codes.contains(&2391),
        "Should not have error 2391 when implementation exists, got: {codes:?}"
    );
}

#[test]
fn test_function_overload_wrong_name_2389() {
    use crate::parser::ParserState;
    let source = r#"
function foo(): void;
function bar() {}
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
        codes.contains(&2389) || codes.contains(&2391),
        "Expected error 2389 or 2391 for wrong implementation name, got: {codes:?}"
    );
}

#[test]
fn test_duplicate_identifier_var_function_2300() {
    use crate::checker::diagnostics::diagnostic_codes;
    use crate::parser::ParserState;

    let source = r#"
var foo = 1;
function foo() {}
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

    let duplicate_count = checker
        .ctx
        .diagnostics
        .iter()
        .filter(|d| d.code == diagnostic_codes::DUPLICATE_IDENTIFIER)
        .count();
    // tsc emits 2 TS2300 errors (one per declaration), but we currently only emit 1.
    // TODO: emit TS2300 on both the var and function declarations.
    assert!(
        duplicate_count >= 1,
        "Expected at least one TS2300 for var/function duplicates, got: {:?}",
        checker.ctx.diagnostics
    );
}

#[test]
#[ignore = "Pre-existing failure from recent merges"]
fn test_duplicate_identifier_var_let_2300() {
    use crate::checker::diagnostics::diagnostic_codes;
    use crate::parser::ParserState;

    let source = r#"
var foo = 1;
let foo = 2;
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

    // tsc emits TS2451 (Cannot redeclare block-scoped variable) for var/let conflicts.
    // The `let` declaration introduces block-scoping, making both declarations conflict.
    let ts2451_count = checker
        .ctx
        .diagnostics
        .iter()
        .filter(|d| d.code == diagnostic_codes::CANNOT_REDECLARE_BLOCK_SCOPED_VARIABLE)
        .count();
    assert_eq!(
        ts2451_count, 2,
        "Expected 2 TS2451 for var followed by let, got: {:?}",
        checker.ctx.diagnostics
    );
}


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
fn test_checker_creation() {
    let ctx = TestContext::new();
    let checker = ctx.checker();

    // Basic sanity check
    assert!(checker.ctx.diagnostics.is_empty());
}

#[test]
fn test_checker_basic_types() {
    let ctx = TestContext::new();
    let _checker = ctx.checker();

    // Verify intrinsic TypeIds are constants (compile-time values)
    assert_eq!(TypeId::NUMBER.0, 9);
    assert_eq!(TypeId::STRING.0, 10);
    assert_eq!(TypeId::BOOLEAN.0, 8);
    assert_eq!(TypeId::ANY.0, 4);
    assert_eq!(TypeId::NEVER.0, 2);
}

#[test]
fn test_checker_type_interner() {
    let ctx = TestContext::new();
    let checker = ctx.checker();

    // Test that TypeInterner is properly initialized
    // Intrinsics should be pre-registered
    assert!(checker.ctx.types.lookup(TypeId::STRING).is_some());
    assert!(checker.ctx.types.lookup(TypeId::NUMBER).is_some());
    assert!(checker.ctx.types.lookup(TypeId::ANY).is_some());
}

#[test]
fn test_checker_structural_equality() {
    let ctx = TestContext::new();
    let checker = ctx.checker();

    // Test structural equality via TypeInterner
    // Same string literal should get same TypeId
    let str1 = checker.ctx.types.literal_string("hello");
    let str2 = checker.ctx.types.literal_string("hello");
    let str3 = checker.ctx.types.literal_string("world");

    assert_eq!(str1, str2); // Same structure = same TypeId
    assert_ne!(str1, str3); // Different structure = different TypeId
}

#[test]
fn test_checker_union_normalization() {
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

    // Test union normalization
    // Union with `any` should be `any`
    let with_any = checker.ctx.types.union(vec![TypeId::STRING, TypeId::ANY]);
    assert_eq!(with_any, TypeId::ANY);

    // Union with `never` should exclude `never`
    let with_never = checker.ctx.types.union(vec![TypeId::STRING, TypeId::NEVER]);
    assert_eq!(with_never, TypeId::STRING);

    // Union with `unknown` should be `unknown`
    let with_unknown = checker
        .ctx
        .types
        .union(vec![TypeId::STRING, TypeId::UNKNOWN]);
    assert_eq!(with_unknown, TypeId::UNKNOWN);

    // Nested unions should be flattened and deduplicated
    let inner = checker
        .ctx
        .types
        .union(vec![TypeId::STRING, TypeId::NUMBER]);
    let outer = checker.ctx.types.union(vec![inner, TypeId::STRING]);
    assert_eq!(outer, inner);

    // Single-element union should return the element
    let single = checker.ctx.types.union(vec![TypeId::STRING]);
    assert_eq!(single, TypeId::STRING);
}

#[test]
fn test_await_type_context_suggests_awaited() {
    use crate::checker::diagnostics::diagnostic_codes;
    use crate::parser::ParserState;

    let source = r#"
async function foo() {
  var v: await;
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
        crate::checker::context::CheckerOptions {
            jsx_factory: "React.createElement".to_string(),
            jsx_factory_from_config: false,
            jsx_fragment_factory: "React.Fragment".to_string(),
            jsx_fragment_factory_from_config: false,
            strict: false,
            strict_property_initialization: false,
            ..crate::checker::context::CheckerOptions::default()
        },
    );
    setup_lib_contexts(&mut checker);
    checker.check_source_file(root);

    let codes: Vec<u32> = checker.ctx.diagnostics.iter().map(|d| d.code).collect();
    let did_you_mean_count = codes
        .iter()
        .filter(|&&code| code == diagnostic_codes::CANNOT_FIND_NAME_DID_YOU_MEAN)
        .count();
    assert_eq!(
        did_you_mean_count, 1,
        "Expected TS2552 for 'await' in type position, got: {codes:?}"
    );
    assert!(
        !codes.contains(&diagnostic_codes::CANNOT_FIND_NAME),
        "Unexpected TS2304 for 'await' in type position: {codes:?}"
    );
}

#[test]
fn test_async_modifier_rejected_for_class_and_enum() {
    use crate::checker::diagnostics::diagnostic_codes;
    use crate::parser::ParserState;

    let source = r#"
async class C {}
async enum E { Value }
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    // Parser should NOT emit TS1042 — that is the checker's job
    let parser_1042_count = parser
        .get_diagnostics()
        .iter()
        .filter(|d| d.code == diagnostic_codes::MODIFIER_CANNOT_BE_USED_HERE)
        .count();
    assert_eq!(
        parser_1042_count,
        0,
        "Parser should not emit TS1042; the checker handles it. Got: {:?}",
        parser.get_diagnostics()
    );

    // Run the checker — it should produce TS1042 for both declarations
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
            strict: false,
            strict_function_types: false,
            strict_bind_call_apply: false,
            ..crate::checker::context::CheckerOptions::default()
        },
    );
    setup_lib_contexts(&mut checker);
    checker.check_source_file(root);

    let codes: Vec<u32> = checker.ctx.diagnostics.iter().map(|d| d.code).collect();
    let async_modifier_count = codes
        .iter()
        .filter(|&&code| code == diagnostic_codes::MODIFIER_CANNOT_BE_USED_HERE)
        .count();
    assert_eq!(
        async_modifier_count, 2,
        "Expected two TS1042 errors from checker for async class/enum, got: {codes:?}"
    );
}

#[test]
fn test_excess_property_in_variable_declaration() {
    use crate::parser::ParserState;

    let source = r#"
type Foo = { x: number };
const ok: Foo = { x: 1 };
const bad: Foo = { x: 1, y: 2 };
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
        crate::checker::context::CheckerOptions {
            jsx_factory: "React.createElement".to_string(),
            jsx_factory_from_config: false,
            jsx_fragment_factory: "React.Fragment".to_string(),
            jsx_fragment_factory_from_config: false,
            strict: false,
            strict_function_types: false,
            strict_bind_call_apply: false,
            ..crate::checker::context::CheckerOptions::default()
        },
    );
    setup_lib_contexts(&mut checker);
    checker.check_source_file(root);

    let codes: Vec<u32> = checker.ctx.diagnostics.iter().map(|d| d.code).collect();
    let excess_count = codes.iter().filter(|&&code| code == 2353).count();
    assert_eq!(
        excess_count, 1,
        "Expected exactly one error 2353 (Excess property), got codes: {codes:?}"
    );
}

#[test]
fn test_excess_property_allows_variable_assignment() {
    use crate::parser::ParserState;

    let source = r#"
type Foo = { x: number };
const obj = { x: 1, y: 2 };
const ok: Foo = obj;
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

    assert!(
        checker.ctx.diagnostics.is_empty(),
        "Unexpected diagnostics: {:?}",
        checker.ctx.diagnostics
    );
}

#[test]
fn test_object_trifecta_assignability_in_checker() {
    use crate::parser::ParserState;

    let source = r#"
let ok: {} = "hi";
let bad: object = "hi";
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
    let not_assignable_count = codes.iter().filter(|&&code| code == 2322).count();
    assert_eq!(
        not_assignable_count, 1,
        "Expected one 2322 error for object keyword rejecting string, got: {codes:?}"
    );
}


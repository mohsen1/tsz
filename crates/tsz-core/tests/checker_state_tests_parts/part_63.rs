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
/// Test that multiple required parameters after optional are all flagged
#[test]
fn test_multiple_required_params_after_optional_ts1016() {
    use crate::binder::BinderState;
    use crate::checker::diagnostics::diagnostic_codes;
    use crate::checker::state::CheckerState;
    use crate::parser::ParserState;
    use tsz_solver::TypeInterner;

    let source = r#"
function foo(a?: number, b: string, c: boolean) {
    return a;
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

    let ts1016_count = checker
        .ctx
        .diagnostics
        .iter()
        .filter(|d| {
            d.code == diagnostic_codes::A_REQUIRED_PARAMETER_CANNOT_FOLLOW_AN_OPTIONAL_PARAMETER
        })
        .count();

    assert_eq!(
        ts1016_count, 2,
        "Expected 2 TS1016 errors for two required params after optional. Got: {:?}",
        checker.ctx.diagnostics
    );
}

// =============================================================================
// Contextual Typing Tests for Destructuring Parameters
// =============================================================================

/// Test that destructuring parameters get contextual types from callback signatures
#[test]
fn test_contextual_typing_destructuring_param_object() {
    use crate::binder::BinderState;
    use crate::checker::state::CheckerState;
    use crate::parser::ParserState;
    use tsz_solver::TypeInterner;

    let source = r#"
type Handler = (item: { x: number, y: string }) => void;
const handler: Handler = ({ x, y }) => {
    // x should be number, y should be string
    let numVal: number = x;
    let strVal: string = y;
};
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

    // Should have no type errors - x and y should be inferred from contextual type
    let type_errors: Vec<_> = checker
        .ctx
        .diagnostics
        .iter()
        .filter(|d| d.code == 2322) // TS2322: Type is not assignable
        .collect();

    assert!(
        type_errors.is_empty(),
        "Expected no TS2322 errors when destructuring params get contextual types. Got: {type_errors:?}"
    );
}

/// Test that array destructuring parameters get contextual types from callback signatures
#[test]
fn test_contextual_typing_destructuring_param_array() {
    use crate::binder::BinderState;
    use crate::checker::state::CheckerState;
    use crate::parser::ParserState;
    use tsz_solver::TypeInterner;

    let source = r#"
type Handler = (item: [number, string]) => void;
const handler: Handler = ([first, second]) => {
    // first should be number, second should be string
    let numVal: number = first;
    let strVal: string = second;
};
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

    // Should have no type errors - first and second should be inferred from contextual type
    let type_errors: Vec<_> = checker
        .ctx
        .diagnostics
        .iter()
        .filter(|d| d.code == 2322) // TS2322: Type is not assignable
        .collect();

    assert!(
        type_errors.is_empty(),
        "Expected no TS2322 errors when array destructuring params get contextual types. Got: {type_errors:?}"
    );
}

// =============================================================================
// TS2322 Type Not Assignable - Comprehensive Tests
// =============================================================================

/// Test TS2322 emission for variable declaration with type annotation mismatch
#[test]
fn test_ts2322_variable_declaration_type_mismatch() {
    use crate::checker::diagnostics::diagnostic_codes;

    let source = r#"
let x: string = 42;
let y: number = "hello";
let z: boolean = null;
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

    let ts2322_errors: Vec<_> = checker
        .ctx
        .diagnostics
        .iter()
        .filter(|d| d.code == diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE)
        .collect();

    // Should have at least 2 errors (x and y - z may or may not depending on strictNullChecks)
    assert!(
        ts2322_errors.len() >= 2,
        "Expected at least 2 TS2322 errors for type mismatches. Got {}: {:?}",
        ts2322_errors.len(),
        ts2322_errors
    );
}

/// Test TS2322 emission for return statement type mismatch
#[test]
fn test_ts2322_return_statement_type_mismatch() {
    use crate::checker::diagnostics::diagnostic_codes;

    let source = r#"
function getString(): string {
    return 42;
}

function getNumber(): number {
    return "hello";
}

function getBoolean(): boolean {
    return {};
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

    let ts2322_errors: Vec<_> = checker
        .ctx
        .diagnostics
        .iter()
        .filter(|d| d.code == diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE)
        .collect();

    assert!(
        ts2322_errors.len() >= 3,
        "Expected at least 3 TS2322 errors for return type mismatches. Got {}: {:?}",
        ts2322_errors.len(),
        ts2322_errors
    );
}

/// Test TS2322 emission for class property initializer type mismatch
#[test]
fn test_ts2322_class_property_initializer_mismatch() {
    use crate::checker::diagnostics::diagnostic_codes;

    let source = r#"
class Example {
    stringProp: string = 42;
    numberProp: number = "hello";
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

    let ts2322_errors: Vec<_> = checker
        .ctx
        .diagnostics
        .iter()
        .filter(|d| d.code == diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE)
        .collect();

    assert!(
        ts2322_errors.len() >= 2,
        "Expected at least 2 TS2322 errors for class property initializer mismatches. Got {}: {:?}",
        ts2322_errors.len(),
        ts2322_errors
    );
}

/// Test TS2322 emission for object literal property type mismatch
#[test]
fn test_ts2322_object_literal_property_mismatch() {
    use crate::checker::diagnostics::diagnostic_codes;

    let source = r#"
interface Person {
    name: string;
    age: number;
}

const p: Person = {
    name: 123,
    age: "thirty"
};
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

    let ts2322_errors: Vec<_> = checker
        .ctx
        .diagnostics
        .iter()
        .filter(|d| d.code == diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE)
        .collect();

    // Object literal with mismatched property types should trigger TS2322
    assert!(
        !ts2322_errors.is_empty(),
        "Expected at least 1 TS2322 error for object literal property type mismatch. Got: {:?}",
        checker.ctx.diagnostics
    );
}

/// Test TS2322 emission for array element type mismatch
#[test]
fn test_ts2322_array_element_type_mismatch() {
    use crate::checker::diagnostics::diagnostic_codes;

    let source = r#"
const arr: number[] = [1, 2, "three", 4];
const arr2: string[] = ["a", "b", 3, "d"];
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

    let ts2322_errors: Vec<_> = checker
        .ctx
        .diagnostics
        .iter()
        .filter(|d| d.code == diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE)
        .collect();

    // Array literals with wrong element types should trigger TS2322
    assert!(
        ts2322_errors.len() >= 2,
        "Expected at least 2 TS2322 errors for array element type mismatches. Got {}: {:?}",
        ts2322_errors.len(),
        checker.ctx.diagnostics
    );
}

/// Test TS2322 is NOT emitted for valid assignments
#[test]
fn test_ts2322_valid_assignments_no_error() {
    use crate::checker::diagnostics::diagnostic_codes;

    let source = r#"
let x: string = "hello";
let y: number = 42;
let z: boolean = true;
let a: any = 123;
let b: unknown = "anything";

function getString(): string {
    return "valid";
}

function getNumber(): number {
    return 42;
}

class Valid {
    name: string = "test";
    count: number = 0;
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

    let ts2322_errors: Vec<_> = checker
        .ctx
        .diagnostics
        .iter()
        .filter(|d| d.code == diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE)
        .collect();

    assert!(
        ts2322_errors.is_empty(),
        "Expected no TS2322 errors for valid assignments. Got: {ts2322_errors:?}"
    );
}

/// Test TS2322 for function parameter default value mismatch
#[test]
fn test_ts2322_parameter_default_mismatch() {
    use crate::checker::diagnostics::diagnostic_codes;

    let source = r#"
function greet(name: string = 42) {
    return name;
}

function compute(value: number = "hello") {
    return value;
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

    let ts2322_errors: Vec<_> = checker
        .ctx
        .diagnostics
        .iter()
        .filter(|d| d.code == diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE)
        .collect();

    assert!(
        ts2322_errors.len() >= 2,
        "Expected at least 2 TS2322 errors for parameter default value mismatches. Got {}: {:?}",
        ts2322_errors.len(),
        ts2322_errors
    );
}

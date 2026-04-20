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
/// Test APISample-like pattern with noImplicitAny - simulates `compiler/APISample_Watch.ts`
/// Expected: 1 TS2307 (module), multiple TS7006 (implicit any params)
/// Note: We don't include `console.log` as that would emit TS2304 since console
/// isn't available without lib.d.ts
#[test]
fn test_apisample_pattern_errors() {
    use crate::checker::diagnostics::diagnostic_codes;
    use crate::parser::ParserState;

    // Pattern similar to APISample_Watch.ts
    let source = r#"
// @noImplicitAny: true
import * as ts from "typescript";

// Callback with no type annotation should produce TS7006
function watchFile(host: ts.WatchHost, callback): ts.Watch<ts.BuilderProgram> {
    return {} as any;
}

// More callbacks without types - each should produce TS7006
function createProgram(
    configFileName: string,
    reportDiagnostic,
    reportWatchStatus
): void {
    // Empty body to avoid using console (which might produce TS2304)
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
    checker.ctx.report_unresolved_imports = true;
    checker.check_source_file(root);

    let codes: Vec<u32> = checker.ctx.diagnostics.iter().map(|d| d.code).collect();
    let ts2304_count = codes
        .iter()
        .filter(|&&c| c == diagnostic_codes::CANNOT_FIND_NAME)
        .count();
    // Count module-not-found diagnostics (either TS2307 or TS2792)
    let module_not_found_count = codes
        .iter()
        .filter(|&&c| {
            c == diagnostic_codes::CANNOT_FIND_MODULE_OR_ITS_CORRESPONDING_TYPE_DECLARATIONS
                || c == diagnostic_codes::CANNOT_FIND_MODULE_DID_YOU_MEAN_TO_SET_THE_MODULERESOLUTION_OPTION_TO_NODENEXT_O
        })
        .count();
    let ts7006_count = codes.iter().filter(|&&c| c == 7006).count();

    // Should have exactly 1 module-not-found error for the unresolved module
    assert_eq!(
        module_not_found_count, 1,
        "Expected 1 TS2307/TS2792 for unresolved module, got {module_not_found_count}. All codes: {codes:?}"
    );

    // Should NOT have any TS2304 errors from ts.X references
    // (the module is unresolved, so ts.X should silently return ANY)
    assert_eq!(
        ts2304_count, 0,
        "Should not emit extra TS2304 for types from unresolved namespace import. All codes: {codes:?}"
    );

    // Should have TS7006 for parameters without type annotations
    // 3 parameters: callback, reportDiagnostic, reportWatchStatus
    assert_eq!(
        ts7006_count, 3,
        "Expected 3 TS7006 for implicit any parameters. All codes: {codes:?}"
    );
}

// =============================================================================
// TS2362/TS2363: Arithmetic Operand Type Checking Tests
// =============================================================================

#[test]
fn test_ts2362_left_hand_side_of_arithmetic() {
    use crate::checker::diagnostics::diagnostic_codes;
    use crate::parser::ParserState;

    let source = r#"
const str = "hello";
const result = str - 1;  // TS2362: left-hand side must be number/bigint/enum
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
    let ts2362_count = codes
        .iter()
        .filter(|&&c| c == diagnostic_codes::THE_LEFT_HAND_SIDE_OF_AN_ARITHMETIC_OPERATION_MUST_BE_OF_TYPE_ANY_NUMBER_BIGINT)
        .count();

    assert_eq!(
        ts2362_count, 1,
        "Expected 1 TS2362 for string - number. All codes: {codes:?}"
    );
}

#[test]
fn test_ts2363_right_hand_side_of_arithmetic() {
    use crate::checker::diagnostics::diagnostic_codes;
    use crate::parser::ParserState;

    let source = r#"
const num = 10;
const str = "hello";
const result = num - str;  // TS2363: right-hand side must be number/bigint/enum
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
    let ts2363_count = codes
        .iter()
        .filter(|&&c| c == diagnostic_codes::THE_RIGHT_HAND_SIDE_OF_AN_ARITHMETIC_OPERATION_MUST_BE_OF_TYPE_ANY_NUMBER_BIGINT)
        .count();

    assert_eq!(
        ts2363_count, 1,
        "Expected 1 TS2363 for number - string. All codes: {codes:?}"
    );
}

#[test]
fn test_ts2362_ts2363_both_operands_invalid() {
    use crate::checker::diagnostics::diagnostic_codes;
    use crate::parser::ParserState;

    let source = r#"
const a = "hello";
const b = "world";
const result = a * b;  // TS2362 and TS2363: both operands invalid
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
    let ts2362_count = codes
        .iter()
        .filter(|&&c| c == diagnostic_codes::THE_LEFT_HAND_SIDE_OF_AN_ARITHMETIC_OPERATION_MUST_BE_OF_TYPE_ANY_NUMBER_BIGINT)
        .count();
    let ts2363_count = codes
        .iter()
        .filter(|&&c| c == diagnostic_codes::THE_RIGHT_HAND_SIDE_OF_AN_ARITHMETIC_OPERATION_MUST_BE_OF_TYPE_ANY_NUMBER_BIGINT)
        .count();

    assert_eq!(
        ts2362_count, 1,
        "Expected 1 TS2362 for left string operand. All codes: {codes:?}"
    );
    assert_eq!(
        ts2363_count, 1,
        "Expected 1 TS2363 for right string operand. All codes: {codes:?}"
    );
}

#[test]
fn test_arithmetic_valid_with_number_types() {
    use crate::checker::diagnostics::diagnostic_codes;
    use crate::parser::ParserState;

    let source = r#"
const a = 10;
const b = 20;
const result1 = a - b;
const result2 = a * b;
const result3 = a / b;
const result4 = a % b;
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
    let ts2362_count = codes
        .iter()
        .filter(|&&c| c == diagnostic_codes::THE_LEFT_HAND_SIDE_OF_AN_ARITHMETIC_OPERATION_MUST_BE_OF_TYPE_ANY_NUMBER_BIGINT)
        .count();
    let ts2363_count = codes
        .iter()
        .filter(|&&c| c == diagnostic_codes::THE_RIGHT_HAND_SIDE_OF_AN_ARITHMETIC_OPERATION_MUST_BE_OF_TYPE_ANY_NUMBER_BIGINT)
        .count();

    assert_eq!(
        ts2362_count, 0,
        "Expected no TS2362 errors for valid number arithmetic. All codes: {codes:?}"
    );
    assert_eq!(
        ts2363_count, 0,
        "Expected no TS2363 errors for valid number arithmetic. All codes: {codes:?}"
    );
}

#[test]
fn test_arithmetic_valid_with_any_type() {
    use crate::checker::diagnostics::diagnostic_codes;
    use crate::parser::ParserState;

    let source = r#"
declare const anyVal: any;
const result1 = anyVal - 1;
const result2 = 1 * anyVal;
const result3 = anyVal / anyVal;
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
    let ts2362_count = codes
        .iter()
        .filter(|&&c| c == diagnostic_codes::THE_LEFT_HAND_SIDE_OF_AN_ARITHMETIC_OPERATION_MUST_BE_OF_TYPE_ANY_NUMBER_BIGINT)
        .count();
    let ts2363_count = codes
        .iter()
        .filter(|&&c| c == diagnostic_codes::THE_RIGHT_HAND_SIDE_OF_AN_ARITHMETIC_OPERATION_MUST_BE_OF_TYPE_ANY_NUMBER_BIGINT)
        .count();

    assert_eq!(
        ts2362_count, 0,
        "Expected no TS2362 errors when using 'any' type. All codes: {codes:?}"
    );
    assert_eq!(
        ts2363_count, 0,
        "Expected no TS2363 errors when using 'any' type. All codes: {codes:?}"
    );
}

#[test]
fn test_arithmetic_valid_with_bigint() {
    use crate::checker::diagnostics::diagnostic_codes;
    use crate::parser::ParserState;

    let source = r#"
const a: bigint = 10n;
const b: bigint = 20n;
const result1 = a - b;
const result2 = a * b;
const result3 = a / b;
const result4 = a % b;
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
    let ts2362_count = codes
        .iter()
        .filter(|&&c| c == diagnostic_codes::THE_LEFT_HAND_SIDE_OF_AN_ARITHMETIC_OPERATION_MUST_BE_OF_TYPE_ANY_NUMBER_BIGINT)
        .count();
    let ts2363_count = codes
        .iter()
        .filter(|&&c| c == diagnostic_codes::THE_RIGHT_HAND_SIDE_OF_AN_ARITHMETIC_OPERATION_MUST_BE_OF_TYPE_ANY_NUMBER_BIGINT)
        .count();

    assert_eq!(
        ts2362_count, 0,
        "Expected no TS2362 errors for valid bigint arithmetic. All codes: {codes:?}"
    );
    assert_eq!(
        ts2363_count, 0,
        "Expected no TS2363 errors for valid bigint arithmetic. All codes: {codes:?}"
    );
}

#[test]
fn test_arithmetic_valid_with_enum() {
    use crate::checker::diagnostics::diagnostic_codes;
    use crate::parser::ParserState;

    // Note: This test is ignored because enum member type resolution
    // doesn't currently return the numeric literal types that would
    // allow the is_arithmetic_operand check to pass.
    // The is_arithmetic_operand method correctly handles unions of
    // number literals (which is how enum types are represented),
    // but the checker needs to properly resolve enum member values
    // to their numeric literal types first.
    let source = r#"
enum Direction {
    Up = 1,
    Down = 2,
    Left = 3,
    Right = 4
}
const a = Direction.Up;
const b = Direction.Down;
const result = a - b;
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
    let ts2362_count = codes
        .iter()
        .filter(|&&c| c == diagnostic_codes::THE_LEFT_HAND_SIDE_OF_AN_ARITHMETIC_OPERATION_MUST_BE_OF_TYPE_ANY_NUMBER_BIGINT)
        .count();
    let ts2363_count = codes
        .iter()
        .filter(|&&c| c == diagnostic_codes::THE_RIGHT_HAND_SIDE_OF_AN_ARITHMETIC_OPERATION_MUST_BE_OF_TYPE_ANY_NUMBER_BIGINT)
        .count();

    assert_eq!(
        ts2362_count, 0,
        "Expected no TS2362 errors for valid enum arithmetic. All codes: {codes:?}"
    );
    assert_eq!(
        ts2363_count, 0,
        "Expected no TS2363 errors for valid enum arithmetic. All codes: {codes:?}"
    );
}

#[test]
fn test_ts2362_with_boolean() {
    use crate::checker::diagnostics::diagnostic_codes;
    use crate::parser::ParserState;

    let source = r#"
const flag = true;
const result = flag - 1;  // TS2362: boolean is not a valid arithmetic operand
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
    let ts2362_count = codes
        .iter()
        .filter(|&&c| c == diagnostic_codes::THE_LEFT_HAND_SIDE_OF_AN_ARITHMETIC_OPERATION_MUST_BE_OF_TYPE_ANY_NUMBER_BIGINT)
        .count();

    assert_eq!(
        ts2362_count, 1,
        "Expected 1 TS2362 for boolean - number. All codes: {codes:?}"
    );
}

#[test]
fn test_ts2363_with_object() {
    use crate::checker::diagnostics::diagnostic_codes;
    use crate::parser::ParserState;

    let source = r#"
const obj = { x: 1 };
const result = 10 / obj;  // TS2363: object is not a valid arithmetic operand
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
    let ts2363_count = codes
        .iter()
        .filter(|&&c| c == diagnostic_codes::THE_RIGHT_HAND_SIDE_OF_AN_ARITHMETIC_OPERATION_MUST_BE_OF_TYPE_ANY_NUMBER_BIGINT)
        .count();

    assert_eq!(
        ts2363_count, 1,
        "Expected 1 TS2363 for number / object. All codes: {codes:?}"
    );
}


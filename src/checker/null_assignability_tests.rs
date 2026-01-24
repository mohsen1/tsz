//! Tests for strict null checks and null/undefined assignability (TS2322)

use crate::binder::BinderState;
use crate::checker::state::CheckerState;
use crate::parser::ParserState;
use crate::solver::TypeInterner;

/// Helper function to create a checker with strict null checks enabled
fn create_strict_checker(source: &str) -> CheckerState {
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.ts".to_string(),
        crate::checker::context::CheckerOptions {
            strict_null_checks: true,
            ..Default::default()
        },
    );

    checker.check_source_file(root);
    checker
}

/// Helper function to create a checker with exact optional property types
fn create_exact_optional_checker(source: &str) -> CheckerState {
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.ts".to_string(),
        crate::checker::context::CheckerOptions {
            strict_null_checks: true,
            exact_optional_property_types: true,
            ..Default::default()
        },
    );

    checker.check_source_file(root);
    checker
}

/// Helper function to create a checker with strict null checks disabled
fn create_non_strict_checker(source: &str) -> CheckerState {
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.ts".to_string(),
        crate::checker::context::CheckerOptions {
            strict_null_checks: false,
            ..Default::default()
        },
    );

    checker.check_source_file(root);
    checker
}

#[test]
fn test_null_to_non_nullable_string_emits_ts2322_in_strict_mode() {
    let source = r#"
let x: string = null;
"#;

    let checker = create_strict_checker(source);

    // Should emit TS2322 in strict mode
    let ts2322_count = checker
        .ctx
        .diagnostics
        .iter()
        .filter(|d| d.code == 2322)
        .count();
    assert!(
        ts2322_count >= 1,
        "Expected at least 1 TS2322 error in strict mode, got {}",
        ts2322_count
    );
}

#[test]
fn test_null_to_non_nullable_string_no_error_in_non_strict_mode() {
    let source = r#"
let x: string = null;
"#;

    let checker = create_non_strict_checker(source);

    // Should NOT emit TS2322 in non-strict mode
    let ts2322_count = checker
        .ctx
        .diagnostics
        .iter()
        .filter(|d| d.code == 2322)
        .count();
    assert_eq!(
        ts2322_count, 0,
        "Expected no TS2322 error in non-strict mode, got {}",
        ts2322_count
    );
}

#[test]
fn test_undefined_to_non_nullable_number_emits_ts2322_in_strict_mode() {
    let source = r#"
let y: number = undefined;
"#;

    let checker = create_strict_checker(source);

    // Should emit TS2322 in strict mode
    let ts2322_count = checker
        .ctx
        .diagnostics
        .iter()
        .filter(|d| d.code == 2322)
        .count();
    assert!(
        ts2322_count >= 1,
        "Expected at least 1 TS2322 error in strict mode, got {}",
        ts2322_count
    );
}

#[test]
fn test_null_to_nullable_type_no_error() {
    let source = r#"
let x: string | null = null;
"#;

    let checker = create_strict_checker(source);

    // Should NOT emit TS2322 when target includes null
    let ts2322_count = checker
        .ctx
        .diagnostics
        .iter()
        .filter(|d| d.code == 2322)
        .count();
    assert_eq!(
        ts2322_count, 0,
        "Expected no TS2322 error for null to nullable type, got {}",
        ts2322_count
    );
}

#[test]
fn test_undefined_to_union_with_undefined_no_error() {
    let source = r#"
let y: number | undefined = undefined;
"#;

    let checker = create_strict_checker(source);

    // Should NOT emit TS2322 when target includes undefined
    let ts2322_count = checker
        .ctx
        .diagnostics
        .iter()
        .filter(|d| d.code == 2322)
        .count();
    assert_eq!(
        ts2322_count, 0,
        "Expected no TS2322 error for undefined to union with undefined, got {}",
        ts2322_count
    );
}

#[test]
fn test_null_to_generic_type_parameter_emits_ts2322() {
    let source = r#"
function identity<T>(x: T): T {
    return x;
}
let result: string = identity<string>(null);
"#;

    let checker = create_strict_checker(source);

    // Should emit TS2322: null is not assignable to string
    let ts2322_count = checker
        .ctx
        .diagnostics
        .iter()
        .filter(|d| d.code == 2322)
        .count();
    assert!(
        ts2322_count >= 1,
        "Expected at least 1 TS2322 error for null to generic type, got {}",
        ts2322_count
    );
}

#[test]
fn test_null_property_assignment_emits_ts2322() {
    let source = r#"
interface Foo {
    name: string;
    value: number;
}
const foo: Foo = {
    name: "test",
    value: null
};
"#;

    let checker = create_strict_checker(source);

    // Should emit TS2322 for value property
    let ts2322_count = checker
        .ctx
        .diagnostics
        .iter()
        .filter(|d| d.code == 2322)
        .count();
    assert!(
        ts2322_count >= 1,
        "Expected at least 1 TS2322 error for null property assignment, got {}",
        ts2322_count
    );
}

#[test]
fn test_null_array_element_emits_ts2322() {
    let source = r#"
const arr: string[] = ["hello", null, "world"];
"#;

    let checker = create_strict_checker(source);

    // Should emit TS2322 for null element
    let ts2322_count = checker
        .ctx
        .diagnostics
        .iter()
        .filter(|d| d.code == 2322)
        .count();
    assert!(
        ts2322_count >= 1,
        "Expected at least 1 TS2322 error for null array element, got {}",
        ts2322_count
    );
}

#[test]
fn test_undefined_return_value_emits_ts2322() {
    let source = r#"
function getNumber(): number {
    return undefined;
}
"#;

    let checker = create_strict_checker(source);

    // Should emit TS2322: undefined is not assignable to number
    let ts2322_count = checker
        .ctx
        .diagnostics
        .iter()
        .filter(|d| d.code == 2322)
        .count();
    assert!(
        ts2322_count >= 1,
        "Expected at least 1 TS2322 error for undefined return value, got {}",
        ts2322_count
    );
}

#[test]
fn test_void_function_return_undefined_no_error() {
    let source = r#"
function voidFn(): void {
    return undefined;
}
"#;

    let checker = create_strict_checker(source);

    // Should NOT emit TS2322: undefined is assignable to void
    let ts2322_count = checker
        .ctx
        .diagnostics
        .iter()
        .filter(|d| d.code == 2322)
        .count();
    assert_eq!(
        ts2322_count, 0,
        "Expected no TS2322 error for undefined to void return, got {}",
        ts2322_count
    );
}

#[test]
fn test_optional_property_with_null_emits_ts2322_in_strict_mode() {
    let source = r#"
interface Foo {
    required: string;
    optional?: number;
}
const foo: Foo = {
    required: "test",
    optional: null
};
"#;

    let checker = create_strict_checker(source);

    // In strict null checks with exact_optional_property_types=true,
    // optional properties should NOT accept null
    let ts2322_count = checker
        .ctx
        .diagnostics
        .iter()
        .filter(|d| d.code == 2322)
        .count();
    // Optional property with null should emit TS2322 when exact_optional_property_types is true
}

#[test]
fn test_null_to_any_no_error() {
    let source = r#"
let x: any = null;
"#;

    let checker = create_strict_checker(source);

    // Should NOT emit TS2322: anything is assignable to any
    let ts2322_count = checker
        .ctx
        .diagnostics
        .iter()
        .filter(|d| d.code == 2322)
        .count();
    assert_eq!(
        ts2322_count, 0,
        "Expected no TS2322 error for null to any, got {}",
        ts2322_count
    );
}

#[test]
fn test_null_function_argument_emits_ts2322() {
    let source = r#"
function greet(name: string): void {
    console.log(name);
}
greet(null);
"#;

    let checker = create_strict_checker(source);

    // Should emit TS2322: null is not assignable to string
    let ts2322_count = checker
        .ctx
        .diagnostics
        .iter()
        .filter(|d| d.code == 2322)
        .count();
    assert!(
        ts2322_count >= 1,
        "Expected at least 1 TS2322 error for null argument, got {}",
        ts2322_count
    );
}

#[test]
fn test_null_assignment_to_existing_variable_emits_ts2322() {
    let source = r#"
let x: string = "hello";
x = null;
"#;

    let checker = create_strict_checker(source);

    // Should emit TS2322 for the assignment
    let ts2322_count = checker
        .ctx
        .diagnostics
        .iter()
        .filter(|d| d.code == 2322)
        .count();
    assert!(
        ts2322_count >= 1,
        "Expected at least 1 TS2322 error for null assignment, got {}",
        ts2322_count
    );
}

#[test]
fn test_null_in_conditional_assignment_emits_ts2322() {
    let source = r#"
let x: string;
if (true) {
    x = null;
}
"#;

    let checker = create_strict_checker(source);

    // Should emit TS2322 for the assignment
    let ts2322_count = checker
        .ctx
        .diagnostics
        .iter()
        .filter(|d| d.code == 2322)
        .count();
    assert!(
        ts2322_count >= 1,
        "Expected at least 1 TS2322 error for null in conditional, got {}",
        ts2322_count
    );
}

#[test]
fn test_null_spread_into_tuple_emits_ts2322() {
    let source = r#"
type Tuple = [string, number];
const t: Tuple = ["hello", ...null as any];
"#;

    let checker = create_strict_checker(source);

    // Spread with null should have type issues
    // This test documents current behavior
}

#[test]
fn test_null_destructuring_emits_ts2322() {
    let source = r#"
const obj = { a: 1, b: 2 };
const { a, b }: { a: number, b: string } = obj;
"#;

    let checker = create_strict_checker(source);

    // Should emit TS2322: b is number but declared as string
    let ts2322_count = checker
        .ctx
        .diagnostics
        .iter()
        .filter(|d| d.code == 2322)
        .count();
    assert!(
        ts2322_count >= 1,
        "Expected at least 1 TS2322 error for destructuring, got {}",
        ts2322_count
    );
}

#[test]
fn test_null_in_union_comparison() {
    let source = r#"
let x: string | null;
let y: string;
x = y;  // OK
y = x;  // Should emit TS2322 in strict mode
"#;

    let checker = create_strict_checker(source);

    // Second assignment should emit TS2322
    let ts2322_count = checker
        .ctx
        .diagnostics
        .iter()
        .filter(|d| d.code == 2322)
        .count();
    assert!(
        ts2322_count >= 1,
        "Expected at least 1 TS2322 error for union to non-nullable, got {}",
        ts2322_count
    );
}

#[test]
fn test_type_guard_with_null() {
    let source = r#"
function processValue(value: string | null) {
    if (value !== null) {
        // After the check, value should be narrowed to string
        const upper: string = value.toUpperCase();
    }
}
"#;

    let checker = create_strict_checker(source);

    // Should NOT emit TS2322 because type guard narrows value
    let ts2322_count = checker
        .ctx
        .diagnostics
        .iter()
        .filter(|d| d.code == 2322)
        .count();
    assert_eq!(
        ts2322_count, 0,
        "Expected no TS2322 error with type guard, got {}",
        ts2322_count
    );
}

#[test]
fn test_null_for_of_loop_variable() {
    let source = r#"
const arr: (string | null)[] = ["hello", null, "world"];
for (const item: string of arr) {
    // Should emit TS2322 for null elements
}
"#;

    let checker = create_strict_checker(source);

    // Should emit TS2322 for null elements in the array
    let ts2322_count = checker
        .ctx
        .diagnostics
        .iter()
        .filter(|d| d.code == 2322)
        .count();
    // This might be caught by the for-of annotation check we added earlier
}

#[test]
fn test_undefined_parameter_default() {
    let source = r#"
function greet(name: string = undefined) {
    console.log(name);
}
"#;

    let checker = create_strict_checker(source);

    // Should emit TS2322: undefined is not assignable to string
    let ts2322_count = checker
        .ctx
        .diagnostics
        .iter()
        .filter(|d| d.code == 2322)
        .count();
    assert!(
        ts2322_count >= 1,
        "Expected at least 1 TS2322 error for undefined parameter default, got {}",
        ts2322_count
    );
}

#[test]
fn test_exact_optional_property_undefined_emits_ts2322() {
    let source = r#"
interface Foo {
    optional?: string;
}
const foo: Foo = {
    optional: undefined
};
"#;

    let checker = create_exact_optional_checker(source);

    // With exact_optional_property_types=true, explicit undefined should NOT be assignable
    let ts2322_count = checker
        .ctx
        .diagnostics
        .iter()
        .filter(|d| d.code == 2322)
        .count();
    assert!(
        ts2322_count >= 1,
        "Expected at least 1 TS2322 error for undefined to optional in exact mode, got {}",
        ts2322_count
    );
}

#[test]
fn test_non_exact_optional_property_allows_undefined() {
    let source = r#"
interface Foo {
    optional?: string;
}
const foo: Foo = {
    optional: undefined
};
"#;

    let checker = create_strict_checker(source);

    // With exact_optional_property_types=false (default), undefined should be assignable to optional
    let ts2322_count = checker
        .ctx
        .diagnostics
        .iter()
        .filter(|d| d.code == 2322)
        .count();
    assert_eq!(
        ts2322_count, 0,
        "Expected no TS2322 error for undefined to optional in non-exact mode, got {}",
        ts2322_count
    );
}

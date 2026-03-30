//! Tests for TS2693 (type-only as value) and TS2362/TS2363 (arithmetic operand errors)

use crate::state::CheckerState;
use tsz_binder::BinderState;
use tsz_parser::parser::ParserState;
use tsz_solver::TypeInterner;

#[test]
fn test_interface_used_as_value() {
    let source = r"
interface Foo {
    a: number;
}
const x = new Foo();
";
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
        crate::context::CheckerOptions::default(),
    );

    checker.check_source_file(root);

    // Should emit TS2693 for using interface as value
    let ts2693_count = checker
        .ctx
        .diagnostics
        .iter()
        .filter(|d| d.code == 2693)
        .count();
    assert!(
        ts2693_count >= 1,
        "Expected at least 1 TS2693 error, got {ts2693_count}"
    );
}

#[test]
fn test_type_alias_used_as_value() {
    let source = r"
type Foo = {
    a: number;
};
const x = new Foo();
";
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
        crate::context::CheckerOptions::default(),
    );

    checker.check_source_file(root);

    // Should emit TS2693 for using type alias as value
    let ts2693_count = checker
        .ctx
        .diagnostics
        .iter()
        .filter(|d| d.code == 2693)
        .count();
    assert!(
        ts2693_count >= 1,
        "Expected at least 1 TS2693 error, got {ts2693_count}"
    );
}

/// For `number[]` parse-recovery, tsc only emits TS1011 (missing element access argument),
/// NOT TS2693. The parse error is sufficient — no need for a semantic "type used as value" error.
#[test]
fn test_primitive_array_type_recovery_no_ts2693() {
    let source = r"
var results = number[];
";
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
        crate::context::CheckerOptions::default(),
    );

    checker.check_source_file(root);

    let ts2693_count = checker
        .ctx
        .diagnostics
        .iter()
        .filter(|d| d.code == 2693)
        .count();
    assert!(
        ts2693_count == 0,
        "Should NOT emit TS2693 for `number[]` parse-recovery (TS1011 is sufficient), got {ts2693_count}"
    );
}

#[test]
fn test_string_subtraction_emits_ts2362() {
    let source = r#"
const str = "hello";
const result = str - 5;
"#;
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
        crate::context::CheckerOptions::default(),
    );

    checker.check_source_file(root);

    // Should emit TS2362 for left-hand side of - operation
    let ts2362_count = checker
        .ctx
        .diagnostics
        .iter()
        .filter(|d| d.code == 2362)
        .count();
    assert!(
        ts2362_count >= 1,
        "Expected at least 1 TS2362 error, got {ts2362_count}"
    );
}

#[test]
fn test_boolean_multiplication_emits_ts2362() {
    let source = r"
const flag = true;
const result = flag * 10;
";
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
        crate::context::CheckerOptions::default(),
    );

    checker.check_source_file(root);

    // Should emit TS2362 for left-hand side of * operation
    let ts2362_count = checker
        .ctx
        .diagnostics
        .iter()
        .filter(|d| d.code == 2362)
        .count();
    assert!(
        ts2362_count >= 1,
        "Expected at least 1 TS2362 error, got {ts2362_count}"
    );
}

#[test]
fn test_number_divided_by_string_emits_ts2363() {
    let source = r#"
const num = 10;
const str = "hello";
const result = num / str;
"#;
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
        crate::context::CheckerOptions::default(),
    );

    checker.check_source_file(root);

    // Should emit TS2363 for right-hand side of / operation
    let ts2363_count = checker
        .ctx
        .diagnostics
        .iter()
        .filter(|d| d.code == 2363)
        .count();
    assert!(
        ts2363_count >= 1,
        "Expected at least 1 TS2363 error, got {ts2363_count}"
    );
}

#[test]
fn test_arithmetic_on_non_numeric_types() {
    let source = r"
const obj = { a: 1 };
const arr = [1, 2, 3];
const r1 = obj - 1;  // TS2362
const r2 = 10 * arr;  // TS2363
const r3 = obj % 2;  // TS2362
";
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
        crate::context::CheckerOptions::default(),
    );

    checker.check_source_file(root);

    // Should emit multiple TS2362 and TS2363 errors
    let ts2362_count = checker
        .ctx
        .diagnostics
        .iter()
        .filter(|d| d.code == 2362)
        .count();
    let ts2363_count = checker
        .ctx
        .diagnostics
        .iter()
        .filter(|d| d.code == 2363)
        .count();

    assert!(
        ts2362_count >= 2,
        "Expected at least 2 TS2362 errors, got {ts2362_count}"
    );
    assert!(
        ts2363_count >= 1,
        "Expected at least 1 TS2363 error, got {ts2363_count}"
    );
}

#[test]
fn test_valid_arithmetic_no_errors() {
    let source = r"
const a = 10;
const b = 5;
const r1 = a + b;  // OK - number addition
const r2 = a - b;  // OK - number subtraction
const r3 = a * b;  // OK - number multiplication
const r4 = a / b;  // OK - number division
const r5 = a % b;  // OK - number modulo
";
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
        crate::context::CheckerOptions::default(),
    );

    checker.check_source_file(root);

    // Should not emit TS2362 or TS2363 for valid arithmetic
    let error_count = checker
        .ctx
        .diagnostics
        .iter()
        .filter(|d| d.code == 2362 || d.code == 2363)
        .count();
    assert_eq!(
        error_count, 0,
        "Expected no TS2362/TS2363 errors, got {error_count}"
    );
}

#[test]
fn test_for_of_variable_type_annotation_emits_ts2322() {
    let source = r"
const numbers = [1, 2, 3];
for (const x: string of numbers) {
    // Should emit TS2322: number is not assignable to string
}
";
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
        crate::context::CheckerOptions::default(),
    );

    checker.check_source_file(root);

    // Should emit TS2322 for for-of variable with incompatible type annotation
    let ts2322_count = checker
        .ctx
        .diagnostics
        .iter()
        .filter(|d| d.code == 2322)
        .count();
    assert!(
        ts2322_count >= 1,
        "Expected at least 1 TS2322 error, got {ts2322_count}"
    );
}

#[test]
fn test_for_of_variable_compatible_type_no_error() {
    let source = r"
const numbers = [1, 2, 3];
for (const x: number of numbers) {
    // OK - number is assignable to number
}
";
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
        crate::context::CheckerOptions::default(),
    );

    checker.check_source_file(root);

    // Should not emit TS2322 for compatible types
    let ts2322_count = checker
        .ctx
        .diagnostics
        .iter()
        .filter(|d| d.code == 2322)
        .count();
    assert_eq!(
        ts2322_count, 0,
        "Expected no TS2322 errors, got {ts2322_count}"
    );
}

#[test]
fn test_type_import_used_as_value() {
    // Test that type-only imports emit TS1361 when used as values
    // Note: In single-file mode, the imported module doesn't exist, so the
    // binder may not fully resolve the symbol. This test verifies that when
    // the symbol IS resolved, TS1361 is emitted instead of TS2693.
    let source = r"
import type { Foo } from './foo';
const x = new Foo();  // TS1361: Foo cannot be used as a value because it was imported using 'import type'
";
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
        crate::context::CheckerOptions::default(),
    );

    checker.check_source_file(root);

    // In single-file mode, either TS1361 (type-only import as value) or
    // TS2693 (type used as value) should be emitted, depending on symbol resolution
    let type_value_errors: Vec<_> = checker
        .ctx
        .diagnostics
        .iter()
        .filter(|d| d.code == 1361 || d.code == 2693)
        .collect();
    assert!(
        !type_value_errors.is_empty() || {
            // Acceptable if no error when module can't be resolved in single-file mode
            checker.ctx.diagnostics.iter().any(|d| d.code == 2307)
        },
        "Expected TS1361/TS2693 or TS2307 (module not found), got: {:?}",
        checker
            .ctx
            .diagnostics
            .iter()
            .map(|d| d.code)
            .collect::<Vec<_>>()
    );
}

#[test]
fn test_interface_property_access_emits_ts18050() {
    // Test accessing interface as if it were an object with properties
    let source = r"
interface MyInterface {
    prop: string;
}
const x = MyInterface.prop;  // TS2693: MyInterface only refers to a type
";
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
        crate::context::CheckerOptions::default(),
    );

    checker.check_source_file(root);

    // Should emit TS2693 for using interface as value
    let ts2693_count = checker
        .ctx
        .diagnostics
        .iter()
        .filter(|d| d.code == 2693)
        .count();
    assert!(
        ts2693_count >= 1,
        "Expected at least 1 TS2693 error for interface property access, got {ts2693_count}"
    );
}

#[test]
fn test_exponentiation_with_string_emits_ts2362() {
    // Test ** operator with string operand
    let source = r#"
const base = "2";
const exp = 3;
const result = base ** exp;  // TS2362: base is string, not number
"#;
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
        crate::context::CheckerOptions::default(),
    );

    checker.check_source_file(root);

    // Should emit TS2362 for string operand in **
    let ts2362_count = checker
        .ctx
        .diagnostics
        .iter()
        .filter(|d| d.code == 2362)
        .count();
    assert!(
        ts2362_count >= 1,
        "Expected at least 1 TS2362 error for exponentiation, got {ts2362_count}"
    );
}

#[test]
fn test_bitwise_operations_with_invalid_operands() {
    // Test bitwise operators (&, |, ^, <<, >>, >>>) with non-integer types
    let source = r#"
const str = "test";
const obj = { a: 1 };
const r1 = str & 5;      // TS2362
const r2 = 10 | obj;     // TS2363
const r3 = obj ^ 2;      // TS2362
const r4 = 5 << str;     // TS2363
const r5 = str >> 1;     // TS2362
const r6 = 10 >>> obj;   // TS2363
"#;
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
        crate::context::CheckerOptions::default(),
    );

    checker.check_source_file(root);

    // Should emit multiple TS2362 and TS2363 errors
    let ts2362_count = checker
        .ctx
        .diagnostics
        .iter()
        .filter(|d| d.code == 2362)
        .count();
    let ts2363_count = checker
        .ctx
        .diagnostics
        .iter()
        .filter(|d| d.code == 2363)
        .count();

    assert!(
        ts2362_count >= 3,
        "Expected at least 3 TS2362 errors for bitwise operations, got {ts2362_count}"
    );
    assert!(
        ts2363_count >= 3,
        "Expected at least 3 TS2363 errors for bitwise operations, got {ts2363_count}"
    );
}

#[test]
fn test_string_plus_number_no_error() {
    // Test that string + number is valid (string concatenation)
    let source = r#"
const str = "hello";
const num = 42;
const result = str + num;  // OK: string concatenation
"#;
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
        crate::context::CheckerOptions::default(),
    );

    checker.check_source_file(root);

    // Should not emit TS2362/TS2363 for string + number (valid concatenation)
    let error_count = checker
        .ctx
        .diagnostics
        .iter()
        .filter(|d| d.code == 2362 || d.code == 2363)
        .count();
    assert_eq!(
        error_count, 0,
        "Expected no TS2362/TS2363 errors for string + number, got {error_count}"
    );
}

#[test]
fn test_enum_arithmetic_valid() {
    // Test that enum members can be used in arithmetic
    let source = r"
enum MyEnum {
    A = 0,
    B = 1,
    C = 2,
}
const result = MyEnum.A + MyEnum.B;  // OK: enum arithmetic is valid
";
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
        crate::context::CheckerOptions::default(),
    );

    checker.check_source_file(root);

    // Should not emit TS2362/TS2363 for enum arithmetic
    let error_count = checker
        .ctx
        .diagnostics
        .iter()
        .filter(|d| d.code == 2362 || d.code == 2363)
        .count();
    assert_eq!(
        error_count, 0,
        "Expected no TS2362/TS2363 errors for enum arithmetic, got {error_count}"
    );
}

#[test]
fn test_null_property_access_emits_ts18050() {
    // Test accessing property on null literal - should emit TS18050
    let source = r"
const x = null.toString();  // TS18050: The value 'null' cannot be used here.
";
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
        crate::context::CheckerOptions::default(),
    );

    checker.check_source_file(root);

    // Should emit TS18050 for null.toString()
    let ts18050_count = checker
        .ctx
        .diagnostics
        .iter()
        .filter(|d| d.code == 18050)
        .count();
    assert!(
        ts18050_count >= 1,
        "Expected at least 1 TS18050 error for null property access, got {ts18050_count}"
    );
}

#[test]
fn test_undefined_property_access_emits_ts18050() {
    // Test accessing property on undefined - should emit TS18050
    let source = r"
const x = undefined.toString();  // TS18050: The value 'undefined' cannot be used here.
";
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
        crate::context::CheckerOptions::default(),
    );

    checker.check_source_file(root);

    // Should emit TS18050 for undefined.toString()
    let ts18050_count = checker
        .ctx
        .diagnostics
        .iter()
        .filter(|d| d.code == 18050)
        .count();
    assert!(
        ts18050_count >= 1,
        "Expected at least 1 TS18050 error for undefined property access, got {ts18050_count}"
    );
}

#[test]
fn test_string_string_subtraction_emits_ts2362() {
    // Test string - string should emit TS2362
    let source = r#"
const a = "hello";
const b = "world";
const result = a - b;  // TS2362: left-hand side must be number/bigint/any/enum
"#;
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
        crate::context::CheckerOptions::default(),
    );

    checker.check_source_file(root);

    // Should emit TS2362 for string - string
    let ts2362_count = checker
        .ctx
        .diagnostics
        .iter()
        .filter(|d| d.code == 2362)
        .count();
    assert!(
        ts2362_count >= 1,
        "Expected at least 1 TS2362 error for string subtraction, got {ts2362_count}"
    );
}

#[test]
fn test_never_type_property_access_no_error() {
    // In TSC, property access on `never` returns `never` without any error.
    // `never` is the bottom type — accessing properties on it is valid because
    // it represents unreachable code (exhaustive narrowing patterns).
    let source = r"
function test(x: never) {
    const y = x.toString();
}
";
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
        crate::context::CheckerOptions::default(),
    );

    checker.check_source_file(root);

    // TSC does NOT emit TS18050 for property access on never — it returns never
    let ts18050_count = checker
        .ctx
        .diagnostics
        .iter()
        .filter(|d| d.code == 18050)
        .count();
    assert_eq!(
        ts18050_count, 0,
        "Property access on never should NOT emit TS18050 (TSC returns never silently)"
    );
}

#[test]
fn test_never_type_call_no_error() {
    // In TSC, calling `never` returns `never` without any error.
    let source = r"
function test(x: never) {
    const y = x();
}
";
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
        crate::context::CheckerOptions::default(),
    );

    checker.check_source_file(root);

    // TSC does NOT emit TS18050 for calling never — it returns never
    let ts18050_count = checker
        .ctx
        .diagnostics
        .iter()
        .filter(|d| d.code == 18050)
        .count();
    assert_eq!(
        ts18050_count, 0,
        "Calling never should NOT emit TS18050 (TSC returns never silently)"
    );
}

#[test]
fn test_array_object_prototype_properties() {
    // Arrays should have access to Object.prototype properties like constructor,
    // valueOf, hasOwnProperty, etc. (through Array<T> → Object prototype chain)
    let source = r#"
var arr: number[] = [1, 2, 3];
arr.constructor;
arr.valueOf();
arr.hasOwnProperty("length");
"#;
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
        crate::context::CheckerOptions::default(),
    );

    checker.check_source_file(root);

    // Should have no TS2339 errors — these properties exist on Object.prototype
    let ts2339_count = checker
        .ctx
        .diagnostics
        .iter()
        .filter(|d| d.code == 2339)
        .count();
    assert!(
        ts2339_count == 0,
        "Expected no TS2339 errors for array Object.prototype properties, got {ts2339_count}"
    );
}

#[test]
fn test_shorthand_property_missing_value_emits_ts18004() {
    let source = r"
const make = () => {
    return { arguments };
};
";
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
        crate::context::CheckerOptions::default(),
    );

    checker.check_source_file(root);

    let ts18004_count = checker
        .ctx
        .diagnostics
        .iter()
        .filter(|d| d.code == 18004)
        .count();
    assert!(
        ts18004_count >= 1,
        "Expected TS18004 for missing shorthand property value, got diagnostics: {:?}",
        checker.ctx.diagnostics
    );
}

#[test]
fn test_unknown_property_access_emits_ts18046() {
    let source = r"
function f(x: unknown) {
    x.foo;
    x[10];
}
";
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
        crate::context::CheckerOptions::default(),
    );

    checker.check_source_file(root);

    let ts18046_count = checker
        .ctx
        .diagnostics
        .iter()
        .filter(|d| d.code == 18046)
        .count();
    assert!(
        ts18046_count >= 2,
        "Expected at least 2 TS18046 for property/element access on unknown, got {ts18046_count}. Diagnostics: {:?}",
        checker.ctx.diagnostics
    );
    // Should NOT emit TS2339 for unknown type accesses
    let ts2339_count = checker
        .ctx
        .diagnostics
        .iter()
        .filter(|d| d.code == 2339)
        .count();
    assert_eq!(
        ts2339_count, 0,
        "Expected no TS2339 for unknown property access (should be TS18046), got {ts2339_count}"
    );
}

#[test]
fn test_namespace_import_unknown_does_not_emit_ts18046() {
    let source = r#"
import * as ns from "./missing";
ns.foo.bar();
"#;
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
        crate::context::CheckerOptions::default(),
    );

    checker.check_source_file(root);

    let ts18046_count = checker
        .ctx
        .diagnostics
        .iter()
        .filter(|d| d.code == 18046)
        .count();
    assert_eq!(
        ts18046_count, 0,
        "Expected no TS18046 for namespace import access fallback, got diagnostics: {:?}",
        checker.ctx.diagnostics
    );
}

// NOTE: TS18046 for calls, binary ops, and unary ops on unknown is deferred.
// Our type system sometimes resolves non-unknown types (e.g., iterator values,
// unresolved imports) as TypeId::UNKNOWN. Adding TS18046 in those paths causes
// false positives. Property access TS18046 is safe because it replaces existing
// TS2339 errors (wrong code → correct code), so no tests flip from pass to fail.

#[test]
fn test_for_await_no_ts1103_ts1431_ts1432() {
    // tsc 6.0 no longer emits TS1103/TS1431/TS1432 for `for await` statements.
    // Top-level `for await` and `for await` in non-async functions are accepted.
    // Only TS18038 (for-await in class static blocks) is still emitted.
    let source = r#"
async function ok() {
    let y: any;
    for await (const x of y) {}
}
function notAsync() {
    let y: any;
    for await (const x of y) {}
}
for await (const x of []) {}
"#;
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
        crate::context::CheckerOptions::default(),
    );

    checker.check_source_file(root);

    let obsolete: Vec<_> = checker
        .ctx
        .diagnostics
        .iter()
        .filter(|d| d.code == 1103 || d.code == 1431 || d.code == 1432)
        .collect();
    assert!(
        obsolete.is_empty(),
        "Expected no TS1103/TS1431/TS1432 (obsolete in tsc 6.0), got: {obsolete:?}"
    );
}

#[test]
fn test_literal_undefined_in_binary_op_emits_ts18050() {
    // When the literal `undefined` keyword is used in a binary operation,
    // tsc emits TS18050 "The value 'undefined' cannot be used here."
    let source = r"
var r = undefined + 1;
";
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
        crate::context::CheckerOptions::default(),
    );

    checker.check_source_file(root);

    let ts18050_count = checker
        .ctx
        .diagnostics
        .iter()
        .filter(|d| d.code == 18050)
        .count();
    assert!(
        ts18050_count >= 1,
        "Expected TS18050 for literal `undefined` in binary op, got {ts18050_count}"
    );

    // Should NOT emit TS18048 for literal undefined
    let ts18048_count = checker
        .ctx
        .diagnostics
        .iter()
        .filter(|d| d.code == 18048)
        .count();
    assert_eq!(
        ts18048_count, 0,
        "Should not emit TS18048 for literal `undefined`; expected TS18050"
    );
}

#[test]
fn test_variable_with_undefined_type_in_binary_op_emits_ts18048() {
    // When a variable whose type is `undefined` is used in a binary operation,
    // tsc emits TS18048 "'x' is possibly 'undefined'." (not TS18050).
    let source = r"
var x: typeof undefined;
var r = x < 1;
";
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
        crate::context::CheckerOptions::default(),
    );

    checker.check_source_file(root);

    let ts18048_count = checker
        .ctx
        .diagnostics
        .iter()
        .filter(|d| d.code == 18048)
        .count();
    assert!(
        ts18048_count >= 1,
        "Expected TS18048 for variable with undefined type in binary op, got {ts18048_count}"
    );

    // Should NOT emit TS18050 for a variable (only for the literal keyword)
    let ts18050_count = checker
        .ctx
        .diagnostics
        .iter()
        .filter(|d| d.code == 18050)
        .count();
    assert_eq!(
        ts18050_count, 0,
        "Should not emit TS18050 for variable with undefined type; expected TS18048"
    );
}

#[test]
fn test_union_with_undefined_in_binary_op_prefers_ts18048_over_ts2365() {
    let source = r"
let x: number | undefined = Math.random() ? 1 : undefined;
let r = x < 1;
";
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
        crate::context::CheckerOptions::default(),
    );

    checker.check_source_file(root);

    let ts18048_count = checker
        .ctx
        .diagnostics
        .iter()
        .filter(|d| d.code == 18048)
        .count();
    assert!(
        ts18048_count >= 1,
        "Expected TS18048 for `number | undefined` operand, got diagnostics: {:?}",
        checker.ctx.diagnostics
    );

    let ts2365_count = checker
        .ctx
        .diagnostics
        .iter()
        .filter(|d| d.code == 2365)
        .count();
    assert_eq!(
        ts2365_count, 0,
        "Should suppress TS2365 when TS18048 is emitted, got diagnostics: {:?}",
        checker.ctx.diagnostics
    );
}

#[test]
fn test_optional_property_chain_in_binary_op_uses_ts18048_name() {
    let source = r"
declare const item: { id?: number };
let r = item.id < 5;
";
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
        crate::context::CheckerOptions::default(),
    );

    checker.check_source_file(root);

    let ts18048 = checker.ctx.diagnostics.iter().find(|d| d.code == 18048);
    assert!(
        ts18048.is_some(),
        "Expected TS18048 for optional property comparison, got diagnostics: {:?}",
        checker.ctx.diagnostics
    );

    let has_item_dot_id_message = checker.ctx.diagnostics.iter().any(|d| {
        d.code == 18048
            && d.message_text
                .contains("'item.id' is possibly 'undefined'.")
    });
    assert!(
        has_item_dot_id_message,
        "Expected TS18048 message for 'item.id', got diagnostics: {:?}",
        checker.ctx.diagnostics
    );

    let ts2365_count = checker
        .ctx
        .diagnostics
        .iter()
        .filter(|d| d.code == 2365)
        .count();
    assert_eq!(
        ts2365_count, 0,
        "Should suppress TS2365 for optional property nullish check, got diagnostics: {:?}",
        checker.ctx.diagnostics
    );
}

#[test]
fn test_literal_null_in_binary_op_emits_ts18050() {
    // When the literal `null` keyword is used in a binary operation,
    // tsc emits TS18050 "The value 'null' cannot be used here."
    let source = r"
var r = null + 1;
";
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
        crate::context::CheckerOptions::default(),
    );

    checker.check_source_file(root);

    let ts18050_count = checker
        .ctx
        .diagnostics
        .iter()
        .filter(|d| d.code == 18050)
        .count();
    assert!(
        ts18050_count >= 1,
        "Expected TS18050 for literal `null` in binary op, got {ts18050_count}"
    );
}

//! Tests for TS2362 with string literal union types
//!
//! These tests verify that we don't emit false positive TS2362 errors
//! when the + operator is used with union types containing string literals.

#[test]
fn test_string_literal_union_plus_no_error() {
    let source = r#"
let x: "hello" | number;
let y = x + 1;  // Should not emit TS2362 - this is string concatenation
"#;
    let codes = tsz_checker::test_utils::check_source_codes(source);
    let error_count = codes.iter().filter(|&&c| c == 2362 || c == 2363).count();
    assert_eq!(
        error_count, 0,
        "Expected no TS2362/TS2363 errors for string literal union + number, got {error_count}"
    );
}

#[test]
fn test_number_string_union_minus_emits_ts2362() {
    let source = r"
declare let x: number | string;
let y = x - 1;  // Should emit TS2362 - this is arithmetic, not string concatenation
";
    let codes = tsz_checker::test_utils::check_source_codes(source);
    let error_count = codes.iter().filter(|&&c| c == 2362).count();
    assert!(
        error_count >= 1,
        "Expected at least 1 TS2362 error for number | string - number, got {error_count}"
    );
}

#[test]
fn test_multiple_string_literals_union_plus_no_error() {
    let source = r#"
let x: "hello" | "world" | number;
let y = x + 1;  // Should not emit TS2362
"#;
    let codes = tsz_checker::test_utils::check_source_codes(source);
    let error_count = codes.iter().filter(|&&c| c == 2362 || c == 2363).count();
    assert_eq!(
        error_count, 0,
        "Expected no TS2362/TS2363 errors, got {error_count}"
    );
}

#[test]
fn test_number_literal_union_plus_number_no_error() {
    let source = r"
let x: 1 | 2 | 3;
let y = x + 1;  // Should not emit TS2362 - number literal union is valid for arithmetic
";
    let codes = tsz_checker::test_utils::check_source_codes(source);
    let error_count = codes.iter().filter(|&&c| c == 2362 || c == 2363).count();
    assert_eq!(
        error_count, 0,
        "Expected no TS2362/TS2363 errors for number literal union, got {error_count}"
    );
}

#[test]
fn test_exact_primitive_arithmetic_pairs_no_errors() {
    let source = r#"
const a = 1 + 2;
const b = "x" + "y";
const c = 1n + 2n;
const d = 4n * 3n;
"#;
    let codes = tsz_checker::test_utils::check_source_codes(source);
    let error_count = codes
        .iter()
        .filter(|&&c| c == 2362 || c == 2363 || c == 2365)
        .count();
    assert_eq!(
        error_count, 0,
        "Expected no TS2362/TS2363/TS2365 errors for exact primitive arithmetic pairs, got {error_count}"
    );
}

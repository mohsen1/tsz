//! Comprehensive tests for binary operation type evaluation.
//!
//! Covers arithmetic, string concatenation, comparison, equality,
//! bitwise, logical operators, `BigInt` operations, enum operations,
//! template literal concatenation, and type widening in arithmetic contexts.

use crate::intern::TypeInterner;
use crate::operations::BinaryOpEvaluator;
use crate::operations::binary_ops::BinaryOpResult;
use crate::types::*;

// =============================================================================
// Helpers
// =============================================================================

fn assert_success(result: &BinaryOpResult, expected: TypeId) {
    match result {
        BinaryOpResult::Success(t) => assert_eq!(
            *t, expected,
            "Expected Success({expected:?}), got Success({t:?})"
        ),
        BinaryOpResult::TypeError { left, right, op } => {
            panic!(
                "Expected Success({expected:?}), got TypeError {{ left: {left:?}, right: {right:?}, op: {op} }}"
            )
        }
    }
}

fn assert_type_error(result: &BinaryOpResult) {
    assert!(
        matches!(result, BinaryOpResult::TypeError { .. }),
        "Expected TypeError, got {result:?}"
    );
}

// =============================================================================
// Arithmetic operators: +, -, *, /, %, **
// =============================================================================

#[test]
fn test_number_plus_number() {
    let interner = TypeInterner::new();
    let eval = BinaryOpEvaluator::new(&interner);
    assert_success(
        &eval.evaluate(TypeId::NUMBER, TypeId::NUMBER, "+"),
        TypeId::NUMBER,
    );
}

#[test]
fn test_number_literal_plus_number_literal() {
    let interner = TypeInterner::new();
    let eval = BinaryOpEvaluator::new(&interner);
    let a = interner.literal_number(1.0);
    let b = interner.literal_number(2.0);
    // Number literal + number literal widens to number
    assert_success(&eval.evaluate(a, b, "+"), TypeId::NUMBER);
}

#[test]
fn test_number_literal_plus_number() {
    let interner = TypeInterner::new();
    let eval = BinaryOpEvaluator::new(&interner);
    let lit = interner.literal_number(42.0);
    assert_success(&eval.evaluate(lit, TypeId::NUMBER, "+"), TypeId::NUMBER);
    assert_success(&eval.evaluate(TypeId::NUMBER, lit, "+"), TypeId::NUMBER);
}

#[test]
fn test_subtraction_number() {
    let interner = TypeInterner::new();
    let eval = BinaryOpEvaluator::new(&interner);
    assert_success(
        &eval.evaluate(TypeId::NUMBER, TypeId::NUMBER, "-"),
        TypeId::NUMBER,
    );
}

#[test]
fn test_multiplication_number() {
    let interner = TypeInterner::new();
    let eval = BinaryOpEvaluator::new(&interner);
    assert_success(
        &eval.evaluate(TypeId::NUMBER, TypeId::NUMBER, "*"),
        TypeId::NUMBER,
    );
}

#[test]
fn test_division_number() {
    let interner = TypeInterner::new();
    let eval = BinaryOpEvaluator::new(&interner);
    assert_success(
        &eval.evaluate(TypeId::NUMBER, TypeId::NUMBER, "/"),
        TypeId::NUMBER,
    );
}

#[test]
fn test_modulo_number() {
    let interner = TypeInterner::new();
    let eval = BinaryOpEvaluator::new(&interner);
    assert_success(
        &eval.evaluate(TypeId::NUMBER, TypeId::NUMBER, "%"),
        TypeId::NUMBER,
    );
}

#[test]
fn test_exponentiation_number() {
    let interner = TypeInterner::new();
    let eval = BinaryOpEvaluator::new(&interner);
    assert_success(
        &eval.evaluate(TypeId::NUMBER, TypeId::NUMBER, "**"),
        TypeId::NUMBER,
    );
}

#[test]
fn test_arithmetic_with_any_returns_number() {
    let interner = TypeInterner::new();
    let eval = BinaryOpEvaluator::new(&interner);
    for op in &["-", "*", "/", "%", "**"] {
        assert_success(
            &eval.evaluate(TypeId::ANY, TypeId::NUMBER, op),
            TypeId::NUMBER,
        );
        assert_success(
            &eval.evaluate(TypeId::NUMBER, TypeId::ANY, op),
            TypeId::NUMBER,
        );
        assert_success(&eval.evaluate(TypeId::ANY, TypeId::ANY, op), TypeId::NUMBER);
    }
}

#[test]
fn test_arithmetic_string_is_error() {
    let interner = TypeInterner::new();
    let eval = BinaryOpEvaluator::new(&interner);
    for op in &["-", "*", "/", "%", "**"] {
        assert_type_error(&eval.evaluate(TypeId::STRING, TypeId::NUMBER, op));
        assert_type_error(&eval.evaluate(TypeId::NUMBER, TypeId::STRING, op));
        assert_type_error(&eval.evaluate(TypeId::STRING, TypeId::STRING, op));
    }
}

#[test]
fn test_arithmetic_boolean_is_error() {
    let interner = TypeInterner::new();
    let eval = BinaryOpEvaluator::new(&interner);
    assert_type_error(&eval.evaluate(TypeId::BOOLEAN, TypeId::NUMBER, "-"));
    assert_type_error(&eval.evaluate(TypeId::NUMBER, TypeId::BOOLEAN, "-"));
}

#[test]
fn test_arithmetic_with_number_literals() {
    let interner = TypeInterner::new();
    let eval = BinaryOpEvaluator::new(&interner);
    let a = interner.literal_number(10.0);
    let b = interner.literal_number(3.0);
    assert_success(&eval.evaluate(a, b, "-"), TypeId::NUMBER);
    assert_success(&eval.evaluate(a, b, "*"), TypeId::NUMBER);
    assert_success(&eval.evaluate(a, b, "/"), TypeId::NUMBER);
    assert_success(&eval.evaluate(a, b, "%"), TypeId::NUMBER);
    assert_success(&eval.evaluate(a, b, "**"), TypeId::NUMBER);
}

#[test]
fn test_arithmetic_with_never_returns_never() {
    let interner = TypeInterner::new();
    let eval = BinaryOpEvaluator::new(&interner);
    for op in &["+", "-", "*", "/", "%", "**"] {
        assert_success(
            &eval.evaluate(TypeId::NEVER, TypeId::NUMBER, op),
            TypeId::NEVER,
        );
        assert_success(
            &eval.evaluate(TypeId::NUMBER, TypeId::NEVER, op),
            TypeId::NEVER,
        );
    }
}

#[test]
fn test_arithmetic_with_unknown_returns_unknown() {
    let interner = TypeInterner::new();
    let eval = BinaryOpEvaluator::new(&interner);
    for op in &["-", "*", "/", "%", "**"] {
        assert_success(
            &eval.evaluate(TypeId::UNKNOWN, TypeId::NUMBER, op),
            TypeId::UNKNOWN,
        );
        assert_success(
            &eval.evaluate(TypeId::NUMBER, TypeId::UNKNOWN, op),
            TypeId::UNKNOWN,
        );
    }
}

#[test]
fn test_arithmetic_with_error_type() {
    let interner = TypeInterner::new();
    let eval = BinaryOpEvaluator::new(&interner);
    // ERROR acts like any - prevents cascading errors
    assert_success(
        &eval.evaluate(TypeId::ERROR, TypeId::NUMBER, "-"),
        TypeId::NUMBER,
    );
    assert_success(
        &eval.evaluate(TypeId::NUMBER, TypeId::ERROR, "-"),
        TypeId::NUMBER,
    );
}

// =============================================================================
// String concatenation: + with string types
// =============================================================================

#[test]
fn test_string_plus_string() {
    let interner = TypeInterner::new();
    let eval = BinaryOpEvaluator::new(&interner);
    assert_success(
        &eval.evaluate(TypeId::STRING, TypeId::STRING, "+"),
        TypeId::STRING,
    );
}

#[test]
fn test_string_plus_number_coercion() {
    let interner = TypeInterner::new();
    let eval = BinaryOpEvaluator::new(&interner);
    // string + number = string (number is coerced to string)
    assert_success(
        &eval.evaluate(TypeId::STRING, TypeId::NUMBER, "+"),
        TypeId::STRING,
    );
    assert_success(
        &eval.evaluate(TypeId::NUMBER, TypeId::STRING, "+"),
        TypeId::STRING,
    );
}

#[test]
fn test_string_plus_boolean_coercion() {
    let interner = TypeInterner::new();
    let eval = BinaryOpEvaluator::new(&interner);
    assert_success(
        &eval.evaluate(TypeId::STRING, TypeId::BOOLEAN, "+"),
        TypeId::STRING,
    );
    assert_success(
        &eval.evaluate(TypeId::BOOLEAN, TypeId::STRING, "+"),
        TypeId::STRING,
    );
}

#[test]
fn test_string_plus_bigint_coercion() {
    let interner = TypeInterner::new();
    let eval = BinaryOpEvaluator::new(&interner);
    assert_success(
        &eval.evaluate(TypeId::STRING, TypeId::BIGINT, "+"),
        TypeId::STRING,
    );
    assert_success(
        &eval.evaluate(TypeId::BIGINT, TypeId::STRING, "+"),
        TypeId::STRING,
    );
}

#[test]
fn test_string_plus_null_coercion() {
    let interner = TypeInterner::new();
    let eval = BinaryOpEvaluator::new(&interner);
    assert_success(
        &eval.evaluate(TypeId::STRING, TypeId::NULL, "+"),
        TypeId::STRING,
    );
    assert_success(
        &eval.evaluate(TypeId::NULL, TypeId::STRING, "+"),
        TypeId::STRING,
    );
}

#[test]
fn test_string_plus_undefined_coercion() {
    let interner = TypeInterner::new();
    let eval = BinaryOpEvaluator::new(&interner);
    assert_success(
        &eval.evaluate(TypeId::STRING, TypeId::UNDEFINED, "+"),
        TypeId::STRING,
    );
    assert_success(
        &eval.evaluate(TypeId::UNDEFINED, TypeId::STRING, "+"),
        TypeId::STRING,
    );
}

#[test]
fn test_string_plus_void_coercion() {
    let interner = TypeInterner::new();
    let eval = BinaryOpEvaluator::new(&interner);
    assert_success(
        &eval.evaluate(TypeId::STRING, TypeId::VOID, "+"),
        TypeId::STRING,
    );
}

#[test]
fn test_string_literal_plus_string_literal() {
    let interner = TypeInterner::new();
    let eval = BinaryOpEvaluator::new(&interner);
    let a = interner.literal_string("hello");
    let b = interner.literal_string(" world");
    // String literal + string literal = string (widened)
    assert_success(&eval.evaluate(a, b, "+"), TypeId::STRING);
}

#[test]
fn test_string_literal_plus_number() {
    let interner = TypeInterner::new();
    let eval = BinaryOpEvaluator::new(&interner);
    let s = interner.literal_string("count: ");
    assert_success(&eval.evaluate(s, TypeId::NUMBER, "+"), TypeId::STRING);
}

#[test]
fn test_string_literal_plus_number_literal() {
    let interner = TypeInterner::new();
    let eval = BinaryOpEvaluator::new(&interner);
    let s = interner.literal_string("value");
    let n = interner.literal_number(42.0);
    assert_success(&eval.evaluate(s, n, "+"), TypeId::STRING);
}

#[test]
fn test_string_plus_any() {
    let interner = TypeInterner::new();
    let eval = BinaryOpEvaluator::new(&interner);
    // any + anything = any (takes precedence over string concat)
    assert_success(
        &eval.evaluate(TypeId::ANY, TypeId::STRING, "+"),
        TypeId::ANY,
    );
    assert_success(
        &eval.evaluate(TypeId::STRING, TypeId::ANY, "+"),
        TypeId::ANY,
    );
}

#[test]
fn test_symbol_plus_anything_is_error() {
    let interner = TypeInterner::new();
    let eval = BinaryOpEvaluator::new(&interner);
    // TS2469: Symbol cannot be used in +
    assert_type_error(&eval.evaluate(TypeId::SYMBOL, TypeId::STRING, "+"));
    assert_type_error(&eval.evaluate(TypeId::STRING, TypeId::SYMBOL, "+"));
    assert_type_error(&eval.evaluate(TypeId::SYMBOL, TypeId::NUMBER, "+"));
    assert_type_error(&eval.evaluate(TypeId::NUMBER, TypeId::SYMBOL, "+"));
}

// =============================================================================
// Comparison operators: <, >, <=, >=
// =============================================================================

#[test]
fn test_number_comparison_returns_boolean() {
    let interner = TypeInterner::new();
    let eval = BinaryOpEvaluator::new(&interner);
    for op in &["<", ">", "<=", ">="] {
        assert_success(
            &eval.evaluate(TypeId::NUMBER, TypeId::NUMBER, op),
            TypeId::BOOLEAN,
        );
    }
}

#[test]
fn test_string_comparison_returns_boolean() {
    let interner = TypeInterner::new();
    let eval = BinaryOpEvaluator::new(&interner);
    for op in &["<", ">", "<=", ">="] {
        assert_success(
            &eval.evaluate(TypeId::STRING, TypeId::STRING, op),
            TypeId::BOOLEAN,
        );
    }
}

#[test]
fn test_bigint_comparison_returns_boolean() {
    let interner = TypeInterner::new();
    let eval = BinaryOpEvaluator::new(&interner);
    for op in &["<", ">", "<=", ">="] {
        assert_success(
            &eval.evaluate(TypeId::BIGINT, TypeId::BIGINT, op),
            TypeId::BOOLEAN,
        );
    }
}

#[test]
fn test_boolean_comparison_returns_boolean() {
    let interner = TypeInterner::new();
    let eval = BinaryOpEvaluator::new(&interner);
    for op in &["<", ">", "<=", ">="] {
        assert_success(
            &eval.evaluate(TypeId::BOOLEAN, TypeId::BOOLEAN, op),
            TypeId::BOOLEAN,
        );
    }
}

#[test]
fn test_mixed_type_comparison_is_error() {
    let interner = TypeInterner::new();
    let eval = BinaryOpEvaluator::new(&interner);
    // number < string => TypeError
    assert_type_error(&eval.evaluate(TypeId::NUMBER, TypeId::STRING, "<"));
    assert_type_error(&eval.evaluate(TypeId::STRING, TypeId::NUMBER, ">"));
    // boolean < number => TypeError
    assert_type_error(&eval.evaluate(TypeId::BOOLEAN, TypeId::NUMBER, "<="));
}

#[test]
fn test_comparison_with_any_returns_boolean() {
    let interner = TypeInterner::new();
    let eval = BinaryOpEvaluator::new(&interner);
    for op in &["<", ">", "<=", ">="] {
        assert_success(
            &eval.evaluate(TypeId::ANY, TypeId::NUMBER, op),
            TypeId::BOOLEAN,
        );
        assert_success(
            &eval.evaluate(TypeId::NUMBER, TypeId::ANY, op),
            TypeId::BOOLEAN,
        );
    }
}

#[test]
fn test_comparison_number_literals() {
    let interner = TypeInterner::new();
    let eval = BinaryOpEvaluator::new(&interner);
    let a = interner.literal_number(1.0);
    let b = interner.literal_number(2.0);
    assert_success(&eval.evaluate(a, b, "<"), TypeId::BOOLEAN);
    assert_success(&eval.evaluate(a, b, ">="), TypeId::BOOLEAN);
}

#[test]
fn test_comparison_string_literals() {
    let interner = TypeInterner::new();
    let eval = BinaryOpEvaluator::new(&interner);
    let a = interner.literal_string("abc");
    let b = interner.literal_string("xyz");
    assert_success(&eval.evaluate(a, b, "<"), TypeId::BOOLEAN);
}

#[test]
fn test_comparison_with_unknown() {
    let interner = TypeInterner::new();
    let eval = BinaryOpEvaluator::new(&interner);
    // unknown prevents cascading errors
    for op in &["<", ">", "<=", ">="] {
        assert_success(
            &eval.evaluate(TypeId::UNKNOWN, TypeId::NUMBER, op),
            TypeId::BOOLEAN,
        );
        assert_success(
            &eval.evaluate(TypeId::NUMBER, TypeId::UNKNOWN, op),
            TypeId::BOOLEAN,
        );
    }
}

#[test]
fn test_comparison_symbol_is_error() {
    let interner = TypeInterner::new();
    let eval = BinaryOpEvaluator::new(&interner);
    // TS2469: Symbol cannot be used in comparison
    assert_type_error(&eval.evaluate(TypeId::SYMBOL, TypeId::NUMBER, "<"));
    assert_type_error(&eval.evaluate(TypeId::NUMBER, TypeId::SYMBOL, ">"));
}

#[test]
fn test_comparison_with_error_type() {
    let interner = TypeInterner::new();
    let eval = BinaryOpEvaluator::new(&interner);
    // ERROR acts like any
    assert_success(
        &eval.evaluate(TypeId::ERROR, TypeId::NUMBER, "<"),
        TypeId::BOOLEAN,
    );
    assert_success(
        &eval.evaluate(TypeId::NUMBER, TypeId::ERROR, ">"),
        TypeId::BOOLEAN,
    );
}

// =============================================================================
// Equality operators: ==, ===, !=, !==
// =============================================================================

#[test]
fn test_equality_always_returns_boolean() {
    let interner = TypeInterner::new();
    let eval = BinaryOpEvaluator::new(&interner);
    // Equality operators always produce boolean regardless of operand types
    for op in &["==", "===", "!=", "!=="] {
        assert_success(
            &eval.evaluate(TypeId::STRING, TypeId::NUMBER, op),
            TypeId::BOOLEAN,
        );
        assert_success(
            &eval.evaluate(TypeId::NUMBER, TypeId::NUMBER, op),
            TypeId::BOOLEAN,
        );
        assert_success(
            &eval.evaluate(TypeId::BOOLEAN, TypeId::STRING, op),
            TypeId::BOOLEAN,
        );
        assert_success(
            &eval.evaluate(TypeId::NULL, TypeId::UNDEFINED, op),
            TypeId::BOOLEAN,
        );
    }
}

#[test]
fn test_equality_with_any() {
    let interner = TypeInterner::new();
    let eval = BinaryOpEvaluator::new(&interner);
    for op in &["==", "===", "!=", "!=="] {
        assert_success(
            &eval.evaluate(TypeId::ANY, TypeId::NUMBER, op),
            TypeId::BOOLEAN,
        );
    }
}

#[test]
fn test_equality_with_never() {
    let interner = TypeInterner::new();
    let eval = BinaryOpEvaluator::new(&interner);
    // never is the bottom type - operations on never produce never
    for op in &["==", "===", "!=", "!=="] {
        assert_success(
            &eval.evaluate(TypeId::NEVER, TypeId::NUMBER, op),
            TypeId::NEVER,
        );
        assert_success(
            &eval.evaluate(TypeId::NUMBER, TypeId::NEVER, op),
            TypeId::NEVER,
        );
    }
}

#[test]
fn test_equality_literals() {
    let interner = TypeInterner::new();
    let eval = BinaryOpEvaluator::new(&interner);
    let a = interner.literal_string("hello");
    let b = interner.literal_number(42.0);
    assert_success(&eval.evaluate(a, b, "==="), TypeId::BOOLEAN);
    assert_success(&eval.evaluate(a, b, "!=="), TypeId::BOOLEAN);
}

// =============================================================================
// Bitwise operators: &, |, ^, <<, >>, >>>
// =============================================================================

#[test]
fn test_bitwise_number_returns_number() {
    let interner = TypeInterner::new();
    let eval = BinaryOpEvaluator::new(&interner);
    for op in &["&", "|", "^", "<<", ">>", ">>>"] {
        assert_success(
            &eval.evaluate(TypeId::NUMBER, TypeId::NUMBER, op),
            TypeId::NUMBER,
        );
    }
}

#[test]
fn test_bitwise_number_literals() {
    let interner = TypeInterner::new();
    let eval = BinaryOpEvaluator::new(&interner);
    let a = interner.literal_number(0xFF as f64);
    let b = interner.literal_number(0x0F as f64);
    for op in &["&", "|", "^", "<<", ">>", ">>>"] {
        assert_success(&eval.evaluate(a, b, op), TypeId::NUMBER);
    }
}

#[test]
fn test_bitwise_string_is_error() {
    let interner = TypeInterner::new();
    let eval = BinaryOpEvaluator::new(&interner);
    for op in &["&", "|", "^", "<<", ">>", ">>>"] {
        assert_type_error(&eval.evaluate(TypeId::STRING, TypeId::NUMBER, op));
        assert_type_error(&eval.evaluate(TypeId::NUMBER, TypeId::STRING, op));
    }
}

#[test]
fn test_bitwise_boolean_is_error() {
    let interner = TypeInterner::new();
    let eval = BinaryOpEvaluator::new(&interner);
    // boolean is not valid for bitwise operations
    assert_type_error(&eval.evaluate(TypeId::BOOLEAN, TypeId::NUMBER, "&"));
    assert_type_error(&eval.evaluate(TypeId::NUMBER, TypeId::BOOLEAN, "|"));
}

#[test]
fn test_bitwise_with_any_returns_number() {
    let interner = TypeInterner::new();
    let eval = BinaryOpEvaluator::new(&interner);
    for op in &["&", "|", "^", "<<", ">>", ">>>"] {
        assert_success(
            &eval.evaluate(TypeId::ANY, TypeId::NUMBER, op),
            TypeId::NUMBER,
        );
        assert_success(
            &eval.evaluate(TypeId::NUMBER, TypeId::ANY, op),
            TypeId::NUMBER,
        );
    }
}

#[test]
fn test_bitwise_bigint_returns_bigint() {
    let interner = TypeInterner::new();
    let eval = BinaryOpEvaluator::new(&interner);
    // Bitwise ops on bigint return bigint (only &, |, ^)
    for op in &["&", "|", "^"] {
        assert_success(
            &eval.evaluate(TypeId::BIGINT, TypeId::BIGINT, op),
            TypeId::BIGINT,
        );
    }
}

#[test]
fn test_bitwise_shift_bigint_returns_bigint() {
    let interner = TypeInterner::new();
    let eval = BinaryOpEvaluator::new(&interner);
    // Shift ops on bigint return bigint
    assert_success(
        &eval.evaluate(TypeId::BIGINT, TypeId::BIGINT, "<<"),
        TypeId::BIGINT,
    );
    assert_success(
        &eval.evaluate(TypeId::BIGINT, TypeId::BIGINT, ">>"),
        TypeId::BIGINT,
    );
}

// =============================================================================
// Logical operators: &&, ||, ??
// =============================================================================

#[test]
fn test_logical_and_number_string() {
    let interner = TypeInterner::new();
    let eval = BinaryOpEvaluator::new(&interner);
    // number && string => definitely-falsy-part-of-number | string
    // The definitely falsy part of number is literal 0
    let result = eval.evaluate(TypeId::NUMBER, TypeId::STRING, "&&");
    match result {
        BinaryOpResult::Success(t) => {
            // Result should be a union containing the falsy part of number and string
            assert_ne!(t, TypeId::NUMBER);
            assert_ne!(t, TypeId::STRING);
        }
        _ => panic!("Expected Success for && operation"),
    }
}

#[test]
fn test_logical_and_never_left() {
    let interner = TypeInterner::new();
    let eval = BinaryOpEvaluator::new(&interner);
    // never && string => never (never can't be truthy)
    // The truthy narrowing of never = never, so falsy_left = never, truthy_left = never
    // Since truthy_left == NEVER, result is left (= never)
    let result = eval.evaluate(TypeId::NEVER, TypeId::STRING, "&&");
    assert_success(&result, TypeId::NEVER);
}

#[test]
fn test_logical_or_number_string() {
    let interner = TypeInterner::new();
    let eval = BinaryOpEvaluator::new(&interner);
    // number || string => truthy-part-of-number | string
    let result = eval.evaluate(TypeId::NUMBER, TypeId::STRING, "||");
    match result {
        BinaryOpResult::Success(t) => {
            // Result is a union of truthy-number and string
            assert_ne!(t, TypeId::STRING);
        }
        _ => panic!("Expected Success for || operation"),
    }
}

#[test]
fn test_logical_or_never_left() {
    let interner = TypeInterner::new();
    let eval = BinaryOpEvaluator::new(&interner);
    // never || string => never (never is the bottom type, code is unreachable)
    // Both falsy and truthy parts of never are never, so falsy_left == NEVER
    // which means the expression returns left (= never).
    let result = eval.evaluate(TypeId::NEVER, TypeId::STRING, "||");
    assert_success(&result, TypeId::NEVER);
}

#[test]
fn test_logical_nullish_coalescing() {
    let interner = TypeInterner::new();
    let eval = BinaryOpEvaluator::new(&interner);
    // null ?? string => string
    let result = eval.evaluate(TypeId::NULL, TypeId::STRING, "??");
    assert_success(&result, TypeId::STRING);
}

#[test]
fn test_logical_nullish_coalescing_undefined() {
    let interner = TypeInterner::new();
    let eval = BinaryOpEvaluator::new(&interner);
    // undefined ?? number => number
    let result = eval.evaluate(TypeId::UNDEFINED, TypeId::NUMBER, "??");
    assert_success(&result, TypeId::NUMBER);
}

#[test]
fn test_logical_nullish_coalescing_non_nullable() {
    let interner = TypeInterner::new();
    let eval = BinaryOpEvaluator::new(&interner);
    // number ?? string => number (number is non-nullable, so left is always used)
    let result = eval.evaluate(TypeId::NUMBER, TypeId::STRING, "??");
    assert_success(&result, TypeId::NUMBER);
}

#[test]
fn test_logical_nullish_coalescing_union_nullable() {
    let interner = TypeInterner::new();
    let eval = BinaryOpEvaluator::new(&interner);
    // (string | null) ?? number => string | number
    let left = interner.union(vec![TypeId::STRING, TypeId::NULL]);
    let result = eval.evaluate(left, TypeId::NUMBER, "??");
    match result {
        BinaryOpResult::Success(t) => {
            // Should contain string and number (but not null)
            assert_ne!(t, left);
            assert_ne!(t, TypeId::NUMBER);
        }
        _ => panic!("Expected Success for ?? operation"),
    }
}

#[test]
fn test_logical_and_boolean_string() {
    let interner = TypeInterner::new();
    let eval = BinaryOpEvaluator::new(&interner);
    // boolean && string => false | string
    let result = eval.evaluate(TypeId::BOOLEAN, TypeId::STRING, "&&");
    match result {
        BinaryOpResult::Success(t) => {
            // Result should be a union of false and string
            assert_ne!(t, TypeId::BOOLEAN);
            assert_ne!(t, TypeId::STRING);
        }
        _ => panic!("Expected Success for && operation"),
    }
}

#[test]
fn test_logical_or_boolean_string() {
    let interner = TypeInterner::new();
    let eval = BinaryOpEvaluator::new(&interner);
    // boolean || string => true | string
    let result = eval.evaluate(TypeId::BOOLEAN, TypeId::STRING, "||");
    match result {
        BinaryOpResult::Success(t) => {
            assert_ne!(t, TypeId::BOOLEAN);
        }
        _ => panic!("Expected Success for || operation"),
    }
}

// =============================================================================
// BigInt operations
// =============================================================================

#[test]
fn test_bigint_arithmetic() {
    let interner = TypeInterner::new();
    let eval = BinaryOpEvaluator::new(&interner);
    // bigint + bigint = bigint
    assert_success(
        &eval.evaluate(TypeId::BIGINT, TypeId::BIGINT, "+"),
        TypeId::BIGINT,
    );
    assert_success(
        &eval.evaluate(TypeId::BIGINT, TypeId::BIGINT, "-"),
        TypeId::BIGINT,
    );
    assert_success(
        &eval.evaluate(TypeId::BIGINT, TypeId::BIGINT, "*"),
        TypeId::BIGINT,
    );
    assert_success(
        &eval.evaluate(TypeId::BIGINT, TypeId::BIGINT, "/"),
        TypeId::BIGINT,
    );
    assert_success(
        &eval.evaluate(TypeId::BIGINT, TypeId::BIGINT, "%"),
        TypeId::BIGINT,
    );
    assert_success(
        &eval.evaluate(TypeId::BIGINT, TypeId::BIGINT, "**"),
        TypeId::BIGINT,
    );
}

#[test]
fn test_bigint_plus_number_is_error() {
    let interner = TypeInterner::new();
    let eval = BinaryOpEvaluator::new(&interner);
    // BigInt + number is a TypeError
    assert_type_error(&eval.evaluate(TypeId::BIGINT, TypeId::NUMBER, "+"));
    assert_type_error(&eval.evaluate(TypeId::NUMBER, TypeId::BIGINT, "+"));
}

#[test]
fn test_bigint_minus_number_is_error() {
    let interner = TypeInterner::new();
    let eval = BinaryOpEvaluator::new(&interner);
    assert_type_error(&eval.evaluate(TypeId::BIGINT, TypeId::NUMBER, "-"));
    assert_type_error(&eval.evaluate(TypeId::NUMBER, TypeId::BIGINT, "-"));
}

#[test]
fn test_bigint_literal_arithmetic() {
    let interner = TypeInterner::new();
    let eval = BinaryOpEvaluator::new(&interner);
    let a = interner.literal_bigint("100");
    let b = interner.literal_bigint("200");
    assert_success(&eval.evaluate(a, b, "+"), TypeId::BIGINT);
    assert_success(&eval.evaluate(a, b, "-"), TypeId::BIGINT);
    assert_success(&eval.evaluate(a, b, "*"), TypeId::BIGINT);
}

#[test]
fn test_bigint_literal_plus_number_literal_is_error() {
    let interner = TypeInterner::new();
    let eval = BinaryOpEvaluator::new(&interner);
    let bigint = interner.literal_bigint("42");
    let num = interner.literal_number(42.0);
    assert_type_error(&eval.evaluate(bigint, num, "+"));
    assert_type_error(&eval.evaluate(num, bigint, "+"));
}

#[test]
fn test_bigint_comparison() {
    let interner = TypeInterner::new();
    let eval = BinaryOpEvaluator::new(&interner);
    for op in &["<", ">", "<=", ">="] {
        assert_success(
            &eval.evaluate(TypeId::BIGINT, TypeId::BIGINT, op),
            TypeId::BOOLEAN,
        );
    }
}

#[test]
fn test_bigint_comparison_cross_type_is_error() {
    let interner = TypeInterner::new();
    let eval = BinaryOpEvaluator::new(&interner);
    // bigint < number => TypeError (mixed orderable types)
    assert_type_error(&eval.evaluate(TypeId::BIGINT, TypeId::NUMBER, "<"));
    assert_type_error(&eval.evaluate(TypeId::NUMBER, TypeId::BIGINT, ">"));
}

// =============================================================================
// Enum operations
// =============================================================================

#[test]
fn test_numeric_enum_union_arithmetic() {
    let interner = TypeInterner::new();
    let eval = BinaryOpEvaluator::new(&interner);
    // A union of number literals (simulating numeric enum) should be valid in arithmetic
    let enum_type = interner.union(vec![
        interner.literal_number(0.0),
        interner.literal_number(1.0),
        interner.literal_number(2.0),
    ]);
    assert_success(
        &eval.evaluate(enum_type, TypeId::NUMBER, "+"),
        TypeId::NUMBER,
    );
    assert_success(
        &eval.evaluate(enum_type, TypeId::NUMBER, "-"),
        TypeId::NUMBER,
    );
    assert_success(&eval.evaluate(enum_type, enum_type, "+"), TypeId::NUMBER);
}

#[test]
fn test_enum_type_arithmetic() {
    let interner = TypeInterner::new();
    let eval = BinaryOpEvaluator::new(&interner);
    // TypeData::Enum with a number structural type should be number-like
    let enum_ty = interner.enum_type(crate::def::DefId(1000), TypeId::NUMBER);
    assert_success(&eval.evaluate(enum_ty, TypeId::NUMBER, "+"), TypeId::NUMBER);
    assert_success(&eval.evaluate(enum_ty, enum_ty, "-"), TypeId::NUMBER);
}

#[test]
fn test_numeric_enum_comparison() {
    let interner = TypeInterner::new();
    let eval = BinaryOpEvaluator::new(&interner);
    let enum_type = interner.union(vec![
        interner.literal_number(0.0),
        interner.literal_number(1.0),
    ]);
    assert_success(
        &eval.evaluate(enum_type, TypeId::NUMBER, "<"),
        TypeId::BOOLEAN,
    );
    assert_success(&eval.evaluate(enum_type, enum_type, ">="), TypeId::BOOLEAN);
}

// =============================================================================
// Template literal + string
// =============================================================================

#[test]
fn test_template_literal_plus_string() {
    let interner = TypeInterner::new();
    let eval = BinaryOpEvaluator::new(&interner);
    // Template literal is string-like, so template + string = string
    let template = interner.template_literal(vec![
        TemplateSpan::Text(interner.intern_string("hello ")),
        TemplateSpan::Type(TypeId::STRING),
    ]);
    assert_success(
        &eval.evaluate(template, TypeId::STRING, "+"),
        TypeId::STRING,
    );
    assert_success(
        &eval.evaluate(TypeId::STRING, template, "+"),
        TypeId::STRING,
    );
}

#[test]
fn test_template_literal_plus_number() {
    let interner = TypeInterner::new();
    let eval = BinaryOpEvaluator::new(&interner);
    let template = interner.template_literal(vec![
        TemplateSpan::Text(interner.intern_string("value: ")),
        TemplateSpan::Type(TypeId::NUMBER),
    ]);
    // Template literal (string-like) + number = string
    assert_success(
        &eval.evaluate(template, TypeId::NUMBER, "+"),
        TypeId::STRING,
    );
}

#[test]
fn test_template_literal_comparison() {
    let interner = TypeInterner::new();
    let eval = BinaryOpEvaluator::new(&interner);
    let template = interner.template_literal(vec![
        TemplateSpan::Text(interner.intern_string("prefix")),
        TemplateSpan::Type(TypeId::STRING),
    ]);
    // Template literal (string-like) comparison with string is valid
    assert_success(
        &eval.evaluate(template, TypeId::STRING, "<"),
        TypeId::BOOLEAN,
    );
    assert_success(
        &eval.evaluate(TypeId::STRING, template, ">="),
        TypeId::BOOLEAN,
    );
}

// =============================================================================
// Type widening: literal types in arithmetic
// =============================================================================

#[test]
fn test_number_literal_arithmetic_widens_to_number() {
    let interner = TypeInterner::new();
    let eval = BinaryOpEvaluator::new(&interner);
    let lit = interner.literal_number(5.0);
    // Literal 5 + number = number (not literal 5)
    let result = eval.evaluate(lit, TypeId::NUMBER, "+");
    assert_success(&result, TypeId::NUMBER);
}

#[test]
fn test_string_literal_concat_widens_to_string() {
    let interner = TypeInterner::new();
    let eval = BinaryOpEvaluator::new(&interner);
    let s = interner.literal_string("hello");
    // "hello" + string = string (not literal "hello...")
    let result = eval.evaluate(s, TypeId::STRING, "+");
    assert_success(&result, TypeId::STRING);
}

#[test]
fn test_boolean_literal_comparison() {
    let interner = TypeInterner::new();
    let eval = BinaryOpEvaluator::new(&interner);
    let t = interner.literal_boolean(true);
    let f = interner.literal_boolean(false);
    // Boolean literals can be compared
    assert_success(&eval.evaluate(t, f, "<"), TypeId::BOOLEAN);
    assert_success(&eval.evaluate(t, TypeId::BOOLEAN, ">="), TypeId::BOOLEAN);
}

// =============================================================================
// Plus chain optimization
// =============================================================================

#[test]
fn test_plus_chain_all_number() {
    let interner = TypeInterner::new();
    let eval = BinaryOpEvaluator::new(&interner);
    let result = eval.evaluate_plus_chain(&[TypeId::NUMBER, TypeId::NUMBER, TypeId::NUMBER]);
    assert_eq!(result, Some(TypeId::NUMBER));
}

#[test]
fn test_plus_chain_all_string() {
    let interner = TypeInterner::new();
    let eval = BinaryOpEvaluator::new(&interner);
    let result = eval.evaluate_plus_chain(&[TypeId::STRING, TypeId::STRING, TypeId::STRING]);
    assert_eq!(result, Some(TypeId::STRING));
}

#[test]
fn test_plus_chain_all_bigint() {
    let interner = TypeInterner::new();
    let eval = BinaryOpEvaluator::new(&interner);
    let result = eval.evaluate_plus_chain(&[TypeId::BIGINT, TypeId::BIGINT]);
    assert_eq!(result, Some(TypeId::BIGINT));
}

#[test]
fn test_plus_chain_has_any() {
    let interner = TypeInterner::new();
    let eval = BinaryOpEvaluator::new(&interner);
    let result = eval.evaluate_plus_chain(&[TypeId::NUMBER, TypeId::ANY, TypeId::STRING]);
    assert_eq!(result, Some(TypeId::ANY));
}

#[test]
fn test_plus_chain_has_error() {
    let interner = TypeInterner::new();
    let eval = BinaryOpEvaluator::new(&interner);
    let result = eval.evaluate_plus_chain(&[TypeId::NUMBER, TypeId::ERROR]);
    assert_eq!(result, Some(TypeId::ERROR));
}

#[test]
fn test_plus_chain_mixed_returns_none() {
    let interner = TypeInterner::new();
    let eval = BinaryOpEvaluator::new(&interner);
    // Mixed types that don't have any need normal binary evaluation
    let result = eval.evaluate_plus_chain(&[TypeId::NUMBER, TypeId::STRING]);
    assert_eq!(result, None);
}

#[test]
fn test_plus_chain_single_operand() {
    let interner = TypeInterner::new();
    let eval = BinaryOpEvaluator::new(&interner);
    let result = eval.evaluate_plus_chain(&[TypeId::NUMBER]);
    assert_eq!(result, None);
}

#[test]
fn test_plus_chain_symbol_bails() {
    let interner = TypeInterner::new();
    let eval = BinaryOpEvaluator::new(&interner);
    let result = eval.evaluate_plus_chain(&[TypeId::NUMBER, TypeId::SYMBOL]);
    assert_eq!(result, None);
}

// =============================================================================
// is_arithmetic_operand
// =============================================================================

#[test]
fn test_is_arithmetic_operand_valid_types() {
    let interner = TypeInterner::new();
    let eval = BinaryOpEvaluator::new(&interner);
    assert!(eval.is_arithmetic_operand(TypeId::NUMBER));
    assert!(eval.is_arithmetic_operand(TypeId::ANY));
    assert!(eval.is_arithmetic_operand(TypeId::BIGINT));
    assert!(eval.is_arithmetic_operand(TypeId::ERROR));
    assert!(eval.is_arithmetic_operand(TypeId::UNKNOWN));
    assert!(eval.is_arithmetic_operand(TypeId::NEVER));
}

#[test]
fn test_is_arithmetic_operand_invalid_types() {
    let interner = TypeInterner::new();
    let eval = BinaryOpEvaluator::new(&interner);
    assert!(!eval.is_arithmetic_operand(TypeId::STRING));
    assert!(!eval.is_arithmetic_operand(TypeId::BOOLEAN));
    assert!(!eval.is_arithmetic_operand(TypeId::NULL));
    assert!(!eval.is_arithmetic_operand(TypeId::UNDEFINED));
    assert!(!eval.is_arithmetic_operand(TypeId::VOID));
    assert!(!eval.is_arithmetic_operand(TypeId::SYMBOL));
}

#[test]
fn test_is_arithmetic_operand_number_literal() {
    let interner = TypeInterner::new();
    let eval = BinaryOpEvaluator::new(&interner);
    let lit = interner.literal_number(42.0);
    assert!(eval.is_arithmetic_operand(lit));
}

#[test]
fn test_is_arithmetic_operand_bigint_literal() {
    let interner = TypeInterner::new();
    let eval = BinaryOpEvaluator::new(&interner);
    let lit = interner.literal_bigint("99");
    assert!(eval.is_arithmetic_operand(lit));
}

#[test]
fn test_is_arithmetic_operand_string_literal_invalid() {
    let interner = TypeInterner::new();
    let eval = BinaryOpEvaluator::new(&interner);
    let lit = interner.literal_string("nope");
    assert!(!eval.is_arithmetic_operand(lit));
}

// =============================================================================
// has_overlap
// =============================================================================

#[test]
fn test_has_overlap_same_type() {
    let interner = TypeInterner::new();
    let eval = BinaryOpEvaluator::new(&interner);
    assert!(eval.has_overlap(TypeId::STRING, TypeId::STRING));
    assert!(eval.has_overlap(TypeId::NUMBER, TypeId::NUMBER));
}

#[test]
fn test_has_overlap_disjoint_primitives() {
    let interner = TypeInterner::new();
    let eval = BinaryOpEvaluator::new(&interner);
    assert!(!eval.has_overlap(TypeId::STRING, TypeId::NUMBER));
    assert!(!eval.has_overlap(TypeId::NUMBER, TypeId::BOOLEAN));
    assert!(!eval.has_overlap(TypeId::STRING, TypeId::BIGINT));
}

#[test]
fn test_has_overlap_any_with_everything() {
    let interner = TypeInterner::new();
    let eval = BinaryOpEvaluator::new(&interner);
    assert!(eval.has_overlap(TypeId::ANY, TypeId::STRING));
    assert!(eval.has_overlap(TypeId::ANY, TypeId::NUMBER));
    assert!(eval.has_overlap(TypeId::ANY, TypeId::BOOLEAN));
}

#[test]
fn test_has_overlap_never_with_nothing() {
    let interner = TypeInterner::new();
    let eval = BinaryOpEvaluator::new(&interner);
    // has_overlap fast path: left == right returns true, even for never
    assert!(!eval.has_overlap(TypeId::NEVER, TypeId::STRING));
    assert!(!eval.has_overlap(TypeId::NEVER, TypeId::NUMBER));
    // never == never hits the identity fast path and returns true
    assert!(eval.has_overlap(TypeId::NEVER, TypeId::NEVER));
}

#[test]
fn test_has_overlap_literal_with_primitive() {
    let interner = TypeInterner::new();
    let eval = BinaryOpEvaluator::new(&interner);
    let lit_s = interner.literal_string("hello");
    let lit_n = interner.literal_number(42.0);
    assert!(eval.has_overlap(lit_s, TypeId::STRING));
    assert!(eval.has_overlap(lit_n, TypeId::NUMBER));
    assert!(!eval.has_overlap(lit_s, TypeId::NUMBER));
    assert!(!eval.has_overlap(lit_n, TypeId::STRING));
}

#[test]
fn test_has_overlap_union_with_primitive() {
    let interner = TypeInterner::new();
    let eval = BinaryOpEvaluator::new(&interner);
    let union = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);
    assert!(eval.has_overlap(union, TypeId::STRING));
    assert!(eval.has_overlap(union, TypeId::NUMBER));
    assert!(!eval.has_overlap(union, TypeId::BOOLEAN));
}

#[test]
fn test_has_overlap_different_literals_same_primitive() {
    let interner = TypeInterner::new();
    let eval = BinaryOpEvaluator::new(&interner);
    let a = interner.literal_string("hello");
    let b = interner.literal_string("world");
    // Different string literals don't overlap (they're disjoint values)
    assert!(!eval.has_overlap(a, b));
}

// =============================================================================
// is_symbol_like
// =============================================================================

#[test]
fn test_is_symbol_like() {
    let interner = TypeInterner::new();
    let eval = BinaryOpEvaluator::new(&interner);
    assert!(eval.is_symbol_like(TypeId::SYMBOL));
    assert!(!eval.is_symbol_like(TypeId::STRING));
    assert!(!eval.is_symbol_like(TypeId::NUMBER));
}

#[test]
fn test_is_symbol_like_unique_symbol() {
    let interner = TypeInterner::new();
    let eval = BinaryOpEvaluator::new(&interner);
    let unique_sym = interner.unique_symbol(SymbolRef(1));
    assert!(eval.is_symbol_like(unique_sym));
}

// =============================================================================
// is_boolean_like
// =============================================================================

#[test]
fn test_is_boolean_like() {
    let interner = TypeInterner::new();
    let eval = BinaryOpEvaluator::new(&interner);
    assert!(eval.is_boolean_like(TypeId::BOOLEAN));
    assert!(eval.is_boolean_like(TypeId::ANY));
    assert!(!eval.is_boolean_like(TypeId::STRING));
    assert!(!eval.is_boolean_like(TypeId::NUMBER));
}

#[test]
fn test_is_boolean_like_literal() {
    let interner = TypeInterner::new();
    let eval = BinaryOpEvaluator::new(&interner);
    let t = interner.literal_boolean(true);
    let f = interner.literal_boolean(false);
    assert!(eval.is_boolean_like(t));
    assert!(eval.is_boolean_like(f));
}

// =============================================================================
// is_valid_computed_property_name_type
// =============================================================================

#[test]
fn test_valid_computed_property_name_types() {
    let interner = TypeInterner::new();
    let eval = BinaryOpEvaluator::new(&interner);
    assert!(eval.is_valid_computed_property_name_type(TypeId::STRING));
    assert!(eval.is_valid_computed_property_name_type(TypeId::NUMBER));
    assert!(eval.is_valid_computed_property_name_type(TypeId::SYMBOL));
    assert!(eval.is_valid_computed_property_name_type(TypeId::ANY));
    assert!(eval.is_valid_computed_property_name_type(TypeId::NEVER));
    assert!(eval.is_valid_computed_property_name_type(TypeId::ERROR));
}

#[test]
fn test_invalid_computed_property_name_types() {
    let interner = TypeInterner::new();
    let eval = BinaryOpEvaluator::new(&interner);
    assert!(!eval.is_valid_computed_property_name_type(TypeId::BOOLEAN));
    assert!(!eval.is_valid_computed_property_name_type(TypeId::NULL));
    assert!(!eval.is_valid_computed_property_name_type(TypeId::UNDEFINED));
    assert!(!eval.is_valid_computed_property_name_type(TypeId::VOID));
}

#[test]
fn test_computed_property_name_string_literal() {
    let interner = TypeInterner::new();
    let eval = BinaryOpEvaluator::new(&interner);
    let lit = interner.literal_string("key");
    assert!(eval.is_valid_computed_property_name_type(lit));
}

#[test]
fn test_computed_property_name_number_literal() {
    let interner = TypeInterner::new();
    let eval = BinaryOpEvaluator::new(&interner);
    let lit = interner.literal_number(0.0);
    assert!(eval.is_valid_computed_property_name_type(lit));
}

#[test]
fn test_computed_property_name_union_valid() {
    let interner = TypeInterner::new();
    let eval = BinaryOpEvaluator::new(&interner);
    let union = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);
    assert!(eval.is_valid_computed_property_name_type(union));
}

#[test]
fn test_computed_property_name_union_invalid() {
    let interner = TypeInterner::new();
    let eval = BinaryOpEvaluator::new(&interner);
    // Union containing boolean makes it invalid
    let union = interner.union(vec![TypeId::STRING, TypeId::BOOLEAN]);
    assert!(!eval.is_valid_computed_property_name_type(union));
}

// =============================================================================
// is_valid_instanceof_left_operand
// =============================================================================

#[test]
fn test_instanceof_left_valid_types() {
    let interner = TypeInterner::new();
    let eval = BinaryOpEvaluator::new(&interner);
    assert!(eval.is_valid_instanceof_left_operand(TypeId::ANY));
    assert!(eval.is_valid_instanceof_left_operand(TypeId::UNKNOWN));
    assert!(eval.is_valid_instanceof_left_operand(TypeId::ERROR));
}

#[test]
fn test_instanceof_left_object_type() {
    let interner = TypeInterner::new();
    let eval = BinaryOpEvaluator::new(&interner);
    let obj = interner.object(vec![PropertyInfo::new(
        interner.intern_string("x"),
        TypeId::NUMBER,
    )]);
    assert!(eval.is_valid_instanceof_left_operand(obj));
}

#[test]
fn test_instanceof_left_primitive_invalid() {
    let interner = TypeInterner::new();
    let eval = BinaryOpEvaluator::new(&interner);
    // Primitive types are not valid left operands for instanceof
    assert!(!eval.is_valid_instanceof_left_operand(TypeId::STRING));
    assert!(!eval.is_valid_instanceof_left_operand(TypeId::NUMBER));
    assert!(!eval.is_valid_instanceof_left_operand(TypeId::BOOLEAN));
}

// =============================================================================
// Unknown operator
// =============================================================================

#[test]
fn test_unknown_operator_returns_type_error() {
    let interner = TypeInterner::new();
    let eval = BinaryOpEvaluator::new(&interner);
    assert_type_error(&eval.evaluate(TypeId::NUMBER, TypeId::NUMBER, "???"));
}

// =============================================================================
// Edge cases: both sides ERROR
// =============================================================================

#[test]
fn test_error_plus_error() {
    let interner = TypeInterner::new();
    let eval = BinaryOpEvaluator::new(&interner);
    // ERROR + ERROR => any + any => any
    assert_success(
        &eval.evaluate(TypeId::ERROR, TypeId::ERROR, "+"),
        TypeId::ANY,
    );
}

#[test]
fn test_error_minus_error() {
    let interner = TypeInterner::new();
    let eval = BinaryOpEvaluator::new(&interner);
    // ERROR - ERROR => any - any => number
    assert_success(
        &eval.evaluate(TypeId::ERROR, TypeId::ERROR, "-"),
        TypeId::NUMBER,
    );
}

// =============================================================================
// String + unknown is error (unknown is not valid string concat operand)
// =============================================================================

#[test]
fn test_string_plus_unknown() {
    let interner = TypeInterner::new();
    let eval = BinaryOpEvaluator::new(&interner);
    // string + unknown => unknown (prevents cascading errors)
    assert_success(
        &eval.evaluate(TypeId::STRING, TypeId::UNKNOWN, "+"),
        TypeId::UNKNOWN,
    );
    assert_success(
        &eval.evaluate(TypeId::UNKNOWN, TypeId::STRING, "+"),
        TypeId::UNKNOWN,
    );
}

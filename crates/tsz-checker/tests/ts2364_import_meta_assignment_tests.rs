//! Tests for TS2364: The left-hand side of an assignment expression must be a
//! variable or a property access.
//!
//! Covers: `import.meta = ...` (TS2364), array/object literals as compound
//! assignment LHS (TS2364), and valid assignment targets.

use tsz_checker::test_utils::check_source_codes as get_error_codes;

fn has_error_with_code(source: &str, code: u32) -> bool {
    get_error_codes(source).contains(&code)
}

fn codes(source: &str) -> Vec<u32> {
    get_error_codes(source)
}

#[test]
fn test_import_meta_direct_assignment_is_invalid() {
    // `import.meta = foo` must emit TS2364 because import.meta itself
    // is not a valid assignment target (it's a meta-property, not a variable
    // or property access).
    let source = r#"
const foo: any = {};
import.meta = foo;
"#;
    assert!(
        has_error_with_code(source, 2364),
        "Should emit TS2364 for direct assignment to import.meta"
    );
}

#[test]
fn test_import_meta_property_assignment_is_valid() {
    // `import.meta.foo = value` is fine — it's a property access on import.meta
    let source = r#"
import.meta.foo = 42;
"#;
    let codes = get_error_codes(source);
    assert!(
        !codes.contains(&2364),
        "Should NOT emit TS2364 for assignment to import.meta.foo; got codes: {codes:?}",
    );
}

#[test]
fn test_import_meta_compound_assignment_is_invalid() {
    // `import.meta += foo` must also emit TS2364
    let source = r#"
import.meta += 1;
"#;
    assert!(
        has_error_with_code(source, 2364),
        "Should emit TS2364 for compound assignment to import.meta"
    );
}

#[test]
fn test_super_compound_exponentiation_no_ts2364() {
    // `super **= 5` — parser emits TS1034 (captured in parse diagnostics, not
    // checker diagnostics). The checker must NOT emit TS2364: the parser creates
    // a PropertyAccessExpression recovery node which is a valid assignment target.
    let c = codes(
        r#"
class A { foo() {} }
class B extends A {
    bar() {
        super **= 5;
    }
}
"#,
    );
    assert!(
        !c.contains(&2364),
        "Must NOT have TS2364 for super **= (parse error suppresses semantic check): {c:?}",
    );
}

#[test]
fn test_super_compound_addition_no_ts2364() {
    // `super += 5` — same invariant as the exponentiation case.
    let c = codes(
        r#"
class A { foo() {} }
class B extends A {
    bar() {
        super += 5;
    }
}
"#,
    );
    assert!(
        !c.contains(&2364),
        "Must NOT have TS2364 for super += (parse error suppresses semantic check): {c:?}",
    );
}

// =========================================================================
// Array/object literal as compound assignment LHS — must emit TS2364
// =========================================================================
//
// Structural rule: for compound assignments (+=, -=, etc.), the LHS must be a
// variable, property access, or element access. Array/object destructuring
// patterns ([a, b] = rhs) are valid for simple assignment (`=`) but NOT for
// compound assignments ([a, b] += rhs is always TS2364). tsc emits TS2364 for
// compound-assignment destructuring; tsz must do the same.

#[test]
fn array_literal_compound_assignment_lhs_emits_ts2364() {
    let source = r#"
let a: number, b: number;
[a, b] += [1, 2];
"#;
    assert!(
        has_error_with_code(source, 2364),
        "Array literal as compound assignment LHS should emit TS2364"
    );
}

#[test]
fn object_literal_compound_assignment_lhs_emits_ts2364() {
    let source = r#"
let a: number;
({a} += {a: 1});
"#;
    assert!(
        has_error_with_code(source, 2364),
        "Object literal as compound assignment LHS should emit TS2364"
    );
}

#[test]
fn simple_array_destructuring_is_valid() {
    // [a, b] = rhs is valid (simple destructuring, not compound)
    let source = r#"
let a: number, b: number;
[a, b] = [1, 2];
"#;
    let codes = get_error_codes(source);
    assert!(
        !codes.contains(&2364),
        "Array destructuring simple assignment must NOT emit TS2364; got: {codes:?}"
    );
}

#[test]
fn function_call_compound_assignment_lhs_emits_ts2364() {
    let source = r#"
declare function f(): number;
f() += 1;
"#;
    assert!(
        has_error_with_code(source, 2364),
        "Function call result as compound assignment LHS should emit TS2364"
    );
}

#[test]
fn array_literal_exponentiation_compound_assignment_lhs_emits_ts2364() {
    let source = r#"
let x: number;
[x] **= 2;
"#;
    assert!(
        has_error_with_code(source, 2364),
        "Array literal as **= LHS should emit TS2364"
    );
}

#[test]
fn simple_object_destructuring_assignment_is_valid() {
    // ({a} = rhs) is valid (simple destructuring, not compound)
    let source = r#"
let a: number;
({a} = {a: 1});
"#;
    let codes = get_error_codes(source);
    assert!(
        !codes.contains(&2364),
        "Object destructuring simple assignment must NOT emit TS2364; got: {codes:?}"
    );
}

#[test]
fn parenthesized_array_literal_compound_assignment_lhs_emits_ts2364() {
    // ([a, b]) += rhs — parenthesized array literal as LHS — is still invalid
    let source = r#"
let a: number, b: number;
([a, b]) += 1;
"#;
    assert!(
        has_error_with_code(source, 2364),
        "Parenthesized array literal as compound assignment LHS should emit TS2364"
    );
}

#[test]
fn all_arithmetic_compound_operators_reject_array_literal_lhs() {
    // All compound arithmetic operators should reject array literal LHS,
    // not just +=. The rule is structural, not operator-specific.
    for op in ["-=", "*=", "/=", "%=", "**="] {
        let source = format!("let a: number, b: number;\n[a, b] {op} 1;\n",);
        assert!(
            has_error_with_code(&source, 2364),
            "Array literal as {op} LHS should emit TS2364"
        );
    }
}

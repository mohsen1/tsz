//! Tests for TS2364: The left-hand side of an assignment expression must be a
//! variable or a property access.
//!
//! Specifically tests that `import.meta = ...` is rejected (TS2364) while
//! `import.meta.prop = ...` is allowed (it's a real property access).

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

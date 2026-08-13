//! TS18016 / TS1451: a bare `PrivateIdentifier` reached as a standalone
//! expression, outside the three positions where it is legal (member-access
//! name, class-member declaration, direct unparenthesized LHS of `in`).
//!
//! Structural rule: mirrors tsc's `checkGrammarPrivateIdentifierExpression`.
//! When a `PrivateIdentifier` is checked as an expression, tsc reports
//! TS18016 ("Private identifiers are not allowed outside class bodies")
//! if there is no enclosing class at all, and TS1451 ("Private identifiers
//! are only allowed in class bodies...") if there is one but this position
//! is still invalid. The direct (unparenthesized) LHS of `in`, and member
//! access, are the only expression-shaped positions that are valid — both
//! are owned by other checkers, which must fire exactly once, not twice
//! with this grammar check.
//!
//! Owner layer: checker, expression dispatcher's `PrivateIdentifier` case
//! (`crates/tsz-checker/src/dispatch/mod.rs`) plus
//! `check_bare_private_identifier_expression` in
//! `crates/tsz-checker/src/types/type_checking/core.rs`.
//!
//! Oracle: `typescript@7.0.2`, `--noEmit --strict false --target es2022`.
//!
//! Binder names are varied throughout so no assertion can pass by matching a
//! particular identifier spelling.

use tsz_checker::test_utils::check_source_codes;

fn count(codes: &[u32], code: u32) -> usize {
    codes.iter().filter(|&&c| c == code).count()
}

// ============================================================
// Outside any class — TS18016
// ============================================================

#[test]
fn ts18016_assignment_rhs_outside_class() {
    let codes = check_source_codes("const z = #a;");
    assert_eq!(
        count(&codes, 18016),
        1,
        "expected TS18016 for `#a` as an assignment RHS outside a class, got: {codes:?}"
    );
}

#[test]
fn ts18016_return_statement_outside_class() {
    let codes = check_source_codes(
        r#"
function f() {
    return #a;
}
"#,
    );
    assert_eq!(
        count(&codes, 18016),
        1,
        "expected TS18016 for `#a` in a return statement outside a class, got: {codes:?}"
    );
}

#[test]
fn ts18016_array_element_outside_class() {
    let codes = check_source_codes("const arr = [#a];");
    assert_eq!(
        count(&codes, 18016),
        1,
        "expected TS18016 for `#a` as an array element outside a class, got: {codes:?}"
    );
}

#[test]
fn ts18016_call_argument_outside_class() {
    let codes = check_source_codes(
        r#"
declare function f(x: unknown): void;
f(#a);
"#,
    );
    assert_eq!(
        count(&codes, 18016),
        1,
        "expected TS18016 for `#a` as a call argument outside a class, got: {codes:?}"
    );
}

#[test]
fn ts18016_expression_statement_outside_class() {
    let codes = check_source_codes("#a;");
    assert_eq!(
        count(&codes, 18016),
        1,
        "expected TS18016 for a bare `#a;` expression statement outside a class, got: {codes:?}"
    );
}

#[test]
fn ts18016_binary_operand_outside_class() {
    let codes = check_source_codes("const z = #a + 1;");
    assert_eq!(
        count(&codes, 18016),
        1,
        "expected TS18016 for `#a` as a binary operand outside a class, got: {codes:?}"
    );
}

#[test]
fn ts18016_parenthesized_standalone_outside_class() {
    let codes = check_source_codes("(#a);");
    assert_eq!(
        count(&codes, 18016),
        1,
        "expected TS18016 for a parenthesized standalone `#a` outside a class, got: {codes:?}"
    );
}

/// `(#a) in {}` — parenthesizing the LHS of `in` makes it a standalone
/// expression again. Outside a class this must be TS18016, not TS1451: the
/// containing-class check runs before the in-LHS structural check.
#[test]
fn ts18016_parenthesized_in_lhs_outside_class() {
    let codes = check_source_codes("(#a) in {};");
    assert_eq!(
        count(&codes, 18016),
        1,
        "expected TS18016 for `(#a) in {{}}` outside a class, got: {codes:?}"
    );
    assert_eq!(
        count(&codes, 1451),
        0,
        "must not also/instead report TS1451 outside a class, got: {codes:?}"
    );
}

// ============================================================
// Inside a class, invalid position — TS1451
// ============================================================

#[test]
fn ts1451_return_statement_inside_class() {
    let codes = check_source_codes(
        r#"
class C {
    m() {
        return #a;
    }
}
"#,
    );
    assert_eq!(
        count(&codes, 1451),
        1,
        "expected TS1451 for `#a` in a return statement inside a class, got: {codes:?}"
    );
    assert_eq!(
        count(&codes, 18016),
        0,
        "must not also report TS18016 inside a class, got: {codes:?}"
    );
}

#[test]
fn ts1451_expression_statement_inside_class() {
    let codes = check_source_codes(
        r#"
class Widget {
    m() {
        #secretField;
    }
}
"#,
    );
    assert_eq!(
        count(&codes, 1451),
        1,
        "expected TS1451 for a bare `#secretField;` statement inside a class, got: {codes:?}"
    );
}

/// `(#field) in v` inside a class — parenthesizing still makes it invalid
/// (TS1451, not the valid direct-LHS path), with renamed binders from the
/// existing `private_brands.rs` coverage of this exact shape.
#[test]
fn ts1451_parenthesized_in_lhs_inside_class_renamed_binders() {
    let codes = check_source_codes(
        r#"
class Widget {
    #secretField = 1;
    check(candidate: unknown) {
        return (#secretField) in candidate;
    }
}
"#,
    );
    assert_eq!(
        count(&codes, 1451),
        1,
        "expected exactly 1 TS1451 for a parenthesized private-identifier in-LHS inside a class, got: {codes:?}"
    );
    assert_eq!(
        count(&codes, 18016),
        0,
        "must not also report TS18016 inside a class, got: {codes:?}"
    );
}

// ============================================================
// Valid positions stay clean — must not double-report
// ============================================================

#[test]
fn valid_direct_in_lhs_inside_class_stays_clean() {
    let codes = check_source_codes(
        r#"
class Widget {
    #secretField = 1;
    check(candidate: unknown) {
        return #secretField in candidate;
    }
}
"#,
    );
    assert_eq!(
        count(&codes, 18016) + count(&codes, 1451),
        0,
        "a direct, unparenthesized `#field in x` inside its declaring class must stay clean, got: {codes:?}"
    );
}

#[test]
fn valid_member_access_stays_clean() {
    let codes = check_source_codes(
        r#"
class Widget {
    #secretField = 1;
    read() {
        return this.#secretField;
    }
}
"#,
    );
    assert_eq!(
        count(&codes, 18016) + count(&codes, 1451),
        0,
        "`this.#field` member access must stay clean, got: {codes:?}"
    );
}

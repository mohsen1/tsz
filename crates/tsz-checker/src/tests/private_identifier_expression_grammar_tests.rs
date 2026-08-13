//! Regression tests for the private-identifier expression grammar check
//! (tsc's `checkGrammarPrivateIdentifierExpression`), the TS18016 slice of
//! #16279's private-identifier family surfaced as #17308.
//!
//! A `PrivateIdentifier` (`#field`) is legal in only three positions: the name
//! of a member access (`obj.#field`), the direct left-hand side of an `in`
//! expression (`#field in obj`), or a class-member declaration. Anywhere else
//! it is a standalone expression, which tsc rejects:
//!
//! - **TS18016** (`Private identifiers are not allowed outside class bodies`)
//!   when the name is outside any class body.
//! - **TS1451** (`... may only be used as part of a class member declaration,
//!   property access, or on the left-hand-side of an 'in' expression`) when it
//!   is inside a class body but still in an invalid position.
//!
//! Before the fix tsz was silent for the standalone-expression case in every
//! position (a false negative — it accepted code tsc rejects) and mis-coded
//! the parenthesized-`in` case as TS1451 even outside a class.
//!
//! Every expectation is pinned against a live
//! `tsc@7.0.2 --noEmit --strict false --target es2022` run. The two valid
//! expression positions (member access, direct-`in`-LHS) are asserted to still
//! report **exactly once** — the grammar check must not double them. Binder
//! names are varied (never `#foo` / `C`) so no expectation can be satisfied by
//! a name-shaped predicate.

use crate::test_utils::{check_source_codes, check_source_diagnostics};

/// Count of `code` diagnostics in `source`. Counting rather than membership:
/// `error_at_node` deduplicates by `(start, code)`, so a doubled emission at
/// distinct positions would still be invisible to a `contains` assertion.
fn count_code(source: &str, code: u32) -> usize {
    check_source_diagnostics(source)
        .iter()
        .filter(|d| d.code == code)
        .count()
}

// --- Outside any class body: TS18016 in every standalone position ---

#[test]
fn bare_private_id_in_variable_initializer_outside_class_reports_ts18016() {
    let source = "const seat = #handle;\nexport {};\n";
    assert_eq!(
        count_code(source, 18016),
        1,
        "got {:?}",
        check_source_codes(source)
    );
    assert_eq!(count_code(source, 1451), 0);
}

#[test]
fn bare_private_id_in_return_outside_class_reports_ts18016() {
    let source = "function pull() { return #handle; }\nexport {};\n";
    assert_eq!(
        count_code(source, 18016),
        1,
        "got {:?}",
        check_source_codes(source)
    );
}

#[test]
fn bare_private_id_in_expression_statement_outside_class_reports_ts18016() {
    let source = "#handle;\nexport {};\n";
    assert_eq!(
        count_code(source, 18016),
        1,
        "got {:?}",
        check_source_codes(source)
    );
}

#[test]
fn bare_private_id_in_array_literal_outside_class_reports_ts18016() {
    let source = "const list = [#handle];\nexport {};\n";
    assert_eq!(
        count_code(source, 18016),
        1,
        "got {:?}",
        check_source_codes(source)
    );
}

#[test]
fn bare_private_id_in_call_argument_outside_class_reports_ts18016() {
    let source = "declare function take(v: unknown): void;\ntake(#handle);\nexport {};\n";
    assert_eq!(
        count_code(source, 18016),
        1,
        "got {:?}",
        check_source_codes(source)
    );
}

#[test]
fn bare_private_id_as_binary_operand_outside_class_reports_ts18016() {
    let source = "const n = #handle + 1;\nexport {};\n";
    assert_eq!(
        count_code(source, 18016),
        1,
        "got {:?}",
        check_source_codes(source)
    );
}

#[test]
fn parenthesized_private_id_outside_class_reports_ts18016() {
    let source = "(#handle);\nexport {};\n";
    assert_eq!(
        count_code(source, 18016),
        1,
        "got {:?}",
        check_source_codes(source)
    );
}

#[test]
fn double_parenthesized_private_id_outside_class_reports_ts18016() {
    let source = "const wrapped = ((#handle));\nexport {};\n";
    assert_eq!(
        count_code(source, 18016),
        1,
        "got {:?}",
        check_source_codes(source)
    );
}

/// `(#handle) in obj` outside a class: the private identifier is *not* the
/// direct LHS of `in`, so it is a standalone expression — TS18016, not TS1451.
#[test]
fn parenthesized_private_id_as_in_lhs_outside_class_reports_ts18016_not_ts1451() {
    let source = "declare const box: object;\nconst has = (#handle) in box;\nexport {};\n";
    assert_eq!(
        count_code(source, 18016),
        1,
        "got {:?}",
        check_source_codes(source)
    );
    assert_eq!(
        count_code(source, 1451),
        0,
        "got {:?}",
        check_source_codes(source)
    );
}

// --- Inside a class body, invalid position: TS1451 ---

#[test]
fn bare_private_id_in_return_inside_class_reports_ts1451() {
    let source = "class Widget { render() { return #slot; } }\nexport {};\n";
    assert_eq!(
        count_code(source, 1451),
        1,
        "got {:?}",
        check_source_codes(source)
    );
    assert_eq!(count_code(source, 18016), 0);
}

#[test]
fn bare_private_id_in_variable_initializer_inside_class_reports_ts1451() {
    let source = "class Widget { render() { const q = #slot; return q; } }\nexport {};\n";
    assert_eq!(
        count_code(source, 1451),
        1,
        "got {:?}",
        check_source_codes(source)
    );
}

#[test]
fn bare_private_id_in_expression_statement_inside_class_reports_ts1451() {
    let source = "class Widget { render() { #slot; } }\nexport {};\n";
    assert_eq!(
        count_code(source, 1451),
        1,
        "got {:?}",
        check_source_codes(source)
    );
}

/// `(#slot) in this` inside a class: parenthesized, so standalone — TS1451.
#[test]
fn parenthesized_private_id_as_in_lhs_inside_class_reports_ts1451() {
    let source = "class Widget { has() { return (#slot) in this; } }\nexport {};\n";
    assert_eq!(
        count_code(source, 1451),
        1,
        "got {:?}",
        check_source_codes(source)
    );
    assert_eq!(count_code(source, 18016), 0);
}

// --- The two valid expression positions must report EXACTLY ONCE (no double) ---

#[test]
fn member_access_private_id_outside_class_reports_ts18016_exactly_once() {
    let source = "declare const box: any;\nconst v = box.#handle;\nexport {};\n";
    assert_eq!(
        count_code(source, 18016),
        1,
        "member access is owned by the property-access checker and must not double: got {:?}",
        check_source_codes(source)
    );
    assert_eq!(count_code(source, 1451), 0);
}

#[test]
fn direct_in_lhs_private_id_outside_class_reports_ts18016_exactly_once() {
    let source = "declare const box: object;\nconst has = #handle in box;\nexport {};\n";
    assert_eq!(
        count_code(source, 18016),
        1,
        "direct `in`-LHS is owned by the `in`-operator checker and must not double: got {:?}",
        check_source_codes(source)
    );
    assert_eq!(count_code(source, 1451), 0);
}

// --- Valid uses stay clean (no false positive) ---

#[test]
fn valid_in_brand_check_and_member_access_inside_class_are_clean() {
    let source = "class Vault {\n  #token = 1;\n  has(o: object) { return #token in o; }\n  read() { return this.#token; }\n}\nexport {};\n";
    assert_eq!(
        count_code(source, 18016),
        0,
        "got {:?}",
        check_source_codes(source)
    );
    assert_eq!(
        count_code(source, 1451),
        0,
        "got {:?}",
        check_source_codes(source)
    );
}

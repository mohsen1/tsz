//! Tests that TS2322 return-type mismatches anchor on the *returned expression*,
//! not on the `return` keyword.
//!
//! Structural rule: when a return statement has an expression and that
//! expression's type is not assignable to the declared return type, tsc anchors
//! the TS2322 diagnostic at the returned expression, not at the `return` keyword.
//!
//! This matters for fingerprint-accurate conformance: the `return` keyword lives
//! at a different column than the expression it returns.

use tsz_checker::test_utils::check_source_diagnostics;

fn expr_offset_after(source: &str, needle: &str, prefix: &str) -> u32 {
    source
        .find(needle)
        .unwrap_or_else(|| panic!("{needle:?} not found in source")) as u32
        + prefix.len() as u32
}

/// Indexed-access return: `Things[A1]` returned where `Things[A2]` is expected.
/// tsc anchors TS2322 at the returned identifier, not the `return` keyword.
/// Runs with two type-parameter name pairs (anti-hardcoding §25).
#[test]
fn indexed_access_return_anchors_at_expression_not_return_keyword() {
    for (a1, a2) in [("K1", "K2"), ("T1", "T2")] {
        let source = format!(
            r#"
interface Things {{
    a: {{ id?: string }};
}}
function f<{a1} extends keyof Things, {a2} extends keyof Things>(p: Things[{a1}]): Things[{a2}] {{
    return p;
}}
"#
        );
        let diags = check_source_diagnostics(&source);
        let ts2322: Vec<_> = diags.iter().filter(|d| d.code == 2322).collect();
        assert_eq!(
            ts2322.len(),
            1,
            "({a1}/{a2}) expected exactly one TS2322; got: {diags:#?}"
        );
        let expected = expr_offset_after(&source, "    return p", "    return ");
        assert_eq!(
            ts2322[0].start, expected,
            "({a1}/{a2}) TS2322 must anchor at returned `p` (offset {expected}), got offset {}",
            ts2322[0].start
        );
    }
}

/// Simple concrete mismatch: `return 42` in a `string`-returning function.
/// The error must point at `42`, not at `return`.
#[test]
fn simple_primitive_mismatch_anchors_at_returned_expression() {
    let source = "function f(): string { return 42; }\n";
    let diags = check_source_diagnostics(source);
    let ts2322: Vec<_> = diags.iter().filter(|d| d.code == 2322).collect();
    assert_eq!(
        ts2322.len(),
        1,
        "expected exactly one TS2322 for `return 42` in string fn; got: {diags:#?}"
    );
    let expected = source.find("42").expect("42 in source") as u32;
    assert_eq!(
        ts2322[0].start, expected,
        "TS2322 must anchor at `42` (offset {expected}), got offset {}",
        ts2322[0].start
    );
}

/// `return x` where `x` has a concrete type incompatible with declared return.
/// The error must point at the identifier `x`, not at the `return` keyword.
#[test]
fn identifier_return_mismatch_anchors_at_identifier() {
    let source = "function f(x: number): string { return x; }\n";
    let diags = check_source_diagnostics(source);
    let ts2322: Vec<_> = diags.iter().filter(|d| d.code == 2322).collect();
    assert_eq!(
        ts2322.len(),
        1,
        "expected exactly one TS2322 for `return x` (number vs string); got: {diags:#?}"
    );
    let expected = expr_offset_after(source, "return x", "return ");
    assert_eq!(
        ts2322[0].start, expected,
        "TS2322 must anchor at the returned `x` (offset {expected}), got offset {}",
        ts2322[0].start
    );
}

/// Bare `return;` in a void function produces no TS2322.
#[test]
fn bare_return_void_fn_no_ts2322() {
    let source = "function f(): void { return; }\n";
    let diags = check_source_diagnostics(source);
    let count = diags.iter().filter(|d| d.code == 2322).count();
    assert_eq!(
        count, 0,
        "bare `return;` in void fn must not produce TS2322; got: {diags:#?}"
    );
}

/// Multi-key indexed access — verify the rule holds for a multi-element map.
#[test]
fn multi_element_indexed_access_return_anchors_at_expression() {
    let source = r#"
interface A { x?: number }
interface B { y?: string }
interface Map {
    a: A;
    b: B;
}
function pick<X extends keyof Map, Y extends keyof Map>(v: Map[X]): Map[Y] {
    return v;
}
"#;
    let diags = check_source_diagnostics(source);
    let ts2322: Vec<_> = diags.iter().filter(|d| d.code == 2322).collect();
    assert_eq!(
        ts2322.len(),
        1,
        "expected exactly one TS2322 for deferred-indexed cross-return; got: {diags:#?}"
    );
    let expected = expr_offset_after(source, "    return v", "    return ");
    assert_eq!(
        ts2322[0].start, expected,
        "TS2322 must anchor at returned `v` (offset {expected}), got offset {}",
        ts2322[0].start
    );
}

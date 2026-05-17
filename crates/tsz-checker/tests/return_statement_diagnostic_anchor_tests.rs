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

/// Indexed-access return: `Things[K1]` returned where `Things[K2]` is expected.
/// tsc anchors TS2322 at the returned identifier, not the `return` keyword.
#[test]
fn indexed_access_return_anchors_at_expression_not_return_keyword() {
    let source = r#"
interface Things {
    a: { id?: string };
}
function f<K1 extends keyof Things, K2 extends keyof Things>(p: Things[K1]): Things[K2] {
    return p;
}
"#;
    let diags = check_source_diagnostics(source);
    let ts2322: Vec<_> = diags.iter().filter(|d| d.code == 2322).collect();
    assert_eq!(
        ts2322.len(),
        1,
        "expected exactly one TS2322; got: {diags:#?}"
    );
    let diag = ts2322[0];

    // Find the offset of `p` in `return p` (the last `p` in the source before `;`)
    let return_p_offset = source.find("    return p").expect("return p in source") as u32
        + "    return ".len() as u32;

    let return_keyword_offset =
        source.find("    return p").expect("return p") as u32 + "    ".len() as u32;

    assert_ne!(
        diag.start, return_keyword_offset,
        "TS2322 must NOT anchor at the `return` keyword"
    );
    assert_eq!(
        diag.start, return_p_offset,
        "TS2322 must anchor at returned expression `p` (offset {return_p_offset}), got offset {}",
        diag.start
    );
}

/// Same rule with alt type-parameter names (anti-hardcoding: the fix must not
/// match on `K1`/`K2` literally).
#[test]
fn indexed_access_return_anchors_at_expression_alt_names() {
    let source = r#"
interface Things {
    a: { id?: string };
}
function f<T1 extends keyof Things, T2 extends keyof Things>(p: Things[T1]): Things[T2] {
    return p;
}
"#;
    let diags = check_source_diagnostics(source);
    let ts2322: Vec<_> = diags.iter().filter(|d| d.code == 2322).collect();
    assert_eq!(
        ts2322.len(),
        1,
        "expected exactly one TS2322 with alt names; got: {diags:#?}"
    );
    let diag = ts2322[0];
    let return_p_offset = source.find("    return p").expect("return p in source") as u32
        + "    return ".len() as u32;
    let return_keyword_offset =
        source.find("    return p").expect("return p") as u32 + "    ".len() as u32;
    assert_ne!(
        diag.start, return_keyword_offset,
        "TS2322 must NOT anchor at the `return` keyword (T1/T2 variant)"
    );
    assert_eq!(
        diag.start, return_p_offset,
        "TS2322 must anchor at returned expression `p` (offset {return_p_offset}), got offset {}",
        diag.start
    );
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
    let diag = ts2322[0];
    let expr_offset = source.find("42").expect("42 in source") as u32;
    assert_eq!(
        diag.start, expr_offset,
        "TS2322 must anchor at `42` (offset {expr_offset}), got offset {}",
        diag.start
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
    let diag = ts2322[0];
    // `x` appears three times; the returned one is the last occurrence.
    let last_x = source.rfind('x').expect("x in source") as u32;
    assert_eq!(
        diag.start, last_x,
        "TS2322 must anchor at the returned `x` (offset {last_x}), got offset {}",
        diag.start
    );
}

/// When the returned expression IS the `return` statement (no expression),
/// no TS2322 fires for a void function returning nothing.
#[test]
fn bare_return_void_fn_no_ts2322() {
    let source = "function f(): void { return; }\n";
    let diags = check_source_diagnostics(source);
    let ts2322_count = diags.iter().filter(|d| d.code == 2322).count();
    assert_eq!(
        ts2322_count, 0,
        "bare `return;` in void fn must not produce TS2322; got: {diags:#?}"
    );
}

/// Multi-key indexed access map — verify the rule holds for a multi-element
/// `Things` interface as well.
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
    let diag = ts2322[0];
    let return_v_offset = source.find("    return v").expect("return v in source") as u32
        + "    return ".len() as u32;
    let return_keyword_offset =
        source.find("    return v").expect("return v") as u32 + "    ".len() as u32;
    assert_ne!(
        diag.start, return_keyword_offset,
        "TS2322 must NOT anchor at the `return` keyword (multi-element variant)"
    );
    assert_eq!(
        diag.start, return_v_offset,
        "TS2322 must anchor at returned expression `v` (offset {return_v_offset}), got offset {}",
        diag.start
    );
}

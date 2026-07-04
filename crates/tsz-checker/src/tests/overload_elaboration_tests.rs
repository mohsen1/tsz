//! Regression tests for tsc's per-overload `TS2772` elaboration under a
//! `TS2769` ("No overload matches this call.") diagnostic.
//!
//! When a call matches no overload and 2 or 3 candidate signatures reached
//! argument checking, tsc wraps each candidate's applicability error in a
//! `Overload {n} of {total}, '{signature}', gave the following error.` chain
//! node (`TS2772`) — in declaration order, with the applicability error nested
//! one level deeper. A single failing candidate collapses to a plain `TS2345`
//! (no `TS2769`), and `>3` candidates use tsc's distinct "last overload" shape,
//! left as the flat fallback here.
//!
//! Before the fix tsz defined the `TS2772` string but never emitted it: it
//! flattened every candidate's argument error directly under `TS2769` and, due
//! to the related-info `(file, start, depth, message)` sort, rendered them out
//! of declaration order.
//!
//! See `crates/tsz-checker/src/error_reporter/call_errors/error_emission.rs`
//! (`error_no_overload_matches_at`) and
//! `crates/tsz-checker/src/error_reporter/fingerprint_policy.rs`
//! (`RelatedInformationPolicy::OVERLOAD_CHAINS`).

use crate::test_utils::check_source_diagnostics;

/// The `(depth, code, message)` triples of the sole `TS2769` diagnostic's
/// related information, in emission order.
fn overload_chain(source: &str) -> Vec<(u8, u32, String)> {
    let diags = check_source_diagnostics(source);
    let ts2769: Vec<_> = diags.iter().filter(|d| d.code == 2769).collect();
    assert_eq!(
        ts2769.len(),
        1,
        "Expected exactly one TS2769. Got: {:?}",
        diags
            .iter()
            .map(|d| (d.code, &d.message_text))
            .collect::<Vec<_>>()
    );
    ts2769[0]
        .related_information
        .iter()
        .map(|r| (r.depth, r.code, r.message_text.clone()))
        .collect()
}

/// Two failing overloads: both wrapped in `TS2772`, in declaration order, each
/// header at depth 0 with its applicability error nested at depth 1. The
/// signature is the `signatureToString` colon form, not the `=>` function-type
/// form.
#[test]
fn two_failing_overloads_wrap_each_candidate_in_declaration_order() {
    let chain = overload_chain(
        r#"
declare function f(x: number): number;
declare function f(x: string): string;
f(true);
"#,
    );

    assert_eq!(
        chain,
        vec![
            (
                0,
                2772,
                "Overload 1 of 2, '(x: number): number', gave the following error.".to_string()
            ),
            (
                1,
                2345,
                "Argument of type 'boolean' is not assignable to parameter of type 'number'."
                    .to_string()
            ),
            (
                0,
                2772,
                "Overload 2 of 2, '(x: string): string', gave the following error.".to_string()
            ),
            (
                1,
                2345,
                "Argument of type 'boolean' is not assignable to parameter of type 'string'."
                    .to_string()
            ),
        ],
        "expected two declaration-ordered TS2772 chains with the colon signature form"
    );
}

/// Three failing overloads: all wrapped, `of 3`, and declaration order is
/// preserved rather than re-sorted. The bodies' alphabetical order
/// (`boolean` < `number` < `string`) differs from declaration order
/// (`number`, `string`, `boolean`); the fix must keep declaration order.
#[test]
fn three_failing_overloads_preserve_declaration_order() {
    let chain = overload_chain(
        r#"
declare function g(x: number): number;
declare function g(x: string): string;
declare function g(x: boolean): boolean;
g({});
"#,
    );

    let headers: Vec<&String> = chain
        .iter()
        .filter(|(_, code, _)| *code == 2772)
        .map(|(_, _, m)| m)
        .collect();
    assert_eq!(
        headers,
        vec![
            "Overload 1 of 3, '(x: number): number', gave the following error.",
            "Overload 2 of 3, '(x: string): string', gave the following error.",
            "Overload 3 of 3, '(x: boolean): boolean', gave the following error.",
        ],
        "expected declaration-ordered `of 3` headers, got: {chain:?}"
    );

    // Bodies stay under their header in declaration order (number, string,
    // boolean) — not the alphabetical (boolean, number, string) the old
    // unconditional sort produced.
    let bodies: Vec<&String> = chain
        .iter()
        .filter(|(_, code, _)| *code == 2345)
        .map(|(_, _, m)| m)
        .collect();
    assert_eq!(
        bodies,
        vec![
            "Argument of type '{}' is not assignable to parameter of type 'number'.",
            "Argument of type '{}' is not assignable to parameter of type 'string'.",
            "Argument of type '{}' is not assignable to parameter of type 'boolean'.",
        ],
        "expected declaration-ordered bodies, got: {chain:?}"
    );
}

/// Anti-hardcoding: the wrapping is structural, independent of the callee and
/// parameter binder names. Renaming them only changes the rendered signature.
#[test]
fn wrapping_is_independent_of_binder_names() {
    let chain = overload_chain(
        r#"
declare function pick(value: number): number;
declare function pick(other: string): string;
pick(true);
"#,
    );

    let headers: Vec<&String> = chain
        .iter()
        .filter(|(_, code, _)| *code == 2772)
        .map(|(_, _, m)| m)
        .collect();
    assert_eq!(
        headers,
        vec![
            "Overload 1 of 2, '(value: number): number', gave the following error.",
            "Overload 2 of 2, '(other: string): string', gave the following error.",
        ],
        "signature renders the declared parameter names, got: {chain:?}"
    );
}

/// Constructor (`new`) overloads are wrapped identically, and tsc's
/// `signatureToString` renders them in the call-signature colon form
/// (`(x: number): C`) — no `new` prefix.
#[test]
fn constructor_overloads_wrap_in_call_signature_form() {
    let chain = overload_chain(
        r#"
declare class C {
    constructor(x: number);
    constructor(x: string);
}
new C(true);
"#,
    );

    let headers: Vec<&String> = chain
        .iter()
        .filter(|(_, code, _)| *code == 2772)
        .map(|(_, _, m)| m)
        .collect();
    assert_eq!(
        headers,
        vec![
            "Overload 1 of 2, '(x: number): C', gave the following error.",
            "Overload 2 of 2, '(x: string): C', gave the following error.",
        ],
        "constructor overloads render as call signatures returning the class, got: {chain:?}"
    );
    assert!(
        headers.iter().all(|h| !h.contains("new ")),
        "constructor overload headers must not carry a `new` prefix, got: {headers:?}"
    );
}

/// The literal-source generalization stays applied inside each wrapped error:
/// a fresh boolean-literal argument widens to `boolean` against non-singleton
/// parameters, exactly as on the single-overload TS2345 path.
#[test]
fn literal_source_generalization_survives_wrapping() {
    let chain = overload_chain(
        r#"
declare function f(x: number): number;
declare function f(x: string): string;
f(true);
"#,
    );

    assert!(
        chain
            .iter()
            .all(|(_, _, m)| !m.contains("Argument of type 'true'")),
        "the raw boolean literal must not leak into the wrapped elaboration, got: {chain:?}"
    );
    assert!(
        chain
            .iter()
            .any(|(_, code, m)| *code == 2345 && m.contains("Argument of type 'boolean'")),
        "expected the widened `boolean` source inside a wrapped body, got: {chain:?}"
    );
}

/// A single failing signature collapses to a plain `TS2345` with no `TS2769`
/// wrapper at all (tsc's single-candidate branch).
#[test]
fn single_signature_stays_plain_ts2345() {
    let diags = check_source_diagnostics(
        r#"
declare function f(x: number): number;
f(true);
"#,
    );

    assert!(
        diags.iter().any(|d| d.code == 2345),
        "expected a plain TS2345, got: {:?}",
        diags.iter().map(|d| d.code).collect::<Vec<_>>()
    );
    assert!(
        !diags.iter().any(|d| d.code == 2769),
        "a single failing signature must not produce TS2769, got: {:?}",
        diags.iter().map(|d| d.code).collect::<Vec<_>>()
    );
    assert!(
        !diags.iter().any(|d| d.code == 2772),
        "a single failing signature must not produce TS2772, got: {:?}",
        diags.iter().map(|d| d.code).collect::<Vec<_>>()
    );
}

/// More than three failing candidates keep tsc's distinct top-level shape:
/// tsz leaves them as the flat fallback (no `TS2772` wrappers), and the
/// top-level `TS2769` is unchanged.
#[test]
fn more_than_three_overloads_stay_flat() {
    let diags = check_source_diagnostics(
        r#"
declare function f(x: number): number;
declare function f(x: string): string;
declare function f(x: boolean): boolean;
declare function f(x: symbol): symbol;
f({});
"#,
    );

    let ts2769: Vec<_> = diags.iter().filter(|d| d.code == 2769).collect();
    assert_eq!(
        ts2769.len(),
        1,
        "expected the top-level TS2769 to be unchanged"
    );
    assert!(
        !ts2769[0].related_information.iter().any(|r| r.code == 2772),
        "the >3-candidate case must not emit TS2772 wrappers, got: {:?}",
        ts2769[0]
            .related_information
            .iter()
            .map(|r| (r.code, &r.message_text))
            .collect::<Vec<_>>()
    );
}

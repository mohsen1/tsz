//! Regression tests for tsc's per-overload TS2772 elaboration under a
//! TS2769 ("No overload matches this call") failure.
//!
//! When a call matches no overload and 2 or 3 candidate signatures reached
//! argument checking, tsc groups the failure per candidate: the top-level
//! TS2769 is followed, for each candidate, by a TS2772 chain node
//! `Overload {n} of {m}, '{signature}', gave the following error.` (depth 0)
//! with that candidate's applicability error nested one level deeper (depth 1),
//! in declaration order. A single failing candidate collapses to a plain
//! TS2345 (no TS2769); more than 3 candidates keep the flat rendering.
//!
//! See `crates/tsz-checker/src/error_reporter/call_errors/error_emission.rs`
//! (`error_no_overload_matches_at`), the solver's `OverloadElaboration`
//! (`tsz_solver::operations`), and the `preserve_order` policy in
//! `crates/tsz-checker/src/error_reporter/fingerprint_policy.rs`.
//!
//! Binder names (function, parameter, class) are varied per case so the
//! elaboration is driven by the structural overload set, not any identifier.

use crate::test_utils::check_source_diagnostics;

/// The `(code, depth, message)` triples of the single TS2769 diagnostic's
/// related information, in emitted order.
fn overload_elaboration(source: &str) -> Vec<(u32, u8, String)> {
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
        .map(|r| (r.code, r.depth, r.message_text.clone()))
        .collect()
}

/// Two failing overloads: each candidate is wrapped in a TS2772 header at
/// depth 0 with its applicability error nested at depth 1, in declaration
/// order.
#[test]
fn two_failing_overloads_wrap_each_candidate_in_declaration_order() {
    let chain = overload_elaboration(
        r#"
declare function pick(value: number): number;
declare function pick(value: string): string;
pick(true);
"#,
    );

    assert_eq!(
        chain,
        vec![
            (
                2772,
                0,
                "Overload 1 of 2, '(value: number): number', gave the following error.".to_string()
            ),
            (
                2345,
                1,
                "Argument of type 'boolean' is not assignable to parameter of type 'number'."
                    .to_string()
            ),
            (
                2772,
                0,
                "Overload 2 of 2, '(value: string): string', gave the following error.".to_string()
            ),
            (
                2345,
                1,
                "Argument of type 'boolean' is not assignable to parameter of type 'string'."
                    .to_string()
            ),
        ],
        "expected header/child pairs in declaration order"
    );
}

/// Three failing overloads: all three are wrapped with an `of 3` total and the
/// declared order is preserved (the old flat path re-sorted them by message).
#[test]
fn three_failing_overloads_preserve_declaration_order_not_alphabetical() {
    let chain = overload_elaboration(
        r#"
declare function convert(input: number): number;
declare function convert(input: string): string;
declare function convert(input: boolean): boolean;
convert(null);
"#,
    );

    let headers: Vec<&String> = chain
        .iter()
        .filter(|(code, _, _)| *code == 2772)
        .map(|(_, _, m)| m)
        .collect();
    assert_eq!(
        headers,
        vec![
            "Overload 1 of 3, '(input: number): number', gave the following error.",
            "Overload 2 of 3, '(input: string): string', gave the following error.",
            "Overload 3 of 3, '(input: boolean): boolean', gave the following error.",
        ],
        "declaration order (number, string, boolean) must be preserved, not sorted"
    );
    // Each header is immediately followed by its own depth-1 applicability error.
    assert_eq!(chain.len(), 6, "3 headers + 3 nested errors: {chain:?}");
    for pair in chain.chunks(2) {
        assert_eq!(pair[0].0, 2772, "header code: {pair:?}");
        assert_eq!(pair[0].1, 0, "header depth: {pair:?}");
        assert_eq!(pair[1].1, 1, "nested error depth: {pair:?}");
    }
}

/// The wrapper header carries TS2772 at depth 0 and the applicability error is
/// nested at depth 1 (renamed binders only change the rendered signature text).
#[test]
fn wrapper_uses_ts2772_at_depth_zero_with_nested_error_at_depth_one() {
    let chain = overload_elaboration(
        r#"
declare function transform(arg: number): void;
declare function transform(arg: string): void;
transform(false);
"#,
    );

    assert_eq!(
        chain.first().map(|(code, depth, _)| (*code, *depth)),
        Some((2772, 0)),
        "first entry is the TS2772 header at depth 0: {chain:?}"
    );
    assert_eq!(
        chain.get(1).map(|(code, depth, _)| (*code, *depth)),
        Some((2345, 1)),
        "second entry is the nested applicability error at depth 1: {chain:?}"
    );
}

/// Constructor (`new`) overloads are wrapped identically. tsc's
/// `signatureToString(candidate)` in this context passes no `SignatureKind`, so
/// the construct signature renders without a leading `new` — `(x: number): C`.
#[test]
fn constructor_overloads_wrap_without_new_prefix() {
    let chain = overload_elaboration(
        r#"
declare class Widget {
  constructor(size: number);
  constructor(label: string);
}
new Widget(true);
"#,
    );

    let headers: Vec<&String> = chain
        .iter()
        .filter(|(code, _, _)| *code == 2772)
        .map(|(_, _, m)| m)
        .collect();
    assert_eq!(
        headers,
        vec![
            "Overload 1 of 2, '(size: number): Widget', gave the following error.",
            "Overload 2 of 2, '(label: string): Widget', gave the following error.",
        ],
        "construct signatures render in colon form without `new`"
    );
}

/// Generic overloads render their type parameters in the wrapped signature,
/// matching tsc's `signatureToString` of the declared candidate.
#[test]
fn generic_overloads_render_type_parameters_in_signature() {
    let chain = overload_elaboration(
        r#"
interface Container<T> { readonly item: T; }
declare function unwrap<T>(source: Container<T>): T;
declare function unwrap<T extends string>(source: T[]): T;
unwrap(99);
"#,
    );

    let headers: Vec<&String> = chain
        .iter()
        .filter(|(code, _, _)| *code == 2772)
        .map(|(_, _, m)| m)
        .collect();
    assert_eq!(
        headers,
        vec![
            "Overload 1 of 2, '<T>(source: Container<T>): T', gave the following error.",
            "Overload 2 of 2, '<T extends string>(source: T[]): T', gave the following error.",
        ],
        "declared type parameters must appear in the wrapped signature"
    );
}

/// A single overload never produces a TS2769: the mismatch collapses to a plain
/// TS2345, with no TS2772 wrapper.
#[test]
fn single_overload_reports_plain_ts2345_without_wrapper() {
    let diags = check_source_diagnostics(
        r#"
declare function only(param: number): number;
only(true);
"#,
    );
    assert!(
        diags.iter().any(|d| d.code == 2345),
        "expected a plain TS2345: {diags:?}"
    );
    assert!(
        !diags.iter().any(|d| d.code == 2769),
        "a single overload must not report TS2769: {diags:?}"
    );
    assert!(
        diags
            .iter()
            .all(|d| d.related_information.iter().all(|r| r.code != 2772)),
        "no TS2772 wrapper for a single overload: {diags:?}"
    );
}

/// More than 3 candidates keep the flat rendering (the TS2770 last-overload
/// shape is a documented follow-up): TS2769 stays, but no TS2772 header is
/// emitted and every applicability error stays at depth 0.
#[test]
fn more_than_three_overloads_keep_flat_rendering() {
    let chain = overload_elaboration(
        r#"
declare function route(seg: number): number;
declare function route(seg: string): string;
declare function route(seg: boolean): boolean;
declare function route(seg: symbol): symbol;
route(null);
"#,
    );

    assert!(
        chain.iter().all(|(code, _, _)| *code != 2772),
        "no per-overload TS2772 wrapper beyond 3 candidates: {chain:?}"
    );
    assert!(
        chain.iter().all(|(_, depth, _)| *depth == 0),
        "flat fallback keeps every applicability error at depth 0: {chain:?}"
    );
}

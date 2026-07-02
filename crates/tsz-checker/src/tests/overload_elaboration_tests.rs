//! Regression tests for tsc's per-overload TS2769 elaboration.
//!
//! When a call matches no overload and 2 or 3 candidate signatures reached
//! argument checking, tsc (`resolveCall`) wraps each candidate's applicability
//! error in an `Overload {n} of {total}, '{signature}', gave the following
//! error.` (TS2772) header, rendered in declaration order with the specific
//! error nested one level deeper. tsz previously flattened the per-overload
//! errors directly under the TS2769 header (dropping the wrapper and sorting
//! them out of declaration order).
//!
//! Owner: `error_no_overload_matches_at`
//! (`crates/tsz-checker/src/error_reporter/call_errors/error_emission.rs`),
//! fed by the parallel `overload_candidates` metadata recorded in
//! `resolve_overloaded_call_with_signatures`
//! (`crates/tsz-checker/src/checkers/call_checker/overload_resolution/resolve_signatures.rs`).

use crate::test_utils::check_source_diagnostics;

/// The flat list of every related-information message on the single TS2769.
fn ts2769_related(source: &str) -> Vec<String> {
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
        .map(|r| r.message_text.clone())
        .collect()
}

/// The ordered `(depth, code, message)` related chain of the single TS2769.
fn ts2769_related_chain(source: &str) -> Vec<(u8, u32, String)> {
    let diags = check_source_diagnostics(source);
    let ts2769: Vec<_> = diags.iter().filter(|d| d.code == 2769).collect();
    assert_eq!(ts2769.len(), 1, "Expected exactly one TS2769");
    ts2769[0]
        .related_information
        .iter()
        .map(|r| (r.depth, r.code, r.message_text.clone()))
        .collect()
}

/// Two failing overloads: each argument error is wrapped in its own TS2772
/// header, in declaration order.
#[test]
fn two_overloads_wrap_each_failure_in_order() {
    let related = ts2769_related(
        r#"
declare function f(x: number): number;
declare function f(x: string): string;
f(true);
"#,
    );

    assert_eq!(
        related,
        vec![
            "Overload 1 of 2, '(x: number): number', gave the following error.".to_string(),
            "Argument of type 'boolean' is not assignable to parameter of type 'number'."
                .to_string(),
            "Overload 2 of 2, '(x: string): string', gave the following error.".to_string(),
            "Argument of type 'boolean' is not assignable to parameter of type 'string'."
                .to_string(),
        ],
        "expected per-overload TS2772 wrappers in declaration order, got: {related:#?}"
    );
}

/// The wrapper (TS2772, depth 0) precedes its applicability error (depth 1) so
/// the CLI renders the error indented one level under its header.
#[test]
fn wrapper_header_is_depth_zero_and_error_is_depth_one() {
    let chain = ts2769_related_chain(
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
        "expected outer TS2772 (depth 0) then nested TS2345 (depth 1) per overload"
    );
}

/// Three failing overloads: all three are wrapped, and the `of {total}` count
/// and declaration order are preserved (tsc's flat-branch order, not the sorted
/// order the flat fallback would produce).
#[test]
fn three_overloads_wrap_all_in_declaration_order() {
    let related = ts2769_related(
        r#"
declare function f(x: number): 1;
declare function f(x: string): 2;
declare function f(x: boolean): 3;
f({});
"#,
    );

    assert_eq!(
        related,
        vec![
            "Overload 1 of 3, '(x: number): 1', gave the following error.".to_string(),
            "Argument of type '{}' is not assignable to parameter of type 'number'.".to_string(),
            "Overload 2 of 3, '(x: string): 2', gave the following error.".to_string(),
            "Argument of type '{}' is not assignable to parameter of type 'string'.".to_string(),
            "Overload 3 of 3, '(x: boolean): 3', gave the following error.".to_string(),
            "Argument of type '{}' is not assignable to parameter of type 'boolean'.".to_string(),
        ],
        "expected three per-overload wrappers in declaration order, got: {related:#?}"
    );
}

/// The elaboration keys on structure, not identifier spelling: renaming the
/// function and its parameters leaves the wrapper text unchanged apart from the
/// rendered signature.
#[test]
fn wrapper_is_independent_of_binder_names() {
    let related = ts2769_related(
        r#"
declare function combine(value: number): number;
declare function combine(value: string): string;
combine(true);
"#,
    );

    assert_eq!(
        related,
        vec![
            "Overload 1 of 2, '(value: number): number', gave the following error.".to_string(),
            "Argument of type 'boolean' is not assignable to parameter of type 'number'."
                .to_string(),
            "Overload 2 of 2, '(value: string): string', gave the following error.".to_string(),
            "Argument of type 'boolean' is not assignable to parameter of type 'string'."
                .to_string(),
        ],
        "wrapper must follow the renamed signature verbatim, got: {related:#?}"
    );
}

/// The wrapped signature is rendered via the shared signature formatter, so a
/// type-guard overload keeps its `x is T` predicate in the header (rather than
/// collapsing to the bare return type).
#[test]
fn wrapper_preserves_type_predicate_signature() {
    let related = ts2769_related(
        r#"
declare function f(x: string): x is "a";
declare function f(x: number): x is 1;
f(true);
"#,
    );

    assert_eq!(
        related,
        vec![
            "Overload 1 of 2, '(x: string): x is \"a\"', gave the following error.".to_string(),
            "Argument of type 'boolean' is not assignable to parameter of type 'string'."
                .to_string(),
            "Overload 2 of 2, '(x: number): x is 1', gave the following error.".to_string(),
            "Argument of type 'boolean' is not assignable to parameter of type 'number'."
                .to_string(),
        ],
        "type-guard overload headers must keep the `x is T` predicate, got: {related:#?}"
    );
}

/// `new` with two failing constructor overloads is wrapped identically to a
/// call.
#[test]
fn constructor_overloads_are_wrapped() {
    let related = ts2769_related(
        r#"
declare class C {
    constructor(x: number);
    constructor(x: string);
}
new C(true);
"#,
    );

    assert!(
        related
            .iter()
            .any(|m| m.starts_with("Overload 1 of 2,") && m.ends_with("gave the following error.")),
        "expected a wrapped first constructor overload, got: {related:#?}"
    );
    assert!(
        related
            .iter()
            .any(|m| m
                == "Argument of type 'boolean' is not assignable to parameter of type 'number'."),
        "expected the nested applicability error, got: {related:#?}"
    );
}

/// More than three failing overloads fall back to the flat related list (tsc
/// uses a different top-level shape there, which tsz does not yet reproduce);
/// no TS2772 wrapper is emitted, keeping the top-level TS2769 unchanged.
#[test]
fn more_than_three_overloads_stay_flat() {
    let related = ts2769_related(
        r#"
declare function f(x: number): 1;
declare function f(x: string): 2;
declare function f(x: boolean): 3;
declare function f(x: bigint): 4;
f({});
"#,
    );

    assert!(
        !related
            .iter()
            .any(|m| m.contains("gave the following error.")),
        "with >3 overloads the flat fallback must not emit TS2772 wrappers, got: {related:#?}"
    );
    assert!(
        related
            .iter()
            .any(|m| m == "Argument of type '{}' is not assignable to parameter of type 'number'."),
        "the flat argument errors must still be present, got: {related:#?}"
    );
}

//! Regression tests for the relation failure-reason sub-chain nested under the
//! last candidate's argument line in a TS2769 ("No overload matches this
//! call") elaboration.
//!
//! tsc 7.0.2 wraps every multi-candidate failure in a single depth-0
//! `The last overload gave the following error.` (TS2770) header with the LAST
//! argument-error candidate's `Argument of type … is not assignable …`
//! (TS2345) line nested one level beneath it. When that argument mismatch has
//! an elaborable relation failure reason (e.g. a contravariant
//! callback-parameter incompatibility), tsc nests the full reason chain under
//! the argument line — the same `checkTypeRelatedToAndOptionallyElaborate`
//! chain the single-signature TS2345 path renders
//! (`getSignatureApplicabilityError` reuses the single-signature relation
//! elaboration).
//!
//! The chain routes through the shared `related_from_failure_reason` gateway
//! (owner: `crates/tsz-checker/src/error_reporter/call_errors/error_emission.rs`,
//! `error_no_overload_matches_at`), anchored two nesting levels beneath the
//! TS2770 header (header 0, argument line 1, chain 2+). Every expectation is
//! differential-verified against the pinned tsc 7.0.2 binary.

use crate::test_utils::check_source_diagnostics;

/// One flattened related-information entry: `(depth, code, message)`.
type RelatedLine = (u8, u32, String);

/// Collect the flattened related-information chain of the single TS2769
/// diagnostic, preserving the emitted order and per-line nesting depth.
fn overload_related_chain(source: &str) -> Vec<RelatedLine> {
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

/// Assert the last-overload wrapper shape: exactly one depth-0 `TS2770`
/// header, immediately followed by its depth-1 `TS2345` argument line.
fn assert_overload_wrapper_shape(chain: &[RelatedLine]) {
    let header_positions: Vec<usize> = chain
        .iter()
        .enumerate()
        .filter(|(_, (d, _, _))| *d == 0)
        .map(|(i, _)| i)
        .collect();
    assert_eq!(
        header_positions,
        vec![0],
        "exactly one depth-0 TS2770 header, got: {chain:?}"
    );
    let (_, code, msg) = &chain[0];
    assert!(
        *code == 2770 && msg == "The last overload gave the following error.",
        "the depth-0 entry must be the TS2770 last-overload header, got: {chain:?}"
    );
    assert!(
        chain.get(1).is_some_and(|(depth, code, msg)| *depth == 1
            && *code == 2345
            && msg.starts_with("Argument of type")),
        "the header must be directly followed by its depth-1 TS2345 argument line, got: {chain:?}"
    );
}

/// Assert the contravariant callback-parameter sub-chain nests beneath a
/// candidate's argument line: the `param_frame` line (`Types of parameters '_'
/// and '_' are incompatible.`) at depth >= 2 (under the depth-1 argument line)
/// and the `string`/`boolean` contravariant leaf one level deeper (depth >= 3).
/// Shared by the callback witness, the renamed-binder, and the
/// constructor-overload cases, which differ only in the frame's parameter
/// identifiers.
fn assert_contravariant_nesting(chain: &[RelatedLine], param_frame: &str) {
    assert!(
        chain
            .iter()
            .any(|(depth, _, msg)| *depth >= 2 && msg == param_frame),
        "expected the nested parameter-incompatibility frame `{param_frame}` under the last candidate's argument line, got: {chain:?}"
    );
    // The shown candidate is the LAST overload (the number-typed callback),
    // so the contravariant leaf names `number`.
    assert!(
        chain.iter().any(|(depth, _, msg)| *depth >= 3
            && msg == "Type 'number' is not assignable to type 'boolean'."),
        "expected the contravariant leaf under the parameter frame, got: {chain:?}"
    );
}

/// The witness from the bug report: an overloaded callback called with a
/// contravariantly-incompatible parameter. Each candidate's TS2772 header
/// carries its TS2345 argument line at depth 1 plus the nested `Types of
/// parameters 'a' and 'x' are incompatible.` frame and its contravariant leaf,
/// exactly as the single-signature TS2345 path nests them — not a bare
/// argument line.
#[test]
fn overloaded_callback_parameter_mismatch_nests_reason_chain() {
    let chain = overload_related_chain(
        r#"
declare function each(cb: (x: string) => void): void;
declare function each(cb: (x: number, i: number) => void): void;
each((a: boolean) => {});
"#,
    );

    assert_overload_wrapper_shape(&chain);

    // The reason sub-chain must appear nested beneath each candidate's
    // argument line: the parameter frame at depth >= 2 and the contravariant
    // leaf one level deeper.
    assert_contravariant_nesting(&chain, "Types of parameters 'a' and 'x' are incompatible.");
}

/// Anti-hardcoding: the behavior is structural, not keyed to specific binder or
/// parameter names. Renaming the callee and the parameters produces the same
/// nested frame (with the renamed parameter identifiers).
#[test]
fn nested_reason_chain_is_independent_of_binder_names() {
    let chain = overload_related_chain(
        r#"
declare function apply(handler: (value: string) => void): void;
declare function apply(handler: (value: number, index: number) => void): void;
apply((flag: boolean) => {});
"#,
    );

    assert_contravariant_nesting(
        &chain,
        "Types of parameters 'flag' and 'value' are incompatible.",
    );
}

/// `new` overloads take the same construction path: a constructor-overload
/// callback mismatch must nest its reason chain identically to the `call` form.
#[test]
fn constructor_overload_callback_mismatch_nests_reason_chain() {
    let chain = overload_related_chain(
        r#"
declare class Box {
    constructor(cb: (x: string) => void);
    constructor(cb: (x: number, i: number) => void);
}
new Box((a: boolean) => {});
"#,
    );

    assert_contravariant_nesting(&chain, "Types of parameters 'a' and 'x' are incompatible.");
}

/// A non-fresh object argument that is missing a required property nests the
/// `Property 'id' is missing …` reason (TS2741) beneath the candidate's
/// argument line, confirming the gateway drills reason variants other than
/// parameter mismatches. A declared variable source (not a fresh object
/// literal) is used so the failure is a genuine `MissingProperty` relation
/// reason rather than the excess-property (TS2353) path, which is elaborated
/// elsewhere.
#[test]
fn overloaded_object_argument_missing_property_nests_reason_chain() {
    let chain = overload_related_chain(
        r#"
interface WithId { id: number; }
interface WithBoth { id: number; tag: string; }
declare function save(x: WithId): void;
declare function save(x: WithBoth): void;
declare const partial: { tag: string };
save(partial);
"#,
    );

    // The missing-property reason HEAD-PROMOTES: it replaces the argument
    // line directly under the TS2770 header (tsc renders no
    // `Argument of type ...` wrapper for this shape).
    assert_eq!(
        chain
            .iter()
            .map(|(depth, _, msg)| (*depth, msg.as_str()))
            .collect::<Vec<_>>(),
        vec![
            (0, "The last overload gave the following error."),
            (
                1,
                "Property 'id' is missing in type '{ tag: string; }' but required in type 'WithBoth'."
            ),
        ],
        "expected the promoted missing-property reason directly under the header, got: {chain:?}"
    );
}

/// Control: non-elaborable primitive-vs-primitive overload mismatches keep just
/// the TS2772 headers and their depth-1 argument lines — the fix must not
/// synthesize an empty or spurious nested chain when the relation has no
/// reason to elaborate.
#[test]
fn primitive_overload_mismatch_keeps_flat_headers() {
    let chain = overload_related_chain(
        r#"
declare function f(x: number): void;
declare function f(x: string): void;
f(true);
"#,
    );

    assert_overload_wrapper_shape(&chain);
    assert!(
        chain.iter().all(|(depth, _, _)| *depth <= 1),
        "primitive overload mismatches must not nest a reason chain under the argument line, got: {chain:?}"
    );
    assert!(
        chain.iter().any(|(depth, _, msg)| *depth == 1
            && msg
                == "Argument of type 'boolean' is not assignable to parameter of type 'string'."),
        "expected the widened boolean argument line against the LAST (string) overload, got: {chain:?}"
    );
}

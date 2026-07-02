//! Regression tests for the relation failure-reason sub-chain nested under each
//! candidate's argument line in a TS2769 ("No overload matches this call")
//! elaboration.
//!
//! When a call matches no overload and a candidate's argument mismatch has an
//! elaborable relation failure reason (e.g. a contravariant callback-parameter
//! incompatibility), tsc nests the full reason chain under that candidate — the
//! same `checkTypeRelatedToAndOptionallyElaborate` chain the single-signature
//! TS2345 path renders. Previously tsz built each candidate failure as a bare
//! two-argument diagnostic and dropped the reason chain, so an overloaded
//! callback mismatch printed only the flat `Argument of type … is not
//! assignable …` line while the identical single-signature call printed the
//! nested `Types of parameters 'a' and 'x' are incompatible.` sub-chain.
//!
//! The fix routes each candidate's argument failure through the shared
//! `related_from_failure_reason` gateway (owner:
//! `crates/tsz-checker/src/error_reporter/call_errors/error_emission.rs`,
//! `error_no_overload_matches_at`) and re-anchors the chain one nesting level
//! beneath the candidate's header line.

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

/// Assert the contravariant callback-parameter sub-chain nests beneath a
/// candidate header: the `param_frame` line (`Types of parameters '_' and '_'
/// are incompatible.`) at depth >= 1, and the `string`/`boolean` contravariant
/// leaf one level deeper (depth >= 2). Shared by the callback witness, the
/// renamed-binder, and the constructor-overload cases, which differ only in the
/// frame's parameter identifiers.
fn assert_contravariant_nesting(chain: &[RelatedLine], param_frame: &str) {
    assert!(
        chain
            .iter()
            .any(|(depth, _, msg)| *depth >= 1 && msg == param_frame),
        "expected the nested parameter-incompatibility frame `{param_frame}` under a candidate, got: {chain:?}"
    );
    assert!(
        chain.iter().any(|(depth, _, msg)| *depth >= 2
            && msg == "Type 'string' is not assignable to type 'boolean'."),
        "expected the contravariant leaf under the parameter frame, got: {chain:?}"
    );
}

/// The witness from the bug report: an overloaded callback called with a
/// contravariantly-incompatible parameter. Each candidate's argument line must
/// carry the nested `Types of parameters 'a' and 'x' are incompatible.` frame
/// plus its contravariant leaf, exactly as the single-signature TS2345 path
/// does — not a bare argument line.
#[test]
fn overloaded_callback_parameter_mismatch_nests_reason_chain() {
    let chain = overload_related_chain(
        r#"
declare function each(cb: (x: string) => void): void;
declare function each(cb: (x: number, i: number) => void): void;
each((a: boolean) => {});
"#,
    );

    // Every candidate header line is a depth-0 argument-not-assignable entry.
    let headers: Vec<&RelatedLine> = chain.iter().filter(|(d, _, _)| *d == 0).collect();
    assert!(
        headers
            .iter()
            .all(|(_, code, msg)| *code == 2345 && msg.starts_with("Argument of type")),
        "each candidate header must be a depth-0 TS2345 argument line, got: {chain:?}"
    );
    assert!(
        headers.len() >= 2,
        "both overloads must contribute a header line, got: {chain:?}"
    );

    // The reason sub-chain must appear nested beneath the headers: the parameter
    // frame at depth >= 1 and the contravariant leaf one level deeper.
    assert_contravariant_nesting(&chain, "Types of parameters 'a' and 'x' are incompatible.");

    // Grouping invariant: a candidate header is immediately followed by its own
    // deeper chain (no interleaving of the two candidates' chains).
    let first_header = chain
        .iter()
        .position(|(d, _, _)| *d == 0)
        .expect("a header must exist");
    assert!(
        chain
            .get(first_header + 1)
            .is_some_and(|(depth, _, _)| *depth >= 1),
        "the first header must be directly followed by its nested chain, got: {chain:?}"
    );
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
/// `Property 'id' is missing …` reason (TS2741) beneath the candidate header,
/// confirming the gateway drills reason variants other than parameter
/// mismatches. A declared variable source (not a fresh object literal) is used
/// so the failure is a genuine `MissingProperty` relation reason rather than the
/// excess-property (TS2353) path, which is elaborated elsewhere.
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

    // At least one candidate nests a missing-property reason (TS2741)
    // beneath the header instead of leaving a bare argument line.
    assert!(
        chain
            .iter()
            .any(|(depth, _, msg)| *depth >= 1 && msg.contains("is missing in type")),
        "expected a nested missing-property reason under a candidate, got: {chain:?}"
    );
}

/// Control: non-elaborable primitive-vs-primitive overload mismatches keep just
/// the flat argument headers — the fix must not synthesize an empty or spurious
/// nested chain when the relation has no reason to elaborate.
#[test]
fn primitive_overload_mismatch_keeps_flat_headers() {
    let chain = overload_related_chain(
        r#"
declare function f(x: number): void;
declare function f(x: string): void;
f(true);
"#,
    );

    assert!(
        chain.iter().all(|(depth, _, _)| *depth == 0),
        "primitive overload mismatches must stay flat (no nested chain), got: {chain:?}"
    );
    assert!(
        chain.iter().any(|(_, _, msg)| msg
            == "Argument of type 'boolean' is not assignable to parameter of type 'number'."),
        "expected the widened boolean header against the number overload, got: {chain:?}"
    );
}

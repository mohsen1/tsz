//! Regression tests: a source **property** measured against a target **index
//! signature** must (a) actually fail the assignability relation even when the
//! target also declares a same-named property, and (b) elaborate as `TS2530`
//! "Property '{name}' is incompatible with index signature." — not `TS2634`
//! "'{kind}' index signatures are incompatible.", which `tsc` reserves for a
//! source *index signature* vs the target index.
//!
//! Structural rule (pinned against `typescript@7.0.2`, `--noEmit --strict`):
//! when `source <: target` and `target` carries an index signature, `tsc`'s
//! `membersRelatedToIndexInfo` checks **every** source property against the
//! target index info, including a property that also matches a named target
//! member. A target whose own declared property conflicts with its index
//! signature (the `TS2411` shape, e.g. `{ [k: string]: number; a: boolean }`)
//! therefore still rejects an assignment whose `a` violates the index. tsz
//! previously skipped named-matched properties in
//! `check_properties_against_index_signatures`, silently accepting the
//! assignment (a false negative); and its renderer labeled every
//! property-vs-index failure `TS2634`.
//!
//! The head code stays `TS2322` (assignment) / `TS2345` (argument); only the
//! elaboration line distinguishes the property case (`TS2530`) from the
//! index-signature case (`TS2634`). Binder names are varied so no fixture-name
//! string can drive the routing.

use crate::diagnostics::Diagnostic;
use crate::test_utils::{check_source_diagnostics, diagnostic_count};
use tsz_common::diagnostics::diagnostic_codes;

const TS2322: u32 = diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE;
const TS2345: u32 = diagnostic_codes::ARGUMENT_OF_TYPE_IS_NOT_ASSIGNABLE_TO_PARAMETER_OF_TYPE;
const TS2411: u32 = diagnostic_codes::PROPERTY_OF_TYPE_IS_NOT_ASSIGNABLE_TO_INDEX_TYPE;
const TS2530: u32 = diagnostic_codes::PROPERTY_IS_INCOMPATIBLE_WITH_INDEX_SIGNATURE;
const TS2634: u32 = diagnostic_codes::INDEX_SIGNATURES_ARE_INCOMPATIBLE;

fn only(diags: &[Diagnostic], code: u32) -> Diagnostic {
    let matching: Vec<_> = diags.iter().filter(|d| d.code == code).collect();
    assert_eq!(
        matching.len(),
        1,
        "expected exactly one TS{code}; got {:?}",
        diags
            .iter()
            .map(|d| (d.code, d.message_text.clone()))
            .collect::<Vec<_>>()
    );
    matching[0].clone()
}

/// Every rendered line of `diag` (head + related chain) as one haystack.
fn chain_text(diag: &Diagnostic) -> String {
    let mut s = diag.message_text.clone();
    for r in &diag.related_information {
        s.push('\n');
        s.push_str(&r.message_text);
    }
    s
}

/// A source variable whose property matches a named target property that itself
/// conflicts with the target's own string index signature: the assignment must
/// fail (`TS2322`) and elaborate with `TS2530`, not `TS2634`.
#[test]
fn named_property_matching_conflicting_string_index_fails_with_ts2530() {
    let source = r#"
type Box = { [k: string]: number; flag: boolean };
declare const src: { flag: boolean };
const b: Box = src;
"#;
    let diags = check_source_diagnostics(source);
    // TS2411 on the type declaration itself is expected and unchanged.
    assert_eq!(diagnostic_count(&diags, TS2411), 1, "diags: {diags:?}");
    let diag = only(&diags, TS2322);
    let text = chain_text(&diag);
    assert!(
        text.contains("Property 'flag' is incompatible with index signature."),
        "expected TS2530 property elaboration, got: {text}"
    );
    assert!(
        diag.related_information.iter().any(|r| r.code == TS2530),
        "expected a TS2530 chain link, got: {:?}",
        diag.related_information
            .iter()
            .map(|r| (r.code, r.message_text.clone()))
            .collect::<Vec<_>>()
    );
    assert_eq!(
        diagnostic_count(&diags, TS2634),
        0,
        "must not label a property-vs-index failure as TS2634: {diags:?}"
    );
}

/// A source variable carrying an *excess* property (no matching named target
/// member) that violates the target string index: pre-existing path, also
/// `TS2530` in `tsc`. Confirms the renderer routes on property-vs-index for the
/// non-conflicting-target case too.
#[test]
fn excess_property_against_string_index_uses_ts2530() {
    let source = r#"
type Dict = { [k: string]: number };
declare const src: { label: string };
const d: Dict = src;
"#;
    let diags = check_source_diagnostics(source);
    let diag = only(&diags, TS2322);
    let text = chain_text(&diag);
    assert!(
        text.contains("Property 'label' is incompatible with index signature."),
        "expected TS2530 property elaboration, got: {text}"
    );
    assert_eq!(diagnostic_count(&diags, TS2634), 0, "diags: {diags:?}");
}

/// The false-negative regression itself: a fresh object literal whose property
/// matches a named target member conflicting with the index must still be
/// rejected. Asserts the relation fires (`TS2322` + `TS2530` chain) without
/// pinning the head's fresh-literal display.
#[test]
fn fresh_object_literal_named_property_conflict_is_rejected() {
    let source = r#"
type Box = { [k: string]: number; flag: boolean };
const b: Box = { flag: true };
"#;
    let diags = check_source_diagnostics(source);
    let diag = only(&diags, TS2322);
    let text = chain_text(&diag);
    assert!(
        text.contains("Property 'flag' is incompatible with index signature.")
            && text.contains("is not assignable to type 'number'"),
        "expected the TS2530 chain down to the value mismatch, got: {text}"
    );
}

/// Number index signature variant: a numeric-keyed property conflicting with a
/// `[k: number]` index elaborates as `TS2530` too.
#[test]
fn named_property_matching_conflicting_number_index_fails_with_ts2530() {
    let source = r#"
type NumBox = { [k: number]: string; 0: boolean };
declare const src: { 0: boolean };
const n: NumBox = src;
"#;
    let diags = check_source_diagnostics(source);
    let diag = only(&diags, TS2322);
    let text = chain_text(&diag);
    assert!(
        text.contains("Property '0' is incompatible with index signature."),
        "expected TS2530 property elaboration, got: {text}"
    );
}

/// Argument position: the same property-vs-index failure under a `TS2345` head
/// carries the `TS2530` elaboration, matching the assignment path.
#[test]
fn named_property_conflict_in_argument_uses_ts2345_with_ts2530() {
    let source = r#"
type Box = { [k: string]: number; flag: boolean };
declare function take(x: Box): void;
declare const src: { flag: boolean };
take(src);
"#;
    let diags = check_source_diagnostics(source);
    let diag = only(&diags, TS2345);
    let text = chain_text(&diag);
    assert!(
        text.contains("Property 'flag' is incompatible with index signature."),
        "expected TS2530 elaboration under the TS2345 head, got: {text}"
    );
    assert_eq!(diagnostic_count(&diags, TS2634), 0, "diags: {diags:?}");
}

/// Interface heritage variant: the conflict arrives through `extends`, and the
/// assignment of a matching source must still be rejected with the `TS2530`
/// elaboration (the `TS2411` fires on the interface declaration as before).
#[test]
fn interface_heritage_index_conflict_rejects_with_ts2530() {
    let source = r#"
interface HasFlag { flag: boolean }
interface Indexed { [k: string]: number }
interface Merged extends HasFlag, Indexed {}
declare const src: { flag: boolean };
const m: Merged = src;
"#;
    let diags = check_source_diagnostics(source);
    assert_eq!(diagnostic_count(&diags, TS2411), 1, "diags: {diags:?}");
    let diag = only(&diags, TS2322);
    let text = chain_text(&diag);
    assert!(
        text.contains("Property 'flag' is incompatible with index signature."),
        "expected TS2530 elaboration, got: {text}"
    );
}

/// Compatibility guard: when the named property IS assignable to the index, no
/// spurious failure appears — the fix only adds errors for genuinely
/// conflicting (`TS2411`) targets.
#[test]
fn compatible_named_property_stays_clean() {
    let source = r#"
type Box = { [k: string]: number; count: number };
declare const src: { count: number };
const b: Box = src;
const lit: Box = { count: 7 };
"#;
    let diags = check_source_diagnostics(source);
    assert_eq!(
        diagnostic_count(&diags, TS2322),
        0,
        "compatible named property must not fail: {diags:?}"
    );
    assert_eq!(diagnostic_count(&diags, TS2411), 0, "diags: {diags:?}");
    assert_eq!(diagnostic_count(&diags, TS2530), 0, "diags: {diags:?}");
}

/// A genuine source **index signature** vs the target index still renders as
/// `TS2634` — the property routing must not swallow the index-vs-index case.
#[test]
fn source_index_signature_mismatch_stays_ts2634() {
    let source = r#"
type Src = { [k: string]: string };
type Dst = { [k: string]: number };
declare const src: Src;
const d: Dst = src;
"#;
    let diags = check_source_diagnostics(source);
    let diag = only(&diags, TS2322);
    let text = chain_text(&diag);
    assert!(
        text.contains("index signatures are incompatible"),
        "index-vs-index must stay TS2634, got: {text}"
    );
    assert_eq!(
        diagnostic_count(&diags, TS2530),
        0,
        "index-vs-index must not use TS2530: {diags:?}"
    );
}

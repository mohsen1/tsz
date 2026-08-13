//! Regression tests for a source *property* measured against a target *index
//! signature* (issue #17322). Two defects on the object-to-indexed-target
//! subtype path, both oracled against `typescript@7.0.2` (`--noEmit --strict`):
//!
//! 1. **False negative.** `tsc`'s `membersRelatedToIndexInfo` checks *every*
//!    source property against the target index — including a property that also
//!    matches a same-named target member. A target whose own declared property
//!    conflicts with its index signature (the `TS2411` shape) therefore still
//!    rejects an assignment whose property violates the index. tsz's decision
//!    function `check_properties_against_index_signatures` skipped named-matched
//!    source properties, silently accepting the assignment (only `TS2411` fired,
//!    never the `TS2322`/`TS2345`).
//!
//! 2. **Mislabel.** A source *property* vs the target index elaborates as
//!    `TS2530` ("Property '{0}' is incompatible with index signature."); only a
//!    source *index signature* vs the target index is `TS2634` ("'{0}' index
//!    signatures are incompatible."). tsz rendered every property-vs-index leaf
//!    as `TS2634`.
//!
//! Binder names are varied across cases so no identifier string is load-bearing.

use crate::test_utils::{check_source_codes, check_with_options, strict_checker_options};

/// Sorted head diagnostic codes under strict options.
fn codes(source: &str) -> Vec<u32> {
    let mut c = check_source_codes(source);
    c.sort_unstable();
    c
}

/// Full elaboration text (primary message plus every related-information line)
/// of the single error with `code`, under strict options.
fn elaboration(source: &str, code: u32) -> String {
    let diags = check_with_options(source, strict_checker_options());
    let matching: Vec<_> = diags.iter().filter(|d| d.code == code).collect();
    assert_eq!(
        matching.len(),
        1,
        "Expected exactly one TS{code}. Got: {:?}",
        diags
            .iter()
            .map(|d| (d.code, d.message_text.clone()))
            .collect::<Vec<_>>()
    );
    let mut lines = vec![matching[0].message_text.clone()];
    lines.extend(
        matching[0]
            .related_information
            .iter()
            .map(|info| info.message_text.clone()),
    );
    lines.join("\n")
}

// ---------------------------------------------------------------------------
// 1. False negative: a named-matched source property is still index-checked.
// ---------------------------------------------------------------------------

/// The core repro: a non-fresh variable source. `flag` matches the target's own
/// `flag` member *and* must satisfy the `number` index — it does not, so the
/// assignment is rejected (`TS2322`) in addition to the target's own `TS2411`.
#[test]
fn named_matched_property_variable_source_is_index_checked() {
    let source = r#"
type Box = { [k: string]: number; flag: boolean };
declare const s: { flag: boolean };
const c: Box = s;
"#;
    // TS2411 on the target's own conflicting member; TS2322 on the assignment.
    assert_eq!(codes(source), vec![2322, 2411]);
}

/// Same defect through a call argument: the assignment surface is `TS2345`, and
/// its elaboration is also the `TS2530` property message — the argument path has
/// its own reason renderer (`related_from_failure_reason`).
#[test]
fn named_matched_property_argument_is_index_checked() {
    let source = r#"
type Crate = { [entry: string]: number; marker: boolean };
declare function take(x: Crate): void;
declare const src: { marker: boolean };
take(src);
"#;
    assert_eq!(codes(source), vec![2345, 2411]);
    let text = elaboration(source, 2345);
    assert!(
        text.contains("Property 'marker' is incompatible with index signature."),
        "the argument-path elaboration must also render TS2530. Got: {text:?}"
    );
    assert!(
        !text.contains("index signature is incompatible:"),
        "the argument path must not keep the index-signature combined form for a property. Got: {text:?}"
    );
}

/// The named member is contributed through heritage rather than a literal
/// member list — the skip was keyed on the merged member set, so an interface
/// merging a flag-bearing base with an indexed base has the same shape.
#[test]
fn named_matched_property_through_extends_is_index_checked() {
    let source = r#"
interface HasFlag { flag: boolean }
interface Indexed { [k: string]: number }
interface Merged extends HasFlag, Indexed {}
declare const s: { flag: boolean };
const m: Merged = s;
"#;
    let c = codes(source);
    assert!(
        c.contains(&2322),
        "the named-matched heritage property must still be rejected by the index; got {c:?}"
    );
}

/// An intersection target `{ flag } & { [k: string]: number }` requires the
/// source property to satisfy the indexed member too.
#[test]
fn intersection_target_index_checks_named_property() {
    let source = r#"
type Inter = { flag: boolean } & { [k: string]: number };
declare const s: { flag: boolean };
const x: Inter = s;
"#;
    assert!(
        codes(source).contains(&2322),
        "intersection target must index-check the named source property"
    );
}

/// Positive guard: a well-formed target whose named member *is* assignable to
/// its index must stay clean — removing the skip only adds a failure on the
/// ill-formed `TS2411` shape.
#[test]
fn well_formed_named_member_stays_clean() {
    let source = r#"
type Good = { [k: string]: number; count: number };
declare const s: { count: number };
const g: Good = s;
"#;
    assert_eq!(
        codes(source),
        Vec::<u32>::new(),
        "a named member assignable to the index must not add a spurious failure"
    );
}

// ---------------------------------------------------------------------------
// 2. Mislabel: property-vs-index is TS2530, index-vs-index stays TS2634.
// ---------------------------------------------------------------------------

/// An excess (non-matching) source property vs the target index elaborates the
/// `TS2530` property message, not the `TS2634` index-signature message.
#[test]
fn excess_property_vs_index_renders_ts2530_not_ts2634() {
    let source = r#"
type Dict = { [k: string]: number };
declare const s: { label: string };
const d: Dict = s;
"#;
    let text = elaboration(source, 2322);
    assert!(
        text.contains("Property 'label' is incompatible with index signature."),
        "property-vs-index must elaborate TS2530. Got: {text:?}"
    );
    assert!(
        !text.contains("index signatures are incompatible"),
        "property-vs-index must not use the TS2634 index-signature message. Got: {text:?}"
    );
}

/// Negative guard: a source *index signature* vs the target index stays
/// `TS2634` — the fix must not reroute index-vs-index to `TS2530`.
#[test]
fn source_index_signature_vs_index_stays_ts2634() {
    let source = r#"
type TgtIdx = { [k: string]: number };
declare const s: { [k: string]: string };
const t: TgtIdx = s;
"#;
    let text = elaboration(source, 2322);
    assert!(
        text.contains("'string' index signatures are incompatible."),
        "index-vs-index must stay TS2634. Got: {text:?}"
    );
    assert!(
        !text.contains("is incompatible with index signature"),
        "index-vs-index must not render the TS2530 property message. Got: {text:?}"
    );
}

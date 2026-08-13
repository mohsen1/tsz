//! Regression tests for #17322: a source property measured against a target
//! index signature.
//!
//! Structural rule: `tsc`'s `membersRelatedToIndexInfo` relates *every*
//! applicable source property to the target index signature — including a
//! property whose name the target also declares (the ill-formed `TS2411`
//! shape). A source property that violates the index is therefore a real
//! assignment failure, elaborated with `TS2530` "Property '{name}' is
//! incompatible with index signature."; only a source *index signature* vs the
//! target index is the `TS2634` "'{kind}' index signatures are incompatible."
//! line.
//!
//! Two defects are covered:
//!  1. false-negative — the decision path skipped a named-matched source
//!     property, silently accepting an assignment `tsc` rejects.
//!  2. mislabel — a source *property* vs the target index rendered as `TS2634`
//!     where `tsc` emits `TS2530`.
//!
//! The rule is structural, so the matrix varies identifier spellings, alias vs
//! `extends`-merged target shapes, the assignment vs argument surface, and keeps
//! the genuine index-vs-index case on `TS2634` plus a compatible negative case.

use crate::test_utils::{check_with_options, strict_checker_options};

/// The `(code, message)` pairs of the head diagnostic plus its related chain,
/// for the single diagnostic with `code`.
fn chain(source: &str, code: u32) -> Vec<(u32, String)> {
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
    let mut out = vec![(matching[0].code, matching[0].message_text.clone())];
    out.extend(
        matching[0]
            .related_information
            .iter()
            .map(|info| (info.code, info.message_text.clone())),
    );
    out
}

fn has_line(chain: &[(u32, String)], code: u32, needle: &str) -> bool {
    chain.iter().any(|(c, m)| *c == code && m.contains(needle))
}

/// A named-matched source property that violates the target index signature is
/// no longer silently accepted (defect 1) and elaborates as `TS2530` (defect 2).
#[test]
fn named_matched_property_violating_index_reports_ts2530() {
    let src = r#"
type Box = { [k: string]: number; flag: boolean };
declare const s: { flag: boolean };
const c: Box = s;
"#;
    // The ill-formed target still reports its own TS2411.
    let diags = check_with_options(src, strict_checker_options());
    assert!(
        diags.iter().any(|d| d.code == 2411),
        "Expected TS2411 on the Box declaration. Got: {:?}",
        diags.iter().map(|d| d.code).collect::<Vec<_>>()
    );

    let ch = chain(src, 2322);
    assert!(
        has_line(
            &ch,
            2530,
            "Property 'flag' is incompatible with index signature."
        ),
        "Expected TS2530 property-vs-index line. Got: {ch:?}"
    );
    assert!(
        has_line(
            &ch,
            2322,
            "Type 'boolean' is not assignable to type 'number'."
        ),
        "Expected the value-type leaf. Got: {ch:?}"
    );
    // The mislabelled TS2634 index-vs-index line must not appear.
    assert!(
        !ch.iter().any(|(c, _)| *c == 2634),
        "A source property must not render TS2634. Got: {ch:?}"
    );
}

/// Same rule, different identifier spellings — the fix is structural, not
/// keyed on `flag`/`Box`.
#[test]
fn named_matched_property_report_is_name_independent() {
    let src = r#"
type Container = { [key: string]: number; marker: boolean };
declare const v: { marker: boolean };
const w: Container = v;
"#;
    let ch = chain(src, 2322);
    assert!(
        has_line(
            &ch,
            2530,
            "Property 'marker' is incompatible with index signature."
        ),
        "Expected TS2530 naming 'marker'. Got: {ch:?}"
    );
}

/// An *excess* (non-matching) source property that violates the index also
/// elaborates as `TS2530` (defect 2, on a well-formed target with no `TS2411`).
#[test]
fn excess_property_violating_index_reports_ts2530() {
    let src = r#"
type Dict = { [k: string]: number };
declare const s: { label: string };
const d: Dict = s;
"#;
    let diags = check_with_options(src, strict_checker_options());
    assert!(
        !diags.iter().any(|d| d.code == 2411),
        "Dict is well-formed; no TS2411 expected. Got: {:?}",
        diags.iter().map(|d| d.code).collect::<Vec<_>>()
    );
    let ch = chain(src, 2322);
    assert!(
        has_line(
            &ch,
            2530,
            "Property 'label' is incompatible with index signature."
        ),
        "Expected TS2530 naming 'label'. Got: {ch:?}"
    );
    assert!(
        has_line(
            &ch,
            2322,
            "Type 'string' is not assignable to type 'number'."
        ),
        "Expected the value-type leaf. Got: {ch:?}"
    );
}

/// A target that merges the named member and the index via `extends` behaves
/// identically to the alias form.
#[test]
fn extends_merged_named_member_reports_ts2530() {
    let src = r#"
interface HasFlag { flag: boolean }
interface Indexed { [k: string]: number }
interface Merged extends HasFlag, Indexed {}
declare const s: { flag: boolean };
const m: Merged = s;
"#;
    let ch = chain(src, 2322);
    assert!(
        has_line(
            &ch,
            2530,
            "Property 'flag' is incompatible with index signature."
        ),
        "Expected TS2530 on the extends-merged target. Got: {ch:?}"
    );
}

/// A source property flowing through a call argument surfaces the same `TS2530`
/// elaboration under `TS2345`.
#[test]
fn named_matched_property_argument_reports_ts2530_under_ts2345() {
    let src = r#"
type Box = { [k: string]: number; flag: boolean };
declare function take(b: Box): void;
declare const s: { flag: boolean };
take(s);
"#;
    let ch = chain(src, 2345);
    assert!(
        has_line(
            &ch,
            2530,
            "Property 'flag' is incompatible with index signature."
        ),
        "Expected TS2530 under the TS2345 argument surface. Got: {ch:?}"
    );
}

/// A genuine source *index signature* vs the target index keeps `TS2634` — the
/// fix must not relabel index-vs-index failures.
#[test]
fn source_index_vs_target_index_keeps_ts2634() {
    let src = r#"
type NumDict = { [k: string]: number };
declare const s: { [k: string]: string };
const e: NumDict = s;
"#;
    let ch = chain(src, 2322);
    assert!(
        has_line(&ch, 2634, "'string' index signatures are incompatible."),
        "Index-vs-index must stay TS2634. Got: {ch:?}"
    );
    assert!(
        !ch.iter().any(|(c, _)| *c == 2530),
        "Index-vs-index must not render TS2530. Got: {ch:?}"
    );
}

/// A named member compatible with the index (well-formed target, matching
/// source) stays clean — removing the skip adds no spurious diagnostic.
#[test]
fn compatible_named_member_stays_clean() {
    let src = r#"
type Box = { [k: string]: number; count: number };
declare const s: { count: number };
const g: Box = s;
"#;
    let diags = check_with_options(src, strict_checker_options());
    assert!(
        diags.is_empty(),
        "Expected no diagnostics for a compatible assignment. Got: {:?}",
        diags
            .iter()
            .map(|d| (d.code, d.message_text.clone()))
            .collect::<Vec<_>>()
    );
}

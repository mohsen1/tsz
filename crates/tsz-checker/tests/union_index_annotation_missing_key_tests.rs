//! TS2339 must fire for every invalid member of a union-key indexed access
//! (`I["a" | "z"]`) regardless of syntactic position — in particular in a
//! variable type-annotation position, not only in a `type` alias position.
//!
//! Structural rule: when an indexed-access index type resolves to a union of
//! string-literal keys and the object type is concrete (non-generic, non-
//! deferred), tsc reports one TS2339 per union member that is not a property
//! of the object (and is not accepted by an index signature), anchored at the
//! index node. This test pins that behavior across renamed keys, source types,
//! `typeof` receivers, and both alias and annotation positions.

use tsz_common::diagnostics::Diagnostic;

fn check(source: &str) -> Vec<Diagnostic> {
    tsz_checker::test_utils::check_source_diagnostics(source)
}

fn missing_keys(diags: &[Diagnostic]) -> Vec<String> {
    diags
        .iter()
        .filter(|d| d.code == 2339)
        .map(|d| d.message_text.clone())
        .collect()
}

/// Reported repro: a single missing member inside a union index in annotation
/// position must report, with the valid member suppressed.
#[test]
fn union_index_annotation_reports_single_missing_member() {
    let diags = check(
        r#"
interface I { a: number; b: string }
let x: I["z" | "a"];
"#,
    );
    let msgs = missing_keys(&diags);
    assert!(
        msgs.iter()
            .any(|m| m.contains("'z'") && m.contains("type 'I'")),
        "expected TS2339 for missing 'z', got: {msgs:?}"
    );
    assert!(
        !msgs.iter().any(|m| m.contains("'a'")),
        "valid member 'a' must not report, got: {msgs:?}"
    );
}

/// Both members missing must yield two TS2339, both anchored at the index node
/// (same position, distinct messages) — exercising message-aware dedup.
#[test]
fn union_index_annotation_reports_both_missing_members() {
    let diags = check(
        r#"
interface I { a: number; b: string }
let z: I["z" | "w"];
"#,
    );
    let msgs = missing_keys(&diags);
    assert!(
        msgs.iter().any(|m| m.contains("'z'")),
        "expected TS2339 for 'z', got: {msgs:?}"
    );
    assert!(
        msgs.iter().any(|m| m.contains("'w'")),
        "expected TS2339 for 'w', got: {msgs:?}"
    );
}

/// Renamed keys and a renamed source type prove the rule is structural, not
/// keyed to the spelling in the reported repro.
#[test]
fn union_index_annotation_is_structural_not_name_based() {
    let diags = check(
        r#"
interface J { x: number; y: string }
let a: J["x" | "nope"];
"#,
    );
    let msgs = missing_keys(&diags);
    assert!(
        msgs.iter()
            .any(|m| m.contains("'nope'") && m.contains("type 'J'")),
        "expected TS2339 for 'nope', got: {msgs:?}"
    );
    assert!(
        !msgs.iter().any(|m| m.contains("'x'")),
        "valid member 'x' must not report, got: {msgs:?}"
    );
}

/// `typeof obj` receiver in annotation position must report missing keys too.
#[test]
fn union_index_annotation_typeof_receiver_reports_missing() {
    let diags = check(
        r#"
const o = { p: 1, q: 2 };
let m: (typeof o)["p" | "nope"];
"#,
    );
    let msgs = missing_keys(&diags);
    assert!(
        msgs.iter().any(|m| m.contains("'nope'")),
        "expected TS2339 for 'nope' on typeof receiver, got: {msgs:?}"
    );
    assert!(
        !msgs.iter().any(|m| m.contains("'p'")),
        "valid member 'p' must not report, got: {msgs:?}"
    );
}

/// Negative control: the same union in alias position must still report (no
/// regression of the previously-working path).
#[test]
fn union_index_alias_position_still_reports() {
    let diags = check(
        r#"
interface I { a: number; b: string }
type T = I["a" | "z"];
"#,
    );
    let msgs = missing_keys(&diags);
    assert!(
        msgs.iter().any(|m| m.contains("'z'")),
        "expected TS2339 for 'z' in alias position, got: {msgs:?}"
    );
}

/// Negative control: an all-valid union must stay clean in annotation position.
#[test]
fn union_index_annotation_all_valid_is_clean() {
    let diags = check(
        r#"
interface I { a: number; b: string }
let d: I["a" | "b"];
"#,
    );
    assert!(
        missing_keys(&diags).is_empty(),
        "all-valid union must not report TS2339, got: {:?}",
        missing_keys(&diags)
    );
}

/// Negative control: a string index signature accepts any string key, so a
/// union of arbitrary string literals must not report.
#[test]
fn union_index_annotation_index_signature_is_clean() {
    let diags = check(
        r#"
interface Dict { [k: string]: number }
let c: Dict["anything" | "else"];
"#,
    );
    assert!(
        missing_keys(&diags).is_empty(),
        "string index signature must accept union keys, got: {:?}",
        missing_keys(&diags)
    );
}

/// Negative control: a generic/deferred index must not eagerly report; the
/// access site validates it once instantiated.
#[test]
fn generic_union_index_does_not_report() {
    let diags = check(
        r#"
function f<T, K extends keyof T>(t: T, k: K) {
  type R = T[K];
  return t[k];
}
"#,
    );
    assert!(
        missing_keys(&diags).is_empty(),
        "generic deferred index must not report TS2339, got: {:?}",
        missing_keys(&diags)
    );
}

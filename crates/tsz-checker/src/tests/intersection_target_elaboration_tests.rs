//! Regression tests for the TS2322/TS2345 target-**intersection** elaboration.
//!
//! Structural rule (verified against `tsc` 6.0.2): when a source is related to a
//! target intersection `C1 & C2 & …`, `tsc` (`typeRelatedToEachType`) relates it
//! to each constituent in written order and elaborates the **first** failing
//! constituent — the top-level `Type 'S' is not assignable to type 'C1 & C2 &
//! …'.` headline is followed by `Type 'S' is not assignable to type 'Ci'.` one
//! level deeper, then that constituent's own (path-compressed) drill.
//!
//! tsz previously evaluated the intersection target into a single merged object
//! before building the failure reason, so the chain skipped straight to the
//! merged property mismatch and dropped the constituent frame that explains
//! which member of the intersection requires the failing shape. The fix
//! reconstructs the constituent frame at the assignability gateway
//! (`analyze_assignability_failure` -> `IntersectionTargetMismatch`) from the
//! original (pre-evaluation) intersection, so it applies regardless of how the
//! intersection is spelled. See the diagnostics family tracker (#12179); this is
//! the dual of the intersection-*source* fix in #10962.

use crate::test_utils::check_source_diagnostics;

/// Collect a single diagnostic's full elaboration text (main message plus all
/// related-information lines, joined by newlines) for the given code.
fn elaboration(source: &str, code: u32) -> String {
    let diags = check_source_diagnostics(source);
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

/// A two-object intersection target with a failing first constituent emits the
/// constituent frame (`Type 'S' is not assignable to type '{ x: number; }'.`)
/// between the intersection headline and the property drill.
#[test]
fn anonymous_object_intersection_emits_first_constituent_frame() {
    let text = elaboration(
        r#"
declare let a: { x: number } & { y: string };
declare let b: { x: string; y: string };
a = b;
"#,
        2322,
    );
    assert!(
        text.contains("is not assignable to type '{ x: number; } & { y: string; }'"),
        "Expected the intersection headline. Got: {text:?}"
    );
    assert!(
        text.contains(
            "Type '{ x: string; y: string; }' is not assignable to type '{ x: number; }'."
        ),
        "Expected the failing constituent frame. Got: {text:?}"
    );
    assert!(
        text.contains("Types of property 'x' are incompatible."),
        "Expected the constituent's property drill. Got: {text:?}"
    );
}

/// The elaboration reports the **first** failing constituent in written order:
/// when the second constituent is the one that fails, its frame is emitted, not
/// the first's.
#[test]
fn reports_failing_constituent_in_written_order() {
    let text = elaboration(
        r#"
declare let a: { x: number } & { y: string };
declare let b: { x: number; y: number };
a = b;
"#,
        2322,
    );
    assert!(
        text.contains("is not assignable to type '{ y: string; }'."),
        "Expected the second constituent frame (the one that fails). Got: {text:?}"
    );
    assert!(
        text.contains("Types of property 'y' are incompatible."),
        "Expected the failing property to be 'y'. Got: {text:?}"
    );
}

/// Anti-hardcoding cover: the rule is structural, not tied to a spelling. An
/// interface intersection (`P & Q`) — which stays an intersection rather than
/// being merged at construction — produces the same constituent frame, and the
/// frame names the interface (`P`), not the merged shape.
#[test]
fn interface_intersection_names_the_constituent_interface() {
    let text = elaboration(
        r#"
interface Alpha { x: number }
interface Beta { y: string }
declare let a: Alpha & Beta;
declare let b: { x: string; y: string };
a = b;
"#,
        2322,
    );
    assert!(
        text.contains("is not assignable to type 'Alpha'."),
        "Expected the constituent frame to name the interface 'Alpha'. Got: {text:?}"
    );
    assert!(
        text.contains("Types of property 'x' are incompatible."),
        "Expected the property drill beneath the constituent frame. Got: {text:?}"
    );
}

/// A non-generic type alias for the intersection keeps its alias spelling in the
/// headline (`T`) while the constituent frame renders the structural
/// constituent — matching tsc's `aliasSymbol` policy.
#[test]
fn aliased_intersection_keeps_alias_in_headline_structural_constituent() {
    let text = elaboration(
        r#"
type Combined = { x: number } & { y: string };
declare let a: Combined;
declare let b: { x: string; y: string };
a = b;
"#,
        2322,
    );
    assert!(
        text.contains("is not assignable to type 'Combined'."),
        "Expected the alias name in the headline. Got: {text:?}"
    );
    assert!(
        text.contains("is not assignable to type '{ x: number; }'."),
        "Expected the structural constituent in the frame. Got: {text:?}"
    );
}

/// A failing constituent whose property mismatch is itself a single-property
/// chain keeps tsc's path-compressed drill (`The types of 'x.p' are
/// incompatible between these types.`) beneath the constituent frame, rather
/// than re-expanding it into nested `Types of property` lines.
#[test]
fn constituent_drill_preserves_dotted_path_compression() {
    let text = elaboration(
        r#"
declare let a: { x: { p: number } } & { y: string };
declare let b: { x: { p: string }; y: string };
a = b;
"#,
        2322,
    );
    assert!(
        text.contains("is not assignable to type '{ x: { p: number; }; }'."),
        "Expected the constituent frame. Got: {text:?}"
    );
    assert!(
        text.contains("The types of 'x.p' are incompatible between these types."),
        "Expected the path-compressed drill. Got: {text:?}"
    );
    assert!(
        !text.contains("Types of property 'x' are incompatible."),
        "Compressed chain must not re-expand the leading property. Got: {text:?}"
    );
}

/// The elaboration applies in argument position (TS2345) as well, since both
/// flow through the shared assignability gateway. A declared-variable argument
/// (not an inline object literal, which is checked property-wise) exercises the
/// whole-argument TS2345 path.
#[test]
fn argument_position_intersection_emits_constituent_frame() {
    let text = elaboration(
        r#"
declare function consume(p: { x: number } & { y: string }): void;
declare const arg: { x: string; y: string };
consume(arg);
"#,
        2345,
    );
    assert!(
        text.contains("is not assignable to type '{ x: number; }'."),
        "Expected the constituent frame in the argument elaboration. Got: {text:?}"
    );
    assert!(
        text.contains("Types of property 'x' are incompatible."),
        "Expected the property drill in the argument elaboration. Got: {text:?}"
    );
}

/// A branded primitive intersection (`string & { __brand }`) collapses to the
/// constituent frame alone: the source fails the object constituent, and there
/// is no deeper structural drill.
#[test]
fn branded_primitive_intersection_frame_stands_alone() {
    let text = elaboration(
        r#"
type Tagged = string & { __tag: 1 };
declare let a: Tagged;
declare let b: string;
a = b;
"#,
        2322,
    );
    assert!(
        text.contains("is not assignable to type 'Tagged'."),
        "Expected the alias headline. Got: {text:?}"
    );
    assert!(
        text.contains("is not assignable to type '{ __tag: 1; }'."),
        "Expected the object-constituent frame. Got: {text:?}"
    );
}

/// When the first failing constituent (in written order) fails because the
/// source is *missing* a property it requires, the elaboration folds to the
/// `Property 'p' is missing in type 'S' but required in type 'Ci'.` line (which
/// already names the constituent) with no extra `Type 'S' is not assignable to
/// type 'Ci'.` frame — even though the merged top-level reason is a different
/// (property-type) mismatch. This exercises the missing-property fold reached
/// via the per-constituent inner reason.
#[test]
fn first_failing_constituent_missing_property_folds() {
    let text = elaboration(
        r#"
declare let a: { y: string } & { x: number };
declare let b: { x: string };
a = b;
"#,
        2322,
    );
    assert!(
        text.contains(
            "Property 'y' is missing in type '{ x: string; }' but required in type '{ y: string; }'."
        ),
        "Expected the folded missing-property line naming the first constituent. Got: {text:?}"
    );
    assert!(
        !text.contains("is not assignable to type '{ y: string; }'."),
        "Missing-property fold must not also emit a constituent frame. Got: {text:?}"
    );
}

/// Control: a non-intersection target is unaffected — the chain stays the plain
/// `Type 'S' is not assignable to type 'T'.` + property drill with no spurious
/// constituent frame.
#[test]
fn non_intersection_target_is_unchanged() {
    let text = elaboration(
        r#"
declare let a: { x: number };
declare let b: { x: string };
a = b;
"#,
        2322,
    );
    assert!(
        text.contains("Types of property 'x' are incompatible."),
        "Expected the plain property drill. Got: {text:?}"
    );
    // Exactly one `is not assignable to type` line (the headline); no extra
    // constituent frame for a single-object target.
    assert_eq!(
        text.matches("is not assignable to type").count(),
        2,
        "Non-intersection target must not gain a constituent frame. Got: {text:?}"
    );
}

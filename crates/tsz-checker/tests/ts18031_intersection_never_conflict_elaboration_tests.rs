//! TS18031: when a disjoint-object-literal intersection collapses to `never`
//! at intern time (`crates/tsz-solver/src/intern/normalize.rs`'s
//! `intersection_has_disjoint_object_literals`), tsc attaches a related-info
//! line naming the conflicting property:
//! `The intersection '...' was reduced to 'never' because property '...' has
//! conflicting types in some constituents.`
//!
//! tsz already reports the top-level `TS2339`/`never` correctly; this family
//! covers the elaboration line, which was previously dropped entirely.
//! Oracle-verified against `typescript@7.0.2`.
//!
//! Scope: this only fires when *exactly one* property name conflicts.
//! `ObjectShape::properties` is sorted by interned `Atom` id (for canonical
//! hashing identity), not source declaration order, so when more than one
//! property conflicts there is no order-independent way to recover tsc's
//! "first written" choice from the interned shape alone — seeing that is a
//! silent (not wrong) no-elaboration case, tracked separately.

use tsz_checker::diagnostics::Diagnostic;
use tsz_checker::test_utils::check_source_diagnostics;

fn ts2339(source: &str) -> Diagnostic {
    let diagnostics: Vec<Diagnostic> = check_source_diagnostics(source)
        .into_iter()
        .filter(|diagnostic| diagnostic.code == 2339)
        .collect();
    assert_eq!(
        diagnostics.len(),
        1,
        "expected exactly one TS2339 diagnostic, got {diagnostics:#?}"
    );
    diagnostics.into_iter().next().unwrap()
}

fn related_messages(diagnostic: &Diagnostic) -> Vec<String> {
    diagnostic
        .related_information
        .iter()
        .map(|related| related.message_text.clone())
        .collect()
}

#[test]
fn interface_intersection_single_conflict_names_property() {
    let diag = ts2339(
        r"
interface A { x: 1 }
interface B { x: 2 }
declare const c: A & B;
c.x;
",
    );
    assert_eq!(
        related_messages(&diag),
        vec![
            "The intersection 'A & B' was reduced to 'never' because property 'x' has conflicting types in some constituents."
        ]
    );
}

#[test]
fn renamed_binders_single_conflict_names_property() {
    let diag = ts2339(
        r"
interface Left { shape: 1 }
interface Right { shape: 2 }
declare const value: Left & Right;
value.shape;
",
    );
    assert_eq!(
        related_messages(&diag),
        vec![
            "The intersection 'Left & Right' was reduced to 'never' because property 'shape' has conflicting types in some constituents."
        ]
    );
}

#[test]
fn conflict_named_independent_of_accessed_property() {
    // Only `y` conflicts (`x` is compatible across both members); tsc names
    // the conflicting property, not the accessed one.
    let diag = ts2339(
        r"
interface A { x: number; y: 'a' }
interface B { x: number; y: 'b' }
declare const c: A & B;
c.x;
",
    );
    assert_eq!(
        related_messages(&diag),
        vec![
            "The intersection 'A & B' was reduced to 'never' because property 'y' has conflicting types in some constituents."
        ]
    );
}

#[test]
fn multiple_conflicting_properties_omits_ambiguous_elaboration() {
    // Both `x` and `y` conflict; tsz cannot recover tsc's "first written"
    // pick from the interned (Atom-sorted) shape, so it stays silent rather
    // than risk naming the wrong one.
    let diag = ts2339(
        r"
interface A { x: 1; y: 'a' }
interface B { x: 2; y: 'b' }
declare const c: A & B;
c.x;
",
    );
    assert!(related_messages(&diag).is_empty());
}

#[test]
fn primitive_disjoint_intersection_has_no_elaboration() {
    // `string & number` also reduces to `never`, but tsc attaches no
    // "was reduced to never" elaboration for primitive-disjoint intersections
    // — only the object-literal-conflict family gets one.
    let diag = ts2339(
        r"
declare const c: string & number;
c.toString();
",
    );
    assert!(related_messages(&diag).is_empty());
}

#[test]
fn explicit_never_receiver_has_no_elaboration() {
    let diag = ts2339(
        r"
declare const c: never;
c.x;
",
    );
    assert!(related_messages(&diag).is_empty());
}

#[test]
fn compatible_intersection_members_report_no_ts2339() {
    let diagnostics: Vec<Diagnostic> = check_source_diagnostics(
        r"
interface A { x: number }
interface B { y: string }
declare const c: A & B;
c.x;
c.y;
",
    )
    .into_iter()
    .filter(|diagnostic| diagnostic.code == 2339)
    .collect();
    assert!(diagnostics.is_empty(), "unexpected: {diagnostics:#?}");
}

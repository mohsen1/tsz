//! Regression tests for #16753: a target intersection that mixes an object with
//! a non-mergeable member (a tuple/array) must elaborate the first **written**
//! failing constituent, matching tsc's `typeRelatedToEachType`.
//!
//! Structural rule (verified against tsc 7.0.2): tsc relates a source to each
//! constituent of `C1 & C2 & …` in source order and elaborates the first one it
//! fails. tsz's interner reorders intersection members into a canonical
//! (object-last) form so that structurally equal intersections built along
//! different paths hash-cons to one `TypeId` — which dropped the source order,
//! so `{ z: 1 } & [string, number]` named the tuple where tsc names `{ z: 1; }`.
//! The interner now records the written order separately
//! (`intersection_source_order`, diagnostics-only) and the assignability
//! elaboration recovers it, without perturbing type identity, display, or
//! relations.
//!
//! Note: because `A & B` and `B & A` intern to the same `TypeId` in tsz, only
//! the first-written order can be recorded for a shared canonical type; the
//! cases below are each self-contained so the order under test is the one
//! recorded.

use tsz_checker::diagnostics::Diagnostic;

fn check(source: &str) -> Vec<Diagnostic> {
    let libs = tsz_checker::test_utils::load_default_lib_files();
    tsz_checker::test_utils::check_source_with_libs(
        source,
        "test.ts",
        tsz_checker::context::CheckerOptions {
            strict: true,
            ..Default::default()
        },
        &libs,
    )
}

/// The full elaboration text of the single TS2322 (main message + every
/// related-information line).
fn ts2322_elaboration(source: &str) -> String {
    let diags = check(source);
    let matching: Vec<_> = diags.iter().filter(|d| d.code == 2322).collect();
    assert_eq!(
        matching.len(),
        1,
        "expected exactly one TS2322, got: {:?}",
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

fn assert_names_object(source: &str, label: &str) {
    let text = ts2322_elaboration(source);
    assert!(
        text.contains("to type '{ z: 1; }'"),
        "[{label}] expected the elaboration to name the object constituent `{{ z: 1; }}`, got:\n{text}"
    );
}

/// The headline witness: an object-first intersection with a spread tuple names
/// the object, not the tuple.
#[test]
fn issue_16753_object_first_spread_tuple_names_object() {
    assert_names_object(
        "type B = { z: 1 } & [string, ...[number, boolean]];\nconst b: B = 1;",
        "{ z: 1 } & [string, ...[number, boolean]]",
    );
}

/// Not spread-specific: a plain tuple member behaves the same.
#[test]
fn issue_16753_object_first_plain_tuple_names_object() {
    assert_names_object(
        "type C = { z: 1 } & [string, number];\nconst c: C = 1;",
        "{ z: 1 } & [string, number]",
    );
}

/// Three-way: two objects then a tuple names the first written object.
#[test]
fn issue_16753_object_first_three_way_names_first_object() {
    assert_names_object(
        "type E = { z: 1 } & { w: 2 } & [string, number];\nconst e: E = 1;",
        "{ z: 1 } & { w: 2 } & [string, number]",
    );
}

/// Renamed binders: the alias/property names do not drive the selection.
#[test]
fn issue_16753_object_first_names_object_binder_independent() {
    let text =
        ts2322_elaboration("type Blend = { marker: 1 } & [string, number];\nconst v: Blend = 1;");
    assert!(
        text.contains("to type '{ marker: 1; }'"),
        "expected the object constituent named, got:\n{text}"
    );
}

/// Control (matches on `main` already): a **tuple-first** intersection still
/// names the tuple — no source-order reorder is recorded, so the canonical
/// order (tuple already first) is used. This pins that the fix does not flip the
/// already-correct direction.
#[test]
fn issue_16753_tuple_first_still_names_tuple() {
    let text = ts2322_elaboration("type D = [string, number] & { z: 1 };\nconst d: D = 1;");
    assert!(
        text.contains("to type '[string, number]'"),
        "expected the tuple constituent named for a tuple-first intersection, got:\n{text}"
    );
}

/// Control: a pure object∧object intersection is unaffected (it merges into a
/// single object with a written-order display alias and already named the first
/// constituent correctly).
#[test]
fn issue_16753_object_only_intersection_unaffected() {
    assert_names_object(
        "type O = { z: 1 } & { w: 2 };\nconst o: O = 1;",
        "{ z: 1 } & { w: 2 }",
    );
}

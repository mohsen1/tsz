//! Regression tests for issue #14792 — an outer annotation must not leak into
//! a generic call's inline arrow-callback return-position object literal.
//!
//! For `declare function call<T, U>(x: T, fn: (x: T) => U): U;` and
//! `const bad: { v: string } = call(1, (x) => ({ v: x }));`, `tsc` seeds `U`
//! from the outer contextual type but ranks the callback's argument inference
//! above it (`InferencePriority.ReturnType`). The callback's return value
//! `{ v: x }` is fully determined by the pinned input `x: T = number`, so the
//! outer `{ v: string }` cannot refine it and the body is checked against the
//! inferred `U = { v: number }`. The single mismatch is reported once, on the
//! assignment, never a second time inside the callback body.
//!
//! `tsz` previously pushed the outer annotation onto the return literal and
//! emitted an extra TS2322 inside the callback body.
use crate::test_utils::{check_source_diagnostics, diagnostic_line_column};

/// The callback's arrow is the SECOND `=>` in these sources (the first is the
/// `(x: T) => U` function-type annotation on line 1). Diagnostics anchored
/// before it are on the assignment side; those after it are inside the callback
/// body.
fn callback_arrow_offset(source: &str) -> u32 {
    source
        .match_indices("=>")
        .nth(1)
        .expect("callback arrow present")
        .0 as u32
}

fn dump<'a>(
    source: &str,
    diags: impl IntoIterator<Item = &'a crate::diagnostics::Diagnostic>,
) -> String {
    diags
        .into_iter()
        .map(|d| {
            format!(
                "TS{}@{:?}: {}",
                d.code,
                diagnostic_line_column(source, d),
                d.message_text
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Witness from the issue: exactly one TS2322, on the assignment — not a
/// second one inside the callback body.
#[test]
fn outer_annotation_does_not_duplicate_into_callback_return_literal() {
    let source = "declare function call<T, U>(x: T, fn: (x: T) => U): U;\nconst bad: { v: string } = call(1, (x) => ({ v: x }));\n";
    let diags = check_source_diagnostics(source);

    let ts2322: Vec<_> = diags.iter().filter(|d| d.code == 2322).collect();
    assert_eq!(
        ts2322.len(),
        1,
        "Expected exactly one TS2322 (on the assignment). All diags:\n{}",
        dump(source, diags.iter())
    );
    let arrow = callback_arrow_offset(source);
    assert!(
        ts2322[0].start < arrow,
        "TS2322 should anchor on the assignment, not inside the callback body; got {:?}",
        diagnostic_line_column(source, ts2322[0])
    );
}

/// Same shape with renamed binders and a renamed-type-parameter signature.
/// Locks in that the fix is structural, not keyed to identifier spellings.
#[test]
fn outer_annotation_leak_fix_is_not_name_keyed() {
    let source = "declare function apply<A, R>(seed: A, project: (input: A) => R): R;\nconst out: { label: string } = apply(7, (input) => ({ label: input }));\n";
    let diags = check_source_diagnostics(source);

    let ts2322: Vec<_> = diags.iter().filter(|d| d.code == 2322).collect();
    assert_eq!(
        ts2322.len(),
        1,
        "Expected exactly one TS2322 with renamed binders. All diags:\n{}",
        dump(source, diags.iter())
    );
    let arrow = callback_arrow_offset(source);
    assert!(
        ts2322[0].start < arrow,
        "TS2322 should anchor on the assignment, got {:?}",
        diagnostic_line_column(source, ts2322[0])
    );
}

/// Compatible annotation: the callback return matches the inferred `U`, so the
/// call is clean. Suppression must not invent an error here.
#[test]
fn compatible_annotation_stays_clean() {
    let source = "declare function call<T, U>(x: T, fn: (x: T) => U): U;\nconst ok: { v: number } = call(1, (x) => ({ v: x }));\n";
    let diags = check_source_diagnostics(source);

    let ts2322: Vec<_> = diags.iter().filter(|d| d.code == 2322).collect();
    assert!(
        ts2322.is_empty(),
        "Compatible annotation must stay clean. All diags:\n{}",
        dump(source, diags.iter())
    );
}

/// Literal-preservation control: a nullary callback returning a free literal
/// still needs the contextual return to refine the literal (`1` stays `1`).
/// The narrow #14792 suppression must NOT fire here.
#[test]
fn free_literal_callback_return_keeps_contextual_refinement() {
    let source = "declare function invoke<T>(f: () => T): T;\nconst x: 1 = invoke(() => 1);\n";
    let diags = check_source_diagnostics(source);

    let unexpected: Vec<_> = diags.iter().filter(|d| d.code == 2322).collect();
    assert!(
        unexpected.is_empty(),
        "Free-literal callback return must keep contextual refinement (no TS2322). All diags:\n{}",
        dump(source, diags.iter())
    );
}

/// Free array-literal callback return: `tsc` lets the contextual return refine
/// the array literal into a tuple, so the narrow #14792 suppression must NOT
/// fire (its body is an array literal, not an object literal built from pinned
/// params). The element values are free literals the contextual type refines.
#[test]
fn free_array_literal_callback_return_is_not_suppressed() {
    let source = "declare function pick<T, U>(x: T, fn: (v: T) => U): U;\nconst r: [number, number] = pick(0, (v) => [1, 2]);\n";
    let diags = check_source_diagnostics(source);

    // The point of this control is that the #14792 suppression does not fire on
    // a free array-literal return; whatever the baseline diagnostics are, they
    // must not gain a NEW duplicate from this change. tsc accepts this program,
    // so there should be no TS2322.
    let ts2322: Vec<_> = diags.iter().filter(|d| d.code == 2322).collect();
    assert!(
        ts2322.is_empty(),
        "Free array-literal return must keep contextual tuple refinement (no TS2322). All diags:\n{}",
        dump(source, diags.iter())
    );
}

/// Literal-target variant: the duplicate/leak is gone — exactly one TS2322 is
/// emitted (the result `{ v: number }` vs the target `{ v: 5 }`), matching
/// `tsc`'s error count. (The leaf-anchor location for the literal target is
/// governed by a separate diagnostic-elaboration path and is not part of this
/// inference fix.)
#[test]
fn literal_target_variant_has_no_duplicate_error() {
    let source = "declare function call<T, U>(x: T, fn: (x: T) => U): U;\nconst r: { v: 5 } = call(1, (x) => ({ v: x }));\n";
    let diags = check_source_diagnostics(source);

    let ts2322: Vec<_> = diags.iter().filter(|d| d.code == 2322).collect();
    assert_eq!(
        ts2322.len(),
        1,
        "Expected exactly one TS2322 for the literal-target variant (no duplicate). All diags:\n{}",
        dump(source, diags.iter())
    );
}

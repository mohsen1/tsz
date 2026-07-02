//! A fresh **array literal** assigned to a **tuple target with a rest element**
//! must match `tsc`'s element-drill-in cap.
//!
//! `tsc`'s `generateLimitedTupleElements` skips any source element whose index
//! has no *fixed* slot in the tuple-like target (`isTupleLikeType(target) &&
//! !getPropertyOfType(target, `${i}`)`). So only the **leading fixed prefix**
//! before the first rest element is ever reported at the element level; any
//! source element covered by the rest element falls back to the whole-tuple
//! relation, which renders the `Type at position(s) i[ through j] in source is
//! not compatible with type at position k in target.` chain anchored at the
//! initializer.
//!
//! `tsz` previously capped element drill-in only when the target tuple had
//! *trailing* fixed elements after the rest, so `[number, ...boolean[]]` wrongly
//! drilled into the rest-covered source element (wrong anchor, no chain, and one
//! range-frame split into N leaf errors). The cap now applies whenever the
//! target pairs a rest element with any fixed element. The rule is structural:
//! it depends on the tuple shape, not the binder name or element type, so the
//! cases vary both.

use tsz_checker::diagnostics::Diagnostic;
use tsz_checker::test_utils::check_source_diagnostics;

fn ts2322(source: &str) -> Vec<Diagnostic> {
    check_source_diagnostics(source)
        .into_iter()
        .filter(|diagnostic| diagnostic.code == 2322)
        .collect()
}

fn related_lines(diagnostic: &Diagnostic) -> Vec<String> {
    diagnostic
        .related_information
        .iter()
        .map(|related| related.message_text.clone())
        .collect()
}

fn has_related(diagnostic: &Diagnostic, expected: &str) -> bool {
    diagnostic
        .related_information
        .iter()
        .any(|related| related.message_text == expected)
}

/// Assert the source produces exactly one TS2322 whose headline is `expected` —
/// the per-element leaf drilled at the mismatching slot (no whole-tuple chain).
fn assert_single_leaf(source: &str, expected: &str) {
    let diagnostics = ts2322(source);
    assert_eq!(
        diagnostics.len(),
        1,
        "expected exactly one TS2322 element leaf, got {diagnostics:#?}"
    );
    assert_eq!(diagnostics[0].message_text, expected);
}

// ---------------------------------------------------------------------------
// Rest-covered failure only -> one whole-tuple relation chain, not a leaf.
// ---------------------------------------------------------------------------

/// A single rest-covered element mismatch reports the whole-tuple relation at
/// the initializer with the `Type at position 1 …` frame, not a bare element
/// leaf.
#[test]
fn rest_covered_single_failure_reports_whole_tuple_chain() {
    for source in [
        "const bad: [number, ...boolean[]] = [1, \"no\"];",
        // Renamed binder + different element types: same structural reason.
        "const other: [string, ...number[]] = [\"lead\", true];",
    ] {
        let diagnostics = ts2322(source);
        assert_eq!(
            diagnostics.len(),
            1,
            "expected exactly one TS2322 (the whole-tuple chain), got {diagnostics:#?}"
        );
        let diagnostic = &diagnostics[0];
        assert!(
            has_related(
                diagnostic,
                "Type at position 1 in source is not compatible with type at position 1 in target."
            ),
            "missing the whole-tuple position frame; related = {:#?}",
            related_lines(diagnostic)
        );
    }
}

/// Several consecutive rest-covered failures collapse into a single
/// `Type at positions i through j …` range frame, not one leaf per element.
#[test]
fn rest_covered_multiple_failures_report_position_range() {
    let diagnostics = ts2322("const bad: [number, ...boolean[]] = [1, \"a\", \"b\"];");
    assert_eq!(
        diagnostics.len(),
        1,
        "expected one TS2322 range frame, got {diagnostics:#?}"
    );
    assert!(
        has_related(
            &diagnostics[0],
            "Type at positions 1 through 2 in source is not compatible with type at position 1 in target."
        ),
        "missing the positional-range frame; related = {:#?}",
        related_lines(&diagnostics[0])
    );
}

// ---------------------------------------------------------------------------
// Mixed failures: the leading fixed prefix still drills; rest-covered elements
// are dropped (tsc reports only the fixed-prefix leaves).
// ---------------------------------------------------------------------------

/// When a leading-fixed element fails, `tsc` drills into it and drops the
/// rest-covered failure entirely (element elaboration reported an error, so the
/// whole-tuple fallback never fires). Exactly one leaf, at the fixed slot.
#[test]
fn fixed_prefix_failure_drills_and_drops_rest_covered() {
    assert_single_leaf(
        "const bad: [string, ...boolean[]] = [1, \"no\"];",
        "Type 'number' is not assignable to type 'string'.",
    );
}

/// A fixed-prefix failure that is *not* at position 0 still drills in at its
/// true slot, and the trailing rest-covered element is dropped.
#[test]
fn fixed_prefix_failure_after_passing_slot_drills() {
    assert_single_leaf(
        "const bad: [number, boolean, ...string[]] = [1, \"no\", \"ok\"];",
        "Type 'string' is not assignable to type 'boolean'.",
    );
}

// ---------------------------------------------------------------------------
// Array-like and closed targets keep drilling into each element (unchanged):
// a lone-rest `[...T[]]` normalizes to an array, a plain array target indexes
// at every position, and a closed tuple makes every position a fixed slot.
// ---------------------------------------------------------------------------

/// A lone-rest tuple `[...T[]]` is array-like, so `tsc` reports each element
/// individually — the cap must not intercept it.
#[test]
fn lone_rest_tuple_still_drills_into_element() {
    assert_single_leaf(
        "const bad: [...boolean[]] = [\"no\"];",
        "Type 'string' is not assignable to type 'boolean'.",
    );
}

/// A plain array target keeps per-element drill-in.
#[test]
fn plain_array_target_still_drills_into_element() {
    assert_single_leaf(
        "const bad: boolean[] = [true, \"no\"];",
        "Type 'string' is not assignable to type 'boolean'.",
    );
}

/// A closed (rest-free) tuple keeps its per-element leaf — the cap only applies
/// when a rest element is present.
#[test]
fn closed_tuple_still_drills_into_element() {
    assert_single_leaf(
        "const bad: [number, boolean] = [1, \"no\"];",
        "Type 'string' is not assignable to type 'boolean'.",
    );
}

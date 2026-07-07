//! A fresh **array literal** assigned to a **tuple target with a rest element**
//! must match `tsc`'s element-drill-in cap.
//!
//! `tsc`'s `generateLimitedTupleElements` skips any source element whose index
//! has no *fixed* slot in the tuple-like target (`isTupleLikeType(target) &&
//! !getPropertyOfType(target, `${i}`)`). So only the **leading fixed prefix**
//! (required or optional slots) before the first rest element is ever reported
//! at the element level; any source element covered by the rest element falls
//! back to the whole-tuple relation, which renders the `Type at position(s)
//! i[ through j] in source is not compatible with type at position k in
//! target.` chain anchored at the initializer (or, for a call argument, at the
//! argument under `TS2345`).
//!
//! `tsz` previously capped element drill-in only when the target tuple had
//! *trailing* fixed elements after the rest, so `[number, ...boolean[]]` (rest
//! last) wrongly drilled into the rest-covered source element — wrong anchor,
//! no positional chain, and one range-frame split into N leaf errors. The cap
//! now applies whenever the target pairs a rest element with any fixed
//! element. The rule is structural: it depends only on the tuple shape, so the
//! cases vary binder names, element types, wrappers (readonly), aliases,
//! generic instantiations, and both elaboration entry points.
//!
//! Differential-verified against `tsc 6.0.2` (`--noEmit --strict`).

use tsz_checker::diagnostics::Diagnostic;
use tsz_checker::test_utils::check_source_diagnostics;

fn with_code(source: &str, code: u32) -> Vec<Diagnostic> {
    check_source_diagnostics(source)
        .into_iter()
        .filter(|diagnostic| diagnostic.code == code)
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

/// Assert the source produces exactly one diagnostic with `code` and return it.
fn single(source: &str, code: u32) -> Diagnostic {
    let diagnostics = with_code(source, code);
    assert_eq!(
        diagnostics.len(),
        1,
        "expected exactly one TS{code}, got {diagnostics:#?}"
    );
    diagnostics.into_iter().next().unwrap()
}

/// Assert the source produces exactly one TS2322 whose headline is `expected` —
/// the per-element leaf drilled at the mismatching slot (no whole-tuple chain).
fn assert_single_leaf(source: &str, expected: &str) {
    let diagnostic = single(source, 2322);
    assert_eq!(diagnostic.message_text, expected);
}

/// Assert the source produces exactly one diagnostic with `code` carrying the
/// whole-tuple positional frame `expected_frame` in its related information.
fn assert_single_chain_with_code(source: &str, code: u32, expected_frame: &str) {
    let diagnostic = single(source, code);
    assert!(
        has_related(&diagnostic, expected_frame),
        "missing the whole-tuple position frame; related = {:#?}",
        related_lines(&diagnostic)
    );
}

/// Assert the source produces exactly one TS2322 carrying the whole-tuple
/// positional frame `expected_frame` in its related information.
fn assert_single_chain(source: &str, expected_frame: &str) {
    assert_single_chain_with_code(source, 2322, expected_frame);
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
        assert_single_chain(
            source,
            "Type at position 1 in source is not compatible with type at position 1 in target.",
        );
    }
}

/// Several consecutive rest-covered failures collapse into a single
/// `Type at positions i through j …` range frame, not one leaf per element.
#[test]
fn rest_covered_multiple_failures_report_position_range() {
    assert_single_chain(
        "const bad: [number, ...boolean[]] = [1, \"a\", \"b\"];",
        "Type at positions 1 through 2 in source is not compatible with type at position 1 in target.",
    );
}

/// A rest-covered failure past a longer passing fixed prefix still defers to
/// the whole-tuple relation (the frame indexes the rest element's slot).
#[test]
fn rest_covered_failure_after_two_passing_fixed_slots_chains() {
    assert_single_chain(
        "const bad: [number, string, ...boolean[]] = [1, \"s\", \"no\"];",
        "Type at position 2 in source is not compatible with type at position 2 in target.",
    );
}

// ---------------------------------------------------------------------------
// Target wrappers and indirections resolve to the same tuple shape: the cap
// must apply through `readonly`, aliases, and generic instantiations.
// ---------------------------------------------------------------------------

/// A `readonly` rest-tuple target chains exactly like the mutable one.
#[test]
fn readonly_rest_tuple_target_reports_whole_tuple_chain() {
    assert_single_chain(
        "const bad: readonly [number, ...boolean[]] = [1, \"no\"];",
        "Type at position 1 in source is not compatible with type at position 1 in target.",
    );
}

/// An aliased rest-tuple target chains: the cap sees the resolved tuple shape,
/// not the alias node.
#[test]
fn alias_rest_tuple_target_reports_whole_tuple_chain() {
    assert_single_chain(
        "type Row = [number, ...boolean[]];\nconst bad: Row = [1, \"no\"];",
        "Type at position 1 in source is not compatible with type at position 1 in target.",
    );
}

/// A generic alias instantiated to a rest tuple chains: the cap applies to the
/// instantiated shape.
#[test]
fn generic_alias_instantiation_reports_whole_tuple_chain() {
    assert_single_chain(
        "type Wrap<T> = [number, ...T[]];\nconst bad: Wrap<boolean> = [1, \"no\"];",
        "Type at position 1 in source is not compatible with type at position 1 in target.",
    );
}

// ---------------------------------------------------------------------------
// Optional fixed slots are fixed slots: they count toward the drill prefix and
// keep drilling; only rest-covered positions defer.
// ---------------------------------------------------------------------------

/// A mismatch at an optional fixed slot ahead of the rest element still drills
/// into that element (`tsc` has a fixed property for the optional index).
#[test]
fn optional_fixed_slot_mismatch_drills() {
    assert_single_leaf(
        "const bad: [number, string?, ...boolean[]] = [1, 2, true];",
        "Type 'number' is not assignable to type 'string'.",
    );
}

/// With the optional fixed slot passing, a rest-covered failure after it
/// defers to the whole-tuple relation.
#[test]
fn rest_covered_failure_after_optional_fixed_slot_chains() {
    assert_single_chain(
        "const bad: [number, string?, ...boolean[]] = [1, \"x\", \"no\"];",
        "Type at position 2 in source is not compatible with type at position 2 in target.",
    );
}

// ---------------------------------------------------------------------------
// Entry points: the same cap governs the variable-initializer and the
// call-argument elaborators, and it recurses into nested array literals.
// ---------------------------------------------------------------------------

/// A call argument with a rest-covered mismatch reports the `TS2345` header
/// with the whole-tuple frame, not a drilled element leaf.
#[test]
fn call_argument_rest_covered_failure_reports_ts2345_chain() {
    assert_single_chain_with_code(
        "function take(row: [number, ...boolean[]]) {}\ntake([1, \"no\"]);",
        2345,
        "Type at position 1 in source is not compatible with type at position 1 in target.",
    );
}

/// A nested array literal drills through the (closed) outer tuple slot, then
/// the inner rest-tuple failure renders the whole-tuple chain for the inner
/// relation.
#[test]
fn nested_array_literal_inner_rest_tuple_chains() {
    assert_single_chain(
        "const bad: [[number, ...boolean[]]] = [[1, \"no\"]];",
        "Type at position 1 in source is not compatible with type at position 1 in target.",
    );
}

/// A parenthesized nested array literal takes the same path: the element gate
/// and the display anchor both see through parens, so the inner rest-tuple
/// failure still renders the whole-tuple chain.
#[test]
fn parenthesized_nested_array_literal_inner_rest_tuple_chains() {
    assert_single_chain(
        "const bad: [[number, ...boolean[]]] = [([1, \"no\"])];",
        "Type at position 1 in source is not compatible with type at position 1 in target.",
    );
}

// ---------------------------------------------------------------------------
// Mixed failures: the leading fixed prefix still drills; rest-covered elements
// are dropped (tsc reports only the fixed-prefix leaves).
// ---------------------------------------------------------------------------

/// When a leading-fixed element fails, `tsc` drills into it and drops the
/// rest-covered failure entirely (element elaboration reported an error, so
/// the whole-tuple fallback never fires). Exactly one leaf, at the fixed slot.
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

/// A closed (rest-free) tuple keeps its per-element leaf — the cap only
/// applies when a rest element is present.
#[test]
fn closed_tuple_still_drills_into_element() {
    assert_single_leaf(
        "const bad: [number, boolean] = [1, \"no\"];",
        "Type 'string' is not assignable to type 'boolean'.",
    );
}

// ---------------------------------------------------------------------------
// Middle-rest and trailing-fixed shapes: the pre-existing cap behavior, kept
// as regression guards (these were already correct before the generalization).
// ---------------------------------------------------------------------------

/// A middle-rest tuple (trailing fixed after the rest) defers rest-span
/// failures to the whole-tuple relation.
#[test]
fn middle_rest_with_trailing_fixed_chains() {
    assert_single_chain(
        "const bad: [number, ...boolean[], string] = [1, \"no\", \"s\"];",
        "Type at position 1 in source is not compatible with type at position 1 in target.",
    );
}

/// Zero leading fixed elements with a trailing fixed element caps drill-in at
/// index 0: everything defers to the whole-tuple relation.
#[test]
fn zero_leading_fixed_with_trailing_fixed_chains() {
    assert_single_chain(
        "const bad: [...boolean[], string] = [\"no\", \"s\"];",
        "Type at position 0 in source is not compatible with type at position 0 in target.",
    );
}

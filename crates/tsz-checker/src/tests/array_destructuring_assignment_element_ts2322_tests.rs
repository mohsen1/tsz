//! TS2322 parity for the elements of an array destructuring **assignment**.
//!
//! Structural rule: when an array destructuring assignment pattern's element is
//! a non-spread target (an identifier, a property access, an element access, or
//! any of those carrying a default), `tsc` judges the source's element type at
//! that index against the target's own type and reports TS2322 anchored on the
//! target — `checkArrayLiteralDestructuringElementAssignment`, the array half of
//! the same walk that already judges each property target of an object pattern.
//! tsz runs that judgement through the checker's shared destructuring
//! leaf-assignability owner.
//!
//! The whole-pattern relation is deliberately skipped for destructuring
//! assignments (`assignment_ops.rs`: "tsc processes each property/element
//! individually"), so before this fix nothing at all was reported for the plain
//! `[a, b] = [b, a]` shape — the per-element judgement only ran when
//! `noUncheckedIndexedAccess` was on.
//!
//! Every expectation here is pinned against `typescript@7.0.2`, the version in
//! `scripts/conformance/typescript-versions.json`. The witness comes from
//! `conformance/es6/destructuring/declarationsAndAssignments.ts`, whose `f18`
//! row expects exactly this pair of diagnostics.
//!
//! The decision is structural, never keyed on a binder's name: the renamed-
//! binder case below writes the same shape with different identifiers and
//! expects the same two diagnostics.

use crate::test_utils::check_source_non_strict;

/// `(code, anchor byte offset, anchored source text, message)` for every
/// diagnostic, in source order.
///
/// The offset is part of the row on purpose: an assertion that discards
/// position cannot tell "two targets each reported once" from "one target
/// reported twice", and this family reports one diagnostic per element.
fn rows(source: &str) -> Vec<(u32, u32, String, String)> {
    let mut out: Vec<(u32, u32, String, String)> = check_source_non_strict(source)
        .into_iter()
        .map(|d| {
            let start = d.start as usize;
            let end = start + d.length as usize;
            let text = source.get(start..end).unwrap_or_default().to_string();
            (d.code, d.start, text, d.message_text)
        })
        .collect();
    out.sort_by_key(|row| row.1);
    out
}

/// Byte offset of the `nth` (0-based) occurrence of `needle` in `source`.
fn offset_of(source: &str, needle: &str, nth: usize) -> u32 {
    let mut from = 0usize;
    for _ in 0..nth {
        from = source[from..].find(needle).expect("needle occurrence") + from + needle.len();
    }
    u32::try_from(source[from..].find(needle).expect("needle occurrence") + from).expect("offset")
}

fn ts2322(source: &str) -> Vec<(u32, u32, String, String)> {
    rows(source).into_iter().filter(|r| r.0 == 2322).collect()
}

// ---------------------------------------------------------------------------
// The witness: the conformance row's own shape.
// ---------------------------------------------------------------------------

#[test]
fn swapped_element_targets_each_report_their_own_ts2322() {
    // conformance/es6/destructuring/declarationsAndAssignments.ts, `f18`.
    // tsc 7.0.2:
    //   (4,2): Type 'string' is not assignable to type 'number'.
    //   (4,5): Type 'number' is not assignable to type 'string'.
    let source = "\ndeclare let n: number;\ndeclare let s: string;\n[n, s] = [s, n];\n";
    let pattern = offset_of(source, "[n, s]", 0);
    assert_eq!(
        ts2322(source),
        vec![
            (
                2322,
                pattern + 1,
                "n".to_string(),
                "Type 'string' is not assignable to type 'number'.".to_string(),
            ),
            (
                2322,
                pattern + 4,
                "s".to_string(),
                "Type 'number' is not assignable to type 'string'.".to_string(),
            ),
        ],
    );
}

#[test]
fn renamed_binders_report_the_same_two_diagnostics() {
    // Same shape, different identifiers: the rule is positional, not name-driven.
    let source = "\ndeclare let alpha: number;\ndeclare let omega: string;\n[alpha, omega] = [omega, alpha];\n";
    let pattern = offset_of(source, "[alpha, omega] =", 0);
    assert_eq!(
        ts2322(source),
        vec![
            (
                2322,
                pattern + 1,
                "alpha".to_string(),
                "Type 'string' is not assignable to type 'number'.".to_string(),
            ),
            (
                2322,
                pattern + 8,
                "omega".to_string(),
                "Type 'number' is not assignable to type 'string'.".to_string(),
            ),
        ],
    );
}

// ---------------------------------------------------------------------------
// Source spellings: the element type comes from a tuple, an array, or a
// literal, and all three reach the same judgement.
// ---------------------------------------------------------------------------

#[test]
fn tuple_typed_source_variable_reports_per_element() {
    let source = "\ndeclare let n: number;\ndeclare let s: string;\ndeclare let tup: [string, number];\n[n, s] = tup;\n";
    let pattern = offset_of(source, "[n, s] = tup", 0);
    assert_eq!(
        ts2322(source),
        vec![
            (
                2322,
                pattern + 1,
                "n".to_string(),
                "Type 'string' is not assignable to type 'number'.".to_string(),
            ),
            (
                2322,
                pattern + 4,
                "s".to_string(),
                "Type 'number' is not assignable to type 'string'.".to_string(),
            ),
        ],
    );
}

#[test]
fn array_typed_source_variable_reports_its_element_type() {
    // An unbounded `string[]` source: every index has element type `string`.
    let source = "\ndeclare let n: number;\ndeclare let sa: string[];\n[n] = sa;\n";
    assert_eq!(
        ts2322(source),
        vec![(
            2322,
            offset_of(source, "[n] = sa", 0) + 1,
            "n".to_string(),
            "Type 'string' is not assignable to type 'number'.".to_string(),
        )],
    );
}

// ---------------------------------------------------------------------------
// Target spellings other than a bare identifier.
// ---------------------------------------------------------------------------

#[test]
fn element_access_target_is_judged_like_an_identifier() {
    let source = "\ndeclare let n: number;\ndeclare let s: string;\ndeclare let na: number[];\n[na[0], s] = [s, n];\n";
    let pattern = offset_of(source, "[na[0], s]", 0);
    assert_eq!(
        ts2322(source),
        vec![
            (
                2322,
                pattern + 1,
                "na[0]".to_string(),
                "Type 'string' is not assignable to type 'number'.".to_string(),
            ),
            (
                2322,
                pattern + 8,
                "s".to_string(),
                "Type 'number' is not assignable to type 'string'.".to_string(),
            ),
        ],
    );
}

#[test]
fn property_access_target_is_judged_like_an_identifier() {
    let source = "\ndeclare let n: number;\ndeclare let s: string;\ndeclare let o: { p: number };\n[o.p, s] = [s, n];\n";
    let pattern = offset_of(source, "[o.p, s]", 0);
    // The anchored slice for the first row is `o.p,` — one token too long. A
    // property-access node used as an array-pattern element ends at the token
    // AFTER it, so the span swallows the `,` here and the `]` in the control
    // below. That end is the parser's to fix (the `#16251`/`#16259` node-span
    // family), not this judgement's, and it is deliberately left alone here:
    // the reported *start* is correct in both spellings, and start is what
    // tsc's line/column and the conformance fingerprint are scored on. Pinned
    // rather than papered over so the follow-up has a live witness.
    assert_eq!(
        ts2322(source),
        vec![
            (
                2322,
                pattern + 1,
                "o.p,".to_string(),
                "Type 'string' is not assignable to type 'number'.".to_string(),
            ),
            (
                2322,
                pattern + 6,
                "s".to_string(),
                "Type 'number' is not assignable to type 'string'.".to_string(),
            ),
        ],
    );
}

#[test]
fn property_access_target_span_over_extends_by_one_token() {
    // Control for the span quirk noted above: the extra character is whatever
    // token follows the target, so as the only element it swallows the `]`
    // rather than a comma. Same correct start, same message.
    let source = "\ndeclare let s: string;\ndeclare let o: { p: number };\n[o.p] = [s];\n";
    assert_eq!(
        ts2322(source),
        vec![(
            2322,
            offset_of(source, "[o.p] =", 0) + 1,
            "o.p]".to_string(),
            "Type 'string' is not assignable to type 'number'.".to_string(),
        )],
    );
}

// ---------------------------------------------------------------------------
// Defaults: tsc judges the DEFAULT against the target, and leaves the element
// alone when the source supplies it.
// ---------------------------------------------------------------------------

#[test]
fn a_default_that_mismatches_reports_the_defaults_type() {
    // tsc 7.0.2 reports exactly one diagnostic here, on `s`, about the default
    // `2` — the source element for index 1 is a `string` and is fine.
    let source = "\ndeclare let n: number;\ndeclare let s: string;\n[n = 1, s = 2] = [n, s];\n";
    assert_eq!(
        ts2322(source),
        vec![(
            2322,
            offset_of(source, "[n = 1, s = 2]", 0) + 8,
            "s".to_string(),
            "Type 'number' is not assignable to type 'string'.".to_string(),
        )],
    );
}

#[test]
fn a_matching_default_is_clean() {
    // conformance/es6/destructuring/declarationsAndAssignments.ts, `f18`'s last
    // row: `[a = 1, b = "abc"] = [2, "def"];` is clean under tsc.
    let source =
        "\ndeclare let n: number;\ndeclare let s: string;\n[n = 1, s = \"abc\"] = [2, \"def\"];\n";
    assert_eq!(ts2322(source), vec![]);
}

// ---------------------------------------------------------------------------
// Negative controls — shapes that must stay clean, so the new judgement cannot
// be read as "report whenever the element list is not identical".
// ---------------------------------------------------------------------------

#[test]
fn a_well_typed_swap_free_assignment_is_clean() {
    let source = "\ndeclare let n: number;\ndeclare let s: string;\n[n, s] = [n, s];\n";
    assert_eq!(ts2322(source), vec![]);
}

#[test]
fn an_any_source_is_clean() {
    let source =
        "\ndeclare let n: number;\ndeclare let s: string;\ndeclare let a: any;\n[n, s] = a;\n";
    assert_eq!(ts2322(source), vec![]);
}

#[test]
fn an_any_typed_target_is_clean() {
    let source = "\ndeclare let a: any;\ndeclare let s: string;\n[a, s] = [s, s];\n";
    assert_eq!(ts2322(source), vec![]);
}

#[test]
fn a_widening_union_target_accepts_both_element_types() {
    let source = "\ndeclare let u: number | string;\ndeclare let v: number | string;\n[u, v] = [1, \"x\"];\n";
    assert_eq!(ts2322(source), vec![]);
}

#[test]
fn an_omitted_element_does_not_shift_the_index() {
    // `[, s] = [n, n]` — the hole occupies index 0, so `s` is judged against
    // index 1 (`number`), not index 0. tsc reports one diagnostic, on `s`.
    let source = "\ndeclare let n: number;\ndeclare let s: string;\n[, s] = [n, n];\n";
    assert_eq!(
        ts2322(source),
        vec![(
            2322,
            offset_of(source, "[, s] =", 0) + 3,
            "s".to_string(),
            "Type 'number' is not assignable to type 'string'.".to_string(),
        )],
    );
}

// ---------------------------------------------------------------------------
// Nesting: the element judgement must not swallow the recursion that already
// handles nested patterns, and must not fire on the pattern node itself.
// ---------------------------------------------------------------------------

#[test]
fn a_nested_array_pattern_reports_inside_the_nested_pattern() {
    let source = "\ndeclare let n: number;\ndeclare let s: string;\n[[n], s] = [[s], n];\n";
    let pattern = offset_of(source, "[[n], s]", 0);
    assert_eq!(
        ts2322(source),
        vec![
            (
                2322,
                pattern + 2,
                "n".to_string(),
                "Type 'string' is not assignable to type 'number'.".to_string(),
            ),
            (
                2322,
                pattern + 6,
                "s".to_string(),
                "Type 'number' is not assignable to type 'string'.".to_string(),
            ),
        ],
    );
}

#[test]
fn a_nested_object_pattern_in_an_element_slot_reports_on_its_leaf() {
    let source = "\ndeclare let n: number;\ndeclare let s: string;\n[{ p: n }] = [{ p: s }];\n";
    assert_eq!(
        ts2322(source),
        vec![(
            2322,
            offset_of(source, "[{ p: n }] =", 0) + 6,
            "n".to_string(),
            "Type 'string' is not assignable to type 'number'.".to_string(),
        )],
    );
}

// ---------------------------------------------------------------------------
// Sources that cannot answer "what is the type at index i" make no judgement.
// Both rows were real conformance regressions caught by a reviewer's corpus
// run on the first revision of this change; both are `tsc`-clean.
// ---------------------------------------------------------------------------

#[test]
fn a_rest_elements_own_pattern_is_not_judged_positionally() {
    // conformance/es6/destructuring/restElementWithAssignmentPattern1.ts.
    // The rest slice falls back to the whole array when the source is not a
    // tuple, so index 0 and index 1 both report the merged `string | number`.
    // tsc types the source as `[string, number]` and is clean; judging against
    // the merged type would report two false TS2322.
    let source = "\ndeclare let a: string;\ndeclare let b: number;\n[...[a, b = 0]] = [\"\", 1];\n";
    assert_eq!(ts2322(source), vec![]);
}

#[test]
fn a_union_of_tuples_source_is_not_judged() {
    // conformance/types/tuple/unionsOfTupleTypes1.ts. tsc's element type at
    // index `i` is the union of each constituent's element at `i` — for
    // `[boolean] | [string, number]` that is `string | boolean` at 0 — while
    // the array fallback flattens every position of every constituent into
    // `string | number | boolean`. tsc reports no TS2322 on this row.
    let source = concat!(
        "\ntype T2 = [boolean] | [string, number];",
        "\ndeclare let d20: string | boolean;",
        "\ndeclare let d21: number | undefined;",
        "\ndeclare let d22: undefined;",
        "\ndeclare let t2: T2;",
        "\n[d20, d21, d22] = t2;\n"
    );
    assert_eq!(ts2322(source), vec![]);
}

#[test]
fn a_rest_element_with_a_simple_target_still_reports() {
    // The rest *target* itself is judged by the pre-existing spread branch,
    // which this containment does not touch: only the recursion into a rest
    // element's own nested pattern is de-positioned.
    let source = "\ndeclare let s: string;\ndeclare let sa: string[];\n[s, ...sa] = [s, s];\n";
    assert_eq!(ts2322(source), vec![]);
}

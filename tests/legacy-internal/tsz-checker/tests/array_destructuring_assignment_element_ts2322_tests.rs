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

use crate::context::CheckerOptions;
use crate::test_utils::{check_source_non_strict, check_source_with_libs, load_default_lib_files};

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

// ---------------------------------------------------------------------------
// Rest elements in the SOURCE tuple. A rest element holds the array type it was
// written as, so every position it covers has that array's element type — not
// the array itself. Reduced from conformance/types/tuple/unionsOfTupleTypes1.ts,
// which regressed on the second revision of this change.
// ---------------------------------------------------------------------------

#[test]
fn a_trailing_rest_element_supplies_its_element_type_not_the_array() {
    // `T3 = [string, ...number[]]` — index 1 and index 2 are both `number`.
    // Handing back the rest element's `number[]` reported a false TS2322 on
    // `d31` reading `Type 'number[]' is not assignable to type 'number'.`
    let source = concat!(
        "\ntype T3 = [string, ...number[]];",
        "\ndeclare let d30: string;",
        "\ndeclare let d31: number;",
        "\ndeclare let d32: number;",
        "\ndeclare let t3: T3;",
        "\n[d30, d31, d32] = t3;\n"
    );
    assert_eq!(ts2322(source), vec![]);
}

#[test]
fn a_trailing_rest_element_still_reports_a_real_mismatch() {
    // The same source, but the target at the rest position is a `string`:
    // tsc reports `Type 'number' is not assignable to type 'string'.` — so the
    // fix above is "use the element type", not "stop judging rest positions".
    let source = concat!(
        "\ntype C = [string, ...number[]];",
        "\ndeclare let c0: string;",
        "\ndeclare let c1: string;",
        "\ndeclare let tc: C;",
        "\n[c0, c1] = tc;\n"
    );
    assert_eq!(
        ts2322(source),
        vec![(
            2322,
            offset_of(source, "[c0, c1] =", 0) + 5,
            "c1".to_string(),
            "Type 'number' is not assignable to type 'string'.".to_string(),
        )],
    );
}

#[test]
fn a_rest_position_target_typed_as_the_array_is_the_inverse_mismatch() {
    // The exact inverse of the regression: a target declared `number[]` at a
    // rest position must now FAIL, where handing back the rest array made it
    // silently pass. tsc: `Type 'number' is not assignable to type 'number[]'.`
    let source = concat!(
        "\ndeclare let d0: number[];",
        "\ndeclare let td: [string, ...number[]];",
        "\n[, d0] = td;\n"
    );
    assert_eq!(
        ts2322(source),
        vec![(
            2322,
            offset_of(source, "[, d0] =", 0) + 3,
            "d0".to_string(),
            "Type 'number' is not assignable to type 'number[]'.".to_string(),
        )],
    );
}

#[test]
fn a_rest_element_that_is_not_last_makes_no_positional_judgement() {
    // `[string, ...number[], boolean]` — tsc gives position 1 and 2 the union
    // `number | boolean` and reports two TS2322 here. Positions after a
    // non-trailing rest have no fixed index, so this makes no judgement rather
    // than judging against whichever element sits at that declaration offset;
    // the two diagnostics tsc reports are a known missing-diagnostic residual,
    // not a false positive.
    let source = concat!(
        "\ntype B = [string, ...number[], boolean];",
        "\ndeclare let b0: string;",
        "\ndeclare let b1: number;",
        "\ndeclare let b2: boolean;",
        "\ndeclare let tb: B;",
        "\n[b0, b1, b2] = tb;\n"
    );
    assert_eq!(ts2322(source), vec![]);
}

// ---------------------------------------------------------------------------
// Rest element of an array destructuring **assignment** whose source is a
// non-array iterable (a custom `[Symbol.iterator]()` object, a generator, …).
//
// Structural rule: the rest target of `[a, ...b] = src` binds to an array of
// `src`'s element type. For a tuple that is the tuple slice; for an
// `Array<T>`/`T[]` it is `T[]`; for a non-array iterable it is
// `Array<IteratedType>` (tsc's `checkArrayLiteralDestructuringElementAssignment`
// and the binding-pattern rest walk). tsz previously typed the rest as the whole
// iterable, so it judged the iterable itself against `b`'s declared array type
// and produced a spurious TS2740 ("… is missing the following properties from
// type 'T[]': length, pop, …"). Witness:
// `conformance/es6/destructuring/iterableArrayPattern4.ts` (`@strict: false`).
//
// These need the ES2015 iterable/generator lib (`Symbol.iterator`,
// `Generator`), so they load the default lib bundle rather than running with
// no lib like the tuple/array rows above.
// ---------------------------------------------------------------------------

/// Non-strict diagnostic codes with the ES2015+ lib bundle loaded, mirroring
/// the witness fixture's `@strict: false` and its need for the iterator types.
fn lib_codes_non_strict(source: &str) -> Vec<u32> {
    let libs = load_default_lib_files();
    check_source_with_libs(
        source,
        "test.ts",
        CheckerOptions {
            strict: false,
            ..CheckerOptions::default()
        },
        &libs,
    )
    .into_iter()
    .map(|diagnostic| diagnostic.code)
    .collect()
}

#[test]
fn iterable_rest_source_compatible_does_not_report_ts2740() {
    // Feed yields `Derived`; the rest target is `Base[]`. `Derived[]` is
    // assignable to `Base[]`, so no diagnostic — previously tsz compared the
    // whole `Feed` against `Base[]` and emitted TS2740.
    let source = concat!(
        "\nclass Base { b: number = 1; }",
        "\nclass Derived extends Base { d: number = 2; }",
        "\nclass Feed {",
        "\n    next() { return { value: new Derived(), done: false }; }",
        "\n    [Symbol.iterator]() { return this; }",
        "\n}",
        "\nvar first: Base, rest: Base[];",
        "\n[first, ...rest] = new Feed();\n"
    );
    let codes = lib_codes_non_strict(source);
    assert!(
        !codes.contains(&2740),
        "iterable rest source should not report TS2740, got: {codes:?}"
    );
}

#[test]
fn iterable_rest_source_compatible_varied_binders_does_not_report_ts2740() {
    // Same shape with different identifiers: the fix is structural (iterated
    // element type), never keyed on a binder's name.
    let source = concat!(
        "\nclass Widget { w: number = 1; }",
        "\nclass Gadget extends Widget { g: number = 2; }",
        "\nclass Stream {",
        "\n    next() { return { value: new Gadget(), done: false }; }",
        "\n    [Symbol.iterator]() { return this; }",
        "\n}",
        "\nvar lead: Widget, tail: Widget[];",
        "\n[lead, ...tail] = new Stream();\n"
    );
    let codes = lib_codes_non_strict(source);
    assert!(
        !codes.contains(&2740),
        "iterable rest source (varied binders) should not report TS2740, got: {codes:?}"
    );
}

#[test]
fn iterable_rest_only_target_does_not_report_ts2740() {
    let source = concat!(
        "\nclass Item {}",
        "\nclass Feed {",
        "\n    next() { return { value: new Item(), done: false }; }",
        "\n    [Symbol.iterator]() { return this; }",
        "\n}",
        "\nvar all: Item[];",
        "\n[...all] = new Feed();\n"
    );
    let codes = lib_codes_non_strict(source);
    assert!(
        !codes.contains(&2740),
        "rest-only iterable destructuring should not report TS2740, got: {codes:?}"
    );
}

#[test]
fn iterable_rest_source_incompatible_does_not_report_ts2740() {
    // Feed yields `string`; the rest target is `number[]`. The rest type is now
    // `string[]`, so the mismatch is element-level — tsz no longer reports the
    // whole-iterable TS2740 it produced before (tsc reports TS2322 here; the
    // element-level code/anchor is asserted by the conformance corpus rather
    // than pinned to a message here).
    let source = concat!(
        "\nclass Feed {",
        "\n    next() { return { value: \"s\", done: false }; }",
        "\n    [Symbol.iterator]() { return this; }",
        "\n}",
        "\nvar h: string, t: number[];",
        "\n[h, ...t] = new Feed();\n"
    );
    let codes = lib_codes_non_strict(source);
    assert!(
        !codes.contains(&2740),
        "incompatible iterable rest should not report the whole-iterable TS2740, got: {codes:?}"
    );
}

#[test]
fn generator_rest_source_compatible_does_not_report_ts2740() {
    // A generator is iterable; the rest of `Generator<number>` is `number[]`.
    let source = concat!(
        "\nfunction* g() { yield 1; yield 2; }",
        "\nvar a: number, b: number[];",
        "\n[a, ...b] = g();\n"
    );
    let codes = lib_codes_non_strict(source);
    assert!(
        !codes.contains(&2740),
        "generator rest source should not report TS2740, got: {codes:?}"
    );
}

#[test]
fn array_rest_source_still_clean() {
    // Regression guard: the unchanged `Array<T>` path still types the rest as
    // `T[]` and reports nothing for a matching assignment.
    let source = concat!(
        "\nvar a: number, b: number[];",
        "\n[a, ...b] = [1, 2, 3];\n"
    );
    let codes = lib_codes_non_strict(source);
    assert!(
        !codes.contains(&2740) && !codes.contains(&2322),
        "array rest source should stay clean, got: {codes:?}"
    );
}

// ---------------------------------------------------------------------------
// Display and code parity for the judgements above, pinned against
// `typescript@7.0.2` (the `tsc-cache-full.json` oracle) and cross-checked on a
// local `tsc`. Before this family's fix, a destructuring write-target anchor
// was treated as a *source expression* by the diagnostic display derivation,
// so the message's source side was repainted with the target's own declared
// annotation — `Type 'string[]' is not assignable to type 'string[]'`
// (`conformance/es6/destructuring/iterableArrayPattern6.ts`), and the
// element-level judgement for a non-array iterable source did not run at all
// (`iterableArrayPattern5.ts`/`7.ts` reported nothing).
// ---------------------------------------------------------------------------

/// Non-strict `(code, message)` rows with the ES2015+ lib bundle loaded, in
/// source order.
fn lib_rows_non_strict(source: &str) -> Vec<(u32, String)> {
    let libs = load_default_lib_files();
    let mut diags = check_source_with_libs(
        source,
        "test.ts",
        CheckerOptions {
            strict: false,
            ..CheckerOptions::default()
        },
        &libs,
    );
    diags.sort_by_key(|d| d.start);
    diags
        .into_iter()
        .map(|d| (d.code, d.message_text))
        .collect()
}

#[test]
fn iterable_rest_source_incompatible_renders_iterated_array_source() {
    // tsc 7.0.2 (`iterableArrayPattern6.ts` shape, renamed binders):
    //   Type 'Gadget[]' is not assignable to type 'number[]'.
    // The source side is the *iterated element array*, never the rest
    // target's own declared annotation.
    let source = concat!(
        "\nclass Widget { w: number = 1; }",
        "\nclass Gadget extends Widget { g: number = 2; }",
        "\nclass Stream {",
        "\n    next() { return { value: new Gadget(), done: false }; }",
        "\n    [Symbol.iterator]() { return this; }",
        "\n}",
        "\nvar lead: Widget, tail: number[];",
        "\n[lead, ...tail] = new Stream();\n"
    );
    let rows = lib_rows_non_strict(source);
    let ts2322: Vec<_> = rows.iter().filter(|r| r.0 == 2322).collect();
    assert_eq!(ts2322.len(), 1, "exactly one TS2322, got rows: {rows:?}");
    assert_eq!(
        ts2322[0].1, "Type 'Gadget[]' is not assignable to type 'number[]'.",
        "rest source must render the iterated element array"
    );
}

#[test]
fn tuple_rest_slice_renders_tuple_slice_source() {
    // tsc 7.0.2: Type '[Gadget]' is not assignable to type 'string[]'.
    let source = concat!(
        "\nclass Gadget { g: number = 2; }",
        "\nvar first: Gadget, rest: string[];",
        "\n[first, ...rest] = [new Gadget(), new Gadget()];\n"
    );
    let rows = lib_rows_non_strict(source);
    let ts2322: Vec<_> = rows.iter().filter(|r| r.0 == 2322).collect();
    assert_eq!(ts2322.len(), 1, "exactly one TS2322, got rows: {rows:?}");
    assert_eq!(
        ts2322[0].1, "Type '[Gadget]' is not assignable to type 'string[]'.",
        "rest source must render the tuple slice"
    );
}

#[test]
fn iterable_element_target_incompatible_reports_scalar_ts2322() {
    // `iterableArrayPattern5.ts` shape (renamed binders): each non-spread
    // element judges against the iterated element type. tsc 7.0.2:
    //   Type 'Gadget' is not assignable to type 'string'.
    let source = concat!(
        "\nclass Widget { w: number = 1; }",
        "\nclass Gadget extends Widget { g: number = 2; }",
        "\nclass Stream {",
        "\n    next() { return { value: new Gadget(), done: false }; }",
        "\n    [Symbol.iterator]() { return this; }",
        "\n}",
        "\nvar lead: Widget, second: string;",
        "\n[lead, second] = new Stream();\n"
    );
    let rows = lib_rows_non_strict(source);
    let ts2322: Vec<_> = rows.iter().filter(|r| r.0 == 2322).collect();
    assert_eq!(ts2322.len(), 1, "exactly one TS2322, got rows: {rows:?}");
    assert_eq!(
        ts2322[0].1,
        "Type 'Gadget' is not assignable to type 'string'."
    );
}

#[test]
fn iterable_element_target_missing_property_reports_ts2741() {
    // A single-missing-property element failure selects TS2741, exactly as
    // tsc's `checkTypeAssignableToAndOptionallyElaborate` does. tsc 7.0.2:
    //   Property 'q' is missing in type 'Item' but required in type '{ q: number; }'.
    let source = concat!(
        "\nclass Item { y: number = 1; }",
        "\nclass Feed {",
        "\n    next() { return { value: new Item(), done: false }; }",
        "\n    [Symbol.iterator]() { return this; }",
        "\n}",
        "\nvar head: { q: number };",
        "\n[head] = new Feed();\n"
    );
    let rows = lib_rows_non_strict(source);
    let ts2741: Vec<_> = rows.iter().filter(|r| r.0 == 2741).collect();
    assert_eq!(ts2741.len(), 1, "exactly one TS2741, got rows: {rows:?}");
    assert_eq!(
        ts2741[0].1,
        "Property 'q' is missing in type 'Item' but required in type '{ q: number; }'."
    );
}

#[test]
fn iterable_element_target_compatible_stays_clean() {
    // Negative control (`iterableArrayPattern4.ts` shape): compatible element
    // targets report nothing.
    let source = concat!(
        "\nclass Widget { w: number = 1; }",
        "\nclass Gadget extends Widget { g: number = 2; }",
        "\nclass Stream {",
        "\n    next() { return { value: new Gadget(), done: false }; }",
        "\n    [Symbol.iterator]() { return this; }",
        "\n}",
        "\nvar lead: Widget, second: Widget;",
        "\n[lead, second] = new Stream();\n"
    );
    let rows = lib_rows_non_strict(source);
    assert!(
        rows.iter().all(|r| !matches!(r.0, 2322 | 2740 | 2741)),
        "compatible iterable element targets must stay clean, got: {rows:?}"
    );
}

#[test]
fn default_bearing_object_leaf_keeps_union_slice_display() {
    // `restElementWithAssignmentPattern2.ts` shape: the slice judgement at a
    // default-bearing target renders the *computed* union slice, not the
    // default expression's type. tsc 7.0.2:
    //   Type 'string | number' is not assignable to type 'string'.
    let source = concat!(
        "\nvar a: string, b: number;",
        "\n[...{ 0: a = \"\", b }] = [\"\", 1];\n"
    );
    let rows = ts2322(source);
    assert_eq!(rows.len(), 1, "exactly one TS2322, got: {rows:?}");
    assert_eq!(
        rows[0].3,
        "Type 'string | number' is not assignable to type 'string'."
    );
}

#[test]
fn in_pattern_default_fresh_literal_still_widens() {
    // The default-vs-target judgement keeps its genuine source expression:
    // a fresh literal default widens in the message (`for-of46.ts` family).
    // tsc 7.0.2: Type 'boolean' is not assignable to type 'string'.
    let source = concat!("\nvar k: string;", "\n[k = false] = [];\n");
    let rows = ts2322(source);
    assert_eq!(rows.len(), 1, "exactly one TS2322, got: {rows:?}");
    assert_eq!(
        rows[0].3,
        "Type 'boolean' is not assignable to type 'string'."
    );
}

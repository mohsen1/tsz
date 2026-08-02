//! Regression tests for the `arrayLiteralWidened.ts` conformance witness.
//!
//! Structural rule: when a nested array/tuple literal element (e.g. `[[null]]`
//! inside `[[[null]],[undefined]]`) resolves its `null`/`undefined` leaves,
//! that resolution normally happens only at the enclosing `var`/`let`
//! binding's widening seam — *after* the outer array literal's own
//! best-common-type (BCT) has already combined the sibling element types.
//! Pre-widening, `[[null]]` and `[undefined]` are structurally unrelated
//! (`Array<null-sentinel>` isn't a subtype of `Array<undefined>` the way the
//! post-widening `any[][]`/`any[]` pair is), so the BCT tournament never gets
//! a chance to see that the two siblings collapse once widened.
//!
//! tsc widens `[[null]]` to `any[][]` and `[undefined]` to `any[]` before
//! comparing them, and its `removeSubtypes` keeps the first-declared,
//! mutually-related candidate (`any[][]`) — never emitting a spurious
//! `any[] | any[][]` and never disagreeing with a same-shape sibling
//! redeclaration (`var c = [[[]]]; var c = [[[null]],[undefined]]` compiles
//! clean, both `any[][][]`).
//!
//! tsz's fix has two parts, at their respective owner layers:
//! - `crates/tsz-checker/src/types/computation/array_literal.rs`: eagerly
//!   widen each COMPOUND (array/tuple/object) element's nullish leaves before
//!   handing the element types to the solver's BCT, under `!strictNullChecks`
//!   (a bare scalar `null`/`undefined` element is left alone — it is already
//!   a universal subtype for BCT's own tournament under non-strict mode).
//! - `crates/tsz-solver/src/operations/expression_ops.rs`
//!   (`compute_best_common_type_cached`'s tournament): only replace the
//!   current "best" candidate with a new one that *strictly* dominates it
//!   (is a supertype but not also a subtype). Two candidates can be mutually
//!   related only through `any`'s absorption; the un-fixed tournament drifted
//!   to whichever mutually-related candidate came LAST.

use tsz_checker::test_utils::check_source_non_strict_codes;
use tsz_checker::test_utils::check_with_options_code_messages;
use tsz_checker::test_utils::non_strict_checker_options;

fn assert_clean(source: &str, why: &str) {
    let found = check_source_non_strict_codes(source);
    assert!(
        found.is_empty(),
        "{why}: expected no diagnostics under non-strict mode, got {found:?}"
    );
}

fn messages_non_strict(source: &str) -> Vec<(u32, String)> {
    check_with_options_code_messages(source, non_strict_checker_options())
}

// --- The reported witness (arrayLiteralWidened.ts) -------------------------

#[test]
fn nested_empty_array_redeclaration_matches_tsc() {
    // `TypeScript/tests/cases/conformance/types/typeRelationships/widenedTypes/arrayLiteralWidened.ts`
    assert_clean(
        r#"
var c = [[[]]];
var c = [[[null]],[undefined]];
"#,
        "tsc compiles this redeclaration clean (both sides are `any[][][]`)",
    );
}

#[test]
fn nested_empty_array_redeclaration_survives_renamed_binder() {
    assert_clean(
        r#"
var container = [[[]]];
var container = [[[null]],[undefined]];
"#,
        "the rule is structural, not name-keyed",
    );
}

#[test]
fn nested_empty_array_redeclaration_survives_unrelated_preceding_code() {
    // The tie-break must depend on this literal's own element order, not on
    // whatever `any[]`/`any[][]` shapes unrelated earlier code interned first.
    assert_clean(
        r#"
var unrelated1 = [];
var unrelated2 = [[], [null, null]];
var c = [[[]]];
var c = [[[null]],[undefined]];
"#,
        "an unrelated `any[]`/`any[][]` created earlier in the file must not change the tie-break",
    );
}

#[test]
fn sibling_elements_widen_to_the_same_type_directly() {
    // A single level of nesting: `[null]` and `[undefined]` both widen to
    // `any[]` directly (identical, not merely mutually related), so this
    // never needs the tie-break at all — a cheap adjacent control. The
    // TS2322 below is the deliberate witness assignment (`string` can never
    // hold an array), not a defect: it exists only to read the inferred
    // element type back out of the diagnostic message.
    let found = messages_non_strict(
        r#"
var oneLevel = [[null],[undefined]];
var check: string = oneLevel;
"#,
    );
    assert!(
        found
            .iter()
            .any(|(code, msg)| *code == 2322 && msg.contains("any[][]")),
        "expected TS2322 naming `any[][]`, got {found:?}"
    );
}

#[test]
fn deep_nested_mutual_tie_reports_first_declared_shape() {
    let found = messages_non_strict(
        r#"
var deep = [[[[null]]],[[undefined]]];
var check: string = deep;
"#,
    );
    assert!(
        found
            .iter()
            .any(|(code, msg)| *code == 2322 && msg.contains("any[][][][]")),
        "first-declared element must survive the mutual tie at any nesting depth, got {found:?}"
    );
}

// --- Negative controls: this must not become a blanket "prefer first" rule -

#[test]
fn genuinely_different_primitive_siblings_still_form_a_real_union() {
    // `[1]` and `["a"]` are not mutually related through `any` — ordinary BCT
    // must still produce a real union, not silently collapse to the first.
    let found = messages_non_strict(
        r#"
var mixed = [[1], ["a"]];
var check: string = mixed;
"#,
    );
    assert!(
        found.iter().any(|(code, msg)| *code == 2322
            && msg.contains("number[]")
            && msg.contains("string[]")),
        "unrelated sibling shapes must still union, got {found:?}"
    );
}

#[test]
fn strict_supertype_chain_still_prefers_the_supertype_not_the_first() {
    // A genuine (non-mutual) supertype relationship must still win regardless
    // of declaration order: `[1, "a" as string | number]` widens to
    // `(string | number)[]`, not `number[]` (the first element's own type).
    let found = messages_non_strict(
        r#"
declare const sn: string | number;
var chain = [1, sn];
var check: string = chain;
"#,
    );
    assert!(
        found
            .iter()
            .any(|(code, msg)| *code == 2322 && msg.contains("(string | number)[]")),
        "a strict (non-mutual) supertype must still win the tournament, got {found:?}"
    );
}

#[test]
fn unrelated_same_shape_classes_still_form_a_union() {
    // From the existing BCT tournament comment: unrelated same-shape classes
    // must not crown one of them winner via structural mutual-subtyping.
    let found = messages_non_strict(
        r#"
class A { x = 0; }
class B { x = 0; }
declare const a: A;
declare const b: B;
var instances = [a, b];
var check: string = instances;
"#,
    );
    assert!(
        found
            .iter()
            .any(|(code, msg)| *code == 2322 && msg.contains('|')),
        "unrelated same-shape classes must keep a union, not crown one winner, got {found:?}"
    );
}

#[test]
fn strict_null_checks_on_is_unaffected() {
    // Under strictNullChecks, `null`/`undefined` are never implicitly widened
    // to `any` (empty array literals become `never[]`, not `any[]`, and the
    // redeclaration reports TS2403 against a `never[][][]`/nullish-tuple
    // mismatch instead of matching). This pins that the eager pre-BCT
    // widening in `array_literal.rs` is gated on `!strict_null_checks()` and
    // does not fire — this fix is a non-strict-mode-only change.
    use tsz_checker::test_utils::check_source_strict_codes;
    let found = check_source_strict_codes(
        r#"
var c = [[[]]];
var c = [[[null]],[undefined]];
"#,
    );
    assert!(
        !found.is_empty(),
        "strictNullChecks mode must not silently accept a real nullish-in-never error"
    );
}

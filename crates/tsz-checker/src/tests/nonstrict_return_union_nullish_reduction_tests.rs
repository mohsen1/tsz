//! Tests for the non-strict (`strictNullChecks: false`) subtype reduction of a
//! block-bodied function's **inferred return union** (#16580 b5).
//!
//! tsc computes the inferred return type as
//! `getWidenedType(getUnionType(returns, UnionReduction.Subtype))`. Without
//! `strictNullChecks`, `null`/`undefined` are subtypes of every type, so
//! `getUnionType` drops a nullish return contribution whenever a non-nullish
//! sibling exists, and only widens the survivor afterwards:
//!
//! ```ts
//! function g() { if (c) return 1; return null; }   // () => number
//! function h(x) { if (x) return 1; }               // () => number (implicit undefined dropped)
//! ```
//!
//! tsz previously widened each nullish contribution to `any` *per branch, before*
//! the union, so `1 | null` collapsed to `any` and the later assignment was
//! silently accepted — a false negative. The widening is now deferred past the
//! reduction, so a sole-nullish return still reaches `any`
//! (`function f() { return null; }` → `any`) while a mixed union reduces to its
//! non-nullish survivor and widens it.
//!
//! Every case is pinned against `typescript@7.0.2`
//! (`--noEmit --strict false --target es2015`). Binder names are varied across
//! cases per the anti-hardcoding contract.

use crate::test_utils::{
    check_source_strict_codes, check_with_options_code_messages, non_strict_checker_options,
};

fn non_strict_2322_messages(src: &str) -> Vec<String> {
    check_with_options_code_messages(src, non_strict_checker_options())
        .into_iter()
        .filter(|(code, _)| *code == 2322)
        .map(|(_, msg)| msg)
        .collect()
}

fn non_strict_2322_present(src: &str) -> bool {
    check_with_options_code_messages(src, non_strict_checker_options())
        .iter()
        .any(|(code, _)| *code == 2322)
}

// ---------------------------------------------------------------------------
// Row b5: the headline false negative. `1 | null` reduces to `number`, so the
// `string` assignment is rejected — previously the union collapsed to `any` and
// nothing was reported.
// ---------------------------------------------------------------------------

#[test]
fn mixed_number_and_null_return_reduces_to_number() {
    let messages = non_strict_2322_messages(
        "function pickCount() { if (1) return 1; return null; }\n\
         var label: string = pickCount();",
    );
    assert_eq!(
        messages.len(),
        1,
        "expected exactly one TS2322: {messages:?}"
    );
    assert!(
        messages[0].contains("Type 'number' is not assignable to type 'string'"),
        "survivor should widen to `number`, not stay `1 | null`/`any`: {}",
        messages[0]
    );
}

#[test]
fn mixed_literal_and_undefined_return_reduces_and_widens() {
    // `"a" | undefined` → drop `undefined` → `"a"` → widen → `string`.
    let messages = non_strict_2322_messages(
        "function chooseTag(flag: boolean) { if (flag) return \"a\"; return undefined; }\n\
         var count: number = chooseTag(true);",
    );
    assert_eq!(
        messages.len(),
        1,
        "expected exactly one TS2322: {messages:?}"
    );
    assert!(
        messages[0].contains("Type 'string' is not assignable to type 'number'"),
        "survivor should widen to `string`: {}",
        messages[0]
    );
}

#[test]
fn implicit_fall_through_undefined_is_dropped() {
    // `function h(x) { if (x) return 1; }` — the implicit fall-through
    // `undefined` is dropped in non-strict, so the inferred return is `number`.
    let messages = non_strict_2322_messages(
        "function measure(active: boolean) { if (active) return 1; }\n\
         var name: string = measure(true);",
    );
    assert_eq!(
        messages.len(),
        1,
        "expected exactly one TS2322: {messages:?}"
    );
    assert!(
        messages[0].contains("Type 'number' is not assignable to type 'string'"),
        "implicit undefined must be dropped, leaving `number`: {}",
        messages[0]
    );
}

#[test]
fn bare_empty_return_undefined_is_dropped() {
    // A bare `return;` contributes `undefined`, dropped the same way.
    let messages = non_strict_2322_messages(
        "function tally(seen: boolean) { if (seen) return 1; return; }\n\
         var text: string = tally(true);",
    );
    assert_eq!(
        messages.len(),
        1,
        "expected exactly one TS2322: {messages:?}"
    );
    assert!(
        messages[0].contains("Type 'number' is not assignable to type 'string'"),
        "bare `return;` undefined must be dropped: {}",
        messages[0]
    );
}

#[test]
fn non_literal_survivor_beside_null_reduces_to_the_survivor() {
    // A non-literal survivor (`number`, from a parameter) beside a `null` return
    // reduces to plain `number` — no literal-widening ambiguity, just the drop.
    let messages = non_strict_2322_messages(
        "function classify(n: number, seen: boolean) {\n\
         \x20   if (seen) return n;\n\
         \x20   return null;\n\
         }\n\
         var ok: boolean = classify(0, true);",
    );
    assert_eq!(
        messages.len(),
        1,
        "expected exactly one TS2322: {messages:?}"
    );
    assert!(
        messages[0].contains("Type 'number' is not assignable to type 'boolean'"),
        "null dropped, survivor is `number`: {}",
        messages[0]
    );
}

#[test]
fn dropped_null_is_absent_from_the_rendered_survivor_union() {
    // With two non-nullish survivors the exact literal-widening is a separate
    // concern; the invariant the reduction owns is that `null` is gone from the
    // rendered source type (previously it collapsed the whole union to `any`).
    let messages = non_strict_2322_messages(
        "function route(n: number, s: string, flag: boolean) {\n\
         \x20   if (flag) return n;\n\
         \x20   if (n === 1) return s;\n\
         \x20   return null;\n\
         }\n\
         var ok: boolean = route(0, \"x\", true);",
    );
    assert_eq!(
        messages.len(),
        1,
        "expected exactly one TS2322: {messages:?}"
    );
    assert!(
        !messages[0].contains("null"),
        "`null` must be dropped from the inferred return union: {}",
        messages[0]
    );
}

// ---------------------------------------------------------------------------
// Positive controls: a SOLE nullish return is a widening source and still
// infers `any`, so these assignments stay clean.
// ---------------------------------------------------------------------------

#[test]
fn sole_null_return_still_widens_to_any() {
    // `function f() { return null; }` → `any`; `string` assignment is fine.
    assert!(
        !non_strict_2322_present(
            "function emptySlot() { return null; }\n\
             var handle: string = emptySlot();",
        ),
        "a sole `return null` must still widen to `any`",
    );
}

#[test]
fn sole_undefined_return_still_widens_to_any() {
    assert!(
        !non_strict_2322_present(
            "function blank() { return undefined; }\n\
             var handle: number = blank();",
        ),
        "a sole `return undefined` must still widen to `any`",
    );
}

#[test]
fn two_null_returns_still_widen_to_any() {
    // Every contribution is a widening-source nullish → `any`.
    assert!(
        !non_strict_2322_present(
            "function twoWays(flag: boolean) { if (flag) return null; return null; }\n\
             var handle: string = twoWays(true);",
        ),
        "an all-null return must still widen to `any`",
    );
}

#[test]
fn nested_undefined_array_return_still_widens_in_place() {
    // `return [undefined]` → `any[]` (nested-composite widening is untouched by
    // the top-level reduction).
    assert!(
        !non_strict_2322_present(
            "function makeRow() { return [undefined]; }\n\
             var row: string[] = makeRow();",
        ),
        "a nested nullish leaf must still widen to `any[]`",
    );
}

// ---------------------------------------------------------------------------
// Negative control: strict mode keeps every nullish member, so the reduction
// must not fire.
// ---------------------------------------------------------------------------

#[test]
fn strict_mode_keeps_the_null_member() {
    // Under `--strict`, `number | null` survives and the `string` assignment is
    // rejected with the union rendered intact.
    let codes = check_source_strict_codes(
        "function pickCount() { if (1) return 1; return null; }\n\
         var label: string = pickCount();",
    );
    assert!(
        codes.contains(&2322),
        "strict mode must keep `number | null` and report TS2322: {codes:?}",
    );
}

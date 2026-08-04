//! Tests for the non-strict (`strictNullChecks: false`) `null`/`undefined` →
//! `any` widening of a **block-bodied** function's inferred return type.
//!
//! tsc gives the `null` keyword and the global `undefined` the widening flavour
//! (`nullWideningType` / `undefinedWideningType`), propagates it through
//! array/object-literal construction, and `getWidenedType` maps those leaves to
//! `any` at every inferred position. So with `strictNullChecks` off:
//!
//! ```ts
//! function f() { return [undefined]; }  // () => any[]
//! function g() { return { p: null }; }  // () => { p: any }
//! ```
//!
//! tsz applied that widening only at the *expression-bodied* return seam
//! (`maybe_widen_return_contribution`), so the block-bodied twin kept
//! `undefined[]` / `{ p: undefined }` and produced a false `TS2322` on any later
//! assignment into the binding.
//!
//! The widening flavour is a property of the *expression*, not of the type: a
//! leaf that is merely typed `undefined` carries none of it, so
//! `declare var q: undefined; function f() { return [q]; }` keeps `undefined[]`
//! in tsc and must keep it here. Every case below is pinned against
//! `typescript@7.0.2`.
//!
//! Binder names are varied across cases per the anti-hardcoding contract.

use crate::test_utils::{check_source_non_strict_codes, check_source_strict_codes};

fn non_strict_2322(src: &str) -> Vec<u32> {
    check_source_non_strict_codes(src)
        .into_iter()
        .filter(|&code| code == 2322)
        .collect()
}

#[test]
fn block_bodied_undefined_array_return_widens_to_any_array() {
    // `() => any[]`, so a later `string[]` assignment is fine. Unwidened
    // (`undefined[]`) this is a false TS2322.
    assert_eq!(
        non_strict_2322(
            "function makeRow() { return [undefined]; }\n\
             var row = makeRow();\n\
             row = [\"\"];",
        ),
        Vec::<u32>::new(),
    );
}

#[test]
fn block_bodied_null_array_return_widens_to_any_array() {
    assert_eq!(
        non_strict_2322(
            "function buildSlots() { return [null]; }\n\
             var slots = buildSlots();\n\
             slots = [\"\"];",
        ),
        Vec::<u32>::new(),
    );
}

#[test]
fn block_bodied_nested_undefined_array_return_widens_every_level() {
    assert_eq!(
        non_strict_2322(
            "function grid() { return [[undefined]]; }\n\
             var cells = grid();\n\
             cells = [[\"\"]];",
        ),
        Vec::<u32>::new(),
    );
}

#[test]
fn block_bodied_object_literal_nullish_property_return_widens_to_any() {
    assert_eq!(
        non_strict_2322(
            "function makeBox() { return { payload: undefined }; }\n\
             var box1 = makeBox();\n\
             box1 = { payload: \"\" };",
        ),
        Vec::<u32>::new(),
    );
}

#[test]
fn block_bodied_undefined_tuple_in_object_property_return_widens() {
    assert_eq!(
        non_strict_2322(
            "function wrap() { return { items: [null, undefined] }; }\n\
             var wrapped = wrap();\n\
             wrapped = { items: [\"a\", \"b\"] };",
        ),
        Vec::<u32>::new(),
    );
}

#[test]
fn expression_bodied_and_block_bodied_returns_agree() {
    // The expression-bodied seam already widened; the block-bodied twin now
    // reaches the same inferred return type rather than diverging by body form.
    let arrow = non_strict_2322(
        "var pick = () => [undefined];\n\
         var picked = pick();\n\
         picked = [\"\"];",
    );
    let block = non_strict_2322(
        "function choose() { return [undefined]; }\n\
         var chosen = choose();\n\
         chosen = [\"\"];",
    );
    assert_eq!(arrow, block);
    assert_eq!(block, Vec::<u32>::new());
}

#[test]
fn recursive_function_expression_undefined_return_widens() {
    // `wideningTuples2.ts`'s shape: a self-referential function expression whose
    // only return is `[undefined]`. tsc reports nothing here.
    assert_eq!(
        non_strict_2322(
            "var head: () => [any] = function walk() {\n\
             \x20   let step = walk();\n\
             \x20   step = [\"\"];\n\
             \x20   return [undefined];\n\
             };",
        ),
        Vec::<u32>::new(),
    );
}

// ---------------------------------------------------------------------------
// Negative side: a leaf that is only *typed* `undefined` carries no widening
// flavour, so the contribution must stay unwidened and keep reporting.
// ---------------------------------------------------------------------------

#[test]
fn declared_undefined_element_return_keeps_undefined_array() {
    // `declare var seed: undefined` is not a widening source: tsc infers
    // `() => undefined[]` and reports TS2322 on the `string[]` assignment.
    assert_eq!(
        non_strict_2322(
            "declare var seed: undefined;\n\
             function collect() { return [seed]; }\n\
             var collected = collect();\n\
             collected = [\"\"];",
        ),
        vec![2322],
    );
}

#[test]
fn declared_undefined_property_return_keeps_undefined_property() {
    assert_eq!(
        non_strict_2322(
            "declare var blank: undefined;\n\
             function shape() { return { slot: blank }; }\n\
             var shaped = shape();\n\
             shaped = { slot: \"\" };",
        ),
        vec![2322],
    );
}

#[test]
fn annotated_return_type_is_not_widened() {
    // An explicit `undefined[]` annotation is a contextual return type, so the
    // contribution is not widenable at all and the target keeps `undefined[]`.
    assert_eq!(
        non_strict_2322(
            "function fixed(): undefined[] { return [undefined]; }\n\
             var held = fixed();\n\
             held = [\"\"];",
        ),
        vec![2322],
    );
}

#[test]
fn local_shadowing_undefined_is_not_a_widening_source() {
    // A user binding spelled `undefined` shadows the global sentinel; it is a
    // plain reference, so nothing widens and the assignment still reports.
    assert_eq!(
        non_strict_2322(
            "function scope(undefined: undefined) {\n\
             \x20   return [undefined];\n\
             }\n\
             var scoped = scope(void 0);\n\
             scoped = [\"\"];",
        ),
        vec![2322],
    );
}

#[test]
fn non_nullish_literal_returns_are_unaffected() {
    // The nullish pass is a no-op when there is no nullish leaf: the ordinary
    // literal widening still applies (`() => number[]`), so this is clean.
    assert_eq!(
        non_strict_2322(
            "function counts() { return [1, 2, 3]; }\n\
             var tally = counts();\n\
             tally = [4];",
        ),
        Vec::<u32>::new(),
    );
}

#[test]
fn strict_mode_keeps_undefined_array_return_unwidened() {
    // The whole rule is `strictNullChecks: false`-only. Under strict mode the
    // inferred return stays `undefined[]` and the assignment still reports.
    let codes: Vec<u32> = check_source_strict_codes(
        "function makeRow() { return [undefined]; }\n\
         var row = makeRow();\n\
         row = [\"\"];",
    )
    .into_iter()
    .filter(|&code| code == 2322)
    .collect();
    assert_eq!(codes, vec![2322]);
}

#[test]
fn elided_array_holes_in_a_return_are_widening_sources() {
    // `return [,,]` — the user wrote no value, so tsc gives each hole
    // `undefinedWideningType` and infers `() => any[]`.
    assert_eq!(
        non_strict_2322(
            "function sparse() { return [,,]; }\n\
             var holes = sparse();\n\
             holes = [\"\"];",
        ),
        Vec::<u32>::new(),
    );
}

#[test]
fn one_declared_undefined_sibling_makes_an_elided_literal_non_widening() {
    // A hole is permissive on its own and decisive nowhere: the declared
    // `undefined` element still pins the whole literal to `undefined[]`.
    assert_eq!(
        non_strict_2322(
            "declare var gap: undefined;\n\
             function mixed() { return [, gap]; }\n\
             var mixture = mixed();\n\
             mixture = [\"\"];",
        ),
        vec![2322],
    );
}

#[test]
fn a_declared_nullish_array_leaf_is_not_a_widening_source() {
    // `g()` returns a declared `undefined[]`; the array literal built around it
    // carries no widening flavour, so tsc keeps `undefined[][]`.
    assert_eq!(
        non_strict_2322(
            "declare function supply(): undefined[];\n\
             function nest() { return [supply()]; }\n\
             var nested = nest();\n\
             nested = [[\"\"]];",
        ),
        vec![2322],
    );
}

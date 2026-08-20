//! Contextual union discrimination for a fresh object literal follows tsc's
//! `discriminateTypeByDiscriminableItems` include-state walk
//! (`discriminateContextualTypeByObjectMembers`).
//!
//! Structural rule: a union member is eliminated by a written unit
//! discriminant only when that discriminant MATCHED some member; a written
//! value that matches no member eliminates nothing (tsc's `Maybe` state
//! reverts to included). Members lacking the property stay included, and
//! primitive/nullish members are pre-excluded from the discriminated result.
//!
//! Before this rule, a literal matching NO member completely (`{ p: 1, q: 8 }`
//! against `{ p: 1; q: 4 } | { p: 2; q: 8 } | number[]`) collapsed the
//! contextual union to the vacuously-matching array arm: every per-property
//! contextual type vanished, the literal's own property types widened
//! (`p: number`), the solver's best-member selection lost its unit
//! discriminants, and the diagnostic chain reported the wrong property with a
//! widened source against a cross-arm union target.
//!
//! All expectations oracle-pinned against pinned typescript@7.0.2
//! (`scripts/conformance/oracle.sh --strict`, byte-identical CLI output).
//! Binder and property names vary across cases so the behavior is proven
//! structural.

use crate::CheckerOptions;
use crate::diagnostics::Diagnostic;
use crate::test_utils::{check_source_with_libs, load_default_lib_files};

// Lib-backed harness: the witnesses' array arms (`number[]`) need the lib
// `Array` declaration — in the no-lib harness they collapse and the union
// shapes under test never form (standing board gotcha).
fn strict_diags(source: &str) -> Vec<Diagnostic> {
    check_source_with_libs(
        source,
        "case.ts",
        CheckerOptions {
            strict: true,
            ..Default::default()
        },
        &load_default_lib_files(),
    )
}

fn diags_with_code(source: &str, code: u32) -> Vec<Diagnostic> {
    strict_diags(source)
        .into_iter()
        .filter(|d| d.code == code)
        .collect()
}

/// Head text plus the flattened `(depth, text)` elaboration chain of the
/// single TS`code` diagnostic the fixture produces.
fn head_and_chain(source: &str, code: u32) -> (String, Vec<(u8, String)>) {
    let mut diags = diags_with_code(source, code);
    assert_eq!(
        diags.len(),
        1,
        "expected exactly one TS{code} for `{source}`, got: {diags:?}"
    );
    let diag = diags.remove(0);
    let chain = diag
        .related_information
        .iter()
        .map(|info| (info.depth, info.message_text.clone()))
        .collect();
    (diag.message_text, chain)
}

fn assert_no_diags(source: &str) {
    let diags = strict_diags(source);
    assert!(
        diags.is_empty(),
        "expected no diagnostics for `{source}`, got: {diags:?}"
    );
}

fn assert_property_chain(source: &str, code: u32, prop: &str, leaf: &str) {
    let (head, chain) = head_and_chain(source, code);
    assert_eq!(
        chain,
        vec![
            (0, format!("Types of property '{prop}' are incompatible.")),
            (1, leaf.to_string()),
        ],
        "expected the '{prop}' fold beneath the head `{head}` for `{source}`"
    );
}

// ---------------------------------------------------------------------------
// Array-arm mixed union: the discriminant-matched object arm owns the
// per-property elaboration target; the source property keeps its literal.
// ---------------------------------------------------------------------------

#[test]
fn assignment_discriminant_matched_arm_owns_property_target() {
    // tsc: p: 1 selects the first arm; q fails there with the unwidened
    // literal source. Previously: `p`: 'number' vs '1 | 2'.
    assert_property_chain(
        r#"
type U = { p: 1; q: 4 } | { p: 2; q: 8 };
const x: U | number[] = { p: 1, q: 8 };
"#,
        2322,
        "q",
        "Type '8' is not assignable to type '4'.",
    );
}

#[test]
fn generic_call_argument_discriminant_matched_arm_owns_property_target() {
    // The #17770 residual-1 shape.
    assert_property_chain(
        r#"
type U = { p: 1; q: 4 } | { p: 2; q: 8 };
declare function both<T>(t: T, u: U | T[]): void;
both(0, { p: 1, q: 8 });
"#,
        2345,
        "q",
        "Type '8' is not assignable to type '4'.",
    );
}

#[test]
fn renamed_binders_string_discriminant_generic_call() {
    assert_property_chain(
        r#"
type Zulu = { kind: "a"; val: 4 } | { kind: "b"; val: 8 };
declare function grab<Elem>(seed: Elem, sink: Zulu | Elem[]): void;
grab("x", { kind: "a", val: 8 });
"#,
        2345,
        "val",
        "Type '8' is not assignable to type '4'.",
    );
}

#[test]
fn no_full_match_keeps_declaring_arms_and_reports_first_property() {
    // p: 3 matches NO arm — tsc's unmatched discriminator eliminates nothing;
    // q: 8 then discriminates to the second arm, and the fold reports the
    // first-declared failing property with its literal source.
    assert_property_chain(
        r#"
type U = { p: 1; q: 4 } | { p: 2; q: 8 };
declare function both<T>(t: T, u: U | T[]): void;
both(0, { p: 3, q: 8 });
"#,
        2345,
        "p",
        "Type '3' is not assignable to type '2'.",
    );
}

#[test]
fn unmatched_first_discriminant_keeps_literal_leaf_source() {
    // p: 1 matches neither arm's 5/9; q: 8 selects the second arm. The leaf
    // source must NOT widen to `number`.
    assert_property_chain(
        r#"
const x: { p: 5; q: 4 } | { p: 9; q: 8 } | number[] = { p: 1, q: 8 };
"#,
        2322,
        "p",
        "Type '1' is not assignable to type '9'.",
    );
}

#[test]
fn string_discriminant_matching_arm_survives_other_discriminant_failure() {
    // p: "a" matches the first arm; q: 8 matches no still-included arm, so
    // its failures revert and the first arm survives.
    assert_property_chain(
        r#"
const x: { p: "a"; q: 4 } | { p: "b"; q: 8 } | number[] = { p: "a", q: 8 };
"#,
        2322,
        "q",
        "Type '8' is not assignable to type '4'.",
    );
}

#[test]
fn null_discriminant_leaf_keeps_literal_source() {
    // The discriminant match already worked here (null stays unit); the leaf
    // source previously widened to `number`.
    assert_property_chain(
        r#"
const x: { p: null; q: 4 } | { p: 2; q: 8 } | number[] = { p: null, q: 8 };
"#,
        2322,
        "q",
        "Type '8' is not assignable to type '4'.",
    );
}

#[test]
fn primitive_arm_is_pre_excluded_and_single_arm_anchors_inner() {
    // A primitive arm is pre-excluded from the discriminated result, leaving
    // a SINGLE member — tsc anchors the bare leaf at the property node
    // (`getBestMatchIndexedAccessTypeOrUndefined` over the single member).
    let (head, chain) = head_and_chain(
        r#"
const x: { p: 1; q: 4 } | { p: 2; q: 8 } | string = { p: 1, q: 8 };
"#,
        2322,
    );
    assert_eq!(
        head, "Type '8' is not assignable to type '4'.",
        "primitive-arm union anchors the inner property leaf"
    );
    assert!(
        chain.is_empty(),
        "inner anchor carries no elaboration chain, got: {chain:?}"
    );
}

#[test]
fn optional_undefined_discriminant_selects_optional_arm() {
    // A written `undefined` matches the optional `on?: false` arm; the fold's
    // compared read type of the optional slot carries `| undefined`, so the
    // genuinely failing property reports.
    assert_property_chain(
        r#"
type Q = { on: true; cb: string } | { on?: false; cb: number } | number[];
const q: Q = { on: undefined, cb: "s" };
"#,
        2322,
        "cb",
        "Type 'string' is not assignable to type 'number'.",
    );
}

// ---------------------------------------------------------------------------
// Object-arm mixed union: no TS2353 excess-property misroute against the
// vacuously-matching arm.
// ---------------------------------------------------------------------------

#[test]
#[ignore = "pre-existing excess-property arm-selection residual (unchanged by the discriminant-include fix): the TS2353 check reports 'p' against the non-declaring '{ z: string }' arm where tsc emits the outer TS2322 with the matched arm's q chain. Owner: object-literal excess-property routing for union targets — the same owner as the #17770 TS2353-vs-TS2345 ignored residual in ts2345_generic_call_concrete_alias_parameter_display_tests."]
fn extra_object_arm_reports_matched_arm_not_excess_against_vacuous_arm() {
    let source = r#"
const x: { p: 1; q: 4 } | { p: 2; q: 8 } | { z: string } = { p: 1, q: 8 };
"#;
    let excess = diags_with_code(source, 2353);
    assert!(
        excess.is_empty(),
        "no excess-property misroute against the non-declaring arm, got: {excess:?}"
    );
    assert_property_chain(source, 2322, "q", "Type '8' is not assignable to type '4'.");
}

// ---------------------------------------------------------------------------
// Controls: shapes that already matched tsc must stay byte-identical.
// ---------------------------------------------------------------------------

#[test]
fn control_pure_object_union_fold_unchanged() {
    assert_property_chain(
        r#"
const x: { p: 1; q: 4 } | { p: 2; q: 8 } = { p: 1, q: 8 };
"#,
        2322,
        "q",
        "Type '8' is not assignable to type '4'.",
    );
}

#[test]
fn control_non_generic_call_argument_unchanged() {
    assert_property_chain(
        r#"
type U = { p: 1; q: 4 } | { p: 2; q: 8 };
declare function f(u: U | number[]): void;
f({ p: 1, q: 8 });
"#,
        2345,
        "q",
        "Type '8' is not assignable to type '4'.",
    );
}

#[test]
fn control_alias_wrapped_mixed_union_unchanged() {
    assert_property_chain(
        r#"
type U = { p: 1; q: 4 } | { p: 2; q: 8 };
type W = U | number[];
declare function f(u: W): void;
f({ p: 1, q: 8 });
"#,
        2345,
        "q",
        "Type '8' is not assignable to type '4'.",
    );
}

#[test]
fn control_single_arm_discriminant_match_head_and_chain() {
    // tsc widens the non-matching property against a non-literal context in
    // the HEAD (`v: number`) and drills the discriminant-matched arm.
    let (head, chain) = head_and_chain(
        r#"
type R = { k: "a"; v: string } | { k: "b"; v: number } | string[];
const r: R = { k: "a", v: 1 };
"#,
        2322,
    );
    assert!(
        head.contains("Type '{ k: \"a\"; v: number; }' is not assignable to type 'R'."),
        "head keeps the widened non-matching property, got: {head}"
    );
    assert_eq!(
        chain,
        vec![
            (0, "Types of property 'v' are incompatible.".to_string()),
            (
                1,
                "Type 'number' is not assignable to type 'string'.".to_string()
            ),
        ],
    );
}

#[test]
fn positives_stay_clean() {
    assert_no_diags(
        r#"
const a: { p: 1 } | number[] = { p: 1 };
const b: { p: 1; q: 4 } | { p: 2; q: 8 } | number[] = { p: 2, q: 8 };
const c: { p: 1; q: 4 } | { p: 2; q: 8 } | number[] = [1, 2];
declare function both<T>(t: T, u: { p: 1; q: 4 } | { p: 2; q: 8 } | T[]): void;
both(0, { p: 1, q: 4 });
"#,
    );
}

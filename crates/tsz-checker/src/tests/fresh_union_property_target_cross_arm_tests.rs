//! Per-property elaboration target for a fresh object literal against a union
//! target (`tsc`'s `getBestMatchIndexedAccessTypeOrUndefined`).
//!
//! Structural rule: when a fresh object-literal expression elaborates against
//! a union target and EVERY union constituent exposes the failing key, the
//! per-property target — used for the property check, the leaf display, and
//! the nested-literal recursion alike — is the indexed access over the FULL
//! union (the union of the constituents' property types), not the
//! discriminant-narrowed member's property type. Only when some constituent
//! lacks the key does the best-matching (discriminant-matched) member own the
//! target. Because the check itself runs against the cross-arm union, a
//! nested property value that satisfies it produces NO inner anchor: the
//! outer head reports with the folded property chain (`The types of 'v.x'
//! are incompatible between these types.`), matching `tsc`.
//!
//! All expectations oracle-pinned against pinned typescript@7.0.2
//! (`scripts/conformance/oracle.sh --strict`). Binder and property names are
//! varied across cases so the behavior is proven structural.

use crate::diagnostics::Diagnostic;
use crate::test_utils::check_source_strict;

fn diags_with_code(source: &str, code: u32) -> Vec<Diagnostic> {
    check_source_strict(source)
        .into_iter()
        .filter(|d| d.code == code)
        .collect()
}

fn single_diag(source: &str, code: u32) -> Diagnostic {
    let mut diags = diags_with_code(source, code);
    assert_eq!(
        diags.len(),
        1,
        "expected exactly one TS{code} for `{source}`, got: {diags:?}"
    );
    diags.remove(0)
}

#[test]
fn nested_leaf_reports_cross_arm_property_union() {
    // tsc: w.ts(2,32): error TS2322: Type '2' is not assignable to type '1 | 9'.
    let diag = single_diag(
        r#"
type U = { kind: "a"; v: { x: 1 } } | { kind: "b"; v: { x: 9 } };
const u: U = { kind: "a", v: { x: 2 } };
"#,
        2322,
    );
    assert_eq!(
        diag.message_text, "Type '2' is not assignable to type '1 | 9'.",
        "nested leaf must report the cross-arm property union"
    );
}

#[test]
fn deep_nested_leaf_reports_cross_arm_property_union() {
    // Three levels deep: the cross-arm union derivation must survive each
    // nested-literal recursion step.
    let diag = single_diag(
        r#"
type D = { tag: "l"; w: { m: { z: 1 } } } | { tag: "r"; w: { m: { z: 9 } } };
const d: D = { tag: "l", w: { m: { z: 5 } } };
"#,
        2322,
    );
    assert_eq!(
        diag.message_text, "Type '5' is not assignable to type '1 | 9'.",
        "deeply nested leaf must report the cross-arm property union"
    );
}

#[test]
fn string_literal_nested_leaf_reports_cross_arm_property_union() {
    let diag = single_diag(
        r#"
type Shape = { face: "circle"; dims: { r: "big" } } | { face: "square"; dims: { r: "small" } };
const s: Shape = { face: "circle", dims: { r: "huge" } };
"#,
        2322,
    );
    assert_eq!(
        diag.message_text, r#"Type '"huge"' is not assignable to type '"big" | "small"'."#,
        "string-literal nested leaf must report the cross-arm property union"
    );
}

#[test]
fn satisfies_nested_leaf_reports_cross_arm_property_union() {
    let diag = single_diag(
        r#"
type Cfg = { mode: "x"; opts: { depth: 1 } } | { mode: "y"; opts: { depth: 9 } };
const c = { mode: "x", opts: { depth: 2 } } satisfies Cfg;
"#,
        2322,
    );
    assert_eq!(
        diag.message_text, "Type '2' is not assignable to type '1 | 9'.",
        "satisfies-position nested leaf must report the cross-arm property union"
    );
}

#[test]
fn argument_nested_leaf_reports_cross_arm_property_union() {
    let diag = single_diag(
        r#"
type Evt = { op: "add"; data: { n: 1 } } | { op: "del"; data: { n: 9 } };
declare function handle(e: Evt): void;
handle({ op: "add", data: { n: 2 } });
"#,
        2322,
    );
    assert_eq!(
        diag.message_text, "Type '2' is not assignable to type '1 | 9'.",
        "argument-position nested leaf must report the cross-arm property union"
    );
}

#[test]
fn inner_value_matching_other_arm_reports_outer_fold_not_inner_anchor() {
    // The nested value satisfies the cross-arm union (`9` <: `1 | 9`), so no
    // property-level anchor exists; tsc reports the outer head with the
    // path-compressed fold. tsc:
    //   w.ts(2,7): error TS2322: Type '{ kind: "a"; v: { x: 9; }; }' is not assignable to type 'U'.
    //     The types of 'v.x' are incompatible between these types.
    //       Type '9' is not assignable to type '1'.
    let source = r#"
type U = { kind: "a"; v: { x: 1 } } | { kind: "b"; v: { x: 9 } };
const u: U = { kind: "a", v: { x: 9 } };
"#;
    let diag = single_diag(source, 2322);
    assert!(
        diag.message_text.contains("is not assignable to type 'U'"),
        "head must be the outer union relation, got: {}",
        diag.message_text
    );
    let related = &diag.related_information;
    assert!(
        related
            .iter()
            .any(|r| r.message_text == "The types of 'v.x' are incompatible between these types."),
        "fold must path-compress the nested property chain, got: {related:?}"
    );
    assert!(
        related
            .iter()
            .any(|r| r.message_text == "Type '9' is not assignable to type '1'."),
        "fold leaf must report against the discriminant-matched arm, got: {related:?}"
    );
}

#[test]
fn argument_inner_value_matching_other_arm_reports_ts2345_outer_fold() {
    let source = r#"
type Msg = { ch: "up"; body: { code: 1 } } | { ch: "dn"; body: { code: 9 } };
declare function send(m: Msg): void;
send({ ch: "up", body: { code: 9 } });
"#;
    let diag = single_diag(source, 2345);
    assert!(
        diag.message_text
            .contains("is not assignable to parameter of type 'Msg'"),
        "head must be the argument-level relation, got: {}",
        diag.message_text
    );
    let related = &diag.related_information;
    assert!(
        related
            .iter()
            .any(|r| r.message_text
                == "The types of 'body.code' are incompatible between these types."),
        "fold must path-compress the nested property chain, got: {related:?}"
    );
    assert!(
        related
            .iter()
            .any(|r| r.message_text == "Type '9' is not assignable to type '1'."),
        "fold leaf must report against the discriminant-matched arm, got: {related:?}"
    );
    assert!(
        diags_with_code(source, 2322).is_empty(),
        "no inner-anchored TS2322 may remain once the value satisfies the cross-arm union"
    );
}

#[test]
fn arm_lacking_key_keeps_best_match_member_target() {
    // Arm `{ kind: "b" }` lacks `v`, so the indexed access over the union is
    // undefined and the discriminant-matched member owns the target. tsc:
    // Type '2' is not assignable to type '1'.
    let diag = single_diag(
        r#"
type W = { kind: "a"; v: { x: 1 } } | { kind: "b" };
const w: W = { kind: "a", v: { x: 2 } };
"#,
        2322,
    );
    assert_eq!(
        diag.message_text, "Type '2' is not assignable to type '1'.",
        "a constituent lacking the key must keep the best-match member target"
    );
}

#[test]
fn three_arm_union_with_keyless_arm_keeps_best_match_member_target() {
    // Two arms expose `v`, a third lacks it — still the best-match member.
    let diag = single_diag(
        r#"
type V = { kind: "a"; v: { x: 1 } } | { kind: "b"; v: { x: 9 } } | { kind: "c" };
const q: V = { kind: "a", v: { x: 2 } };
"#,
        2322,
    );
    assert_eq!(
        diag.message_text, "Type '2' is not assignable to type '1'.",
        "a keyless third arm must keep the best-match member target"
    );
}

#[test]
fn nullish_arm_keeps_best_match_member_target() {
    // `undefined` is a union constituent that lacks every key, so tsc's
    // indexed access over the full union is undefined and the
    // discriminant-matched member owns the target — the nullish arm must not
    // be stripped before the every-constituent check. tsc:
    // Type '2' is not assignable to type '1'.
    let diag = single_diag(
        r#"
type N2 = { kind: "a"; v: { x: 1 } } | { kind: "b"; v: { x: 9 } } | undefined;
const nb: N2 = { kind: "a", v: { x: 2 } };
"#,
        2322,
    );
    assert_eq!(
        diag.message_text, "Type '2' is not assignable to type '1'.",
        "a nullish arm must keep the best-match member target"
    );
}

#[test]
fn primitive_arm_keeps_best_match_member_target() {
    let diag = single_diag(
        r#"
type P = string | { v: { x: 1 } };
const p: P = { v: { x: 2 } };
"#,
        2322,
    );
    assert_eq!(
        diag.message_text, "Type '2' is not assignable to type '1'.",
        "a primitive arm must keep the best-match member target"
    );
}

#[test]
fn flat_leaf_still_reports_cross_arm_property_union() {
    // Negative control: the flat (non-nested) leaf already used the cross-arm
    // union before this change and must keep it.
    let diag = single_diag(
        r#"
type F = { kind: "a"; n: 1 } | { kind: "b"; n: 9 };
const fv: F = { kind: "a", n: 2 };
"#,
        2322,
    );
    assert_eq!(
        diag.message_text, "Type '2' is not assignable to type '1 | 9'.",
        "flat leaf must keep the cross-arm property union"
    );
}

#[test]
fn flat_value_matching_other_arm_still_reports_outer_fold() {
    // Negative control: flat analog of the outer-fold shape, already at
    // parity before this change.
    let diag = single_diag(
        r#"
type F2 = { kind: "a"; n: 1 } | { kind: "b"; n: 9 };
const f2: F2 = { kind: "a", n: 9 };
"#,
        2322,
    );
    assert!(
        diag.message_text.contains("is not assignable to type 'F2'"),
        "head must stay the outer union relation, got: {}",
        diag.message_text
    );
    assert!(
        diag.related_information
            .iter()
            .any(|r| r.message_text == "Types of property 'n' are incompatible."),
        "single-level fold keeps the property header, got: {:?}",
        diag.related_information
    );
}

#[test]
fn three_arm_flat_leaf_uses_best_match_member() {
    // A failing unit-literal sibling (`n: 2`) matches no arm; tsc's
    // `discriminateTypeByDiscriminableItems` reverts that discriminator and
    // keeps the `kind: "a"` match, so the keyless third arm forces the
    // best-match member (`{ kind: "a"; n: 1 }`) to own the target. tsc:
    // Type '2' is not assignable to type '1'.
    let diag = single_diag(
        r#"
type G = { kind: "a"; n: 1 } | { kind: "b"; n: 9 } | { kind: "c" };
const g: G = { kind: "a", n: 2 };
"#,
        2322,
    );
    assert_eq!(diag.message_text, "Type '2' is not assignable to type '1'.");
}

#[test]
fn no_arm_unit_literal_is_ignored_with_renamed_binders_and_string_literals() {
    // Same structural condition, renamed binders and string-literal units.
    // tsc 7.0.2: Type '"m"' is not assignable to type '"s"'.
    let diag = single_diag(
        r#"
type Cmd = { op: "put"; sz: "s" } | { op: "get"; sz: "l" } | { op: "del" };
const c: Cmd = { op: "put", sz: "m" };
"#,
        2322,
    );
    assert_eq!(
        diag.message_text, r#"Type '"m"' is not assignable to type '"s"'."#,
        "renamed-binder string-literal form must take the best-match member"
    );
}

#[test]
fn no_arm_unit_literal_is_ignored_regardless_of_property_order() {
    // The failing no-arm literal written FIRST: the per-discriminator revert
    // must be order-stable (tsc processes discriminators sequentially and
    // reverts each unmatched one independently). tsc 7.0.2:
    // Type '2' is not assignable to type '1'.
    let diag = single_diag(
        r#"
type G = { kind: "a"; n: 1 } | { kind: "b"; n: 9 } | { kind: "c" };
const g: G = { n: 2, kind: "a" };
"#,
        2322,
    );
    assert_eq!(diag.message_text, "Type '2' is not assignable to type '1'.");
}

#[test]
fn surviving_discriminant_still_narrows_contextual_typing_for_callbacks() {
    // The narrowing that survives the reverted no-arm discriminator is real
    // contextual narrowing, not only display: the callback parameter takes the
    // matched arm's signature (`x: string`, so `x.length` is legal and no
    // TS7006 fires). tsc 7.0.2 reports exactly one diagnostic:
    // Type '2' is not assignable to type '1'.
    let source = r#"
type CB = { kind: "a"; f: (x: string) => void; n: 1 } | { kind: "b"; f: (x: number) => void; n: 9 } | { kind: "c" };
const cb: CB = { kind: "a", n: 2, f: x => x.length };
"#;
    let diag = single_diag(source, 2322);
    assert_eq!(diag.message_text, "Type '2' is not assignable to type '1'.");
    assert!(
        diags_with_code(source, 7006).is_empty(),
        "the surviving `kind` discriminant must contextually type the callback parameter"
    );
}

/// Pinned residual (oracle-verified divergence, deliberately out of scope):
/// when EVERY discriminator matches no arm (`kind: "zz"`, `n: 2`), tsc's
/// `getBestMatchingType` falls through to `findMostOverlappyType`, whose
/// last-best-wins scan selects the LAST arm sharing the most keys — the `n`
/// leaf renders `'9'` (arm `"b"`). tsz has no most-overlappy fallback and
/// renders the key-bearing-arm union. The `kind` head (`'"zz"'` vs
/// `'"a" | "b" | "c"'`) already agrees.
#[test]
#[ignore = "tsz renders '1 | 9' where tsc 7.0.2 renders '9' — findMostOverlappyType fallback not modeled when every discriminator fails"]
fn all_discriminators_failing_uses_most_overlappy_member() {
    let source = r#"
type G = { kind: "a"; n: 1 } | { kind: "b"; n: 9 } | { kind: "c" };
const g: G = { kind: "zz", n: 2 };
"#;
    let diags = diags_with_code(source, 2322);
    assert!(
        diags
            .iter()
            .any(|d| d.message_text == "Type '2' is not assignable to type '9'."),
        "n leaf must report the most-overlappy member's property type, got: {diags:?}"
    );
}

/// The outer fold's HEAD preserves a nested object property's literal
/// (`v: { x: 9; }`) like tsc: the nested render recurses with the target's
/// own per-property type as its contextual target (#17782; adjacent matrix in
/// `fresh_union_fold_head_nested_literal_display_tests.rs`).
#[test]
fn outer_fold_head_preserves_nested_literal_display() {
    let diag = single_diag(
        r#"
type U = { kind: "a"; v: { x: 1 } } | { kind: "b"; v: { x: 9 } };
const u: U = { kind: "a", v: { x: 9 } };
"#,
        2322,
    );
    assert_eq!(
        diag.message_text,
        r#"Type '{ kind: "a"; v: { x: 9; }; }' is not assignable to type 'U'."#
    );
}

/// A union arm that does not expose the discriminant key at all stays
/// included during discrimination (tsc's `discriminateTypeByDiscriminableItems`
/// only touches `include[i]` when `getTypeOfPropertyOrIndexSignatureOfType`
/// returns a type), so the discriminated result stays a multi-member union
/// (`{ kind: "a"; n: 1 } | { other?: string }`), the keyless arm makes the
/// per-property indexed access undefined, and the whole-object head reports —
/// never a bare drill-in leaf, and never a false TS2353 pinning the keyless
/// arm. Oracle (typescript@7.0.2):
/// `Type '{ kind: "a"; n: 2; }' is not assignable to type 'G'.`
///
/// Regression fence for the #17802 family: the keyless-arm shape had no
/// coverage, so the drift #17789 introduced (absent-key members eliminated,
/// TS2345/TS2322 heads rewritten into per-property leaves) was invisible to
/// every suite until `main` went red. Requires both the #17798 discriminate
/// semantics and the #17801 excess-property revert semantics; either
/// regressing flips this fence.
#[test]
fn keyless_object_arm_survives_discrimination_and_blocks_per_property_drill() {
    let source = r#"
type G = { kind: "a"; n: 1 } | { kind: "b"; n: 9 } | { other?: string };
const g: G = { kind: "a", n: 2 };
"#;
    let diag = single_diag(source, 2322);
    assert!(
        diag.message_text
            .starts_with(r#"Type '{ kind: "a"; n: 2; }' is not assignable to type 'G'."#),
        "keyless arm must block the per-property drill, got: {:?}",
        diag.message_text
    );
    assert!(
        diags_with_code(source, 2353).is_empty(),
        "the keyless arm must not be pinned by a false excess-property TS2353"
    );
}

/// Same structural condition, renamed binders and different unit values.
/// Oracle (typescript@7.0.2):
/// `Type '{ tag: "x"; len: 9; }' is not assignable to type 'Wire'.`
#[test]
fn keyless_object_arm_blocks_drill_with_renamed_binders() {
    let source = r#"
type Wire = { tag: "x"; len: 3 } | { tag: "y"; len: 7 } | { fallback?: boolean };
const w: Wire = { tag: "x", len: 9 };
"#;
    let diag = single_diag(source, 2322);
    assert!(
        diag.message_text
            .starts_with(r#"Type '{ tag: "x"; len: 9; }' is not assignable to type 'Wire'."#),
        "renamed-binder keyless arm must block the per-property drill, got: {:?}",
        diag.message_text
    );
    assert!(
        diags_with_code(source, 2353).is_empty(),
        "the keyless arm must not be pinned by a false excess-property TS2353"
    );
}

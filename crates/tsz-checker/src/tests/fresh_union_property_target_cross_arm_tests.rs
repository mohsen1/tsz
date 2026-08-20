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

/// Pinned residual (oracle-verified divergence, deliberately out of scope):
/// when a keyless arm forces the best-match fallback but the discriminant
/// narrowing bails (a failing unit-literal sibling matches no arm), the flat
/// leaf still reports the union of the key-bearing arms where tsc reports the
/// best-match member alone.
#[test]
#[ignore = "tsz renders '1 | 9' where tsc 7.0.2 renders '1' — best-match fallback not taken when discriminant narrowing bails on a no-arm unit literal"]
fn three_arm_flat_leaf_uses_best_match_member() {
    let diag = single_diag(
        r#"
type G = { kind: "a"; n: 1 } | { kind: "b"; n: 9 } | { kind: "c" };
const g: G = { kind: "a", n: 2 };
"#,
        2322,
    );
    assert_eq!(diag.message_text, "Type '2' is not assignable to type '1'.");
}

/// Pinned residual (oracle-verified divergence, deliberately out of scope):
/// the outer fold's HEAD widens a nested object property's literal
/// (`v: { x: number; }`) where tsc preserves it (`v: { x: 9; }`) — the
/// fresh-literal display surface preservation is per top-level property and
/// does not recurse into nested object properties.
#[test]
#[ignore = "tsz head renders 'v: { x: number; }' where tsc 7.0.2 renders 'v: { x: 9; }' — nested literal display preservation residual"]
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

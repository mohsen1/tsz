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

/// When EVERY discriminator matches no arm (`kind: "zz"`, `n: 2`), tsc's
/// `getBestMatchingType` falls through to `findMostOverlappyType`, whose
/// last-best-wins scan selects the LAST arm sharing the most keys — the `n`
/// leaf renders `'9'` (arm `"b"`), while the `kind` head (present in every
/// arm) keeps the cross-arm union. tsz mirrors this through the solver's
/// `select_union_target_best_member` when the full-union indexed access is
/// undefined (`unnarrowed_union_object_literal_property_target`).
#[test]
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
    assert!(
        diags.iter().any(
            |d| d.message_text == r#"Type '"zz"' is not assignable to type '"a" | "b" | "c"'."#
        ),
        "the kind head must keep the cross-arm union, got: {diags:?}"
    );
}

/// Ties break to the LAST max-overlap arm in written order (tsc compares with
/// `>=`): with the `9` arm written FIRST, the selected arm flips to the later
/// `1` arm. tsc 7.0.2: Type '2' is not assignable to type '1'.
#[test]
fn most_overlappy_tie_breaks_to_the_last_written_arm() {
    let diags = diags_with_code(
        r#"
type H = { tag: "y"; m: 9 } | { tag: "x"; m: 1 } | { tag: "w" };
const h: H = { tag: "zz", m: 2 };
"#,
        2322,
    );
    assert!(
        diags
            .iter()
            .any(|d| d.message_text == "Type '2' is not assignable to type '1'."),
        "the LAST max-overlap arm must win the tie, got: {diags:?}"
    );
}

/// A higher key overlap beats written position: the first arm shares three
/// keys with the source, the last only one, so the first arm owns the target
/// even though ties would go to the last. tsc 7.0.2:
/// Type '2' is not assignable to type '1'.
#[test]
fn higher_key_overlap_beats_later_written_position() {
    let diags = diags_with_code(
        r#"
type R = { op: "put"; sz: 1; extra: true } | { op: "get" };
const r: R = { op: "zz", sz: 2, extra: true };
"#,
        2322,
    );
    assert!(
        diags
            .iter()
            .any(|d| d.message_text == "Type '2' is not assignable to type '1'."),
        "the max-overlap arm must beat a later low-overlap arm, got: {diags:?}"
    );
}

/// Alias-wrapped arms behave like inline arms: the overlap scan hops each
/// alias to its object shape. tsc 7.0.2: Type '2' is not assignable to type '9'.
#[test]
fn most_overlappy_fallback_hops_alias_arms() {
    let diags = diags_with_code(
        r#"
type A1 = { m: "p"; w: 1 };
type A2 = { m: "q"; w: 9 };
type A3 = { m: "r" };
type U2 = A1 | A2 | A3;
const uu: U2 = { m: "zz", w: 2 };
"#,
        2322,
    );
    assert!(
        diags
            .iter()
            .any(|d| d.message_text == "Type '2' is not assignable to type '9'."),
        "alias arms must resolve for the overlap scan, got: {diags:?}"
    );
}

/// Argument position takes the same fallback: `elaborateElementwise` runs for
/// call arguments too, so the property-node TS2322 pair matches the
/// declaration form. tsc 7.0.2 reports the same two diagnostics.
#[test]
fn argument_position_uses_most_overlappy_member() {
    let source = r#"
type Gc = { kind: "a"; n: 1 } | { kind: "b"; n: 9 } | { kind: "c" };
declare function take(g: Gc): void;
take({ kind: "zz", n: 2 });
"#;
    let diags = diags_with_code(source, 2322);
    assert!(
        diags
            .iter()
            .any(|d| d.message_text == "Type '2' is not assignable to type '9'."),
        "argument-position n leaf must use the most-overlappy member, got: {diags:?}"
    );
}

/// A nullish arm is PRE-EXCLUDED by the include-walk
/// (`discriminateTypeByDiscriminableItems` marks primitive constituents
/// `False` up front), so the written `... | undefined` union counts as
/// discriminated down to its object arms even though `kind: "zz"` matches
/// none of them: `kind` (exposed by every survivor) keeps the cross-arm
/// union, and `n` (missing from the `"c"` survivor) has an undefined indexed
/// access — NO `n` diagnostic, never the most-overlappy single arm. tsc
/// 7.0.2 reports exactly one diagnostic.
#[test]
fn nullish_arm_discriminates_to_object_arms_not_most_overlappy_member() {
    let source = r#"
type NU = { kind: "a"; n: 1 } | { kind: "b"; n: 9 } | { kind: "c" } | undefined;
const nu: NU = { kind: "zz", n: 2 };
"#;
    let diags = diags_with_code(source, 2322);
    assert_eq!(
        diags.len(),
        1,
        "only the kind diagnostic may fire, got: {diags:?}"
    );
    assert_eq!(
        diags[0].message_text, r#"Type '"zz"' is not assignable to type '"a" | "b" | "c"'."#,
        "the kind head must keep the surviving object arms' union"
    );
}

/// Same pre-exclusion for a non-nullish primitive arm (`number`): the
/// surviving object arms own the target, `t` renders their cross-arm union,
/// and `u` (missing from a survivor) is skipped. tsc 7.0.2 reports exactly
/// one diagnostic.
#[test]
fn primitive_arm_discriminates_to_object_arms_not_most_overlappy_member() {
    let source = r#"
type Pn = { t: "x"; u: 1 } | { t: "y" } | number;
const pn: Pn = { t: "zz", u: 2 };
"#;
    let diags = diags_with_code(source, 2322);
    assert_eq!(
        diags.len(),
        1,
        "only the t diagnostic may fire, got: {diags:?}"
    );
    assert_eq!(
        diags[0].message_text, r#"Type '"zz"' is not assignable to type '"x" | "y"'."#,
        "the t head must keep the surviving object arms' union"
    );
}

/// The written-union arm ORDER half (#17696 residuals family): instantiating
/// a generic union alias (`GU<1>`) re-interns the substituted arm with a
/// fresh `ShapeId`, so the canonical member list sorts it away from its
/// declared position — the last-best-wins scan must still walk declaration
/// order (restored from the interner's `union_origin` side table) and pick
/// the LAST max-overlap arm as written, `{ k: "b"; v: 9 }`. tsc 7.0.2:
/// `Type '2' is not assignable to type '9'.` The concrete forms above prove
/// the fallback itself; this guards the order half.
#[test]
fn instantiated_generic_union_keeps_declaration_order_for_most_overlappy() {
    let diags = diags_with_code(
        r#"
type GU<T> = { k: "a"; v: T } | { k: "b"; v: 9 } | { k: "c" };
const gx: GU<1> = { k: "zz", v: 2 };
"#,
        2322,
    );
    assert!(
        diags
            .iter()
            .any(|d| d.message_text == "Type '2' is not assignable to type '9'."),
        "declaration order must drive the last-best-wins tie, got: {diags:?}"
    );
}

/// Flipped declaration: the type-parameter arm is written LAST of the two
/// max-overlap arms, so tsc's last-best-wins tie-break selects the
/// SUBSTITUTED arm (`m: Z` with `Z = 1`) — the rule is declaration order,
/// not "prefer the concrete arm". tsc 7.0.2:
/// `Type '2' is not assignable to type '1'.`
#[test]
fn instantiated_generic_union_tie_breaks_to_last_written_substituted_arm() {
    let diags = diags_with_code(
        r#"
type GT<Z> = { tag: "y"; m: 9 } | { tag: "x"; m: Z } | { tag: "w" };
const gt: GT<1> = { tag: "zz", m: 2 };
"#,
        2322,
    );
    assert!(
        diags
            .iter()
            .any(|d| d.message_text == "Type '2' is not assignable to type '1'."),
        "the last-written substituted arm must win the tie, got: {diags:?}"
    );
}

/// A forwarding alias hop (`W<T> = GU2<T>`) does not disturb the restored
/// declaration order. tsc 7.0.2: `Type '2' is not assignable to type '9'.`
#[test]
fn forwarded_generic_union_alias_keeps_declaration_order_for_most_overlappy() {
    let diags = diags_with_code(
        r#"
type GU2<T> = { k: "a"; v: T } | { k: "b"; v: 9 } | { k: "c" };
type W<T> = GU2<T>;
const wx: W<1> = { k: "zz", v: 2 };
"#,
        2322,
    );
    assert!(
        diags
            .iter()
            .any(|d| d.message_text == "Type '2' is not assignable to type '9'."),
        "the alias hop must not disturb declaration order, got: {diags:?}"
    );
}

/// Argument position takes the same restored order: `elaborateElementwise`
/// runs for call arguments too. tsc 7.0.2 reports the same property-node
/// TS2322 pair as the declaration form.
#[test]
fn instantiated_generic_union_argument_position_keeps_declaration_order() {
    let diags = diags_with_code(
        r#"
type GA<T> = { k: "a"; v: T } | { k: "b"; v: 9 } | { k: "c" };
declare function take(g: GA<1>): void;
take({ k: "zz", v: 2 });
"#,
        2322,
    );
    assert!(
        diags
            .iter()
            .any(|d| d.message_text == "Type '2' is not assignable to type '9'."),
        "argument-position elaboration must keep declaration order, got: {diags:?}"
    );
}

/// Renamed binders and different literal values: the behavior is structural,
/// not tied to the `k`/`v`/`GU` spellings. tsc 7.0.2:
/// `Type '5' is not assignable to type '8'.` (plus the discriminant head,
/// whose cross-arm union tsc prints in its own sorted display order).
#[test]
fn instantiated_generic_union_declaration_order_survives_renamed_binders() {
    let diags = diags_with_code(
        r#"
type Pick2<Z> = { s: "one"; q: Z } | { s: "two"; q: 8 } | { s: "three" };
const p2: Pick2<3> = { s: "zz", q: 5 };
"#,
        2322,
    );
    assert!(
        diags
            .iter()
            .any(|d| d.message_text == "Type '5' is not assignable to type '8'."),
        "renamed binders must not change the order rule, got: {diags:?}"
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

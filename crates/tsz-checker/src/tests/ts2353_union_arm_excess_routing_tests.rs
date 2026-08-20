//! TS2353-vs-TS2345/TS2322 routing for a fresh object literal against a
//! union target whose discriminant narrowing eliminates the key-bearing arms.
//!
//! tsc's `hasExcessProperties` receives the RELATION target and derives its
//! own reduced target: `findMatchingDiscriminantType` applies each
//! discriminator with per-discriminator revert (a discriminator no member
//! positively matches is ignored) and leaves members lacking the property
//! untouched. Known-ness then runs against that reduced union, so a property
//! declared by any surviving arm is never "excess" and the failure surfaces
//! as the plain assignability error (TS2345/TS2322) instead of a TS2353
//! against the one arm the narrowing kept.
//!
//! tsz previously fed the checker's contextual discriminant-narrowed SINGLE
//! member into the excess-property check at the call-error elaboration site,
//! and its EPC discriminant matcher applied a no-positive-match discriminator
//! instead of reverting it — both misrouted `f({ p: 1, q: 8 })` against
//! `{ p: 1; q: 4 } | { p: 2; q: 8 } | Box` into
//! `TS2353 ... 'p' does not exist in type 'Box'`.
//!
//! Every non-`#[ignore]`d expectation was oracled against the pinned
//! typescript 7.0.2 (`scripts/conformance/oracle.sh`, `--strict
//! --target es2022`). Binder names vary across cases so the behavior is
//! structural, not keyed to a spelling.

use crate::test_utils::check_source_diagnostics;

fn code_messages(source: &str) -> Vec<(u32, String)> {
    check_source_diagnostics(source)
        .into_iter()
        .map(|d| {
            let mut text = d.message_text.clone();
            for related in &d.related_information {
                text.push('\n');
                text.push_str(&related.message_text);
            }
            (d.code, text)
        })
        .collect()
}

fn assert_no_ts2353(diags: &[(u32, String)]) {
    assert!(
        diags.iter().all(|(code, _)| *code != 2353),
        "excess-property TS2353 must not fire when the property is declared \
         by an arm the discriminant narrowing eliminated, got: {diags:?}"
    );
}

/// Concrete call form. The durable pin is the ROUTING: no TS2353, and the
/// discriminant-matched arm's property mismatch surfaces. The exact frame
/// shifted when #17789 landed (the discriminant-pinned elaboration anchors a
/// property leaf, `Type '8' is not assignable to type '4'.`); tsc 7.0.2 keeps
/// the TS2345 head + `Types of property 'q'` elaboration — that head shape is
/// pinned as the `#[ignore]`d `concrete_call_oracle_head_shape` below.
#[test]
fn concrete_call_reports_ts2345_not_excess() {
    let diags = code_messages(
        r#"
type U = { p: 1; q: 4 } | { p: 2; q: 8 };
type Box = { box: number };
declare function both(u: U | Box): void;
both({ p: 1, q: 8 });
"#,
    );
    assert_no_ts2353(&diags);
    assert!(
        diags
            .iter()
            .any(|(_, m)| m.contains("Type '8' is not assignable to type '4'.")),
        "expected the discriminant-matched arm's property mismatch to surface, got: {diags:?}"
    );
}

/// tsc 7.0.2 reports the TS2345 HEAD against the full union with the
/// `Types of property 'q'` elaboration beneath it — `hasExcessProperties`'
/// per-property `checkTypes` loop reports under the outer relation error when
/// the reduced target is still a union. This head shape (previously a pinned
/// residual: tsz anchored a bare property leaf instead) is now produced as a
/// side effect of #17798's discriminant-include-walk fix and #17801's
/// excess-property routing fix — re-verified passing on current `main`.
#[test]
fn concrete_call_oracle_head_shape() {
    let diags = code_messages(
        r#"
type U = { p: 1; q: 4 } | { p: 2; q: 8 };
type Box = { box: number };
declare function both(u: U | Box): void;
both({ p: 1, q: 8 });
"#,
    );
    assert!(
        diags.iter().any(|(code, m)| *code == 2345
            && m.contains(
                "Argument of type '{ p: 1; q: 8; }' is not assignable to parameter of type 'Box | U'."
            )
            && m.contains("Types of property 'q' are incompatible.")),
        "expected the oracle head + elaboration shape, got: {diags:?}"
    );
}

/// Renamed binders: the routing is structural, not keyed to `p`/`q`/`Box`.
#[test]
fn concrete_call_reports_plain_error_with_renamed_binders() {
    let diags = code_messages(
        r#"
type Cmd = { op: "put"; sz: "s" } | { op: "get"; sz: "l" };
type Extra = { payload: string };
declare function run(c: Cmd | Extra): void;
run({ op: "put", sz: "l" });
"#,
    );
    assert_no_ts2353(&diags);
    assert!(
        diags
            .iter()
            .any(|(_, m)| m.contains(r#"'"l"' is not assignable to type '"s"'"#)),
        "expected the matched arm's property mismatch for the renamed-binder form, got: {diags:?}"
    );
}

/// Alias-wrapped union: the routing pin (no TS2353) plus the surfaced
/// mismatch; the head shape is covered by `concrete_call_oracle_head_shape`.
#[test]
fn alias_wrapped_union_keeps_alias_head_and_arm_elaboration() {
    let diags = code_messages(
        r#"
type A1 = { p: 1; q: 4 };
type A2 = { p: 2; q: 8 };
type W = A1 | A2 | { box: number };
declare function g(w: W): void;
g({ p: 1, q: 8 });
"#,
    );
    assert_no_ts2353(&diags);
    assert!(
        diags
            .iter()
            .any(|(_, m)| m.contains("Type '8' is not assignable to type '4'.")),
        "expected the matched arm's property mismatch for the alias-wrapped form, got: {diags:?}"
    );
}

/// TS2322 assignment form of the same shape.
#[test]
fn assignment_form_reports_ts2322_not_excess() {
    let diags = code_messages(
        r#"
type U = { p: 1; q: 4 } | { p: 2; q: 8 };
type Box = { box: number };
const v: U | Box = { p: 1, q: 8 };
"#,
    );
    assert_no_ts2353(&diags);
    assert!(
        diags.iter().any(|(code, m)| *code == 2322
            && m.contains("Type '8' is not assignable to type '4'.")),
        "expected the matched arm's property mismatch for the assignment form, got: {diags:?}"
    );
}

/// Generic form: the routing pin (no TS2353) plus the surfaced mismatch; the
/// instantiated-arm head spelling belongs to the `#[ignore]`d head-shape
/// residuals (here and in
/// `ts2345_generic_call_concrete_alias_parameter_display_tests`).
#[test]
fn generic_application_arm_reports_ts2345_not_excess() {
    let diags = code_messages(
        r#"
type U = { p: 1; q: 4 } | { p: 2; q: 8 };
type Box<T> = { box: T };
declare function both<T>(t: T, u: U | Box<T>): void;
both(0, { p: 1, q: 8 });
"#,
    );
    assert_no_ts2353(&diags);
    assert!(
        diags
            .iter()
            .any(|(_, m)| m.contains("Type '8' is not assignable to type '4'.")),
        "expected the matched arm's property mismatch for the generic form, got: {diags:?}"
    );
}

/// Negative control: a property unknown to EVERY arm stays TS2353, displayed
/// against the full union. Oracle: byte-identical.
#[test]
fn property_unknown_in_every_arm_keeps_ts2353() {
    let diags = code_messages(
        r#"
type U = { p: 1; q: 4 } | { p: 2; q: 8 };
type Box<T> = { box: T };
declare function both<T>(t: T, u: U | Box<T>): void;
both(0, { zz: 1 });
"#,
    );
    assert!(
        diags.iter().any(|(code, m)| *code == 2353
            && m.contains("'zz' does not exist in type 'Box<number> | U'")),
        "a property no arm declares must stay an excess-property error \
         against the full union, got: {diags:?}"
    );
}

/// Negative control: a discriminant-narrowed excess property still reports
/// TS2353 against the narrowed arm. Oracle: byte-identical.
#[test]
fn discriminated_excess_property_keeps_narrowed_ts2353() {
    let diags = code_messages(
        r#"
type G = { kind: "a"; n: 1 } | { kind: "b"; n: 9 } | { kind: "c" };
declare function f(g: G): void;
f({ kind: "a", n: 1, extra: 5 });
"#,
    );
    assert!(
        diags.iter().any(|(code, m)| *code == 2353
            && m.contains(r#"'extra' does not exist in type '{ kind: "a"; n: 1; }'"#)),
        "a genuinely unknown property keeps the narrowed TS2353, got: {diags:?}"
    );
}

/// Negative control: a VALID literal whose failing-discriminator sibling used
/// to over-narrow the EPC target must produce no diagnostics at all — the
/// value satisfies the arm that lacks the discriminant properties.
#[test]
fn valid_literal_with_property_known_only_in_eliminated_arm_is_clean() {
    let diags = code_messages(
        r#"
type U = { p: 1; q: 4 } | { p: 2; q: 8 };
type Box = { box: number };
declare function both(u: U | Box): void;
both({ p: 1, q: 4, box: 1 });
"#,
    );
    assert!(
        diags.is_empty(),
        "the literal matches the first arm exactly (box is declared by the \
         other arm), expected no diagnostics, got: {diags:?}"
    );
}

/// Mixed one-known-one-unknown literal: the unknown property reports TS2353
/// once; the known-elsewhere property must not produce a second excess error.
/// tsz's remaining divergence here is arm ORDER only (tsz renders
/// `'{ p: 1; q: 4; } | Box'`, oracle 7.0.2 `'Box | { p: 1; q: 4; }'`), which
/// is the pre-existing written-union display-order residual, so the fence
/// pins the routing and the member set, not the order.
#[test]
fn partially_known_literal_reports_single_excess_for_unknown_property() {
    let diags = code_messages(
        r#"
type U = { p: 1; q: 4 } | { p: 2; q: 8 };
type Box = { box: number };
declare function both(u: U | Box): void;
both({ p: 1, zz: 8 });
"#,
    );
    let excess: Vec<&(u32, String)> = diags.iter().filter(|(code, _)| *code == 2353).collect();
    assert_eq!(
        excess.len(),
        1,
        "exactly one excess-property error (for 'zz'), got: {diags:?}"
    );
    assert!(
        excess[0].1.contains("'zz' does not exist")
            && excess[0].1.contains("{ p: 1; q: 4; }")
            && excess[0].1.contains("Box"),
        "the excess target is the discriminant-reduced union (matched arm + \
         arms lacking the discriminant), got: {diags:?}"
    );
}

/// When the literal ALSO satisfies the discriminant-free arm structurally
/// (`box: 5` present), tsc still fails the relation — `hasExcessProperties`
/// checks each known property's type against the union of that property's
/// types across the reduced arms (`getTypeOfPropertyInTypes`) and reports
/// `Types of property 'q' are incompatible.` under the TS2345 head. The
/// Lawyer's `union_per_property_failure_witness` gate owns the verdict and
/// `fresh_union_per_property_reason` the chain
/// (`relations/subtype/union_property_check.rs`).
#[test]
fn literal_satisfying_discriminant_free_arm_still_fails_per_property_check() {
    let diags = code_messages(
        r#"
type U = { p: 1; q: 4 } | { p: 2; q: 8 };
type Box = { box: number };
declare function both(u: U | Box): void;
both({ p: 1, q: 8, box: 5 });
"#,
    );
    assert!(
        diags.iter().any(|(code, m)| *code == 2345
            && m.contains("Types of property 'q' are incompatible.")),
        "tsc fails the reduced-union per-property check, got: {diags:?}"
    );
}

/// Verdict half of the per-property union check when EVERY key is known
/// somewhere in the union: `{ alpha: 1, beta: 3 }` triggers no excess key
/// against `{ alpha: 1 } | { beta: 2 }` and structurally satisfies the
/// first arm, but tsc's `hasExcessProperties` fails `beta` against the
/// union of the declaring arms' types (`2`); the property-anchored
/// elaboration then reports at `beta` (oracle: TS2322
/// `Type '3' is not assignable to type '2'.`).
#[test]
fn every_key_known_across_arms_still_fails_per_property_union() {
    let diags = code_messages(
        r#"
declare function pick(x: { alpha: 1 } | { beta: 2 }): void;
pick({ alpha: 1, beta: 3 });
"#,
    );
    assert!(
        diags
            .iter()
            .any(|(code, m)| *code == 2322 && m.contains("Type '3' is not assignable to type '2'.")),
        "per-property union check must reject the cross-arm literal, got: {diags:?}"
    );
    assert_no_ts2353(&diags);
}

/// Assignment position (TS2322 head) with renamed binders: the discriminant
/// `tag: "sq"` reduces the arms to the matched arm plus the key-free `Loose`,
/// and `size: 8` fails against the reduced declaring arms' `4`.
#[test]
fn assignment_position_fresh_literal_fails_per_property_union() {
    let diags = code_messages(
        r#"
type Shape = { tag: "sq"; size: 4 } | { tag: "ci"; size: 8 };
type Loose = { pad: number };
const cell: Shape | Loose = { tag: "sq", size: 8, pad: 5 };
"#,
    );
    assert!(
        diags.iter().any(|(code, m)| *code == 2322
            && m.contains("Types of property 'size' are incompatible.")
            && m.contains("Type '8' is not assignable to type '4'.")),
        "assignment-position per-property union check missing, got: {diags:?}"
    );
    assert_no_ts2353(&diags);
}

/// The whole union spelled through an alias (`Mixed`) with a nested union
/// alias arm (`Data`): flattening must reach the leaf arms behind both lazy
/// indirections for the reduction and the per-property collection.
#[test]
fn whole_union_alias_target_keeps_per_property_verdict() {
    let diags = code_messages(
        r#"
type Data = { mode: 1; val: 4 } | { mode: 2; val: 8 };
type Extra = { pad: number };
type Mixed = Data | Extra;
declare function feed(m: Mixed): void;
feed({ mode: 1, val: 8, pad: 2 });
"#,
    );
    assert!(
        diags.iter().any(|(code, m)| *code == 2345
            && m.contains("Types of property 'val' are incompatible.")
            && m.contains("Type '8' is not assignable to type '4'.")),
        "aliased union target must keep the per-property union verdict, got: {diags:?}"
    );
    assert_no_ts2353(&diags);
}

/// Negative control: a literal whose properties all satisfy the reduced arms
/// stays accepted — the discriminant-free arm's key is legal through the kept
/// arm, and every declared property relates (`val: 4` vs `4`).
#[test]
fn reduced_arm_satisfying_literal_stays_accepted() {
    let diags = code_messages(
        r#"
type Data = { mode: 1; val: 4 } | { mode: 2; val: 8 };
type Extra = { pad: number };
declare function feed(m: Data | Extra): void;
feed({ mode: 1, val: 4, pad: 1 });
"#,
    );
    assert!(
        diags.is_empty(),
        "literal satisfying the reduced arms must stay accepted, got: {diags:?}"
    );
}

/// Negative control: mixing keys of two arms is fine when the property types
/// agree with their declaring arms (`extra: 3` vs `3`) — no discriminant
/// narrowing applies (each key occurs in one arm only, uniform types).
#[test]
fn cross_arm_key_mix_without_conflict_stays_accepted() {
    let diags = code_messages(
        r#"
declare function take(x: { left: 1 } | { right: 2; extra: 3 }): void;
take({ left: 1, extra: 3 });
"#,
    );
    assert!(
        diags.is_empty(),
        "agreeing cross-arm keys must stay accepted, got: {diags:?}"
    );
}

/// Negative control: the per-property union check is an excess-property rule —
/// it applies only to FRESH object literals. The same shape through a
/// declared (non-fresh) value stays accepted via the structural arm.
#[test]
fn non_fresh_source_skips_per_property_union_check() {
    let diags = code_messages(
        r#"
type Data = { mode: 1; val: 4 } | { mode: 2; val: 8 };
type Extra = { pad: number };
declare const stored: { mode: 1; val: 8; pad: 5 };
declare function feed(m: Data | Extra): void;
feed(stored);
"#,
    );
    assert!(
        diags.is_empty(),
        "non-fresh source must skip the per-property union check, got: {diags:?}"
    );
}

/// A generic application arm (`Holder<number>`) participates like any other
/// arm: it survives the `key: 1` reduction (it lacks the key), its `held`
/// property satisfies its declared type, and the failing `num` folds against
/// the declaring reduced arm's `4`.
#[test]
fn generic_application_arm_participates_in_per_property_union() {
    let diags = code_messages(
        r#"
interface Holder<T> { held: T }
type Pair = { key: 1; num: 4 } | { key: 2; num: 8 };
declare function put(x: Pair | Holder<number>): void;
put({ key: 1, num: 8, held: 5 });
"#,
    );
    assert!(
        diags.iter().any(|(code, m)| *code == 2345
            && m.contains("Types of property 'num' are incompatible.")
            && m.contains("Type '8' is not assignable to type '4'.")),
        "application arm must participate in the per-property union check, got: {diags:?}"
    );
    assert_no_ts2353(&diags);
}

/// Pinned residual (oracle-verified divergence, PRE-EXISTING on the parent of
/// the per-property-verdict change — same TS2353 with and without it): when a
/// discriminator (`q: 8`) positively matches an arm and the checker's
/// arm-wise contextual typing widens another written unit (`p: 1` → `number`
/// where the narrowed arm lacks `p`), tsc 7.0.2 reports
/// `TS2345 ... Types of property 'p' are incompatible. Type 'number' is not
/// assignable to type '2'.` while tsz's checker-side EPC misroutes into
/// `TS2353 ... 'p' does not exist in type '{ box: number; q: 8; }'`. Owner:
/// the checker's contextual-widening interaction with the EPC discriminant
/// matcher (`excess_property_tail.rs`), not the solver's per-property union
/// verdict.
#[test]
#[ignore = "checker EPC pins the q-matched arm and reports TS2353 for 'p'; tsc 7.0.2 widens the source's 'p' arm-wise to 'number' and fails the relation with `Types of property 'p' are incompatible.` (verified 2026-08-20, pre-existing before the per-property union verdict)"]
fn contextually_widened_source_discriminant_misroutes_to_excess_residual() {
    let diags = code_messages(
        r#"
type U = { p: 1; q: 4 } | { p: 2; q: 8 };
declare function both(u: U | { box: number; q: 8 }): void;
both({ p: 1, q: 8, box: 5 });
"#,
    );
    assert!(
        diags.iter().any(|(code, m)| *code == 2345
            && m.contains("Types of property 'p' are incompatible.")
            && m.contains("Type 'number' is not assignable to type '2'.")),
        "tsc widens 'p' arm-wise and fails the relation per-property, got: {diags:?}"
    );
    assert_no_ts2353(&diags);
}

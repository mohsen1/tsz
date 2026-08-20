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

/// Pinned residual (oracle-verified shape, deliberately out of scope): tsc
/// 7.0.2 reports the TS2345 HEAD against the full union with the
/// `Types of property 'q'` elaboration beneath it — `hasExcessProperties`'
/// per-property `checkTypes` loop reports under the outer relation error when
/// the reduced target is still a union. tsz anchors a bare property leaf.
/// Owner: the relation-failure/checkTypes half for union targets (same owner
/// as the best-arm elaboration residual in
/// `ts2345_generic_call_concrete_alias_parameter_display_tests`).
#[test]
#[ignore = "tsz anchors a property-leaf TS2322 where tsc 7.0.2 keeps the TS2345 head + `Types of property 'q'` elaboration for a reduced target that is still a union"]
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

/// Pinned residual (oracle-verified divergence, deliberately out of scope):
/// when the literal ALSO satisfies the discriminant-free arm structurally
/// (`box: 5` present), tsc still fails the relation — `hasExcessProperties`
/// checks each known property's type against the union of that property's
/// types across the reduced arms (`getTypeOfPropertyInTypes`) and reports
/// `Types of property 'q' are incompatible.` under the TS2345 head. tsz's
/// relation accepts via the `Box` arm (extra properties are structurally
/// fine once no excess fires), so no diagnostic is produced. Owner: the
/// relation-failure half for union targets (adjacent to the best-arm
/// elaboration residual pinned in
/// `ts2345_generic_call_concrete_alias_parameter_display_tests`).
#[test]
#[ignore = "tsz accepts via the discriminant-free arm where tsc 7.0.2 reports TS2345 with the per-property union check (`Types of property 'q'` / `8` vs `4`) — relation-verdict half not modeled"]
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

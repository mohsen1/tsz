//! Freshness-gated enum-member widening at mutable observation points.
//!
//! Structural rule: when an observation point widens an enum-member-typed
//! initializer (mutable binding, parameter default, inferred return), the
//! widening applies only when the initializer expression is *fresh* — a direct
//! enum-member access or a chain of parentheses/conditional branches/
//! unannotated `const` references ending in one — and the widening target is
//! the enum *instance* type `E`, never the enum's static object type
//! (`typeof E`). Non-fresh enum-member sources (annotated consts, property
//! reads) keep the member type. Mirrors tsc's fresh/regular enum literal
//! types and `getWidenedLiteralTypeForInitializer`.
//!
//! Regression tests for #15445 (false-negative: annotated enum-member const
//! references over-widened) and #15444 (false-positive: `typeof` of an
//! enum-initialized binding observed the enum object type).

use tsz_checker::test_utils::check_source_codes as get_error_codes;

fn count(codes: &[u32], code: u32) -> usize {
    codes.iter().filter(|&&c| c == code).count()
}

// ---------------------------------------------------------------------------
// Fresh sources widen to the enum type (must keep working).
// ---------------------------------------------------------------------------

#[test]
fn direct_member_access_widens_to_enum() {
    let codes = get_error_codes(
        r#"
enum E { A, B }
let x = E.A;
x = E.B;
"#,
    );
    assert!(
        codes.is_empty(),
        "fresh member access must widen `x` to `E`, got: {codes:?}"
    );
}

#[test]
fn conditional_over_members_widens_to_enum() {
    let codes = get_error_codes(
        r#"
enum Dir { Up, Down }
declare const cond: boolean;
let d = cond ? Dir.Up : Dir.Down;
d = Dir.Up;
"#,
    );
    assert!(
        codes.is_empty(),
        "fresh conditional branches must widen `d` to `Dir`, got: {codes:?}"
    );
}

#[test]
fn unannotated_const_chain_stays_fresh() {
    let codes = get_error_codes(
        r#"
enum E { A, B }
const u = E.A;
let x = u;
x = E.B;
"#,
    );
    assert!(
        codes.is_empty(),
        "unannotated const chain keeps freshness; `x` must widen to `E`, got: {codes:?}"
    );
}

#[test]
fn element_access_member_is_fresh() {
    let codes = get_error_codes(
        r#"
enum E { A, B }
let x = E["A"];
x = E.B;
"#,
    );
    assert!(
        codes.is_empty(),
        "element access of a member is a fresh enum literal, got: {codes:?}"
    );
}

#[test]
fn member_access_through_const_enum_object_alias_is_fresh() {
    let codes = get_error_codes(
        r#"
enum E { A, B }
const alias = E;
let x = alias.A;
x = E.B;
"#,
    );
    assert!(
        codes.is_empty(),
        "member access through an unannotated const enum-object alias stays fresh, got: {codes:?}"
    );
}

#[test]
fn parameter_default_member_widens_to_enum() {
    let codes = get_error_codes(
        r#"
enum E { A, B }
function g(p = E.A) {
    p = E.B;
}
"#,
    );
    assert!(
        codes.is_empty(),
        "fresh parameter default must widen `p` to `E`, got: {codes:?}"
    );
}

#[test]
fn fresh_return_widens_to_enum() {
    let codes = get_error_codes(
        r#"
enum E { A, B }
function h() { return E.A; }
declare function wantsE(x: E): void;
wantsE(h());
const back: E = h();
"#,
    );
    assert!(
        codes.is_empty(),
        "fresh member return must widen the inferred return type to `E`, got: {codes:?}"
    );
}

// ---------------------------------------------------------------------------
// Non-fresh sources keep the member type (#15445).
// ---------------------------------------------------------------------------

#[test]
fn annotated_const_reference_keeps_member_type() {
    let codes = get_error_codes(
        r#"
enum E { A, B }
const c: E.A = E.A;
let x = c;
x = E.B;
"#,
    );
    assert_eq!(
        count(&codes, 2322),
        1,
        "annotated const reference is non-fresh; `x: E.A` must reject `E.B`, got: {codes:?}"
    );
}

#[test]
fn annotated_const_reference_keeps_member_type_string_enum() {
    let codes = get_error_codes(
        r#"
enum S { On = "on", Off = "off" }
const sc: S.On = S.On;
let x = sc;
x = S.Off;
"#,
    );
    assert_eq!(
        count(&codes, 2322),
        1,
        "string-enum annotated const reference must keep `S.On`, got: {codes:?}"
    );
}

#[test]
fn property_read_keeps_member_type() {
    let codes = get_error_codes(
        r#"
enum E { A, B }
declare const o: { p: E.A };
let x = o.p;
x = E.B;
"#,
    );
    assert_eq!(
        count(&codes, 2322),
        1,
        "property read is non-fresh; `x: E.A` must reject `E.B`, got: {codes:?}"
    );
}

#[test]
fn non_fresh_return_keeps_member_type() {
    let codes = get_error_codes(
        r#"
enum E { A, B }
function h() { const c: E.A = E.A; return c; }
const back: E.A = h();
"#,
    );
    assert!(
        codes.is_empty(),
        "non-fresh member return must keep `E.A` so the annotated read-back succeeds, got: {codes:?}"
    );
}

// ---------------------------------------------------------------------------
// Widening target: enum type `E`, never `typeof E` (#15444).
// ---------------------------------------------------------------------------

#[test]
fn typeof_of_widened_binding_is_enum_type() {
    let codes = get_error_codes(
        r#"
enum Phase { Init, Run }
let stage = Phase.Init;
declare function expectStage(value: typeof stage): void;
expectStage(Phase.Run);
"#,
    );
    assert!(
        codes.is_empty(),
        "`typeof stage` must observe the widened enum type `Phase`, got: {codes:?}"
    );
}

#[test]
fn typeof_of_widened_binding_in_annotation_is_enum_type() {
    let codes = get_error_codes(
        r#"
enum Phase { Init, Run }
let stage = Phase.Init;
const x: typeof stage = Phase.Run;
"#,
    );
    assert!(
        codes.is_empty(),
        "`typeof stage` annotation must admit other members of `Phase`, got: {codes:?}"
    );
}

#[test]
fn typeof_of_namespace_nested_binding_is_enum_type() {
    let codes = get_error_codes(
        r#"
enum Phase { Init, Run }
namespace N {
    export let inner = Phase.Init;
}
declare function wantsInner(v: typeof N.inner): void;
wantsInner(Phase.Run);
"#,
    );
    assert!(
        codes.is_empty(),
        "`typeof N.inner` must observe the widened enum type `Phase`, got: {codes:?}"
    );
}

#[test]
fn typeof_of_enum_itself_stays_object_type() {
    let codes = get_error_codes(
        r#"
enum Phase { Init, Run }
declare function wantsObj(o: typeof Phase): void;
wantsObj(Phase);
wantsObj(Phase.Run);
"#,
    );
    assert_eq!(
        count(&codes, 2345),
        1,
        "`typeof Phase` stays the enum object type: `Phase` ok, `Phase.Run` rejected, got: {codes:?}"
    );
}

#[test]
fn keyof_typeof_enum_still_enumerates_member_names() {
    let codes = get_error_codes(
        r#"
enum Phase { Init, Run }
type Names = keyof typeof Phase;
const ok: Names = "Init";
const bad: Names = "Nope";
"#,
    );
    assert_eq!(
        count(&codes, 2322),
        1,
        "`keyof typeof Phase` keeps enumerating member names, got: {codes:?}"
    );
}

// ---------------------------------------------------------------------------
// Direct assignment across members keeps compiling after the target fix.
// ---------------------------------------------------------------------------

#[test]
fn direct_reassignment_across_members_allowed() {
    let codes = get_error_codes(
        r#"
enum Signal { Go, Stop }
let current = Signal.Go;
current = Signal.Stop;
declare function wantsSignal(s: Signal): void;
wantsSignal(current);
"#,
    );
    assert!(
        codes.is_empty(),
        "widened binding must accept every member and satisfy `Signal` positions, got: {codes:?}"
    );
}

#[test]
fn widened_binding_rejects_other_enum_and_raw_string() {
    let codes = get_error_codes(
        r#"
enum A { X }
enum B { Y }
let a = A.X;
a = B.Y;
a = "nope";
"#,
    );
    assert_eq!(
        count(&codes, 2322),
        2,
        "widened `A` binding must reject another enum and raw strings, got: {codes:?}"
    );
}

//! Enum-member widening at mutable bindings, gated on freshness.
//!
//! Two coupled defects (#15444 false-positive, #15445 false-negative) shared
//! one owner: enum-member widening at mutable-binding / return-position
//! observation points.
//!
//! Structural rule (matching `tsc`'s `getWidenedLiteralTypeForInitializer` +
//! fresh/regular enum literal types): a mutable binding whose initializer is a
//! *fresh* enum-member access (`let x = E.A`, a conditional over member
//! accesses, or a chain through unannotated consts) widens to the enum **type**
//! `E` (the union of member literals), never the enum **object** type
//! (`typeof E`). A *non-fresh* enum-member reference (an annotated const, a
//! property read) keeps the specific member type `E.A`.
//!
//! Binder names are varied across cases so no fix can key on a fixed identifier.

fn messages(source: &str) -> Vec<(u32, String)> {
    tsz_checker::test_utils::check_source_code_messages(source)
}

fn codes(source: &str) -> Vec<u32> {
    messages(source).into_iter().map(|(c, _)| c).collect()
}

// ---------------------------------------------------------------------------
// #15444 — a fresh enum-member binding widens to `E`, not `typeof E`.
// ---------------------------------------------------------------------------

#[test]
fn fresh_enum_binding_widens_to_enum_type_not_object_via_typeof() {
    // `stage: Phase` (not `typeof Phase`), so `typeof stage` is `Phase` and
    // another member is assignable. Previously TS2345 / `typeof Phase`.
    let src = r#"
enum Phase { Init, Run }
let stage = Phase.Init;
declare function expectStage(value: typeof stage): void;
expectStage(Phase.Run);
"#;
    assert!(
        !codes(src).contains(&2345) && !codes(src).contains(&2322),
        "fresh enum binding must widen to the enum type `Phase`, got: {:?}",
        messages(src)
    );
}

#[test]
fn fresh_enum_binding_typeof_query_assigns_sibling_member() {
    let src = r#"
enum Mode { On, Off }
let flag = Mode.On;
declare let mirror: typeof flag;
mirror = Mode.Off;
"#;
    assert!(
        codes(src).is_empty(),
        "`typeof flag` must be the enum type `Mode`, got: {:?}",
        messages(src)
    );
}

#[test]
fn fresh_enum_binding_direct_reassignment_stays_clean() {
    let src = r#"
enum Signal { Go, Stop }
let current = Signal.Go;
current = Signal.Stop;
"#;
    assert!(codes(src).is_empty(), "got: {:?}", messages(src));
}

#[test]
fn fresh_enum_binding_conditional_widens_to_enum() {
    let src = r#"
enum Dir { Up, Down }
declare const cond: boolean;
let d = cond ? Dir.Up : Dir.Down;
d = Dir.Up;
d = Dir.Down;
"#;
    assert!(codes(src).is_empty(), "got: {:?}", messages(src));
}

#[test]
fn fresh_string_enum_binding_widens_to_enum() {
    let src = r#"
enum Color { Red = "red", Blue = "blue" }
let c = Color.Red;
c = Color.Blue;
"#;
    assert!(codes(src).is_empty(), "got: {:?}", messages(src));
}

#[test]
fn fresh_enum_parameter_default_widens_to_enum() {
    let src = r#"
enum Level { Low, High }
function grade(rank = Level.Low) {
    rank = Level.High;
}
"#;
    assert!(codes(src).is_empty(), "got: {:?}", messages(src));
}

#[test]
fn fresh_namespace_qualified_enum_member_widens_to_enum() {
    let src = r#"
namespace Space { export enum Kind { A, B } }
let k = Space.Kind.A;
k = Space.Kind.B;
"#;
    assert!(codes(src).is_empty(), "got: {:?}", messages(src));
}

// ---------------------------------------------------------------------------
// #15445 — a *non-fresh* enum-member reference keeps the member type `E.A`.
// ---------------------------------------------------------------------------

#[test]
fn annotated_enum_const_reference_is_non_fresh_and_keeps_member() {
    // `x2: E.A` (not `E`), so `x2 = E.B` must error. Previously silently
    // dropped (tsz widened `x2` to `E`).
    let src = r#"
enum E { A, B }
const anchor: E.A = E.A;
let x2 = anchor;
x2 = E.B;
"#;
    assert!(
        codes(src).contains(&2322),
        "annotated non-fresh enum const must keep `E.A`; expected TS2322, got: {:?}",
        messages(src)
    );
}

#[test]
fn plain_enum_const_reference_is_non_fresh_and_keeps_member() {
    // `const c = E.A` keeps `E.A`; `typeof c` is `E.A`, so a sibling errors.
    let src = r#"
enum Fruit { Apple, Pear }
const picked = Fruit.Apple;
declare function taste(v: typeof picked): void;
taste(Fruit.Pear);
"#;
    assert!(
        codes(src).contains(&2345),
        "a plain enum const must keep `Fruit.Apple`; expected TS2345, got: {:?}",
        messages(src)
    );
}

#[test]
fn unannotated_const_chain_is_fresh_by_reference() {
    // `const u = E.A` (unannotated) is a widening literal; a `let` copy widens.
    let src = r#"
enum E { A, B }
const u = E.A;
let x = u;
x = E.B;
"#;
    assert!(codes(src).is_empty(), "got: {:?}", messages(src));
}

#[test]
fn property_read_of_enum_member_is_non_fresh() {
    let src = r#"
enum E { A, B }
declare const holder: { slot: E.A };
let x = holder.slot;
x = E.B;
"#;
    assert!(
        codes(src).contains(&2322),
        "a property read is non-fresh and must keep `E.A`; expected TS2322, got: {:?}",
        messages(src)
    );
}

// ---------------------------------------------------------------------------
// Return-position widening shares the same freshness rule (#15445).
// ---------------------------------------------------------------------------

#[test]
fn fresh_enum_return_widens_to_enum() {
    let src = r#"
enum E { A, B }
function pick() { return E.A; }
const ok: E = pick();
"#;
    assert!(codes(src).is_empty(), "got: {:?}", messages(src));
}

#[test]
fn fresh_enum_return_does_not_pin_member() {
    let src = r#"
enum E { A, B }
function pick() { return E.A; }
const narrowed: E.A = pick();
"#;
    assert!(
        codes(src).contains(&2322),
        "a fresh enum return widens to `E`, not `E.A`; expected TS2322, got: {:?}",
        messages(src)
    );
}

#[test]
fn non_fresh_enum_return_keeps_member() {
    let src = r#"
enum E { A, B }
function pick(): E.A {
    const anchor: E.A = E.A;
    return anchor;
}
"#;
    assert!(codes(src).is_empty(), "got: {:?}", messages(src));
}

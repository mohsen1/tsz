//! The TS2345 TARGET half of a generic call keeps the written alias spelling
//! when the failing parameter's type mentions none of the signature's type
//! parameters: instantiation is the identity on such a parameter, so tsc
//! renders the parameter type as written (`... is not assignable to parameter
//! of type 'U'.`), alias reference included.
//!
//! Before the fix, `instantiated_call_parameter_display` re-instantiated and
//! evaluated the raw parameter type for ANY generic signature, so a concrete
//! union alias parameter rendered structurally (`{ p: number; q: number; }`)
//! whenever the call inferred its type arguments. The guard now mirrors
//! `generic_call_parameter_alias_display`: a raw parameter type without type
//! parameters declines the instantiated display and the alias-preserving
//! fallback owns it.
//!
//! Every expectation was oracled against the pinned typescript 7.0.2
//! (`scripts/conformance/oracle.sh`, default flags), byte-identical output per
//! witness, including the elaboration lines. Binder names vary across cases so
//! the behavior is structural, not keyed to a spelling.

use crate::test_utils::check_source_diagnostics;

fn ts2345_messages(source: &str) -> Vec<String> {
    check_source_diagnostics(source)
        .into_iter()
        .filter(|d| d.code == 2345)
        .map(|d| d.message_text)
        .collect()
}

#[test]
fn inferred_generic_call_union_alias_parameter_keeps_alias_spelling() {
    // The witness: a free generic function whose second parameter is a
    // concrete union alias; the call infers T from the first argument.
    let messages = ts2345_messages(
        r#"
type U = { p: 1; q: 4 } | { p: 2; q: 8 };
declare function id<T>(t: T, u: U): void;
id(0, { p: 1, q: 8 });
"#,
    );
    assert!(
        messages
            .iter()
            .any(|m| m.contains("is not assignable to parameter of type 'U'")),
        "generic-call TS2345 target should keep the alias spelling, got: {messages:?}"
    );
    assert!(
        !messages.iter().any(|m| m.contains("parameter of type '{")),
        "generic-call TS2345 target must not render the alias structurally, got: {messages:?}"
    );
}

#[test]
fn renamed_binders_keep_alias_spelling() {
    let messages = ts2345_messages(
        r#"
type Zebra = { left: "a"; right: "b" } | { left: "c"; right: "d" };
declare function pick<Q>(head: Q, tail: Zebra): void;
pick("x", { left: "a", right: "d" });
"#,
    );
    assert!(
        messages
            .iter()
            .any(|m| m.contains("is not assignable to parameter of type 'Zebra'")),
        "alias preservation must not depend on binder spelling, got: {messages:?}"
    );
}

#[test]
fn non_fresh_variable_argument_keeps_alias_target_and_member_frame() {
    // The fix is not freshness-dependent: a widened variable source still
    // renders the target as the alias, and the best-member frame survives
    // (tsc: `Type '{ p: number; q: number; }' is not assignable to type
    // '{ p: 2; q: 8; }'.` beneath the head).
    let messages = ts2345_messages(
        r#"
type U = { p: 1; q: 4 } | { p: 2; q: 8 };
declare function id<T>(t: T, u: U): void;
const v = { p: 1, q: 8 };
id(0, v);
"#,
    );
    assert!(
        messages
            .iter()
            .any(|m| m.contains("is not assignable to parameter of type 'U'")),
        "non-fresh source still keeps the alias target, got: {messages:?}"
    );
}

#[test]
fn generic_method_call_keeps_alias_spelling() {
    // Positive control (already correct before the fix): a generic class
    // method resolves the callee through the qualified-symbol path.
    let messages = ts2345_messages(
        r#"
type U = { p: 1; q: 4 } | { p: 2; q: 8 };
class Holder<T> {
  put(t: T, u: U): void {}
}
new Holder<number>().put(0, { p: 1, q: 8 });
"#,
    );
    assert!(
        messages
            .iter()
            .any(|m| m.contains("is not assignable to parameter of type 'U'")),
        "generic method call should keep the alias spelling, got: {messages:?}"
    );
}

#[test]
fn explicit_type_arguments_keep_alias_spelling() {
    // Positive control (already correct before the fix): an explicit
    // type-argument list fixes every type parameter before inference.
    let messages = ts2345_messages(
        r#"
type U = { p: 1; q: 4 } | { p: 2; q: 8 };
declare function id<T>(t: T, u: U): void;
id<number>(0, { p: 1, q: 8 });
"#,
    );
    assert!(
        messages
            .iter()
            .any(|m| m.contains("is not assignable to parameter of type 'U'")),
        "explicit type arguments should keep the alias spelling, got: {messages:?}"
    );
}

#[test]
fn non_generic_call_keeps_alias_spelling() {
    // Negative/base control: the non-generic path never entered the
    // instantiated display and already kept the alias.
    let messages = ts2345_messages(
        r#"
type U = { p: 1; q: 4 } | { p: 2; q: 8 };
declare function g(u: U): void;
g({ p: 1, q: 8 });
"#,
    );
    assert!(
        messages
            .iter()
            .any(|m| m.contains("is not assignable to parameter of type 'U'")),
        "non-generic call keeps the alias spelling, got: {messages:?}"
    );
}

#[test]
fn primitive_union_alias_parameter_keeps_alias_spelling() {
    let messages = ts2345_messages(
        r#"
type Mode = "on" | "off";
declare function set<T>(t: T, m: Mode): void;
set(0, "auto");
"#,
    );
    assert!(
        messages
            .iter()
            .any(|m| m.contains("is not assignable to parameter of type 'Mode'")),
        "primitive union alias keeps its spelling, got: {messages:?}"
    );
}

#[test]
fn rest_parameter_concrete_alias_element_keeps_alias_spelling() {
    // A rest parameter's element type mentions no type parameters either; the
    // per-argument display renders the element alias (tsc: `parameter of type
    // 'Flag'`).
    let messages = ts2345_messages(
        r#"
type Flag = "on" | "off";
declare function spread<T>(t: T, ...flags: Flag[]): void;
spread(0, "auto");
"#,
    );
    assert!(
        messages
            .iter()
            .any(|m| m.contains("is not assignable to parameter of type 'Flag'")),
        "rest element alias keeps its spelling, got: {messages:?}"
    );
}

#[test]
fn parameter_mentioning_type_parameter_keeps_instantiated_display() {
    // Negative control: a parameter that DOES mention a signature type
    // parameter stays on the instantiated display path (no bare alias, no raw
    // `T[]` leak). tsc 7.0.2 renders `number[] | U` here — the alias arm
    // surviving INSIDE the instantiated union is a separate residual pinned
    // below; this control only guards that the instantiated path still fires.
    let messages = ts2345_messages(
        r#"
type U = { p: 1; q: 4 } | { p: 2; q: 8 };
declare function both<T>(t: T, u: U | T[]): void;
both(0, { p: 1, q: 8 });
"#,
    );
    assert!(
        !messages.iter().any(|m| m.contains("T[]")),
        "instantiated display must not leak the raw type parameter, got: {messages:?}"
    );
}

// The arm-wise half of the family (`u: U | T[]`, a union mixing concrete
// arms with type-parameter arms): tsc instantiates the union arm-wise, so
// the alias arm keeps its written reference while the type-parameter arm
// renders through the call's actual inference — widened when the inference
// source was a fresh literal, unwidened under explicit type arguments or a
// non-fresh literal-typed source. The display recovers the instantiated
// arms from the relation's own final parameter type, so the widening
// decision is inference's, never re-made at display time. Every expectation
// below was oracled against typescript 7.0.2 (`scripts/conformance/oracle.sh`,
// default flags); heads are byte-identical.

#[test]
fn alias_arm_survives_inside_instantiated_union_target() {
    let messages = ts2345_messages(
        r#"
type U = { p: 1; q: 4 } | { p: 2; q: 8 };
declare function both<T>(t: T, u: U | T[]): void;
both(0, { p: 1, q: 8 });
"#,
    );
    assert!(
        messages
            .iter()
            .any(|m| m.contains("is not assignable to parameter of type 'number[] | U'")),
        "tsc renders the widened instantiated arm and keeps the alias arm, got: {messages:?}"
    );
    assert!(
        !messages
            .iter()
            .any(|m| m.contains("0[]") || m.contains("parameter of type '{")),
        "the fresh-literal candidate widens and the alias arm never renders structurally, got: {messages:?}"
    );
}

#[test]
fn written_arm_order_does_not_drive_the_display_order() {
    // `T[] | U` written the other way round renders identically: the printer's
    // stable union ordering owns the member order, not the annotation.
    let messages = ts2345_messages(
        r#"
type U = { p: 1; q: 4 } | { p: 2; q: 8 };
declare function both<T>(t: T, u: T[] | U): void;
both(0, { p: 1, q: 8 });
"#,
    );
    assert!(
        messages
            .iter()
            .any(|m| m.contains("is not assignable to parameter of type 'number[] | U'")),
        "swapped written arms keep the same display, got: {messages:?}"
    );
}

#[test]
fn explicit_type_arguments_keep_the_unwidened_type_param_arm() {
    // `both<0>(...)` fixes T to the literal itself, so tsc renders `0[]`.
    let messages = ts2345_messages(
        r#"
type U = { p: 1; q: 4 } | { p: 2; q: 8 };
declare function both<T>(t: T, u: U | T[]): void;
both<0>(0, { p: 1, q: 8 });
"#,
    );
    assert!(
        messages
            .iter()
            .any(|m| m.contains("is not assignable to parameter of type '0[] | U'")),
        "explicit type arguments keep the literal type argument, got: {messages:?}"
    );
}

#[test]
fn non_fresh_literal_source_keeps_the_unwidened_type_param_arm() {
    // A literal-annotated variable is not a fresh literal, so inference keeps
    // `0` and tsc renders `0[]`.
    let messages = ts2345_messages(
        r#"
type U = { p: 1; q: 4 } | { p: 2; q: 8 };
declare function both<T>(t: T, u: U | T[]): void;
const t: 0 = 0;
both(t, { p: 1, q: 8 });
"#,
    );
    assert!(
        messages
            .iter()
            .any(|m| m.contains("is not assignable to parameter of type '0[] | U'")),
        "a non-fresh literal inference source stays unwidened, got: {messages:?}"
    );
}

#[test]
fn renamed_binders_keep_arm_wise_display_without_partial_match() {
    // Renamed binders and a source that matches no arm at all (no elaboration
    // in tsc either): the head still renders arm-wise.
    let messages = ts2345_messages(
        r#"
type Cargo = { kind: "a"; n: 1 } | { kind: "b"; n: 2 };
declare function ship<W>(w: W, u: Cargo | W[]): void;
const v = { other: 1 };
ship("s", v);
"#,
    );
    assert!(
        messages
            .iter()
            .any(|m| m.contains("is not assignable to parameter of type 'string[] | Cargo'")),
        "arm-wise display is structural, not keyed to a spelling, got: {messages:?}"
    );
}

#[test]
fn multiple_concrete_arms_keep_spelling_and_stable_order() {
    // A second concrete (non-alias) arm: tsc renders
    // `number[] | boolean[] | U` — instantiated arm and concrete array arm
    // both under the stable array ordering, alias arm after.
    let messages = ts2345_messages(
        r#"
type U = { p: 1; q: 4 } | { p: 2; q: 8 };
declare function both<T>(t: T, u: U | T[] | boolean[]): void;
const src = { p: 5, q: 9 };
both(0, src);
"#,
    );
    assert!(
        messages.iter().any(
            |m| m.contains("is not assignable to parameter of type 'number[] | boolean[] | U'")
        ),
        "three-arm union renders arm-wise in stable order, got: {messages:?}"
    );
}

#[test]
fn rest_parameter_union_element_renders_arm_wise() {
    // A rest argument relates against the rest element type, so the arm-wise
    // display reads the element union (tsc: `number[] | U`, not
    // `(0[] | U)[]`).
    let messages = ts2345_messages(
        r#"
type U = { p: 1; q: 4 } | { p: 2; q: 8 };
declare function spread<T>(t: T, ...xs: (U | T[])[]): void;
spread(0, { p: 1, q: 8 });
"#,
    );
    assert!(
        messages
            .iter()
            .any(|m| m.contains("is not assignable to parameter of type 'number[] | U'")),
        "rest element union renders arm-wise, got: {messages:?}"
    );
}

#[test]
fn rest_parameter_primitive_alias_union_element_renders_arm_wise() {
    let messages = ts2345_messages(
        r#"
type Flag = "on" | "off";
declare function spread<T>(t: T, ...flags: (Flag | T[])[]): void;
spread(0, "auto");
"#,
    );
    assert!(
        messages
            .iter()
            .any(|m| m.contains("is not assignable to parameter of type 'number[] | Flag'")),
        "primitive-alias arm keeps its spelling beside the instantiated arm, got: {messages:?}"
    );
}

#[test]
fn new_expression_keeps_arm_wise_display() {
    // Positive control: the construct-call path renders the same family.
    let messages = ts2345_messages(
        r#"
type U = { p: 1; q: 4 } | { p: 2; q: 8 };
class C<T> {
  constructor(t: T, u: U | T[]) {}
}
new C(0, { p: 1, q: 8 });
"#,
    );
    assert!(
        messages
            .iter()
            .any(|m| m.contains("is not assignable to parameter of type 'number[] | U'")),
        "construct calls render the union arm-wise too, got: {messages:?}"
    );
}

#[test]
fn string_inference_widens_the_type_param_arm_to_string() {
    let messages = ts2345_messages(
        r#"
type U = { p: 1; q: 4 } | { p: 2; q: 8 };
declare function both<T>(t: T, u: U | T[]): void;
both("s", { p: 1, q: 8 });
"#,
    );
    assert!(
        messages
            .iter()
            .any(|m| m.contains("is not assignable to parameter of type 'string[] | U'")),
        "a fresh string literal widens the instantiated arm to string, got: {messages:?}"
    );
}

#[test]
#[ignore = "known pre-existing residual (red on main before the arm-wise display too): tsc 7.0.2 follows the mixed-union head with `Types of property 'q' are incompatible. / Type '8' is not assignable to type '4'.` (best-arm elaboration); tsz emits the bare head. Owner: relation failure reason for union targets, not the display gateway."]
fn mixed_union_head_carries_best_arm_property_elaboration() {
    let messages = ts2345_messages(
        r#"
type U = { p: 1; q: 4 } | { p: 2; q: 8 };
declare function both<T>(t: T, u: U | T[]): void;
both(0, { p: 1, q: 8 });
"#,
    );
    assert!(
        messages
            .iter()
            .any(|m| m.contains("Types of property 'q' are incompatible")),
        "tsc elaborates the discriminant-matched arm's property mismatch, got: {messages:?}"
    );
}

#[test]
#[ignore = "known pre-existing residual (red on main before the arm-wise display too): a generic alias-application arm (`u: U | Box<T>`) — tsc 7.0.2 reports TS2345 with target `Box<number> | U`; tsz routes the fresh object literal through the excess-property check and reports TS2353 against `Box<number>` alone. Owner: argument excess-property vs assignability routing for mixed-union parameters, not the display gateway."]
fn generic_alias_application_arm_keeps_application_spelling() {
    let messages = ts2345_messages(
        r#"
type U = { p: 1; q: 4 } | { p: 2; q: 8 };
type Box<T> = { box: T };
declare function both<T>(t: T, u: U | Box<T>): void;
both(0, { p: 1, q: 8 });
"#,
    );
    assert!(
        messages
            .iter()
            .any(|m| m.contains("is not assignable to parameter of type 'Box<number> | U'")),
        "tsc keeps the instantiated application spelling beside the alias arm, got: {messages:?}"
    );
}

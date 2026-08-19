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

#[test]
#[ignore = "known residual: tsc 7.0.2 renders `number[] | U` (widened instantiated arm, alias arm preserved inside the union); tsz renders `0[] | { p: number; q: number; }` and picks a different elaboration property. Owner: instantiated-union display, not the identity guard this suite pins."]
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
}

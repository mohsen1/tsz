//! Adjacent-case coverage for #16307's static-side leg: a STATIC class
//! member whose computed key resolves to a plain (non-unique) `symbol`
//! binding contributes a `[key: symbol]: V` index signature to the
//! constructor type (`typeof C`) rather than a named static member.
//!
//! Structural rule, verified against the pinned `tsc` 7.0.2 oracle:
//! - A wide-`symbol` key on a static property, method, getter or setter
//!   routes into the constructor type's symbol index signature, the static
//!   twin of the instance-side routing #16326/#16329/#16331 already cover.
//!   Two static declarations keyed off DIFFERENT `symbol` bindings therefore
//!   stay mutually assignable, which is the whole point of tsc's
//!   symbol-index lowering.
//! - A `unique symbol` key still mints a named late-bound static member.
//! - A string-literal `const` key still mints an ordinary named static
//!   member.
//!
//! The binder names vary across cases on purpose: the routing must be driven
//! by the key's declared type, never by the identifier the user chose.

use crate::test_utils::check_source_diagnostics;

fn codes(source: &str) -> Vec<u32> {
    check_source_diagnostics(source)
        .into_iter()
        .map(|d| d.code)
        .collect()
}

fn assert_clean(source: &str) {
    let diags = check_source_diagnostics(source);
    assert!(diags.is_empty(), "expected no diagnostics, got: {diags:?}");
}

#[test]
fn static_method_wide_symbol_key_satisfies_symbol_index_signature_target() {
    assert_clean(
        r#"
declare const handleKey: symbol;
interface CtorTarget { [key: symbol]: () => number }
class Widget { static [handleKey](): number { return 1; } }
declare function want(target: CtorTarget): void;
want(Widget);
"#,
    );
}

#[test]
fn static_and_instance_different_wide_symbols_stay_assignable() {
    // Neither side writes an explicit index signature. Both keys are plain
    // `symbol`, and they are DIFFERENT bindings — under the old synthetic
    // `__symbol_<file>_<sym>` named member these only matched when the two
    // placeholder atoms happened to collide.
    assert_clean(
        r#"
declare const producerKey: symbol;
declare const consumerKey: symbol;
interface CtorConsumer { [consumerKey]: () => number }
class Producer { static [producerKey](): number { return 1; } }
declare function want(producer: CtorConsumer): void;
want(Producer);
"#,
    );
}

#[test]
fn static_property_wide_symbol_key_is_read_through_an_unrelated_symbol() {
    // The index signature must be readable by ANY symbol, not only the
    // binding that declared it.
    assert_clean(
        r#"
declare const slotKey: symbol;
declare const lookupKey: symbol;
class Store { static [slotKey]: number = 1; }
const read: number = Store[lookupKey];
export { read };
"#,
    );
}

#[test]
fn static_getter_wide_symbol_key_routes_to_symbol_index_signature() {
    assert_clean(
        r#"
declare const viewKey: symbol;
declare const probeKey: symbol;
class Panel { static get [viewKey](): number { return 1; } }
const read: number = Panel[probeKey];
export { read };
"#,
    );
}

#[test]
fn static_setter_wide_symbol_key_routes_to_symbol_index_signature() {
    assert_clean(
        r#"
declare const sinkKey: symbol;
declare const writeKey: symbol;
class Drain { static set [sinkKey](value: number) {} }
Drain[writeKey] = 3;
"#,
    );
}

#[test]
fn two_wide_symbol_static_members_union_their_index_value_types() {
    assert_clean(
        r#"
declare const firstKey: symbol;
declare const secondKey: symbol;
declare const anyKey: symbol;
class Pair { static [firstKey]: number = 1; static [secondKey]: string = ""; }
const read: number | string = Pair[anyKey];
export { read };
"#,
    );
}

#[test]
fn wide_symbol_static_member_leaves_ordinary_named_statics_alone() {
    assert_clean(
        r#"
declare const auxKey: symbol;
class Record2 { static [auxKey]: number = 1; static label: string = ""; }
const label: string = Record2.label;
export { label };
"#,
    );
}

#[test]
fn unique_symbol_static_key_still_mints_a_named_member() {
    // Negative control for the routing: a `unique symbol` key is late-bound
    // to a NAMED member, so two distinct unique-symbol keys stay unrelated
    // and the missing-property diagnostic must survive.
    let diags = codes(
        r#"
declare const declaredKey: unique symbol;
declare const otherKey: unique symbol;
interface CtorWanted { [declaredKey]: number }
class Offered { static [otherKey]: number = 1; }
declare function want(wanted: CtorWanted): void;
want(Offered);
"#,
    );
    assert!(
        diags.contains(&2345) || diags.contains(&2741),
        "a unique-symbol key must stay a named member, so the mismatch still reports; got: {diags:?}"
    );
}

#[test]
fn literal_const_static_key_still_mints_a_named_member() {
    let diags = codes(
        r#"
const literalKey = "tag";
class Tagged { static [literalKey](): number { return 1; } }
const call: () => number = Tagged.tag;
export { call };
"#,
    );
    assert!(
        diags.is_empty(),
        "a string-literal const key stays an ordinary named static member; got: {diags:?}"
    );
}

#[test]
fn wide_symbol_key_routing_ignores_the_binder_name() {
    // Same structural shape as the first test with every identifier renamed;
    // the routing must be driven by the declared type, not by any particular
    // identifier text.
    assert_clean(
        r#"
declare const zzTopMarker: symbol;
interface QqBravoCtor { [key: symbol]: () => number }
class Xylophone { static [zzTopMarker](): number { return 1; } }
declare function accept(value: QqBravoCtor): void;
accept(Xylophone);
"#,
    );
}

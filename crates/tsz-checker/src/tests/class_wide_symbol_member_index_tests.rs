//! Adjacent-case coverage for #16307's class leg: a class member whose
//! computed key resolves to a plain (non-unique) `symbol` binding contributes
//! a `[key: symbol]: V` index signature to the class instance shape rather
//! than a named member.
//!
//! Structural rule, verified against the pinned `tsc` 7.0.2 oracle:
//! - A wide-`symbol` key on a class property, method, getter or setter routes
//!   into the shape's symbol index signature, exactly as the object-literal
//!   and interface lowering paths already do. Two declarations keyed off
//!   DIFFERENT `symbol` bindings therefore stay mutually assignable, which is
//!   the whole point of tsc's symbol-index lowering.
//! - A `unique symbol` key still mints a named late-bound member, so a
//!   mismatch between two distinct `unique symbol` keys still reports.
//! - A string/number literal `const` key still mints an ordinary named member.
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
fn class_method_wide_symbol_key_satisfies_symbol_index_signature_target() {
    // The witness shape behind xstate's `class Actor implements
    // InteropObservable`: the target declares an explicit symbol index
    // signature, the class keys its member off an unrelated `symbol` binding.
    assert_clean(
        r#"
declare const handleKey: symbol;
interface Target { [key: symbol]: () => number }
class Widget { [handleKey](): number { return 1; } }
declare function want(target: Target): void;
declare const widget: Widget;
want(widget);
"#,
    );
}

#[test]
fn class_and_interface_keyed_off_different_wide_symbols_stay_assignable() {
    // Neither side writes an explicit index signature. Both keys are plain
    // `symbol`, and they are DIFFERENT bindings — under the old synthetic
    // `__symbol_<file>_<sym>` named member these only matched when the two
    // placeholder atoms happened to collide.
    assert_clean(
        r#"
declare const producerKey: symbol;
declare const consumerKey: symbol;
interface Producer { [consumerKey]: () => number }
class Consumer { [producerKey](): number { return 1; } }
declare function want(producer: Producer): void;
declare const consumer: Consumer;
want(consumer);
"#,
    );
}

#[test]
fn class_property_wide_symbol_key_is_read_through_an_unrelated_symbol() {
    // The index signature must be readable by ANY symbol, not only the
    // binding that declared it — that is what distinguishes a real index
    // signature from a late-bound named member.
    assert_clean(
        r#"
declare const slotKey: symbol;
declare const lookupKey: symbol;
class Store { [slotKey]: number = 1; }
declare const store: Store;
const read: number = store[lookupKey];
export { read };
"#,
    );
}

#[test]
fn class_getter_wide_symbol_key_routes_to_symbol_index_signature() {
    assert_clean(
        r#"
declare const viewKey: symbol;
declare const probeKey: symbol;
class Panel { get [viewKey](): number { return 1; } }
declare const panel: Panel;
const read: number = panel[probeKey];
export { read };
"#,
    );
}

#[test]
fn class_setter_wide_symbol_key_routes_to_symbol_index_signature() {
    assert_clean(
        r#"
declare const sinkKey: symbol;
declare const writeKey: symbol;
class Drain { set [sinkKey](value: number) {} }
declare const drain: Drain;
drain[writeKey] = 3;
"#,
    );
}

#[test]
fn two_wide_symbol_class_members_union_their_index_value_types() {
    // Several contributors widen the one index signature rather than each
    // minting a member of its own.
    assert_clean(
        r#"
declare const firstKey: symbol;
declare const secondKey: symbol;
declare const anyKey: symbol;
class Pair { [firstKey]: number = 1; [secondKey]: string = ""; }
declare const pair: Pair;
const read: number | string = pair[anyKey];
export { read };
"#,
    );
}

#[test]
fn two_wide_symbol_interface_members_union_their_index_value_types() {
    // #16307's own 2026-08-05T15:38Z comment recorded this shape as a still-open
    // gap: `merge_index_signature`'s duplicate-key collapse (meant for genuine
    // explicit `[k: symbol]: T` conflicts) was said to also swallow the
    // interface leg of the implicit wide-symbol union, reporting a false
    // TS7053 on read. Verified against pinned `typescript@7.0.2` (exit 0) and
    // against current main: this interface leg already routes through
    // `merge_implicit_symbol_index` (shared lowering, `signature_members.rs`)
    // and unions cleanly, matching the class leg's own coverage above. Pinning
    // it so the "still open" note stops drawing future sessions back to a gap
    // that closed without a matching regression test.
    assert_clean(
        r#"
declare const firstKey: symbol;
declare const secondKey: symbol;
interface T { [firstKey]: number; [secondKey]: string }
declare const anyKey: symbol;
declare const t: T;
const read: number | string = t[anyKey];
export { read };
"#,
    );
}

#[test]
fn two_wide_symbol_interface_members_union_with_renamed_binders() {
    // Same shape as above with different declaration order and binder names,
    // confirming the routing is driven by the key's declared `symbol` type
    // rather than by identifier spelling or source position.
    assert_clean(
        r#"
interface Holder { [beta]: string; [alpha]: number }
declare const alpha: symbol;
declare const beta: symbol;
declare const probe: symbol;
declare const holder: Holder;
const value: string | number = holder[probe];
export { value };
"#,
    );
}

#[test]
fn wide_symbol_class_member_leaves_ordinary_named_members_alone() {
    // The symbol index must not swallow string-named members, and `keyof`
    // must still surface them.
    assert_clean(
        r#"
declare const auxKey: symbol;
class Record2 { [auxKey]: number = 1; label: string = ""; }
declare const rec: Record2;
const label: string = rec.label;
type Keys = keyof Record2;
const key: Keys = "label";
export { label, key };
"#,
    );
}

#[test]
fn wide_symbol_index_signature_is_inherited_by_a_subclass() {
    assert_clean(
        r#"
declare const baseKey: symbol;
declare const probeKey: symbol;
class Origin { [baseKey](): number { return 1; } }
class Extended extends Origin { extra: number = 1; }
declare const extended: Extended;
const call: () => number = extended[probeKey];
export { call };
"#,
    );
}

#[test]
fn unique_symbol_class_key_still_mints_a_named_member() {
    // Negative control for the routing: a `unique symbol` key is late-bound to
    // a NAMED member, so two distinct unique-symbol keys stay unrelated and
    // the missing-property diagnostic must survive. If the wide-symbol routing
    // over-applied to unique symbols this would silently go clean.
    let diags = codes(
        r#"
declare const declaredKey: unique symbol;
declare const otherKey: unique symbol;
interface Wanted { [declaredKey]: number }
class Offered { [otherKey]: number = 1; }
declare function want(wanted: Wanted): void;
declare const offered: Offered;
want(offered);
"#,
    );
    assert!(
        diags.contains(&2345) || diags.contains(&2741),
        "a unique-symbol key must stay a named member, so the mismatch still reports; got: {diags:?}"
    );
}

#[test]
fn literal_const_class_key_still_mints_a_named_member() {
    // Negative control for the key-type test: a string-literal `const` key is
    // an ordinary named member, reachable by its literal name and NOT by an
    // arbitrary symbol.
    assert_clean(
        r#"
const literalKey = "tag";
class Tagged { [literalKey](): number { return 1; } }
declare const tagged: Tagged;
const call: () => number = tagged.tag;
export { call };
"#,
    );
}

#[test]
fn classifying_the_key_does_not_re_report_its_value_position_diagnostics() {
    // Regression guard for the routing's one hazard: classifying the key
    // evaluates its expression in VALUE position, and several value-position
    // diagnostics are suppressed only inside a computed-property-name context
    // (`is_in_ambient_computed_property_context`). An `abstract` member is
    // emit-free, so tsc reports nothing for its key; classifying without
    // publishing that context re-fired the diagnostic here.
    //
    // The cross-file `import type` form this mirrors is
    // `conformance/externalModules/typeOnly/computedPropertyName.ts`; the
    // single-file harness cannot resolve a module specifier, so the ambient
    // spelling stands in — both reach the same suppression.
    assert_clean(
        r#"
declare const hookKey: symbol;
declare class Ambient { [hookKey]: number; }
abstract class Partial2 { abstract [hookKey](): void; }
export { Partial2 };
"#,
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
interface QqBravo { [key: symbol]: () => number }
class Xylophone { [zzTopMarker](): number { return 1; } }
declare function accept(value: QqBravo): void;
declare const instance: Xylophone;
accept(instance);
"#,
    );
}

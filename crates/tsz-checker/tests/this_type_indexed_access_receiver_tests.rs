//! Regression coverage for #14166: an indexed access that reads a property whose
//! declared type references the polymorphic `this` must bind `this` to the
//! *receiver* the property is read from.
//!
//! Structural rule: when `interface I { return: this["args"] }` and the receiver
//! is an intersection `(I & { args: V })`, `tsc` substitutes `this` with that
//! receiver (`getTypeWithThisArgument`), so `(I & { args: V })["return"]` reduces
//! to `(I & { args: V })["args"]` = `V`. `tsz` previously left an unreduced
//! deferred `this["args"]`, which was neither assignable to nor identical with
//! its reduced form, producing a false `TS2322` and breaking `Equal`-style
//! identity tricks (the `Fn`/`Call`/`Apply` HKT core).
//!
//! The same rule binds a bare `this`-typed member (`self: this`) to the receiver,
//! and — read directly on the interface — resolves `this` through the receiver
//! `I` itself (so `I["return"]` is `I["args"]`, i.e. the declared constraint).
//!
//! Verified against `tsc` 6.x: every positive case exits 0; every negative
//! control keeps its `TS2322`.

use tsz_checker::test_utils::{check_source_strict, diagnostic_count};

fn ts2322(source: &str) -> usize {
    diagnostic_count(&check_source_strict(source), 2322)
}

/// The exact repro from #14166: `this["args"]` read through an intersection
/// receiver reduces to the concrete value supplied by the sibling member.
#[test]
fn this_indexed_access_binds_to_intersection_receiver() {
    let source = r#"
interface Fn { args: unknown; return: unknown; }
interface Identity extends Fn { return: this["args"]; }
type R = (Identity & { args: true })["return"];
const ok: true = null as any as R;
"#;
    assert_eq!(
        ts2322(source),
        0,
        "reduced `this[\"args\"]` must accept `true`"
    );
}

/// Negative control: the reduced value is `true`, so a `false` target still errors
/// (the access is genuinely reduced, not widened to `any`/`unknown`).
#[test]
fn reduced_this_indexed_access_still_rejects_wrong_literal() {
    let source = r#"
interface Fn { args: unknown; return: unknown; }
interface Identity extends Fn { return: this["args"]; }
type R = (Identity & { args: true })["return"];
const bad: false = null as any as R;
"#;
    assert_eq!(
        ts2322(source),
        1,
        "`true` must not be assignable to `false`"
    );
}

/// A non-literal value type flows through the same reduction.
#[test]
fn this_indexed_access_reduces_primitive_value() {
    let ok = r#"
interface Fn { args: unknown; return: unknown; }
interface Identity extends Fn { return: this["args"]; }
type R = (Identity & { args: string })["return"];
const ok: string = null as any as R;
"#;
    assert_eq!(ts2322(ok), 0, "reduced value `string` must accept `string`");

    let bad = r#"
interface Fn { args: unknown; return: unknown; }
interface Identity extends Fn { return: this["args"]; }
type R = (Identity & { args: string })["return"];
const bad: number = null as any as R;
"#;
    assert_eq!(
        ts2322(bad),
        1,
        "reduced value `string` must reject `number`"
    );
}

/// Anti-hardcoding: the rule is structural, not keyed on `Fn`/`Identity`/`args`.
/// Renamed binders behave identically.
#[test]
fn this_indexed_access_is_binder_name_invariant() {
    let ok = r#"
interface Hkt { input: unknown; output: this["input"]; }
type Run = (Hkt & { input: 7 })["output"];
const ok: 7 = null as any as Run;
"#;
    assert_eq!(ts2322(ok), 0, "renamed-binder reduction must accept `7`");

    let bad = r#"
interface Hkt { input: unknown; output: this["input"]; }
type Run = (Hkt & { input: 7 })["output"];
const bad: 8 = null as any as Run;
"#;
    assert_eq!(ts2322(bad), 1, "renamed-binder reduction must reject `8`");
}

/// A bare `this`-typed member (not `this[K]`) also binds to the receiver: the
/// member reads as the receiver intersection, so a follow-on key read sees the
/// sibling member's concrete value.
#[test]
fn bare_this_member_binds_to_intersection_receiver() {
    let ok = r#"
interface Builder { self: this; name: string; }
type S = (Builder & { name: "x" })["self"];
type N = S["name"];
const ok: "x" = null as any as N;
"#;
    assert_eq!(
        ts2322(ok),
        0,
        "`this`-typed `self` must carry the receiver's `name`"
    );

    let bad = r#"
interface Builder { self: this; name: string; }
type S = (Builder & { name: "x" })["self"];
type N = S["name"];
const bad: "y" = null as any as N;
"#;
    assert_eq!(ts2322(bad), 1, "receiver `name` is `\"x\"`, not `\"y\"`");
}

/// Read directly on the interface (no intersection), `this` binds to the
/// interface itself, so `this["args"]` resolves through `Identity`'s own
/// `args` (`unknown` here) rather than leaking a deferred `this[...]`.
#[test]
fn standalone_this_indexed_access_resolves_through_receiver_interface() {
    let source = r#"
interface Fn { args: unknown; return: unknown; }
interface Identity extends Fn { return: this["args"]; }
type Solo = Identity["return"];
const ok: unknown = null as any as Solo;
"#;
    assert_eq!(
        ts2322(source),
        0,
        "`Identity[\"return\"]` resolves to `unknown` and accepts an `unknown` target"
    );
}

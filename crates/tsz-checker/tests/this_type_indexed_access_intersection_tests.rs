//! Checker integration tests for reducing a `this`-relative property read off a
//! receiver formed by intersection.
//!
//! Structural rule: a property whose declared type is `this`-relative
//! (`return: this["args"]`) is stored with `this` unsubstituted. When the
//! receiver is a *merged* object — the shape formed from an intersection like
//! `Identity & { args: true }` — the merged member keeps the unbound `this`, and
//! there is no `this`-type in scope at index-evaluation time, so the access used
//! to leak a deferred `this["args"]` (neither assignable to nor identical to its
//! reduced form: spurious TS2322). tsc substitutes `this` with the concrete
//! receiver when reading a property off it, so `(Identity & { args: true })
//! ["return"]` reduces to `true`.
//!
//! Owner: `evaluate_index_access`'s `visit_object` arm
//! (`tsz_solver::evaluation::evaluate_rules::index_access`): when the looked-up
//! member type still contains `this`, substitute `this` with the object being
//! indexed.
//!
//! This is the hotscript `Call`/`Apply`/`Fn` HKT core (#14166). Cases vary
//! binder names and cover the bare and conditional `this[K]` forms, plus the
//! polymorphic-`this` method cases that must keep working.

use tsz_checker::test_utils::check_source_codes;

fn assert_no_errors(source: &str, label: &str) {
    let codes = check_source_codes(source);
    assert!(
        codes.is_empty(),
        "{label}: expected no diagnostics, got {codes:?}"
    );
}

// =============================================================================
// Positive: `this[K]` reduces against an intersection receiver
// =============================================================================

#[test]
fn this_indexed_access_reduces_over_intersection_receiver() {
    // The reported repro (#14166): tsc reduces to `true`; tsz left it deferred.
    assert_no_errors(
        r#"
interface Fn { args: unknown; return: unknown; }
interface Identity extends Fn { return: this["args"]; }
type R = (Identity & { args: true })["return"];
const ok: true = (null as any as R);
"#,
        "(Identity & { args: true })[\"return\"] reduces to true",
    );
}

#[test]
fn this_indexed_access_intersection_is_binder_name_independent() {
    assert_no_errors(
        r#"
interface Base { input: unknown; output: unknown; }
interface Echo extends Base { output: this["input"]; }
type Out = (Echo & { input: "x" })["output"];
const v: "x" = (null as any as Out);
"#,
        "renamed binders still reduce this-indexed access over intersection",
    );
}

#[test]
fn this_indexed_access_in_conditional_reduces_over_intersection() {
    // The actual hotscript HKT shape: `this["args"]` inside a conditional.
    assert_no_errors(
        r#"
interface Fn { args: unknown; return: unknown; }
interface Head extends Fn { return: this["args"] extends [infer H, ...any] ? H : never; }
type Applied = (Head & { args: [1, 2, 3] })["return"];
const h: 1 = (null as any as Applied);
"#,
        "conditional over this[\"args\"] reduces against intersection receiver",
    );
}

// =============================================================================
// Controls: plain-interface receivers and polymorphic `this` keep working
// =============================================================================

#[test]
fn this_indexed_access_plain_interface_receiver_still_reduces() {
    assert_no_errors(
        r#"
interface Direct { args: true; return: this["args"]; }
type R = Direct["return"];
const ok: true = (null as any as R);
"#,
        "Direct[\"return\"] reduces to true (plain interface receiver)",
    );
}

#[test]
fn this_indexed_access_resolves_to_member_type_without_intersection() {
    assert_no_errors(
        r#"
interface Fn { args: unknown; return: unknown; }
interface Identity extends Fn { return: this["args"]; }
type R = Identity["return"];
const ok: unknown = (null as any as R);
"#,
        "Identity[\"return\"] is unknown (this -> Identity, args: unknown)",
    );
}

#[test]
fn polymorphic_this_method_is_not_collapsed() {
    // The fix must not bake `this` into polymorphic-`this` method results.
    assert_no_errors(
        r#"
interface Box<T> { value: T; self(): this; }
declare const b: Box<number>;
const same = b.self();
const val: number = same.value;
type SelfFn = Box<string>["self"];
declare const f: SelfFn;
const rv: string = f().value;
"#,
        "polymorphic this method chaining still resolves the receiver",
    );
}

//! Regression tests for TS2445 enforcement when a free function declares a
//! `this` parameter typed as a subclass of the class that declares a
//! `protected` member.
//!
//! tsc allows protected access through a `this` expression whenever the `this`
//! type is the declaring class itself *or derives from it* (transitively).
//! tsz previously gated the free-function/contextual-`this` fallback in
//! `property_checker.rs` on exact class equality
//! (`receiver_class_idx == declaring_class_idx`), so an inherited protected
//! member reached through a subclass `this` type was wrongly rejected with
//! TS2445. The fallback now also accepts `is_class_derived_from`, mirroring the
//! `super.x` branch. Private members stay equality-gated (not inherited).

use crate::test_utils::check_source_diagnostics;

fn diag_codes(source: &str) -> Vec<u32> {
    check_source_diagnostics(source)
        .into_iter()
        .map(|d| d.code)
        .collect()
}

/// Inherited protected member reached through a direct-subclass `this` type is
/// allowed — no TS2445.
#[test]
fn no_ts2445_on_protected_access_via_subclass_this() {
    let source = "\
class Base { protected x = 1; }
class Derived extends Base { }
function f(this: Derived) { return this.x; }
";
    let codes = diag_codes(source);
    assert!(
        !codes.contains(&2445),
        "Protected access through `this: Derived` must not fire TS2445. Got: {codes:?}"
    );
}

/// Anti-hardcoding cover: renamed binders, same structural shape.
#[test]
fn no_ts2445_on_protected_access_via_subclass_this_renamed() {
    let source = "\
class Vehicle { protected speed = 0; }
class Car extends Vehicle { wheels = 4; }
function accelerate(this: Car) { return this.speed; }
";
    let codes = diag_codes(source);
    assert!(
        !codes.contains(&2445),
        "Renamed variant: protected access through subclass `this` must not fire TS2445. Got: {codes:?}"
    );
}

/// Multi-level inheritance: protected access through a grandchild `this` type is
/// allowed via transitive derivation.
#[test]
fn no_ts2445_on_protected_access_via_grandchild_this() {
    let source = "\
class A { protected v = 1; }
class B extends A { }
class C extends B { }
function g(this: C) { return this.v; }
";
    let codes = diag_codes(source);
    assert!(
        !codes.contains(&2445),
        "Protected access through a multi-level subclass `this` must not fire TS2445. Got: {codes:?}"
    );
}

/// The declaring-class `this` type was already accepted — keep it green.
#[test]
fn no_ts2445_on_protected_access_via_declaring_class_this() {
    let source = "\
class Base { protected x = 1; }
function f(this: Base) { return this.x; }
";
    let codes = diag_codes(source);
    assert!(
        !codes.contains(&2445),
        "Protected access through `this: Base` (declaring class) must not fire TS2445. Got: {codes:?}"
    );
}

/// Private members are *not* inherited: access through a subclass `this` type
/// must still report TS2341 (parity control — the widening is protected-only).
#[test]
fn ts2341_still_fires_on_private_access_via_subclass_this() {
    let source = "\
class P { private p = 1; }
class C extends P { }
function bad(this: C) { return this.p; }
";
    let codes = diag_codes(source);
    assert!(
        codes.contains(&2341),
        "Private access through subclass `this` must still fire TS2341. Got: {codes:?}"
    );
    assert!(
        !codes.contains(&2445),
        "Private access must report TS2341, not TS2445. Got: {codes:?}"
    );
}

// -----------------------------------------------------------------------
// A free function's `this: T` parameter is the accessibility context for
// EVERY receiver checked in its body, not only a bare `this.x` access.
// tsc's `checkPropertyAccessibility` walks the containing signature's `this`
// parameter for any receiver; tsz previously only special-cased a literal
// `this` receiver, so a sibling parameter's protected access (`arg.x`) fell
// through to "no enclosing class" and was always denied with TS2445.
// -----------------------------------------------------------------------

/// A same-hierarchy `arg` receiver is allowed when the function's own `this`
/// parameter derives from the declaring class — not just a `this.x` access.
#[test]
fn no_ts2445_on_protected_access_via_sibling_param_when_this_param_derives() {
    let source = "\
class A { protected a() {} }
class B extends A { protected b() {} }
function f<T extends B>(this: T, arg: B) {
  arg.a();
  arg.b();
}
";
    let codes = diag_codes(source);
    assert!(
        !codes.contains(&2445),
        "arg.a()/arg.b() must be allowed when `this: T extends B` derives from B. Got: {codes:?}"
    );
}

/// Cross-hierarchy: the function's `this` parameter derives from a sibling
/// class, not from the receiver's declaring class, so access is still denied.
#[test]
fn ts2445_fires_on_protected_access_via_sibling_param_cross_hierarchy() {
    let source = "\
class A { protected a() {} }
class B extends A { protected b() {} }
class C extends A { protected c() {} }
function f<T extends C>(this: T, arg: B) {
  arg.b();
}
";
    let codes = diag_codes(source);
    assert!(
        codes.contains(&2445),
        "arg.b() must still fire TS2445 when `this: T extends C` does not derive from B. Got: {codes:?}"
    );
}

/// A class member's own explicit `this: T` parameter narrows the
/// accessibility context past its declaring class: a subclass-only protected
/// member is reachable through a same-subclass sibling parameter even though
/// the enclosing method is declared on the (less-derived) base class.
#[test]
fn no_ts2445_on_protected_access_via_narrower_this_param_in_class_member() {
    let source = "\
class D { protected d() {} }
class D1 extends D { protected d1() {} }
class Container {
  m(this: D1, arg: D1) {
    arg.d1();
  }
}
";
    let codes = diag_codes(source);
    assert!(
        !codes.contains(&2445),
        "arg.d1() must be allowed: the member's own `this: D1` narrows past its declaring context. Got: {codes:?}"
    );
}

/// Renamed-binder control for the class-member narrowing case, keeping the
/// anti-hardcoding matrix honest.
#[test]
fn no_ts2445_on_protected_access_via_narrower_this_param_in_class_member_renamed() {
    let source = "\
class Vehicle { protected speed = 0; }
class Car extends Vehicle { protected wheels = 4; }
class Garage {
  inspect(this: Car, other: Car) {
    other.wheels;
  }
}
";
    let codes = diag_codes(source);
    assert!(
        !codes.contains(&2445),
        "Renamed variant: narrower `this: Car` must allow other.wheels. Got: {codes:?}"
    );
}

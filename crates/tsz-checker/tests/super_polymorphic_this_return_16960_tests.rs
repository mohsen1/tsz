//! Regression coverage for #16960: `super.method()` must preserve the base
//! method's polymorphic `this` return, binding it to the *enclosing (derived)*
//! class — exactly as a direct `this.method()` call would.
//!
//! Structural rule (matches `tsc`): when the receiver of a property access is
//! the `super` keyword, a base member whose type mentions the polymorphic
//! `this` (e.g. an inferred `() => this` return, or an explicit `: this`
//! annotation) binds that `this` to the *current* class's `this`, not to the
//! base class's own materialized instance type. The previous implementation
//! substituted with `current_this_type()` — the top of `this_type_stack` — which
//! during phase-2 method checking is a *partial prescan* instance type. For a
//! derived class that declares no own instance properties (only methods, which
//! are deferred), that partial type collapses toward the base, turning
//! `() => this` into `() => Base` and drawing a spurious `TS2339`/`TS2322`.
//!
//! All expectations below are oracle-verified against `typescript@7.0.2`.

use tsz_checker::test_utils::{check_source_code_messages, check_source_diagnostics};

/// The distilled #16960 repro: `returnThis()` forwards `super.returnThis()`,
/// whose base returns the inferred polymorphic `this`. `instance.returnThis()`
/// must be the *derived* class, so a derived-only member resolves cleanly.
#[test]
fn super_call_forwarding_inferred_this_binds_to_derived() {
    let source = r#"
class SomeBaseClass {
    returnThis() {
        return this;
    }
}

class SomeDerivedClass extends SomeBaseClass {
    returnThis() {
        return super.returnThis();
    }
    fn() {}
}

let instance = new SomeDerivedClass();
instance.returnThis().fn();
"#;
    let diagnostics = check_source_diagnostics(source);
    assert!(
        diagnostics.is_empty(),
        "expected clean (derived `this` carries `fn`); got: {:?}",
        check_source_code_messages(source)
    );
}

/// Negative control: a member that exists on *neither* class must still report
/// TS2339 — the fix must not widen `super`'s result to `any`.
#[test]
fn super_call_forwarding_this_absent_member_still_errors() {
    let source = r#"
class SomeBaseClass {
    returnThis() {
        return this;
    }
}

class SomeDerivedClass extends SomeBaseClass {
    returnThis() {
        return super.returnThis();
    }
    fn() {}
}

let instance = new SomeDerivedClass();
instance.returnThis().nope();
"#;
    let codes: Vec<u32> = check_source_diagnostics(source)
        .iter()
        .map(|d| d.code)
        .collect();
    assert!(
        codes.contains(&2339),
        "expected TS2339 for a genuinely-absent member; got: {:?}",
        check_source_code_messages(source)
    );
}

/// Base method returns `this` via an *explicit* `: this` annotation (not
/// inference). The same super-substitution must apply.
#[test]
fn super_call_explicit_this_annotation_binds_to_derived() {
    let source = r#"
class Base {
    self(): this {
        return this;
    }
}

class Derived extends Base {
    self(): this {
        return super.self();
    }
    fn() {}
}

let d = new Derived();
d.self().fn();
"#;
    let diagnostics = check_source_diagnostics(source);
    assert!(
        diagnostics.is_empty(),
        "explicit `: this` annotation must bind to derived; got: {:?}",
        check_source_code_messages(source)
    );
}

/// Multi-level inheritance: `C.m()` calls `super.m()` (B's), which itself calls
/// `super.m()` (A's). `this` must thread through as the *original* receiver
/// (`C`), not collapse at each hop.
#[test]
fn super_call_multi_level_threads_original_receiver() {
    let source = r#"
class A {
    m() {
        return this;
    }
}
class B extends A {
    m() {
        return super.m();
    }
}
class C extends B {
    m() {
        return super.m();
    }
    fn() {}
}

let c = new C();
c.m().fn();
"#;
    let diagnostics = check_source_diagnostics(source);
    assert!(
        diagnostics.is_empty(),
        "multi-level super chain must thread `this` to the original receiver; got: {:?}",
        check_source_code_messages(source)
    );
}

/// A base method whose return does NOT involve `this` must be unaffected: the
/// `super` call keeps the base's declared (non-`this`) return type.
#[test]
fn super_call_non_this_return_is_unaffected() {
    let source = r#"
class Base {
    make(): Base {
        return this;
    }
}
class Derived extends Base {
    make(): Base {
        return super.make();
    }
    fn() {}
}

let d = new Derived();
d.make().fn();
"#;
    // `make()` is annotated `: Base`, so the result is `Base`, which has no
    // `fn` — TS2339 in both tsc and tsz. The fix must not "helpfully" upgrade a
    // non-`this` return to the derived class.
    let codes: Vec<u32> = check_source_diagnostics(source)
        .iter()
        .map(|d| d.code)
        .collect();
    assert!(
        codes.contains(&2339),
        "a non-`this` annotated return must stay `Base`; got: {:?}",
        check_source_code_messages(source)
    );
}

/// Regression guard: a direct instance call (no `super`) of a `this`-returning
/// method already resolved to the derived class and must keep doing so.
#[test]
fn direct_instance_call_this_return_still_binds_to_receiver() {
    let source = r#"
class Base {
    returnThis() {
        return this;
    }
}
class Derived extends Base {
    fn() {}
}

let d = new Derived();
d.returnThis().fn();
"#;
    let diagnostics = check_source_diagnostics(source);
    assert!(
        diagnostics.is_empty(),
        "direct instance `this`-return must bind to the receiver; got: {:?}",
        check_source_code_messages(source)
    );
}

/// The `super`-substitution must not break `this`-typed *parameters* in the
/// override-forwarding shape (the case the original `super` special-case was
/// written for): `super.accept(this)` where the parameter is `this`. Kept
/// lib-free (no `Array`) so the check exercises only the `this`-binding path.
#[test]
fn super_call_this_param_override_forwarding_is_clean() {
    let source = r#"
class Base {
    accept(other: this): void {}
}
class Derived extends Base {
    accept(other: this): void {
        super.accept(this);
    }
}
"#;
    let diagnostics = check_source_diagnostics(source);
    assert!(
        diagnostics.is_empty(),
        "`super.accept(this)` with a `this` parameter must stay clean; got: {:?}",
        check_source_code_messages(source)
    );
}

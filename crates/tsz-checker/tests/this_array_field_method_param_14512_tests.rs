//! Regression coverage for #14512: passing the polymorphic `this` into a member
//! whose type still refers to that same `this` must not draw a spurious `TS2345`.
//!
//! Structural rule: when a property access `recv.member` reads a member whose
//! type references the polymorphic `this`, `tsc` substitutes `this` with the
//! receiver's instance type only when the receiver is a *concrete* anchor. When
//! the receiver type *itself* still contains the enclosing class's `this`
//! (`this.children: this[]`, `this.pair: [this, this]`, ...), the member's `this`
//! is already in the correct scope and must stay `this`. `tsz` previously
//! substituted `this` with the whole receiver type, turning the element `this` of
//! `Array<this>.push`/`indexOf` into `this[]` and rejecting a `this` argument.
//!
//! These cases exercise real lib array methods, so they load the default lib.
//! Verified against `tsc`: every positive case exits 0; the negative controls
//! keep their `TS2345`.

use std::sync::Arc;

use tsz_binder::lib_loader::LibFile;
use tsz_checker::diagnostics::Diagnostic;
use tsz_checker::test_utils::{
    check_source_with_libs, diagnostic_count, load_default_lib_files, strict_checker_options,
};

fn check(source: &str) -> Vec<Diagnostic> {
    let libs: Vec<Arc<LibFile>> = load_default_lib_files();
    check_source_with_libs(source, "test.ts", strict_checker_options(), &libs)
}

fn ts2345(source: &str) -> usize {
    diagnostic_count(&check(source), 2345)
}

fn ts2322(source: &str) -> usize {
    diagnostic_count(&check(source), 2322)
}

/// The exact repro from the issue: `this.children.push(c)` where
/// `children: this[]` and `c: this`.
#[test]
fn push_this_into_this_array_field_is_accepted() {
    let source = r#"
class TreeNode {
  children: this[] = [];
  addChild(c: this): void {
    this.children.push(c);
  }
}
"#;
    assert_eq!(
        ts2345(source),
        0,
        "push(c: this) into this[] must be accepted"
    );
}

/// `Array<this>` written as the generic application form behaves identically.
#[test]
fn push_this_into_array_of_this_application_form() {
    let source = r#"
class C {
  xs: Array<this> = [];
  add(c: this): void {
    this.xs.push(c);
  }
}
"#;
    assert_eq!(ts2345(source), 0);
}

/// Non-rest element-typed methods (`indexOf`/`includes`) take a single `T = this`
/// parameter and must also accept a `this` argument.
#[test]
fn non_rest_element_methods_accept_this() {
    let source = r#"
class C {
  xs: this[] = [];
  find(c: this): number {
    return this.xs.indexOf(c) + (this.xs.includes(c) ? 1 : 0);
  }
}
"#;
    assert_eq!(ts2345(source), 0);
}

/// `ReadonlyArray<this>` keeps the same element identity.
#[test]
fn readonly_this_array_indexof_accepts_this() {
    let source = r#"
class C {
  readonly xs: ReadonlyArray<this> = [];
  find(c: this): number {
    return this.xs.indexOf(c);
  }
}
"#;
    assert_eq!(ts2345(source), 0);
}

/// A `this`-valued getter feeds back into the same `this[]` field.
#[test]
fn getter_returning_this_pushes_into_this_array() {
    let source = r#"
class C {
  xs: this[] = [];
  get self(): this { return this; }
  m(): void { this.xs.push(this.self); }
}
"#;
    assert_eq!(ts2345(source), 0);
}

/// A subclass overriding the method (and calling `super`) keeps `this` identity.
#[test]
fn subclass_override_keeps_this_identity() {
    let source = r#"
class TreeNode {
  children: this[] = [];
  addChild(c: this): void { this.children.push(c); }
}
class Sub extends TreeNode {
  addChild(c: this): void {
    this.children.push(c);
    super.addChild(c);
  }
}
"#;
    assert_eq!(ts2345(source), 0);
}

/// A tuple field `[this, this]` is also a receiver that still contains `this`;
/// writing a `this` element through it must not be rewritten.
#[test]
fn this_tuple_field_index_assign_accepts_this() {
    let source = r#"
class C {
  pair: [this, this] = [this, this];
  m(c: this): void { this.pair[0] = c; }
}
"#;
    assert_eq!(
        ts2322(source),
        0,
        "assigning `this` into a `[this, this]` slot must be accepted"
    );
}

/// Negative control: pushing a *different* class instance into `this[]` must
/// still be rejected — the element parameter is `this`, not widened to `any`.
#[test]
fn pushing_unrelated_instance_still_errors() {
    let source = r#"
class A { tag: "a" = "a"; }
class C {
  xs: this[] = [];
  add(): void { this.xs.push(new A()); }
}
"#;
    assert_eq!(
        ts2345(source),
        1,
        "an unrelated instance must not be assignable to the `this` element"
    );
}

/// Negative control from outside the class: `node.children` resolves `this` to
/// the concrete instance type, so pushing an unrelated instance still errors.
#[test]
fn pushing_unrelated_instance_from_outside_still_errors() {
    let source = r#"
class A { tag: "a" = "a"; }
class C { children: C[] = []; m(): void {} }
const n = new C();
n.children.push(new A());
"#;
    // From outside, the receiver resolves `this` to the CONCRETE `C`, so tsc
    // 7.0.2 promotes the missing-property head: TS2739 (children, m), not the
    // generic TS2345 kept for polymorphic `this` targets.
    assert_eq!(diagnostic_count(&check(source), 2739), 1);
}

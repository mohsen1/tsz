//! A value whose computed type is the polymorphic `this` must be assignable to
//! a `this`-typed property (`this <: this` within one class).
//!
//! tsz's polymorphic-`this` property-assignment special-case
//! (`check_polymorphic_this_property_assignment`) only recognized a RHS that
//! was *syntactically* the `this` keyword or a `this.prop` access, so a binding
//! whose TYPE is `this` — a parameter `c: this`, a `this`-returning method, a
//! `const t: this` — drew a spurious TS2322 ("Type 'this' is not assignable to
//! type 'this'"). The fix also accepts the RHS when its computed `right_type`
//! is the canonical `ThisType`, while preserving the genuine rejection of a
//! concrete instance or base-type value assigned into a `this`-typed property.
//!
//! Owner: `crates/tsz-checker/src/assignability/assignment_checker/assignment_ops.rs`.
//! Found during the #14512 investigation (distinct mechanism from #14512/#14516,
//! which is the `Array<this>.push` receiver path).

use tsz_checker::test_utils::check_source_codes;

fn ts2322(source: &str) -> usize {
    check_source_codes(source)
        .into_iter()
        .filter(|&c| c == 2322)
        .count()
}

/// Positive: a parameter typed `this` assigned into a `this`-typed field.
#[test]
fn param_typed_this_assigns_into_this_field() {
    let source = r#"
class Node {
  link!: this;
  attach(other: this): void {
    this.link = other;
  }
}
"#;
    assert_eq!(
        ts2322(source),
        0,
        "a `this`-typed parameter must assign into a `this`-typed field: {:?}",
        check_source_codes(source)
    );
}

/// Positive: a method whose return type is `this` produces a this-typed value.
/// Renamed binders keep the rule structural, not identifier-driven.
#[test]
fn this_returning_method_result_assigns_into_this_field() {
    let source = r#"
class Cell {
  partner!: this;
  itself(): this { return this; }
  pair(): void {
    this.partner = this.itself();
  }
}
"#;
    assert_eq!(
        ts2322(source),
        0,
        "a `this`-returning method's result must assign into a `this`-typed field: {:?}",
        check_source_codes(source)
    );
}

/// Positive: a local binding annotated `this`.
#[test]
fn const_binding_typed_this_assigns_into_this_field() {
    let source = r#"
class Wrapper {
  inner!: this;
  store(): void {
    const me: this = this;
    this.inner = me;
  }
}
"#;
    assert_eq!(
        ts2322(source),
        0,
        "a `this`-typed local binding must assign into a `this`-typed field: {:?}",
        check_source_codes(source)
    );
}

/// Negative control: a concrete unrelated instance must STILL be rejected.
#[test]
fn concrete_instance_into_this_field_still_errors() {
    let source = r#"
class Foreign {}
class Holder {
  slot!: this;
  put(value: Foreign): void {
    this.slot = value;
  }
}
"#;
    assert!(
        ts2322(source) >= 1,
        "a concrete unrelated instance must not assign into a `this`-typed field: {:?}",
        check_source_codes(source)
    );
}

/// Negative control: a base-type value (the class's own base, not `this`) must
/// STILL be rejected — `this` is narrower than the base instance type.
#[test]
fn base_type_value_into_this_field_still_errors() {
    let source = r#"
class Shape { sides = 0; }
class Square extends Shape {
  twin!: this;
  set(value: Shape): void {
    this.twin = value;
  }
}
"#;
    assert!(
        ts2322(source) >= 1,
        "a base-type value must not assign into a derived `this`-typed field: {:?}",
        check_source_codes(source)
    );
}

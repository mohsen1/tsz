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

use tsz_checker::test_utils::{
    DiagnosticShape, assert_diagnostic_shape, check_source_codes, check_source_diagnostics,
};

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

/// Negative control (the `typeRelationships.ts` conformance case): a concrete
/// field whose declared type is the class — not `this` — must STILL be rejected
/// when assigned into a `this`-typed field, EVEN AFTER it was flow-narrowed to
/// `this` by an earlier `field = <this-typed value>`. tsc relates the read
/// against the field's *declared* type (`Holder`), which is wider than `this`.
/// Acceptance keyed on the RHS's *current* (flow-narrowed) `this` type would
/// wrongly suppress this error — the fix excludes property-access RHS from the
/// type-based acceptance for exactly this reason.
#[test]
fn flow_narrowed_concrete_field_into_this_field_still_errors() {
    let source = r#"
class Holder {
  mirror = this;
  peer = new Holder();
  swap(): void {
    this.peer = this.mirror;
    this.mirror = this.peer;
  }
}
"#;
    assert!(
        ts2322(source) >= 1,
        "a concrete field flow-narrowed to `this` must still be rejected against a `this`-typed field: {:?}",
        check_source_codes(source)
    );
}

/// A `this`-returning accessor read through a property access stays accepted —
/// its property's *declared* type is `this`, recognized by the syntactic path,
/// so the property-access exclusion (which targets flow-narrowed concrete
/// fields) does not regress this genuine positive.
#[test]
fn this_returning_getter_property_access_assigns_into_this_field() {
    let source = r#"
class Loop {
  next!: this;
  get current(): this { return this; }
  advance(): void {
    this.next = this.current;
  }
}
"#;
    assert_eq!(
        ts2322(source),
        0,
        "a `this`-returning accessor read via property access must assign into a `this`-typed field: {:?}",
        check_source_codes(source)
    );
}

#[test]
fn conformance_this_property_read_reports_class_source_at_lhs() {
    let source = "// @target: es2015
class C {
    self = this;
    c = new C();
    foo() {
        return this;
    }
    f1() {
        this.c = this.self;
        this.self = this.c;  // Error
    }
}
";
    let diagnostics = check_source_diagnostics(source);
    assert_diagnostic_shape(
        source,
        &diagnostics,
        &DiagnosticShape::code(2322)
            .at(10, 9)
            .with_message_fragment("Type 'C' is not assignable to type 'this'."),
    );
}

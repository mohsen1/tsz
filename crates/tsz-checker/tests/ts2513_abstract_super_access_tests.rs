//! Locks in TS2513 ("Abstract method 'X' in class 'Y' cannot be accessed via
//! super expression.") for `super.member` access where the member resolved by
//! the super receiver's class chain is declared `abstract`.
//!
//! An abstract member has no base implementation to dispatch to, so referencing
//! it through `super.` is always an error, regardless of how many further
//! accesses follow (method call, accessor read) and regardless of the member or
//! class spelling. Abstractness is decided by the nearest base class that
//! declares the member: a concrete override in a closer base suppresses it.
//!
//! Regression: issue #9677.

use tsz_checker::test_utils::check_source_codes;

#[test]
fn ts2513_fires_for_abstract_method_via_super() {
    let source = r#"
abstract class A { abstract m(): void; }
class B extends A { m() { super.m(); } }
"#;
    let codes = check_source_codes(source);
    assert!(
        codes.contains(&2513),
        "abstract method via super should trigger TS2513; got {codes:?}",
    );
}

#[test]
fn ts2513_fires_with_renamed_member_and_class() {
    // Different member/class spellings prove the rule is structural, not keyed
    // on a particular identifier.
    let source = r#"
abstract class Shape { abstract area(): number; }
class Circle extends Shape { area() { return super.area(); } }
"#;
    let codes = check_source_codes(source);
    assert!(
        codes.contains(&2513),
        "renamed abstract method via super should trigger TS2513; got {codes:?}",
    );
}

#[test]
fn ts2513_fires_for_abstract_getter_via_super() {
    let source = r#"
abstract class A { abstract get x(): number; }
class B extends A { get x() { return super.x; } }
"#;
    let codes = check_source_codes(source);
    assert!(
        codes.contains(&2513),
        "abstract accessor via super should trigger TS2513; got {codes:?}",
    );
}

#[test]
fn ts2513_fires_for_abstract_setter_via_super() {
    let source = r#"
abstract class A { abstract set x(v: number); }
class B extends A { set x(v: number) { super.x = v; } }
"#;
    let codes = check_source_codes(source);
    assert!(
        codes.contains(&2513),
        "abstract setter via super should trigger TS2513; got {codes:?}",
    );
}

#[test]
fn ts2513_fires_for_abstract_field_via_super() {
    // tsc prioritizes the abstract-via-super rule over the field-via-super
    // diagnostic (TS2855): an abstract instance field accessed through `super`
    // reports TS2513, not TS2855.
    let source = r#"
abstract class A { abstract x: number; }
class B extends A { x = 1; m() { return super.x; } }
"#;
    let codes = check_source_codes(source);
    assert!(
        codes.contains(&2513),
        "abstract field via super should trigger TS2513; got {codes:?}",
    );
    assert!(
        !codes.contains(&2855),
        "abstract field via super must not fall back to TS2855; got {codes:?}",
    );
}

#[test]
fn ts2513_fires_through_intermediate_non_declaring_base() {
    // `super` in C resolves to B; B does not declare `m`, so the chain walk
    // reaches A's abstract declaration.
    let source = r#"
abstract class A { abstract m(): void; }
abstract class B extends A {}
class C extends B { m() { super.m(); } }
"#;
    let codes = check_source_codes(source);
    assert!(
        codes.contains(&2513),
        "abstract method inherited through a non-declaring base should trigger TS2513; got {codes:?}",
    );
}

#[test]
fn ts2513_fires_for_generic_abstract_class() {
    let source = r#"
abstract class Box<T> { abstract get(): T; }
class NumBox extends Box<number> { get() { return super.get(); } }
"#;
    let codes = check_source_codes(source);
    assert!(
        codes.contains(&2513),
        "abstract method on a generic base via super should trigger TS2513; got {codes:?}",
    );
}

#[test]
fn ts2513_not_fired_for_non_abstract_method_via_super() {
    let source = r#"
class A { m(): void {} }
class B extends A { m() { super.m(); } }
"#;
    let codes = check_source_codes(source);
    assert!(
        !codes.contains(&2513),
        "concrete base method via super must not trigger TS2513; got {codes:?}",
    );
}

#[test]
fn ts2513_not_fired_when_nearer_base_overrides_concretely() {
    // `super` in C resolves to B, which provides a concrete `m`; the abstract
    // declaration in A is shadowed and `super.m` dispatches to B's version.
    let source = r#"
abstract class A { abstract m(): void; }
class B extends A { m() {} }
class C extends B { m() { super.m(); } }
"#;
    let codes = check_source_codes(source);
    assert!(
        !codes.contains(&2513),
        "concrete override in a nearer base must suppress TS2513; got {codes:?}",
    );
}

#[test]
fn ts2513_not_fired_for_element_access_super() {
    // tsc applies the abstract-via-super rule only to `super.x` property access,
    // not to the `super["x"]` element-access form.
    let source = r#"
abstract class A { abstract m(): void; }
class B extends A { m() { super["m"](); } }
"#;
    let codes = check_source_codes(source);
    assert!(
        !codes.contains(&2513),
        "element-access super must not trigger TS2513; got {codes:?}",
    );
}

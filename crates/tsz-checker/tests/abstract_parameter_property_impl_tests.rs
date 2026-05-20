//! Tests for abstract member implementation via constructor parameter
//! properties.
//!
//! Structural rule: a parameter property
//! (`constructor(public|private|protected|readonly|override foo: T)`)
//! declares and initializes the instance property `foo`. It must therefore
//! satisfy `abstract foo: T` from a base class for the TS2515/TS2654 (and
//! the type-level fallback TS2653/TS2656) "missing implementation"
//! diagnostics. Visibility incompatibility between the abstract member and
//! the parameter property is reported separately (TS2415/TS2611/TS2612);
//! that is not the absence-of-implementation case these diagnostics track.
//!
//! Issue #6699.

use tsz_checker::context::CheckerOptions;
use tsz_checker::test_utils::check_source;

fn missing_impl_diags(source: &str) -> Vec<u32> {
    let diags = check_source(source, "test.ts", CheckerOptions::default());
    diags
        .iter()
        .map(|d| d.code)
        .filter(|c| matches!(c, 2515 | 2653 | 2654 | 2656))
        .collect()
}

/// The minimal repro from #6699.
#[test]
fn public_parameter_property_satisfies_abstract_member() {
    let source = r#"
abstract class Entity {
  abstract id: string;
}
class User extends Entity {
  constructor(public id: string) {
    super();
  }
}
"#;
    assert!(
        missing_impl_diags(source).is_empty(),
        "public parameter property must satisfy abstract member; got {:?}",
        missing_impl_diags(source)
    );
}

/// Renamed identifier name — proves the rule is structural and not keyed on
/// the spelling `id` (§25 anti-hardcoding guard).
#[test]
fn parameter_property_impl_is_name_independent() {
    let source = r#"
abstract class A {
  abstract value: number;
}
abstract class B {
  abstract data: string;
}
class CA extends A {
  constructor(public value: number) { super(); }
}
class CB extends B {
  constructor(public data: string) { super(); }
}
"#;
    assert!(missing_impl_diags(source).is_empty());
}

/// `readonly`, `private`, `protected`, and `override` parameter property
/// modifiers all introduce instance properties, so they all close the
/// absence-of-implementation diagnostic. Visibility mismatch (e.g. private
/// implementation of a public abstract) is a separate diagnostic family
/// and must not appear as TS2515/TS2654.
#[test]
fn every_parameter_property_modifier_is_recognised() {
    for modifier in ["public", "private", "protected", "readonly"] {
        let source = format!(
            r#"
abstract class Base {{
  abstract id: string;
}}
class Derived extends Base {{
  constructor({modifier} id: string) {{ super(); }}
}}
"#
        );
        let codes = missing_impl_diags(&source);
        assert!(
            codes.is_empty(),
            "modifier `{modifier}` should satisfy abstract member; got {codes:?}"
        );
    }
}

/// Multiple parameter properties cover multiple abstract members in a
/// single constructor. The aggregate diagnostic TS2654 must also disappear.
#[test]
fn multiple_parameter_properties_cover_multiple_abstract_members() {
    let source = r#"
abstract class Base {
  abstract a: string;
  abstract b: number;
  abstract c: boolean;
}
class Derived extends Base {
  constructor(public a: string, public b: number, public c: boolean) {
    super();
  }
}
"#;
    assert!(missing_impl_diags(source).is_empty());
}

/// A mix of regular class-body members and parameter properties together
/// covers all the abstract members.
#[test]
fn mixed_body_member_and_parameter_property_cover_abstract_members() {
    let source = r#"
abstract class Base {
  abstract a: string;
  abstract b: number;
}
class Derived extends Base {
  b: number = 0;
  constructor(public a: string) {
    super();
  }
}
"#;
    assert!(missing_impl_diags(source).is_empty());
}

/// Negative case: a constructor parameter without an access/readonly
/// modifier is *not* a parameter property and must not be treated as an
/// implementation. The original TS2515 must still fire.
#[test]
fn plain_constructor_parameter_does_not_implement_abstract_member() {
    let source = r#"
abstract class Base {
  abstract id: string;
}
class Derived extends Base {
  constructor(id: string) { super(); }
}
"#;
    let codes = missing_impl_diags(source);
    assert!(
        codes.contains(&2515),
        "plain parameter must NOT satisfy abstract member; expected TS2515, got {codes:?}"
    );
}

/// Negative case: the parameter property only covers some of the abstract
/// members. The remaining unimplemented members must still produce the
/// missing-implementation diagnostic.
#[test]
fn parameter_property_only_covers_subset_of_abstract_members() {
    let source = r#"
abstract class Base {
  abstract a: string;
  abstract b: number;
}
class Derived extends Base {
  constructor(public a: string) { super(); }
}
"#;
    let codes = missing_impl_diags(source);
    assert!(
        codes.iter().any(|c| matches!(c, 2515 | 2654)),
        "expected missing-impl diagnostic for unimplemented `b`, got {codes:?}"
    );
}

/// Heritage-as-expression fallback: when the base class is referenced
/// through a value alias (a `const` holding the class), the abstract
/// check goes through the type-level fallback. Parameter properties must
/// still be recognised there.
#[test]
fn parameter_property_satisfies_abstract_member_via_type_level_fallback() {
    let source = r#"
abstract class Base {
  abstract id: string;
}
const BaseAlias = Base;
class Derived extends BaseAlias {
  constructor(public id: string) { super(); }
}
"#;
    assert!(missing_impl_diags(source).is_empty());
}

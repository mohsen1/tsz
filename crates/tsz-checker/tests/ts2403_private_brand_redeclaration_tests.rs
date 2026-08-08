//! Regression coverage: TS2403 ("Subsequent variable declarations must have
//! the same type") must respect TypeScript's nominal private/protected brand
//! rule, not just plain structural identity.
//!
//! `are_types_identical_for_redeclaration` (`crates/tsz-solver/src/relations/
//! compat_overrides.rs`) previously delegated straight to the raw structural
//! (Judge) bidirectional-subtype relation, which knows nothing about private
//! member branding — that rule lives in `private_brand_assignability_override`,
//! otherwise reached only through the ordinary assignability ("Lawyer") path.
//! Two classes each declaring their own `private` member of the same name are
//! bidirectional structural subtypes of one another (private members are
//! erased from the raw structural comparison) yet tsc's `isTypeIdenticalTo`
//! still treats them as distinct declaration surfaces, exactly like ordinary
//! assignability does. Without the fix, `var x: A.Foo; var x: B.Foo;` for two
//! unrelated `Foo` declarations that each carry a private member silently
//! dropped TS2403.
//!
//! All expected diagnostics oracle-verified against pinned `typescript@7.0.2`.
//!
//! Note: the conformance fixture `compiler/propertyIdentityWithPrivacyMismatch.ts`
//! has a second, SAME-simple-name redeclaration (`var x: m1.Foo; var x: m2.Foo;`)
//! that this fix does not resolve — `are_types_identical_for_redeclaration` itself
//! correctly returns "not identical" for that pair (verified directly), but no
//! diagnostic is emitted. That is a distinct bug downstream in the checker's
//! emission path, not the private-brand identity gap fixed here. See #16888.

use tsz_checker::test_utils::check_source_diagnostics;

fn ts2403_count(source: &str) -> usize {
    check_source_diagnostics(source)
        .iter()
        .filter(|d| d.code == 2403)
        .count()
}

#[test]
fn cross_namespace_private_brand_mismatch_renamed_binders_emits_ts2403() {
    // Renamed-binder variant of the above: differently-named classes that are
    // otherwise structurally identical still conflict, because the private
    // member brand — not the class name — drives identity here.
    let source = r#"
namespace A { export class Foo { private m: number = 0; } }
namespace B { export class Bar { private m: number = 0; } }
var x: A.Foo;
var x: B.Bar;
"#;
    assert_eq!(
        ts2403_count(source),
        1,
        "Expected TS2403: private brand mismatch must fire regardless of the \
         declaring classes' names"
    );
}

#[test]
fn same_class_via_type_alias_is_not_a_brand_mismatch() {
    // Negative: an alias to the SAME declaration must not trip the new
    // private-brand check — only genuinely distinct declarations conflict.
    let source = r#"
namespace A { export class Foo { private n: number = 0; } }
type FooAlias = A.Foo;
var x: A.Foo;
var x: FooAlias;
"#;
    assert_eq!(
        ts2403_count(source),
        0,
        "Expected no TS2403: FooAlias resolves to the identical A.Foo declaration"
    );
}

#[test]
fn protected_member_hierarchy_widening_is_not_a_brand_mismatch() {
    // Negative: `protected` is hierarchical, not exact-declaration nominal —
    // a subclass that inherits (does not redeclare) the protected member
    // stays redeclaration-identical to its base, matching
    // `private_brand_assignability_override`'s existing protected handling.
    let source = r#"
class Base { protected m: number = 0; }
class Derived extends Base {}
var x: Base;
var x: Derived;
"#;
    assert_eq!(
        ts2403_count(source),
        0,
        "Expected no TS2403: Derived inherits (does not redeclare) Base's \
         protected member, so the hierarchical protected rule allows it"
    );
}

#[test]
fn same_declaration_reused_with_private_member_is_not_a_brand_mismatch() {
    // Negative: redeclaring against the SAME class declaration twice must
    // stay clean even when it carries a private member — the fast
    // physical-identity path (`a == b`) already covers this, this test
    // guards against the new check regressing it.
    let source = r#"
class A { private n: number = 0; }
var x: A;
var x: A;
"#;
    assert_eq!(
        ts2403_count(source),
        0,
        "Expected no TS2403: both declarations reference the same class A"
    );
}

#[test]
fn structurally_identical_public_classes_without_private_members_unaffected() {
    // Negative control: without any private/protected members, two
    // differently-named but structurally identical classes remain
    // redeclaration-identical (purely structural comparison, unaffected by
    // the new private-brand check).
    let source = r#"
namespace A { export class Foo { m: number = 0; } }
namespace B { export class Bar { m: number = 0; } }
var x: A.Foo;
var x: B.Bar;
"#;
    assert_eq!(
        ts2403_count(source),
        0,
        "Expected no TS2403: neither class has a private/protected member, so \
         plain structural identity applies"
    );
}

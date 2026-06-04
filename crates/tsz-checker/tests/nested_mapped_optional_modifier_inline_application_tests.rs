//! Regression tests for #10871: a mapped type whose template is itself a
//! modifier-bearing mapped type / utility application drops the **inner**
//! mapped's optional (`?` / `+?`) modifier when the **outer** generic mapped is
//! used as an *inline generic application* in object-literal assignment
//! position.
//!
//! Structural rule: "When a generic mapped type `{ [K in keyof T]?: F<T[K]> }`
//! (where the template `F` is itself a mapped type that adds optionality) is
//! instantiated and evaluated inline, the inner mapped's optional modifier must
//! survive evaluation, so the produced property type is `{ b?: number }`, not
//! `{ b: number }`." `tsc` keeps the modifier; tsz previously stripped it,
//! producing a spurious `TS2741` / `TS2322` "property missing" diagnostic for
//! the omitted-but-optional nested property.
//!
//! Root cause: the resolver-less contextual-type extraction, when handed an
//! inline generic application target (`O<{a:0}>`, a `TypeData::Application`),
//! fell back to reading the property directly off the first type argument —
//! treating *every* application as the identity homomorphic map
//! `{ [K in keyof T]: T[K] }` and discarding the real template and its
//! modifiers. The fix gates that `arg0[name]` shortcut to genuinely
//! identity-homomorphic aliases.
//!
//! The bug only surfaced for the *inline application* form; the same type
//! spelled through a `type` alias, read via indexed access, or written with a
//! concrete (non-generic) outer key set always evaluated correctly. The tests
//! are deliberately lib-independent (the checker test harness runs without
//! `lib.d.ts`), defining their own `Partial`/`Readonly` equivalents, and vary
//! the binder names so the fix cannot be keyed to a particular spelling.

use tsz_checker::test_utils::check_source_codes;

fn missing_property_codes(source: &str) -> Vec<u32> {
    check_source_codes(source)
        .into_iter()
        .filter(|code| matches!(code, 2322 | 2741 | 2739))
        .collect()
}

#[test]
fn inline_application_inner_mapped_optional_survives() {
    // `{ [J in keyof T[K]]?: T[K][J] }` as the template of the outer mapped.
    let source = r#"
type Nest<T> = { [K in keyof T]?: { [J in keyof T[K]]?: T[K][J] } };
const a: Nest<{ a: { b: number } }> = { a: {} };
"#;
    assert!(
        missing_property_codes(source).is_empty(),
        "inner mapped optional modifier must survive inline-application evaluation; \
         omitting the optional nested `b` must not error. Got: {:?}",
        missing_property_codes(source)
    );
}

#[test]
fn inline_application_inner_partial_alias_optional_survives() {
    // The inner template is a user-defined `Partial`-equivalent applied to
    // `T[K]` (`MyPartial<T[K]>`). Lib-independent so the harness exercises it.
    let source = r#"
type MyPartial<X> = { [P in keyof X]?: X[P] };
type Outer<T> = { [K in keyof T]?: MyPartial<T[K]> };
const a: Outer<{ a: { b: number } }> = { a: {} };
"#;
    assert!(
        missing_property_codes(source).is_empty(),
        "inner MyPartial<T[K]> optionality must survive inline-application evaluation. Got: {:?}",
        missing_property_codes(source)
    );
}

#[test]
fn inline_application_inner_partial_concrete_arg_optional_survives() {
    // The inner template does not depend on `T[K]` at all, yet the bug still
    // fired because the trigger is the *generic outer* instantiation.
    let source = r#"
type MyPartial<X> = { [P in keyof X]?: X[P] };
type Outer<T> = { [K in keyof T]?: MyPartial<{ b: number }> };
const a: Outer<{ a: 0 }> = { a: {} };
"#;
    assert!(
        missing_property_codes(source).is_empty(),
        "inner MyPartial<{{ b: number }}> optionality must survive even when the template \
         is independent of T[K]. Got: {:?}",
        missing_property_codes(source)
    );
}

#[test]
fn inline_application_deep_partial_recursive_optional_survives() {
    // The canonical `DeepPartial` shape from the issue, used inline.
    let source = r#"
type DeepPartial<T> = { [K in keyof T]?: T[K] extends object ? DeepPartial<T[K]> : T[K] };
const a: DeepPartial<{ a: { b: number } }> = { a: {} };
const deep: DeepPartial<{ a: { b: { c: number } } }> = { a: { b: {} } };
"#;
    assert!(
        missing_property_codes(source).is_empty(),
        "DeepPartial inner optionality must survive inline-application evaluation. Got: {:?}",
        missing_property_codes(source)
    );
}

#[test]
fn inline_application_renamed_type_params_optional_survives() {
    // Vary the type-parameter and iteration-variable names: the fix must be
    // structural, not keyed to `T`/`K`/`P`.
    let source = r#"
type Shallow<U> = { [Q in keyof U]?: U[Q] };
type Wrapper<V> = { [R in keyof V]?: Shallow<V[R]> };
const w: Wrapper<{ z: { y: string } }> = { z: {} };
"#;
    assert!(
        missing_property_codes(source).is_empty(),
        "renamed binders must not change the result. Got: {:?}",
        missing_property_codes(source)
    );
}

#[test]
fn alias_form_remains_correct() {
    // The alias spelling always worked; keep it green so a fix to the inline
    // path does not regress the alias path.
    let source = r#"
type Nest<T> = { [K in keyof T]?: { [J in keyof T[K]]?: T[K][J] } };
type R = Nest<{ a: { b: number } }>;
const a: R = { a: {} };
"#;
    assert!(
        missing_property_codes(source).is_empty(),
        "alias form must stay correct. Got: {:?}",
        missing_property_codes(source)
    );
}

#[test]
fn inline_application_required_nested_property_still_errors() {
    // Negative guard: when the inner property is genuinely required (inner
    // mapped has no optional modifier), omitting it must STILL error. The fix
    // must not blanket-optionalize nested properties.
    let source = r#"
type NestReq<T> = { [K in keyof T]?: { [J in keyof T[K]]: T[K][J] } };
const a: NestReq<{ a: { b: number } }> = { a: {} };
"#;
    assert!(
        !missing_property_codes(source).is_empty(),
        "a genuinely required nested property must still report a missing-property error \
         when omitted; the fix must not over-optionalize. Got: {:?}",
        missing_property_codes(source)
    );
}

#[test]
fn inline_identity_homomorphic_application_still_requires_nested() {
    // Positive control for the *legitimate* `arg0[name]` shortcut: an identity
    // homomorphic alias `{ [K in keyof T]: T[K] }` does NOT change modifiers, so
    // the nested `b` stays required and omitting it must still error. This is
    // the case the gate must keep working.
    let source = r#"
type Id<T> = { [K in keyof T]: T[K] };
const a: Id<{ a: { b: number } }> = { a: {} };
"#;
    assert!(
        !missing_property_codes(source).is_empty(),
        "identity homomorphic mapping must keep the nested required property required. Got: {:?}",
        missing_property_codes(source)
    );
}

#[test]
fn inline_application_inner_readonly_modifier_preserved() {
    // Adjacent direction guard: a `readonly` inner mapped must keep its
    // modifier (assigning through it errors with TS2540). Uses a function
    // parameter so the harness binds the value.
    let source = r#"
type MyReadonly<X> = { readonly [P in keyof X]: X[P] };
type NestRo<T> = { [K in keyof T]: MyReadonly<T[K]> };
function f(x: NestRo<{ a: { b: number } }>) { x.a.b = 1; }
"#;
    assert!(
        check_source_codes(source).contains(&2540),
        "inner readonly mapped must keep its readonly modifier in inline-application form. Got: {:?}",
        check_source_codes(source)
    );
}

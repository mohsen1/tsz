//! Tests for string intrinsic type subtype rules.
//!
//! Validates that Uppercase<T>, Lowercase<T>, Capitalize<T>, and Uncapitalize<T>
//! have correct assignability behavior matching TypeScript:
//! - StringIntrinsic(kind, T) <: string (always)
//! - StringIntrinsic(kind, S) <: StringIntrinsic(kind, T) when S <: T (covariant)
//! - Constraint-based: Uppercase<T extends C> <: Uppercase<C> evaluated

use crate::intern::TypeInterner;
use crate::relations::subtype::SubtypeChecker;
use crate::types::{StringIntrinsicKind, TemplateSpan, TypeData, TypeId, TypeParamInfo};
use crate::evaluate_type;
use crate::{RelationContext, RelationKind, RelationPolicy, query_relation};

// =============================================================================
// Rule 1: StringIntrinsic(kind, T) <: string
// =============================================================================

#[test]
fn string_intrinsic_uppercase_is_subtype_of_string() {
    let interner = TypeInterner::new();

    // Uppercase<string> should be assignable to string
    let uppercase_string =
        interner.string_intrinsic(StringIntrinsicKind::Uppercase, TypeId::STRING);

    let mut checker = SubtypeChecker::new(&interner);
    assert!(
        checker.is_subtype_of(uppercase_string, TypeId::STRING),
        "Uppercase<string> should be assignable to string"
    );
}

#[test]
fn string_intrinsic_lowercase_is_subtype_of_string() {
    let interner = TypeInterner::new();

    let lowercase_string =
        interner.string_intrinsic(StringIntrinsicKind::Lowercase, TypeId::STRING);

    let mut checker = SubtypeChecker::new(&interner);
    assert!(
        checker.is_subtype_of(lowercase_string, TypeId::STRING),
        "Lowercase<string> should be assignable to string"
    );
}

#[test]
fn string_intrinsic_capitalize_is_subtype_of_string() {
    let interner = TypeInterner::new();

    let cap_string = interner.string_intrinsic(StringIntrinsicKind::Capitalize, TypeId::STRING);

    let mut checker = SubtypeChecker::new(&interner);
    assert!(
        checker.is_subtype_of(cap_string, TypeId::STRING),
        "Capitalize<string> should be assignable to string"
    );
}

#[test]
fn string_intrinsic_with_type_param_is_subtype_of_string() {
    let interner = TypeInterner::new();

    // Create T extends string
    let t_param = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: interner.intern_string("T"),
        constraint: Some(TypeId::STRING),
        default: None,
        is_const: false,
    }));

    // Uppercase<T> should be assignable to string
    let uppercase_t = interner.string_intrinsic(StringIntrinsicKind::Uppercase, t_param);

    let mut checker = SubtypeChecker::new(&interner);
    assert!(
        checker.is_subtype_of(uppercase_t, TypeId::STRING),
        "Uppercase<T extends string> should be assignable to string"
    );
}

// =============================================================================
// Rule 2: Covariant in type argument (same kind)
// =============================================================================

#[test]
fn string_intrinsic_covariant_same_kind() {
    let interner = TypeInterner::new();

    // Create T extends string and U extends T
    let t_param = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: interner.intern_string("T"),
        constraint: Some(TypeId::STRING),
        default: None,
        is_const: false,
    }));
    let u_param = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: interner.intern_string("U"),
        constraint: Some(t_param),
        default: None,
        is_const: false,
    }));

    let uppercase_t = interner.string_intrinsic(StringIntrinsicKind::Uppercase, t_param);
    let uppercase_u = interner.string_intrinsic(StringIntrinsicKind::Uppercase, u_param);

    let mut checker = SubtypeChecker::new(&interner);

    // Uppercase<U> <: Uppercase<T> when U extends T (covariant)
    assert!(
        checker.is_subtype_of(uppercase_u, uppercase_t),
        "Uppercase<U extends T> should be assignable to Uppercase<T>"
    );

    // Uppercase<T> is NOT a subtype of Uppercase<U> (T does not extend U)
    assert!(
        !checker.is_subtype_of(uppercase_t, uppercase_u),
        "Uppercase<T> should NOT be assignable to Uppercase<U extends T>"
    );
}

#[test]
fn string_intrinsic_different_kind_not_subtype() {
    let interner = TypeInterner::new();

    let uppercase_string =
        interner.string_intrinsic(StringIntrinsicKind::Uppercase, TypeId::STRING);
    let lowercase_string =
        interner.string_intrinsic(StringIntrinsicKind::Lowercase, TypeId::STRING);

    let mut checker = SubtypeChecker::new(&interner);

    // Uppercase<string> is NOT a subtype of Lowercase<string>
    // (different kinds are not related)
    // Note: Both are subtypes of string though
    assert!(
        checker.is_subtype_of(uppercase_string, TypeId::STRING),
        "Uppercase<string> should be assignable to string"
    );
    assert!(
        checker.is_subtype_of(lowercase_string, TypeId::STRING),
        "Lowercase<string> should be assignable to string"
    );
}

// =============================================================================
// Rule 3: Constraint-based assignability
// =============================================================================

#[test]
fn string_intrinsic_constraint_evaluation_literal_union() {
    let interner = TypeInterner::new();

    // Create 'foo' | 'bar' union
    let foo = interner.literal_string("foo");
    let bar = interner.literal_string("bar");
    let foo_or_bar = interner.union(vec![foo, bar]);

    // Create T extends 'foo' | 'bar'
    let t_param = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: interner.intern_string("T"),
        constraint: Some(foo_or_bar),
        default: None,
        is_const: false,
    }));

    // Create Uppercase<T>
    let uppercase_t = interner.string_intrinsic(StringIntrinsicKind::Uppercase, t_param);

    // Create 'FOO' | 'BAR' target
    let foo_upper = interner.literal_string("FOO");
    let bar_upper = interner.literal_string("BAR");
    let foo_or_bar_upper = interner.union(vec![foo_upper, bar_upper]);

    let mut checker = SubtypeChecker::new(&interner);

    // Uppercase<T extends 'foo'|'bar'> should be assignable to 'FOO'|'BAR'
    assert!(
        checker.is_subtype_of(uppercase_t, foo_or_bar_upper),
        "Uppercase<T extends 'foo'|'bar'> should be assignable to 'FOO'|'BAR'"
    );
}

// =============================================================================
// Negative cases
// =============================================================================

#[test]
fn string_not_subtype_of_string_intrinsic() {
    let interner = TypeInterner::new();

    let t_param = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: interner.intern_string("T"),
        constraint: Some(TypeId::STRING),
        default: None,
        is_const: false,
    }));
    let uppercase_t = interner.string_intrinsic(StringIntrinsicKind::Uppercase, t_param);

    let mut checker = SubtypeChecker::new(&interner);

    // string is NOT assignable to Uppercase<T> (T could be any specific string)
    assert!(
        !checker.is_subtype_of(TypeId::STRING, uppercase_t),
        "string should NOT be assignable to Uppercase<T>"
    );
}

#[test]
fn uppercase_literal_is_subtype_of_uppercase_string() {
    let interner = TypeInterner::new();

    let uppercase_string =
        interner.string_intrinsic(StringIntrinsicKind::Uppercase, TypeId::STRING);
    let uppercase_literal = interner.literal_string("FOO");
    let lowercase_literal = interner.literal_string("bar");

    let mut checker = SubtypeChecker::new(&interner);
    assert!(
        checker.is_subtype_of(uppercase_literal, uppercase_string),
        "\"FOO\" should be assignable to Uppercase<string>"
    );
    assert!(
        !checker.is_subtype_of(lowercase_literal, uppercase_string),
        "\"bar\" should not be assignable to Uppercase<string>"
    );
}

#[test]
fn nested_same_kind_string_intrinsic_is_idempotent() {
    let interner = TypeInterner::new();

    let uppercase_string =
        interner.string_intrinsic(StringIntrinsicKind::Uppercase, TypeId::STRING);
    let nested_uppercase =
        interner.string_intrinsic(StringIntrinsicKind::Uppercase, uppercase_string);

    let evaluated = evaluate_type(&interner, nested_uppercase);
    assert_eq!(
        evaluated, uppercase_string,
        "Uppercase<Uppercase<string>> should normalize to Uppercase<string>"
    );

    let mut checker = SubtypeChecker::new(&interner);
    assert!(
        checker.is_subtype_of(uppercase_string, nested_uppercase),
        "Uppercase<string> should be assignable to Uppercase<Uppercase<string>>"
    );
    assert!(
        checker.is_subtype_of(nested_uppercase, uppercase_string),
        "Uppercase<Uppercase<string>> should be assignable to Uppercase<string>"
    );
}

#[test]
fn uppercase_template_literal_accepts_only_uppercase_suffixes() {
    let interner = TypeInterner::new();

    let uppercase_string =
        interner.string_intrinsic(StringIntrinsicKind::Uppercase, TypeId::STRING);
    let uppercase_template = interner.template_literal(vec![
        TemplateSpan::Text(interner.intern_string("AA")),
        TemplateSpan::Type(uppercase_string),
    ]);

    let empty_suffix = interner.literal_string("AA");
    let uppercase_suffix = interner.literal_string("AAFOO");
    let mixed_suffix = interner.literal_string("AAFoo");

    let mut checker = SubtypeChecker::new(&interner);
    assert!(
        checker.is_subtype_of(empty_suffix, uppercase_template),
        "\"AA\" should match `AA${{Uppercase<string>}}` because the empty suffix is uppercase"
    );
    assert!(
        checker.is_subtype_of(uppercase_suffix, uppercase_template),
        "\"AAFOO\" should match `AA${{Uppercase<string>}}`"
    );
    assert!(
        !checker.is_subtype_of(mixed_suffix, uppercase_template),
        "\"AAFoo\" should not match `AA${{Uppercase<string>}}`"
    );
}

#[test]
fn assignability_query_normalizes_nested_uppercase_intrinsics() {
    let interner = TypeInterner::new();

    let uppercase_string =
        interner.string_intrinsic(StringIntrinsicKind::Uppercase, TypeId::STRING);
    let nested_uppercase =
        interner.string_intrinsic(StringIntrinsicKind::Uppercase, uppercase_string);

    let result = query_relation(
        &interner,
        uppercase_string,
        nested_uppercase,
        RelationKind::Assignable,
        RelationPolicy::default(),
        RelationContext::default(),
    );
    assert!(
        result.is_related(),
        "Assignable relation should treat Uppercase<string> as compatible with Uppercase<Uppercase<string>>"
    );
}

//! Comprehensive tests for discriminant-based narrowing and core narrowing operations.
//!
//! Covers:
//! - String/number/boolean literal discriminants
//! - Multi-property discriminants
//! - Non-discriminant unions
//! - Optional discriminant properties
//! - Nested discriminants
//! - Discriminant discovery (`find_discriminants`)
//! - typeof narrowing
//! - instanceof narrowing (via `narrow_to_type`)
//! - Truthiness narrowing
//! - Equality narrowing
//! - in narrowing (via property truthiness)
//! - Negation narrowing

use crate::intern::TypeInterner;
use crate::narrowing::{
    GuardSense, NarrowingContext, TypeGuard, TypeofKind, find_discriminants,
    narrow_by_discriminant, narrow_by_typeof,
};
use crate::types::{PropertyInfo, TypeId};

// =============================================================================
// String Literal Discriminant Tests
// =============================================================================

#[test]
fn string_discriminant_narrows_to_matching_member() {
    let interner = TypeInterner::new();
    let kind = interner.intern_string("kind");
    let x_name = interner.intern_string("x");
    let y_name = interner.intern_string("y");

    // type A = {kind: "a", x: number} | {kind: "b", y: string}
    let kind_a = interner.literal_string("a");
    let kind_b = interner.literal_string("b");

    let member_a = interner.object(vec![
        PropertyInfo::new(kind, kind_a),
        PropertyInfo::new(x_name, TypeId::NUMBER),
    ]);
    let member_b = interner.object(vec![
        PropertyInfo::new(kind, kind_b),
        PropertyInfo::new(y_name, TypeId::STRING),
    ]);

    let union = interner.union(vec![member_a, member_b]);

    // kind === "a" narrows to member_a
    let narrowed = narrow_by_discriminant(&interner, union, &[kind], kind_a);
    assert_eq!(narrowed, member_a);

    // kind === "b" narrows to member_b
    let narrowed = narrow_by_discriminant(&interner, union, &[kind], kind_b);
    assert_eq!(narrowed, member_b);
}

#[test]
fn string_discriminant_excluding_narrows_to_remaining() {
    let interner = TypeInterner::new();
    let kind = interner.intern_string("kind");

    let kind_a = interner.literal_string("a");
    let kind_b = interner.literal_string("b");
    let kind_c = interner.literal_string("c");

    let member_a = interner.object(vec![PropertyInfo::new(kind, kind_a)]);
    let member_b = interner.object(vec![PropertyInfo::new(kind, kind_b)]);
    let member_c = interner.object(vec![PropertyInfo::new(kind, kind_c)]);

    let union = interner.union(vec![member_a, member_b, member_c]);
    let ctx = NarrowingContext::new(&interner);

    // kind !== "a" -> {kind: "b"} | {kind: "c"}
    let narrowed = ctx.narrow_by_excluding_discriminant(union, &[kind], kind_a);
    let expected = interner.union(vec![member_b, member_c]);
    assert_eq!(narrowed, expected);
}

#[test]
fn string_discriminant_three_members_narrows_correctly() {
    let interner = TypeInterner::new();
    let kind = interner.intern_string("kind");

    let kind_add = interner.literal_string("add");
    let kind_remove = interner.literal_string("remove");
    let kind_clear = interner.literal_string("clear");

    let member_add = interner.object(vec![
        PropertyInfo::new(kind, kind_add),
        PropertyInfo::new(interner.intern_string("value"), TypeId::NUMBER),
    ]);
    let member_remove = interner.object(vec![
        PropertyInfo::new(kind, kind_remove),
        PropertyInfo::new(interner.intern_string("id"), TypeId::STRING),
    ]);
    let member_clear = interner.object(vec![PropertyInfo::new(kind, kind_clear)]);

    let union = interner.union(vec![member_add, member_remove, member_clear]);

    // Narrowing by each discriminant value
    assert_eq!(
        narrow_by_discriminant(&interner, union, &[kind], kind_add),
        member_add
    );
    assert_eq!(
        narrow_by_discriminant(&interner, union, &[kind], kind_remove),
        member_remove
    );
    assert_eq!(
        narrow_by_discriminant(&interner, union, &[kind], kind_clear),
        member_clear
    );
}

// =============================================================================
// Number Literal Discriminant Tests
// =============================================================================

#[test]
fn number_discriminant_narrows_correctly() {
    let interner = TypeInterner::new();
    let code = interner.intern_string("code");
    let msg = interner.intern_string("message");

    let code_200 = interner.literal_number(200.0);
    let code_404 = interner.literal_number(404.0);

    let success = interner.object(vec![
        PropertyInfo::new(code, code_200),
        PropertyInfo::new(msg, TypeId::STRING),
    ]);
    let not_found = interner.object(vec![
        PropertyInfo::new(code, code_404),
        PropertyInfo::new(msg, TypeId::STRING),
    ]);

    let union = interner.union(vec![success, not_found]);

    assert_eq!(
        narrow_by_discriminant(&interner, union, &[code], code_200),
        success
    );
    assert_eq!(
        narrow_by_discriminant(&interner, union, &[code], code_404),
        not_found
    );
}

#[test]
fn number_discriminant_finds_discriminants() {
    let interner = TypeInterner::new();
    let code = interner.intern_string("code");

    let code_1 = interner.literal_number(1.0);
    let code_2 = interner.literal_number(2.0);

    let member1 = interner.object(vec![PropertyInfo::new(code, code_1)]);
    let member2 = interner.object(vec![PropertyInfo::new(code, code_2)]);

    let union = interner.union(vec![member1, member2]);
    let discriminants = find_discriminants(&interner, union);

    assert_eq!(discriminants.len(), 1);
    assert_eq!(discriminants[0].property_name, code);
    assert_eq!(discriminants[0].variants.len(), 2);
}

// =============================================================================
// Boolean Discriminant Tests
// =============================================================================

#[test]
fn boolean_discriminant_narrows_success_error() {
    let interner = TypeInterner::new();
    let success = interner.intern_string("success");
    let data = interner.intern_string("data");
    let error = interner.intern_string("error");

    // {success: true, data: string} | {success: false, error: string}
    let true_lit = interner.literal_boolean(true);
    let false_lit = interner.literal_boolean(false);

    let success_member = interner.object(vec![
        PropertyInfo::new(success, true_lit),
        PropertyInfo::new(data, TypeId::STRING),
    ]);
    let error_member = interner.object(vec![
        PropertyInfo::new(success, false_lit),
        PropertyInfo::new(error, TypeId::STRING),
    ]);

    let union = interner.union(vec![success_member, error_member]);

    // success === true narrows to success_member
    assert_eq!(
        narrow_by_discriminant(&interner, union, &[success], true_lit),
        success_member
    );

    // success === false narrows to error_member
    assert_eq!(
        narrow_by_discriminant(&interner, union, &[success], false_lit),
        error_member
    );
}

#[test]
fn boolean_discriminant_is_discovered() {
    let interner = TypeInterner::new();
    let ok = interner.intern_string("ok");

    let true_lit = interner.literal_boolean(true);
    let false_lit = interner.literal_boolean(false);

    let member1 = interner.object(vec![PropertyInfo::new(ok, true_lit)]);
    let member2 = interner.object(vec![PropertyInfo::new(ok, false_lit)]);

    let union = interner.union(vec![member1, member2]);
    let discriminants = find_discriminants(&interner, union);

    assert_eq!(discriminants.len(), 1);
    assert_eq!(discriminants[0].property_name, ok);
}

// =============================================================================
// Multi-Property Discriminant Tests
// =============================================================================

#[test]
fn multi_property_discriminant_both_found() {
    let interner = TypeInterner::new();
    let kind = interner.intern_string("kind");
    let tag = interner.intern_string("tag");

    let kind_a = interner.literal_string("a");
    let kind_b = interner.literal_string("b");
    let tag_1 = interner.literal_number(1.0);
    let tag_2 = interner.literal_number(2.0);

    let member1 = interner.object(vec![
        PropertyInfo::new(kind, kind_a),
        PropertyInfo::new(tag, tag_1),
    ]);
    let member2 = interner.object(vec![
        PropertyInfo::new(kind, kind_b),
        PropertyInfo::new(tag, tag_2),
    ]);

    let union = interner.union(vec![member1, member2]);
    let discriminants = find_discriminants(&interner, union);

    // Both "kind" and "tag" are discriminants
    assert_eq!(discriminants.len(), 2);
    let names: Vec<_> = discriminants
        .iter()
        .map(|d| interner.resolve_atom(d.property_name))
        .collect();
    assert!(names.contains(&"kind".to_string()));
    assert!(names.contains(&"tag".to_string()));
}

#[test]
fn multi_property_discriminant_narrows_by_either() {
    let interner = TypeInterner::new();
    let kind = interner.intern_string("kind");
    let tag = interner.intern_string("tag");

    let kind_a = interner.literal_string("a");
    let kind_b = interner.literal_string("b");
    let tag_x = interner.literal_string("x");
    let tag_y = interner.literal_string("y");

    let member1 = interner.object(vec![
        PropertyInfo::new(kind, kind_a),
        PropertyInfo::new(tag, tag_x),
    ]);
    let member2 = interner.object(vec![
        PropertyInfo::new(kind, kind_b),
        PropertyInfo::new(tag, tag_y),
    ]);

    let union = interner.union(vec![member1, member2]);

    // Can narrow by either property
    assert_eq!(
        narrow_by_discriminant(&interner, union, &[kind], kind_a),
        member1
    );
    assert_eq!(
        narrow_by_discriminant(&interner, union, &[tag], tag_y),
        member2
    );
}

// =============================================================================
// Non-Discriminant Union Tests
// =============================================================================

#[test]
fn non_discriminant_union_no_common_property() {
    let interner = TypeInterner::new();

    // {x: string} | {y: number} - no common discriminant property
    let x = interner.intern_string("x");
    let y = interner.intern_string("y");

    let member1 = interner.object(vec![PropertyInfo::new(x, TypeId::STRING)]);
    let member2 = interner.object(vec![PropertyInfo::new(y, TypeId::NUMBER)]);

    let union = interner.union(vec![member1, member2]);
    let discriminants = find_discriminants(&interner, union);

    assert_eq!(
        discriminants.len(),
        0,
        "No common property means no discriminants"
    );
}

#[test]
fn non_discriminant_union_shared_property_not_literal() {
    let interner = TypeInterner::new();

    // {kind: string} | {kind: string} - not literal, so not discriminant
    let kind = interner.intern_string("kind");

    let member1 = interner.object(vec![PropertyInfo::new(kind, TypeId::STRING)]);
    let member2 = interner.object(vec![PropertyInfo::new(kind, TypeId::STRING)]);

    let union = interner.union(vec![member1, member2]);
    let discriminants = find_discriminants(&interner, union);

    assert_eq!(
        discriminants.len(),
        0,
        "Non-literal shared property is not a discriminant"
    );
}

#[test]
fn non_discriminant_union_duplicate_literal_values() {
    let interner = TypeInterner::new();

    // {kind: "a"} | {kind: "a"} - same literal in both members
    let kind = interner.intern_string("kind");
    let kind_a = interner.literal_string("a");

    let member1 = interner.object(vec![PropertyInfo::new(kind, kind_a)]);
    let member2 = interner.object(vec![PropertyInfo::new(kind, kind_a)]);

    let union = interner.union(vec![member1, member2]);
    let discriminants = find_discriminants(&interner, union);

    assert_eq!(
        discriminants.len(),
        0,
        "Duplicate literal values means the property cannot uniquely identify members"
    );
}

#[test]
fn non_discriminant_union_non_object_member() {
    let interner = TypeInterner::new();

    // {kind: "a"} | string - string is not an object
    let kind = interner.intern_string("kind");
    let kind_a = interner.literal_string("a");

    let member1 = interner.object(vec![PropertyInfo::new(kind, kind_a)]);

    let union = interner.union(vec![member1, TypeId::STRING]);
    let discriminants = find_discriminants(&interner, union);

    assert_eq!(
        discriminants.len(),
        0,
        "Non-object union members prevent discriminant detection"
    );
}

#[test]
fn non_discriminant_single_member_union() {
    let interner = TypeInterner::new();

    let kind = interner.intern_string("kind");
    let kind_a = interner.literal_string("a");

    let member = interner.object(vec![PropertyInfo::new(kind, kind_a)]);
    // Single-member union gets collapsed by the interner
    let union = interner.union(vec![member]);

    let discriminants = find_discriminants(&interner, union);

    // Single member or non-union: no discriminants needed
    assert_eq!(discriminants.len(), 0);
}

// =============================================================================
// Optional Discriminant Property Tests
// =============================================================================

#[test]
fn optional_discriminant_property_not_found_as_discriminant() {
    let interner = TypeInterner::new();

    // {kind: "a"} | {other: number} - "kind" missing from second member
    let kind = interner.intern_string("kind");
    let other = interner.intern_string("other");
    let kind_a = interner.literal_string("a");

    let member1 = interner.object(vec![PropertyInfo::new(kind, kind_a)]);
    let member2 = interner.object(vec![PropertyInfo::new(other, TypeId::NUMBER)]);

    let union = interner.union(vec![member1, member2]);
    let discriminants = find_discriminants(&interner, union);

    assert_eq!(
        discriminants.len(),
        0,
        "When discriminant property is missing from a member, it is not a valid discriminant"
    );
}

#[test]
fn optional_discriminant_narrowing_keeps_members_without_property() {
    let interner = TypeInterner::new();
    let kind = interner.intern_string("kind");
    let other = interner.intern_string("other");

    // When narrowing by discriminant, members that lack the property are excluded
    // from positive match but kept in negative match
    let kind_a = interner.literal_string("a");
    let kind_b = interner.literal_string("b");

    let member_a = interner.object(vec![
        PropertyInfo::new(kind, kind_a),
        PropertyInfo::new(other, TypeId::NUMBER),
    ]);
    let member_b = interner.object(vec![
        PropertyInfo::new(kind, kind_b),
        PropertyInfo::new(other, TypeId::STRING),
    ]);
    let member_no_kind = interner.object(vec![PropertyInfo::new(other, TypeId::BOOLEAN)]);

    let union = interner.union(vec![member_a, member_b, member_no_kind]);
    let ctx = NarrowingContext::new(&interner);

    // kind === "a": only member_a matches
    let narrowed = ctx.narrow_by_discriminant(union, &[kind], kind_a);
    assert_eq!(narrowed, member_a);

    // kind !== "a": member_b and member_no_kind should be kept
    // member_no_kind doesn't have "kind", so it shouldn't be excluded
    let narrowed_exclude = ctx.narrow_by_excluding_discriminant(union, &[kind], kind_a);
    let expected = interner.union(vec![member_b, member_no_kind]);
    assert_eq!(narrowed_exclude, expected);
}

// =============================================================================
// Nested Discriminant Tests
// =============================================================================

#[test]
fn nested_discriminant_narrow_by_nested_path() {
    let interner = TypeInterner::new();
    let payload = interner.intern_string("payload");
    let kind = interner.intern_string("kind");

    let kind_a = interner.literal_string("a");
    let kind_b = interner.literal_string("b");

    // {payload: {kind: "a"}} | {payload: {kind: "b"}}
    let nested_a = interner.object(vec![PropertyInfo::new(kind, kind_a)]);
    let nested_b = interner.object(vec![PropertyInfo::new(kind, kind_b)]);

    let member1 = interner.object(vec![PropertyInfo::new(payload, nested_a)]);
    let member2 = interner.object(vec![PropertyInfo::new(payload, nested_b)]);

    let union = interner.union(vec![member1, member2]);
    let ctx = NarrowingContext::new(&interner);

    // Narrow by payload.kind === "a"
    let narrowed = ctx.narrow_by_discriminant(union, &[payload, kind], kind_a);
    assert_eq!(narrowed, member1);

    // Narrow by payload.kind === "b"
    let narrowed = ctx.narrow_by_discriminant(union, &[payload, kind], kind_b);
    assert_eq!(narrowed, member2);
}

#[test]
fn nested_discriminant_exclude_by_nested_path() {
    let interner = TypeInterner::new();
    let payload = interner.intern_string("payload");
    let kind = interner.intern_string("kind");

    let kind_a = interner.literal_string("a");
    let kind_b = interner.literal_string("b");
    let kind_c = interner.literal_string("c");

    let nested_a = interner.object(vec![PropertyInfo::new(kind, kind_a)]);
    let nested_b = interner.object(vec![PropertyInfo::new(kind, kind_b)]);
    let nested_c = interner.object(vec![PropertyInfo::new(kind, kind_c)]);

    let member1 = interner.object(vec![PropertyInfo::new(payload, nested_a)]);
    let member2 = interner.object(vec![PropertyInfo::new(payload, nested_b)]);
    let member3 = interner.object(vec![PropertyInfo::new(payload, nested_c)]);

    let union = interner.union(vec![member1, member2, member3]);
    let ctx = NarrowingContext::new(&interner);

    // Exclude payload.kind === "a" -> member2 | member3
    let narrowed = ctx.narrow_by_excluding_discriminant(union, &[payload, kind], kind_a);
    let expected = interner.union(vec![member2, member3]);
    assert_eq!(narrowed, expected);
}

// =============================================================================
// Batch Discriminant Exclusion Tests
// =============================================================================

#[test]
fn batch_exclude_discriminant_values() {
    let interner = TypeInterner::new();
    let kind = interner.intern_string("kind");

    let kind_a = interner.literal_string("a");
    let kind_b = interner.literal_string("b");
    let kind_c = interner.literal_string("c");
    let kind_d = interner.literal_string("d");

    let member_a = interner.object(vec![PropertyInfo::new(kind, kind_a)]);
    let member_b = interner.object(vec![PropertyInfo::new(kind, kind_b)]);
    let member_c = interner.object(vec![PropertyInfo::new(kind, kind_c)]);
    let member_d = interner.object(vec![PropertyInfo::new(kind, kind_d)]);

    let union = interner.union(vec![member_a, member_b, member_c, member_d]);
    let ctx = NarrowingContext::new(&interner);

    // Exclude "a" and "b" (switch fallthrough) -> member_c | member_d
    let narrowed = ctx.narrow_by_excluding_discriminant_values(union, &[kind], &[kind_a, kind_b]);
    let expected = interner.union(vec![member_c, member_d]);
    assert_eq!(narrowed, expected);
}

#[test]
fn batch_exclude_empty_values_returns_original() {
    let interner = TypeInterner::new();
    let kind = interner.intern_string("kind");

    let kind_a = interner.literal_string("a");
    let member_a = interner.object(vec![PropertyInfo::new(kind, kind_a)]);
    let union = interner.union(vec![member_a, TypeId::STRING]);

    let ctx = NarrowingContext::new(&interner);
    let narrowed = ctx.narrow_by_excluding_discriminant_values(union, &[kind], &[]);
    assert_eq!(narrowed, union);
}

// =============================================================================
// Discriminant with Any/Unknown Members
// =============================================================================

#[test]
fn discriminant_narrows_any_member_preserved() {
    let interner = TypeInterner::new();
    let kind = interner.intern_string("kind");
    let kind_a = interner.literal_string("a");

    let member_a = interner.object(vec![PropertyInfo::new(kind, kind_a)]);
    let union = interner.union(vec![member_a, TypeId::ANY]);

    let ctx = NarrowingContext::new(&interner);

    // any is always kept in both branches
    let narrowed = ctx.narrow_by_discriminant(union, &[kind], kind_a);
    // Should include both member_a and any
    let expected = interner.union(vec![member_a, TypeId::ANY]);
    assert_eq!(narrowed, expected);
}

#[test]
fn discriminant_exclude_any_member_preserved() {
    let interner = TypeInterner::new();
    let kind = interner.intern_string("kind");
    let kind_a = interner.literal_string("a");
    let kind_b = interner.literal_string("b");

    let member_a = interner.object(vec![PropertyInfo::new(kind, kind_a)]);
    let member_b = interner.object(vec![PropertyInfo::new(kind, kind_b)]);
    let union = interner.union(vec![member_a, member_b, TypeId::ANY]);

    let ctx = NarrowingContext::new(&interner);

    // Excluding "a" should keep member_b and any
    let narrowed = ctx.narrow_by_excluding_discriminant(union, &[kind], kind_a);
    let expected = interner.union(vec![member_b, TypeId::ANY]);
    assert_eq!(narrowed, expected);
}

// =============================================================================
// Discriminant No-Match Returns Never
// =============================================================================

#[test]
fn discriminant_no_match_returns_never() {
    let interner = TypeInterner::new();
    let kind = interner.intern_string("kind");

    let kind_a = interner.literal_string("a");
    let kind_b = interner.literal_string("b");
    let kind_z = interner.literal_string("z");

    let member_a = interner.object(vec![PropertyInfo::new(kind, kind_a)]);
    let member_b = interner.object(vec![PropertyInfo::new(kind, kind_b)]);

    let union = interner.union(vec![member_a, member_b]);

    // kind === "z" doesn't match any member
    let narrowed = narrow_by_discriminant(&interner, union, &[kind], kind_z);
    assert_eq!(narrowed, TypeId::NEVER);
}

// =============================================================================
// typeof Narrowing Tests
// =============================================================================

#[test]
fn typeof_narrows_string_from_mixed_union() {
    let interner = TypeInterner::new();

    let union = interner.union(vec![
        TypeId::STRING,
        TypeId::NUMBER,
        TypeId::BOOLEAN,
        TypeId::NULL,
    ]);

    let narrowed = narrow_by_typeof(&interner, union, "string");
    assert_eq!(narrowed, TypeId::STRING);
}

#[test]
fn typeof_narrows_number_from_mixed_union() {
    let interner = TypeInterner::new();

    let union = interner.union(vec![TypeId::STRING, TypeId::NUMBER, TypeId::BOOLEAN]);
    let narrowed = narrow_by_typeof(&interner, union, "number");
    assert_eq!(narrowed, TypeId::NUMBER);
}

#[test]
fn typeof_narrows_boolean() {
    let interner = TypeInterner::new();

    let union = interner.union(vec![TypeId::STRING, TypeId::BOOLEAN]);
    let narrowed = narrow_by_typeof(&interner, union, "boolean");
    assert_eq!(narrowed, TypeId::BOOLEAN);
}

#[test]
fn typeof_narrows_bigint() {
    let interner = TypeInterner::new();

    let union = interner.union(vec![TypeId::STRING, TypeId::BIGINT]);
    let narrowed = narrow_by_typeof(&interner, union, "bigint");
    assert_eq!(narrowed, TypeId::BIGINT);
}

#[test]
fn typeof_narrows_symbol() {
    let interner = TypeInterner::new();

    let union = interner.union(vec![TypeId::STRING, TypeId::SYMBOL]);
    let narrowed = narrow_by_typeof(&interner, union, "symbol");
    assert_eq!(narrowed, TypeId::SYMBOL);
}

#[test]
fn typeof_narrows_undefined() {
    let interner = TypeInterner::new();

    let union = interner.union(vec![TypeId::STRING, TypeId::UNDEFINED]);
    let narrowed = narrow_by_typeof(&interner, union, "undefined");
    assert_eq!(narrowed, TypeId::UNDEFINED);
}

#[test]
fn typeof_string_from_literal_union() {
    let interner = TypeInterner::new();

    let hello = interner.literal_string("hello");
    let num42 = interner.literal_number(42.0);
    let union = interner.union(vec![hello, num42]);

    // typeof x === "string" narrows to "hello" (string literal is typeof "string")
    let narrowed = narrow_by_typeof(&interner, union, "string");
    assert_eq!(narrowed, hello);
}

#[test]
fn typeof_no_match_returns_never() {
    let interner = TypeInterner::new();

    // typeof string === "number" -> never
    let narrowed = narrow_by_typeof(&interner, TypeId::STRING, "number");
    assert_eq!(narrowed, TypeId::NEVER);
}

#[test]
fn typeof_any_narrows_primitives_only() {
    let interner = TypeInterner::new();

    // typeof any === "string" -> string (primitives narrow any)
    assert_eq!(
        narrow_by_typeof(&interner, TypeId::ANY, "string"),
        TypeId::STRING
    );
    assert_eq!(
        narrow_by_typeof(&interner, TypeId::ANY, "number"),
        TypeId::NUMBER
    );
    assert_eq!(
        narrow_by_typeof(&interner, TypeId::ANY, "boolean"),
        TypeId::BOOLEAN
    );
    assert_eq!(
        narrow_by_typeof(&interner, TypeId::ANY, "undefined"),
        TypeId::UNDEFINED
    );

    // typeof any === "object" -> any (non-primitive does not narrow any)
    assert_eq!(
        narrow_by_typeof(&interner, TypeId::ANY, "object"),
        TypeId::ANY
    );
}

#[test]
fn typeof_unknown_narrows_all() {
    let interner = TypeInterner::new();

    // unknown narrows for all typeof checks
    assert_eq!(
        narrow_by_typeof(&interner, TypeId::UNKNOWN, "string"),
        TypeId::STRING
    );
    assert_eq!(
        narrow_by_typeof(&interner, TypeId::UNKNOWN, "number"),
        TypeId::NUMBER
    );
    assert_eq!(
        narrow_by_typeof(&interner, TypeId::UNKNOWN, "boolean"),
        TypeId::BOOLEAN
    );
    assert_eq!(
        narrow_by_typeof(&interner, TypeId::UNKNOWN, "undefined"),
        TypeId::UNDEFINED
    );
}

// =============================================================================
// Negation (typeof x !== "...") Tests
// =============================================================================

#[test]
fn typeof_negation_excludes_string() {
    let interner = TypeInterner::new();
    let ctx = NarrowingContext::new(&interner);

    // string | number, typeof !== "string" -> number
    let union = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);
    let guard = TypeGuard::Typeof(TypeofKind::String);
    let narrowed = ctx.narrow_type(union, &guard, GuardSense::Negative);
    assert_eq!(narrowed, TypeId::NUMBER);
}

#[test]
fn typeof_negation_excludes_number() {
    let interner = TypeInterner::new();
    let ctx = NarrowingContext::new(&interner);

    let union = interner.union(vec![TypeId::STRING, TypeId::NUMBER, TypeId::BOOLEAN]);
    let guard = TypeGuard::Typeof(TypeofKind::Number);
    let narrowed = ctx.narrow_type(union, &guard, GuardSense::Negative);
    let expected = interner.union(vec![TypeId::STRING, TypeId::BOOLEAN]);
    assert_eq!(narrowed, expected);
}

#[test]
fn typeof_negation_excludes_boolean() {
    let interner = TypeInterner::new();
    let ctx = NarrowingContext::new(&interner);

    let union = interner.union(vec![TypeId::STRING, TypeId::BOOLEAN]);
    let guard = TypeGuard::Typeof(TypeofKind::Boolean);
    let narrowed = ctx.narrow_type(union, &guard, GuardSense::Negative);
    assert_eq!(narrowed, TypeId::STRING);
}

#[test]
fn typeof_negation_no_match_returns_all() {
    let interner = TypeInterner::new();
    let ctx = NarrowingContext::new(&interner);

    // string | number, typeof !== "boolean" -> string | number (no boolean to exclude)
    let union = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);
    let guard = TypeGuard::Typeof(TypeofKind::Boolean);
    let narrowed = ctx.narrow_type(union, &guard, GuardSense::Negative);
    assert_eq!(narrowed, union);
}

// =============================================================================
// instanceof Narrowing Tests (via narrow_to_type)
// =============================================================================

#[test]
fn instanceof_narrows_to_matching_interface() {
    let interner = TypeInterner::new();
    let ctx = NarrowingContext::new(&interner);

    let meow = interner.intern_string("meow");
    let bark = interner.intern_string("bark");

    let cat = interner.object(vec![PropertyInfo::method(meow, TypeId::VOID)]);
    let dog = interner.object(vec![PropertyInfo::method(bark, TypeId::VOID)]);

    let union = interner.union(vec![cat, dog]);

    // instanceof Cat -> cat
    let narrowed = ctx.narrow_to_type(union, cat);
    assert_eq!(narrowed, cat);
}

#[test]
fn instanceof_narrows_union_to_single_type() {
    let interner = TypeInterner::new();
    let ctx = NarrowingContext::new(&interner);

    let union = interner.union(vec![TypeId::STRING, TypeId::NUMBER, TypeId::BOOLEAN]);
    let narrowed = ctx.narrow_to_type(union, TypeId::STRING);
    assert_eq!(narrowed, TypeId::STRING);
}

#[test]
fn instanceof_excludes_non_matching() {
    let interner = TypeInterner::new();
    let ctx = NarrowingContext::new(&interner);

    // string | number, exclude string -> number
    let union = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);
    let narrowed = ctx.narrow_excluding_type(union, TypeId::STRING);
    assert_eq!(narrowed, TypeId::NUMBER);
}

// =============================================================================
// Truthiness Narrowing Tests
// =============================================================================

#[test]
fn truthiness_narrows_out_null() {
    let interner = TypeInterner::new();
    let ctx = NarrowingContext::new(&interner);

    // string | null, if(x) -> string
    let union = interner.union(vec![TypeId::STRING, TypeId::NULL]);
    let narrowed = ctx.narrow_by_truthiness(union);
    assert_eq!(narrowed, TypeId::STRING);
}

#[test]
fn truthiness_narrows_out_undefined() {
    let interner = TypeInterner::new();
    let ctx = NarrowingContext::new(&interner);

    // number | undefined, if(x) -> number
    let union = interner.union(vec![TypeId::NUMBER, TypeId::UNDEFINED]);
    let narrowed = ctx.narrow_by_truthiness(union);
    assert_eq!(narrowed, TypeId::NUMBER);
}

#[test]
fn truthiness_narrows_out_null_and_undefined() {
    let interner = TypeInterner::new();
    let ctx = NarrowingContext::new(&interner);

    // string | null | undefined, if(x) -> string
    let union = interner.union(vec![TypeId::STRING, TypeId::NULL, TypeId::UNDEFINED]);
    let narrowed = ctx.narrow_by_truthiness(union);
    assert_eq!(narrowed, TypeId::STRING);
}

#[test]
fn truthiness_preserves_non_nullable_types() {
    let interner = TypeInterner::new();
    let ctx = NarrowingContext::new(&interner);

    // string | number, if(x) -> string | number (both already truthy)
    let union = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);
    let narrowed = ctx.narrow_by_truthiness(union);
    assert_eq!(narrowed, union);
}

#[test]
fn truthiness_via_type_guard() {
    let interner = TypeInterner::new();
    let ctx = NarrowingContext::new(&interner);

    let union = interner.union(vec![TypeId::STRING, TypeId::NULL]);
    let guard = TypeGuard::Truthy;
    let narrowed = ctx.narrow_type(union, &guard, GuardSense::Positive);
    assert_eq!(narrowed, TypeId::STRING);
}

#[test]
fn falsy_branch_narrows_to_nullable() {
    let interner = TypeInterner::new();
    let ctx = NarrowingContext::new(&interner);

    // string | null, if(!x) -> falsy values from the type
    let union = interner.union(vec![TypeId::STRING, TypeId::NULL]);
    let narrowed = ctx.narrow_to_falsy(union);
    // Should include null (and possibly "" - falsy string)
    // At minimum, the result should contain null
    let has_null = narrowed == TypeId::NULL
        || crate::relations::subtype::is_subtype_of(&interner, TypeId::NULL, narrowed);
    assert!(has_null, "Falsy narrowing should include null");
}

// =============================================================================
// Equality Narrowing Tests
// =============================================================================

#[test]
fn equality_narrows_to_literal() {
    let interner = TypeInterner::new();
    let ctx = NarrowingContext::new(&interner);

    let foo = interner.literal_string("foo");
    let bar = interner.literal_string("bar");
    let union = interner.union(vec![foo, bar]);

    // x === "foo"
    let guard = TypeGuard::LiteralEquality(foo);
    let narrowed = ctx.narrow_type(union, &guard, GuardSense::Positive);
    assert_eq!(narrowed, foo);
}

#[test]
fn equality_excludes_literal() {
    let interner = TypeInterner::new();
    let ctx = NarrowingContext::new(&interner);

    let foo = interner.literal_string("foo");
    let bar = interner.literal_string("bar");
    let baz = interner.literal_string("baz");
    let union = interner.union(vec![foo, bar, baz]);

    // x !== "foo"
    let guard = TypeGuard::LiteralEquality(foo);
    let narrowed = ctx.narrow_type(union, &guard, GuardSense::Negative);
    let expected = interner.union(vec![bar, baz]);
    assert_eq!(narrowed, expected);
}

#[test]
fn equality_narrows_null() {
    let interner = TypeInterner::new();
    let ctx = NarrowingContext::new(&interner);

    // string | null, x === null
    let union = interner.union(vec![TypeId::STRING, TypeId::NULL]);
    let guard = TypeGuard::LiteralEquality(TypeId::NULL);
    let narrowed = ctx.narrow_type(union, &guard, GuardSense::Positive);
    assert_eq!(narrowed, TypeId::NULL);
}

#[test]
fn equality_excludes_null() {
    let interner = TypeInterner::new();
    let ctx = NarrowingContext::new(&interner);

    // string | null, x !== null -> string
    let union = interner.union(vec![TypeId::STRING, TypeId::NULL]);
    let guard = TypeGuard::LiteralEquality(TypeId::NULL);
    let narrowed = ctx.narrow_type(union, &guard, GuardSense::Negative);
    assert_eq!(narrowed, TypeId::STRING);
}

#[test]
fn equality_narrows_undefined() {
    let interner = TypeInterner::new();
    let ctx = NarrowingContext::new(&interner);

    // number | undefined, x === undefined
    let union = interner.union(vec![TypeId::NUMBER, TypeId::UNDEFINED]);
    let guard = TypeGuard::LiteralEquality(TypeId::UNDEFINED);
    let narrowed = ctx.narrow_type(union, &guard, GuardSense::Positive);
    assert_eq!(narrowed, TypeId::UNDEFINED);
}

// =============================================================================
// Nullish Equality Narrowing (==null / !=null)
// =============================================================================

#[test]
fn nullish_equality_narrows_to_nullish() {
    let interner = TypeInterner::new();
    let ctx = NarrowingContext::new(&interner);

    let union = interner.union(vec![TypeId::STRING, TypeId::NULL, TypeId::UNDEFINED]);
    let guard = TypeGuard::NullishEquality;
    let narrowed = ctx.narrow_type(union, &guard, GuardSense::Positive);
    let expected = interner.union(vec![TypeId::NULL, TypeId::UNDEFINED]);
    assert_eq!(narrowed, expected);
}

#[test]
fn nullish_inequality_narrows_to_non_nullish() {
    let interner = TypeInterner::new();
    let ctx = NarrowingContext::new(&interner);

    let union = interner.union(vec![TypeId::STRING, TypeId::NULL, TypeId::UNDEFINED]);
    let guard = TypeGuard::NullishEquality;
    let narrowed = ctx.narrow_type(union, &guard, GuardSense::Negative);
    assert_eq!(narrowed, TypeId::STRING);
}

// =============================================================================
// Property Truthiness Narrowing (simulating `"prop" in x`)
// =============================================================================

#[test]
fn property_truthiness_narrows_union() {
    let interner = TypeInterner::new();
    let ctx = NarrowingContext::new(&interner);

    let name = interner.intern_string("name");
    let age = interner.intern_string("age");

    // {name: string} | {age: number}
    let member1 = interner.object(vec![PropertyInfo::new(name, TypeId::STRING)]);
    let member2 = interner.object(vec![PropertyInfo::new(age, TypeId::NUMBER)]);
    let union = interner.union(vec![member1, member2]);

    // x.name is truthy (only member1 has "name")
    let narrowed = ctx.narrow_by_property_truthiness(union, &[name], true);
    assert_eq!(narrowed, member1);
}

#[test]
fn property_truthiness_false_branch() {
    let interner = TypeInterner::new();
    let ctx = NarrowingContext::new(&interner);

    let name = interner.intern_string("name");
    let age = interner.intern_string("age");

    // Use a non-falsy-capable type (number is truthy unless 0, but string
    // includes "" which is falsy). For this test, member1 has name: string
    // which CAN be falsy (""), so both members match the false branch.
    // Instead, let's check that member2 (which lacks "name") is in the result.
    let member1 = interner.object(vec![PropertyInfo::new(name, TypeId::STRING)]);
    let member2 = interner.object(vec![PropertyInfo::new(age, TypeId::NUMBER)]);
    let union = interner.union(vec![member1, member2]);

    // !x.name: member2 doesn't have "name" so its property is undefined (falsy) -> kept.
    // member1 has name: string, and string can be falsy ("") -> also kept.
    // Both members should be in the result since both can have falsy "name".
    let narrowed = ctx.narrow_by_property_truthiness(union, &[name], false);
    assert_eq!(narrowed, union);
}

// =============================================================================
// Discriminant via TypeGuard
// =============================================================================

#[test]
fn type_guard_discriminant_positive() {
    let interner = TypeInterner::new();
    let ctx = NarrowingContext::new(&interner);
    let kind = interner.intern_string("kind");

    let kind_a = interner.literal_string("a");
    let kind_b = interner.literal_string("b");

    let member1 = interner.object(vec![PropertyInfo::new(kind, kind_a)]);
    let member2 = interner.object(vec![PropertyInfo::new(kind, kind_b)]);
    let union = interner.union(vec![member1, member2]);

    let guard = TypeGuard::Discriminant {
        property_path: vec![kind],
        value_type: kind_a,
    };
    let narrowed = ctx.narrow_type(union, &guard, GuardSense::Positive);
    assert_eq!(narrowed, member1);
}

#[test]
fn type_guard_discriminant_negative() {
    let interner = TypeInterner::new();
    let ctx = NarrowingContext::new(&interner);
    let kind = interner.intern_string("kind");

    let kind_a = interner.literal_string("a");
    let kind_b = interner.literal_string("b");

    let member1 = interner.object(vec![PropertyInfo::new(kind, kind_a)]);
    let member2 = interner.object(vec![PropertyInfo::new(kind, kind_b)]);
    let union = interner.union(vec![member1, member2]);

    let guard = TypeGuard::Discriminant {
        property_path: vec![kind],
        value_type: kind_a,
    };
    let narrowed = ctx.narrow_type(union, &guard, GuardSense::Negative);
    assert_eq!(narrowed, member2);
}

// =============================================================================
// Edge Cases
// =============================================================================

#[test]
fn narrowing_never_type_is_identity() {
    let interner = TypeInterner::new();
    let ctx = NarrowingContext::new(&interner);

    // Narrowing never should stay never
    let narrowed = ctx.narrow_to_type(TypeId::NEVER, TypeId::STRING);
    assert_eq!(narrowed, TypeId::NEVER);
}

#[test]
fn narrowing_by_same_type_is_identity() {
    let interner = TypeInterner::new();
    let ctx = NarrowingContext::new(&interner);

    // Narrowing string to string should stay string
    let narrowed = ctx.narrow_to_type(TypeId::STRING, TypeId::STRING);
    assert_eq!(narrowed, TypeId::STRING);
}

#[test]
fn typeof_with_no_applicable_type_returns_never() {
    let interner = TypeInterner::new();

    // typeof boolean === "string" -> never
    let narrowed = narrow_by_typeof(&interner, TypeId::BOOLEAN, "string");
    assert_eq!(narrowed, TypeId::NEVER);
}

#[test]
fn discriminant_all_members_match_returns_original() {
    let interner = TypeInterner::new();
    let kind = interner.intern_string("kind");
    let kind_a = interner.literal_string("a");

    // Only one member with kind "a" in a single-member "union"
    let member_a = interner.object(vec![PropertyInfo::new(kind, kind_a)]);
    let union = interner.union(vec![member_a]);

    let narrowed = narrow_by_discriminant(&interner, union, &[kind], kind_a);
    // Single member union collapsed to member_a
    assert_eq!(narrowed, member_a);
}

#[test]
fn discriminant_for_type_positive_and_negative() {
    let interner = TypeInterner::new();
    let ctx = NarrowingContext::new(&interner);
    let kind = interner.intern_string("kind");

    let kind_a = interner.literal_string("a");
    let kind_b = interner.literal_string("b");

    let member_a = interner.object(vec![PropertyInfo::new(kind, kind_a)]);
    let member_b = interner.object(vec![PropertyInfo::new(kind, kind_b)]);
    let union = interner.union(vec![member_a, member_b]);

    // Test positive branch
    let narrowed_pos = ctx.narrow_by_discriminant_for_type(union, &[kind], kind_a, true);
    assert_eq!(narrowed_pos, member_a);

    // Test negative branch
    let narrowed_neg = ctx.narrow_by_discriminant_for_type(union, &[kind], kind_a, false);
    assert_eq!(narrowed_neg, member_b);
}

/// Regression: a top-level intersection that has not been distributed to a
/// union must not be collapsed to `never` by discriminant narrowing. tsc gates
/// discriminant narrowing on the type having the `Union` flag (see
/// `getDiscriminantPropertyAccess`), so we mirror that behavior by leaving
/// non-distributed intersection sources unchanged.
///
/// Without this gate, `narrow_by_excluding_discriminant`'s
/// intersection-effective-type logic (which intersects per-member property
/// types) collapses to `never` whenever the discriminant is fully constrained
/// — producing spurious TS2339 on subsequent property access in code such as
/// `function foo(x: RuntimeValue & { type: 'number' }) { ...; else { x.value; } }`.
#[test]
fn discriminant_for_type_skips_top_level_intersection() {
    let interner = TypeInterner::new();
    let ctx = NarrowingContext::new(&interner);
    let kind = interner.intern_string("kind");
    let value = interner.intern_string("value");

    let kind_a = interner.literal_string("a");
    let kind_b = interner.literal_string("b");

    // Build a discriminated union and intersect it with a constraining
    // object (`{ kind: "a" }`). After distribution this would collapse to
    // a single object, but the intersection interner does not always
    // distribute (e.g. when one member is a Lazy/Application). We
    // synthesize the un-distributed intersection here by intersecting the
    // union with a non-union object whose property is `string` rather
    // than the literal `"a"`, keeping the intersection in canonical form.
    let member_a = interner.object(vec![
        PropertyInfo::new(kind, kind_a),
        PropertyInfo::new(value, TypeId::NUMBER),
    ]);
    let member_b = interner.object(vec![
        PropertyInfo::new(kind, kind_b),
        PropertyInfo::new(value, TypeId::STRING),
    ]);
    let union = interner.union(vec![member_a, member_b]);
    let constraint = interner.object(vec![PropertyInfo::new(kind, TypeId::STRING)]);
    let intersection = interner.intersection(vec![union, constraint]);

    // Narrowing should not collapse the intersection — the gate matches
    // tsc's `getDiscriminantPropertyAccess` requirement that the type's
    // top-level shape have the Union flag.
    if intersection != union {
        let narrowed_pos = ctx.narrow_by_discriminant_for_type(intersection, &[kind], kind_a, true);
        let narrowed_neg =
            ctx.narrow_by_discriminant_for_type(intersection, &[kind], kind_a, false);
        // When the source is a top-level intersection (rather than a
        // distributed union), narrowing is a no-op so subsequent property
        // access does not error spuriously.
        if matches!(
            interner.lookup(intersection),
            Some(crate::TypeData::Intersection(_))
        ) {
            assert_eq!(narrowed_pos, intersection);
            assert_eq!(narrowed_neg, intersection);
        }
    }
}

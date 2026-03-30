//! Tests for union type checking (SOLV-3).

use super::*;
use crate::TypeInterner;
use crate::def::DefId;

// =============================================================================
// Debug Test - Verify union normalization
// =============================================================================

#[test]
fn debug_union_normalization() {
    let interner = TypeInterner::new();

    // Union containing `any` is normalized to just `any` (TypeScript behavior)
    let any_or_string = interner.union(vec![TypeId::ANY, TypeId::STRING]);
    assert_eq!(
        any_or_string,
        TypeId::ANY,
        "any | string should normalize to any"
    );

    // Union without `any` stays as a union
    let string_or_number = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);
    assert_ne!(string_or_number, TypeId::STRING);
    assert_ne!(string_or_number, TypeId::NUMBER);

    // Verify it's a union type
    if let Some(TypeData::Union(_)) = interner.lookup(string_or_number) {
        // OK
    } else {
        panic!("Expected union type");
    }
}

// =============================================================================
// Union Type Checking Tests - SOLV-3
// =============================================================================

#[test]
fn test_union_literal_narrow_to_wider() {
    // type A = 1 | 2; type B = 1 | 2 | 3; let a: A = 1 as B;
    // B (1 | 2 | 3) is NOT a subtype of A (1 | 2) because B has member 3 that A doesn't have
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let one = interner.literal_number(1.0);
    let two = interner.literal_number(2.0);
    let three = interner.literal_number(3.0);

    let type_a = interner.union(vec![one, two]);
    let type_b = interner.union(vec![one, two, three]);

    // B is NOT a subtype of A (B has extra member 3)
    assert!(!checker.is_subtype_of(type_b, type_a));

    // A IS a subtype of B (all members of A are in B)
    assert!(checker.is_subtype_of(type_a, type_b));

    // Each literal is subtype of both unions
    assert!(checker.is_subtype_of(one, type_a));
    assert!(checker.is_subtype_of(one, type_b));
    assert!(checker.is_subtype_of(two, type_a));
    assert!(checker.is_subtype_of(two, type_b));
    assert!(checker.is_subtype_of(three, type_b));

    // 3 is NOT in A
    assert!(!checker.is_subtype_of(three, type_a));
}

#[test]
fn test_union_normalization_with_any() {
    // Unions containing `any` are normalized to just `any` (TypeScript behavior)
    let interner = TypeInterner::new();

    // any | string normalizes to any
    let any_or_string = interner.union(vec![TypeId::ANY, TypeId::STRING]);
    assert_eq!(any_or_string, TypeId::ANY);

    // any | number normalizes to any
    let any_or_number = interner.union(vec![TypeId::ANY, TypeId::NUMBER]);
    assert_eq!(any_or_number, TypeId::ANY);

    // any | string | number normalizes to any
    let any_or_string_or_number = interner.union(vec![TypeId::ANY, TypeId::STRING, TypeId::NUMBER]);
    assert_eq!(any_or_string_or_number, TypeId::ANY);
}

#[test]
fn test_union_without_any_stays_union() {
    // Unions without `any` remain as unions
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let string_or_number = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);

    // This is a union type, not a single type
    assert_ne!(string_or_number, TypeId::STRING);
    assert_ne!(string_or_number, TypeId::NUMBER);

    // string is subtype of string | number
    assert!(checker.is_subtype_of(TypeId::STRING, string_or_number));

    // number is subtype of string | number
    assert!(checker.is_subtype_of(TypeId::NUMBER, string_or_number));

    // string | number is subtype of string | number | boolean
    let string_or_number_or_boolean =
        interner.union(vec![TypeId::STRING, TypeId::NUMBER, TypeId::BOOLEAN]);
    assert!(checker.is_subtype_of(string_or_number, string_or_number_or_boolean));

    // string | number is NOT subtype of string | boolean (number is not in the target)
    let string_or_boolean = interner.union(vec![TypeId::STRING, TypeId::BOOLEAN]);
    assert!(!checker.is_subtype_of(string_or_number, string_or_boolean));
}

// =============================================================================
// Union Literal Widening Tests - SOLV-3
// =============================================================================

#[test]
fn test_union_literal_widening_to_optional_properties() {
    // {a: 'x'} | {b: 'y'} should be assignable to {a?: string, b?: string}
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let a_literal = interner.literal_string("x");
    let b_literal = interner.literal_string("y");

    let obj_a = interner.object(vec![PropertyInfo::new(
        interner.intern_string("a"),
        a_literal,
    )]);

    let obj_b = interner.object(vec![PropertyInfo::new(
        interner.intern_string("b"),
        b_literal,
    )]);

    let union_ab = interner.union2(obj_a, obj_b);

    let target = interner.object(vec![
        PropertyInfo::opt(interner.intern_string("a"), TypeId::STRING),
        PropertyInfo::opt(interner.intern_string("b"), TypeId::STRING),
    ]);

    // Union should be assignable to target with all optional properties
    assert!(
        checker.is_subtype_of(union_ab, target),
        "{{a: 'x'}} | {{b: 'y'}} should be assignable to {{a?: string, b?: string}}"
    );
}

#[test]
fn test_union_literal_widening_with_different_types() {
    // {a: 1} | {b: true} should be assignable to {a?: number, b?: boolean}
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let one_literal = interner.literal_number(1.0);
    let true_literal = TypeId::BOOLEAN_TRUE;

    let obj_a = interner.object(vec![PropertyInfo::new(
        interner.intern_string("a"),
        one_literal,
    )]);

    let obj_b = interner.object(vec![PropertyInfo::new(
        interner.intern_string("b"),
        true_literal,
    )]);

    let union_ab = interner.union2(obj_a, obj_b);

    let target = interner.object(vec![
        PropertyInfo::opt(interner.intern_string("a"), TypeId::NUMBER),
        PropertyInfo::opt(interner.intern_string("b"), TypeId::BOOLEAN),
    ]);

    assert!(
        checker.is_subtype_of(union_ab, target),
        "{{a: 1}} | {{b: true}} should be assignable to {{a?: number, b?: boolean}}"
    );
}

#[test]
fn test_union_not_assignable_to_mixed_optional_required() {
    // {a: 'x'} | {b: 'y'} should NOT be assignable to {a: string, b?: string}
    // because 'a' is required in target
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let a_literal = interner.literal_string("x");
    let b_literal = interner.literal_string("y");

    let obj_a = interner.object(vec![PropertyInfo::new(
        interner.intern_string("a"),
        a_literal,
    )]);

    let obj_b = interner.object(vec![PropertyInfo::new(
        interner.intern_string("b"),
        b_literal,
    )]);

    let union_ab = interner.union2(obj_a, obj_b);

    let target = interner.object(vec![
        PropertyInfo {
            name: interner.intern_string("a"),
            type_id: TypeId::STRING,
            write_type: TypeId::STRING,
            optional: false, // Required!
            readonly: false,
            is_method: false,
            is_class_prototype: false,
            visibility: Visibility::Public,
            parent_id: None,
            declaration_order: 0,
            is_string_named: false,
        },
        PropertyInfo::opt(interner.intern_string("b"), TypeId::STRING),
    ]);

    // Should NOT be assignable because obj_b doesn't have required property 'a'
    assert!(
        !checker.is_subtype_of(union_ab, target),
        "{{a: 'x'}} | {{b: 'y'}} should NOT be assignable to {{a: string, b?: string}}"
    );
}

#[test]
fn test_union_with_type_mismatch_not_assignable() {
    // {a: 'x'} | {b: 'y'} should NOT be assignable to {a?: number, b?: string}
    // because 'a' type is incompatible
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let a_literal = interner.literal_string("x");
    let b_literal = interner.literal_string("y");

    let obj_a = interner.object(vec![PropertyInfo::new(
        interner.intern_string("a"),
        a_literal,
    )]);

    let obj_b = interner.object(vec![PropertyInfo::new(
        interner.intern_string("b"),
        b_literal,
    )]);

    let union_ab = interner.union2(obj_a, obj_b);

    let target = interner.object(vec![
        PropertyInfo {
            name: interner.intern_string("a"),
            type_id: TypeId::NUMBER, // Type mismatch!
            write_type: TypeId::NUMBER,
            optional: true,
            readonly: false,
            is_method: false,
            is_class_prototype: false,
            visibility: Visibility::Public,
            parent_id: None,
            declaration_order: 0,
            is_string_named: false,
        },
        PropertyInfo::opt(interner.intern_string("b"), TypeId::STRING),
    ]);

    assert!(
        !checker.is_subtype_of(union_ab, target),
        "Should fail due to type mismatch on property 'a'"
    );
}

#[test]
fn test_union_to_object_with_all_optional_and_extra_source_props() {
    // {a: 'x', c: 1} | {b: 'y'} should be assignable to {a?: string, b?: string}
    // Extra property 'c' in source is OK
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let a_literal = interner.literal_string("x");
    let b_literal = interner.literal_string("y");
    let one_literal = interner.literal_number(1.0);

    let obj_a = interner.object(vec![
        PropertyInfo::new(interner.intern_string("a"), a_literal),
        PropertyInfo::new(interner.intern_string("c"), one_literal),
    ]);

    let obj_b = interner.object(vec![PropertyInfo::new(
        interner.intern_string("b"),
        b_literal,
    )]);

    let union_ab = interner.union2(obj_a, obj_b);

    let target = interner.object(vec![
        PropertyInfo::opt(interner.intern_string("a"), TypeId::STRING),
        PropertyInfo::opt(interner.intern_string("b"), TypeId::STRING),
    ]);

    assert!(
        checker.is_subtype_of(union_ab, target),
        "Union with extra properties should be assignable"
    );
}

// =============================================================================
// Union vs Intersection Assignability Tests
// =============================================================================

#[test]
fn test_union_to_intersection_distributivity() {
    // (A | B) <: (C & D) requires each union member to satisfy ALL intersection members
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let obj_a = interner.object(vec![PropertyInfo::new(
        interner.intern_string("a"),
        TypeId::STRING,
    )]);

    let obj_b = interner.object(vec![PropertyInfo::new(
        interner.intern_string("b"),
        TypeId::NUMBER,
    )]);

    let obj_c = interner.object(vec![PropertyInfo::new(
        interner.intern_string("c"),
        TypeId::STRING,
    )]);

    let obj_d = interner.object(vec![PropertyInfo::new(
        interner.intern_string("d"),
        TypeId::NUMBER,
    )]);

    // Create intersection C & D
    let intersection_cd = interner.intersection2(obj_c, obj_d);

    // Create union A | B
    let union_ab = interner.union2(obj_a, obj_b);

    // (A | B) should NOT be <: (C & D) because neither A nor B has both c and d
    assert!(
        !checker.is_subtype_of(union_ab, intersection_cd),
        "Union should not satisfy intersection requiring different properties"
    );
}

#[test]
fn test_union_to_intersection_with_overlap() {
    // {a: string} | {b: number} should be assignable to {a?: string} & {b?: number}
    // Each union member satisfies at least one intersection member
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let obj_a = interner.object(vec![PropertyInfo::new(
        interner.intern_string("a"),
        TypeId::STRING,
    )]);

    let obj_b = interner.object(vec![PropertyInfo::new(
        interner.intern_string("b"),
        TypeId::NUMBER,
    )]);

    let obj_c = interner.object(vec![PropertyInfo::opt(
        interner.intern_string("a"),
        TypeId::STRING,
    )]);

    let obj_d = interner.object(vec![PropertyInfo::opt(
        interner.intern_string("b"),
        TypeId::NUMBER,
    )]);

    let intersection_cd = interner.intersection2(obj_c, obj_d);
    let union_ab = interner.union2(obj_a, obj_b);

    // Union should satisfy intersection with optional properties
    // because {a: string} <: {a?: string} and {b: number} <: {b?: number}
    // But wait - intersection requires BOTH to be satisfied by EACH union member
    // So {a: string} must satisfy {a?: string} AND {b?: number}
    // {a: string} does NOT satisfy {b?: number} because 'b' is optional but missing is OK
    // Actually, {b?: number} can be satisfied by not having 'b' (it's optional)
    // Let's verify this behavior

    // In TypeScript: (A | B) <: (C & D) means:
    // - A <: C AND A <: D (first union member must satisfy both intersection members)
    // - B <: C AND B <: D (second union member must satisfy both intersection members)

    // {a: string} <: {a?: string} - YES
    // {a: string} <: {b?: number} - YES (b is optional, so not having it is OK)
    // {b: number} <: {a?: string} - YES (a is optional)
    // {b: number} <: {b?: number} - YES

    assert!(
        checker.is_subtype_of(union_ab, intersection_cd),
        "Union with disjoint properties should satisfy intersection of optional properties"
    );
}

// =============================================================================
// Empty Union/Never Type Tests
// =============================================================================

#[test]
fn test_never_is_subtype_of_union() {
    // never is assignable to everything, including unions
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let string_or_number = interner.union2(TypeId::STRING, TypeId::NUMBER);

    assert!(
        checker.is_subtype_of(TypeId::NEVER, string_or_number),
        "never should be subtype of any union"
    );
}

#[test]
fn test_empty_union_is_never() {
    // Empty union normalizes to never
    let interner = TypeInterner::new();
    let empty_union = interner.union(vec![]);

    assert_eq!(empty_union, TypeId::NEVER, "Empty union should be never");
}

#[test]
fn test_union_containing_never_simplifies() {
    // string | never should simplify to string
    let interner = TypeInterner::new();
    let string_or_never = interner.union2(TypeId::STRING, TypeId::NEVER);

    assert_eq!(
        string_or_never,
        TypeId::STRING,
        "Union with never should simplify to the other member"
    );
}

// =============================================================================
// Discriminant Property Tests
// =============================================================================

#[test]
fn test_discriminated_union_narrowing() {
    // Unions with discriminant properties allow narrowing
    // { kind: 'circle', radius: number } | { kind: 'square', side: number }
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let circle_literal = interner.literal_string("circle");
    let square_literal = interner.literal_string("square");

    let circle = interner.object(vec![
        PropertyInfo::readonly(interner.intern_string("kind"), circle_literal),
        PropertyInfo::new(interner.intern_string("radius"), TypeId::NUMBER),
    ]);

    let square = interner.object(vec![
        PropertyInfo::readonly(interner.intern_string("kind"), square_literal),
        PropertyInfo::new(interner.intern_string("side"), TypeId::NUMBER),
    ]);

    let shape_union = interner.union2(circle, square);

    // The union should be assignable to {kind?: 'circle' | 'square', radius?: number, side?: number}
    let target = interner.object(vec![
        PropertyInfo {
            name: interner.intern_string("kind"),
            type_id: interner.union2(circle_literal, square_literal),
            write_type: interner.union2(circle_literal, square_literal),
            optional: true,
            readonly: false,
            is_method: false,
            is_class_prototype: false,
            visibility: Visibility::Public,
            parent_id: None,
            declaration_order: 0,
            is_string_named: false,
        },
        PropertyInfo::opt(interner.intern_string("radius"), TypeId::NUMBER),
        PropertyInfo::opt(interner.intern_string("side"), TypeId::NUMBER),
    ]);

    assert!(
        checker.is_subtype_of(shape_union, target),
        "Discriminated union should be assignable to optional properties"
    );
}

#[test]
fn test_union_with_common_discriminant_property() {
    // { type: 'a', a: string } | { type: 'b', b: number }
    // Should be assignable to { type: 'a' | 'b', a?: string, b?: number }
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let type_a = interner.literal_string("a");
    let type_b = interner.literal_string("b");

    let variant_a = interner.object(vec![
        PropertyInfo::readonly(interner.intern_string("type"), type_a),
        PropertyInfo::new(interner.intern_string("a"), TypeId::STRING),
    ]);

    let variant_b = interner.object(vec![
        PropertyInfo::readonly(interner.intern_string("type"), type_b),
        PropertyInfo::new(interner.intern_string("b"), TypeId::NUMBER),
    ]);

    let union_variants = interner.union2(variant_a, variant_b);

    // Target has required 'type' property (the discriminant)
    // and optional properties for each variant
    let target = interner.object(vec![
        PropertyInfo {
            name: interner.intern_string("type"),
            type_id: interner.union2(type_a, type_b),
            write_type: interner.union2(type_a, type_b),
            optional: false, // Required discriminant!
            readonly: false,
            is_method: false,
            is_class_prototype: false,
            visibility: Visibility::Public,
            parent_id: None,
            declaration_order: 0,
            is_string_named: false,
        },
        PropertyInfo::opt(interner.intern_string("a"), TypeId::STRING),
        PropertyInfo::opt(interner.intern_string("b"), TypeId::NUMBER),
    ]);

    // This should NOT work with the relaxed rule because 'type' is required
    // But it should work with the standard check because each union member has 'type'
    // Actually, the standard union source check should handle this:
    // Each union member must be <: target
    // { type: 'a', a: string } <: { type: 'a' | 'b', a?: string, b?: number }
    // - type: 'a' <: 'a' | 'b' ✓
    // - a: string <: a?: string ✓
    // - missing 'b' is OK because it's optional ✓
    // So each member IS assignable to the target
    assert!(
        checker.is_subtype_of(union_variants, target),
        "Union with common discriminant should be assignable"
    );
}

// =============================================================================
// Union to Object with Empty Target Tests
// =============================================================================

#[test]
fn test_union_to_empty_object() {
    // {a: string} | {b: number} should be assignable to {}
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let obj_a = interner.object(vec![PropertyInfo::new(
        interner.intern_string("a"),
        TypeId::STRING,
    )]);

    let obj_b = interner.object(vec![PropertyInfo::new(
        interner.intern_string("b"),
        TypeId::NUMBER,
    )]);

    let union_ab = interner.union2(obj_a, obj_b);
    let empty_object = interner.object(vec![]);

    // Both objects are assignable to empty object, so union should be too
    assert!(
        checker.is_subtype_of(union_ab, empty_object),
        "Union of objects should be assignable to empty object"
    );
}

// =============================================================================
// Union Assignability with Index Signatures
// =============================================================================

#[test]
fn test_union_to_object_with_index_signature() {
    // {a: string} | {b: number} should NOT use the relaxed rule when target has index signature
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let obj_a = interner.object(vec![PropertyInfo::new(
        interner.intern_string("a"),
        TypeId::STRING,
    )]);

    let obj_b = interner.object(vec![PropertyInfo::new(
        interner.intern_string("b"),
        TypeId::NUMBER,
    )]);

    let union_ab = interner.union2(obj_a, obj_b);

    // Target has index signature, so the relaxed rule should NOT apply
    let target_with_index = interner.object_with_index(ObjectShape {
        symbol: None,
        flags: ObjectFlags::empty(),
        properties: vec![PropertyInfo::opt(
            interner.intern_string("a"),
            TypeId::STRING,
        )],
        string_index: Some(IndexSignature {
            key_type: TypeId::STRING,
            value_type: TypeId::STRING,
            readonly: false,
            param_name: None,
        }),
        number_index: None,
    });

    // Standard union check should apply - each member must be assignable
    // obj_b doesn't have 'a' property, and while 'a' is optional,
    // the index signature might not satisfy it properly
    // This test verifies we're NOT using the relaxed rule
    let result = checker.is_subtype_of(union_ab, target_with_index);
    // We don't assert the result here, just verify it doesn't panic/crash
    // The actual behavior depends on how index signatures are handled
    let _ = result;
}

// =============================================================================
// Regression: bypass_evaluation must resolve Lazy target types
// =============================================================================
// When simplify_union_members uses SubtypeChecker with bypass_evaluation=true,
// Lazy(DefId) types nested inside ObjectWithIndex (e.g., index signature value
// types) must still be resolved to their structural forms. Otherwise, distinct
// callable interfaces like `(a: number) => number` vs `(a: number) => string`
// are treated as identical, causing one union member to be incorrectly removed.

#[test]
fn test_bypass_evaluation_resolves_lazy_index_value_types() {
    // Regression test: when simplify_union_members uses SubtypeChecker with
    // bypass_evaluation=true, Lazy(DefId) index signature value types must
    // still be resolved to structural forms. Without this fix, two distinct
    // ObjectWithIndex types whose index signature values are Lazy references
    // to different types would appear structurally identical, causing one
    // member to be incorrectly removed from the union.
    //
    // Reproduces: contextualTypeWithUnionTypeIndexSignatures false TS2322
    //   interface SomeType { (a: number): number; }
    //   interface SomeType2 { (a: number): string; }
    //   interface A { [k: string]: SomeType; }
    //   interface B { [k: string]: SomeType2; }
    //   var x: A | B = { z: a => a };  // should be OK
    let interner = TypeInterner::new();
    let mut env = TypeEnvironment::new();

    // Two distinct structural types behind Lazy references.
    // Using simple object types (not callables) for test simplicity —
    // the key invariant is that they're *different* types behind Lazy.
    let type_a = interner.object(vec![PropertyInfo::new(
        interner.intern_string("x"),
        TypeId::NUMBER,
    )]);
    let type_b = interner.object(vec![PropertyInfo::new(
        interner.intern_string("x"),
        TypeId::STRING,
    )]);

    // Register as Lazy(DefId) — simulates interface declarations
    let def_a = DefId(100);
    let def_b = DefId(101);
    env.insert_def(def_a, type_a);
    env.insert_def(def_b, type_b);

    let lazy_a = interner.lazy(def_a);
    let lazy_b = interner.lazy(def_b);

    // Create ObjectWithIndex types using Lazy value types:
    //   A = { [k: string]: Lazy(100) }  -> { [k: string]: {x: number} }
    //   B = { [k: string]: Lazy(101) }  -> { [k: string]: {x: string} }
    let obj_a = interner.object_with_index(ObjectShape {
        symbol: None,
        flags: ObjectFlags::empty(),
        properties: vec![],
        string_index: Some(IndexSignature {
            key_type: TypeId::STRING,
            value_type: lazy_a,
            readonly: false,
            param_name: None,
        }),
        number_index: None,
    });

    let obj_b = interner.object_with_index(ObjectShape {
        symbol: None,
        flags: ObjectFlags::empty(),
        properties: vec![],
        string_index: Some(IndexSignature {
            key_type: TypeId::STRING,
            value_type: lazy_b,
            readonly: false,
            param_name: None,
        }),
        number_index: None,
    });

    // With bypass_evaluation=true and a resolver, the Lazy types should be
    // resolved by resolve_lazy_type, so A and B should NOT be subtypes.
    let mut checker = SubtypeChecker::with_resolver(&interner, &env);
    checker.bypass_evaluation = true;

    // A is NOT a subtype of B (different index value types)
    assert!(
        !checker.is_subtype_of(obj_a, obj_b),
        "ObjectWithIndex with Lazy(number-obj) should NOT be subtype of \
         ObjectWithIndex with Lazy(string-obj) even with bypass_evaluation"
    );

    // B is NOT a subtype of A
    assert!(
        !checker.is_subtype_of(obj_b, obj_a),
        "ObjectWithIndex with Lazy(string-obj) should NOT be subtype of \
         ObjectWithIndex with Lazy(number-obj) even with bypass_evaluation"
    );
}

// =============================================================================
// Discriminated Union: Optional Property Narrowing
// =============================================================================

#[test]
fn test_discriminated_union_optional_property_narrowing() {
    // Regression: { foo?: number | undefined } should be assignable to
    // { foo?: undefined } | { foo: number }.
    //
    // The discriminated union algorithm narrows the source by each possible
    // discriminant value. When narrowing to `number`, the property must be
    // treated as non-optional (the property is present with that value).
    // Previously, narrow_object_property preserved the optional flag, causing
    // the narrowed { foo?: number } to fail against { foo: number } (required).
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let foo_name = interner.intern_string("foo");

    // Source: { foo?: number | undefined }
    let source = interner.object(vec![PropertyInfo {
        name: foo_name,
        type_id: interner.union2(TypeId::NUMBER, TypeId::UNDEFINED),
        write_type: interner.union2(TypeId::NUMBER, TypeId::UNDEFINED),
        optional: true,
        readonly: false,
        is_method: false,
        is_class_prototype: false,
        visibility: Visibility::Public,
        parent_id: None,
        declaration_order: 0,
        is_string_named: false,
    }]);

    // Target member 1: { foo?: undefined }
    let target_a = interner.object(vec![PropertyInfo {
        name: foo_name,
        type_id: TypeId::UNDEFINED,
        write_type: TypeId::UNDEFINED,
        optional: true,
        readonly: false,
        is_method: false,
        is_class_prototype: false,
        visibility: Visibility::Public,
        parent_id: None,
        declaration_order: 0,
        is_string_named: false,
    }]);

    // Target member 2: { foo: number }
    let target_b = interner.object(vec![PropertyInfo::new(foo_name, TypeId::NUMBER)]);

    let target_union = interner.union2(target_a, target_b);

    // This should pass via discriminated union decomposition:
    // - foo=number matches { foo: number }
    // - foo=undefined matches { foo?: undefined }
    assert!(
        checker.is_subtype_of(source, target_union),
        "{{ foo?: number | undefined }} should be assignable to {{ foo?: undefined }} | {{ foo: number }}"
    );
}

#[test]
fn test_discriminated_union_narrowing_preserves_non_discriminant_props() {
    // { kind: 'a' | 'b', value: string } should be assignable to
    // { kind: 'a', value: string } | { kind: 'b', value: string }
    // via discriminated union decomposition.
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let kind_name = interner.intern_string("kind");
    let value_name = interner.intern_string("value");
    let lit_a = interner.literal_string("a");
    let lit_b = interner.literal_string("b");

    // Source: { kind: 'a' | 'b', value: string }
    let source = interner.object(vec![
        PropertyInfo::new(kind_name, interner.union2(lit_a, lit_b)),
        PropertyInfo::new(value_name, TypeId::STRING),
    ]);

    // Target: { kind: 'a', value: string } | { kind: 'b', value: string }
    let target_a = interner.object(vec![
        PropertyInfo::new(kind_name, lit_a),
        PropertyInfo::new(value_name, TypeId::STRING),
    ]);
    let target_b = interner.object(vec![
        PropertyInfo::new(kind_name, lit_b),
        PropertyInfo::new(value_name, TypeId::STRING),
    ]);
    let target_union = interner.union2(target_a, target_b);

    assert!(
        checker.is_subtype_of(source, target_union),
        "Object with union discriminant should be assignable to discriminated union"
    );
}

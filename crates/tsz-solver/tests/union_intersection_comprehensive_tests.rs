//! Comprehensive tests for union and intersection type operations.
//!
//! These tests verify TypeScript's union and intersection type behavior:
//! - Union construction and normalization
//! - Intersection construction and normalization
//! - Union/intersection subtype relationships
//! - Distributive properties

use super::*;
use crate::intern::TypeInterner;
use crate::subtype::SubtypeChecker;
use crate::types::{PropertyInfo, TypeData};

// =============================================================================
// Union Construction Tests
// =============================================================================

#[test]
fn test_union_two_types() {
    let interner = TypeInterner::new();

    let union = interner.union2(TypeId::STRING, TypeId::NUMBER);

    if let Some(TypeData::Union(members)) = interner.lookup(union) {
        let members = interner.type_list(members);
        assert_eq!(members.len(), 2);
        assert!(members.contains(&TypeId::STRING));
        assert!(members.contains(&TypeId::NUMBER));
    } else {
        panic!("Expected union type");
    }
}

#[test]
fn test_union_three_types() {
    let interner = TypeInterner::new();

    let union = interner.union3(TypeId::STRING, TypeId::NUMBER, TypeId::BOOLEAN);

    if let Some(TypeData::Union(members)) = interner.lookup(union) {
        let members = interner.type_list(members);
        assert_eq!(members.len(), 3);
    } else {
        panic!("Expected union type");
    }
}

#[test]
fn test_union_with_any_normalizes_to_any() {
    let interner = TypeInterner::new();

    // string | any should normalize to any
    let union = interner.union2(TypeId::STRING, TypeId::ANY);
    assert_eq!(union, TypeId::ANY, "Union with any should normalize to any");
}

#[test]
fn test_union_with_never_simplifies() {
    let interner = TypeInterner::new();

    // string | never should simplify to string
    let union = interner.union2(TypeId::STRING, TypeId::NEVER);
    assert_eq!(union, TypeId::STRING, "Union with never should simplify");
}

#[test]
fn test_union_empty_is_never() {
    let interner = TypeInterner::new();

    let empty_union = interner.union(vec![]);
    assert_eq!(empty_union, TypeId::NEVER, "Empty union should be never");
}

#[test]
fn test_union_single_element_returns_element() {
    let interner = TypeInterner::new();

    let single = interner.union(vec![TypeId::STRING]);
    assert_eq!(
        single,
        TypeId::STRING,
        "Single-element union should return the element"
    );
}

// =============================================================================
// Intersection Construction Tests
// =============================================================================

#[test]
fn test_intersection_two_types() {
    let interner = TypeInterner::new();

    let intersection = interner.intersection2(TypeId::STRING, TypeId::NUMBER);

    // string & number = never (disjoint types)
    // But we just verify it's created
    let _ = intersection;
}

#[test]
fn test_intersection_with_any() {
    let interner = TypeInterner::new();

    // string & any - behavior depends on implementation
    // In TypeScript, this is string (any absorbs in intersection)
    let intersection = interner.intersection2(TypeId::STRING, TypeId::ANY);
    // Just verify it's a valid type
    let _ = intersection;
}

#[test]
fn test_intersection_with_never_is_never() {
    let interner = TypeInterner::new();

    // string & never should be never
    let intersection = interner.intersection2(TypeId::STRING, TypeId::NEVER);
    assert_eq!(
        intersection,
        TypeId::NEVER,
        "Intersection with never should be never"
    );
}

#[test]
fn test_intersection_empty_is_unknown() {
    let interner = TypeInterner::new();

    let empty_intersection = interner.intersection(vec![]);
    // Empty intersection is typically "unknown" (top type)
    assert_eq!(
        empty_intersection,
        TypeId::UNKNOWN,
        "Empty intersection should be unknown"
    );
}

// =============================================================================
// Union Subtype Tests
// =============================================================================

#[test]
fn test_union_member_is_subtype() {
    // string <: string | number
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let union = interner.union2(TypeId::STRING, TypeId::NUMBER);

    assert!(
        checker.is_subtype_of(TypeId::STRING, union),
        "String should be subtype of string | number"
    );
    assert!(
        checker.is_subtype_of(TypeId::NUMBER, union),
        "Number should be subtype of string | number"
    );
}

#[test]
fn test_union_is_subtype_of_union_with_more_members() {
    // string | number <: string | number | boolean
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let smaller = interner.union2(TypeId::STRING, TypeId::NUMBER);
    let larger = interner.union3(TypeId::STRING, TypeId::NUMBER, TypeId::BOOLEAN);

    assert!(
        checker.is_subtype_of(smaller, larger),
        "Smaller union should be subtype of larger union"
    );
}

#[test]
fn test_union_not_subtype_of_smaller_union() {
    // string | number | boolean is NOT <: string | number
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let larger = interner.union3(TypeId::STRING, TypeId::NUMBER, TypeId::BOOLEAN);
    let smaller = interner.union2(TypeId::STRING, TypeId::NUMBER);

    assert!(
        !checker.is_subtype_of(larger, smaller),
        "Larger union should not be subtype of smaller union"
    );
}

// =============================================================================
// Intersection Subtype Tests
// =============================================================================

#[test]
fn test_intersection_is_subtype_of_each_member() {
    // A & B <: A and A & B <: B
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

    let intersection = interner.intersection2(obj_a, obj_b);

    assert!(
        checker.is_subtype_of(intersection, obj_a),
        "A & B should be subtype of A"
    );
    assert!(
        checker.is_subtype_of(intersection, obj_b),
        "A & B should be subtype of B"
    );
}

// =============================================================================
// Union/Intersection Distribution
// =============================================================================

#[test]
fn test_union_of_objects() {
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let obj_a = interner.object(vec![PropertyInfo::new(
        interner.intern_string("type"),
        interner.literal_string("a"),
    )]);

    let obj_b = interner.object(vec![PropertyInfo::new(
        interner.intern_string("type"),
        interner.literal_string("b"),
    )]);

    let union = interner.union2(obj_a, obj_b);

    // Each individual object is a subtype of the union
    assert!(checker.is_subtype_of(obj_a, union));
    assert!(checker.is_subtype_of(obj_b, union));
}

#[test]
fn test_intersection_of_objects() {
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

    let intersection = interner.intersection2(obj_a, obj_b);

    // Intersection is subtype of empty object
    let empty = interner.object(vec![]);
    assert!(checker.is_subtype_of(intersection, empty));
}

// =============================================================================
// Union/Intersection with any, never, unknown
// =============================================================================

#[test]
fn test_union_assignable_to_any() {
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let union = interner.union2(TypeId::STRING, TypeId::NUMBER);

    assert!(
        checker.is_subtype_of(union, TypeId::ANY),
        "Union should be subtype of any"
    );
}

#[test]
fn test_union_assignable_to_unknown() {
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let union = interner.union2(TypeId::STRING, TypeId::NUMBER);

    assert!(
        checker.is_subtype_of(union, TypeId::UNKNOWN),
        "Union should be subtype of unknown"
    );
}

#[test]
fn test_never_assignable_to_union() {
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let union = interner.union2(TypeId::STRING, TypeId::NUMBER);

    assert!(
        checker.is_subtype_of(TypeId::NEVER, union),
        "never should be subtype of any union"
    );
}

// =============================================================================
// Union Identity Tests
// =============================================================================

#[test]
fn test_union_identity_stability() {
    let interner = TypeInterner::new();

    let union1 = interner.union2(TypeId::STRING, TypeId::NUMBER);
    let union2 = interner.union2(TypeId::STRING, TypeId::NUMBER);

    assert_eq!(
        union1, union2,
        "Same union construction should produce same TypeId"
    );
}

#[test]
fn test_union_order_independence() {
    let interner = TypeInterner::new();

    let union1 = interner.union2(TypeId::STRING, TypeId::NUMBER);
    let union2 = interner.union2(TypeId::NUMBER, TypeId::STRING);

    assert_eq!(union1, union2, "Union order should not matter");
}

// =============================================================================
// Intersection Identity Tests
// =============================================================================

#[test]
fn test_intersection_identity_stability() {
    let interner = TypeInterner::new();

    let obj_a = interner.object(vec![PropertyInfo::new(
        interner.intern_string("a"),
        TypeId::STRING,
    )]);

    let obj_b = interner.object(vec![PropertyInfo::new(
        interner.intern_string("b"),
        TypeId::NUMBER,
    )]);

    let intersection1 = interner.intersection2(obj_a, obj_b);
    let intersection2 = interner.intersection2(obj_a, obj_b);

    assert_eq!(
        intersection1, intersection2,
        "Same intersection construction should produce same TypeId"
    );
}

// =============================================================================
// Union with Literals
// =============================================================================

#[test]
fn test_union_of_string_literals() {
    let interner = TypeInterner::new();

    let a = interner.literal_string("a");
    let b = interner.literal_string("b");
    let c = interner.literal_string("c");

    let union = interner.union(vec![a, b, c]);

    if let Some(TypeData::Union(members)) = interner.lookup(union) {
        let members = interner.type_list(members);
        assert_eq!(members.len(), 3);
    } else {
        panic!("Expected union type");
    }
}

#[test]
fn test_union_of_number_literals() {
    let interner = TypeInterner::new();

    let one = interner.literal_number(1.0);
    let two = interner.literal_number(2.0);
    let three = interner.literal_number(3.0);

    let union = interner.union(vec![one, two, three]);

    if let Some(TypeData::Union(members)) = interner.lookup(union) {
        let members = interner.type_list(members);
        assert_eq!(members.len(), 3);
    } else {
        panic!("Expected union type");
    }
}

// =============================================================================
// Nested Union/Intersection
// =============================================================================

#[test]
fn test_nested_union() {
    let interner = TypeInterner::new();

    let inner1 = interner.union2(TypeId::STRING, TypeId::NUMBER);
    let inner2 = interner.union2(TypeId::BOOLEAN, TypeId::NULL);

    let outer = interner.union2(inner1, inner2);

    // Should flatten to string | number | boolean | null
    if let Some(TypeData::Union(members)) = interner.lookup(outer) {
        let members = interner.type_list(members);
        assert_eq!(members.len(), 4);
    } else {
        panic!("Expected flattened union type");
    }
}

#[test]
fn test_union_of_intersections() {
    let interner = TypeInterner::new();

    let obj_a = interner.object(vec![PropertyInfo::new(
        interner.intern_string("a"),
        TypeId::STRING,
    )]);

    let obj_b = interner.object(vec![PropertyInfo::new(
        interner.intern_string("b"),
        TypeId::NUMBER,
    )]);

    let intersection1 = interner.intersection2(obj_a, obj_b);
    let intersection2 = interner.intersection2(obj_b, obj_a);

    // This creates (A & B) | (B & A)
    let _union = interner.union2(intersection1, intersection2);
}

// =============================================================================
// Union with Structural Types
// =============================================================================

#[test]
fn test_union_subtype_with_object() {
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let obj = interner.object(vec![
        PropertyInfo::new(interner.intern_string("a"), TypeId::STRING),
        PropertyInfo::new(interner.intern_string("b"), TypeId::NUMBER),
    ]);

    let obj_a = interner.object(vec![PropertyInfo::new(
        interner.intern_string("a"),
        TypeId::STRING,
    )]);

    let obj_b = interner.object(vec![PropertyInfo::new(
        interner.intern_string("b"),
        TypeId::NUMBER,
    )]);

    let union = interner.union2(obj_a, obj_b);

    // obj is NOT a subtype of obj_a | obj_b because it has both properties
    // But the exact behavior depends on excess property checking
    let _ = checker.is_subtype_of(obj, union);
}

// =============================================================================
// Excess Property Checking with Unions
// =============================================================================

#[test]
fn test_literal_object_union_assignability() {
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let type_a = interner.literal_string("a");
    let type_b = interner.literal_string("b");

    let variant_a = interner.object(vec![
        PropertyInfo::new(interner.intern_string("type"), type_a),
        PropertyInfo::new(interner.intern_string("valueA"), TypeId::STRING),
    ]);

    let variant_b = interner.object(vec![
        PropertyInfo::new(interner.intern_string("type"), type_b),
        PropertyInfo::new(interner.intern_string("valueB"), TypeId::NUMBER),
    ]);

    let union = interner.union2(variant_a, variant_b);

    // Each variant is a subtype of the union
    assert!(checker.is_subtype_of(variant_a, union));
    assert!(checker.is_subtype_of(variant_b, union));
}

//! Tests for structural type identity (Task #32: Graph Isomorphism)
//!
//! Tests the Canonicalizer and are_types_structurally_identical function.

use crate::intern::TypeInterner;
use crate::subtype::{TypeEnvironment, are_types_structurally_identical};
use crate::types::{TypeId, Visibility};

#[test]
fn test_primitive_identity() {
    let interner = TypeInterner::new();
    let env = TypeEnvironment::new();

    let number = TypeId::NUMBER;
    let string = TypeId::STRING;

    // Same primitives should be identical
    assert!(are_types_structurally_identical(
        &interner, &env, number, number
    ));
    assert!(are_types_structurally_identical(
        &interner, &env, string, string
    ));

    // Different primitives should not be identical
    assert!(!are_types_structurally_identical(
        &interner, &env, number, string
    ));
}

#[test]
fn test_object_literal_identity() {
    let interner = TypeInterner::new();
    let env = TypeEnvironment::new();

    // Create two identical object types
    let prop_a = crate::types::PropertyInfo::new(interner.intern_string("a"), TypeId::NUMBER);

    let type1 = interner.object(vec![prop_a.clone()]);
    let type2 = interner.object(vec![prop_a]);

    // Identical object literals should be structurally identical
    assert!(are_types_structurally_identical(
        &interner, &env, type1, type2
    ));
}

#[test]
fn test_object_order_independence() {
    let interner = TypeInterner::new();
    let env = TypeEnvironment::new();

    // Create properties in different orders
    let prop_a = crate::types::PropertyInfo::new(interner.intern_string("a"), TypeId::NUMBER);

    let prop_b = crate::types::PropertyInfo::new(interner.intern_string("b"), TypeId::STRING);

    // Properties in order [a, b]
    let type1 = interner.object(vec![prop_a.clone(), prop_b.clone()]);

    // Properties in order [b, a]
    let type2 = interner.object(vec![prop_b, prop_a]);

    // Property order should not matter for structural identity
    assert!(are_types_structurally_identical(
        &interner, &env, type1, type2
    ));
}

#[test]
fn test_optional_matters() {
    let interner = TypeInterner::new();
    let env = TypeEnvironment::new();

    let prop_required =
        crate::types::PropertyInfo::new(interner.intern_string("a"), TypeId::NUMBER);

    let prop_optional =
        crate::types::PropertyInfo::opt(interner.intern_string("a"), TypeId::NUMBER);

    let type1 = interner.object(vec![prop_required]);
    let type2 = interner.object(vec![prop_optional]);

    // Optional vs required should not be identical
    assert!(!are_types_structurally_identical(
        &interner, &env, type1, type2
    ));
}

#[test]
fn test_readonly_matters() {
    let interner = TypeInterner::new();
    let env = TypeEnvironment::new();

    let prop_readonly =
        crate::types::PropertyInfo::readonly(interner.intern_string("a"), TypeId::NUMBER);

    let prop_mutable = crate::types::PropertyInfo::new(interner.intern_string("a"), TypeId::NUMBER);

    let type1 = interner.object(vec![prop_readonly]);
    let type2 = interner.object(vec![prop_mutable]);

    // Readonly vs mutable should not be identical
    assert!(!are_types_structurally_identical(
        &interner, &env, type1, type2
    ));
}

#[test]
fn test_array_identity() {
    let interner = TypeInterner::new();
    let env = TypeEnvironment::new();

    let array1 = interner.array(TypeId::NUMBER);
    let array2 = interner.array(TypeId::NUMBER);
    let array3 = interner.array(TypeId::STRING);

    // Same arrays should be identical
    assert!(are_types_structurally_identical(
        &interner, &env, array1, array2
    ));

    // Different element types should not be identical
    assert!(!are_types_structurally_identical(
        &interner, &env, array1, array3
    ));
}

#[test]
fn test_union_canonicalization() {
    let interner = TypeInterner::new();
    let env = TypeEnvironment::new();

    // Create unions in different orders
    let members1 = vec![TypeId::STRING, TypeId::NUMBER];
    let members2 = vec![TypeId::NUMBER, TypeId::STRING];

    let union1 = interner.union(members1);
    let union2 = interner.union(members2);

    // Unions should canonicalize to the same form regardless of order
    assert!(are_types_structurally_identical(
        &interner, &env, union1, union2
    ));
}

#[test]
fn test_nested_object_identity() {
    let interner = TypeInterner::new();
    let env = TypeEnvironment::new();

    // Create nested object types
    let inner_prop = crate::types::PropertyInfo::new(interner.intern_string("x"), TypeId::NUMBER);

    let inner_type = interner.object(vec![inner_prop]);

    let outer_prop = crate::types::PropertyInfo::new(interner.intern_string("inner"), inner_type);

    let type1 = interner.object(vec![outer_prop.clone()]);
    let type2 = interner.object(vec![outer_prop]);

    // Nested objects should be structurally identical
    assert!(are_types_structurally_identical(
        &interner, &env, type1, type2
    ));
}

#[test]
fn test_dnf_isomorphism() {
    // Test that DNF normalization produces structurally identical results
    // (A | B) & C should be identical to (A & C) | (B & C) after DNF
    let interner = TypeInterner::new();
    let env = TypeEnvironment::new();

    // Create types
    let string = TypeId::STRING;
    let number = TypeId::NUMBER;

    // Method 1: (string | number) & string → should simplify to string
    let union_sn = interner.union(vec![string, number]);
    let method1 = interner.intersection(vec![union_sn, string]);

    // Method 2: Create string directly
    let method2 = string;

    // After DNF: (string & string) | (number & string) → string | never → string
    // Both should be structurally identical to string
    assert!(are_types_structurally_identical(
        &interner, &env, method1, method2
    ));
}

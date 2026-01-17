//! Tests for union type checking (SOLV-3).

use super::*;

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
    if let Some(TypeKey::Union(_)) = interner.lookup(string_or_number) {
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

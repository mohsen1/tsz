//! Tests for type overlap detection (TS2367).

use crate::intern::TypeInterner;
use crate::subtype::SubtypeChecker;
use crate::types::*;

#[test]
fn test_identical_types_overlap() {
    let interner = TypeInterner::new();
    let checker = SubtypeChecker::new(&interner);

    // Identical types overlap (unless never)
    assert!(checker.are_types_overlapping(TypeId::STRING, TypeId::STRING));
    assert!(checker.are_types_overlapping(TypeId::NUMBER, TypeId::NUMBER));
    assert!(!checker.are_types_overlapping(TypeId::NEVER, TypeId::NEVER));
}

#[test]
fn test_any_unknown_overlap_with_everything_except_never() {
    let interner = TypeInterner::new();
    let checker = SubtypeChecker::new(&interner);

    // any and unknown overlap with everything except never
    assert!(checker.are_types_overlapping(TypeId::ANY, TypeId::STRING));
    assert!(checker.are_types_overlapping(TypeId::STRING, TypeId::ANY));
    assert!(checker.are_types_overlapping(TypeId::UNKNOWN, TypeId::NUMBER));
    assert!(checker.are_types_overlapping(TypeId::NUMBER, TypeId::UNKNOWN));
    assert!(!checker.are_types_overlapping(TypeId::ANY, TypeId::NEVER));
    assert!(!checker.are_types_overlapping(TypeId::UNKNOWN, TypeId::NEVER));
}

#[test]
fn test_different_primitives_do_not_overlap() {
    let interner = TypeInterner::new();
    let checker = SubtypeChecker::new(&interner);

    // Different primitives never overlap
    assert!(!checker.are_types_overlapping(TypeId::STRING, TypeId::NUMBER));
    assert!(!checker.are_types_overlapping(TypeId::NUMBER, TypeId::BOOLEAN));
    assert!(!checker.are_types_overlapping(TypeId::BOOLEAN, TypeId::BIGINT));
    assert!(!checker.are_types_overlapping(TypeId::BIGINT, TypeId::SYMBOL));
}

#[test]
fn test_literal_and_primitive_overlap() {
    let interner = TypeInterner::new();

    let string_literal = interner.literal_string("hello");
    let number_literal = interner.literal_number(42.0);

    let checker = SubtypeChecker::new(&interner);

    // Literal overlaps with its primitive type
    assert!(checker.are_types_overlapping(string_literal, TypeId::STRING));
    assert!(checker.are_types_overlapping(number_literal, TypeId::NUMBER));
}

#[test]
fn test_different_literals_of_same_primitive_do_not_overlap() {
    let interner = TypeInterner::new();

    let hello = interner.literal_string("hello");
    let world = interner.literal_string("world");
    let one = interner.literal_number(1.0);
    let two = interner.literal_number(2.0);

    let checker = SubtypeChecker::new(&interner);

    // Different string literals don't overlap
    assert!(!checker.are_types_overlapping(hello, world));

    // Different number literals don't overlap
    assert!(!checker.are_types_overlapping(one, two));
}

#[test]
fn test_same_literals_overlap() {
    let interner = TypeInterner::new();

    let hello1 = interner.literal_string("hello");
    let hello2 = interner.literal_string("hello");

    let checker = SubtypeChecker::new(&interner);

    // Same literals overlap
    assert!(checker.are_types_overlapping(hello1, hello2));
}

#[test]
fn test_object_property_type_mismatch() {
    let interner = TypeInterner::new();

    // Create { a: string }
    let obj1 = interner.object(vec![PropertyInfo::new(
        interner.intern_string("a"),
        TypeId::STRING,
    )]);

    // Create { a: number }
    let obj2 = interner.object(vec![PropertyInfo::new(
        interner.intern_string("a"),
        TypeId::NUMBER,
    )]);

    let checker = SubtypeChecker::new(&interner);

    // Objects with mismatched property types don't overlap
    assert!(!checker.are_types_overlapping(obj1, obj2));
}

#[test]
fn test_objects_with_different_properties_overlap() {
    let interner = TypeInterner::new();

    // Create { a: number }
    let obj1 = interner.object(vec![PropertyInfo::new(
        interner.intern_string("a"),
        TypeId::NUMBER,
    )]);

    // Create { b: number }
    let obj2 = interner.object(vec![PropertyInfo::new(
        interner.intern_string("b"),
        TypeId::NUMBER,
    )]);

    let checker = SubtypeChecker::new(&interner);

    // Objects with different properties DO overlap (can have { a: number, b: number })
    assert!(checker.are_types_overlapping(obj1, obj2));
}

#[test]
fn test_void_and_undefined_overlap() {
    let interner = TypeInterner::new();
    let checker = SubtypeChecker::new(&interner);

    // void and undefined always overlap
    assert!(checker.are_types_overlapping(TypeId::VOID, TypeId::UNDEFINED));
    assert!(checker.are_types_overlapping(TypeId::UNDEFINED, TypeId::VOID));
}

#[test]
fn test_null_undefined_with_strict_null_checks() {
    let interner = TypeInterner::new();

    // With strict null checks ON (default)
    let checker_strict = SubtypeChecker::new(&interner).with_strict_null_checks(true);

    // null/undefined don't overlap with other primitives in strict mode
    assert!(!checker_strict.are_types_overlapping(TypeId::NULL, TypeId::STRING));
    assert!(!checker_strict.are_types_overlapping(TypeId::UNDEFINED, TypeId::NUMBER));

    // But they overlap with themselves
    assert!(checker_strict.are_types_overlapping(TypeId::NULL, TypeId::NULL));
    assert!(checker_strict.are_types_overlapping(TypeId::UNDEFINED, TypeId::UNDEFINED));
}

#[test]
fn test_null_undefined_without_strict_null_checks() {
    let interner = TypeInterner::new();

    // With strict null checks OFF
    let checker_non_strict = SubtypeChecker::new(&interner).with_strict_null_checks(false);

    // null/undefined overlap with everything in non-strict mode
    assert!(checker_non_strict.are_types_overlapping(TypeId::NULL, TypeId::STRING));
    assert!(checker_non_strict.are_types_overlapping(TypeId::UNDEFINED, TypeId::NUMBER));
    assert!(checker_non_strict.are_types_overlapping(TypeId::NULL, TypeId::BOOLEAN));
}

#[test]
fn test_object_keyword_vs_primitives() {
    let interner = TypeInterner::new();
    let checker = SubtypeChecker::new(&interner);

    // object keyword (non-primitive) doesn't overlap with primitives
    assert!(!checker.are_types_overlapping(TypeId::OBJECT, TypeId::STRING));
    assert!(!checker.are_types_overlapping(TypeId::OBJECT, TypeId::NUMBER));
}

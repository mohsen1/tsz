//! Tests for primitive-to-boxed-type assignability with augmented interfaces.
//!
//! When a user augments a built-in interface (e.g., `interface Number extends ICloneable {}`),
//! the boxed type registered from lib resolution may produce a different TypeId than the
//! type produced by `compute_type_of_symbol`. The shape-level property superset check in
//! `is_target_boxed_type` must handle this case, allowing the primitive to be assignable
//! to the augmented version of its boxed type.

use super::*;
use crate::TypeInterner;
use crate::types::IntrinsicKind;

/// Helper to create a `PropertyInfo` with minimal boilerplate.
fn prop(interner: &TypeInterner, name: &str, type_id: TypeId) -> PropertyInfo {
    PropertyInfo {
        name: interner.intern_string(name),
        type_id,
        write_type: type_id,
        optional: false,
        readonly: false,
        is_method: false,
        is_class_prototype: false,
        visibility: Visibility::Public,
        parent_id: None,
        declaration_order: 0,
        is_string_named: false,
    }
}

/// Test: `number` should be assignable to its boxed `Number` interface when the
/// target is the exact same TypeId as the registered boxed type.
#[test]
fn test_number_assignable_to_exact_boxed_number() {
    let interner = TypeInterner::new();

    // Create a Number-like object type with typical Number methods
    let number_obj = interner.object(vec![
        prop(&interner, "toString", TypeId::STRING),
        prop(&interner, "toFixed", TypeId::STRING),
        prop(&interner, "valueOf", TypeId::NUMBER),
    ]);

    // Register as boxed Number type
    interner.register_boxed_type(IntrinsicKind::Number, number_obj);

    let mut checker = SubtypeChecker::new(&interner);
    // number -> Number (same TypeId) should succeed via identity check
    assert!(
        checker.is_subtype_of(TypeId::NUMBER, number_obj),
        "number should be assignable to its exact boxed Number type"
    );
}

/// Test: `number` should be assignable to an augmented `Number` interface that
/// has all the original Number properties PLUS additional heritage members.
/// This is the key regression test for the genericConstraintOnExtendedBuiltinTypes2 fix.
#[test]
fn test_number_assignable_to_augmented_boxed_number() {
    let interner = TypeInterner::new();

    // Create a "base" Number type (as resolved from lib.d.ts)
    let base_number = interner.object(vec![
        prop(&interner, "toString", TypeId::STRING),
        prop(&interner, "toFixed", TypeId::STRING),
        prop(&interner, "valueOf", TypeId::NUMBER),
    ]);

    // Create an "augmented" Number type (with additional Clone() from heritage)
    // This simulates `interface Number extends ICloneable { }` where ICloneable has Clone()
    let augmented_number = interner.object(vec![
        prop(&interner, "toString", TypeId::STRING),
        prop(&interner, "toFixed", TypeId::STRING),
        prop(&interner, "valueOf", TypeId::NUMBER),
        prop(&interner, "Clone", TypeId::ANY), // From ICloneable heritage
    ]);

    // Register the BASE Number as boxed type (this is what resolve_lib_type_by_name returns)
    interner.register_boxed_type(IntrinsicKind::Number, base_number);

    let mut checker = SubtypeChecker::new(&interner);

    // number -> augmented Number should succeed.
    // The augmented Number has all of base Number's properties plus Clone().
    // The is_target_boxed_type shape-level check should detect this as a
    // superset of the boxed type and accept it.
    assert!(
        checker.is_subtype_of(TypeId::NUMBER, augmented_number),
        "number should be assignable to augmented Number (superset of boxed type)"
    );
}

/// Test: `number` should NOT be assignable to `object`.
/// This ensures the fix doesn't create false positives for broader types.
#[test]
fn test_number_not_assignable_to_object_intrinsic() {
    let interner = TypeInterner::new();

    // Create a Number-like type and register it
    let number_obj = interner.object(vec![
        prop(&interner, "toString", TypeId::STRING),
        prop(&interner, "toFixed", TypeId::STRING),
        prop(&interner, "valueOf", TypeId::NUMBER),
    ]);
    interner.register_boxed_type(IntrinsicKind::Number, number_obj);

    let mut checker = SubtypeChecker::new(&interner);

    // number -> object should NOT succeed (object is an intrinsic, not an Object shape)
    assert!(
        !checker.is_subtype_of(TypeId::NUMBER, TypeId::OBJECT),
        "number should NOT be assignable to object"
    );
}

/// Test: `number` should NOT be assignable to a completely different interface
/// that happens to include some of Number's properties.
#[test]
fn test_number_not_assignable_to_unrelated_interface() {
    let interner = TypeInterner::new();

    // Create a Number-like type and register it
    let number_obj = interner.object(vec![
        prop(&interner, "toString", TypeId::STRING),
        prop(&interner, "toFixed", TypeId::STRING),
        prop(&interner, "valueOf", TypeId::NUMBER),
    ]);
    interner.register_boxed_type(IntrinsicKind::Number, number_obj);

    // Create a completely different interface that has different properties
    let different_obj = interner.object(vec![
        prop(&interner, "foo", TypeId::STRING),
        prop(&interner, "bar", TypeId::NUMBER),
    ]);

    let mut checker = SubtypeChecker::new(&interner);

    // number -> different_obj should NOT succeed
    assert!(
        !checker.is_subtype_of(TypeId::NUMBER, different_obj),
        "number should NOT be assignable to an unrelated interface"
    );
}

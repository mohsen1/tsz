//! Tests for intersection types with optional properties in subtype checking.
//! Regression tests for weak type check (TS2559) not applying to individual
//! intersection members. When checking A <: A & `WeakType`, the check against
//! the `WeakType` member alone should succeed even if A has no common properties
//! with `WeakType`, because the overall intersection check passes.

use super::*;
use crate::intern::TypeInterner;
use crate::types::{PropertyInfo, Visibility};

/// Create a type {x?: number} (object with optional property x of type number)
fn make_optional_object(interner: &TypeInterner, name: &str, type_id: TypeId) -> TypeId {
    let name_atom = interner.intern_string(name);
    let props = vec![PropertyInfo {
        name: name_atom,
        type_id,
        write_type: type_id,
        optional: true,
        readonly: false,
        is_method: false,
        is_class_prototype: false,
        visibility: Visibility::Public,
        parent_id: None,
        declaration_order: 0,
        is_string_named: false,
    }];
    interner.object(props)
}

/// Create a type {x?: number, y?: number} (object with two optional properties)
fn make_two_optional_object(interner: &TypeInterner) -> TypeId {
    let x_atom = interner.intern_string("x");
    let y_atom = interner.intern_string("y");
    let mut props = vec![
        PropertyInfo {
            name: x_atom,
            type_id: TypeId::NUMBER,
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
        PropertyInfo {
            name: y_atom,
            type_id: TypeId::NUMBER,
            write_type: TypeId::NUMBER,
            optional: true,
            readonly: false,
            is_method: false,
            is_class_prototype: false,
            visibility: Visibility::Public,
            parent_id: None,
            declaration_order: 1,
            is_string_named: false,
        },
    ];
    props.sort_by_key(|p| p.name);
    interner.object(props)
}

#[test]
fn test_object_subtype_of_intersection_with_optional() {
    // {x?: number} <: {x?: number} & {Id?: number}
    // This should be true because:
    // 1. {x?: number} <: {x?: number} (identity)
    // 2. {x?: number} <: {Id?: number} (Id is optional, missing is ok)
    let interner = TypeInterner::new();

    let a = make_optional_object(&interner, "x", TypeId::NUMBER);
    let id_obj = make_optional_object(&interner, "Id", TypeId::NUMBER);
    let intersection = interner.intersection2(a, id_obj);

    let result = is_subtype_of(&interner, a, intersection);
    assert!(
        result,
        "Object with optional prop should be subtype of itself intersected with another optional-only object"
    );
}

#[test]
fn test_object_subtype_of_broader_object() {
    // {x?: number} <: {Id?: number}
    // Should be true: Id is optional, source doesn't have it, that's OK.
    let interner = TypeInterner::new();

    let a = make_optional_object(&interner, "x", TypeId::NUMBER);
    let b = make_optional_object(&interner, "Id", TypeId::NUMBER);

    let result = is_subtype_of(&interner, a, b);
    assert!(
        result,
        "Object with different optional prop should be subtype of object with only optional props"
    );
}

#[test]
fn test_object_subtype_of_two_optional() {
    // {x?: number} <: {x?: number, y?: number}
    // Should be true: x matches, y is optional so missing is OK.
    let interner = TypeInterner::new();

    let a = make_optional_object(&interner, "x", TypeId::NUMBER);
    let b = make_two_optional_object(&interner);

    let result = is_subtype_of(&interner, a, b);
    assert!(
        result,
        "Object with one optional prop should be subtype of object with two optional props"
    );
}

/// Regression test: weak type check should NOT reject individual intersection members.
/// When checking {Parent?: T} <: {Parent?: T} & {Id?: number}, the `SubtypeChecker`
/// with `enforce_weak_types=true` used to reject {Parent?: T} <: {Id?: number} because
/// they have no common properties and {Id?: number} is a weak type. But the overall
/// check should pass because {Parent?: T} <: {Parent?: T} is trivially true, and
/// weak type checks should not apply to individual intersection members.
#[test]
fn test_weak_type_not_enforced_on_intersection_members() {
    use crate::relations::subtype::core::SubtypeChecker;

    let interner = TypeInterner::new();

    // Create {Parent?: number} (simulating ITreeItem)
    let source = make_optional_object(&interner, "Parent", TypeId::NUMBER);

    // Create {Id?: number} (the weak type member)
    let id_obj = make_optional_object(&interner, "Id", TypeId::NUMBER);

    // Create intersection: {Parent?: number} & {Id?: number}
    let intersection = interner.intersection2(source, id_obj);

    // Check with enforce_weak_types = true (as CompatChecker would set)
    let mut checker = SubtypeChecker::new(&interner);
    checker.enforce_weak_types = true;

    let result = checker.is_subtype_of(source, intersection);
    assert!(
        result,
        "Source should be subtype of intersection even with enforce_weak_types=true, \
         because weak type check should not apply to individual intersection members"
    );
}

/// Regression test: the global `Object` interface (from lib.d.ts) must be
/// assignable to weak types like `{ wings?: boolean; legs?: number }`.
///
/// In tsc, `Object` is exempt from the weak type check because all object types
/// implicitly inherit `Object`'s properties (`toString`, `valueOf`, etc.),
/// so `hasCommonProperties` always finds overlap. Our shapes don't include
/// inherited properties, so we must exempt `Object` explicitly.
///
/// Reproduces the false positive TS2345 in `typeGuardIntersectionTypes.ts`
/// where `hasWings(beast)` with `beast: Object` and `hasWings(x: Beast)` was
/// incorrectly rejected.
#[test]
fn test_global_object_interface_exempt_from_weak_type_check() {
    use crate::relations::subtype::core::SubtypeChecker;

    let interner = TypeInterner::new();

    // Create a shape resembling the global Object interface:
    // { constructor: Function; toString(): string; valueOf(): Object;
    //   hasOwnProperty(v: PropertyKey): boolean; isPrototypeOf(v: Object): boolean;
    //   propertyIsEnumerable(v: PropertyKey): boolean; }
    let make_prop = |name: &str| -> PropertyInfo {
        let name_atom = interner.intern_string(name);
        PropertyInfo {
            name: name_atom,
            type_id: TypeId::ANY,
            write_type: TypeId::ANY,
            optional: false,
            readonly: false,
            is_method: false,
            is_class_prototype: false,
            visibility: Visibility::Public,
            parent_id: None,
            declaration_order: 0,
            is_string_named: false,
        }
    };

    let object_props = vec![
        make_prop("constructor"),
        make_prop("hasOwnProperty"),
        make_prop("isPrototypeOf"),
        make_prop("propertyIsEnumerable"),
        make_prop("toLocaleString"),
        make_prop("toString"),
        make_prop("valueOf"),
    ];
    let object_type = interner.object(object_props);

    // Create a weak type: { wings?: boolean; legs?: number }
    let beast_type = {
        let wings = interner.intern_string("wings");
        let legs = interner.intern_string("legs");
        let props = vec![
            PropertyInfo {
                name: wings,
                type_id: TypeId::BOOLEAN,
                write_type: TypeId::BOOLEAN,
                optional: true,
                readonly: false,
                is_method: false,
                is_class_prototype: false,
                visibility: Visibility::Public,
                parent_id: None,
                declaration_order: 0,
                is_string_named: false,
            },
            PropertyInfo {
                name: legs,
                type_id: TypeId::NUMBER,
                write_type: TypeId::NUMBER,
                optional: true,
                readonly: false,
                is_method: false,
                is_class_prototype: false,
                visibility: Visibility::Public,
                parent_id: None,
                declaration_order: 1,
                is_string_named: false,
            },
        ];
        interner.object(props)
    };

    // With enforce_weak_types, Object should still be assignable to Beast
    let mut checker = SubtypeChecker::new(&interner);
    checker.enforce_weak_types = true;

    let result = checker.is_subtype_of(object_type, beast_type);
    assert!(
        result,
        "Global Object interface should be assignable to weak types (all optional properties). \
         Object is exempt from weak type checks because all types implicitly inherit its members."
    );
}

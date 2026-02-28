//! Tests for intersection types with type parameters
//! Specifically the pattern `T & {}` which TypeScript uses to exclude null/undefined

use super::*;
use crate::intern::TypeInterner;
use crate::types::{MappedType, PropertyInfo, TypeParamInfo};

#[test]
fn test_intersection_with_empty_object_assignable_to_type_param() {
    // T & {} should be assignable to T
    // This is a common TypeScript pattern to exclude null/undefined from T
    let interner = TypeInterner::new();

    // Create type parameter T
    let t_name = interner.intern_string("T");
    let t_param = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: t_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    // Create {} (empty object type)
    let empty_obj = interner.object(vec![]);

    // Create T & {}
    let t_and_empty = interner.intersection(vec![t_param, empty_obj]);

    // Check: T & {} <: T should be true
    let mut checker = SubtypeChecker::new(&interner);
    assert!(
        checker.is_subtype_of(t_and_empty, t_param),
        "T & {{}} should be assignable to T"
    );
}

#[test]
fn test_intersection_with_type_param_and_constraint() {
    // T & string should be assignable to T extends string
    let interner = TypeInterner::new();

    // Create type parameter T extends string
    let t_name = interner.intern_string("T");
    let t_param = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: t_name,
        constraint: Some(TypeId::STRING),
        default: None,
        is_const: false,
    }));

    // Create T & string
    let t_and_string = interner.intersection(vec![t_param, TypeId::STRING]);

    // Check: T & string <: T should be true
    let mut checker = SubtypeChecker::new(&interner);
    assert!(
        checker.is_subtype_of(t_and_string, t_param),
        "T & string should be assignable to T when T extends string"
    );
}

#[test]
fn test_concrete_intersection_with_empty_still_works() {
    // string & {} should still be assignable to string (existing behavior)
    let interner = TypeInterner::new();

    let empty_obj = interner.object(vec![]);
    let string_and_empty = interner.intersection(vec![TypeId::STRING, empty_obj]);

    let mut checker = SubtypeChecker::new(&interner);
    assert!(
        checker.is_subtype_of(string_and_empty, TypeId::STRING),
        "string & {{}} should be assignable to string"
    );
}

#[test]
fn test_intersection_with_mapped_type_member_matches_target() {
    // Readonly<T> & { name: string } should be assignable to Readonly<T>
    // This tests the fix where source intersection member check runs before
    // type-specific target handlers (mapped type expansion) that would
    // otherwise return False without decomposing the source intersection.
    let interner = TypeInterner::new();

    // Create type parameter T
    let t_name = interner.intern_string("T");
    let t_param = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: t_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    // Create type parameter P (for the mapped type iteration variable)
    let p_name = interner.intern_string("P");
    let p_param = TypeParamInfo {
        name: p_name,
        constraint: None,
        default: None,
        is_const: false,
    };

    // Create keyof T
    let keyof_t = interner.keyof(t_param);

    // Create a mapped type like Readonly<T>: { readonly [P in keyof T]: T[P] }
    let p_param_type = interner.intern(TypeData::TypeParameter(p_param.clone()));
    let t_index_p = interner.index_access(t_param, p_param_type);
    let mapped = interner.mapped(MappedType {
        type_param: p_param,
        constraint: keyof_t,
        name_type: None,
        template: t_index_p,
        optional_modifier: None,
        readonly_modifier: None,
    });

    // Create { name: string }
    let name_atom = interner.intern_string("name");
    let name_obj = interner.object(vec![PropertyInfo::new(name_atom, TypeId::STRING)]);

    // Create: MappedType<T> & { name: string }
    let intersection = interner.intersection(vec![mapped, name_obj]);

    // Check: MappedType<T> & { name: string } <: MappedType<T>
    let mut checker = SubtypeChecker::new(&interner);
    assert!(
        checker.is_subtype_of(intersection, mapped),
        "MappedType<T> & {{ name: string }} should be assignable to MappedType<T>"
    );
}

#[test]
fn test_intersection_member_check_with_application_type() {
    // Application<T> & { x: number } should be assignable to Application<T>
    // Tests that the intersection member check works with Application types too.
    let interner = TypeInterner::new();

    // Create a base type (simulating a type alias like `Readonly`)
    let base = interner.lazy(crate::def::DefId(999));

    // Create type parameter T
    let t_param = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: interner.intern_string("T"),
        constraint: None,
        default: None,
        is_const: false,
    }));

    // Create Application<T> (like Readonly<T>)
    let app = interner.application(base, vec![t_param]);

    // Create { x: number }
    let x_atom = interner.intern_string("x");
    let x_obj = interner.object(vec![PropertyInfo::new(x_atom, TypeId::NUMBER)]);

    // Create: Application<T> & { x: number }
    let intersection = interner.intersection(vec![app, x_obj]);

    // Check: Application<T> & { x: number } <: Application<T>
    let mut checker = SubtypeChecker::new(&interner);
    assert!(
        checker.is_subtype_of(intersection, app),
        "Application<T> & {{ x: number }} should be assignable to Application<T>"
    );
}

#[test]
fn test_intersection_member_check_does_not_allow_non_member() {
    // { name: string } & { age: number } should NOT be assignable to { name: string; age: number; active: boolean }
    // The member check should fail (no individual member has all 3 properties),
    // and property merging should also fail (missing 'active').
    let interner = TypeInterner::new();

    let name_atom = interner.intern_string("name");
    let age_atom = interner.intern_string("age");
    let active_atom = interner.intern_string("active");

    let name_obj = interner.object(vec![PropertyInfo::new(name_atom, TypeId::STRING)]);
    let age_obj = interner.object(vec![PropertyInfo::new(age_atom, TypeId::NUMBER)]);
    let target_obj = interner.object(vec![
        PropertyInfo::new(name_atom, TypeId::STRING),
        PropertyInfo::new(age_atom, TypeId::NUMBER),
        PropertyInfo::new(active_atom, TypeId::BOOLEAN),
    ]);

    let intersection = interner.intersection(vec![name_obj, age_obj]);

    let mut checker = SubtypeChecker::new(&interner);
    assert!(
        !checker.is_subtype_of(intersection, target_obj),
        "{{ name: string }} & {{ age: number }} should NOT be assignable to {{ name, age, active }}"
    );
}

//! Tests for intersection types with type parameters
//! Specifically the pattern `T & {}` which TypeScript uses to exclude null/undefined

use super::*;
use crate::intern::TypeInterner;

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

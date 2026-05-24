//! Relation-path coverage for homomorphic mapped types over a source with an
//! empty key space.
//!
//! Structural rule: a homomorphic mapped type `{ [K in keyof T]: T[K] }` whose
//! constraint `keyof T` resolves to an empty key space (a bare function or
//! constructor type, `{}`, or anything whose `keyof` is `never`) has no members
//! and reduces to the empty object type `{}`. `{}` is assignable to it.
//!
//! The eager evaluator already reduces such mapped types to `{}`. These tests
//! pin the *relation-side* expansion (`try_expand_mapped`) so that a raw,
//! un-evaluated `Mapped` reaching the subtype checker also treats a definitively
//! empty key space as `{}` instead of rejecting it (the original false-positive
//! TS2322 in issue #9724).

use super::*;
use crate::computation::SubtypeChecker;
use crate::construction::TypeInterner;
use crate::types::{FunctionShape, MappedType, TypeData, TypeParamInfo};

/// Build a raw homomorphic mapped type `{ [iter in keyof source]: source[iter] }`
/// without going through the eager evaluator, so the subtype checker sees the
/// deferred `Mapped` node.
fn homomorphic_mapped(interner: &TypeInterner, source: TypeId, iter_name: &str) -> TypeId {
    let iter_param = TypeParamInfo {
        name: interner.intern_string(iter_name),
        constraint: None,
        default: None,
        is_const: false,
    };
    let key_param = interner.intern(TypeData::TypeParameter(iter_param));
    let constraint = interner.keyof(source);
    let template = interner.index_access(source, key_param);
    interner.mapped(MappedType {
        type_param: iter_param,
        constraint,
        name_type: None,
        template,
        optional_modifier: None,
        readonly_modifier: None,
    })
}

fn fn_returning_number(interner: &TypeInterner) -> TypeId {
    interner.function(FunctionShape {
        type_params: vec![],
        params: vec![],
        this_type: None,
        return_type: TypeId::NUMBER,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    })
}

fn ctor_returning_object(interner: &TypeInterner) -> TypeId {
    interner.function(FunctionShape {
        type_params: vec![],
        params: vec![],
        this_type: None,
        return_type: TypeId::OBJECT,
        type_predicate: None,
        is_constructor: true,
        is_method: false,
    })
}

#[test]
fn empty_object_assignable_to_mapped_over_function() {
    let interner = TypeInterner::new();
    let mapped = homomorphic_mapped(&interner, fn_returning_number(&interner), "K");
    let empty = interner.object(vec![]);

    let mut checker = SubtypeChecker::new(&interner);
    assert!(
        checker.is_subtype_of(empty, mapped),
        "{{}} must be assignable to a homomorphic mapped type over a function \
         type (empty key space reduces to {{}})"
    );
}

#[test]
fn empty_object_assignable_to_mapped_over_constructor() {
    let interner = TypeInterner::new();
    let mapped = homomorphic_mapped(&interner, ctor_returning_object(&interner), "K");
    let empty = interner.object(vec![]);

    let mut checker = SubtypeChecker::new(&interner);
    assert!(
        checker.is_subtype_of(empty, mapped),
        "{{}} must be assignable to a homomorphic mapped type over a constructor type"
    );
}

#[test]
fn rule_is_not_bound_to_the_iteration_variable_name() {
    // Renaming the iteration variable (`K` -> `Prop`) must not change the result;
    // the rule is structural, keyed on the empty key space, not the spelling.
    let interner = TypeInterner::new();
    let mapped = homomorphic_mapped(&interner, fn_returning_number(&interner), "Prop");
    let empty = interner.object(vec![]);

    let mut checker = SubtypeChecker::new(&interner);
    assert!(
        checker.is_subtype_of(empty, mapped),
        "renamed iteration variable must still accept {{}}"
    );
}

#[test]
fn empty_object_not_assignable_to_mapped_over_nonempty_object() {
    // Negative control: a non-empty source has real keys, so the mapped type has
    // a required member and must reject `{}`.
    let interner = TypeInterner::new();
    let source = interner.object(vec![crate::types::PropertyInfo::new(
        interner.intern_string("a"),
        TypeId::NUMBER,
    )]);
    let mapped = homomorphic_mapped(&interner, source, "K");
    let empty = interner.object(vec![]);

    let mut checker = SubtypeChecker::new(&interner);
    assert!(
        !checker.is_subtype_of(empty, mapped),
        "{{}} must NOT be assignable to a homomorphic mapped type with a required member"
    );
}

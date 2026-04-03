//! Comprehensive tests for property access resolution helpers.
//!
//! Covers property lookup on object types, union types, intersection types,
//! index signature access, optional property handling, missing property detection,
//! readonly properties, primitive property access, array property access, and more.

use crate::intern::TypeInterner;
use crate::operations::expression_ops::normalize_object_union_members_for_write_target;
use crate::operations::property::{PropertyAccessEvaluator, PropertyAccessResult};
use crate::types::*;

// =============================================================================
// Helpers
// =============================================================================

fn assert_property_success(result: &PropertyAccessResult, expected: TypeId) {
    match result {
        PropertyAccessResult::Success { type_id, .. } => assert_eq!(
            *type_id, expected,
            "Expected Success with type {expected:?}, got type {type_id:?}"
        ),
        PropertyAccessResult::PropertyNotFound {
            property_name,
            type_id,
        } => {
            panic!(
                "Expected Success({expected:?}), got PropertyNotFound(type={type_id:?}, prop={property_name:?})"
            )
        }
        PropertyAccessResult::PossiblyNullOrUndefined { cause, .. } => {
            panic!("Expected Success({expected:?}), got PossiblyNullOrUndefined(cause={cause:?})")
        }
        PropertyAccessResult::IsUnknown => {
            panic!("Expected Success({expected:?}), got IsUnknown")
        }
    }
}

fn assert_property_not_found(result: &PropertyAccessResult) {
    assert!(
        matches!(result, PropertyAccessResult::PropertyNotFound { .. }),
        "Expected PropertyNotFound, got {result:?}"
    );
}

fn assert_possibly_null_or_undefined(result: &PropertyAccessResult) {
    assert!(
        matches!(result, PropertyAccessResult::PossiblyNullOrUndefined { .. }),
        "Expected PossiblyNullOrUndefined, got {result:?}"
    );
}

// =============================================================================
// Property lookup on simple object types
// =============================================================================

#[test]
fn test_simple_object_property_access() {
    let interner = TypeInterner::new();
    let evaluator = PropertyAccessEvaluator::new(&interner);

    let x = interner.intern_string("x");
    let y = interner.intern_string("y");
    let obj = interner.object(vec![
        PropertyInfo::new(x, TypeId::NUMBER),
        PropertyInfo::new(y, TypeId::STRING),
    ]);

    assert_property_success(&evaluator.resolve_property_access(obj, "x"), TypeId::NUMBER);
    assert_property_success(&evaluator.resolve_property_access(obj, "y"), TypeId::STRING);
}

#[test]
fn test_simple_object_missing_property() {
    let interner = TypeInterner::new();
    let evaluator = PropertyAccessEvaluator::new(&interner);

    let x = interner.intern_string("x");
    let obj = interner.object(vec![PropertyInfo::new(x, TypeId::NUMBER)]);

    assert_property_not_found(&evaluator.resolve_property_access(obj, "z"));
}

#[test]
fn test_object_multiple_properties() {
    let interner = TypeInterner::new();
    let evaluator = PropertyAccessEvaluator::new(&interner);

    let a = interner.intern_string("a");
    let b = interner.intern_string("b");
    let c = interner.intern_string("c");
    let obj = interner.object(vec![
        PropertyInfo::new(a, TypeId::NUMBER),
        PropertyInfo::new(b, TypeId::STRING),
        PropertyInfo::new(c, TypeId::BOOLEAN),
    ]);

    assert_property_success(&evaluator.resolve_property_access(obj, "a"), TypeId::NUMBER);
    assert_property_success(&evaluator.resolve_property_access(obj, "b"), TypeId::STRING);
    assert_property_success(
        &evaluator.resolve_property_access(obj, "c"),
        TypeId::BOOLEAN,
    );
    assert_property_not_found(&evaluator.resolve_property_access(obj, "d"));
}

#[test]
fn test_validate_slice_case_reducers_keeps_plain_reducer_property_type() {
    let interner = TypeInterner::new();
    let evaluator = PropertyAccessEvaluator::new(&interner);

    let state_name = interner.intern_string("state");
    let reducer_name = interner.intern_string("reducer");
    let case_name = interner.intern_string("onClientUserChanged");
    let empty_object = interner.object(vec![]);

    let reducer_fn = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![ParamInfo {
            name: Some(state_name),
            type_id: TypeId::STRING,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let acr = interner.object(vec![PropertyInfo::new(case_name, reducer_fn)]);

    let key_param_info = TypeParamInfo {
        name: interner.intern_string("T"),
        constraint: None,
        default: None,
        is_const: false,
    };
    let key_param = interner.intern(TypeData::TypeParameter(key_param_info));
    let check_type = interner.index_access(acr, key_param);

    let reducer_shape = interner.object(vec![PropertyInfo::new(
        reducer_name,
        interner.function(FunctionShape {
            type_params: vec![],
            params: vec![ParamInfo {
                name: Some(state_name),
                type_id: TypeId::STRING,
                optional: false,
                rest: false,
            }],
            this_type: None,
            return_type: TypeId::ANY,
            type_predicate: None,
            is_constructor: false,
            is_method: false,
        }),
    )]);

    let template = interner.conditional(ConditionalType {
        check_type,
        extends_type: reducer_shape,
        true_type: interner.object(vec![PropertyInfo::new(reducer_name, TypeId::ANY)]),
        false_type: empty_object,
        is_distributive: false,
    });

    let mapped = interner.mapped(MappedType {
        type_param: key_param_info,
        constraint: interner.keyof(acr),
        name_type: None,
        template,
        optional_modifier: None,
        readonly_modifier: None,
    });

    let validate = interner.intersection(vec![acr, mapped]);

    assert_property_success(
        &evaluator.resolve_property_access(validate, "onClientUserChanged"),
        reducer_fn,
    );
}

// =============================================================================
// Property lookup on union types
// =============================================================================

#[test]
fn test_union_property_exists_on_all_members() {
    let interner = TypeInterner::new();
    let evaluator = PropertyAccessEvaluator::new(&interner);

    let x = interner.intern_string("x");
    let obj1 = interner.object(vec![PropertyInfo::new(x, TypeId::NUMBER)]);
    let obj2 = interner.object(vec![PropertyInfo::new(x, TypeId::STRING)]);
    let union = interner.union(vec![obj1, obj2]);

    // Property "x" exists on both members: result is union of property types
    let result = evaluator.resolve_property_access(union, "x");
    match &result {
        PropertyAccessResult::Success { type_id, .. } => {
            // The result should be number | string
            assert_ne!(*type_id, TypeId::NUMBER);
            assert_ne!(*type_id, TypeId::STRING);
        }
        _ => panic!("Expected Success for union property access, got {result:?}"),
    }
}

#[test]
fn test_union_property_missing_on_one_member() {
    let interner = TypeInterner::new();
    let evaluator = PropertyAccessEvaluator::new(&interner);

    let x = interner.intern_string("x");
    let y = interner.intern_string("y");
    let obj1 = interner.object(vec![PropertyInfo::new(x, TypeId::NUMBER)]);
    let obj2 = interner.object(vec![PropertyInfo::new(y, TypeId::STRING)]);
    let union = interner.union(vec![obj1, obj2]);

    // Property "x" doesn't exist on obj2 -> PropertyNotFound
    assert_property_not_found(&evaluator.resolve_property_access(union, "x"));
}

#[test]
fn test_union_property_missing_on_fresh_object_literal_member_yields_undefined() {
    let interner = TypeInterner::new();
    let evaluator = PropertyAccessEvaluator::new(&interner);

    let x = interner.intern_string("x");
    let y = interner.intern_string("y");
    let obj1 = interner.object_fresh(vec![
        PropertyInfo::new(x, TypeId::NUMBER),
        PropertyInfo::new(y, TypeId::STRING),
    ]);
    let obj2 = interner.object_fresh(vec![PropertyInfo::new(y, TypeId::STRING)]);
    let union = interner.union(vec![obj1, obj2]);

    let result = evaluator.resolve_property_access(union, "x");
    let Some((type_id, _)) = result.success_info() else {
        panic!("expected Success, got {result:?}");
    };
    let expected = interner.union(vec![TypeId::NUMBER, TypeId::UNDEFINED]);
    assert_eq!(type_id, expected);
}

#[test]
fn test_union_property_with_fresh_empty_object_yields_union_with_undefined() {
    // When a union contains a fresh empty object (from `options || {}`),
    // properties that exist on other members should resolve successfully.
    // The fresh empty member contributes `undefined` to the result type.
    // This matches tsc's behavior where `(options || {}).x` is valid.
    let interner = TypeInterner::new();
    let evaluator = PropertyAccessEvaluator::new(&interner);

    let x = interner.intern_string("x");
    let declared = interner.object(vec![PropertyInfo::new(x, TypeId::NUMBER)]);
    let fresh_empty = interner.object_fresh(vec![]);
    let union = interner.union(vec![declared, fresh_empty]);

    let result = evaluator.resolve_property_access(union, "x");
    let Some((type_id, _)) = result.success_info() else {
        panic!("expected Success, got {result:?}");
    };
    let expected = interner.union(vec![TypeId::NUMBER, TypeId::UNDEFINED]);
    assert_eq!(type_id, expected);
}

#[test]
fn test_union_property_with_non_fresh_empty_object_stays_missing() {
    // When a union contains a non-fresh empty object (from type annotations
    // like `T | {}`), the property should NOT be found. This matches tsc's
    // behavior where `declare const x: {a:number}|{}; x.a` errors.
    let interner = TypeInterner::new();
    let evaluator = PropertyAccessEvaluator::new(&interner);

    let x = interner.intern_string("x");
    let declared = interner.object(vec![PropertyInfo::new(x, TypeId::NUMBER)]);
    let non_fresh_empty = interner.object(vec![]);
    let union = interner.union(vec![declared, non_fresh_empty]);

    assert_property_not_found(&evaluator.resolve_property_access(union, "x"));
}

#[test]
fn test_union_property_missing_on_fresh_empty_object_member_yields_undefined() {
    let interner = TypeInterner::new();
    let evaluator = PropertyAccessEvaluator::new(&interner);

    let x = interner.intern_string("x");
    let fresh_with_x = interner.object_fresh(vec![PropertyInfo::new(x, TypeId::NUMBER)]);
    let fresh_empty = interner.object_fresh(vec![]);
    let union = interner.union(vec![fresh_with_x, fresh_empty]);

    let result = evaluator.resolve_property_access(union, "x");
    let Some((type_id, _)) = result.success_info() else {
        panic!("expected Success, got {result:?}");
    };
    let expected = interner.union(vec![TypeId::NUMBER, TypeId::UNDEFINED]);
    assert_eq!(type_id, expected);
}

#[test]
fn test_write_target_normalization_optionalizes_fresh_empty_object_member() {
    let interner = TypeInterner::new();

    let x = interner.intern_string("x");
    let declared = interner.object(vec![PropertyInfo::new(x, TypeId::NUMBER)]);
    let fresh_empty = interner.object_fresh(vec![]);

    let normalized =
        normalize_object_union_members_for_write_target(&interner, &[declared, fresh_empty])
            .expect("expected write-target normalization");

    let union = interner.union(normalized);
    let evaluator = PropertyAccessEvaluator::new(&interner);
    let result = evaluator.resolve_property_access(union, "x");
    let expected = interner.union(vec![TypeId::NUMBER, TypeId::UNDEFINED]);
    match result {
        PropertyAccessResult::Success { type_id, .. } => {
            assert_eq!(type_id, expected);
        }
        _ => panic!("expected Success, got {result:?}"),
    }
}

#[test]
fn test_union_with_any_returns_any() {
    let interner = TypeInterner::new();
    let evaluator = PropertyAccessEvaluator::new(&interner);

    let x = interner.intern_string("x");
    let obj = interner.object(vec![PropertyInfo::new(x, TypeId::NUMBER)]);
    let union = interner.union(vec![obj, TypeId::ANY]);

    // If any member is `any`, result is `any`
    assert_property_success(&evaluator.resolve_property_access(union, "x"), TypeId::ANY);
}

#[test]
fn test_union_with_null_returns_possibly_null() {
    let interner = TypeInterner::new();
    let evaluator = PropertyAccessEvaluator::new(&interner);

    let x = interner.intern_string("x");
    let obj = interner.object(vec![PropertyInfo::new(x, TypeId::NUMBER)]);
    let union = interner.union(vec![obj, TypeId::NULL]);

    // Union with null -> PossiblyNullOrUndefined
    assert_possibly_null_or_undefined(&evaluator.resolve_property_access(union, "x"));
}

#[test]
fn test_union_with_undefined_returns_possibly_undefined() {
    let interner = TypeInterner::new();
    let evaluator = PropertyAccessEvaluator::new(&interner);

    let x = interner.intern_string("x");
    let obj = interner.object(vec![PropertyInfo::new(x, TypeId::NUMBER)]);
    let union = interner.union(vec![obj, TypeId::UNDEFINED]);

    assert_possibly_null_or_undefined(&evaluator.resolve_property_access(union, "x"));
}

#[test]
fn test_union_same_property_same_type() {
    let interner = TypeInterner::new();
    let evaluator = PropertyAccessEvaluator::new(&interner);

    let x = interner.intern_string("x");
    let obj1 = interner.object(vec![PropertyInfo::new(x, TypeId::NUMBER)]);
    let obj2 = interner.object(vec![PropertyInfo::new(x, TypeId::NUMBER)]);
    let union = interner.union(vec![obj1, obj2]);

    // Same type on both members -> result is that type (collapsed by union)
    assert_property_success(
        &evaluator.resolve_property_access(union, "x"),
        TypeId::NUMBER,
    );
}

// =============================================================================
// Property lookup on intersection types
// =============================================================================

#[test]
fn test_intersection_merges_properties() {
    let interner = TypeInterner::new();
    let evaluator = PropertyAccessEvaluator::new(&interner);

    let x = interner.intern_string("x");
    let y = interner.intern_string("y");
    let obj1 = interner.object(vec![PropertyInfo::new(x, TypeId::NUMBER)]);
    let obj2 = interner.object(vec![PropertyInfo::new(y, TypeId::STRING)]);
    let intersection = interner.intersection(vec![obj1, obj2]);

    // Intersection merges: both properties accessible
    let result_x = evaluator.resolve_property_access(intersection, "x");
    assert!(
        result_x.is_success(),
        "Expected 'x' to be found on intersection type"
    );

    let result_y = evaluator.resolve_property_access(intersection, "y");
    assert!(
        result_y.is_success(),
        "Expected 'y' to be found on intersection type"
    );
}

#[test]
fn test_intersection_same_property_intersects_types() {
    let interner = TypeInterner::new();
    let evaluator = PropertyAccessEvaluator::new(&interner);

    let x = interner.intern_string("x");
    // { x: string } & { x: string } -> { x: string }
    let obj1 = interner.object(vec![PropertyInfo::new(x, TypeId::STRING)]);
    let obj2 = interner.object(vec![PropertyInfo::new(x, TypeId::STRING)]);
    let intersection = interner.intersection(vec![obj1, obj2]);

    let result = evaluator.resolve_property_access(intersection, "x");
    // Should succeed - intersection of same property type is that type
    assert!(
        result.is_success(),
        "Expected 'x' to be found on intersection"
    );
}

#[test]
fn test_intersection_property_not_on_any_member() {
    let interner = TypeInterner::new();
    let evaluator = PropertyAccessEvaluator::new(&interner);

    let x = interner.intern_string("x");
    let y = interner.intern_string("y");
    let obj1 = interner.object(vec![PropertyInfo::new(x, TypeId::NUMBER)]);
    let obj2 = interner.object(vec![PropertyInfo::new(y, TypeId::STRING)]);
    let intersection = interner.intersection(vec![obj1, obj2]);

    // Property "z" doesn't exist on either member
    assert_property_not_found(&evaluator.resolve_property_access(intersection, "z"));
}

#[test]
fn test_intersection_ignores_deferred_any_fallback_when_other_member_has_property() {
    let interner = TypeInterner::new();
    let evaluator = PropertyAccessEvaluator::new(&interner);

    let t_info = TypeParamInfo {
        name: interner.intern_string("T"),
        constraint: None,
        default: None,
        is_const: false,
    };
    let t_param = interner.intern(TypeData::TypeParameter(t_info));
    let keyof_t = interner.keyof(t_param);
    let deferred_member = interner.index_access(t_param, keyof_t);

    let and_name = interner.intern_string("and");
    let spy = interner.object(vec![PropertyInfo::new(and_name, TypeId::STRING)]);
    let intersection = interner.intersection(vec![deferred_member, spy]);

    assert_property_success(
        &evaluator.resolve_property_access(intersection, "and"),
        TypeId::STRING,
    );
}

#[test]
fn test_union_with_deferred_member_and_concrete_member_property_not_found() {
    // Union of T[keyof T] (deferred/unresolvable) and { and: string }.
    // Property access on the union returns PropertyNotFound because the deferred
    // member cannot be resolved to determine if it has the property.
    let interner = TypeInterner::new();
    let evaluator = PropertyAccessEvaluator::new(&interner);

    let t_info = TypeParamInfo {
        name: interner.intern_string("T"),
        constraint: None,
        default: None,
        is_const: false,
    };
    let t_param = interner.intern(TypeData::TypeParameter(t_info));
    let keyof_t = interner.keyof(t_param);
    let deferred_member = interner.index_access(t_param, keyof_t);

    let and_name = interner.intern_string("and");
    let spy = interner.object(vec![PropertyInfo::new(and_name, TypeId::STRING)]);
    let union = interner.union(vec![deferred_member, spy]);

    assert_property_not_found(&evaluator.resolve_property_access(union, "and"));
}

#[test]
fn test_unconstrained_type_parameter_has_no_properties() {
    let interner = TypeInterner::new();
    let evaluator = PropertyAccessEvaluator::new(&interner);

    let t_info = TypeParamInfo {
        name: interner.intern_string("T"),
        constraint: None,
        default: None,
        is_const: false,
    };
    let t_param = interner.intern(TypeData::TypeParameter(t_info));

    // tsc 6.0: unconstrained T has NO properties. The implicit constraint is {}
    // which does NOT include Object prototype methods. Accessing toString on
    // bare T emits TS2339 "Property 'toString' does not exist on type 'T'".
    assert_property_not_found(&evaluator.resolve_property_access(t_param, "toString"));
    assert_property_not_found(&evaluator.resolve_property_access(t_param, "nonExistentProp"));
}

#[test]
fn test_constrained_object_like_type_parameter_keeps_object_members() {
    let interner = TypeInterner::new();
    let evaluator = PropertyAccessEvaluator::new(&interner);

    let t_info = TypeParamInfo {
        name: interner.intern_string("T"),
        constraint: Some(TypeId::OBJECT),
        default: None,
        is_const: false,
    };
    let t_param = interner.intern(TypeData::TypeParameter(t_info));

    assert!(
        evaluator
            .resolve_property_access(t_param, "toString")
            .is_success(),
        "constrained object-like type parameter should still expose Object members"
    );
}

// =============================================================================
// Index signature access
// =============================================================================

#[test]
fn test_string_index_signature_access() {
    let interner = TypeInterner::new();
    let evaluator = PropertyAccessEvaluator::new(&interner);

    // { [key: string]: number }
    let obj = interner.object_with_index(ObjectShape {
        flags: ObjectFlags::empty(),
        properties: vec![],
        string_index: Some(IndexSignature {
            key_type: TypeId::STRING,
            value_type: TypeId::NUMBER,
            readonly: false,
            param_name: None,
        }),
        number_index: None,
        symbol: None,
    });

    // Any string property should resolve via index signature
    let result = evaluator.resolve_property_access(obj, "anything");
    match &result {
        PropertyAccessResult::Success {
            type_id,
            from_index_signature,
            ..
        } => {
            assert_eq!(*type_id, TypeId::NUMBER);
            assert!(
                *from_index_signature,
                "Should be marked as from index signature"
            );
        }
        _ => panic!("Expected Success from index signature, got {result:?}"),
    }
}

#[test]
fn test_number_index_signature_with_numeric_key() {
    let interner = TypeInterner::new();
    let evaluator = PropertyAccessEvaluator::new(&interner);

    // { [key: number]: string }
    let obj = interner.object_with_index(ObjectShape {
        flags: ObjectFlags::empty(),
        properties: vec![],
        string_index: None,
        number_index: Some(IndexSignature {
            key_type: TypeId::NUMBER,
            value_type: TypeId::STRING,
            readonly: false,
            param_name: None,
        }),
        symbol: None,
    });

    // Numeric property names resolve via number index signature
    let result = evaluator.resolve_property_access(obj, "0");
    match &result {
        PropertyAccessResult::Success { type_id, .. } => {
            assert_eq!(*type_id, TypeId::STRING);
        }
        _ => panic!("Expected Success for numeric index access, got {result:?}"),
    }
}

#[test]
fn test_explicit_property_takes_precedence_over_index_signature() {
    let interner = TypeInterner::new();
    let evaluator = PropertyAccessEvaluator::new(&interner);

    let x = interner.intern_string("x");
    // { x: boolean, [key: string]: number }
    let obj = interner.object_with_index(ObjectShape {
        flags: ObjectFlags::empty(),
        properties: vec![PropertyInfo::new(x, TypeId::BOOLEAN)],
        string_index: Some(IndexSignature {
            key_type: TypeId::STRING,
            value_type: TypeId::NUMBER,
            readonly: false,
            param_name: None,
        }),
        number_index: None,
        symbol: None,
    });

    // Explicit property "x" should take precedence over index signature
    assert_property_success(
        &evaluator.resolve_property_access(obj, "x"),
        TypeId::BOOLEAN,
    );

    // Other properties fall through to string index signature
    let result = evaluator.resolve_property_access(obj, "y");
    match &result {
        PropertyAccessResult::Success {
            type_id,
            from_index_signature,
            ..
        } => {
            assert_eq!(*type_id, TypeId::NUMBER);
            assert!(*from_index_signature);
        }
        _ => panic!("Expected Success from index signature for 'y', got {result:?}"),
    }
}

#[test]
fn test_index_signature_with_no_unchecked_indexed_access() {
    let interner = TypeInterner::new();
    let mut evaluator = PropertyAccessEvaluator::new(&interner);
    evaluator.set_no_unchecked_indexed_access(true);

    // { [key: string]: number }
    let obj = interner.object_with_index(ObjectShape {
        flags: ObjectFlags::empty(),
        properties: vec![],
        string_index: Some(IndexSignature {
            key_type: TypeId::STRING,
            value_type: TypeId::NUMBER,
            readonly: false,
            param_name: None,
        }),
        number_index: None,
        symbol: None,
    });

    // With noUncheckedIndexedAccess, result includes | undefined
    let result = evaluator.resolve_property_access(obj, "anything");
    match &result {
        PropertyAccessResult::Success { type_id, .. } => {
            // Type should be number | undefined (not just number)
            assert_ne!(
                *type_id,
                TypeId::NUMBER,
                "Should include undefined with noUncheckedIndexedAccess"
            );
        }
        _ => panic!("Expected Success, got {result:?}"),
    }
}

// =============================================================================
// Optional property handling
// =============================================================================

#[test]
fn test_optional_property_includes_undefined() {
    let interner = TypeInterner::new();
    let evaluator = PropertyAccessEvaluator::new(&interner);

    let x = interner.intern_string("x");
    let obj = interner.object(vec![PropertyInfo::opt(x, TypeId::NUMBER)]);

    // Optional property access should include undefined in result type
    let result = evaluator.resolve_property_access(obj, "x");
    match &result {
        PropertyAccessResult::Success { type_id, .. } => {
            // Type should be number | undefined
            assert_ne!(
                *type_id,
                TypeId::NUMBER,
                "Optional property should include undefined"
            );
        }
        _ => panic!("Expected Success for optional property, got {result:?}"),
    }
}

#[test]
fn test_required_property_does_not_include_undefined() {
    let interner = TypeInterner::new();
    let evaluator = PropertyAccessEvaluator::new(&interner);

    let x = interner.intern_string("x");
    let obj = interner.object(vec![PropertyInfo::new(x, TypeId::NUMBER)]);

    // Required property access should be exactly the declared type
    assert_property_success(&evaluator.resolve_property_access(obj, "x"), TypeId::NUMBER);
}

#[test]
fn test_optional_property_with_union_type() {
    let interner = TypeInterner::new();
    let evaluator = PropertyAccessEvaluator::new(&interner);

    let x = interner.intern_string("x");
    let str_num = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);
    let obj = interner.object(vec![PropertyInfo::opt(x, str_num)]);

    // Optional property of type string | number -> string | number | undefined
    let result = evaluator.resolve_property_access(obj, "x");
    match &result {
        PropertyAccessResult::Success { type_id, .. } => {
            // Should not be the original union (needs undefined added)
            assert_ne!(
                *type_id, str_num,
                "Optional property should include undefined"
            );
        }
        _ => panic!("Expected Success, got {result:?}"),
    }
}

// =============================================================================
// Missing property detection
// =============================================================================

#[test]
fn test_missing_property_on_empty_object() {
    let interner = TypeInterner::new();
    let evaluator = PropertyAccessEvaluator::new(&interner);

    let obj = interner.object(vec![]);

    // Some known Object.prototype methods are always available (e.g., hasOwnProperty)
    let result = evaluator.resolve_property_access(obj, "hasOwnProperty");
    // Should succeed because Object.prototype has hasOwnProperty
    assert!(
        result.is_success(),
        "hasOwnProperty should be found on empty object"
    );

    // Non-existent property that's also not on Object.prototype
    // Note: many uncommon names will be PropertyNotFound
    let result = evaluator.resolve_property_access(obj, "xyzNonExistent");
    assert_property_not_found(&result);
}

#[test]
fn test_property_not_found_on_primitive() {
    let interner = TypeInterner::new();
    let evaluator = PropertyAccessEvaluator::new(&interner);

    // Accessing a non-existent property on a primitive should fail
    let result = evaluator.resolve_property_access(TypeId::NUMBER, "nonExistent");
    assert_property_not_found(&result);
}

// =============================================================================
// Readonly property detection
// =============================================================================

#[test]
fn test_readonly_property_can_be_read() {
    let interner = TypeInterner::new();
    let evaluator = PropertyAccessEvaluator::new(&interner);

    let x = interner.intern_string("x");
    let obj = interner.object(vec![PropertyInfo::readonly(x, TypeId::NUMBER)]);

    // Readonly property can still be accessed (read)
    assert_property_success(&evaluator.resolve_property_access(obj, "x"), TypeId::NUMBER);
}

#[test]
fn test_mix_readonly_and_mutable_properties() {
    let interner = TypeInterner::new();
    let evaluator = PropertyAccessEvaluator::new(&interner);

    let x = interner.intern_string("x");
    let y = interner.intern_string("y");
    let obj = interner.object(vec![
        PropertyInfo::readonly(x, TypeId::NUMBER),
        PropertyInfo::new(y, TypeId::STRING),
    ]);

    // Both properties can be read regardless of readonly modifier
    assert_property_success(&evaluator.resolve_property_access(obj, "x"), TypeId::NUMBER);
    assert_property_success(&evaluator.resolve_property_access(obj, "y"), TypeId::STRING);
}

// =============================================================================
// Property access on intrinsic types
// =============================================================================

#[test]
fn test_any_type_property_access() {
    let interner = TypeInterner::new();
    let evaluator = PropertyAccessEvaluator::new(&interner);

    // any.anything => any
    assert_property_success(
        &evaluator.resolve_property_access(TypeId::ANY, "x"),
        TypeId::ANY,
    );
    assert_property_success(
        &evaluator.resolve_property_access(TypeId::ANY, "whatever"),
        TypeId::ANY,
    );
}

#[test]
fn test_unknown_type_property_access() {
    let interner = TypeInterner::new();
    let evaluator = PropertyAccessEvaluator::new(&interner);

    // unknown.anything => IsUnknown error
    let result = evaluator.resolve_property_access(TypeId::UNKNOWN, "x");
    assert!(
        result.is_unknown(),
        "Expected IsUnknown for property access on unknown"
    );
}

#[test]
fn test_never_type_property_access() {
    let interner = TypeInterner::new();
    let evaluator = PropertyAccessEvaluator::new(&interner);

    // never.anything => never (code is unreachable)
    assert_property_success(
        &evaluator.resolve_property_access(TypeId::NEVER, "anything"),
        TypeId::NEVER,
    );
    assert_property_success(
        &evaluator.resolve_property_access(TypeId::NEVER, "x"),
        TypeId::NEVER,
    );
}

#[test]
fn test_null_type_property_access() {
    let interner = TypeInterner::new();
    let evaluator = PropertyAccessEvaluator::new(&interner);

    // null.anything => PossiblyNullOrUndefined
    assert_possibly_null_or_undefined(&evaluator.resolve_property_access(TypeId::NULL, "x"));
}

#[test]
fn test_undefined_type_property_access() {
    let interner = TypeInterner::new();
    let evaluator = PropertyAccessEvaluator::new(&interner);

    // undefined.anything => PossiblyNullOrUndefined
    assert_possibly_null_or_undefined(&evaluator.resolve_property_access(TypeId::UNDEFINED, "x"));
}

#[test]
fn test_void_type_property_access() {
    let interner = TypeInterner::new();
    let evaluator = PropertyAccessEvaluator::new(&interner);

    // void has no properties; solver returns PropertyNotFound
    assert_property_not_found(&evaluator.resolve_property_access(TypeId::VOID, "x"));
}

// =============================================================================
// Property access on string type (apparent members)
// =============================================================================

#[test]
fn test_string_length_property() {
    let interner = TypeInterner::new();
    let evaluator = PropertyAccessEvaluator::new(&interner);

    let result = evaluator.resolve_property_access(TypeId::STRING, "length");
    assert!(result.is_success(), "string.length should be accessible");
    if let PropertyAccessResult::Success { type_id, .. } = result {
        assert_eq!(type_id, TypeId::NUMBER, "string.length should be number");
    }
}

#[test]
fn test_string_method_property() {
    let interner = TypeInterner::new();
    let evaluator = PropertyAccessEvaluator::new(&interner);

    // string.charAt should be accessible (it's a method from apparent members)
    let result = evaluator.resolve_property_access(TypeId::STRING, "charAt");
    assert!(result.is_success(), "string.charAt should be accessible");
}

#[test]
fn test_string_nonexistent_property() {
    let interner = TypeInterner::new();
    let evaluator = PropertyAccessEvaluator::new(&interner);

    let result = evaluator.resolve_property_access(TypeId::STRING, "nonExistentProp");
    assert_property_not_found(&result);
}

// =============================================================================
// Property access on number type (apparent members)
// =============================================================================

#[test]
fn test_number_to_fixed_property() {
    let interner = TypeInterner::new();
    let evaluator = PropertyAccessEvaluator::new(&interner);

    let result = evaluator.resolve_property_access(TypeId::NUMBER, "toFixed");
    assert!(result.is_success(), "number.toFixed should be accessible");
}

#[test]
fn test_number_nonexistent_property() {
    let interner = TypeInterner::new();
    let evaluator = PropertyAccessEvaluator::new(&interner);

    let result = evaluator.resolve_property_access(TypeId::NUMBER, "nonExistentProp");
    assert_property_not_found(&result);
}

// =============================================================================
// Property access on boolean type (apparent members)
// =============================================================================

#[test]
fn test_boolean_value_of_property() {
    let interner = TypeInterner::new();
    let evaluator = PropertyAccessEvaluator::new(&interner);

    let result = evaluator.resolve_property_access(TypeId::BOOLEAN, "valueOf");
    assert!(result.is_success(), "boolean.valueOf should be accessible");
}

// =============================================================================
// Property access on literal types
// =============================================================================

#[test]
fn test_string_literal_length_property() {
    let interner = TypeInterner::new();
    let evaluator = PropertyAccessEvaluator::new(&interner);

    // "hello".length => number (from String interface)
    let lit = interner.literal_string("hello");
    let result = evaluator.resolve_property_access(lit, "length");
    assert!(
        result.is_success(),
        "string literal.length should be accessible"
    );
    if let PropertyAccessResult::Success { type_id, .. } = result {
        assert_eq!(
            type_id,
            TypeId::NUMBER,
            "string literal.length should be number"
        );
    }
}

#[test]
fn test_number_literal_to_fixed_property() {
    let interner = TypeInterner::new();
    let evaluator = PropertyAccessEvaluator::new(&interner);

    let lit = interner.literal_number(42.0);
    let result = evaluator.resolve_property_access(lit, "toFixed");
    assert!(
        result.is_success(),
        "number literal.toFixed should be accessible"
    );
}

// =============================================================================
// Property access on array types
// =============================================================================

#[test]
fn test_array_length_property_without_lib() {
    let interner = TypeInterner::new();
    let evaluator = PropertyAccessEvaluator::new(&interner);

    // Without a registered Array<T> base type (no lib.d.ts), array.length
    // is not available (only tuple fixed-length is resolved directly).
    let arr = interner.array(TypeId::STRING);
    let result = evaluator.resolve_property_access(arr, "length");
    assert_property_not_found(&result);
}

#[test]
fn test_array_numeric_index() {
    let interner = TypeInterner::new();
    let evaluator = PropertyAccessEvaluator::new(&interner);

    let arr = interner.array(TypeId::STRING);
    let result = evaluator.resolve_property_access(arr, "0");
    // Numeric index access on array returns element_type | undefined
    // (array indices always union with undefined for safety)
    match &result {
        PropertyAccessResult::Success {
            type_id,
            from_index_signature,
            ..
        } => {
            // Result should be string | undefined
            assert_ne!(
                *type_id,
                TypeId::STRING,
                "Array index should include undefined"
            );
            assert!(*from_index_signature, "Should be from index signature");
        }
        _ => panic!("Expected Success for numeric index on array, got {result:?}"),
    }
}

#[test]
fn test_array_nonexistent_property() {
    let interner = TypeInterner::new();
    let evaluator = PropertyAccessEvaluator::new(&interner);

    let arr = interner.array(TypeId::NUMBER);
    let result = evaluator.resolve_property_access(arr, "nonExistentArrayProp");
    assert_property_not_found(&result);
}

// =============================================================================
// Property access on tuple types
// =============================================================================

#[test]
fn test_tuple_length_is_literal() {
    let interner = TypeInterner::new();
    let evaluator = PropertyAccessEvaluator::new(&interner);

    // [string, number] has length 2
    let tuple = interner.tuple(vec![
        TupleElement {
            type_id: TypeId::STRING,
            name: None,
            optional: false,
            rest: false,
        },
        TupleElement {
            type_id: TypeId::NUMBER,
            name: None,
            optional: false,
            rest: false,
        },
    ]);
    let result = evaluator.resolve_property_access(tuple, "length");
    match &result {
        PropertyAccessResult::Success { type_id, .. } => {
            // For fixed-length tuples, length should be a literal number (2)
            let expected_len = interner.literal_number(2.0);
            assert_eq!(
                *type_id, expected_len,
                "Tuple [string, number] should have length 2"
            );
        }
        _ => panic!("Expected Success for tuple.length, got {result:?}"),
    }
}

#[test]
fn test_tuple_empty_length() {
    let interner = TypeInterner::new();
    let evaluator = PropertyAccessEvaluator::new(&interner);

    // [] has length 0
    let tuple = interner.tuple(vec![]);
    let result = evaluator.resolve_property_access(tuple, "length");
    match &result {
        PropertyAccessResult::Success { type_id, .. } => {
            let expected_len = interner.literal_number(0.0);
            assert_eq!(*type_id, expected_len, "Empty tuple should have length 0");
        }
        _ => panic!("Expected Success for empty tuple.length, got {result:?}"),
    }
}

#[test]
fn test_tuple_numeric_index() {
    let interner = TypeInterner::new();
    let evaluator = PropertyAccessEvaluator::new(&interner);

    let tuple = interner.tuple(vec![
        TupleElement {
            type_id: TypeId::STRING,
            name: None,
            optional: false,
            rest: false,
        },
        TupleElement {
            type_id: TypeId::NUMBER,
            name: None,
            optional: false,
            rest: false,
        },
    ]);

    // Access index 0 -> string | undefined, index 1 -> number | undefined
    // (array/tuple numeric index access unions with undefined by default)
    let result0 = evaluator.resolve_property_access(tuple, "0");
    match &result0 {
        PropertyAccessResult::Success {
            type_id,
            from_index_signature,
            ..
        } => {
            // Result includes undefined due to element_type_with_undefined
            assert_ne!(
                *type_id,
                TypeId::STRING,
                "tuple[0] should include undefined"
            );
            assert!(*from_index_signature, "Should be from index signature");
        }
        _ => panic!("Expected Success for tuple[0], got {result0:?}"),
    }

    let result1 = evaluator.resolve_property_access(tuple, "1");
    match &result1 {
        PropertyAccessResult::Success {
            type_id,
            from_index_signature,
            ..
        } => {
            assert_ne!(
                *type_id,
                TypeId::NUMBER,
                "tuple[1] should include undefined"
            );
            assert!(*from_index_signature, "Should be from index signature");
        }
        _ => panic!("Expected Success for tuple[1], got {result1:?}"),
    }
}

// =============================================================================
// Property access on function types
// =============================================================================

#[test]
fn test_function_call_property() {
    let interner = TypeInterner::new();
    let evaluator = PropertyAccessEvaluator::new(&interner);

    let func = interner.function(FunctionShape {
        params: vec![],
        this_type: None,
        return_type: TypeId::VOID,
        type_params: Vec::new(),
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    // function.call should be accessible
    let result = evaluator.resolve_property_access(func, "call");
    assert!(result.is_success(), "function.call should be accessible");
}

#[test]
fn test_function_length_property() {
    let interner = TypeInterner::new();
    let evaluator = PropertyAccessEvaluator::new(&interner);

    let func = interner.function(FunctionShape {
        params: vec![ParamInfo::unnamed(TypeId::NUMBER)],
        this_type: None,
        return_type: TypeId::STRING,
        type_params: Vec::new(),
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    // function.length should be accessible
    let result = evaluator.resolve_property_access(func, "length");
    assert!(result.is_success(), "function.length should be accessible");
}

// =============================================================================
// Property access on error type
// =============================================================================

#[test]
fn test_error_type_property_access() {
    let interner = TypeInterner::new();
    let evaluator = PropertyAccessEvaluator::new(&interner);

    // The ERROR type is handled specially - accessing properties on it
    // depends on how it's treated in the type system. Let's check what it does.
    let result = evaluator.resolve_property_access(TypeId::ERROR, "x");
    // ERROR typically acts like any to prevent cascading errors
    // It should either return error or success with error type
    // The actual behavior depends on the implementation
    assert!(
        result.is_success() || result.is_not_found(),
        "ERROR type property access should not crash, got {result:?}"
    );
}

// =============================================================================
// Property access with divergent read/write types
// =============================================================================

#[test]
fn test_property_with_divergent_write_type() {
    let interner = TypeInterner::new();
    let evaluator = PropertyAccessEvaluator::new(&interner);

    let x = interner.intern_string("x");
    // Create property with different read and write types (TS 4.3+ accessor types)
    let prop = PropertyInfo {
        name: x,
        type_id: TypeId::STRING,    // read type
        write_type: TypeId::NUMBER, // write type (different)
        optional: false,
        readonly: false,
        is_method: false,
        is_class_prototype: false,
        visibility: Visibility::Public,
        parent_id: None,
        declaration_order: 0,
        is_string_named: false,
    };
    let obj = interner.object(vec![prop]);

    let result = evaluator.resolve_property_access(obj, "x");
    // The read type should be returned for reads
    assert_property_success(&result, TypeId::STRING);
}

// =============================================================================
// Object with both string and number index signatures
// =============================================================================

#[test]
fn test_object_with_both_index_signatures() {
    let interner = TypeInterner::new();
    let evaluator = PropertyAccessEvaluator::new(&interner);

    // { [key: string]: string, [key: number]: number }
    let obj = interner.object_with_index(ObjectShape {
        flags: ObjectFlags::empty(),
        properties: vec![],
        string_index: Some(IndexSignature {
            key_type: TypeId::STRING,
            value_type: TypeId::STRING,
            readonly: false,
            param_name: None,
        }),
        number_index: Some(IndexSignature {
            key_type: TypeId::NUMBER,
            value_type: TypeId::NUMBER,
            readonly: false,
            param_name: None,
        }),
        symbol: None,
    });

    // String key access uses string index signature
    let result_str = evaluator.resolve_property_access(obj, "abc");
    match &result_str {
        PropertyAccessResult::Success {
            type_id,
            from_index_signature,
            ..
        } => {
            assert_eq!(*type_id, TypeId::STRING);
            assert!(*from_index_signature);
        }
        _ => panic!("Expected Success for string key on dual-index object, got {result_str:?}"),
    }

    // Numeric key access uses number index signature (falls back to string if no number index)
    let result_num = evaluator.resolve_property_access(obj, "0");
    match &result_num {
        PropertyAccessResult::Success {
            type_id,
            from_index_signature,
            ..
        } => {
            // Numeric key should use number index signature
            assert_eq!(*type_id, TypeId::NUMBER);
            assert!(*from_index_signature);
        }
        _ => panic!("Expected Success for numeric key on dual-index object, got {result_num:?}"),
    }
}

// =============================================================================
// Template literal property access
// =============================================================================

#[test]
fn test_template_literal_property_access() {
    let interner = TypeInterner::new();
    let evaluator = PropertyAccessEvaluator::new(&interner);

    // Template literals are string-like; accessing .length should work
    let template = interner.template_literal(vec![
        TemplateSpan::Text(interner.intern_string("hello ")),
        TemplateSpan::Type(TypeId::STRING),
    ]);

    let result = evaluator.resolve_property_access(template, "length");
    assert!(result.is_success(), "Template literal should have .length");
    if let PropertyAccessResult::Success { type_id, .. } = result {
        assert_eq!(
            type_id,
            TypeId::NUMBER,
            "template literal.length should be number"
        );
    }
}

// =============================================================================
// PropertyAccessResult helper methods
// =============================================================================

#[test]
fn test_property_access_result_simple_constructor() {
    let result = PropertyAccessResult::simple(TypeId::STRING);
    assert!(result.is_success());
    assert!(!result.is_not_found());
    assert!(!result.is_possibly_null_or_undefined());
    assert!(!result.is_unknown());
    assert_eq!(result.success_type(), Some(TypeId::STRING));
}

#[test]
fn test_property_access_result_from_index_constructor() {
    let result = PropertyAccessResult::from_index(TypeId::NUMBER);
    assert!(result.is_success());
    match &result {
        PropertyAccessResult::Success {
            type_id,
            from_index_signature,
            ..
        } => {
            assert_eq!(*type_id, TypeId::NUMBER);
            assert!(*from_index_signature);
        }
        _ => unreachable!(),
    }
}

#[test]
fn test_property_access_result_with_write_type_constructor() {
    let result = PropertyAccessResult::with_write_type(TypeId::STRING, TypeId::NUMBER);
    match &result {
        PropertyAccessResult::Success {
            type_id,
            write_type,
            from_index_signature,
        } => {
            assert_eq!(*type_id, TypeId::STRING);
            assert_eq!(*write_type, Some(TypeId::NUMBER));
            assert!(!*from_index_signature);
        }
        _ => unreachable!(),
    }
}

#[test]
fn test_property_access_result_success_info() {
    let result = PropertyAccessResult::from_index(TypeId::NUMBER);
    let info = result.success_info();
    assert_eq!(info, Some((TypeId::NUMBER, true)));

    let result2 = PropertyAccessResult::simple(TypeId::STRING);
    let info2 = result2.success_info();
    assert_eq!(info2, Some((TypeId::STRING, false)));
}

#[test]
fn test_property_access_result_nullable_property_type() {
    let result = PropertyAccessResult::PossiblyNullOrUndefined {
        property_type: Some(TypeId::STRING),
        cause: TypeId::NULL,
    };
    assert_eq!(result.nullable_property_type(), Some(TypeId::STRING));

    let result2 = PropertyAccessResult::PossiblyNullOrUndefined {
        property_type: None,
        cause: TypeId::UNDEFINED,
    };
    assert_eq!(result2.nullable_property_type(), None);
}

// =============================================================================
// Union of all unknown is IsUnknown
// =============================================================================

#[test]
fn test_union_all_unknown_is_unknown() {
    let interner = TypeInterner::new();
    let evaluator = PropertyAccessEvaluator::new(&interner);

    let union = interner.union(vec![TypeId::UNKNOWN, TypeId::UNKNOWN]);
    let result = evaluator.resolve_property_access(union, "x");
    assert!(
        result.is_unknown(),
        "Union of all unknown should be IsUnknown"
    );
}

// =============================================================================
// Symbol property access
// =============================================================================

#[test]
fn test_symbol_to_string_and_value_of() {
    let interner = TypeInterner::new();
    let evaluator = PropertyAccessEvaluator::new(&interner);

    // symbol.toString and symbol.valueOf should be accessible
    let result_ts = evaluator.resolve_property_access(TypeId::SYMBOL, "toString");
    assert!(
        result_ts.is_success(),
        "symbol.toString should be accessible"
    );

    let result_vo = evaluator.resolve_property_access(TypeId::SYMBOL, "valueOf");
    assert!(
        result_vo.is_success(),
        "symbol.valueOf should be accessible"
    );
}

#[test]
fn test_symbol_nonexistent_property() {
    let interner = TypeInterner::new();
    let evaluator = PropertyAccessEvaluator::new(&interner);

    let result = evaluator.resolve_property_access(TypeId::SYMBOL, "nonExistentProp");
    assert_property_not_found(&result);
}

// =============================================================================
// Bigint property access
// =============================================================================

#[test]
fn test_bigint_to_string_property() {
    let interner = TypeInterner::new();
    let evaluator = PropertyAccessEvaluator::new(&interner);

    let result = evaluator.resolve_property_access(TypeId::BIGINT, "toString");
    assert!(result.is_success(), "bigint.toString should be accessible");
}

// =============================================================================
// Union with error type
// =============================================================================

#[test]
fn test_union_with_error_returns_any() {
    let interner = TypeInterner::new();
    let evaluator = PropertyAccessEvaluator::new(&interner);

    let x = interner.intern_string("x");
    let obj = interner.object(vec![PropertyInfo::new(x, TypeId::NUMBER)]);
    let union = interner.union(vec![obj, TypeId::ERROR]);

    // ERROR in a union preserves the ERROR type through property access
    // to prevent cascading diagnostics from the resolved type.
    let result = evaluator.resolve_property_access(union, "x");
    assert_property_success(&result, TypeId::ERROR);
}

// =============================================================================
// Readonly index signature
// =============================================================================

#[test]
fn test_readonly_index_signature_access() {
    let interner = TypeInterner::new();
    let evaluator = PropertyAccessEvaluator::new(&interner);

    // { readonly [key: string]: number }
    let obj = interner.object_with_index(ObjectShape {
        flags: ObjectFlags::empty(),
        properties: vec![],
        string_index: Some(IndexSignature {
            key_type: TypeId::STRING,
            value_type: TypeId::NUMBER,
            readonly: true,
            param_name: None,
        }),
        number_index: None,
        symbol: None,
    });

    // Readonly index signature is still readable
    let result = evaluator.resolve_property_access(obj, "anything");
    match &result {
        PropertyAccessResult::Success { type_id, .. } => {
            assert_eq!(*type_id, TypeId::NUMBER);
        }
        _ => panic!("Expected Success for readonly index signature, got {result:?}"),
    }
}

// =============================================================================
// Object.prototype members
// =============================================================================

#[test]
fn test_object_prototype_members_on_plain_object() {
    let interner = TypeInterner::new();
    let evaluator = PropertyAccessEvaluator::new(&interner);

    let x = interner.intern_string("x");
    let obj = interner.object(vec![PropertyInfo::new(x, TypeId::NUMBER)]);

    // Object.prototype methods should be available
    let result = evaluator.resolve_property_access(obj, "hasOwnProperty");
    assert!(result.is_success(), "hasOwnProperty should be found");

    let result = evaluator.resolve_property_access(obj, "toString");
    assert!(result.is_success(), "toString should be found");
}

// =============================================================================
// Callable type property access
// =============================================================================

#[test]
fn test_callable_with_properties() {
    let interner = TypeInterner::new();
    let evaluator = PropertyAccessEvaluator::new(&interner);

    let version = interner.intern_string("version");
    let callable = interner.callable(CallableShape {
        call_signatures: vec![CallSignature {
            params: vec![],
            return_type: TypeId::VOID,
            type_params: Vec::new(),
            this_type: None,
            type_predicate: None,
            is_method: false,
        }],
        construct_signatures: vec![],
        properties: vec![PropertyInfo::new(version, TypeId::STRING)],
        string_index: None,
        number_index: None,
        symbol: None,
        is_abstract: false,
    });

    // Access property on callable
    assert_property_success(
        &evaluator.resolve_property_access(callable, "version"),
        TypeId::STRING,
    );

    // Function.prototype members should also be accessible
    let result = evaluator.resolve_property_access(callable, "bind");
    assert!(result.is_success(), "callable.bind should be accessible");
}

// =============================================================================
// Union with null and undefined
// =============================================================================

#[test]
fn test_union_with_null_and_undefined() {
    let interner = TypeInterner::new();
    let evaluator = PropertyAccessEvaluator::new(&interner);

    let x = interner.intern_string("x");
    let obj = interner.object(vec![PropertyInfo::new(x, TypeId::NUMBER)]);
    let union = interner.union(vec![obj, TypeId::NULL, TypeId::UNDEFINED]);

    // Should report PossiblyNullOrUndefined
    let result = evaluator.resolve_property_access(union, "x");
    assert_possibly_null_or_undefined(&result);

    // The nullable result should include the property type from the non-null member
    match &result {
        PropertyAccessResult::PossiblyNullOrUndefined { property_type, .. } => {
            assert!(
                property_type.is_some(),
                "Property type from non-null member should be present"
            );
        }
        _ => unreachable!(),
    }
}

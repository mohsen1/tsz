//! Tests for the Lawyer layer (Any propagation rules).

use super::*;
use crate::TypeInterner;
use crate::interner::Atom;
use crate::solver::{FunctionShape, ParamInfo, PropertyInfo, TupleElement};

/// Helper function to create an object type with properties
fn make_test_object(interner: &TypeInterner, props: Vec<(Atom, TypeId)>) -> TypeId {
    let property_infos: Vec<PropertyInfo> = props
        .into_iter()
        .map(|(name, type_id)| PropertyInfo {
            name,
            type_id,
            write_type: type_id,
            optional: false,
            readonly: false,
            is_method: false,
        })
        .collect();

    interner.object(property_infos)
}

#[test]
fn test_any_propagation_rules_default() {
    let _interner = TypeInterner::new();
    let rules = AnyPropagationRules::new();

    // Default: allow suppression is true
    assert!(rules.allow_any_suppression);
}

#[test]
fn test_any_propagation_rules_strict() {
    let rules = AnyPropagationRules::strict();

    // Strict: allow suppression is false
    assert!(!rules.allow_any_suppression);
}

#[test]
fn test_any_to_any_allows_suppression() {
    let interner = TypeInterner::new();
    let rules = AnyPropagationRules::new();

    // any to any - always allows suppression
    assert!(rules.is_any_allowed_to_suppress(TypeId::ANY, TypeId::ANY, &interner));
}

#[test]
fn test_any_to_non_any_allows_suppression_by_default() {
    let interner = TypeInterner::new();
    let rules = AnyPropagationRules::new();

    // any to string - allows suppression by default
    assert!(rules.is_any_allowed_to_suppress(TypeId::ANY, TypeId::STRING, &interner));

    // string to any - allows suppression by default
    assert!(rules.is_any_allowed_to_suppress(TypeId::STRING, TypeId::ANY, &interner));
}

#[test]
fn test_strict_mode_disallows_suppression() {
    let interner = TypeInterner::new();
    let rules = AnyPropagationRules::strict();

    // In strict mode, any does not suppress
    assert!(!rules.is_any_allowed_to_suppress(TypeId::ANY, TypeId::STRING, &interner));
    assert!(!rules.is_any_allowed_to_suppress(TypeId::STRING, TypeId::ANY, &interner));
}

#[test]
fn test_non_any_types_return_none() {
    let interner = TypeInterner::new();
    let rules = AnyPropagationRules::new();

    // Neither type is any - should return None (delegate to structural checker)
    assert!(
        rules
            .check_any_propagation(TypeId::STRING, TypeId::NUMBER, &interner)
            .is_none()
    );
}

#[test]
fn test_any_with_object_properties() {
    let interner = TypeInterner::new();
    let rules = AnyPropagationRules::new();

    // Create an object with properties
    let name = interner.intern_string("name");
    let obj_type = make_test_object(&interner, vec![(name, TypeId::STRING)]);

    // any to object with properties - by default allows suppression
    // (this is the current TS behavior)
    assert!(rules.is_any_allowed_to_suppress(TypeId::ANY, obj_type, &interner));
}

#[test]
fn test_any_with_array() {
    let interner = TypeInterner::new();
    let rules = AnyPropagationRules::new();

    // Create an array type
    let array_type = interner.array(TypeId::STRING);

    // any to array - by default allows suppression
    assert!(rules.is_any_allowed_to_suppress(TypeId::ANY, array_type, &interner));
}

#[test]
fn test_check_any_propagation_with_any() {
    let interner = TypeInterner::new();
    let rules = AnyPropagationRules::new();

    // any to any - returns Some(true)
    assert_eq!(
        rules.check_any_propagation(TypeId::ANY, TypeId::ANY, &interner),
        Some(true)
    );

    // string to any - returns Some(true)
    assert_eq!(
        rules.check_any_propagation(TypeId::STRING, TypeId::ANY, &interner),
        Some(true)
    );

    // any to string - returns Some(true)
    assert_eq!(
        rules.check_any_propagation(TypeId::ANY, TypeId::STRING, &interner),
        Some(true)
    );
}

#[test]
fn test_check_any_propagation_without_any() {
    let interner = TypeInterner::new();
    let rules = AnyPropagationRules::new();

    // string to number - neither is any, returns None
    assert_eq!(
        rules.check_any_propagation(TypeId::STRING, TypeId::NUMBER, &interner),
        None
    );
}

#[test]
fn test_set_allow_any_suppression() {
    let mut rules = AnyPropagationRules::new();

    // Default is true
    assert!(rules.allow_any_suppression);

    // Set to false
    rules.set_allow_any_suppression(false);
    assert!(!rules.allow_any_suppression);

    // Set back to true
    rules.set_allow_any_suppression(true);
    assert!(rules.allow_any_suppression);
}

#[test]
fn test_default_trait() {
    let rules = AnyPropagationRules::default();

    // Default should match new()
    assert!(rules.allow_any_suppression);
}

#[test]
fn test_any_with_function() {
    let interner = TypeInterner::new();
    let rules = AnyPropagationRules::new();

    // Create a function type
    let function_shape = FunctionShape {
        type_params: Vec::new(),
        params: vec![ParamInfo {
            name: None,
            type_id: TypeId::STRING,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: TypeId::NUMBER,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    };
    let function_type = interner.function(function_shape);

    // any to function - has structure, check behavior
    let result = rules.is_any_allowed_to_suppress(TypeId::ANY, function_type, &interner);
    // By default, allows suppression (legacy TS behavior)
    assert!(result);
}

#[test]
fn test_any_with_tuple() {
    let interner = TypeInterner::new();
    let rules = AnyPropagationRules::new();

    // Create a tuple type
    let tuple_elements = vec![
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
    ];
    let tuple_type = interner.tuple(tuple_elements);

    // any to tuple - has structure, check behavior
    let result = rules.is_any_allowed_to_suppress(TypeId::ANY, tuple_type, &interner);
    // By default, allows suppression (legacy TS behavior)
    assert!(result);
}

#[test]
fn test_has_structural_mismatch_with_primitives() {
    let interner = TypeInterner::new();
    let rules = AnyPropagationRules::new();

    // Primitives don't have "structure" to mismatch
    // any to primitive - allows suppression
    assert!(rules.is_any_allowed_to_suppress(TypeId::ANY, TypeId::STRING, &interner));
    assert!(rules.is_any_allowed_to_suppress(TypeId::ANY, TypeId::NUMBER, &interner));
    assert!(rules.is_any_allowed_to_suppress(TypeId::ANY, TypeId::BOOLEAN, &interner));
}

#[test]
fn test_strict_mode_with_objects() {
    let interner = TypeInterner::new();
    let rules = AnyPropagationRules::strict();

    // Create an object with properties
    let name = interner.intern_string("name");
    let obj_type = make_test_object(&interner, vec![(name, TypeId::STRING)]);

    // In strict mode, even with objects, any should not suppress
    // This means we should delegate to structural checker
    assert!(!rules.is_any_allowed_to_suppress(TypeId::ANY, obj_type, &interner));
    assert_eq!(
        rules.check_any_propagation(TypeId::ANY, obj_type, &interner),
        None
    );
}

// =============================================================================
// TypeScriptQuirks Tests
// =============================================================================

#[test]
fn test_typescript_quirks_list() {
    let quirks = TypeScriptQuirks::QUIRKS;
    assert!(
        quirks.len() >= 9,
        "Should have at least 9 documented quirks"
    );
    let quirk_names: Vec<&str> = quirks.iter().map(|(name, _)| *name).collect();
    assert!(quirk_names.contains(&"any-propagation"));
    assert!(quirk_names.contains(&"function-bivariance"));
    assert!(quirk_names.contains(&"method-bivariance"));
    assert!(quirk_names.contains(&"void-return"));
    assert!(quirk_names.contains(&"weak-types"));
    assert!(quirk_names.contains(&"freshness"));
}

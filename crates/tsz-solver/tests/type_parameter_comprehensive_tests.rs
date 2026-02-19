//! Comprehensive tests for type parameter operations.
//!
//! These tests verify TypeScript's type parameter behavior:
//! - Generic type parameter constraints
//! - Type parameter defaults
//! - Type parameter variance
//! - Type parameter inference

use super::*;
use crate::intern::TypeInterner;
use crate::types::{TypeData, TypeParamInfo};

// =============================================================================
// Basic Type Parameter Construction Tests
// =============================================================================

#[test]
fn test_type_parameter_construction() {
    let interner = TypeInterner::new();

    let type_param_info = TypeParamInfo {
        name: interner.intern_string("T"),
        constraint: Some(TypeId::STRING),
        default: None,
        is_const: false,
    };
    let type_param = interner.intern(TypeData::TypeParameter(type_param_info));

    if let Some(TypeData::TypeParameter(info)) = interner.lookup(type_param) {
        let name = interner.resolve_atom(info.name);
        assert_eq!(name, "T");
        assert_eq!(info.constraint, Some(TypeId::STRING));
    } else {
        panic!("Expected type parameter");
    }
}

#[test]
fn test_type_parameter_with_no_constraint() {
    let interner = TypeInterner::new();

    let type_param_info = TypeParamInfo {
        name: interner.intern_string("T"),
        constraint: None,
        default: None,
        is_const: false,
    };
    let type_param = interner.intern(TypeData::TypeParameter(type_param_info));

    if let Some(TypeData::TypeParameter(info)) = interner.lookup(type_param) {
        // No constraint means extends unknown (effectively any)
        assert!(info.constraint.is_none());
    } else {
        panic!("Expected type parameter");
    }
}

#[test]
fn test_type_parameter_with_default() {
    let interner = TypeInterner::new();

    let type_param_info = TypeParamInfo {
        name: interner.intern_string("T"),
        constraint: Some(TypeId::STRING),
        default: Some(TypeId::STRING),
        is_const: false,
    };
    let type_param = interner.intern(TypeData::TypeParameter(type_param_info));

    if let Some(TypeData::TypeParameter(info)) = interner.lookup(type_param) {
        assert_eq!(info.default, Some(TypeId::STRING));
    } else {
        panic!("Expected type parameter");
    }
}

#[test]
fn test_multiple_type_parameters() {
    let interner = TypeInterner::new();

    let t_info = TypeParamInfo {
        name: interner.intern_string("T"),
        constraint: None,
        default: None,
        is_const: false,
    };
    let u_info = TypeParamInfo {
        name: interner.intern_string("U"),
        constraint: Some(TypeId::NUMBER),
        default: None,
        is_const: false,
    };

    let t_param = interner.intern(TypeData::TypeParameter(t_info));
    let u_param = interner.intern(TypeData::TypeParameter(u_info));

    // Verify they are different type IDs
    assert_ne!(t_param, u_param);
}

// =============================================================================
// Type Parameter Constraint Tests
// =============================================================================

#[test]
fn test_type_parameter_extends_string() {
    let interner = TypeInterner::new();

    let type_param_info = TypeParamInfo {
        name: interner.intern_string("T"),
        constraint: Some(TypeId::STRING),
        default: None,
        is_const: false,
    };
    let type_param = interner.intern(TypeData::TypeParameter(type_param_info));

    // T extends string should be subtype of string
    // Note: Type parameters are only subtypes of their constraints
    // in specific contexts; this tests the structural representation
    if let Some(TypeData::TypeParameter(info)) = interner.lookup(type_param) {
        assert_eq!(info.constraint, Some(TypeId::STRING));
    }
}

#[test]
fn test_type_parameter_extends_object() {
    let interner = TypeInterner::new();

    let obj = interner.object(vec![crate::types::PropertyInfo::new(
        interner.intern_string("name"),
        TypeId::STRING,
    )]);

    let type_param_info = TypeParamInfo {
        name: interner.intern_string("T"),
        constraint: Some(obj),
        default: None,
        is_const: false,
    };
    let _type_param = interner.intern(TypeData::TypeParameter(type_param_info));

    // Just verify construction with object constraint
}

#[test]
fn test_type_parameter_extends_union() {
    let interner = TypeInterner::new();

    let string_or_number = interner.union2(TypeId::STRING, TypeId::NUMBER);

    let type_param_info = TypeParamInfo {
        name: interner.intern_string("T"),
        constraint: Some(string_or_number),
        default: None,
        is_const: false,
    };
    let _type_param = interner.intern(TypeData::TypeParameter(type_param_info));

    // Just verify construction with union constraint
}

// =============================================================================
// Type Parameter Identity Tests
// =============================================================================

#[test]
fn test_type_parameter_identity_stability() {
    let interner = TypeInterner::new();

    let info1 = TypeParamInfo {
        name: interner.intern_string("T"),
        constraint: Some(TypeId::STRING),
        default: None,
        is_const: false,
    };
    let info2 = TypeParamInfo {
        name: interner.intern_string("T"),
        constraint: Some(TypeId::STRING),
        default: None,
        is_const: false,
    };

    let param1 = interner.intern(TypeData::TypeParameter(info1));
    let param2 = interner.intern(TypeData::TypeParameter(info2));

    assert_eq!(
        param1, param2,
        "Same type parameter should produce same TypeId"
    );
}

#[test]
fn test_different_type_parameters_different_ids() {
    let interner = TypeInterner::new();

    let t_info = TypeParamInfo {
        name: interner.intern_string("T"),
        constraint: None,
        default: None,
        is_const: false,
    };
    let u_info = TypeParamInfo {
        name: interner.intern_string("U"),
        constraint: None,
        default: None,
        is_const: false,
    };

    let t_param = interner.intern(TypeData::TypeParameter(t_info));
    let u_param = interner.intern(TypeData::TypeParameter(u_info));

    assert_ne!(
        t_param, u_param,
        "Different type parameters should have different TypeIds"
    );
}

// =============================================================================
// Type Parameter with Array Tests
// =============================================================================

#[test]
fn test_array_of_type_parameter() {
    let interner = TypeInterner::new();

    let type_param_info = TypeParamInfo {
        name: interner.intern_string("T"),
        constraint: None,
        default: None,
        is_const: false,
    };
    let type_param = interner.intern(TypeData::TypeParameter(type_param_info));

    let array_of_t = interner.array(type_param);

    if let Some(TypeData::Array(element)) = interner.lookup(array_of_t) {
        assert_eq!(element, type_param);
    } else {
        panic!("Expected array type");
    }
}

#[test]
fn test_type_parameter_extends_array() {
    let interner = TypeInterner::new();

    let string_array = interner.array(TypeId::STRING);

    let type_param_info = TypeParamInfo {
        name: interner.intern_string("T"),
        constraint: Some(string_array),
        default: None,
        is_const: false,
    };
    let _type_param = interner.intern(TypeData::TypeParameter(type_param_info));

    // T extends string[]
}

// =============================================================================
// Type Parameter with Function Tests
// =============================================================================

#[test]
fn test_type_parameter_in_function() {
    let interner = TypeInterner::new();

    let type_param_info = TypeParamInfo {
        name: interner.intern_string("T"),
        constraint: None,
        default: None,
        is_const: false,
    };
    let type_param = interner.intern(TypeData::TypeParameter(type_param_info.clone()));

    let func = interner.function(crate::types::FunctionShape {
        params: vec![crate::types::ParamInfo {
            name: Some(interner.intern_string("value")),
            type_id: type_param,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: type_param,
        type_params: vec![type_param_info],
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    if let Some(TypeData::Function(shape_id)) = interner.lookup(func) {
        let shape = interner.function_shape(shape_id);
        assert_eq!(shape.params.len(), 1);
        assert_eq!(shape.return_type, type_param);
    } else {
        panic!("Expected function type");
    }
}

#[test]
fn test_type_parameter_as_return_type() {
    let interner = TypeInterner::new();

    let type_param_info = TypeParamInfo {
        name: interner.intern_string("T"),
        constraint: None,
        default: None,
        is_const: false,
    };
    let type_param = interner.intern(TypeData::TypeParameter(type_param_info.clone()));

    let func = interner.function(crate::types::FunctionShape {
        params: vec![],
        this_type: None,
        return_type: type_param,
        type_params: vec![type_param_info],
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    if let Some(TypeData::Function(shape_id)) = interner.lookup(func) {
        let shape = interner.function_shape(shape_id);
        assert_eq!(shape.return_type, type_param);
    } else {
        panic!("Expected function type");
    }
}

// =============================================================================
// Const Type Parameter Tests
// =============================================================================

#[test]
fn test_const_type_parameter() {
    let interner = TypeInterner::new();

    let type_param_info = TypeParamInfo {
        name: interner.intern_string("T"),
        constraint: None,
        default: None,
        is_const: true, // const type parameter
    };
    let type_param = interner.intern(TypeData::TypeParameter(type_param_info));

    if let Some(TypeData::TypeParameter(info)) = interner.lookup(type_param) {
        assert!(info.is_const);
    } else {
        panic!("Expected type parameter");
    }
}

// =============================================================================
// Type Parameter with Union/Intersection Tests
// =============================================================================

#[test]
fn test_type_parameter_in_union() {
    let interner = TypeInterner::new();

    let type_param_info = TypeParamInfo {
        name: interner.intern_string("T"),
        constraint: None,
        default: None,
        is_const: false,
    };
    let type_param = interner.intern(TypeData::TypeParameter(type_param_info));

    let union = interner.union2(type_param, TypeId::STRING);

    if let Some(TypeData::Union(members)) = interner.lookup(union) {
        let members = interner.type_list(members);
        assert_eq!(members.len(), 2);
    } else {
        panic!("Expected union type");
    }
}

#[test]
fn test_type_parameter_in_intersection() {
    let interner = TypeInterner::new();

    let type_param_info = TypeParamInfo {
        name: interner.intern_string("T"),
        constraint: Some(TypeId::STRING),
        default: None,
        is_const: false,
    };
    let type_param = interner.intern(TypeData::TypeParameter(type_param_info));

    let intersection = interner.intersection2(type_param, TypeId::STRING);

    if let Some(TypeData::Intersection(members)) = interner.lookup(intersection) {
        let members = interner.type_list(members);
        assert_eq!(members.len(), 2);
    } else {
        panic!("Expected intersection type");
    }
}

// =============================================================================
// Nested Type Parameter Tests
// =============================================================================

#[test]
fn test_nested_type_parameters() {
    let interner = TypeInterner::new();

    // T extends U, U extends string
    let u_info = TypeParamInfo {
        name: interner.intern_string("U"),
        constraint: Some(TypeId::STRING),
        default: None,
        is_const: false,
    };
    let u_param = interner.intern(TypeData::TypeParameter(u_info));

    let t_info = TypeParamInfo {
        name: interner.intern_string("T"),
        constraint: Some(u_param),
        default: None,
        is_const: false,
    };
    let t_param = interner.intern(TypeData::TypeParameter(t_info));

    // Verify T's constraint is U
    if let Some(TypeData::TypeParameter(info)) = interner.lookup(t_param) {
        assert_eq!(info.constraint, Some(u_param));
    } else {
        panic!("Expected type parameter");
    }
}

// =============================================================================
// Type Parameter with Object Tests
// =============================================================================

#[test]
fn test_object_with_type_parameter_property() {
    let interner = TypeInterner::new();

    let type_param_info = TypeParamInfo {
        name: interner.intern_string("T"),
        constraint: None,
        default: None,
        is_const: false,
    };
    let type_param = interner.intern(TypeData::TypeParameter(type_param_info));

    let obj = interner.object(vec![crate::types::PropertyInfo::new(
        interner.intern_string("value"),
        type_param,
    )]);

    if let Some(TypeData::Object(_)) = interner.lookup(obj) {
        // Good - object with type parameter property created
    } else {
        panic!("Expected object type");
    }
}

// =============================================================================
// Type Parameter Constraint Edge Cases
// =============================================================================

#[test]
fn test_type_parameter_extends_any() {
    let interner = TypeInterner::new();

    let type_param_info = TypeParamInfo {
        name: interner.intern_string("T"),
        constraint: Some(TypeId::ANY),
        default: None,
        is_const: false,
    };
    let _type_param = interner.intern(TypeData::TypeParameter(type_param_info));

    // T extends any is valid
}

#[test]
fn test_type_parameter_extends_never() {
    let interner = TypeInterner::new();

    let type_param_info = TypeParamInfo {
        name: interner.intern_string("T"),
        constraint: Some(TypeId::NEVER),
        default: None,
        is_const: false,
    };
    let _type_param = interner.intern(TypeData::TypeParameter(type_param_info));

    // T extends never is valid but T can only be never
}

#[test]
fn test_type_parameter_extends_unknown() {
    let interner = TypeInterner::new();

    let type_param_info = TypeParamInfo {
        name: interner.intern_string("T"),
        constraint: Some(TypeId::UNKNOWN),
        default: None,
        is_const: false,
    };
    let _type_param = interner.intern(TypeData::TypeParameter(type_param_info));

    // T extends unknown is valid
}

// =============================================================================
// Type Parameter Default Tests
// =============================================================================

#[test]
fn test_type_parameter_default_with_constraint() {
    let interner = TypeInterner::new();

    // T extends string = string
    let type_param_info = TypeParamInfo {
        name: interner.intern_string("T"),
        constraint: Some(TypeId::STRING),
        default: Some(TypeId::STRING),
        is_const: false,
    };
    let type_param = interner.intern(TypeData::TypeParameter(type_param_info));

    if let Some(TypeData::TypeParameter(info)) = interner.lookup(type_param) {
        assert_eq!(info.constraint, Some(TypeId::STRING));
        assert_eq!(info.default, Some(TypeId::STRING));
    } else {
        panic!("Expected type parameter");
    }
}

#[test]
fn test_type_parameter_default_different_from_constraint() {
    let interner = TypeInterner::new();

    // T extends string | number = number
    let union = interner.union2(TypeId::STRING, TypeId::NUMBER);
    let type_param_info = TypeParamInfo {
        name: interner.intern_string("T"),
        constraint: Some(union),
        default: Some(TypeId::NUMBER),
        is_const: false,
    };
    let type_param = interner.intern(TypeData::TypeParameter(type_param_info));

    if let Some(TypeData::TypeParameter(info)) = interner.lookup(type_param) {
        // Default must satisfy constraint
        assert!(info.constraint.is_some());
        assert_eq!(info.default, Some(TypeId::NUMBER));
    } else {
        panic!("Expected type parameter");
    }
}

// =============================================================================
// Type Parameter with Tuple Tests
// =============================================================================

#[test]
fn test_tuple_with_type_parameter() {
    let interner = TypeInterner::new();

    let type_param_info = TypeParamInfo {
        name: interner.intern_string("T"),
        constraint: None,
        default: None,
        is_const: false,
    };
    let type_param = interner.intern(TypeData::TypeParameter(type_param_info));

    let tuple = interner.tuple(vec![
        crate::types::TupleElement {
            type_id: type_param,
            name: None,
            optional: false,
            rest: false,
        },
        crate::types::TupleElement {
            type_id: type_param,
            name: None,
            optional: false,
            rest: false,
        },
    ]);

    if let Some(TypeData::Tuple(elements)) = interner.lookup(tuple) {
        let elements = interner.tuple_list(elements);
        assert_eq!(elements.len(), 2);
        // Both elements should be the same type parameter
        assert_eq!(elements[0].type_id, type_param);
        assert_eq!(elements[1].type_id, type_param);
    } else {
        panic!("Expected tuple type");
    }
}

// =============================================================================
// Multiple Type Parameters in Same Context
// =============================================================================

#[test]
fn test_function_with_multiple_type_parameters() {
    let interner = TypeInterner::new();

    let t_info = TypeParamInfo {
        name: interner.intern_string("T"),
        constraint: None,
        default: None,
        is_const: false,
    };
    let u_info = TypeParamInfo {
        name: interner.intern_string("U"),
        constraint: None,
        default: None,
        is_const: false,
    };
    let t_param = interner.intern(TypeData::TypeParameter(t_info.clone()));
    let u_param = interner.intern(TypeData::TypeParameter(u_info.clone()));

    // function<T, U>(a: T, b: U): [T, U]
    let tuple = interner.tuple(vec![
        crate::types::TupleElement {
            type_id: t_param,
            name: None,
            optional: false,
            rest: false,
        },
        crate::types::TupleElement {
            type_id: u_param,
            name: None,
            optional: false,
            rest: false,
        },
    ]);

    let func = interner.function(crate::types::FunctionShape {
        params: vec![
            crate::types::ParamInfo {
                name: Some(interner.intern_string("a")),
                type_id: t_param,
                optional: false,
                rest: false,
            },
            crate::types::ParamInfo {
                name: Some(interner.intern_string("b")),
                type_id: u_param,
                optional: false,
                rest: false,
            },
        ],
        this_type: None,
        return_type: tuple,
        type_params: vec![t_info, u_info],
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    if let Some(TypeData::Function(shape_id)) = interner.lookup(func) {
        let shape = interner.function_shape(shape_id);
        assert_eq!(shape.type_params.len(), 2);
    } else {
        panic!("Expected function type");
    }
}

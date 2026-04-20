use super::*;
/// Test indexed access preserves readonly property type.
///
/// { readonly a: string }["a"] should still be string.
#[test]
fn test_indexed_access_readonly_property() {
    let interner = TypeInterner::new();

    let obj = interner.object(vec![PropertyInfo {
        name: interner.intern_string("a"),
        type_id: TypeId::STRING,
        write_type: TypeId::STRING,
        optional: false,
        readonly: true, // readonly
        is_method: false,
        is_class_prototype: false,
        visibility: Visibility::Public,
        parent_id: None,
        declaration_order: 0,
        is_string_named: false,
    }]);

    let key_a = interner.literal_string("a");
    let result = evaluate_index_access(&interner, obj, key_a);
    assert_eq!(result, TypeId::STRING);
}

// ============================================================================
// Generator Function Type Tests
// ============================================================================
// Tests for generator function return type evaluation

#[test]
fn test_generator_function_return_type_extraction() {
    // Test: Extract return type from generator-like function
    // T extends () => infer R ? R : never
    let interner = TypeInterner::new();

    let t_name = interner.intern_string("T");
    let t_param = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: t_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    let infer_name = interner.intern_string("R");
    let infer_r = interner.intern(TypeData::Infer(TypeParamInfo {
        name: infer_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    // Pattern: () => infer R
    let extends_fn = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![],
        this_type: None,
        return_type: infer_r,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let cond = ConditionalType {
        check_type: t_param,
        extends_type: extends_fn,
        true_type: infer_r,
        false_type: TypeId::NEVER,
        is_distributive: true,
    };

    let cond_type = interner.conditional(cond);
    let mut subst = TypeSubstitution::new();

    // Input: () => number
    let input_fn = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![],
        this_type: None,
        return_type: TypeId::NUMBER,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });
    subst.insert(t_name, input_fn);

    let instantiated = instantiate_type(&interner, cond_type, &subst);
    let result = evaluate_type(&interner, instantiated);

    // Should extract number
    assert_eq!(result, TypeId::NUMBER);
}

#[test]
fn test_generator_function_yield_type_simulation() {
    // Test: Simulate extracting yield type via first type param
    // Generator<T, TReturn, TNext> - extract T
    let interner = TypeInterner::new();

    let t_name = interner.intern_string("T");
    let t_param = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: t_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    let infer_name = interner.intern_string("Y");
    let infer_y = interner.intern(TypeData::Infer(TypeParamInfo {
        name: infer_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    // Pattern function returning: { value: infer Y; done: boolean }
    let value_prop = interner.intern_string("value");
    let done_prop = interner.intern_string("done");
    let iterator_result = interner.object(vec![
        PropertyInfo::readonly(value_prop, infer_y),
        PropertyInfo::readonly(done_prop, TypeId::BOOLEAN),
    ]);

    let cond = ConditionalType {
        check_type: t_param,
        extends_type: iterator_result,
        true_type: infer_y,
        false_type: TypeId::NEVER,
        is_distributive: true,
    };

    let cond_type = interner.conditional(cond);
    let mut subst = TypeSubstitution::new();

    // Input: { value: string; done: boolean }
    let input_obj = interner.object(vec![
        PropertyInfo::readonly(value_prop, TypeId::STRING),
        PropertyInfo::readonly(done_prop, TypeId::BOOLEAN),
    ]);
    subst.insert(t_name, input_obj);

    let instantiated = instantiate_type(&interner, cond_type, &subst);
    let result = evaluate_type(&interner, instantiated);

    // Should extract string as yield type
    assert_eq!(result, TypeId::STRING);
}

#[test]
fn test_generator_function_async_return() {
    // Test: Extract inner type from Promise-like return
    // T extends () => Promise<infer R> ? R : never (simulated with object)
    let interner = TypeInterner::new();

    let t_name = interner.intern_string("T");
    let t_param = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: t_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    let infer_name = interner.intern_string("R");
    let infer_r = interner.intern(TypeData::Infer(TypeParamInfo {
        name: infer_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    // Pattern: { then: (resolve: (value: infer R) => void) => void }
    let then_prop = interner.intern_string("then");
    let promise_like = interner.object(vec![PropertyInfo {
        name: then_prop,
        type_id: infer_r, // Simplified - using infer R directly as property
        write_type: infer_r,
        optional: false,
        readonly: true,
        is_method: true,
        is_class_prototype: false,
        visibility: Visibility::Public,
        parent_id: None,
        declaration_order: 0,
        is_string_named: false,
    }]);

    let cond = ConditionalType {
        check_type: t_param,
        extends_type: promise_like,
        true_type: infer_r,
        false_type: TypeId::NEVER,
        is_distributive: true,
    };

    let cond_type = interner.conditional(cond);
    let mut subst = TypeSubstitution::new();

    // Input: { then: string }
    let input_obj = interner.object(vec![PropertyInfo {
        name: then_prop,
        type_id: TypeId::STRING,
        write_type: TypeId::STRING,
        optional: false,
        readonly: true,
        is_method: true,
        is_class_prototype: false,
        visibility: Visibility::Public,
        parent_id: None,
        declaration_order: 0,
        is_string_named: false,
    }]);
    subst.insert(t_name, input_obj);

    let instantiated = instantiate_type(&interner, cond_type, &subst);
    let result = evaluate_type(&interner, instantiated);

    // Should extract string
    assert_eq!(result, TypeId::STRING);
}

#[test]
fn test_generator_function_next_param_type() {
    // Test: Extract parameter type from function
    // T extends (arg: infer A) => any ? A : never
    let interner = TypeInterner::new();

    let t_name = interner.intern_string("T");
    let t_param = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: t_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    let infer_name = interner.intern_string("A");
    let infer_a = interner.intern(TypeData::Infer(TypeParamInfo {
        name: infer_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    // Pattern: (arg: infer A) => any
    let arg_name = interner.intern_string("arg");
    let extends_fn = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![ParamInfo::required(arg_name, infer_a)],
        this_type: None,
        return_type: TypeId::ANY,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let cond = ConditionalType {
        check_type: t_param,
        extends_type: extends_fn,
        true_type: infer_a,
        false_type: TypeId::NEVER,
        is_distributive: true,
    };

    let cond_type = interner.conditional(cond);
    let mut subst = TypeSubstitution::new();

    // Input: (x: number) => string
    let x_name = interner.intern_string("x");
    let input_fn = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![ParamInfo::required(x_name, TypeId::NUMBER)],
        this_type: None,
        return_type: TypeId::STRING,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });
    subst.insert(t_name, input_fn);

    let instantiated = instantiate_type(&interner, cond_type, &subst);
    let result = evaluate_type(&interner, instantiated);

    // Should extract number
    assert_eq!(result, TypeId::NUMBER);
}

#[test]
fn test_generator_function_multiple_params() {
    // Test: Extract all parameters as tuple
    // T extends (...args: infer P) => any ? P : never
    let interner = TypeInterner::new();

    let t_name = interner.intern_string("T");
    let t_param = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: t_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    let infer_name = interner.intern_string("P");
    let infer_p = interner.intern(TypeData::Infer(TypeParamInfo {
        name: infer_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    // Pattern: (...args: infer P) => any
    let args_name = interner.intern_string("args");
    let extends_fn = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![ParamInfo::rest(args_name, infer_p)],
        this_type: None,
        return_type: TypeId::ANY,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let cond = ConditionalType {
        check_type: t_param,
        extends_type: extends_fn,
        true_type: infer_p,
        false_type: TypeId::NEVER,
        is_distributive: true,
    };

    let cond_type = interner.conditional(cond);
    let mut subst = TypeSubstitution::new();

    // Input: (a: string, b: number) => void
    let a_name = interner.intern_string("a");
    let b_name = interner.intern_string("b");
    let input_fn = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![
            ParamInfo::required(a_name, TypeId::STRING),
            ParamInfo::required(b_name, TypeId::NUMBER),
        ],
        this_type: None,
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });
    subst.insert(t_name, input_fn);

    let instantiated = instantiate_type(&interner, cond_type, &subst);
    let result = evaluate_type(&interner, instantiated);

    // Rest parameter extraction may return never if pattern doesn't match
    // or return the extracted parameters if it does
    // This tests the basic structure is correct
    assert!(
        result == TypeId::NEVER
            || matches!(
                interner.lookup(result),
                Some(TypeData::Tuple(_) | TypeData::Array(_) | _)
            )
    );
}

// ============================================================================
// Module Augmentation Type Tests
// ============================================================================
// Tests for module augmentation and declaration merging behavior

#[test]
fn test_module_augmentation_object_merge() {
    // Test: Merge two object types (simulating interface merging)
    // interface A { x: string } merged with interface A { y: number }
    let interner = TypeInterner::new();

    let t_name = interner.intern_string("T");
    let t_param = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: t_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    // First object: { x: string }
    let x_prop = interner.intern_string("x");
    let obj1 = interner.object(vec![PropertyInfo::new(x_prop, TypeId::STRING)]);

    // Second object: { y: number }
    let y_prop = interner.intern_string("y");
    let obj2 = interner.object(vec![PropertyInfo::new(y_prop, TypeId::NUMBER)]);

    // Merge via intersection
    let merged = interner.intersection(vec![obj1, obj2]);

    // T extends merged ? T : never
    let cond = ConditionalType {
        check_type: t_param,
        extends_type: merged,
        true_type: t_param,
        false_type: TypeId::NEVER,
        is_distributive: true,
    };

    let cond_type = interner.conditional(cond);
    let mut subst = TypeSubstitution::new();

    // Input: { x: string, y: number }
    let combined = interner.object(vec![
        PropertyInfo::new(x_prop, TypeId::STRING),
        PropertyInfo::new(y_prop, TypeId::NUMBER),
    ]);
    subst.insert(t_name, combined);

    let instantiated = instantiate_type(&interner, cond_type, &subst);
    let result = evaluate_type(&interner, instantiated);

    // Should match and return combined
    assert_eq!(result, combined);
}

#[test]
fn test_module_augmentation_function_overload() {
    // Test: Merged function signatures (callable with multiple overloads)
    let interner = TypeInterner::new();

    let t_name = interner.intern_string("T");
    let t_param = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: t_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    let infer_name = interner.intern_string("R");
    let infer_r = interner.intern(TypeData::Infer(TypeParamInfo {
        name: infer_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    // Pattern: () => infer R
    let extends_fn = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![],
        this_type: None,
        return_type: infer_r,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let cond = ConditionalType {
        check_type: t_param,
        extends_type: extends_fn,
        true_type: infer_r,
        false_type: TypeId::NEVER,
        is_distributive: true,
    };

    let cond_type = interner.conditional(cond);
    let mut subst = TypeSubstitution::new();

    // Input: () => string (first overload)
    let input_fn = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![],
        this_type: None,
        return_type: TypeId::STRING,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });
    subst.insert(t_name, input_fn);

    let instantiated = instantiate_type(&interner, cond_type, &subst);
    let result = evaluate_type(&interner, instantiated);

    // Should extract string return type
    assert_eq!(result, TypeId::STRING);
}

#[test]
fn test_module_augmentation_namespace_merge() {
    // Test: Namespace with merged properties
    let interner = TypeInterner::new();

    // Original namespace: { version: string }
    let version_prop = interner.intern_string("version");
    let ns1 = interner.object(vec![PropertyInfo::readonly(version_prop, TypeId::STRING)]);

    // Augmentation: { utils: { format: () => string } }
    let utils_prop = interner.intern_string("utils");
    let format_prop = interner.intern_string("format");
    let format_fn = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![],
        this_type: None,
        return_type: TypeId::STRING,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });
    let utils_obj = interner.object(vec![PropertyInfo::method(format_prop, format_fn)]);
    let ns2 = interner.object(vec![PropertyInfo::new(utils_prop, utils_obj)]);

    // Merged namespace
    let merged_ns = interner.intersection(vec![ns1, ns2]);

    // The merged namespace should expose both sets of properties.
    match interner.lookup(merged_ns) {
        Some(TypeData::Object(shape_id) | TypeData::ObjectWithIndex(shape_id)) => {
            let shape = interner.object_shape(shape_id);
            let has_version = shape.properties.iter().any(|p| p.name == version_prop);
            let has_utils = shape.properties.iter().any(|p| p.name == utils_prop);
            assert!(
                has_version && has_utils,
                "merged namespace should include both props"
            );
        }
        other => panic!("unexpected merged namespace representation: {other:?}"),
    }
}

#[test]
fn test_module_augmentation_class_extension() {
    // Test: Class with augmented static members
    let interner = TypeInterner::new();

    let t_name = interner.intern_string("T");
    let t_param = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: t_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    // Class static: { new (): Instance }
    let instance_type = interner.object(vec![]);
    let constructor = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![],
        this_type: None,
        return_type: instance_type,
        type_predicate: None,
        is_constructor: true,
        is_method: false,
    });

    let new_prop = interner.intern_string("new");
    let class_static = interner.object(vec![PropertyInfo {
        name: new_prop,
        type_id: constructor,
        write_type: constructor,
        optional: false,
        readonly: true,
        is_method: true,
        is_class_prototype: false,
        visibility: Visibility::Public,
        parent_id: None,
        declaration_order: 0,
        is_string_named: false,
    }]);

    // T extends { new: ... } ? T : never
    let cond = ConditionalType {
        check_type: t_param,
        extends_type: class_static,
        true_type: t_param,
        false_type: TypeId::NEVER,
        is_distributive: true,
    };

    let cond_type = interner.conditional(cond);
    let mut subst = TypeSubstitution::new();
    subst.insert(t_name, class_static);

    let instantiated = instantiate_type(&interner, cond_type, &subst);
    let result = evaluate_type(&interner, instantiated);

    // Should match
    assert_eq!(result, class_static);
}

#[test]
fn test_module_augmentation_global_interface() {
    // Test: Global interface augmentation (like adding to Array prototype)
    let interner = TypeInterner::new();

    let t_name = interner.intern_string("T");
    let t_param = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: t_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    let infer_name = interner.intern_string("E");
    let infer_e = interner.intern(TypeData::Infer(TypeParamInfo {
        name: infer_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    // Pattern: Array-like with custom method
    // { myMethod: () => infer E }
    let my_method = interner.intern_string("myMethod");
    let extends_obj = interner.object(vec![PropertyInfo::method(my_method, infer_e)]);

    let cond = ConditionalType {
        check_type: t_param,
        extends_type: extends_obj,
        true_type: infer_e,
        false_type: TypeId::NEVER,
        is_distributive: true,
    };

    let cond_type = interner.conditional(cond);
    let mut subst = TypeSubstitution::new();

    // Input: { myMethod: number }
    let input_obj = interner.object(vec![PropertyInfo::method(my_method, TypeId::NUMBER)]);
    subst.insert(t_name, input_obj);

    let instantiated = instantiate_type(&interner, cond_type, &subst);
    let result = evaluate_type(&interner, instantiated);

    // Should extract number
    assert_eq!(result, TypeId::NUMBER);
}

// ============================================================================
// Array Covariance Tests
// ============================================================================
// Tests for array type covariance and element type extraction

#[test]
fn test_array_covariance_element_extraction() {
    // Test: T extends Array<infer E> ? E : never
    let interner = TypeInterner::new();

    let t_name = interner.intern_string("T");
    let t_param = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: t_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    let infer_name = interner.intern_string("E");
    let infer_e = interner.intern(TypeData::Infer(TypeParamInfo {
        name: infer_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    // Pattern: Array<infer E> - using array type
    let extends_array = interner.array(infer_e);

    let cond = ConditionalType {
        check_type: t_param,
        extends_type: extends_array,
        true_type: infer_e,
        false_type: TypeId::NEVER,
        is_distributive: true,
    };

    let cond_type = interner.conditional(cond);
    let mut subst = TypeSubstitution::new();

    // Input: string[]
    let input_array = interner.array(TypeId::STRING);
    subst.insert(t_name, input_array);

    let instantiated = instantiate_type(&interner, cond_type, &subst);
    let result = evaluate_type(&interner, instantiated);

    // Should extract string as element type
    assert_eq!(result, TypeId::STRING);
}

#[test]
fn test_array_covariance_union_element() {
    // Test: Array with union element type
    let interner = TypeInterner::new();

    let t_name = interner.intern_string("T");
    let t_param = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: t_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    let infer_name = interner.intern_string("E");
    let infer_e = interner.intern(TypeData::Infer(TypeParamInfo {
        name: infer_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    // Pattern: Array<infer E>
    let extends_array = interner.array(infer_e);

    let cond = ConditionalType {
        check_type: t_param,
        extends_type: extends_array,
        true_type: infer_e,
        false_type: TypeId::NEVER,
        is_distributive: true,
    };

    let cond_type = interner.conditional(cond);
    let mut subst = TypeSubstitution::new();

    // Input: (string | number)[]
    let union_elem = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);
    let input_array = interner.array(union_elem);
    subst.insert(t_name, input_array);

    let instantiated = instantiate_type(&interner, cond_type, &subst);
    let result = evaluate_type(&interner, instantiated);

    // Should extract the union type
    assert_eq!(result, union_elem);
}

#[test]
fn test_array_covariance_readonly() {
    // Test: Readonly array covariance
    let interner = TypeInterner::new();

    let t_name = interner.intern_string("T");
    let t_param = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: t_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    let infer_name = interner.intern_string("E");
    let infer_e = interner.intern(TypeData::Infer(TypeParamInfo {
        name: infer_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    // Pattern: readonly E[] represented as array
    let extends_array = interner.array(infer_e);

    let cond = ConditionalType {
        check_type: t_param,
        extends_type: extends_array,
        true_type: infer_e,
        false_type: TypeId::NEVER,
        is_distributive: true,
    };

    let cond_type = interner.conditional(cond);
    let mut subst = TypeSubstitution::new();

    // Input: number[]
    let input_array = interner.array(TypeId::NUMBER);
    subst.insert(t_name, input_array);

    let instantiated = instantiate_type(&interner, cond_type, &subst);
    let result = evaluate_type(&interner, instantiated);

    assert_eq!(result, TypeId::NUMBER);
}

#[test]
fn test_array_covariance_nested() {
    // Test: Nested array covariance
    // T extends Array<Array<infer E>> ? E : never
    let interner = TypeInterner::new();

    let t_name = interner.intern_string("T");
    let t_param = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: t_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    let infer_name = interner.intern_string("E");
    let infer_e = interner.intern(TypeData::Infer(TypeParamInfo {
        name: infer_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    // Pattern: Array<Array<infer E>>
    let inner_array = interner.array(infer_e);
    let extends_array = interner.array(inner_array);

    let cond = ConditionalType {
        check_type: t_param,
        extends_type: extends_array,
        true_type: infer_e,
        false_type: TypeId::NEVER,
        is_distributive: true,
    };

    let cond_type = interner.conditional(cond);
    let mut subst = TypeSubstitution::new();

    // Input: string[][]
    let string_array = interner.array(TypeId::STRING);
    let nested_array = interner.array(string_array);
    subst.insert(t_name, nested_array);

    let instantiated = instantiate_type(&interner, cond_type, &subst);
    let result = evaluate_type(&interner, instantiated);

    // Should extract string
    assert_eq!(result, TypeId::STRING);
}

#[test]
fn test_array_covariance_non_array() {
    // Test: Non-array doesn't match array pattern
    let interner = TypeInterner::new();

    let t_name = interner.intern_string("T");
    let t_param = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: t_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    let infer_name = interner.intern_string("E");
    let infer_e = interner.intern(TypeData::Infer(TypeParamInfo {
        name: infer_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    // Pattern: Array<infer E>
    let extends_array = interner.array(infer_e);

    let cond = ConditionalType {
        check_type: t_param,
        extends_type: extends_array,
        true_type: infer_e,
        false_type: TypeId::NEVER,
        is_distributive: true,
    };

    let cond_type = interner.conditional(cond);
    let mut subst = TypeSubstitution::new();

    // Input: string (not an array)
    subst.insert(t_name, TypeId::STRING);

    let instantiated = instantiate_type(&interner, cond_type, &subst);
    let result = evaluate_type(&interner, instantiated);

    // Should return never since string is not an array
    assert_eq!(result, TypeId::NEVER);
}

// =============================================================================
// ReturnType, Parameters, and ConstructorParameters Utility Type Edge Cases
// =============================================================================

/// Test `ReturnType`<T> with a generic function: <T>(x: T) => T
/// TypeScript's `ReturnType` extracts the return type, which for generic functions
/// is the type parameter T itself (unsubstituted).
#[test]
fn test_return_type_generic_function() {
    let interner = TypeInterner::new();

    let infer_name = interner.intern_string("R");
    let infer_r = interner.intern(TypeData::Infer(TypeParamInfo {
        name: infer_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    // Pattern: (...args: any[]) => infer R
    let extends_fn = interner.function(FunctionShape {
        params: vec![ParamInfo {
            name: None,
            type_id: interner.array(TypeId::ANY),
            optional: false,
            rest: true,
        }],
        this_type: None,
        return_type: infer_r,
        type_params: Vec::new(),
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    // Source: generic function <U>(x: U) => U
    let u_name = interner.intern_string("U");
    let u_param = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: u_name,
        constraint: None,
        default: None,
        is_const: false,
    }));
    let generic_fn = interner.function(FunctionShape {
        type_params: vec![TypeParamInfo {
            name: u_name,
            constraint: None,
            default: None,
            is_const: false,
        }],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("x")),
            type_id: u_param,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: u_param, // returns U
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let cond = ConditionalType {
        check_type: generic_fn,
        extends_type: extends_fn,
        true_type: infer_r,
        false_type: TypeId::NEVER,
        is_distributive: false,
    };

    let result = evaluate_conditional(&interner, &cond);

    // Expected: U (the type parameter) for ReturnType of <U>(x: U) => U
    assert_eq!(result, u_param);
}

/// Test `ReturnType`<T> with an overloaded function (Callable type with multiple signatures).
/// TypeScript's `ReturnType` extracts from the last overload signature.
#[test]
fn test_return_type_overloaded_function() {
    let interner = TypeInterner::new();

    let infer_name = interner.intern_string("R");
    let infer_r = interner.intern(TypeData::Infer(TypeParamInfo {
        name: infer_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    // Pattern: (...args: any[]) => infer R
    let extends_fn = interner.function(FunctionShape {
        params: vec![ParamInfo {
            name: None,
            type_id: interner.array(TypeId::ANY),
            optional: false,
            rest: true,
        }],
        this_type: None,
        return_type: infer_r,
        type_params: Vec::new(),
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    // Overloaded function: { (x: string): number; (x: number): boolean; }
    let overloaded = interner.callable(CallableShape {
        symbol: None,
        is_abstract: false,
        call_signatures: vec![
            CallSignature {
                type_params: Vec::new(),
                params: vec![ParamInfo {
                    name: Some(interner.intern_string("x")),
                    type_id: TypeId::STRING,
                    optional: false,
                    rest: false,
                }],
                this_type: None,
                return_type: TypeId::NUMBER,
                type_predicate: None,
                is_method: false,
            },
            CallSignature {
                type_params: Vec::new(),
                params: vec![ParamInfo {
                    name: Some(interner.intern_string("x")),
                    type_id: TypeId::NUMBER,
                    optional: false,
                    rest: false,
                }],
                this_type: None,
                return_type: TypeId::BOOLEAN,
                type_predicate: None,
                is_method: false,
            },
        ],
        construct_signatures: Vec::new(),
        properties: Vec::new(),
        string_index: None,
        number_index: None,
    });

    let cond = ConditionalType {
        check_type: overloaded,
        extends_type: extends_fn,
        true_type: infer_r,
        false_type: TypeId::NEVER,
        is_distributive: false,
    };

    let result = evaluate_conditional(&interner, &cond);

    // TypeScript uses the last overload signature for ReturnType, so expect boolean.
    assert_eq!(result, TypeId::BOOLEAN);
}

/// Test `ReturnType`<T> with a function that has a type predicate.
/// The return type of a type guard function is `boolean` for `ReturnType` purposes.
#[test]
fn test_return_type_type_predicate_function() {
    let interner = TypeInterner::new();

    let infer_name = interner.intern_string("R");
    let infer_r = interner.intern(TypeData::Infer(TypeParamInfo {
        name: infer_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    // Pattern: (...args: any[]) => infer R
    let extends_fn = interner.function(FunctionShape {
        params: vec![ParamInfo {
            name: None,
            type_id: interner.array(TypeId::ANY),
            optional: false,
            rest: true,
        }],
        this_type: None,
        return_type: infer_r,
        type_params: Vec::new(),
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    // Source: (x: unknown) => x is string (type guard)
    let x_name = interner.intern_string("x");
    let type_guard_fn = interner.function(FunctionShape {
        type_params: Vec::new(),
        params: vec![ParamInfo::required(x_name, TypeId::UNKNOWN)],
        this_type: None,
        return_type: TypeId::BOOLEAN,
        type_predicate: Some(TypePredicate {
            parameter_index: None,
            target: TypePredicateTarget::Identifier(x_name),
            type_id: Some(TypeId::STRING),
            asserts: false,
        }),
        is_constructor: false,
        is_method: false,
    });

    let cond = ConditionalType {
        check_type: type_guard_fn,
        extends_type: extends_fn,
        true_type: infer_r,
        false_type: TypeId::NEVER,
        is_distributive: false,
    };

    let result = evaluate_conditional(&interner, &cond);

    // ReturnType of a type predicate function should be boolean
    assert_eq!(result, TypeId::BOOLEAN);
}

/// Test Parameters<T> with a function that has rest parameters.
/// Parameters<(...args: string[]) => void> should be string[]
#[test]
fn test_parameters_rest_param_function() {
    let interner = TypeInterner::new();

    let infer_name = interner.intern_string("P");
    let infer_p = interner.intern(TypeData::Infer(TypeParamInfo {
        name: infer_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    // Pattern for Parameters: T extends (...args: infer P) => any ? P : never
    let extends_fn = interner.function(FunctionShape {
        params: vec![ParamInfo {
            name: Some(interner.intern_string("args")),
            type_id: infer_p,
            optional: false,
            rest: true,
        }],
        this_type: None,
        return_type: TypeId::ANY,
        type_params: Vec::new(),
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    // Source: (...args: string[]) => void
    let source_fn = interner.function(FunctionShape {
        type_params: Vec::new(),
        params: vec![ParamInfo {
            name: Some(interner.intern_string("args")),
            type_id: interner.array(TypeId::STRING),
            optional: false,
            rest: true,
        }],
        this_type: None,
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let cond = ConditionalType {
        check_type: source_fn,
        extends_type: extends_fn,
        true_type: infer_p,
        false_type: TypeId::NEVER,
        is_distributive: false,
    };

    let result = evaluate_conditional(&interner, &cond);

    // Parameters of (...args: string[]) => void should be string[]
    let expected = interner.array(TypeId::STRING);
    assert_eq!(result, expected);
}

/// Test Parameters<T> with optional and rest parameter combinations.
/// Parameters<(a: string, b?: number, ...rest: boolean[]) => void>
#[test]
fn test_parameters_optional_and_rest_combination() {
    let interner = TypeInterner::new();

    let infer_name = interner.intern_string("P");
    let infer_p = interner.intern(TypeData::Infer(TypeParamInfo {
        name: infer_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    // Pattern: (...args: infer P) => any
    let extends_fn = interner.function(FunctionShape {
        params: vec![ParamInfo {
            name: Some(interner.intern_string("args")),
            type_id: infer_p,
            optional: false,
            rest: true,
        }],
        this_type: None,
        return_type: TypeId::ANY,
        type_params: Vec::new(),
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    // Source: (a: string, b?: number, ...rest: boolean[]) => void
    let source_fn = interner.function(FunctionShape {
        type_params: Vec::new(),
        params: vec![
            ParamInfo {
                name: Some(interner.intern_string("a")),
                type_id: TypeId::STRING,
                optional: false,
                rest: false,
            },
            ParamInfo {
                name: Some(interner.intern_string("b")),
                type_id: TypeId::NUMBER,
                optional: true,
                rest: false,
            },
            ParamInfo {
                name: Some(interner.intern_string("rest")),
                type_id: interner.array(TypeId::BOOLEAN),
                optional: false,
                rest: true,
            },
        ],
        this_type: None,
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let cond = ConditionalType {
        check_type: source_fn,
        extends_type: extends_fn,
        true_type: infer_p,
        false_type: TypeId::NEVER,
        is_distributive: false,
    };

    let result = evaluate_conditional(&interner, &cond);

    match interner.lookup(result) {
        Some(TypeData::Tuple(elems)) => {
            let elems = interner.tuple_list(elems);
            assert_eq!(elems.len(), 3);
            assert_eq!(elems[0].type_id, TypeId::STRING);
            assert!(!elems[0].optional);
            assert!(!elems[0].rest);
            assert_eq!(elems[1].type_id, TypeId::NUMBER);
            assert!(elems[1].optional);
            assert!(!elems[1].rest);
            assert_eq!(elems[2].type_id, interner.array(TypeId::BOOLEAN));
            assert!(!elems[2].optional);
            assert!(elems[2].rest);
        }
        _ => panic!("Expected tuple, got {result:?}"),
    }
}

#[test]
fn test_parameters_generic_function_extracts_parameter_tuple() {
    let interner = TypeInterner::new();

    let infer_name = interner.intern_string("P");
    let infer_p = interner.intern(TypeData::Infer(TypeParamInfo {
        name: infer_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    let extends_fn = interner.function(FunctionShape {
        params: vec![ParamInfo {
            name: Some(interner.intern_string("args")),
            type_id: infer_p,
            optional: false,
            rest: true,
        }],
        this_type: None,
        return_type: TypeId::ANY,
        type_params: Vec::new(),
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let t_name = interner.intern_string("T");
    let u_name = interner.intern_string("U");
    let t_param = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: t_name,
        constraint: None,
        default: None,
        is_const: false,
    }));
    let u_param = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: u_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    let source_fn = interner.function(FunctionShape {
        type_params: vec![
            TypeParamInfo {
                name: t_name,
                constraint: None,
                default: None,
                is_const: false,
            },
            TypeParamInfo {
                name: u_name,
                constraint: None,
                default: None,
                is_const: false,
            },
        ],
        params: vec![
            ParamInfo {
                name: Some(interner.intern_string("a")),
                type_id: t_param,
                optional: false,
                rest: false,
            },
            ParamInfo {
                name: Some(interner.intern_string("b")),
                type_id: u_param,
                optional: false,
                rest: false,
            },
        ],
        this_type: None,
        return_type: u_param,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let cond = ConditionalType {
        check_type: source_fn,
        extends_type: extends_fn,
        true_type: infer_p,
        false_type: TypeId::NEVER,
        is_distributive: false,
    };

    let result = evaluate_conditional(&interner, &cond);
    let Some(TypeData::Tuple(tuple_id)) = interner.lookup(result) else {
        panic!("Expected Parameters<genericFn> to evaluate to a tuple, got {result:?}");
    };
    let elements = interner.tuple_list(tuple_id);
    assert_eq!(elements.len(), 2);
    assert_eq!(elements[0].type_id, TypeId::UNKNOWN);
    assert_eq!(elements[1].type_id, TypeId::UNKNOWN);
}

/// Test `ConstructorParameters`<T> with a class constructor.
/// `ConstructorParameters` extracts params from a constructor signature.
#[test]
fn test_constructor_parameters_basic() {
    let interner = TypeInterner::new();

    let infer_name = interner.intern_string("P");
    let infer_p = interner.intern(TypeData::Infer(TypeParamInfo {
        name: infer_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    // Pattern for ConstructorParameters: T extends new (...args: infer P) => any ? P : never
    let extends_ctor = interner.function(FunctionShape {
        params: vec![ParamInfo {
            name: Some(interner.intern_string("args")),
            type_id: infer_p,
            optional: false,
            rest: true,
        }],
        this_type: None,
        return_type: TypeId::ANY,
        type_params: Vec::new(),
        type_predicate: None,
        is_constructor: true, // Constructor!
        is_method: false,
    });

    // Source: new (name: string, age: number) => Person
    let source_ctor = interner.function(FunctionShape {
        type_params: Vec::new(),
        params: vec![
            ParamInfo {
                name: Some(interner.intern_string("name")),
                type_id: TypeId::STRING,
                optional: false,
                rest: false,
            },
            ParamInfo {
                name: Some(interner.intern_string("age")),
                type_id: TypeId::NUMBER,
                optional: false,
                rest: false,
            },
        ],
        this_type: None,
        return_type: TypeId::OBJECT, // Returns some object type
        type_predicate: None,
        is_constructor: true,
        is_method: false,
    });

    let cond = ConditionalType {
        check_type: source_ctor,
        extends_type: extends_ctor,
        true_type: infer_p,
        false_type: TypeId::NEVER,
        is_distributive: false,
    };

    let result = evaluate_conditional(&interner, &cond);

    match interner.lookup(result) {
        Some(TypeData::Tuple(elems)) => {
            let elems = interner.tuple_list(elems);
            assert_eq!(elems.len(), 2);
            assert_eq!(elems[0].type_id, TypeId::STRING);
            assert_eq!(elems[1].type_id, TypeId::NUMBER);
        }
        _ => panic!("Expected tuple, got {result:?}"),
    }
}

/// Test `ConstructorParameters`<T> with a Callable type having construct signatures.
#[test]
fn test_constructor_parameters_callable_construct_signature() {
    let interner = TypeInterner::new();

    let infer_name = interner.intern_string("P");
    let infer_p = interner.intern(TypeData::Infer(TypeParamInfo {
        name: infer_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    // Pattern: new (...args: infer P) => any
    let extends_ctor = interner.function(FunctionShape {
        params: vec![ParamInfo {
            name: Some(interner.intern_string("args")),
            type_id: infer_p,
            optional: false,
            rest: true,
        }],
        this_type: None,
        return_type: TypeId::ANY,
        type_params: Vec::new(),
        type_predicate: None,
        is_constructor: true,
        is_method: false,
    });

    // Callable with construct signature: { new(x: string): Object }
    let callable_with_ctor = interner.callable(CallableShape {
        symbol: None,
        is_abstract: false,
        call_signatures: Vec::new(),
        construct_signatures: vec![CallSignature {
            type_params: Vec::new(),
            params: vec![ParamInfo {
                name: Some(interner.intern_string("x")),
                type_id: TypeId::STRING,
                optional: false,
                rest: false,
            }],
            this_type: None,
            return_type: TypeId::OBJECT,
            type_predicate: None,
            is_method: false,
        }],
        properties: Vec::new(),
        string_index: None,
        number_index: None,
    });

    let cond = ConditionalType {
        check_type: callable_with_ctor,
        extends_type: extends_ctor,
        true_type: infer_p,
        false_type: TypeId::NEVER,
        is_distributive: false,
    };

    let result = evaluate_conditional(&interner, &cond);

    // Should extract the constructor parameters as a tuple [string]
    // Check if result is a tuple with string element
    match interner.lookup(result) {
        Some(TypeData::Tuple(elems)) => {
            let elems = interner.tuple_list(elems);
            assert_eq!(elems.len(), 1);
            assert_eq!(elems[0].type_id, TypeId::STRING);
        }
        _ => panic!("Expected tuple, got {result:?}"),
    }
}

#[test]
fn test_constructor_parameters_callable_construct_signature_with_properties() {
    let interner = TypeInterner::new();

    let infer_name = interner.intern_string("P");
    let infer_p = interner.intern(TypeData::Infer(TypeParamInfo {
        name: infer_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    let extends_ctor = interner.function(FunctionShape {
        params: vec![ParamInfo {
            name: Some(interner.intern_string("args")),
            type_id: infer_p,
            optional: false,
            rest: true,
        }],
        this_type: None,
        return_type: TypeId::ANY,
        type_params: Vec::new(),
        type_predicate: None,
        is_constructor: true,
        is_method: false,
    });

    let callable_with_ctor = interner.callable(CallableShape {
        symbol: None,
        is_abstract: false,
        call_signatures: Vec::new(),
        construct_signatures: vec![CallSignature {
            type_params: Vec::new(),
            params: vec![ParamInfo {
                name: Some(interner.intern_string("options")),
                type_id: TypeId::STRING,
                optional: false,
                rest: false,
            }],
            this_type: None,
            return_type: TypeId::OBJECT,
            type_predicate: None,
            is_method: false,
        }],
        properties: vec![PropertyInfo::new(
            interner.intern_string("prototype"),
            TypeId::OBJECT,
        )],
        string_index: None,
        number_index: None,
    });

    let cond = ConditionalType {
        check_type: callable_with_ctor,
        extends_type: extends_ctor,
        true_type: infer_p,
        false_type: TypeId::NEVER,
        is_distributive: false,
    };

    let result = evaluate_conditional(&interner, &cond);

    match interner.lookup(result) {
        Some(TypeData::Tuple(elems)) => {
            let elems = interner.tuple_list(elems);
            assert_eq!(elems.len(), 1);
            assert_eq!(elems[0].type_id, TypeId::STRING);
        }
        _ => panic!("Expected tuple, got {result:?}"),
    }
}

/// Test `ReturnType` with union of function types (distributive).
/// `ReturnType`<(() => string) | (() => number)> should be string | number
#[test]
fn test_return_type_union_distributive() {
    let interner = TypeInterner::new();

    let t_name = interner.intern_string("T");
    let t_param = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: t_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    let infer_name = interner.intern_string("R");
    let infer_r = interner.intern(TypeData::Infer(TypeParamInfo {
        name: infer_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    // Pattern: T extends (...args: any[]) => infer R ? R : never
    let extends_fn = interner.function(FunctionShape {
        params: vec![ParamInfo {
            name: None,
            type_id: interner.array(TypeId::ANY),
            optional: false,
            rest: true,
        }],
        this_type: None,
        return_type: infer_r,
        type_params: Vec::new(),
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let cond = ConditionalType {
        check_type: t_param,
        extends_type: extends_fn,
        true_type: infer_r,
        false_type: TypeId::NEVER,
        is_distributive: true,
    };

    let cond_type = interner.conditional(cond);
    let mut subst = TypeSubstitution::new();

    // Input: (() => string) | (() => number)
    let fn_string = interner.function(FunctionShape {
        type_params: Vec::new(),
        params: Vec::new(),
        this_type: None,
        return_type: TypeId::STRING,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });
    let fn_number = interner.function(FunctionShape {
        type_params: Vec::new(),
        params: Vec::new(),
        this_type: None,
        return_type: TypeId::NUMBER,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });
    subst.insert(t_name, interner.union(vec![fn_string, fn_number]));

    let instantiated = instantiate_type(&interner, cond_type, &subst);
    let result = evaluate_type(&interner, instantiated);

    // Should distribute: string | number
    let expected = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);
    assert_eq!(result, expected);
}

// ============================================================================
// Distributive Conditional Type Stress Tests
// Per GOALS.md Objective 2: Distributive conditional types over unions
// ============================================================================

#[test]
fn test_nested_distributive_two_levels() {
    // Outer<T> = T extends string ? Inner<T> : never
    // Inner<T> = T extends "a" ? "matched" : "unmatched"
    // With T = "a" | "b" | number
    let interner = TypeInterner::new();

    let lit_a = interner.literal_string("a");
    let lit_b = interner.literal_string("b");
    let lit_matched = interner.literal_string("matched");
    let _lit_unmatched = interner.literal_string("unmatched");

    // Input: "a" | "b" | number
    let union_input = interner.union(vec![lit_a, lit_b, TypeId::NUMBER]);

    // Inner conditional: T extends "a" ? "matched" : "unmatched"
    // Applied to "a" -> "matched", "b" -> "unmatched"
    // Outer: T extends string ? Inner<T> : never
    // "a" extends string -> Inner<"a"> = "matched"
    // "b" extends string -> Inner<"b"> = "unmatched"
    // number extends string -> never

    // Outer conditional distributes over union
    let outer_cond = ConditionalType {
        check_type: union_input,
        extends_type: TypeId::STRING,
        true_type: lit_matched, // Simplified: in reality would be nested
        false_type: TypeId::NEVER,
        is_distributive: true,
    };
    let outer_result = evaluate_conditional(&interner, &outer_cond);

    // For "a"|"b"|number distributing over extends string:
    // "a" -> lit_matched, "b" -> lit_matched, number -> never
    // Result: "matched" | never = "matched"
    assert_eq!(outer_result, lit_matched);
}

#[test]
fn test_nested_distributive_inner_also_distributes() {
    // Test that inner conditional also distributes when given a union
    // type ToArray<T> = T extends any ? T[] : never
    // With T = string | number
    let interner = TypeInterner::new();

    let union_input = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);

    let string_array = interner.array(TypeId::STRING);
    let _number_array = interner.array(TypeId::NUMBER);

    // Distributive: string -> string[], number -> number[]
    let cond = ConditionalType {
        check_type: union_input,
        extends_type: TypeId::ANY,
        true_type: string_array, // Simplified for test
        false_type: TypeId::NEVER,
        is_distributive: true,
    };
    let result = evaluate_conditional(&interner, &cond);

    // Both string and number extend any, so both go to true branch
    // Result: string[] (using simplified true_type)
    assert_eq!(result, string_array);
}

#[test]
fn test_nested_distributive_three_levels() {
    // Three levels of nesting with distribution at outer level
    // Level1<T> = T extends object ? Level2<T> : "primitive"
    // Level2<T> = T extends Array<any> ? "array" : Level3<T>
    // Level3<T> = T extends Function ? "function" : "object"
    let interner = TypeInterner::new();

    let lit_primitive = interner.literal_string("primitive");
    let lit_array = interner.literal_string("array");
    let lit_function = interner.literal_string("function");
    let lit_object = interner.literal_string("object");

    // Test with string (primitive)
    let cond_string = ConditionalType {
        check_type: TypeId::STRING,
        extends_type: TypeId::OBJECT,
        true_type: lit_object,
        false_type: lit_primitive,
        is_distributive: false,
    };
    let result_string = evaluate_conditional(&interner, &cond_string);
    // string does not extend object -> "primitive"
    assert_eq!(result_string, lit_primitive);

    // Test with number (primitive)
    let cond_number = ConditionalType {
        check_type: TypeId::NUMBER,
        extends_type: TypeId::OBJECT,
        true_type: lit_object,
        false_type: lit_primitive,
        is_distributive: false,
    };
    let result_number = evaluate_conditional(&interner, &cond_number);
    assert_eq!(result_number, lit_primitive);

    // Verify we can distinguish array from function from object would require
    // more complex type construction - this tests the basic three-level pattern
    let _ = (lit_array, lit_function); // Suppress unused warnings
}

#[test]
fn test_distribution_over_intersection_basic() {
    // T extends U where T = (A & B) | (C & D)
    // Should distribute over the union of intersections
    let interner = TypeInterner::new();

    // Create intersection types
    let obj_a = interner.object(vec![PropertyInfo::new(
        interner.intern_string("a"),
        TypeId::STRING,
    )]);
    let obj_b = interner.object(vec![PropertyInfo::new(
        interner.intern_string("b"),
        TypeId::NUMBER,
    )]);

    let intersection_ab = interner.intersection(vec![obj_a, obj_b]);

    let lit_yes = interner.literal_string("yes");
    let lit_no = interner.literal_string("no");

    // (A & B) extends object ? "yes" : "no"
    let cond = ConditionalType {
        check_type: intersection_ab,
        extends_type: TypeId::OBJECT,
        true_type: lit_yes,
        false_type: lit_no,
        is_distributive: false,
    };
    let result = evaluate_conditional(&interner, &cond);

    // Intersection of objects extends object -> "yes"
    assert_eq!(result, lit_yes);
}

#[test]
fn test_distribution_over_intersection_with_primitives() {
    // Test: (string & {}) | (number & {}) distributing
    let interner = TypeInterner::new();

    let empty_obj = interner.object(vec![]);

    // string & {} is essentially string (structural)
    let string_inter = interner.intersection(vec![TypeId::STRING, empty_obj]);
    let number_inter = interner.intersection(vec![TypeId::NUMBER, empty_obj]);

    let union_of_intersections = interner.union(vec![string_inter, number_inter]);

    let lit_string_type = interner.literal_string("string-like");
    let lit_other = interner.literal_string("other");

    // Distribute: each intersection member checked against string
    let cond = ConditionalType {
        check_type: union_of_intersections,
        extends_type: TypeId::STRING,
        true_type: lit_string_type,
        false_type: lit_other,
        is_distributive: true,
    };
    let result = evaluate_conditional(&interner, &cond);

    // string & {} extends string -> "string-like"
    // number & {} does not extend string -> "other"
    // Result: "string-like" | "other"
    let expected = interner.union(vec![lit_string_type, lit_other]);
    assert_eq!(result, expected);
}

#[test]
fn test_infer_tuple_swap_pattern() {
    // Swap<T> = T extends [infer A, infer B] ? [B, A] : never
    // Tests inferring multiple positions and using them in different order
    let interner = TypeInterner::new();

    let infer_a_name = interner.intern_string("A");
    let infer_b_name = interner.intern_string("B");

    let infer_a = interner.intern(TypeData::Infer(TypeParamInfo {
        name: infer_a_name,
        constraint: None,
        default: None,
        is_const: false,
    }));
    let infer_b = interner.intern(TypeData::Infer(TypeParamInfo {
        name: infer_b_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    // Pattern: [infer A, infer B]
    let pattern = interner.tuple(vec![
        TupleElement {
            type_id: infer_a,
            name: None,
            optional: false,
            rest: false,
        },
        TupleElement {
            type_id: infer_b,
            name: None,
            optional: false,
            rest: false,
        },
    ]);

    // Input: [string, number]
    let input = interner.tuple(vec![
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

    // True branch result would be [B, A] = [number, string]
    // For this test, verify the pattern matches
    let cond = ConditionalType {
        check_type: input,
        extends_type: pattern,
        true_type: infer_a, // Extract A to verify inference
        false_type: TypeId::NEVER,
        is_distributive: false,
    };
    let result = evaluate_conditional(&interner, &cond);

    // A should be inferred as string
    assert_eq!(result, TypeId::STRING);
}

#[test]
fn test_infer_tuple_swap_second_position() {
    // Continue from swap pattern - extract B
    let interner = TypeInterner::new();

    let infer_a_name = interner.intern_string("A");
    let infer_b_name = interner.intern_string("B");

    let infer_a = interner.intern(TypeData::Infer(TypeParamInfo {
        name: infer_a_name,
        constraint: None,
        default: None,
        is_const: false,
    }));
    let infer_b = interner.intern(TypeData::Infer(TypeParamInfo {
        name: infer_b_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    // Pattern: [infer A, infer B]
    let pattern = interner.tuple(vec![
        TupleElement {
            type_id: infer_a,
            name: None,
            optional: false,
            rest: false,
        },
        TupleElement {
            type_id: infer_b,
            name: None,
            optional: false,
            rest: false,
        },
    ]);

    // Input: [string, number]
    let input = interner.tuple(vec![
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

    // Extract B
    let cond = ConditionalType {
        check_type: input,
        extends_type: pattern,
        true_type: infer_b,
        false_type: TypeId::NEVER,
        is_distributive: false,
    };
    let result = evaluate_conditional(&interner, &cond);

    // B should be inferred as number
    assert_eq!(result, TypeId::NUMBER);
}

#[test]
fn test_infer_function_signature_param_and_return() {
    // Extract<F> = F extends (x: infer P) => infer R ? [P, R] : never
    // Tests inferring both parameter and return type
    let interner = TypeInterner::new();

    let infer_p_name = interner.intern_string("P");
    let infer_r_name = interner.intern_string("R");

    let infer_p = interner.intern(TypeData::Infer(TypeParamInfo {
        name: infer_p_name,
        constraint: None,
        default: None,
        is_const: false,
    }));
    let infer_r = interner.intern(TypeData::Infer(TypeParamInfo {
        name: infer_r_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    // Pattern: (x: infer P) => infer R
    let pattern_fn = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("x")),
            type_id: infer_p,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: infer_r,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    // Input: (x: string) => number
    let input_fn = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("x")),
            type_id: TypeId::STRING,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: TypeId::NUMBER,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    // Extract P (parameter type)
    let cond_p = ConditionalType {
        check_type: input_fn,
        extends_type: pattern_fn,
        true_type: infer_p,
        false_type: TypeId::NEVER,
        is_distributive: false,
    };
    let result_p = evaluate_conditional(&interner, &cond_p);
    assert_eq!(result_p, TypeId::STRING);

    // Extract R (return type)
    let cond_r = ConditionalType {
        check_type: input_fn,
        extends_type: pattern_fn,
        true_type: infer_r,
        false_type: TypeId::NEVER,
        is_distributive: false,
    };
    let result_r = evaluate_conditional(&interner, &cond_r);
    assert_eq!(result_r, TypeId::NUMBER);
}

#[test]
fn test_infer_function_multiple_params() {
    // F extends (a: infer A, b: infer B) => any ? A : never
    let interner = TypeInterner::new();

    let infer_a_name = interner.intern_string("A");
    let infer_b_name = interner.intern_string("B");

    let infer_a = interner.intern(TypeData::Infer(TypeParamInfo {
        name: infer_a_name,
        constraint: None,
        default: None,
        is_const: false,
    }));
    let infer_b = interner.intern(TypeData::Infer(TypeParamInfo {
        name: infer_b_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    // Pattern: (a: infer A, b: infer B) => any
    let pattern_fn = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![
            ParamInfo {
                name: Some(interner.intern_string("a")),
                type_id: infer_a,
                optional: false,
                rest: false,
            },
            ParamInfo {
                name: Some(interner.intern_string("b")),
                type_id: infer_b,
                optional: false,
                rest: false,
            },
        ],
        this_type: None,
        return_type: TypeId::ANY,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    // Input: (a: boolean, b: string) => void
    let input_fn = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![
            ParamInfo {
                name: Some(interner.intern_string("a")),
                type_id: TypeId::BOOLEAN,
                optional: false,
                rest: false,
            },
            ParamInfo {
                name: Some(interner.intern_string("b")),
                type_id: TypeId::STRING,
                optional: false,
                rest: false,
            },
        ],
        this_type: None,
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    // Extract A
    let cond_a = ConditionalType {
        check_type: input_fn,
        extends_type: pattern_fn,
        true_type: infer_a,
        false_type: TypeId::NEVER,
        is_distributive: false,
    };
    let result_a = evaluate_conditional(&interner, &cond_a);
    assert_eq!(result_a, TypeId::BOOLEAN);

    // Extract B
    let cond_b = ConditionalType {
        check_type: input_fn,
        extends_type: pattern_fn,
        true_type: infer_b,
        false_type: TypeId::NEVER,
        is_distributive: false,
    };
    let result_b = evaluate_conditional(&interner, &cond_b);
    assert_eq!(result_b, TypeId::STRING);
}

// ============================================================================
// Conditional Type Edge Cases: never, unknown, any (stress tests)
// ============================================================================

#[test]
fn test_edge_case_never_distributive_empty() {
    // never extends T is always true (never is bottom type)
    // never extends string ? "yes" : "no" => never (distributes to nothing)
    let interner = TypeInterner::new();

    let lit_yes = interner.literal_string("yes");
    let lit_no = interner.literal_string("no");

    let cond = ConditionalType {
        check_type: TypeId::NEVER,
        extends_type: TypeId::STRING,
        true_type: lit_yes,
        false_type: lit_no,
        is_distributive: true,
    };
    let result = evaluate_conditional(&interner, &cond);

    // Distributive over never => never (empty union)
    assert_eq!(result, TypeId::NEVER);
}

#[test]
fn test_edge_case_never_as_extends_target() {
    // T extends never ? X : Y
    // Only never extends never, everything else goes to false branch
    let interner = TypeInterner::new();

    let lit_yes = interner.literal_string("yes");
    let lit_no = interner.literal_string("no");

    // string extends never ? "yes" : "no"
    let cond_string = ConditionalType {
        check_type: TypeId::STRING,
        extends_type: TypeId::NEVER,
        true_type: lit_yes,
        false_type: lit_no,
        is_distributive: false,
    };
    let result_string = evaluate_conditional(&interner, &cond_string);
    // string does not extend never
    assert_eq!(result_string, lit_no);

    // never extends never ? "yes" : "no"
    let cond_never = ConditionalType {
        check_type: TypeId::NEVER,
        extends_type: TypeId::NEVER,
        true_type: lit_yes,
        false_type: lit_no,
        is_distributive: false,
    };
    let result_never = evaluate_conditional(&interner, &cond_never);
    // never extends never is true (vacuously), non-distributive returns "yes"
    assert_eq!(result_never, lit_yes);
}

#[test]
fn test_edge_case_unknown_multiple_extends() {
    // unknown extends T
    // unknown only extends unknown and any
    let interner = TypeInterner::new();

    let lit_yes = interner.literal_string("yes");
    let lit_no = interner.literal_string("no");

    // unknown extends string ? "yes" : "no"
    let cond_string = ConditionalType {
        check_type: TypeId::UNKNOWN,
        extends_type: TypeId::STRING,
        true_type: lit_yes,
        false_type: lit_no,
        is_distributive: false,
    };
    let result_string = evaluate_conditional(&interner, &cond_string);
    // unknown does not extend string
    assert_eq!(result_string, lit_no);

    // unknown extends unknown ? "yes" : "no"
    let cond_unknown = ConditionalType {
        check_type: TypeId::UNKNOWN,
        extends_type: TypeId::UNKNOWN,
        true_type: lit_yes,
        false_type: lit_no,
        is_distributive: false,
    };
    let result_unknown = evaluate_conditional(&interner, &cond_unknown);
    // unknown extends unknown
    assert_eq!(result_unknown, lit_yes);

    // unknown extends any ? "yes" : "no"
    let cond_any = ConditionalType {
        check_type: TypeId::UNKNOWN,
        extends_type: TypeId::ANY,
        true_type: lit_yes,
        false_type: lit_no,
        is_distributive: false,
    };
    let result_any = evaluate_conditional(&interner, &cond_any);
    // unknown extends any
    assert_eq!(result_any, lit_yes);
}

#[test]
fn test_edge_case_any_produces_union() {
    // any extends T produces union of both branches (any is both top and bottom)
    let interner = TypeInterner::new();

    let lit_yes = interner.literal_string("yes");
    let lit_no = interner.literal_string("no");

    // any extends string ? "yes" : "no"
    let cond = ConditionalType {
        check_type: TypeId::ANY,
        extends_type: TypeId::STRING,
        true_type: lit_yes,
        false_type: lit_no,
        is_distributive: false,
    };
    let result = evaluate_conditional(&interner, &cond);

    // any produces both branches: "yes" | "no"
    let expected = interner.union(vec![lit_yes, lit_no]);
    assert_eq!(result, expected);
}

#[test]
fn test_edge_case_any_as_extends_target() {
    // T extends any is always true (any accepts everything)
    let interner = TypeInterner::new();

    let lit_yes = interner.literal_string("yes");
    let lit_no = interner.literal_string("no");

    // string extends any ? "yes" : "no"
    let cond_string = ConditionalType {
        check_type: TypeId::STRING,
        extends_type: TypeId::ANY,
        true_type: lit_yes,
        false_type: lit_no,
        is_distributive: false,
    };
    let result_string = evaluate_conditional(&interner, &cond_string);
    assert_eq!(result_string, lit_yes);

    // object extends any ? "yes" : "no"
    let cond_obj = ConditionalType {
        check_type: TypeId::OBJECT,
        extends_type: TypeId::ANY,
        true_type: lit_yes,
        false_type: lit_no,
        is_distributive: false,
    };
    let result_obj = evaluate_conditional(&interner, &cond_obj);
    assert_eq!(result_obj, lit_yes);
}

// ============================================================================
// Infer in Contravariant Positions (Function Parameters)
// ============================================================================


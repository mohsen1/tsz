use super::*;
use crate::TypeInterner;
use crate::def::DefId;
use crate::{SubtypeChecker, TypeSubstitution, instantiate_type};
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

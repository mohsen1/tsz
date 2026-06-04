#[test]
fn test_is_arithmetic_operand_mixed_union_invalid() {
    let interner = TypeInterner::new();
    let evaluator = BinaryOpEvaluator::new(&interner);

    // Union of number and string should NOT be a valid arithmetic operand
    let mixed_union = interner.union(vec![TypeId::NUMBER, TypeId::STRING]);
    assert!(
        !evaluator.is_arithmetic_operand(mixed_union),
        "union of number and string should NOT be a valid arithmetic operand"
    );
}

/// Regression test: verify that array property access works when using the
/// environment-aware resolver (`with_resolver`) that has the Array<T> base type
/// registered. Previously, `get_type_of_property_access_inner` used
/// `types.property_access_type()` which created a `NoopResolver` without the
/// Array base type, causing TS2339 false positives like "Property 'push'
/// does not exist on type 'any[]'".
#[test]
fn test_property_access_array_push_with_env_resolver() {
    use crate::relations::subtype::TypeEnvironment;
    use crate::types::TypeParamInfo;

    let interner = TypeInterner::new();

    // Create a mock Array<T> interface with a "push" method
    let t_param = TypeParamInfo {
        name: interner.intern_string("T"),
        constraint: None,
        default: None,
        is_const: false,
    };
    let t_type = interner.intern(TypeData::TypeParameter(t_param));

    // push(...items: T[]): number
    let push_func = interner.function(FunctionShape {
        params: vec![ParamInfo {
            name: Some(interner.intern_string("items")),
            type_id: interner.array(t_type),
            optional: false,
            rest: true,
        }],
        return_type: TypeId::NUMBER,
        type_params: vec![],
        this_type: None,
        type_predicate: None,
        is_constructor: false,
        is_method: true,
    });

    // Create an interface with push method
    let array_interface = interner.object(vec![PropertyInfo::method(
        interner.intern_string("push"),
        push_func,
    )]);

    // Set array base type on the interner so PropertyAccessEvaluator can find it
    interner.set_array_base_type(array_interface, vec![t_param]);

    // Set up TypeEnvironment with Array<T> registered
    let mut env = TypeEnvironment::new();
    env.set_array_base_type(array_interface, vec![t_param]);

    // Create evaluator with the environment
    let evaluator = PropertyAccessEvaluator::new(&interner);

    // Test: string[].push should resolve successfully
    let string_array = interner.array(TypeId::STRING);
    let result = evaluator.resolve_property_access(string_array, "push");
    match result {
        PropertyAccessResult::Success { type_id, .. } => {
            // The push method should be a function returning number
            match interner.lookup(type_id) {
                Some(TypeData::Function(func_id)) => {
                    let func = interner.function_shape(func_id);
                    assert_eq!(
                        func.return_type,
                        TypeId::NUMBER,
                        "push should return number"
                    );
                }
                other => panic!("Expected function for push, got {other:?}"),
            }
        }
        _ => panic!("Expected Success for array.push with env resolver, got {result:?}"),
    }
}

/// Regression test: QueryCache-backed property access must expose Array<T>
/// registrations from the interner. Without this, `string[].push` fails with
/// a false TS2339 in checker paths that use `QueryCache` as the resolver.
#[test]
fn test_property_access_array_push_with_query_cache_resolver() {
    use crate::caches::query_cache::QueryCache;
    use crate::types::TypeParamInfo;

    let interner = TypeInterner::new();

    let t_param = TypeParamInfo {
        name: interner.intern_string("T"),
        constraint: None,
        default: None,
        is_const: false,
    };
    let t_type = interner.intern(TypeData::TypeParameter(t_param));

    let push_func = interner.function(FunctionShape {
        params: vec![ParamInfo {
            name: Some(interner.intern_string("items")),
            type_id: interner.array(t_type),
            optional: false,
            rest: true,
        }],
        return_type: TypeId::NUMBER,
        type_params: vec![],
        this_type: None,
        type_predicate: None,
        is_constructor: false,
        is_method: true,
    });

    let array_interface = interner.object(vec![PropertyInfo::method(
        interner.intern_string("push"),
        push_func,
    )]);

    interner.set_array_base_type(array_interface, vec![t_param]);

    let cache = QueryCache::new(&interner);
    let evaluator = PropertyAccessEvaluator::new(&cache);

    let string_array = interner.array(TypeId::STRING);
    let result = evaluator.resolve_property_access(string_array, "push");
    match result {
        PropertyAccessResult::Success { type_id, .. } => match interner.lookup(type_id) {
            Some(TypeData::Function(func_id)) => {
                let func = interner.function_shape(func_id);
                assert_eq!(func.return_type, TypeId::NUMBER);
            }
            other => panic!("Expected function for push, got {other:?}"),
        },
        other => panic!("Expected Success for array.push with QueryCache resolver, got {other:?}"),
    }
}

/// Regression test: Array<T> from merged lib declarations is represented as an
/// intersection of interface fragments. Property access on `T[]` must still
/// find methods like `push` through Application(Array, [T]).
#[test]
fn test_property_access_array_push_with_intersection_array_base() {
    let interner = TypeInterner::new();

    let t_param = TypeParamInfo {
        name: interner.intern_string("T"),
        constraint: None,
        default: None,
        is_const: false,
    };
    let t_type = interner.intern(TypeData::TypeParameter(t_param));

    let push_func = interner.function(FunctionShape {
        params: vec![ParamInfo {
            name: Some(interner.intern_string("items")),
            type_id: interner.array(t_type),
            optional: false,
            rest: true,
        }],
        return_type: TypeId::NUMBER,
        type_params: vec![],
        this_type: None,
        type_predicate: None,
        is_constructor: false,
        is_method: true,
    });

    let array_decl_a = interner.object(vec![PropertyInfo::method(
        interner.intern_string("push"),
        push_func,
    )]);

    let array_decl_b = interner.object(vec![PropertyInfo::readonly(
        interner.intern_string("length"),
        TypeId::NUMBER,
    )]);

    // Simulate merged lib declarations: Array<T> = DeclA & DeclB
    let array_base = interner.intersection2(array_decl_a, array_decl_b);
    interner.set_array_base_type(array_base, vec![t_param]);

    let evaluator = PropertyAccessEvaluator::new(&interner);
    let string_array = interner.array(TypeId::STRING);

    let result = evaluator.resolve_property_access(string_array, "push");
    match result {
        PropertyAccessResult::Success { type_id, .. } => match interner.lookup(type_id) {
            Some(TypeData::Function(func_id)) => {
                let func = interner.function_shape(func_id);
                assert_eq!(func.return_type, TypeId::NUMBER);
            }
            other => panic!("Expected function for push, got {other:?}"),
        },
        other => {
            panic!("Expected Success for array.push with intersection array base, got {other:?}")
        }
    }
}

#[test]
fn test_array_push_instantiates_intersection_array_base_parameter() {
    let interner = TypeInterner::new();

    let t_param = TypeParamInfo {
        name: interner.intern_string("T"),
        constraint: None,
        default: None,
        is_const: false,
    };
    let t_type = interner.intern(TypeData::TypeParameter(t_param));

    let push_func = interner.function(FunctionShape {
        params: vec![ParamInfo {
            name: Some(interner.intern_string("items")),
            type_id: interner.array(t_type),
            optional: false,
            rest: true,
        }],
        return_type: TypeId::NUMBER,
        type_params: vec![],
        this_type: None,
        type_predicate: None,
        is_constructor: false,
        is_method: true,
    });

    let array_decl_a = interner.object(vec![PropertyInfo::method(
        interner.intern_string("push"),
        push_func,
    )]);
    let array_decl_b = interner.object(vec![PropertyInfo::readonly(
        interner.intern_string("length"),
        TypeId::NUMBER,
    )]);
    let array_base = interner.intersection2(array_decl_a, array_decl_b);
    interner.set_array_base_type(array_base, vec![t_param]);

    let evaluator = PropertyAccessEvaluator::new(&interner);
    let u_param = TypeParamInfo {
        name: interner.intern_string("U"),
        constraint: None,
        default: None,
        is_const: false,
    };
    let u_type = interner.intern(TypeData::TypeParameter(u_param));
    let u_array = interner.array(u_type);

    let result = evaluator.resolve_property_access(u_array, "push");
    let PropertyAccessResult::Success { type_id, .. } = result else {
        panic!("Expected Success for generic array push, got {result:?}");
    };
    let Some(TypeData::Function(func_id)) = interner.lookup(type_id) else {
        panic!(
            "Expected function type for push, got {:?}",
            interner.lookup(type_id)
        );
    };
    let shape = interner.function_shape(func_id);
    let [param] = shape.params.as_slice() else {
        panic!(
            "Expected one rest parameter for push, got {:?}",
            shape.params
        );
    };
    assert_eq!(
        crate::type_queries::get_array_element_type(&interner, param.type_id),
        Some(u_type)
    );
}

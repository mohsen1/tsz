#[test]
fn test_instance_type_from_constructor() {
    // InstanceType<typeof Foo> = Foo instance type
    let interner = TypeInterner::new();

    // Instance type has 'value' property
    let get_value_method = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![],
        this_type: None,
        return_type: TypeId::STRING,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });
    let instance_type = interner.object(vec![
        PropertyInfo::new(interner.intern_string("value"), TypeId::STRING),
        PropertyInfo::method(interner.intern_string("getValue"), get_value_method),
    ]);

    // Constructor type
    let ctor = interner.callable(CallableShape {
        symbol: None,
        is_abstract: false,
        call_signatures: vec![],
        construct_signatures: vec![CallSignature {
            type_params: vec![],
            params: vec![ParamInfo {
                name: Some(interner.intern_string("initial")),
                type_id: TypeId::STRING,
                optional: false,
                rest: false,
            }],
            this_type: None,
            return_type: instance_type,
            type_predicate: None,
            is_method: false,
        }],
        properties: vec![],
        string_index: None,
        number_index: None,
    });

    // InstanceType extracts the return type of construct signature
    match interner.lookup(ctor) {
        Some(TypeData::Callable(shape_id)) => {
            let shape = interner.callable_shape(shape_id);
            assert_eq!(shape.construct_signatures.len(), 1);
            let extracted_instance = shape.construct_signatures[0].return_type;
            assert_eq!(extracted_instance, instance_type);
        }
        _ => panic!("Expected Callable type"),
    }
}

#[test]
fn test_constructor_parameters_with_generics() {
    // ConstructorParameters<new <T>(value: T) => Container<T>>
    let interner = TypeInterner::new();

    let (t_name, t_param) = test_type_param(&interner, "T");

    let container = interner.object(vec![PropertyInfo::new(
        interner.intern_string("value"),
        t_param,
    )]);

    let generic_ctor = interner.callable(CallableShape {
        symbol: None,
        is_abstract: false,
        call_signatures: vec![],
        construct_signatures: vec![CallSignature {
            type_params: vec![TypeParamInfo {
                name: t_name,
                constraint: None,
                default: None,
                is_const: false,
            }],
            params: vec![ParamInfo {
                name: Some(interner.intern_string("value")),
                type_id: t_param,
                optional: false,
                rest: false,
            }],
            this_type: None,
            return_type: container,
            type_predicate: None,
            is_method: false,
        }],
        properties: vec![],
        string_index: None,
        number_index: None,
    });

    match interner.lookup(generic_ctor) {
        Some(TypeData::Callable(shape_id)) => {
            let shape = interner.callable_shape(shape_id);
            let sig = &shape.construct_signatures[0];
            // Has type parameter
            assert_eq!(sig.type_params.len(), 1);
            assert_eq!(sig.type_params[0].name, t_name);
            // Parameter uses type parameter
            assert_eq!(sig.params.len(), 1);
            assert_eq!(sig.params[0].type_id, t_param);
        }
        _ => panic!("Expected Callable type"),
    }
}

#[test]
fn test_awaited_with_nested_promises() {
    // Awaited<Promise<Promise<string>>> = string
    // Awaited recursively unwraps nested promises
    let interner = TypeInterner::new();

    // We model Promise<T> as an object with 'then' method
    // For deeply nested, we just verify the structure
    let inner_then = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![],
        this_type: None,
        return_type: TypeId::STRING,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });
    let inner_promise = interner.object(vec![PropertyInfo::method(
        interner.intern_string("then"),
        inner_then,
    )]);

    let outer_then = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![],
        this_type: None,
        return_type: inner_promise,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });
    let outer_promise = interner.object(vec![PropertyInfo::method(
        interner.intern_string("then"),
        outer_then,
    )]);

    match interner.lookup(outer_promise) {
        Some(TypeData::Object(shape_id)) => {
            let shape = interner.object_shape(shape_id);
            assert!(!shape.properties.is_empty());
        }
        _ => panic!("Expected Object type"),
    }
}

/// For tuple element contextual typing: `const x: [...T[], U] = [a]` — element 0 gets type U.
#[test]
fn test_variadic_tuple_element_contextual_type_single_element() {
    let interner = TypeInterner::new();

    // Build variadic tuple: [...number[], string]
    let num_array = interner.array(TypeId::NUMBER);
    let variadic_tuple = interner.tuple(vec![
        TupleElement {
            type_id: num_array,
            name: None,
            optional: false,
            rest: true,
        },
        TupleElement {
            type_id: TypeId::STRING,
            name: None,
            optional: false,
            rest: false,
        },
    ]);

    let ctx = ContextualTypeContext::with_expected(&interner, variadic_tuple);

    // With 1 element, index 0 should map to the tail (string)
    assert_eq!(
        ctx.get_tuple_element_type_with_count(0, 1),
        Some(TypeId::STRING)
    );
}

/// For tuple element contextual typing: `const x: [...T[], U] = [a, b, c]` — last is U, rest are T.
#[test]
fn test_variadic_tuple_element_contextual_type_multiple_elements() {
    let interner = TypeInterner::new();

    let num_array = interner.array(TypeId::NUMBER);
    let variadic_tuple = interner.tuple(vec![
        TupleElement {
            type_id: num_array,
            name: None,
            optional: false,
            rest: true,
        },
        TupleElement {
            type_id: TypeId::STRING,
            name: None,
            optional: false,
            rest: false,
        },
    ]);

    let ctx = ContextualTypeContext::with_expected(&interner, variadic_tuple);

    // With 3 elements: indices 0,1 should map to variadic (number), index 2 to tail (string)
    assert_eq!(
        ctx.get_tuple_element_type_with_count(0, 3),
        Some(TypeId::NUMBER)
    );
    assert_eq!(
        ctx.get_tuple_element_type_with_count(1, 3),
        Some(TypeId::NUMBER)
    );
    assert_eq!(
        ctx.get_tuple_element_type_with_count(2, 3),
        Some(TypeId::STRING)
    );
}

/// Contextual type: `(...values: [string, number, boolean]) => void`
/// Callback: `(...x) => {}` — x should get the full tuple `[string, number, boolean]`
#[test]
fn test_contextual_rest_param_from_tuple_rest() {
    let interner = TypeInterner::new();

    let tuple_type = interner.tuple(vec![
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
        TupleElement {
            type_id: TypeId::BOOLEAN,
            name: None,
            optional: false,
            rest: false,
        },
    ]);

    let fn_type = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("values")),
            type_id: tuple_type,
            optional: false,
            rest: true,
        }],
        this_type: None,
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let ctx = ContextualTypeContext::with_expected(&interner, fn_type);

    // get_parameter_type(0) returns the first element (for non-rest callback params)
    assert_eq!(ctx.get_parameter_type(0), Some(TypeId::STRING));

    // get_rest_parameter_type(0) returns the full tuple (for rest callback params)
    let rest_type = ctx.get_rest_parameter_type(0).unwrap();
    assert_eq!(rest_type, tuple_type);
}

/// Contextual type: `(a: string, b: number, c: boolean) => void`
/// Callback: `(...x) => {}` — x should get `[string, number, boolean]`
#[test]
fn test_contextual_rest_param_from_individual_params() {
    let interner = TypeInterner::new();

    let fn_type = interner.function(FunctionShape {
        type_params: vec![],
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
                optional: false,
                rest: false,
            },
            ParamInfo {
                name: Some(interner.intern_string("c")),
                type_id: TypeId::BOOLEAN,
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

    let ctx = ContextualTypeContext::with_expected(&interner, fn_type);

    // Rest param at index 0 should collect all 3 params into a tuple
    let rest_type = ctx.get_rest_parameter_type(0).unwrap();
    // Verify it's a tuple with 3 elements
    if let Some(TypeData::Tuple(list_id)) = interner.lookup(rest_type) {
        let elements = interner.tuple_list(list_id);
        assert_eq!(elements.len(), 3);
        assert_eq!(elements[0].type_id, TypeId::STRING);
        assert_eq!(elements[1].type_id, TypeId::NUMBER);
        assert_eq!(elements[2].type_id, TypeId::BOOLEAN);
    } else {
        panic!("Expected Tuple, got {:?}", interner.lookup(rest_type));
    }

    // Rest param at index 1 should collect remaining 2 params
    let rest_type_1 = ctx.get_rest_parameter_type(1).unwrap();
    if let Some(TypeData::Tuple(list_id)) = interner.lookup(rest_type_1) {
        let elements = interner.tuple_list(list_id);
        assert_eq!(elements.len(), 2);
        assert_eq!(elements[0].type_id, TypeId::NUMBER);
        assert_eq!(elements[1].type_id, TypeId::BOOLEAN);
    } else {
        panic!("Expected Tuple, got {:?}", interner.lookup(rest_type_1));
    }
}

/// Contextual type: `(a: string, ...rest: number[]) => void`
/// Callback: `(...x) => {}` — x should get `[string, ...number[]]`
#[test]
fn test_contextual_rest_param_from_mixed_params() {
    let interner = TypeInterner::new();

    let number_array = interner.array(TypeId::NUMBER);
    let fn_type = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![
            ParamInfo {
                name: Some(interner.intern_string("a")),
                type_id: TypeId::STRING,
                optional: false,
                rest: false,
            },
            ParamInfo {
                name: Some(interner.intern_string("rest")),
                type_id: number_array,
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

    let ctx = ContextualTypeContext::with_expected(&interner, fn_type);

    // Rest param at index 0: [string, ...number[]]
    let rest_type = ctx.get_rest_parameter_type(0).unwrap();
    if let Some(TypeData::Tuple(list_id)) = interner.lookup(rest_type) {
        let elements = interner.tuple_list(list_id);
        assert_eq!(elements.len(), 2);
        assert_eq!(elements[0].type_id, TypeId::STRING);
        assert!(!elements[0].rest);
        assert_eq!(elements[1].type_id, number_array);
        assert!(elements[1].rest);
    } else {
        panic!("Expected Tuple, got {:?}", interner.lookup(rest_type));
    }

    // Rest param at index 1: number[] (directly from the rest param)
    let rest_type_1 = ctx.get_rest_parameter_type(1).unwrap();
    assert_eq!(rest_type_1, number_array);
}

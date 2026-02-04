//! Tests for callable type (overloaded signatures) subtype checking.

use super::*;
// =============================================================================
// Callable Subtype Tests
// =============================================================================

#[test]
fn test_callable_same_signature() {
    let interner = TypeInterner::new();

    // { (x: string): number } <: { (x: string): number }
    let sig = CallSignature {
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
        is_method: false,
    };

    let source = interner.callable(CallableShape {
        symbol: None,
        call_signatures: vec![sig.clone()],
        construct_signatures: vec![],
        properties: vec![],
        ..Default::default()
    });

    let target = interner.callable(CallableShape {
        symbol: None,
        call_signatures: vec![sig],
        construct_signatures: vec![],
        properties: vec![],
        ..Default::default()
    });

    assert!(is_subtype_of(&interner, source, target));
}

#[test]
fn test_callable_more_overloads() {
    let interner = TypeInterner::new();

    // { (x: string): number; (x: number): string } <: { (x: string): number }
    let sig1 = CallSignature {
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
        is_method: false,
    };

    let sig2 = CallSignature {
        type_params: vec![],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("x")),
            type_id: TypeId::NUMBER,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: TypeId::STRING,
        type_predicate: None,
        is_method: false,
    };

    let source = interner.callable(CallableShape {
        symbol: None,
        call_signatures: vec![sig1.clone(), sig2],
        construct_signatures: vec![],
        properties: vec![],
        ..Default::default()
    });

    let target = interner.callable(CallableShape {
        symbol: None,
        call_signatures: vec![sig1],
        construct_signatures: vec![],
        properties: vec![],
        ..Default::default()
    });

    assert!(is_subtype_of(&interner, source, target));
}

#[test]
fn test_callable_missing_overload() {
    let interner = TypeInterner::new();

    // { (x: string): number } NOT <: { (x: string): number; (x: number): string }
    let sig1 = CallSignature {
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
        is_method: false,
    };

    let sig2 = CallSignature {
        type_params: vec![],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("x")),
            type_id: TypeId::NUMBER,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: TypeId::STRING,
        type_predicate: None,
        is_method: false,
    };

    let source = interner.callable(CallableShape {
        symbol: None,
        call_signatures: vec![sig1.clone()],
        construct_signatures: vec![],
        properties: vec![],
        ..Default::default()
    });

    let target = interner.callable(CallableShape {
        symbol: None,
        call_signatures: vec![sig1, sig2],
        construct_signatures: vec![],
        properties: vec![],
        ..Default::default()
    });

    assert!(!is_subtype_of(&interner, source, target));
}

#[test]
fn test_callable_with_construct() {
    let interner = TypeInterner::new();

    // { new(): Foo } <: { new(): Foo }
    let obj_type = interner.object(vec![PropertyInfo {
        name: interner.intern_string("x"),
        type_id: TypeId::NUMBER,
        write_type: TypeId::NUMBER,
        optional: false,
        readonly: false,
        is_method: false,
    }]);

    let sig = CallSignature {
        type_params: vec![],
        params: vec![],
        this_type: None,
        return_type: obj_type,
        type_predicate: None,
        is_method: false,
    };

    let source = interner.callable(CallableShape {
        symbol: None,
        call_signatures: vec![],
        construct_signatures: vec![sig.clone()],
        properties: vec![],
        ..Default::default()
    });

    let target = interner.callable(CallableShape {
        symbol: None,
        call_signatures: vec![],
        construct_signatures: vec![sig],
        properties: vec![],
        ..Default::default()
    });

    assert!(is_subtype_of(&interner, source, target));
}

#[test]
fn test_callable_covariant_return() {
    let interner = TypeInterner::new();

    // { (): "hello" } <: { (): string }
    let hello = interner.literal_string("hello");

    let source_sig = CallSignature {
        type_params: vec![],
        params: vec![],
        this_type: None,
        return_type: hello,
        type_predicate: None,
        is_method: false,
    };

    let target_sig = CallSignature {
        type_params: vec![],
        params: vec![],
        this_type: None,
        return_type: TypeId::STRING,
        type_predicate: None,
        is_method: false,
    };

    let source = interner.callable(CallableShape {
        symbol: None,
        call_signatures: vec![source_sig],
        construct_signatures: vec![],
        properties: vec![],
        ..Default::default()
    });

    let target = interner.callable(CallableShape {
        symbol: None,
        call_signatures: vec![target_sig],
        construct_signatures: vec![],
        properties: vec![],
        ..Default::default()
    });

    assert!(is_subtype_of(&interner, source, target));
}

#[test]
fn test_function_to_callable() {
    let interner = TypeInterner::new();

    // (x: string) => number <: { (x: string): number }
    let fn_type = interner.function(FunctionShape {
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

    let callable = interner.callable(CallableShape {
        symbol: None,
        call_signatures: vec![CallSignature {
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
            is_method: false,
        }],
        construct_signatures: vec![],
        properties: vec![],
        ..Default::default()
    });

    assert!(is_subtype_of(&interner, fn_type, callable));
}

#[test]
fn test_callable_to_function() {
    let interner = TypeInterner::new();

    // { (x: string): number } <: (x: string) => number
    // At least one signature must match
    let callable = interner.callable(CallableShape {
        symbol: None,
        call_signatures: vec![CallSignature {
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
            is_method: false,
        }],
        construct_signatures: vec![],
        properties: vec![],
        ..Default::default()
    });

    let fn_type = interner.function(FunctionShape {
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

    assert!(is_subtype_of(&interner, callable, fn_type));
}

#[test]
fn test_callable_with_properties() {
    let interner = TypeInterner::new();

    // { (): void; length: number } <: { (): void; length: number }
    let source = interner.callable(CallableShape {
        symbol: None,
        call_signatures: vec![CallSignature {
            type_params: vec![],
            params: vec![],
            this_type: None,
            return_type: TypeId::VOID,
            type_predicate: None,
            is_method: false,
        }],
        construct_signatures: vec![],
        properties: vec![PropertyInfo {
            name: interner.intern_string("length"),
            type_id: TypeId::NUMBER,
            write_type: TypeId::NUMBER,
            optional: false,
            readonly: false,
            is_method: false,
        }],
        ..Default::default()
    });

    let target = interner.callable(CallableShape {
        symbol: None,
        call_signatures: vec![CallSignature {
            type_params: vec![],
            params: vec![],
            this_type: None,
            return_type: TypeId::VOID,
            type_predicate: None,
            is_method: false,
        }],
        construct_signatures: vec![],
        properties: vec![PropertyInfo {
            name: interner.intern_string("length"),
            type_id: TypeId::NUMBER,
            write_type: TypeId::NUMBER,
            optional: false,
            readonly: false,
            is_method: false,
        }],
        ..Default::default()
    });

    assert!(is_subtype_of(&interner, source, target));
}

#[test]
fn test_callable_missing_property() {
    let interner = TypeInterner::new();

    // { (): void } NOT <: { (): void; length: number }
    let source = interner.callable(CallableShape {
        symbol: None,
        call_signatures: vec![CallSignature {
            type_params: vec![],
            params: vec![],
            this_type: None,
            return_type: TypeId::VOID,
            type_predicate: None,
            is_method: false,
        }],
        construct_signatures: vec![],
        properties: vec![],
        ..Default::default()
    });

    let target = interner.callable(CallableShape {
        symbol: None,
        call_signatures: vec![CallSignature {
            type_params: vec![],
            params: vec![],
            this_type: None,
            return_type: TypeId::VOID,
            type_predicate: None,
            is_method: false,
        }],
        construct_signatures: vec![],
        properties: vec![PropertyInfo {
            name: interner.intern_string("length"),
            type_id: TypeId::NUMBER,
            write_type: TypeId::NUMBER,
            optional: false,
            readonly: false,
            is_method: false,
        }],
        ..Default::default()
    });

    assert!(!is_subtype_of(&interner, source, target));
}

// =============================================================================
// Overload Signature Matching Tests
// =============================================================================

#[test]
fn test_overload_signature_exact_match() {
    // Test: Selecting exact matching overload from multiple signatures
    let interner = TypeInterner::new();

    let sig_string_to_number = CallSignature {
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
        is_method: false,
    };

    let sig_number_to_string = CallSignature {
        type_params: vec![],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("x")),
            type_id: TypeId::NUMBER,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: TypeId::STRING,
        type_predicate: None,
        is_method: false,
    };

    let overloaded = interner.callable(CallableShape {
        symbol: None,
        call_signatures: vec![sig_string_to_number.clone(), sig_number_to_string],
        construct_signatures: vec![],
        properties: vec![],
        ..Default::default()
    });

    let string_only = interner.callable(CallableShape {
        symbol: None,
        call_signatures: vec![sig_string_to_number],
        construct_signatures: vec![],
        properties: vec![],
        ..Default::default()
    });

    assert!(is_subtype_of(&interner, overloaded, string_only));
}

#[test]
fn test_overload_signature_order_priority() {
    // Test: Earlier overload takes priority for matching
    let interner = TypeInterner::new();

    let special_lit = interner.literal_string("special");
    let special_return = interner.literal_string("matched-special");
    let sig_special = CallSignature {
        type_params: vec![],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("x")),
            type_id: special_lit,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: special_return,
        type_predicate: None,
        is_method: false,
    };

    let general_return = interner.literal_string("matched-general");
    let sig_general = CallSignature {
        type_params: vec![],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("x")),
            type_id: TypeId::STRING,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: general_return,
        type_predicate: None,
        is_method: false,
    };

    let overloaded = interner.callable(CallableShape {
        symbol: None,
        call_signatures: vec![sig_special.clone(), sig_general],
        construct_signatures: vec![],
        properties: vec![],
        ..Default::default()
    });

    let specific = interner.callable(CallableShape {
        symbol: None,
        call_signatures: vec![sig_special],
        construct_signatures: vec![],
        properties: vec![],
        ..Default::default()
    });

    assert!(is_subtype_of(&interner, overloaded, specific));
}

#[test]
fn test_overload_multiple_arities() {
    // Test: Overloads with different parameter counts
    let interner = TypeInterner::new();

    let sig_0 = CallSignature {
        type_params: vec![],
        params: vec![],
        this_type: None,
        return_type: TypeId::VOID,
        type_predicate: None,
        is_method: false,
    };

    let sig_1 = CallSignature {
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
        is_method: false,
    };

    let sig_2 = CallSignature {
        type_params: vec![],
        params: vec![
            ParamInfo {
                name: Some(interner.intern_string("x")),
                type_id: TypeId::STRING,
                optional: false,
                rest: false,
            },
            ParamInfo {
                name: Some(interner.intern_string("y")),
                type_id: TypeId::NUMBER,
                optional: false,
                rest: false,
            },
        ],
        this_type: None,
        return_type: TypeId::BOOLEAN,
        type_predicate: None,
        is_method: false,
    };

    let overloaded = interner.callable(CallableShape {
        symbol: None,
        call_signatures: vec![sig_0.clone(), sig_1.clone(), sig_2.clone()],
        construct_signatures: vec![],
        properties: vec![],
        ..Default::default()
    });

    let only_sig0 = interner.callable(CallableShape {
        symbol: None,
        call_signatures: vec![sig_0],
        construct_signatures: vec![],
        properties: vec![],
        ..Default::default()
    });
    let only_sig1 = interner.callable(CallableShape {
        symbol: None,
        call_signatures: vec![sig_1],
        construct_signatures: vec![],
        properties: vec![],
        ..Default::default()
    });
    let only_sig2 = interner.callable(CallableShape {
        symbol: None,
        call_signatures: vec![sig_2],
        construct_signatures: vec![],
        properties: vec![],
        ..Default::default()
    });

    assert!(is_subtype_of(&interner, overloaded, only_sig0));
    assert!(is_subtype_of(&interner, overloaded, only_sig1));
    assert!(is_subtype_of(&interner, overloaded, only_sig2));
}

// =============================================================================
// Generic Overload Inference Tests
// =============================================================================

#[test]
fn test_generic_overload_simple() {
    // Test: Generic overload with type parameter <T>(x: T): T
    // Verify the generic callable is correctly structured
    let interner = TypeInterner::new();

    let t_name = interner.intern_string("T");
    let t_param = interner.intern(TypeKey::TypeParameter(TypeParamInfo {
        name: t_name,
        constraint: None,
        is_const: false,
        default: None,
        
    }));

    let generic_sig = CallSignature {
        type_params: vec![TypeParamInfo {
            name: t_name,
            constraint: None,
            is_const: false,
            default: None,
            
        }],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("x")),
            type_id: t_param,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: t_param,
        type_predicate: None,
        is_method: false,
    };

    let generic_fn = interner.callable(CallableShape {
        symbol: None,
        call_signatures: vec![generic_sig],
        construct_signatures: vec![],
        properties: vec![],
        ..Default::default()
    });

    // Verify the callable was created with proper type parameter
    let key = interner.lookup(generic_fn).expect("Should have callable");
    match key {
        TypeKey::Callable(shape_id) => {
            let shape = interner.callable_shape(shape_id);
            assert_eq!(shape.call_signatures.len(), 1);
            assert_eq!(shape.call_signatures[0].type_params.len(), 1);
            assert_eq!(shape.call_signatures[0].params.len(), 1);
            // Return type should be the same as param type (T)
            assert_eq!(shape.call_signatures[0].return_type, t_param);
        }
        _ => panic!("Expected callable type"),
    }
}

#[test]
fn test_generic_overload_with_constraint() {
    // Test: <T extends object>(x: T): keyof T
    let interner = TypeInterner::new();

    let t_name = interner.intern_string("T");
    let t_param = interner.intern(TypeKey::TypeParameter(TypeParamInfo {
        name: t_name,
        constraint: Some(TypeId::OBJECT),
        default: None,
        is_const: false,
    }));

    let keyof_t = interner.intern(TypeKey::KeyOf(t_param));

    let constrained_sig = CallSignature {
        type_params: vec![TypeParamInfo {
            name: t_name,
            constraint: Some(TypeId::OBJECT),
            default: None,
        is_const: false,
        }],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("x")),
            type_id: t_param,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: keyof_t,
        type_predicate: None,
        is_method: false,
    };

    let generic_fn = interner.callable(CallableShape {
        symbol: None,
        call_signatures: vec![constrained_sig],
        construct_signatures: vec![],
        properties: vec![],
        ..Default::default()
    });

    let key = interner.lookup(generic_fn).expect("Should have callable");
    match key {
        TypeKey::Callable(shape_id) => {
            let shape = interner.callable_shape(shape_id);
            assert_eq!(shape.call_signatures.len(), 1);
            assert!(shape.call_signatures[0].type_params[0].constraint.is_some());
        }
        _ => panic!("Expected callable type"),
    }
}

#[test]
fn test_generic_overload_multiple_type_params() {
    // Test: <T, U>(x: T, y: U): [T, U]
    let interner = TypeInterner::new();

    let t_name = interner.intern_string("T");
    let u_name = interner.intern_string("U");

    let t_param = interner.intern(TypeKey::TypeParameter(TypeParamInfo {
        name: t_name,
        constraint: None,
        is_const: false,
        default: None,
        
    }));
    let u_param = interner.intern(TypeKey::TypeParameter(TypeParamInfo {
        name: u_name,
        constraint: None,
        is_const: false,
        default: None,
        
    }));

    let tuple_return = interner.tuple(vec![
        TupleElement {
            type_id: t_param,
            name: None,
            optional: false,
            rest: false,
        },
        TupleElement {
            type_id: u_param,
            name: None,
            optional: false,
            rest: false,
        },
    ]);

    let multi_param_sig = CallSignature {
        type_params: vec![
            TypeParamInfo {
                name: t_name,
                constraint: None,
                is_const: false,
                default: None,
                
            },
            TypeParamInfo {
                name: u_name,
                constraint: None,
                is_const: false,
                default: None,
                
            },
        ],
        params: vec![
            ParamInfo {
                name: Some(interner.intern_string("x")),
                type_id: t_param,
                optional: false,
                rest: false,
            },
            ParamInfo {
                name: Some(interner.intern_string("y")),
                type_id: u_param,
                optional: false,
                rest: false,
            },
        ],
        this_type: None,
        return_type: tuple_return,
        type_predicate: None,
        is_method: false,
    };

    let generic_fn = interner.callable(CallableShape {
        symbol: None,
        call_signatures: vec![multi_param_sig],
        construct_signatures: vec![],
        properties: vec![],
        ..Default::default()
    });

    let key = interner.lookup(generic_fn).expect("Should have callable");
    match key {
        TypeKey::Callable(shape_id) => {
            let shape = interner.callable_shape(shape_id);
            assert_eq!(shape.call_signatures[0].type_params.len(), 2);
            assert_eq!(shape.call_signatures[0].params.len(), 2);
        }
        _ => panic!("Expected callable type"),
    }
}

// =============================================================================
// Optional Parameter Overload Resolution Tests
// =============================================================================

#[test]
fn test_optional_param_overload_matching() {
    // Test: fn(x: string): number; fn(x: string, y?: number): string;
    let interner = TypeInterner::new();

    let sig_required = CallSignature {
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
        is_method: false,
    };

    let sig_optional = CallSignature {
        type_params: vec![],
        params: vec![
            ParamInfo {
                name: Some(interner.intern_string("x")),
                type_id: TypeId::STRING,
                optional: false,
                rest: false,
            },
            ParamInfo {
                name: Some(interner.intern_string("y")),
                type_id: TypeId::NUMBER,
                optional: true,
                rest: false,
            },
        ],
        this_type: None,
        return_type: TypeId::STRING,
        type_predicate: None,
        is_method: false,
    };

    let overloaded = interner.callable(CallableShape {
        symbol: None,
        call_signatures: vec![sig_required.clone(), sig_optional.clone()],
        construct_signatures: vec![],
        properties: vec![],
        ..Default::default()
    });

    let only_required = interner.callable(CallableShape {
        symbol: None,
        call_signatures: vec![sig_required],
        construct_signatures: vec![],
        properties: vec![],
        ..Default::default()
    });
    let only_optional = interner.callable(CallableShape {
        symbol: None,
        call_signatures: vec![sig_optional],
        construct_signatures: vec![],
        properties: vec![],
        ..Default::default()
    });

    assert!(is_subtype_of(&interner, overloaded, only_required));
    assert!(is_subtype_of(&interner, overloaded, only_optional));
}

#[test]
fn test_all_optional_params_overload() {
    // Test: fn(x?: string, y?: number): void
    let interner = TypeInterner::new();

    let all_optional_sig = CallSignature {
        type_params: vec![],
        params: vec![
            ParamInfo {
                name: Some(interner.intern_string("x")),
                type_id: TypeId::STRING,
                optional: true,
                rest: false,
            },
            ParamInfo {
                name: Some(interner.intern_string("y")),
                type_id: TypeId::NUMBER,
                optional: true,
                rest: false,
            },
        ],
        this_type: None,
        return_type: TypeId::VOID,
        type_predicate: None,
        is_method: false,
    };

    let fn_with_optional = interner.callable(CallableShape {
        symbol: None,
        call_signatures: vec![all_optional_sig],
        construct_signatures: vec![],
        properties: vec![],
        ..Default::default()
    });

    let no_params_sig = CallSignature {
        type_params: vec![],
        params: vec![],
        this_type: None,
        return_type: TypeId::VOID,
        type_predicate: None,
        is_method: false,
    };

    let no_params = interner.callable(CallableShape {
        symbol: None,
        call_signatures: vec![no_params_sig],
        construct_signatures: vec![],
        properties: vec![],
        ..Default::default()
    });

    // () => void is subtype of (x?: string, y?: number) => void
    assert!(is_subtype_of(&interner, no_params, fn_with_optional));
}

#[test]
fn test_optional_and_rest_param_overload() {
    // Test: fn(x: string, ...rest: number[]): void
    let interner = TypeInterner::new();

    let number_array = interner.array(TypeId::NUMBER);

    let rest_sig = CallSignature {
        type_params: vec![],
        params: vec![
            ParamInfo {
                name: Some(interner.intern_string("x")),
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
        is_method: false,
    };

    let fn_with_rest = interner.callable(CallableShape {
        symbol: None,
        call_signatures: vec![rest_sig],
        construct_signatures: vec![],
        properties: vec![],
        ..Default::default()
    });

    let single_param_sig = CallSignature {
        type_params: vec![],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("x")),
            type_id: TypeId::STRING,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: TypeId::VOID,
        type_predicate: None,
        is_method: false,
    };

    let single_param = interner.callable(CallableShape {
        symbol: None,
        call_signatures: vec![single_param_sig],
        construct_signatures: vec![],
        properties: vec![],
        ..Default::default()
    });

    assert!(is_subtype_of(&interner, single_param, fn_with_rest));
}

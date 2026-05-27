#[test]
fn test_overload_basic_two_signatures() {
    // interface Overloaded {
    //   (x: string): number;
    //   (x: number): string;
    // }
    let interner = TypeInterner::new();

    let callable = interner.callable(CallableShape {
        symbol: None,
        is_abstract: false,
        call_signatures: vec![
            CallSignature {
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
            },
            CallSignature {
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
            },
        ],
        construct_signatures: vec![],
        properties: vec![],
        string_index: None,
        number_index: None,
    });

    assert!(callable != TypeId::ERROR);
}

#[test]
fn test_overload_by_argument_count() {
    // interface ByCount {
    //   (): void;
    //   (x: number): number;
    //   (x: number, y: number): number;
    // }
    let interner = TypeInterner::new();

    let callable = interner.callable(CallableShape {
        symbol: None,
        is_abstract: false,
        call_signatures: vec![
            CallSignature {
                type_params: vec![],
                params: vec![],
                this_type: None,
                return_type: TypeId::VOID,
                type_predicate: None,
                is_method: false,
            },
            CallSignature {
                type_params: vec![],
                params: vec![ParamInfo {
                    name: Some(interner.intern_string("x")),
                    type_id: TypeId::NUMBER,
                    optional: false,
                    rest: false,
                }],
                this_type: None,
                return_type: TypeId::NUMBER,
                type_predicate: None,
                is_method: false,
            },
            CallSignature {
                type_params: vec![],
                params: vec![
                    ParamInfo {
                        name: Some(interner.intern_string("x")),
                        type_id: TypeId::NUMBER,
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
                return_type: TypeId::NUMBER,
                type_predicate: None,
                is_method: false,
            },
        ],
        construct_signatures: vec![],
        properties: vec![],
        string_index: None,
        number_index: None,
    });

    assert!(callable != TypeId::ERROR);
}

#[test]
fn test_overload_subtype_more_signatures_to_fewer() {
    // More overloads is subtype of fewer (if matching signatures exist)
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    // Two signatures: (string) => number, (number) => string
    let more_overloads = interner.callable(CallableShape {
        symbol: None,
        is_abstract: false,
        call_signatures: vec![
            CallSignature {
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
            },
            CallSignature {
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
            },
        ],
        construct_signatures: vec![],
        properties: vec![],
        string_index: None,
        number_index: None,
    });

    // One signature: (string) => number
    let fewer_overloads = interner.callable(CallableShape {
        symbol: None,
        is_abstract: false,
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
        string_index: None,
        number_index: None,
    });

    // More overloads should be subtype of fewer (can be used anywhere fewer is expected)
    assert!(checker.is_subtype_of(more_overloads, fewer_overloads));
}

#[test]
fn test_overload_subtype_fewer_not_subtype_of_more() {
    // Fewer overloads is NOT subtype of more (missing capability)
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    // Two signatures
    let more_overloads = interner.callable(CallableShape {
        symbol: None,
        is_abstract: false,
        call_signatures: vec![
            CallSignature {
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
            },
            CallSignature {
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
            },
        ],
        construct_signatures: vec![],
        properties: vec![],
        string_index: None,
        number_index: None,
    });

    // One signature only
    let fewer_overloads = interner.callable(CallableShape {
        symbol: None,
        is_abstract: false,
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
        string_index: None,
        number_index: None,
    });

    // Fewer cannot substitute for more - missing the (number) => string overload
    assert!(!checker.is_subtype_of(fewer_overloads, more_overloads));
}

#[test]
fn test_overload_generic_identity() {
    // interface GenericOverload {
    //   <T>(x: T): T;
    //   (x: string): string;
    // }
    let interner = TypeInterner::new();

    let t_param = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: interner.intern_string("T"),
        constraint: None,
        default: None,
        is_const: false,
    }));

    let callable = interner.callable(CallableShape {
        symbol: None,
        is_abstract: false,
        call_signatures: vec![
            CallSignature {
                type_params: vec![TypeParamInfo {
                    name: interner.intern_string("T"),
                    constraint: None,
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
                return_type: t_param,
                type_predicate: None,
                is_method: false,
            },
            CallSignature {
                type_params: vec![],
                params: vec![ParamInfo {
                    name: Some(interner.intern_string("x")),
                    type_id: TypeId::STRING,
                    optional: false,
                    rest: false,
                }],
                this_type: None,
                return_type: TypeId::STRING,
                type_predicate: None,
                is_method: false,
            },
        ],
        construct_signatures: vec![],
        properties: vec![],
        string_index: None,
        number_index: None,
    });

    assert!(callable != TypeId::ERROR);
}

#[test]
fn test_overload_generic_with_constraint() {
    // interface ConstrainedOverload {
    //   <T extends string>(x: T): T;
    //   <T extends number>(x: T): T;
    // }
    let interner = TypeInterner::new();

    let t_string = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: interner.intern_string("T"),
        constraint: Some(TypeId::STRING),
        default: None,
        is_const: false,
    }));

    let t_number = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: interner.intern_string("T"),
        constraint: Some(TypeId::NUMBER),
        default: None,
        is_const: false,
    }));

    let callable = interner.callable(CallableShape {
        symbol: None,
        is_abstract: false,
        call_signatures: vec![
            CallSignature {
                type_params: vec![TypeParamInfo {
                    name: interner.intern_string("T"),
                    constraint: Some(TypeId::STRING),
                    default: None,
                    is_const: false,
                }],
                params: vec![ParamInfo {
                    name: Some(interner.intern_string("x")),
                    type_id: t_string,
                    optional: false,
                    rest: false,
                }],
                this_type: None,
                return_type: t_string,
                type_predicate: None,
                is_method: false,
            },
            CallSignature {
                type_params: vec![TypeParamInfo {
                    name: interner.intern_string("T"),
                    constraint: Some(TypeId::NUMBER),
                    default: None,
                    is_const: false,
                }],
                params: vec![ParamInfo {
                    name: Some(interner.intern_string("x")),
                    type_id: t_number,
                    optional: false,
                    rest: false,
                }],
                this_type: None,
                return_type: t_number,
                type_predicate: None,
                is_method: false,
            },
        ],
        construct_signatures: vec![],
        properties: vec![],
        string_index: None,
        number_index: None,
    });

    assert!(callable != TypeId::ERROR);
}

#[test]
fn test_overload_with_rest_parameter() {
    // interface WithRest {
    //   (x: number): number;
    //   (...args: number[]): number;
    // }
    let interner = TypeInterner::new();

    let number_array = interner.array(TypeId::NUMBER);

    let callable = interner.callable(CallableShape {
        symbol: None,
        is_abstract: false,
        call_signatures: vec![
            CallSignature {
                type_params: vec![],
                params: vec![ParamInfo {
                    name: Some(interner.intern_string("x")),
                    type_id: TypeId::NUMBER,
                    optional: false,
                    rest: false,
                }],
                this_type: None,
                return_type: TypeId::NUMBER,
                type_predicate: None,
                is_method: false,
            },
            CallSignature {
                type_params: vec![],
                params: vec![ParamInfo {
                    name: Some(interner.intern_string("args")),
                    type_id: number_array,
                    optional: false,
                    rest: true,
                }],
                this_type: None,
                return_type: TypeId::NUMBER,
                type_predicate: None,
                is_method: false,
            },
        ],
        construct_signatures: vec![],
        properties: vec![],
        string_index: None,
        number_index: None,
    });

    assert!(callable != TypeId::ERROR);
}

#[test]
fn test_overload_with_optional_parameters() {
    // interface WithOptional {
    //   (x: string): string;
    //   (x: string, y?: number): string;
    // }
    let interner = TypeInterner::new();

    let callable = interner.callable(CallableShape {
        symbol: None,
        is_abstract: false,
        call_signatures: vec![
            CallSignature {
                type_params: vec![],
                params: vec![ParamInfo {
                    name: Some(interner.intern_string("x")),
                    type_id: TypeId::STRING,
                    optional: false,
                    rest: false,
                }],
                this_type: None,
                return_type: TypeId::STRING,
                type_predicate: None,
                is_method: false,
            },
            CallSignature {
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
            },
        ],
        construct_signatures: vec![],
        properties: vec![],
        string_index: None,
        number_index: None,
    });

    assert!(callable != TypeId::ERROR);
}

#[test]
fn test_overload_mixed_call_and_construct() {
    // interface MixedCallable {
    //   (x: string): string;
    //   new (x: number): object;
    // }
    let interner = TypeInterner::new();

    let callable = interner.callable(CallableShape {
        symbol: None,
        is_abstract: false,
        call_signatures: vec![CallSignature {
            type_params: vec![],
            params: vec![ParamInfo {
                name: Some(interner.intern_string("x")),
                type_id: TypeId::STRING,
                optional: false,
                rest: false,
            }],
            this_type: None,
            return_type: TypeId::STRING,
            type_predicate: None,
            is_method: false,
        }],
        construct_signatures: vec![CallSignature {
            type_params: vec![],
            params: vec![ParamInfo {
                name: Some(interner.intern_string("x")),
                type_id: TypeId::NUMBER,
                optional: false,
                rest: false,
            }],
            this_type: None,
            return_type: TypeId::OBJECT,
            type_predicate: None,
            is_method: false,
        }],
        properties: vec![],
        string_index: None,
        number_index: None,
    });

    assert!(callable != TypeId::ERROR);
}

#[test]
fn test_overload_return_type_union() {
    // interface UnionReturn {
    //   (x: "a"): number;
    //   (x: "b"): string;
    //   (x: string): number | string;
    // }
    let interner = TypeInterner::new();

    let lit_a = interner.literal_string("a");
    let lit_b = interner.literal_string("b");
    let num_or_string = interner.union(vec![TypeId::NUMBER, TypeId::STRING]);

    let callable = interner.callable(CallableShape {
        symbol: None,
        is_abstract: false,
        call_signatures: vec![
            CallSignature {
                type_params: vec![],
                params: vec![ParamInfo {
                    name: Some(interner.intern_string("x")),
                    type_id: lit_a,
                    optional: false,
                    rest: false,
                }],
                this_type: None,
                return_type: TypeId::NUMBER,
                type_predicate: None,
                is_method: false,
            },
            CallSignature {
                type_params: vec![],
                params: vec![ParamInfo {
                    name: Some(interner.intern_string("x")),
                    type_id: lit_b,
                    optional: false,
                    rest: false,
                }],
                this_type: None,
                return_type: TypeId::STRING,
                type_predicate: None,
                is_method: false,
            },
            CallSignature {
                type_params: vec![],
                params: vec![ParamInfo {
                    name: Some(interner.intern_string("x")),
                    type_id: TypeId::STRING,
                    optional: false,
                    rest: false,
                }],
                this_type: None,
                return_type: num_or_string,
                type_predicate: None,
                is_method: false,
            },
        ],
        construct_signatures: vec![],
        properties: vec![],
        string_index: None,
        number_index: None,
    });

    assert!(callable != TypeId::ERROR);
}

#[test]
fn test_overload_subtype_signature_order_matters() {
    // Overload signature order should be preserved for resolution
    let interner = TypeInterner::new();

    let lit_a = interner.literal_string("a");

    // Order: specific first, then general
    let specific_first = interner.callable(CallableShape {
        symbol: None,
        is_abstract: false,
        call_signatures: vec![
            CallSignature {
                type_params: vec![],
                params: vec![ParamInfo {
                    name: Some(interner.intern_string("x")),
                    type_id: lit_a,
                    optional: false,
                    rest: false,
                }],
                this_type: None,
                return_type: TypeId::NUMBER,
                type_predicate: None,
                is_method: false,
            },
            CallSignature {
                type_params: vec![],
                params: vec![ParamInfo {
                    name: Some(interner.intern_string("x")),
                    type_id: TypeId::STRING,
                    optional: false,
                    rest: false,
                }],
                this_type: None,
                return_type: TypeId::STRING,
                type_predicate: None,
                is_method: false,
            },
        ],
        construct_signatures: vec![],
        properties: vec![],
        string_index: None,
        number_index: None,
    });

    // Order: general first, then specific
    let general_first = interner.callable(CallableShape {
        symbol: None,
        is_abstract: false,
        call_signatures: vec![
            CallSignature {
                type_params: vec![],
                params: vec![ParamInfo {
                    name: Some(interner.intern_string("x")),
                    type_id: TypeId::STRING,
                    optional: false,
                    rest: false,
                }],
                this_type: None,
                return_type: TypeId::STRING,
                type_predicate: None,
                is_method: false,
            },
            CallSignature {
                type_params: vec![],
                params: vec![ParamInfo {
                    name: Some(interner.intern_string("x")),
                    type_id: lit_a,
                    optional: false,
                    rest: false,
                }],
                this_type: None,
                return_type: TypeId::NUMBER,
                type_predicate: None,
                is_method: false,
            },
        ],
        construct_signatures: vec![],
        properties: vec![],
        string_index: None,
        number_index: None,
    });

    // These should be different types due to signature order
    assert!(specific_first != general_first);
}

#[test]
fn test_overload_generic_multiple_type_params() {
    // interface MultiGeneric {
    //   <T, U>(x: T, y: U): [T, U];
    //   <T>(x: T): T;
    // }
    let interner = TypeInterner::new();

    let t_param = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: interner.intern_string("T"),
        constraint: None,
        default: None,
        is_const: false,
    }));

    let u_param = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: interner.intern_string("U"),
        constraint: None,
        default: None,
        is_const: false,
    }));

    let tuple_t_u = interner.tuple(vec![
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

    let callable = interner.callable(CallableShape {
        symbol: None,
        is_abstract: false,
        call_signatures: vec![
            CallSignature {
                type_params: vec![
                    TypeParamInfo {
                        name: interner.intern_string("T"),
                        constraint: None,
                        default: None,
                        is_const: false,
                    },
                    TypeParamInfo {
                        name: interner.intern_string("U"),
                        constraint: None,
                        default: None,
                        is_const: false,
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
                return_type: tuple_t_u,
                type_predicate: None,
                is_method: false,
            },
            CallSignature {
                type_params: vec![TypeParamInfo {
                    name: interner.intern_string("T"),
                    constraint: None,
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
                return_type: t_param,
                type_predicate: None,
                is_method: false,
            },
        ],
        construct_signatures: vec![],
        properties: vec![],
        string_index: None,
        number_index: None,
    });

    assert!(callable != TypeId::ERROR);
}

#[test]
fn test_overload_reflexivity() {
    // Same overloaded callable should be subtype of itself
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let callable = interner.callable(CallableShape {
        symbol: None,
        is_abstract: false,
        call_signatures: vec![
            CallSignature {
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
            },
            CallSignature {
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
            },
        ],
        construct_signatures: vec![],
        properties: vec![],
        string_index: None,
        number_index: None,
    });

    assert!(checker.is_subtype_of(callable, callable));
}

#[test]
fn test_overload_covariant_return_types() {
    // Overload with more specific return type should be subtype
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let lit_hello = interner.literal_string("hello");

    // Returns literal "hello"
    let specific_return = interner.callable(CallableShape {
        symbol: None,
        is_abstract: false,
        call_signatures: vec![CallSignature {
            type_params: vec![],
            params: vec![ParamInfo {
                name: Some(interner.intern_string("x")),
                type_id: TypeId::NUMBER,
                optional: false,
                rest: false,
            }],
            this_type: None,
            return_type: lit_hello,
            type_predicate: None,
            is_method: false,
        }],
        construct_signatures: vec![],
        properties: vec![],
        string_index: None,
        number_index: None,
    });

    // Returns string
    let general_return = interner.callable(CallableShape {
        symbol: None,
        is_abstract: false,
        call_signatures: vec![CallSignature {
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
        }],
        construct_signatures: vec![],
        properties: vec![],
        string_index: None,
        number_index: None,
    });

    // More specific return is subtype (covariance)
    assert!(checker.is_subtype_of(specific_return, general_return));
    assert!(!checker.is_subtype_of(general_return, specific_return));
}

#[test]
fn test_overload_contravariant_parameters() {
    // Overload with less specific parameter should be subtype
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let lit_hello = interner.literal_string("hello");

    // Accepts any string
    let general_param = interner.callable(CallableShape {
        symbol: None,
        is_abstract: false,
        call_signatures: vec![CallSignature {
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
        }],
        construct_signatures: vec![],
        properties: vec![],
        string_index: None,
        number_index: None,
    });

    // Accepts only "hello"
    let specific_param = interner.callable(CallableShape {
        symbol: None,
        is_abstract: false,
        call_signatures: vec![CallSignature {
            type_params: vec![],
            params: vec![ParamInfo {
                name: Some(interner.intern_string("x")),
                type_id: lit_hello,
                optional: false,
                rest: false,
            }],
            this_type: None,
            return_type: TypeId::VOID,
            type_predicate: None,
            is_method: false,
        }],
        construct_signatures: vec![],
        properties: vec![],
        string_index: None,
        number_index: None,
    });

    // More general param is subtype (contravariance)
    assert!(checker.is_subtype_of(general_param, specific_param));
    assert!(!checker.is_subtype_of(specific_param, general_param));
}

#[test]
fn test_overload_construct_signature_subtyping() {
    // Constructor overload subtyping
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let obj_with_x = interner.object(vec![PropertyInfo::new(
        interner.intern_string("x"),
        TypeId::NUMBER,
    )]);

    let obj_with_xy = interner.object(vec![
        PropertyInfo::new(interner.intern_string("x"), TypeId::NUMBER),
        PropertyInfo::new(interner.intern_string("y"), TypeId::NUMBER),
    ]);

    // Returns {x, y}
    let specific_constructor = interner.callable(CallableShape {
        symbol: None,
        is_abstract: false,
        call_signatures: vec![],
        construct_signatures: vec![CallSignature {
            type_params: vec![],
            params: vec![],
            this_type: None,
            return_type: obj_with_xy,
            type_predicate: None,
            is_method: false,
        }],
        properties: vec![],
        string_index: None,
        number_index: None,
    });

    // Returns {x}
    let general_constructor = interner.callable(CallableShape {
        symbol: None,
        is_abstract: false,
        call_signatures: vec![],
        construct_signatures: vec![CallSignature {
            type_params: vec![],
            params: vec![],
            this_type: None,
            return_type: obj_with_x,
            type_predicate: None,
            is_method: false,
        }],
        properties: vec![],
        string_index: None,
        number_index: None,
    });

    // More specific instance type is subtype
    assert!(checker.is_subtype_of(specific_constructor, general_constructor));
}

#[test]
fn test_overload_with_this_type() {
    // interface WithThis {
    //   (this: Window, x: string): void;
    //   (this: Document, x: number): void;
    // }
    let interner = TypeInterner::new();

    let window_type = interner.object(vec![PropertyInfo::new(
        interner.intern_string("location"),
        TypeId::STRING,
    )]);

    let document_type = interner.object(vec![PropertyInfo::new(
        interner.intern_string("body"),
        TypeId::OBJECT,
    )]);

    let callable = interner.callable(CallableShape {
        symbol: None,
        is_abstract: false,
        call_signatures: vec![
            CallSignature {
                type_params: vec![],
                params: vec![ParamInfo {
                    name: Some(interner.intern_string("x")),
                    type_id: TypeId::STRING,
                    optional: false,
                    rest: false,
                }],
                this_type: Some(window_type),
                return_type: TypeId::VOID,
                type_predicate: None,
                is_method: false,
            },
            CallSignature {
                type_params: vec![],
                params: vec![ParamInfo {
                    name: Some(interner.intern_string("x")),
                    type_id: TypeId::NUMBER,
                    optional: false,
                    rest: false,
                }],
                this_type: Some(document_type),
                return_type: TypeId::VOID,
                type_predicate: None,
                is_method: false,
            },
        ],
        construct_signatures: vec![],
        properties: vec![],
        string_index: None,
        number_index: None,
    });

    assert!(callable != TypeId::ERROR);
}

#[test]
fn test_overload_empty_callable() {
    // Empty callable (no call or construct signatures)
    let interner = TypeInterner::new();

    let empty_callable = interner.callable(CallableShape {
        symbol: None,
        is_abstract: false,
        call_signatures: vec![],
        construct_signatures: vec![],
        properties: vec![],
        ..Default::default()
    });

    assert!(empty_callable != TypeId::ERROR);
}

#[test]
fn test_overload_with_properties() {
    // interface CallableWithProps {
    //   (x: string): number;
    //   name: string;
    //   version: number;
    // }
    let interner = TypeInterner::new();

    let callable = interner.callable(CallableShape {
        symbol: None,
        is_abstract: false,
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
        properties: vec![
            PropertyInfo::new(interner.intern_string("name"), TypeId::STRING),
            PropertyInfo::new(interner.intern_string("version"), TypeId::NUMBER),
        ],
        string_index: None,
        number_index: None,
    });

    assert!(callable != TypeId::ERROR);
}

#[test]
fn test_overload_generic_default_type() {
    // interface WithDefault {
    //   <T = string>(x: T): T;
    // }
    let interner = TypeInterner::new();

    let t_param = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: interner.intern_string("T"),
        constraint: None,
        default: Some(TypeId::STRING),
        is_const: false,
    }));

    let callable = interner.callable(CallableShape {
        symbol: None,
        is_abstract: false,
        call_signatures: vec![CallSignature {
            type_params: vec![TypeParamInfo {
                name: interner.intern_string("T"),
                constraint: None,
                default: Some(TypeId::STRING),
                is_const: false,
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
        }],
        construct_signatures: vec![],
        properties: vec![],
        string_index: None,
        number_index: None,
    });

    assert!(callable != TypeId::ERROR);
}

#[test]
fn test_overload_array_methods_pattern() {
    // Array-like overloads pattern:
    // interface ArrayLike<T> {
    //   map<U>(fn: (x: T) => U): U[];
    //   filter(fn: (x: T) => boolean): T[];
    //   reduce<U>(fn: (acc: U, x: T) => U, init: U): U;
    // }
    let interner = TypeInterner::new();

    let t_param = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: interner.intern_string("T"),
        constraint: None,
        default: None,
        is_const: false,
    }));

    let u_param = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: interner.intern_string("U"),
        constraint: None,
        default: None,
        is_const: false,
    }));

    // (x: T) => U
    let map_callback = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("x")),
            type_id: t_param,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: u_param,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    // (x: T) => boolean
    let filter_callback = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("x")),
            type_id: t_param,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: TypeId::BOOLEAN,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    // (acc: U, x: T) => U
    let reduce_callback = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![
            ParamInfo {
                name: Some(interner.intern_string("acc")),
                type_id: u_param,
                optional: false,
                rest: false,
            },
            ParamInfo {
                name: Some(interner.intern_string("x")),
                type_id: t_param,
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

    let u_array = interner.array(u_param);
    let t_array = interner.array(t_param);

    // map<U>(fn: (x: T) => U): U[]
    let map_method = interner.function(FunctionShape {
        type_params: vec![TypeParamInfo {
            name: interner.intern_string("U"),
            constraint: None,
            default: None,
            is_const: false,
        }],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("fn")),
            type_id: map_callback,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: u_array,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    // filter(fn: (x: T) => boolean): T[]
    let filter_method = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("fn")),
            type_id: filter_callback,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: t_array,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    // reduce<U>(fn: (acc: U, x: T) => U, init: U): U
    let reduce_method = interner.function(FunctionShape {
        type_params: vec![TypeParamInfo {
            name: interner.intern_string("U"),
            constraint: None,
            default: None,
            is_const: false,
        }],
        params: vec![
            ParamInfo {
                name: Some(interner.intern_string("fn")),
                type_id: reduce_callback,
                optional: false,
                rest: false,
            },
            ParamInfo {
                name: Some(interner.intern_string("init")),
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

    let array_like = interner.object(vec![
        PropertyInfo::method(interner.intern_string("map"), map_method),
        PropertyInfo::method(interner.intern_string("filter"), filter_method),
        PropertyInfo::method(interner.intern_string("reduce"), reduce_method),
    ]);

    assert!(array_like != TypeId::ERROR);
}

#[test]
fn test_overload_event_handler_pattern() {
    // DOM-style event handler overloads:
    // interface EventTarget {
    //   addEventListener(type: "click", listener: (e: MouseEvent) => void): void;
    //   addEventListener(type: "keydown", listener: (e: KeyboardEvent) => void): void;
    //   addEventListener(type: string, listener: (e: Event) => void): void;
    // }
    let interner = TypeInterner::new();

    let lit_click = interner.literal_string("click");
    let lit_keydown = interner.literal_string("keydown");

    let mouse_event = interner.object(vec![
        PropertyInfo::readonly(interner.intern_string("type"), TypeId::STRING),
        PropertyInfo::readonly(interner.intern_string("clientX"), TypeId::NUMBER),
        PropertyInfo::readonly(interner.intern_string("clientY"), TypeId::NUMBER),
    ]);

    let keyboard_event = interner.object(vec![
        PropertyInfo::readonly(interner.intern_string("type"), TypeId::STRING),
        PropertyInfo::readonly(interner.intern_string("key"), TypeId::STRING),
        PropertyInfo::readonly(interner.intern_string("code"), TypeId::STRING),
    ]);

    let base_event = interner.object(vec![PropertyInfo::readonly(
        interner.intern_string("type"),
        TypeId::STRING,
    )]);

    // (e: MouseEvent) => void
    let mouse_listener = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("e")),
            type_id: mouse_event,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    // (e: KeyboardEvent) => void
    let keyboard_listener = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("e")),
            type_id: keyboard_event,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    // (e: Event) => void
    let base_listener = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("e")),
            type_id: base_event,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let add_event_listener = interner.callable(CallableShape {
        symbol: None,
        is_abstract: false,
        call_signatures: vec![
            CallSignature {
                type_params: vec![],
                params: vec![
                    ParamInfo {
                        name: Some(interner.intern_string("type")),
                        type_id: lit_click,
                        optional: false,
                        rest: false,
                    },
                    ParamInfo {
                        name: Some(interner.intern_string("listener")),
                        type_id: mouse_listener,
                        optional: false,
                        rest: false,
                    },
                ],
                this_type: None,
                return_type: TypeId::VOID,
                type_predicate: None,
                is_method: false,
            },
            CallSignature {
                type_params: vec![],
                params: vec![
                    ParamInfo {
                        name: Some(interner.intern_string("type")),
                        type_id: lit_keydown,
                        optional: false,
                        rest: false,
                    },
                    ParamInfo {
                        name: Some(interner.intern_string("listener")),
                        type_id: keyboard_listener,
                        optional: false,
                        rest: false,
                    },
                ],
                this_type: None,
                return_type: TypeId::VOID,
                type_predicate: None,
                is_method: false,
            },
            CallSignature {
                type_params: vec![],
                params: vec![
                    ParamInfo {
                        name: Some(interner.intern_string("type")),
                        type_id: TypeId::STRING,
                        optional: false,
                        rest: false,
                    },
                    ParamInfo {
                        name: Some(interner.intern_string("listener")),
                        type_id: base_listener,
                        optional: false,
                        rest: false,
                    },
                ],
                this_type: None,
                return_type: TypeId::VOID,
                type_predicate: None,
                is_method: false,
            },
        ],
        construct_signatures: vec![],
        properties: vec![],
        string_index: None,
        number_index: None,
    });

    let event_target = interner.object(vec![PropertyInfo::method(
        interner.intern_string("addEventListener"),
        add_event_listener,
    )]);

    assert!(event_target != TypeId::ERROR);
}

#[test]
fn test_overload_promise_then_pattern() {
    // Promise.then overloads:
    // interface Promise<T> {
    //   then<U>(onFulfilled: (value: T) => U): Promise<U>;
    //   then<U>(onFulfilled: (value: T) => Promise<U>): Promise<U>;
    //   then<U, V>(onFulfilled: (value: T) => U, onRejected: (reason: any) => V): Promise<U | V>;
    // }
    let interner = TypeInterner::new();

    let t_param = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: interner.intern_string("T"),
        constraint: None,
        default: None,
        is_const: false,
    }));

    let u_param = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: interner.intern_string("U"),
        constraint: None,
        default: None,
        is_const: false,
    }));

    let v_param = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: interner.intern_string("V"),
        constraint: None,
        default: None,
        is_const: false,
    }));

    // (value: T) => U
    let on_fulfilled_sync = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("value")),
            type_id: t_param,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: u_param,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    // (reason: any) => V
    let on_rejected = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("reason")),
            type_id: TypeId::ANY,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: v_param,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let u_or_v = interner.union(vec![u_param, v_param]);

    let then_method = interner.callable(CallableShape {
        symbol: None,
        is_abstract: false,
        call_signatures: vec![
            // then<U>(onFulfilled: (value: T) => U): Promise<U>
            CallSignature {
                type_params: vec![TypeParamInfo {
                    name: interner.intern_string("U"),
                    constraint: None,
                    default: None,
                    is_const: false,
                }],
                params: vec![ParamInfo {
                    name: Some(interner.intern_string("onFulfilled")),
                    type_id: on_fulfilled_sync,
                    optional: false,
                    rest: false,
                }],
                this_type: None,
                // Would be Promise<U> but simplified here
                return_type: u_param,
                type_predicate: None,
                is_method: false,
            },
            // then<U, V>(onFulfilled, onRejected): Promise<U | V>
            CallSignature {
                type_params: vec![
                    TypeParamInfo {
                        name: interner.intern_string("U"),
                        constraint: None,
                        default: None,
                        is_const: false,
                    },
                    TypeParamInfo {
                        name: interner.intern_string("V"),
                        constraint: None,
                        default: None,
                        is_const: false,
                    },
                ],
                params: vec![
                    ParamInfo {
                        name: Some(interner.intern_string("onFulfilled")),
                        type_id: on_fulfilled_sync,
                        optional: false,
                        rest: false,
                    },
                    ParamInfo {
                        name: Some(interner.intern_string("onRejected")),
                        type_id: on_rejected,
                        optional: false,
                        rest: false,
                    },
                ],
                this_type: None,
                return_type: u_or_v,
                type_predicate: None,
                is_method: false,
            },
        ],
        construct_signatures: vec![],
        properties: vec![],
        string_index: None,
        number_index: None,
    });

    assert!(then_method != TypeId::ERROR);
}

#[test]
fn test_overload_constructor_overloads() {
    // interface DateConstructor {
    //   new (): Date;
    //   new (value: number): Date;
    //   new (value: string): Date;
    //   new (year: number, month: number, date?: number): Date;
    // }
    let interner = TypeInterner::new();

    let date_instance = interner.object(vec![
        PropertyInfo {
            name: interner.intern_string("getTime"),
            type_id: interner.function(FunctionShape {
                type_params: vec![],
                params: vec![],
                this_type: None,
                return_type: TypeId::NUMBER,
                type_predicate: None,
                is_constructor: false,
                is_method: false,
            }),
            write_type: TypeId::NEVER,
            optional: false,
            readonly: true,
            is_method: true,
            is_class_prototype: false,
            visibility: Visibility::Public,
            parent_id: None,
            declaration_order: 0,
            is_string_named: false,
            is_symbol_named: false,
            single_quoted_name: false,
        },
        PropertyInfo {
            name: interner.intern_string("toISOString"),
            type_id: interner.function(FunctionShape {
                type_params: vec![],
                params: vec![],
                this_type: None,
                return_type: TypeId::STRING,
                type_predicate: None,
                is_constructor: false,
                is_method: false,
            }),
            write_type: TypeId::NEVER,
            optional: false,
            readonly: true,
            is_method: true,
            is_class_prototype: false,
            visibility: Visibility::Public,
            parent_id: None,
            declaration_order: 0,
            is_string_named: false,
            is_symbol_named: false,
            single_quoted_name: false,
        },
    ]);

    let date_constructor = interner.callable(CallableShape {
        symbol: None,
        is_abstract: false,
        call_signatures: vec![],
        construct_signatures: vec![
            // new (): Date
            CallSignature {
                type_params: vec![],
                params: vec![],
                this_type: None,
                return_type: date_instance,
                type_predicate: None,
                is_method: false,
            },
            // new (value: number): Date
            CallSignature {
                type_params: vec![],
                params: vec![ParamInfo {
                    name: Some(interner.intern_string("value")),
                    type_id: TypeId::NUMBER,
                    optional: false,
                    rest: false,
                }],
                this_type: None,
                return_type: date_instance,
                type_predicate: None,
                is_method: false,
            },
            // new (value: string): Date
            CallSignature {
                type_params: vec![],
                params: vec![ParamInfo {
                    name: Some(interner.intern_string("value")),
                    type_id: TypeId::STRING,
                    optional: false,
                    rest: false,
                }],
                this_type: None,
                return_type: date_instance,
                type_predicate: None,
                is_method: false,
            },
            // new (year: number, month: number, date?: number): Date
            CallSignature {
                type_params: vec![],
                params: vec![
                    ParamInfo {
                        name: Some(interner.intern_string("year")),
                        type_id: TypeId::NUMBER,
                        optional: false,
                        rest: false,
                    },
                    ParamInfo {
                        name: Some(interner.intern_string("month")),
                        type_id: TypeId::NUMBER,
                        optional: false,
                        rest: false,
                    },
                    ParamInfo {
                        name: Some(interner.intern_string("date")),
                        type_id: TypeId::NUMBER,
                        optional: true,
                        rest: false,
                    },
                ],
                this_type: None,
                return_type: date_instance,
                type_predicate: None,
                is_method: false,
            },
        ],
        properties: vec![],
        string_index: None,
        number_index: None,
    });

    assert!(date_constructor != TypeId::ERROR);
}

// =============================================================================
// TS2322 Detection Improvement Tests
// =============================================================================

#[test]
fn test_explain_failure_intrinsic_mismatch() {
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    // string vs number should produce IntrinsicTypeMismatch
    let reason = checker.explain_failure(TypeId::STRING, TypeId::NUMBER);
    assert!(reason.is_some());
    match reason.unwrap() {
        SubtypeFailureReason::IntrinsicTypeMismatch {
            source_type,
            target_type,
        } => {
            assert_eq!(source_type, TypeId::STRING);
            assert_eq!(target_type, TypeId::NUMBER);
        }
        other => panic!("Expected IntrinsicTypeMismatch, got {other:?}"),
    }
}


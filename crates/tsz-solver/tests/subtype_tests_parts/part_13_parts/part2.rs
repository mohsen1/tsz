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

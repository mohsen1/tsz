/// Overloaded callable like `Object.entries`:
/// - Sig 1: `<T>(o: { [s: string]: T }): [string, T][]`
/// - Sig 2: `(o: {}): [string, any][]`
///
/// Called with `any` → should use sig 1 and return `[string, unknown][]`, not `[string, any][]`.
#[test]
fn test_object_entries_like_callable_any_arg_uses_first_overload() {
    let interner = TypeInterner::new();
    let mut checker = CompatChecker::new(&interner);
    let mut evaluator = CallEvaluator::new(&interner, &mut checker);

    let t_name = interner.intern_string("T");
    let t_type = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: t_name,
        constraint: None,
        default: None,
        is_const: false,
    }));
    // { [s: string]: T }
    let s_name = interner.intern_string("s");
    let str_index_obj = interner.object_with_index(ObjectShape {
        symbol: None,
        flags: ObjectFlags::empty(),
        properties: vec![],
        string_index: Some(IndexSignature {
            key_type: TypeId::STRING,
            value_type: t_type,
            readonly: false,
            param_name: Some(s_name),
        }),
        number_index: None,
    });
    // Simulate ArrayLike<T>: { readonly length: number; readonly [n: number]: T }
    let n_name = interner.intern_string("n");
    let array_like_t = interner.object_with_index(ObjectShape {
        symbol: None,
        flags: ObjectFlags::empty(),
        properties: vec![PropertyInfo::new(
            interner.intern_string("length"),
            TypeId::NUMBER,
        )],
        string_index: None,
        number_index: Some(IndexSignature {
            key_type: TypeId::NUMBER,
            value_type: t_type,
            readonly: true,
            param_name: Some(n_name),
        }),
    });
    // { [s: string]: T } | ArrayLike<T>
    let index_sig_obj = interner.union(vec![str_index_obj, array_like_t]);

    // Return type of sig 1: [string, T][]
    let tuple_elem_str = TupleElement {
        type_id: TypeId::STRING,
        name: None,
        optional: false,
        rest: false,
    };
    let tuple_elem_t = TupleElement {
        type_id: t_type,
        name: None,
        optional: false,
        rest: false,
    };
    let string_t_tuple = interner.tuple(vec![tuple_elem_str, tuple_elem_t]);
    let return_sig1 = interner.array(string_t_tuple);

    // Return type of sig 2: [string, any][]
    let tuple_elem_str2 = TupleElement {
        type_id: TypeId::STRING,
        name: None,
        optional: false,
        rest: false,
    };
    let tuple_elem_any = TupleElement {
        type_id: TypeId::ANY,
        name: None,
        optional: false,
        rest: false,
    };
    let string_any_tuple = interner.tuple(vec![tuple_elem_str2, tuple_elem_any]);
    let return_sig2 = interner.array(string_any_tuple);

    // Sig 2: (o: {}): [string, any][]
    let empty_obj = interner.object(vec![]);
    let o_name = interner.intern_string("o");
    let sig2 = CallSignature {
        type_params: vec![],
        params: vec![ParamInfo {
            name: Some(o_name),
            type_id: empty_obj,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: return_sig2,
        type_predicate: None,
        is_method: false,
    };

    // Sig 1: <T>(o: { [s: string]: T }): [string, T][]
    let sig1 = CallSignature {
        type_params: vec![TypeParamInfo {
            name: t_name,
            constraint: None,
            default: None,
            is_const: false,
        }],
        params: vec![ParamInfo {
            name: Some(o_name),
            type_id: index_sig_obj,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: return_sig1,
        type_predicate: None,
        is_method: false,
    };

    let callable = interner.callable(crate::types::CallableShape {
        call_signatures: vec![sig1, sig2],
        ..Default::default()
    });

    // Expected: [string, unknown][]
    // tsc picks sig 1 (first matching overload) with T = unknown (index-sig value
    // position is NOT a propagation target for `any`), so the return is [string, unknown][].
    let expected_return = {
        let str_elem = TupleElement {
            type_id: TypeId::STRING,
            name: None,
            optional: false,
            rest: false,
        };
        let unk_elem = TupleElement {
            type_id: TypeId::UNKNOWN,
            name: None,
            optional: false,
            rest: false,
        };
        interner.array(interner.tuple(vec![str_elem, unk_elem]))
    };

    let result = evaluator.resolve_call(callable, &[TypeId::ANY]);
    match result {
        CallResult::Success(ret) => {
            assert_eq!(
                ret, expected_return,
                "Callable with any arg: expected [string,unknown][], got {ret:?}"
            );
        }
        other => panic!("Expected Success, got {other:?}"),
    }
}

/// `f<T>(x: { v: T }): T` called with `any` → T = `unknown`.
/// T is an object property type, not naked; `any` must NOT propagate.
#[test]
fn test_any_arg_object_prop_t_infers_unknown() {
    let interner = TypeInterner::new();
    let mut checker = CompatChecker::new(&interner);
    let mut evaluator = CallEvaluator::new(&interner, &mut checker);

    let t_name = interner.intern_string("T");
    let t_type = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: t_name,
        constraint: None,
        default: None,
        is_const: false,
    }));
    let v_name = interner.intern_string("v");
    let param_obj = interner.object(vec![PropertyInfo::new(v_name, t_type)]);

    let func = interner.function(FunctionShape {
        type_params: vec![TypeParamInfo {
            name: t_name,
            constraint: None,
            default: None,
            is_const: false,
        }],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("x")),
            type_id: param_obj,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: t_type,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let result = evaluator.resolve_call(func, &[TypeId::ANY]);
    match result {
        CallResult::Success(ret) => {
            assert_eq!(
                ret,
                TypeId::UNKNOWN,
                "{{ v: T }} with any arg: expected T=unknown (not any), got {ret:?}"
            );
        }
        other => panic!("Expected success, got {other:?}"),
    }
}

/// `f<T>(x: { [s: string]: T }): T` called with `any` → T = `unknown`.
/// T is an index-signature value type, not naked; `any` must NOT propagate.
#[test]
fn test_any_arg_index_sig_t_infers_unknown() {
    let interner = TypeInterner::new();
    let mut checker = CompatChecker::new(&interner);
    let mut evaluator = CallEvaluator::new(&interner, &mut checker);

    let t_name = interner.intern_string("T");
    let t_type = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: t_name,
        constraint: None,
        default: None,
        is_const: false,
    }));
    let param_obj = interner.object_with_index(ObjectShape {
        symbol: None,
        flags: ObjectFlags::empty(),
        properties: vec![],
        string_index: Some(IndexSignature {
            key_type: TypeId::STRING,
            value_type: t_type,
            readonly: false,
            param_name: None,
        }),
        number_index: None,
    });

    let func = interner.function(FunctionShape {
        type_params: vec![TypeParamInfo {
            name: t_name,
            constraint: None,
            default: None,
            is_const: false,
        }],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("x")),
            type_id: param_obj,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: t_type,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let result = evaluator.resolve_call(func, &[TypeId::ANY]);
    match result {
        CallResult::Success(ret) => {
            assert_eq!(
                ret,
                TypeId::UNKNOWN,
                "{{ [s: string]: T }} with any arg: expected T=unknown (not any), got {ret:?}"
            );
        }
        other => panic!("Expected success, got {other:?}"),
    }
}

/// `f<T>(x: string extends string ? T : number): T` with `any` arg.
/// T appears as the true branch (naked placeholder) of a conditional.
/// tsc infers T = `any` — the true/false branches ARE walked.
#[test]
fn test_any_arg_conditional_true_branch_t_infers_any() {
    let interner = TypeInterner::new();
    let mut checker = CompatChecker::new(&interner);
    let mut evaluator = CallEvaluator::new(&interner, &mut checker);

    let t_name = interner.intern_string("T");
    let t_type = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: t_name,
        constraint: None,
        default: None,
        is_const: false,
    }));
    // `string extends string ? T : number`
    let cond_type = interner.conditional(ConditionalType {
        check_type: TypeId::STRING,
        extends_type: TypeId::STRING,
        true_type: t_type,
        false_type: TypeId::NUMBER,
        is_distributive: false,
    });

    let func = interner.function(FunctionShape {
        type_params: vec![TypeParamInfo {
            name: t_name,
            constraint: None,
            default: None,
            is_const: false,
        }],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("x")),
            type_id: cond_type,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: t_type,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let result = evaluator.resolve_call(func, &[TypeId::ANY]);
    match result {
        CallResult::Success(ret) => {
            assert_eq!(
                ret,
                TypeId::ANY,
                "conditional true-branch T with any arg: expected T=any, got {ret:?}"
            );
        }
        other => panic!("Expected success, got {other:?}"),
    }
}

/// `f<T>(x: string extends number ? string : T): T` with `any` arg.
/// T appears as the false branch (naked placeholder).
/// tsc infers T = `any`.
#[test]
fn test_any_arg_conditional_false_branch_t_infers_any() {
    let interner = TypeInterner::new();
    let mut checker = CompatChecker::new(&interner);
    let mut evaluator = CallEvaluator::new(&interner, &mut checker);

    let t_name = interner.intern_string("T");
    let t_type = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: t_name,
        constraint: None,
        default: None,
        is_const: false,
    }));
    // `string extends number ? string : T`  (false branch is T)
    let cond_type = interner.conditional(ConditionalType {
        check_type: TypeId::STRING,
        extends_type: TypeId::NUMBER,
        true_type: TypeId::STRING,
        false_type: t_type,
        is_distributive: false,
    });

    let func = interner.function(FunctionShape {
        type_params: vec![TypeParamInfo {
            name: t_name,
            constraint: None,
            default: None,
            is_const: false,
        }],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("x")),
            type_id: cond_type,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: t_type,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let result = evaluator.resolve_call(func, &[TypeId::ANY]);
    match result {
        CallResult::Success(ret) => {
            assert_eq!(
                ret,
                TypeId::ANY,
                "conditional false-branch T with any arg: expected T=any, got {ret:?}"
            );
        }
        other => panic!("Expected success, got {other:?}"),
    }
}

/// `f<T>(x: T extends string ? string : number): T` with `any` arg.
/// T appears ONLY in the check position, not in true/false branches.
/// tsc infers T = `unknown` — check position is NOT walked.
#[test]
fn test_any_arg_conditional_check_only_t_infers_unknown() {
    let interner = TypeInterner::new();
    let mut checker = CompatChecker::new(&interner);
    let mut evaluator = CallEvaluator::new(&interner, &mut checker);

    let t_name = interner.intern_string("T");
    let t_type = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: t_name,
        constraint: None,
        default: None,
        is_const: false,
    }));
    // `T extends string ? string : number`  (T only in check)
    let cond_type = interner.conditional(ConditionalType {
        check_type: t_type,
        extends_type: TypeId::STRING,
        true_type: TypeId::STRING,
        false_type: TypeId::NUMBER,
        is_distributive: false,
    });

    let func = interner.function(FunctionShape {
        type_params: vec![TypeParamInfo {
            name: t_name,
            constraint: None,
            default: None,
            is_const: false,
        }],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("x")),
            type_id: cond_type,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: t_type,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let result = evaluator.resolve_call(func, &[TypeId::ANY]);
    match result {
        CallResult::Success(ret) => {
            assert_eq!(
                ret,
                TypeId::UNKNOWN,
                "conditional check-only T with any arg: expected T=unknown, got {ret:?}"
            );
        }
        other => panic!("Expected success, got {other:?}"),
    }
}

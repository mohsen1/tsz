use super::*;
use crate::TypeInterner;
use crate::def::DefId;
use crate::{SubtypeChecker, TypeSubstitution, instantiate_type};
#[test]
fn test_conditional_infer_function_param_non_distributive_union_branch() {
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

    // [T] extends [(arg: infer R) => void] ? R : never, with T = ((arg: string) => void) | number.
    let extends_fn = interner.function(FunctionShape {
        params: vec![ParamInfo::unnamed(infer_r)],
        this_type: None,
        return_type: TypeId::VOID,
        type_params: Vec::new(),
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });
    let cond = ConditionalType {
        check_type: interner.tuple(vec![TupleElement {
            type_id: t_param,
            name: None,
            optional: false,
            rest: false,
        }]),
        extends_type: interner.tuple(vec![TupleElement {
            type_id: extends_fn,
            name: None,
            optional: false,
            rest: false,
        }]),
        true_type: infer_r,
        false_type: TypeId::NEVER,
        is_distributive: false,
    };

    let cond_type = interner.conditional(cond);
    let mut subst = TypeSubstitution::new();
    let string_fn = interner.function(FunctionShape {
        params: vec![ParamInfo::unnamed(TypeId::STRING)],
        this_type: None,
        return_type: TypeId::VOID,
        type_params: Vec::new(),
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });
    subst.insert(t_name, interner.union(vec![string_fn, TypeId::NUMBER]));

    let instantiated = instantiate_type(&interner, cond_type, &subst);
    let result = evaluate_type(&interner, instantiated);

    assert_eq!(result, TypeId::NEVER);
}

#[test]
fn test_conditional_infer_function_rest_param_distributive() {
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

    // T extends (...args: infer R) => void ? R : never, with T = ((...args: string[]) => void)
    // | ((...args: number[]) => void).
    let extends_fn = interner.function(FunctionShape {
        params: vec![ParamInfo {
            name: None,
            type_id: infer_r,
            optional: false,
            rest: true,
        }],
        this_type: None,
        return_type: TypeId::VOID,
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
    let string_fn = interner.function(FunctionShape {
        params: vec![ParamInfo {
            name: None,
            type_id: interner.array(TypeId::STRING),
            optional: false,
            rest: true,
        }],
        this_type: None,
        return_type: TypeId::VOID,
        type_params: Vec::new(),
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });
    let number_fn = interner.function(FunctionShape {
        params: vec![ParamInfo {
            name: None,
            type_id: interner.array(TypeId::NUMBER),
            optional: false,
            rest: true,
        }],
        this_type: None,
        return_type: TypeId::VOID,
        type_params: Vec::new(),
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });
    subst.insert(t_name, interner.union(vec![string_fn, number_fn]));

    let instantiated = instantiate_type(&interner, cond_type, &subst);
    let result = evaluate_type(&interner, instantiated);

    let expected = interner.union(vec![
        interner.array(TypeId::STRING),
        interner.array(TypeId::NUMBER),
    ]);
    assert_eq!(result, expected);
}

#[test]
fn test_conditional_infer_function_rest_param_non_distributive_union_input() {
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

    // [T] extends [(...args: infer R) => void] ? R : never, with T = ((...args: string[]) => void)
    // | ((...args: number[]) => void).
    let extends_fn = interner.function(FunctionShape {
        params: vec![ParamInfo {
            name: None,
            type_id: infer_r,
            optional: false,
            rest: true,
        }],
        this_type: None,
        return_type: TypeId::VOID,
        type_params: Vec::new(),
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });
    let cond = ConditionalType {
        check_type: interner.tuple(vec![TupleElement {
            type_id: t_param,
            name: None,
            optional: false,
            rest: false,
        }]),
        extends_type: interner.tuple(vec![TupleElement {
            type_id: extends_fn,
            name: None,
            optional: false,
            rest: false,
        }]),
        true_type: infer_r,
        false_type: TypeId::NEVER,
        is_distributive: false,
    };

    let cond_type = interner.conditional(cond);
    let mut subst = TypeSubstitution::new();
    let string_fn = interner.function(FunctionShape {
        params: vec![ParamInfo {
            name: None,
            type_id: interner.array(TypeId::STRING),
            optional: false,
            rest: true,
        }],
        this_type: None,
        return_type: TypeId::VOID,
        type_params: Vec::new(),
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });
    let number_fn = interner.function(FunctionShape {
        params: vec![ParamInfo {
            name: None,
            type_id: interner.array(TypeId::NUMBER),
            optional: false,
            rest: true,
        }],
        this_type: None,
        return_type: TypeId::VOID,
        type_params: Vec::new(),
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });
    subst.insert(t_name, interner.union(vec![string_fn, number_fn]));

    let instantiated = instantiate_type(&interner, cond_type, &subst);
    let result = evaluate_type(&interner, instantiated);

    // Contravariant: infer in rest parameter position from union source
    // produces intersection (string[] & number[] for disjoint array types).
    let expected = interner.intersection(vec![
        interner.array(TypeId::STRING),
        interner.array(TypeId::NUMBER),
    ]);
    assert_eq!(result, expected);
}

#[test]
fn test_conditional_infer_function_rest_param_non_distributive_union_branch() {
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

    // [T] extends [(...args: infer R) => void] ? R : never, with T = ((...args: string[]) => void)
    // | number.
    let extends_fn = interner.function(FunctionShape {
        params: vec![ParamInfo {
            name: None,
            type_id: infer_r,
            optional: false,
            rest: true,
        }],
        this_type: None,
        return_type: TypeId::VOID,
        type_params: Vec::new(),
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });
    let cond = ConditionalType {
        check_type: interner.tuple(vec![TupleElement {
            type_id: t_param,
            name: None,
            optional: false,
            rest: false,
        }]),
        extends_type: interner.tuple(vec![TupleElement {
            type_id: extends_fn,
            name: None,
            optional: false,
            rest: false,
        }]),
        true_type: infer_r,
        false_type: TypeId::NEVER,
        is_distributive: false,
    };

    let cond_type = interner.conditional(cond);
    let mut subst = TypeSubstitution::new();
    let string_fn = interner.function(FunctionShape {
        params: vec![ParamInfo {
            name: None,
            type_id: interner.array(TypeId::STRING),
            optional: false,
            rest: true,
        }],
        this_type: None,
        return_type: TypeId::VOID,
        type_params: Vec::new(),
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });
    subst.insert(t_name, interner.union(vec![string_fn, TypeId::NUMBER]));

    let instantiated = instantiate_type(&interner, cond_type, &subst);
    let result = evaluate_type(&interner, instantiated);

    assert_eq!(result, TypeId::NEVER);
}

#[test]
fn test_conditional_infer_function_this_param_distributive() {
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

    // T extends (this: infer R) => void ? R : never, with T = ((this: string) => void)
    // | ((this: number) => void).
    let extends_fn = interner.function(FunctionShape {
        params: Vec::new(),
        this_type: Some(infer_r),
        return_type: TypeId::VOID,
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
    let string_fn = interner.function(FunctionShape {
        params: Vec::new(),
        this_type: Some(TypeId::STRING),
        return_type: TypeId::VOID,
        type_params: Vec::new(),
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });
    let number_fn = interner.function(FunctionShape {
        params: Vec::new(),
        this_type: Some(TypeId::NUMBER),
        return_type: TypeId::VOID,
        type_params: Vec::new(),
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });
    subst.insert(t_name, interner.union(vec![string_fn, number_fn]));

    let instantiated = instantiate_type(&interner, cond_type, &subst);
    let result = evaluate_type(&interner, instantiated);
    let expected = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);

    assert_eq!(result, expected);
}

#[test]
fn test_conditional_infer_function_this_param_non_distributive_union_input() {
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

    // [T] extends [(this: infer R) => void] ? R : never, with T = ((this: string) => void)
    // | ((this: number) => void).
    let extends_fn = interner.function(FunctionShape {
        params: Vec::new(),
        this_type: Some(infer_r),
        return_type: TypeId::VOID,
        type_params: Vec::new(),
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });
    let cond = ConditionalType {
        check_type: interner.tuple(vec![TupleElement {
            type_id: t_param,
            name: None,
            optional: false,
            rest: false,
        }]),
        extends_type: interner.tuple(vec![TupleElement {
            type_id: extends_fn,
            name: None,
            optional: false,
            rest: false,
        }]),
        true_type: infer_r,
        false_type: TypeId::NEVER,
        is_distributive: false,
    };

    let cond_type = interner.conditional(cond);
    let mut subst = TypeSubstitution::new();
    let string_fn = interner.function(FunctionShape {
        params: Vec::new(),
        this_type: Some(TypeId::STRING),
        return_type: TypeId::VOID,
        type_params: Vec::new(),
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });
    let number_fn = interner.function(FunctionShape {
        params: Vec::new(),
        this_type: Some(TypeId::NUMBER),
        return_type: TypeId::VOID,
        type_params: Vec::new(),
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });
    subst.insert(t_name, interner.union(vec![string_fn, number_fn]));

    let instantiated = instantiate_type(&interner, cond_type, &subst);
    let result = evaluate_type(&interner, instantiated);
    let expected = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);

    assert_eq!(result, expected);
}

#[test]
fn test_conditional_infer_function_this_param_non_distributive_union_branch() {
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

    // [T] extends [(this: infer R) => void] ? R : never, with T = ((this: string) => void)
    // | number.
    let extends_fn = interner.function(FunctionShape {
        params: Vec::new(),
        this_type: Some(infer_r),
        return_type: TypeId::VOID,
        type_params: Vec::new(),
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });
    let cond = ConditionalType {
        check_type: interner.tuple(vec![TupleElement {
            type_id: t_param,
            name: None,
            optional: false,
            rest: false,
        }]),
        extends_type: interner.tuple(vec![TupleElement {
            type_id: extends_fn,
            name: None,
            optional: false,
            rest: false,
        }]),
        true_type: infer_r,
        false_type: TypeId::NEVER,
        is_distributive: false,
    };

    let cond_type = interner.conditional(cond);
    let mut subst = TypeSubstitution::new();
    let string_fn = interner.function(FunctionShape {
        params: Vec::new(),
        this_type: Some(TypeId::STRING),
        return_type: TypeId::VOID,
        type_params: Vec::new(),
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });
    subst.insert(t_name, interner.union(vec![string_fn, TypeId::NUMBER]));

    let instantiated = instantiate_type(&interner, cond_type, &subst);
    let result = evaluate_type(&interner, instantiated);

    assert_eq!(result, TypeId::NEVER);
}

#[test]
fn test_conditional_infer_function_return_distributive() {
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

    // T extends () => infer R ? R : never, with T = (() => string) | (() => number).
    let extends_fn = interner.function(FunctionShape {
        params: Vec::new(),
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
    let string_fn = interner.function(FunctionShape {
        params: Vec::new(),
        this_type: None,
        return_type: TypeId::STRING,
        type_params: Vec::new(),
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });
    let number_fn = interner.function(FunctionShape {
        params: Vec::new(),
        this_type: None,
        return_type: TypeId::NUMBER,
        type_params: Vec::new(),
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });
    subst.insert(t_name, interner.union(vec![string_fn, number_fn]));

    let instantiated = instantiate_type(&interner, cond_type, &subst);
    let result = evaluate_type(&interner, instantiated);

    let expected = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);
    assert_eq!(result, expected);
}

#[test]
fn test_conditional_infer_function_return_non_distributive_union_input() {
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

    // [T] extends [() => infer R] ? R : never, with T = (() => string) | (() => number).
    let extends_fn = interner.function(FunctionShape {
        params: Vec::new(),
        this_type: None,
        return_type: infer_r,
        type_params: Vec::new(),
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });
    let cond = ConditionalType {
        check_type: interner.tuple(vec![TupleElement {
            type_id: t_param,
            name: None,
            optional: false,
            rest: false,
        }]),
        extends_type: interner.tuple(vec![TupleElement {
            type_id: extends_fn,
            name: None,
            optional: false,
            rest: false,
        }]),
        true_type: infer_r,
        false_type: TypeId::NEVER,
        is_distributive: false,
    };

    let cond_type = interner.conditional(cond);
    let mut subst = TypeSubstitution::new();
    let string_fn = interner.function(FunctionShape {
        params: Vec::new(),
        this_type: None,
        return_type: TypeId::STRING,
        type_params: Vec::new(),
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });
    let number_fn = interner.function(FunctionShape {
        params: Vec::new(),
        this_type: None,
        return_type: TypeId::NUMBER,
        type_params: Vec::new(),
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });
    subst.insert(t_name, interner.union(vec![string_fn, number_fn]));

    let instantiated = instantiate_type(&interner, cond_type, &subst);
    let result = evaluate_type(&interner, instantiated);

    let expected = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);
    assert_eq!(result, expected);
}

#[test]
fn test_conditional_infer_function_param_and_return_distributive() {
    let interner = TypeInterner::new();

    let t_name = interner.intern_string("T");
    let t_param = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: t_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    let p_name = interner.intern_string("P");
    let infer_p = interner.intern(TypeData::Infer(TypeParamInfo {
        name: p_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    let r_name = interner.intern_string("R");
    let infer_r = interner.intern(TypeData::Infer(TypeParamInfo {
        name: r_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    // T extends (arg: infer P) => infer R ? [P, R] : never, with T = ((arg: string) => number)
    // | ((arg: boolean) => string).
    let extends_fn = interner.function(FunctionShape {
        params: vec![ParamInfo::unnamed(infer_p)],
        this_type: None,
        return_type: infer_r,
        type_params: Vec::new(),
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });
    let true_tuple = interner.tuple(vec![
        TupleElement {
            type_id: infer_p,
            name: None,
            optional: false,
            rest: false,
        },
        TupleElement {
            type_id: infer_r,
            name: None,
            optional: false,
            rest: false,
        },
    ]);
    let cond = ConditionalType {
        check_type: t_param,
        extends_type: extends_fn,
        true_type: true_tuple,
        false_type: TypeId::NEVER,
        is_distributive: true,
    };

    let cond_type = interner.conditional(cond);
    let mut subst = TypeSubstitution::new();
    let string_number_fn = interner.function(FunctionShape {
        params: vec![ParamInfo::unnamed(TypeId::STRING)],
        this_type: None,
        return_type: TypeId::NUMBER,
        type_params: Vec::new(),
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });
    let boolean_string_fn = interner.function(FunctionShape {
        params: vec![ParamInfo::unnamed(TypeId::BOOLEAN)],
        this_type: None,
        return_type: TypeId::STRING,
        type_params: Vec::new(),
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });
    subst.insert(
        t_name,
        interner.union(vec![string_number_fn, boolean_string_fn]),
    );

    let instantiated = instantiate_type(&interner, cond_type, &subst);
    let result = evaluate_type(&interner, instantiated);

    let tuple_string_number = interner.tuple(vec![
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
    let tuple_boolean_string = interner.tuple(vec![
        TupleElement {
            type_id: TypeId::BOOLEAN,
            name: None,
            optional: false,
            rest: false,
        },
        TupleElement {
            type_id: TypeId::STRING,
            name: None,
            optional: false,
            rest: false,
        },
    ]);
    let expected = interner.union(vec![tuple_string_number, tuple_boolean_string]);
    assert_eq!(result, expected);
}

#[test]
fn test_conditional_infer_function_return_non_distributive_union_branch() {
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

    // [T] extends [() => infer R] ? R : never, with T = (() => string) | number.
    let extends_fn = interner.function(FunctionShape {
        params: Vec::new(),
        this_type: None,
        return_type: infer_r,
        type_params: Vec::new(),
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });
    let cond = ConditionalType {
        check_type: interner.tuple(vec![TupleElement {
            type_id: t_param,
            name: None,
            optional: false,
            rest: false,
        }]),
        extends_type: interner.tuple(vec![TupleElement {
            type_id: extends_fn,
            name: None,
            optional: false,
            rest: false,
        }]),
        true_type: infer_r,
        false_type: TypeId::NEVER,
        is_distributive: false,
    };

    let cond_type = interner.conditional(cond);
    let mut subst = TypeSubstitution::new();
    let string_fn = interner.function(FunctionShape {
        params: Vec::new(),
        this_type: None,
        return_type: TypeId::STRING,
        type_params: Vec::new(),
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });
    subst.insert(t_name, interner.union(vec![string_fn, TypeId::NUMBER]));

    let instantiated = instantiate_type(&interner, cond_type, &subst);
    let result = evaluate_type(&interner, instantiated);

    assert_eq!(result, TypeId::NEVER);
}

#[test]
fn test_conditional_infer_function_param_and_return_non_distributive_union_input() {
    let interner = TypeInterner::new();

    let t_name = interner.intern_string("T");
    let t_param = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: t_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    let p_name = interner.intern_string("P");
    let infer_p = interner.intern(TypeData::Infer(TypeParamInfo {
        name: p_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    let r_name = interner.intern_string("R");
    let infer_r = interner.intern(TypeData::Infer(TypeParamInfo {
        name: r_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    // [T] extends [(arg: infer P) => infer R] ? [P, R] : never, with T = ((arg: string) => number)
    // | ((arg: boolean) => string).
    let extends_fn = interner.function(FunctionShape {
        params: vec![ParamInfo::unnamed(infer_p)],
        this_type: None,
        return_type: infer_r,
        type_params: Vec::new(),
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });
    let true_tuple = interner.tuple(vec![
        TupleElement {
            type_id: infer_p,
            name: None,
            optional: false,
            rest: false,
        },
        TupleElement {
            type_id: infer_r,
            name: None,
            optional: false,
            rest: false,
        },
    ]);
    let cond = ConditionalType {
        check_type: interner.tuple(vec![TupleElement {
            type_id: t_param,
            name: None,
            optional: false,
            rest: false,
        }]),
        extends_type: interner.tuple(vec![TupleElement {
            type_id: extends_fn,
            name: None,
            optional: false,
            rest: false,
        }]),
        true_type: true_tuple,
        false_type: TypeId::NEVER,
        is_distributive: false,
    };

    let cond_type = interner.conditional(cond);
    let mut subst = TypeSubstitution::new();
    let string_number_fn = interner.function(FunctionShape {
        params: vec![ParamInfo::unnamed(TypeId::STRING)],
        this_type: None,
        return_type: TypeId::NUMBER,
        type_params: Vec::new(),
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });
    let boolean_string_fn = interner.function(FunctionShape {
        params: vec![ParamInfo::unnamed(TypeId::BOOLEAN)],
        this_type: None,
        return_type: TypeId::STRING,
        type_params: Vec::new(),
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });
    subst.insert(
        t_name,
        interner.union(vec![string_number_fn, boolean_string_fn]),
    );

    let instantiated = instantiate_type(&interner, cond_type, &subst);
    let result = evaluate_type(&interner, instantiated);

    // P is contravariant (function parameter): string & boolean = never
    // R is covariant (function return): number | string
    let param_intersection = TypeId::NEVER; // string & boolean = never
    let return_union = interner.union(vec![TypeId::NUMBER, TypeId::STRING]);
    let expected = interner.tuple(vec![
        TupleElement {
            type_id: param_intersection,
            name: None,
            optional: false,
            rest: false,
        },
        TupleElement {
            type_id: return_union,
            name: None,
            optional: false,
            rest: false,
        },
    ]);
    assert_eq!(result, expected);
}

#[test]
fn test_conditional_infer_object_call_signature_distributive() {
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

    // T extends { (x: infer R): void } ? R : never, with T = { (x: string): void }
    // | { (x: number): void }.
    let extends_callable = interner.callable(CallableShape {
        symbol: None,
        is_abstract: false,
        call_signatures: vec![CallSignature {
            params: vec![ParamInfo::unnamed(infer_r)],
            this_type: None,
            return_type: TypeId::VOID,
            type_predicate: None,
            type_params: Vec::new(),
            is_method: false,
        }],
        construct_signatures: Vec::new(),
        properties: Vec::new(),
        string_index: None,
        number_index: None,
    });
    let cond = ConditionalType {
        check_type: t_param,
        extends_type: extends_callable,
        true_type: infer_r,
        false_type: TypeId::NEVER,
        is_distributive: true,
    };

    let cond_type = interner.conditional(cond);
    let mut subst = TypeSubstitution::new();
    let string_callable = interner.callable(CallableShape {
        symbol: None,
        is_abstract: false,
        call_signatures: vec![CallSignature {
            params: vec![ParamInfo::unnamed(TypeId::STRING)],
            this_type: None,
            return_type: TypeId::VOID,
            type_predicate: None,
            type_params: Vec::new(),
            is_method: false,
        }],
        construct_signatures: Vec::new(),
        properties: Vec::new(),
        string_index: None,
        number_index: None,
    });
    let number_callable = interner.callable(CallableShape {
        symbol: None,
        is_abstract: false,
        call_signatures: vec![CallSignature {
            params: vec![ParamInfo::unnamed(TypeId::NUMBER)],
            this_type: None,
            return_type: TypeId::VOID,
            type_predicate: None,
            type_params: Vec::new(),
            is_method: false,
        }],
        construct_signatures: Vec::new(),
        properties: Vec::new(),
        string_index: None,
        number_index: None,
    });
    subst.insert(
        t_name,
        interner.union(vec![string_callable, number_callable]),
    );

    let instantiated = instantiate_type(&interner, cond_type, &subst);
    let result = evaluate_type(&interner, instantiated);

    let expected = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);
    assert_eq!(result, expected);
}

#[test]
fn test_conditional_infer_call_signature_param_from_function_distributive() {
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

    // T extends { (x: infer R): void } ? R : never, with T = ((x: string) => void)
    // | ((x: number) => void).
    let extends_callable = interner.callable(CallableShape {
        symbol: None,
        is_abstract: false,
        call_signatures: vec![CallSignature {
            params: vec![ParamInfo::unnamed(infer_r)],
            this_type: None,
            return_type: TypeId::VOID,
            type_predicate: None,
            type_params: Vec::new(),
            is_method: false,
        }],
        construct_signatures: Vec::new(),
        properties: Vec::new(),
        string_index: None,
        number_index: None,
    });
    let cond = ConditionalType {
        check_type: t_param,
        extends_type: extends_callable,
        true_type: infer_r,
        false_type: TypeId::NEVER,
        is_distributive: true,
    };

    let cond_type = interner.conditional(cond);
    let mut subst = TypeSubstitution::new();
    let string_fn = interner.function(FunctionShape {
        params: vec![ParamInfo::unnamed(TypeId::STRING)],
        this_type: None,
        return_type: TypeId::VOID,
        type_params: Vec::new(),
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });
    let number_fn = interner.function(FunctionShape {
        params: vec![ParamInfo::unnamed(TypeId::NUMBER)],
        this_type: None,
        return_type: TypeId::VOID,
        type_params: Vec::new(),
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });
    subst.insert(t_name, interner.union(vec![string_fn, number_fn]));

    let instantiated = instantiate_type(&interner, cond_type, &subst);
    let result = evaluate_type(&interner, instantiated);

    let expected = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);
    assert_eq!(result, expected);
}

#[test]
fn test_conditional_infer_call_signature_return_from_function_distributive() {
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

    // T extends { (): infer R } ? R : never, with T = (() => string) | (() => number).
    let extends_callable = interner.callable(CallableShape {
        symbol: None,
        is_abstract: false,
        call_signatures: vec![CallSignature {
            params: Vec::new(),
            this_type: None,
            return_type: infer_r,
            type_predicate: None,
            type_params: Vec::new(),
            is_method: false,
        }],
        construct_signatures: Vec::new(),
        properties: Vec::new(),
        string_index: None,
        number_index: None,
    });
    let cond = ConditionalType {
        check_type: t_param,
        extends_type: extends_callable,
        true_type: infer_r,
        false_type: TypeId::NEVER,
        is_distributive: true,
    };

    let cond_type = interner.conditional(cond);
    let mut subst = TypeSubstitution::new();
    let string_fn = interner.function(FunctionShape {
        params: Vec::new(),
        this_type: None,
        return_type: TypeId::STRING,
        type_params: Vec::new(),
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });
    let number_fn = interner.function(FunctionShape {
        params: Vec::new(),
        this_type: None,
        return_type: TypeId::NUMBER,
        type_params: Vec::new(),
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });
    subst.insert(t_name, interner.union(vec![string_fn, number_fn]));

    let instantiated = instantiate_type(&interner, cond_type, &subst);
    let result = evaluate_type(&interner, instantiated);

    let expected = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);
    assert_eq!(result, expected);
}

#[test]
fn test_conditional_infer_object_call_signature_non_distributive_union_input() {
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

    // [T] extends [{ (x: infer R): void }] ? R : never, with T = { (x: string): void }
    // | { (x: number): void }.
    let extends_callable = interner.callable(CallableShape {
        symbol: None,
        is_abstract: false,
        call_signatures: vec![CallSignature {
            params: vec![ParamInfo::unnamed(infer_r)],
            this_type: None,
            return_type: TypeId::VOID,
            type_predicate: None,
            type_params: Vec::new(),
            is_method: false,
        }],
        construct_signatures: Vec::new(),
        properties: Vec::new(),
        string_index: None,
        number_index: None,
    });
    let cond = ConditionalType {
        check_type: interner.tuple(vec![TupleElement {
            type_id: t_param,
            name: None,
            optional: false,
            rest: false,
        }]),
        extends_type: interner.tuple(vec![TupleElement {
            type_id: extends_callable,
            name: None,
            optional: false,
            rest: false,
        }]),
        true_type: infer_r,
        false_type: TypeId::NEVER,
        is_distributive: false,
    };

    let cond_type = interner.conditional(cond);
    let mut subst = TypeSubstitution::new();
    let string_callable = interner.callable(CallableShape {
        symbol: None,
        is_abstract: false,
        call_signatures: vec![CallSignature {
            params: vec![ParamInfo::unnamed(TypeId::STRING)],
            this_type: None,
            return_type: TypeId::VOID,
            type_predicate: None,
            type_params: Vec::new(),
            is_method: false,
        }],
        construct_signatures: Vec::new(),
        properties: Vec::new(),
        string_index: None,
        number_index: None,
    });
    let number_callable = interner.callable(CallableShape {
        symbol: None,
        is_abstract: false,
        call_signatures: vec![CallSignature {
            params: vec![ParamInfo::unnamed(TypeId::NUMBER)],
            this_type: None,
            return_type: TypeId::VOID,
            type_predicate: None,
            type_params: Vec::new(),
            is_method: false,
        }],
        construct_signatures: Vec::new(),
        properties: Vec::new(),
        string_index: None,
        number_index: None,
    });
    subst.insert(
        t_name,
        interner.union(vec![string_callable, number_callable]),
    );

    let instantiated = instantiate_type(&interner, cond_type, &subst);
    let result = evaluate_type(&interner, instantiated);

    // Contravariant: infer in call signature parameter from union source
    let expected = TypeId::NEVER;
    assert_eq!(result, expected);
}

#[test]
fn test_conditional_infer_object_call_signature_optional_param_distributive() {
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

    // T extends { (x?: infer R): void } ? R : never, with T = { (x?: string): void }
    // | { (x?: number): void }.
    let extends_callable = interner.callable(CallableShape {
        symbol: None,
        is_abstract: false,
        call_signatures: vec![CallSignature {
            params: vec![ParamInfo {
                name: None,
                type_id: infer_r,
                optional: true,
                rest: false,
            }],
            this_type: None,
            return_type: TypeId::VOID,
            type_predicate: None,
            type_params: Vec::new(),
            is_method: false,
        }],
        construct_signatures: Vec::new(),
        properties: Vec::new(),
        string_index: None,
        number_index: None,
    });
    let cond = ConditionalType {
        check_type: t_param,
        extends_type: extends_callable,
        true_type: infer_r,
        false_type: TypeId::NEVER,
        is_distributive: true,
    };

    let cond_type = interner.conditional(cond);
    let mut subst = TypeSubstitution::new();
    let string_callable = interner.callable(CallableShape {
        symbol: None,
        is_abstract: false,
        call_signatures: vec![CallSignature {
            params: vec![ParamInfo {
                name: None,
                type_id: TypeId::STRING,
                optional: true,
                rest: false,
            }],
            this_type: None,
            return_type: TypeId::VOID,
            type_predicate: None,
            type_params: Vec::new(),
            is_method: false,
        }],
        construct_signatures: Vec::new(),
        properties: Vec::new(),
        string_index: None,
        number_index: None,
    });
    let number_callable = interner.callable(CallableShape {
        symbol: None,
        is_abstract: false,
        call_signatures: vec![CallSignature {
            params: vec![ParamInfo {
                name: None,
                type_id: TypeId::NUMBER,
                optional: true,
                rest: false,
            }],
            this_type: None,
            return_type: TypeId::VOID,
            type_predicate: None,
            type_params: Vec::new(),
            is_method: false,
        }],
        construct_signatures: Vec::new(),
        properties: Vec::new(),
        string_index: None,
        number_index: None,
    });
    subst.insert(
        t_name,
        interner.union(vec![string_callable, number_callable]),
    );

    let instantiated = instantiate_type(&interner, cond_type, &subst);
    let result = evaluate_type(&interner, instantiated);

    let expected = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);
    assert_eq!(result, expected);
}

#[test]
fn test_conditional_infer_object_call_signature_optional_param_non_distributive_union_input() {
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

    // [T] extends [{ (x?: infer R): void }] ? R : never, with T = { (x?: string): void }
    // | { (x?: number): void }.
    let extends_callable = interner.callable(CallableShape {
        symbol: None,
        is_abstract: false,
        call_signatures: vec![CallSignature {
            params: vec![ParamInfo {
                name: None,
                type_id: infer_r,
                optional: true,
                rest: false,
            }],
            this_type: None,
            return_type: TypeId::VOID,
            type_predicate: None,
            type_params: Vec::new(),
            is_method: false,
        }],
        construct_signatures: Vec::new(),
        properties: Vec::new(),
        string_index: None,
        number_index: None,
    });
    let cond = ConditionalType {
        check_type: interner.tuple(vec![TupleElement {
            type_id: t_param,
            name: None,
            optional: false,
            rest: false,
        }]),
        extends_type: interner.tuple(vec![TupleElement {
            type_id: extends_callable,
            name: None,
            optional: false,
            rest: false,
        }]),
        true_type: infer_r,
        false_type: TypeId::NEVER,
        is_distributive: false,
    };

    let cond_type = interner.conditional(cond);
    let mut subst = TypeSubstitution::new();
    let string_callable = interner.callable(CallableShape {
        symbol: None,
        is_abstract: false,
        call_signatures: vec![CallSignature {
            params: vec![ParamInfo {
                name: None,
                type_id: TypeId::STRING,
                optional: true,
                rest: false,
            }],
            this_type: None,
            return_type: TypeId::VOID,
            type_predicate: None,
            type_params: Vec::new(),
            is_method: false,
        }],
        construct_signatures: Vec::new(),
        properties: Vec::new(),
        string_index: None,
        number_index: None,
    });
    let number_callable = interner.callable(CallableShape {
        symbol: None,
        is_abstract: false,
        call_signatures: vec![CallSignature {
            params: vec![ParamInfo {
                name: None,
                type_id: TypeId::NUMBER,
                optional: true,
                rest: false,
            }],
            this_type: None,
            return_type: TypeId::VOID,
            type_predicate: None,
            type_params: Vec::new(),
            is_method: false,
        }],
        construct_signatures: Vec::new(),
        properties: Vec::new(),
        string_index: None,
        number_index: None,
    });
    subst.insert(
        t_name,
        interner.union(vec![string_callable, number_callable]),
    );

    let instantiated = instantiate_type(&interner, cond_type, &subst);
    let result = evaluate_type(&interner, instantiated);

    // Contravariant: infer in call signature optional parameter from union source
    let expected = TypeId::NEVER;
    assert_eq!(result, expected);
}

#[test]
fn test_conditional_infer_object_call_signature_rest_param_distributive() {
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

    // T extends { (...args: infer R): void } ? R : never, with T = { (...args: string[]): void }
    // | { (...args: number[]): void }.
    let extends_callable = interner.callable(CallableShape {
        symbol: None,
        is_abstract: false,
        call_signatures: vec![CallSignature {
            params: vec![ParamInfo {
                name: None,
                type_id: infer_r,
                optional: false,
                rest: true,
            }],
            this_type: None,
            return_type: TypeId::VOID,
            type_predicate: None,
            type_params: Vec::new(),
            is_method: false,
        }],
        construct_signatures: Vec::new(),
        properties: Vec::new(),
        string_index: None,
        number_index: None,
    });
    let cond = ConditionalType {
        check_type: t_param,
        extends_type: extends_callable,
        true_type: infer_r,
        false_type: TypeId::NEVER,
        is_distributive: true,
    };

    let cond_type = interner.conditional(cond);
    let mut subst = TypeSubstitution::new();
    let string_callable = interner.callable(CallableShape {
        symbol: None,
        is_abstract: false,
        call_signatures: vec![CallSignature {
            params: vec![ParamInfo {
                name: None,
                type_id: interner.array(TypeId::STRING),
                optional: false,
                rest: true,
            }],
            this_type: None,
            return_type: TypeId::VOID,
            type_predicate: None,
            type_params: Vec::new(),
            is_method: false,
        }],
        construct_signatures: Vec::new(),
        properties: Vec::new(),
        string_index: None,
        number_index: None,
    });
    let number_callable = interner.callable(CallableShape {
        symbol: None,
        is_abstract: false,
        call_signatures: vec![CallSignature {
            params: vec![ParamInfo {
                name: None,
                type_id: interner.array(TypeId::NUMBER),
                optional: false,
                rest: true,
            }],
            this_type: None,
            return_type: TypeId::VOID,
            type_predicate: None,
            type_params: Vec::new(),
            is_method: false,
        }],
        construct_signatures: Vec::new(),
        properties: Vec::new(),
        string_index: None,
        number_index: None,
    });
    subst.insert(
        t_name,
        interner.union(vec![string_callable, number_callable]),
    );

    let instantiated = instantiate_type(&interner, cond_type, &subst);
    let result = evaluate_type(&interner, instantiated);

    let expected = interner.union(vec![
        interner.array(TypeId::STRING),
        interner.array(TypeId::NUMBER),
    ]);
    assert_eq!(result, expected);
}

#[test]
fn test_conditional_infer_object_call_signature_rest_param_non_distributive_union_input() {
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

    // [T] extends [{ (...args: infer R): void }] ? R : never, with T = { (...args: string[]): void }
    // | { (...args: number[]): void }.
    let extends_callable = interner.callable(CallableShape {
        symbol: None,
        is_abstract: false,
        call_signatures: vec![CallSignature {
            params: vec![ParamInfo {
                name: None,
                type_id: infer_r,
                optional: false,
                rest: true,
            }],
            this_type: None,
            return_type: TypeId::VOID,
            type_predicate: None,
            type_params: Vec::new(),
            is_method: false,
        }],
        construct_signatures: Vec::new(),
        properties: Vec::new(),
        string_index: None,
        number_index: None,
    });
    let cond = ConditionalType {
        check_type: interner.tuple(vec![TupleElement {
            type_id: t_param,
            name: None,
            optional: false,
            rest: false,
        }]),
        extends_type: interner.tuple(vec![TupleElement {
            type_id: extends_callable,
            name: None,
            optional: false,
            rest: false,
        }]),
        true_type: infer_r,
        false_type: TypeId::NEVER,
        is_distributive: false,
    };

    let cond_type = interner.conditional(cond);
    let mut subst = TypeSubstitution::new();
    let string_callable = interner.callable(CallableShape {
        symbol: None,
        is_abstract: false,
        call_signatures: vec![CallSignature {
            params: vec![ParamInfo {
                name: None,
                type_id: interner.array(TypeId::STRING),
                optional: false,
                rest: true,
            }],
            this_type: None,
            return_type: TypeId::VOID,
            type_predicate: None,
            type_params: Vec::new(),
            is_method: false,
        }],
        construct_signatures: Vec::new(),
        properties: Vec::new(),
        string_index: None,
        number_index: None,
    });
    let number_callable = interner.callable(CallableShape {
        symbol: None,
        is_abstract: false,
        call_signatures: vec![CallSignature {
            params: vec![ParamInfo {
                name: None,
                type_id: interner.array(TypeId::NUMBER),
                optional: false,
                rest: true,
            }],
            this_type: None,
            return_type: TypeId::VOID,
            type_predicate: None,
            type_params: Vec::new(),
            is_method: false,
        }],
        construct_signatures: Vec::new(),
        properties: Vec::new(),
        string_index: None,
        number_index: None,
    });
    subst.insert(
        t_name,
        interner.union(vec![string_callable, number_callable]),
    );

    let instantiated = instantiate_type(&interner, cond_type, &subst);
    let result = evaluate_type(&interner, instantiated);

    // Contravariant: infer in call signature rest parameter from union source
    let expected = interner.intersection(vec![
        interner.array(TypeId::STRING),
        interner.array(TypeId::NUMBER),
    ]);
    assert_eq!(result, expected);
}


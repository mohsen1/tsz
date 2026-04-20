// Tests for type operations.

use super::*;
use crate::CompatChecker;
use crate::def::DefId;
use crate::intern::TypeInterner;
use crate::operations::core::MAX_CONSTRAINT_STEPS;
use crate::operations::property::{PropertyAccessEvaluator, PropertyAccessResult};
use crate::types::{CallableShape, MappedType, TypeData, Visibility};

#[test]
fn test_call_simple_function() {
    let interner = TypeInterner::new();
    let mut subtype = CompatChecker::new(&interner);
    let mut evaluator = CallEvaluator::new(&interner, &mut subtype);

    // function(x: number): string
    let func = interner.function(FunctionShape {
        params: vec![ParamInfo {
            name: Some(interner.intern_string("x")),
            type_id: TypeId::NUMBER,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: TypeId::STRING,
        type_params: Vec::new(),
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    // Call with correct args
    let result = evaluator.resolve_call(func, &[TypeId::NUMBER]);
    match result {
        CallResult::Success(ret) => assert_eq!(ret, TypeId::STRING),
        _ => panic!("Expected success, got {result:?}"),
    }
}
#[test]
fn test_call_argument_count_mismatch() {
    let interner = TypeInterner::new();
    let mut subtype = CompatChecker::new(&interner);
    let mut evaluator = CallEvaluator::new(&interner, &mut subtype);

    // function(x: number): string
    let func = interner.function(FunctionShape {
        params: vec![ParamInfo {
            name: Some(interner.intern_string("x")),
            type_id: TypeId::NUMBER,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: TypeId::STRING,
        type_params: Vec::new(),
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    // Call with no args
    let result = evaluator.resolve_call(func, &[]);
    match result {
        CallResult::ArgumentCountMismatch {
            expected_min,
            actual,
            ..
        } => {
            assert_eq!(expected_min, 1);
            assert_eq!(actual, 0);
        }
        _ => panic!("Expected ArgumentCountMismatch, got {result:?}"),
    }
}
#[test]
fn test_call_argument_type_mismatch() {
    let interner = TypeInterner::new();
    let mut subtype = CompatChecker::new(&interner);
    let mut evaluator = CallEvaluator::new(&interner, &mut subtype);

    // function(x: number): string
    let func = interner.function(FunctionShape {
        params: vec![ParamInfo {
            name: Some(interner.intern_string("x")),
            type_id: TypeId::NUMBER,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: TypeId::STRING,
        type_params: Vec::new(),
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    // Call with wrong type
    let result = evaluator.resolve_call(func, &[TypeId::STRING]);
    match result {
        CallResult::ArgumentTypeMismatch {
            index,
            expected,
            actual,
            ..
        } => {
            assert_eq!(index, 0);
            assert_eq!(expected, TypeId::NUMBER);
            assert_eq!(actual, TypeId::STRING);
        }
        _ => panic!("Expected ArgumentTypeMismatch, got {result:?}"),
    }
}
#[test]
fn test_call_assignability_respects_strict_function_types_toggle() {
    let interner = TypeInterner::new();

    let name = interner.intern_string("name");
    let breed = interner.intern_string("breed");

    let animal = interner.object(vec![PropertyInfo {
        name,
        type_id: TypeId::STRING,
        write_type: TypeId::STRING,
        optional: false,
        readonly: false,
        is_method: false,
        is_class_prototype: false,
        visibility: Visibility::Public,
        parent_id: None,
        declaration_order: 0,
        is_string_named: false,
    }]);
    let dog = interner.object(vec![
        PropertyInfo {
            name,
            type_id: TypeId::STRING,
            write_type: TypeId::STRING,
            optional: false,
            readonly: false,
            is_method: false,
            is_class_prototype: false,
            visibility: Visibility::Public,
            parent_id: None,
            declaration_order: 0,
            is_string_named: false,
        },
        PropertyInfo::new(breed, TypeId::STRING),
    ]);

    let fn_animal = interner.function(FunctionShape {
        params: vec![ParamInfo::unnamed(animal)],
        this_type: None,
        return_type: TypeId::VOID,
        type_params: Vec::new(),
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });
    let fn_dog = interner.function(FunctionShape {
        params: vec![ParamInfo::unnamed(dog)],
        this_type: None,
        return_type: TypeId::VOID,
        type_params: Vec::new(),
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let accepts_fn = interner.function(FunctionShape {
        params: vec![ParamInfo {
            name: Some(interner.intern_string("cb")),
            type_id: fn_animal,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: TypeId::VOID,
        type_params: Vec::new(),
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let mut checker = CompatChecker::new(&interner);
    {
        let mut evaluator = CallEvaluator::new(&interner, &mut checker);
        let result = evaluator.resolve_call(accepts_fn, &[fn_dog]);
        assert!(matches!(result, CallResult::Success(_)));
    }

    checker.set_strict_function_types(true);
    {
        let mut evaluator = CallEvaluator::new(&interner, &mut checker);
        let result = evaluator.resolve_call(accepts_fn, &[fn_dog]);
        assert!(matches!(result, CallResult::ArgumentTypeMismatch { .. }));
    }
}
#[test]
fn test_call_weak_type_with_compat_checker() {
    let interner = TypeInterner::new();
    let mut checker = CompatChecker::new(&interner);
    let mut evaluator = CallEvaluator::new(&interner, &mut checker);

    let weak_target = interner.object(vec![PropertyInfo::opt(
        interner.intern_string("a"),
        TypeId::NUMBER,
    )]);
    let func = interner.function(FunctionShape {
        params: vec![ParamInfo {
            name: Some(interner.intern_string("arg")),
            type_id: weak_target,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: TypeId::VOID,
        type_params: Vec::new(),
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let arg = interner.object(vec![PropertyInfo::new(
        interner.intern_string("b"),
        TypeId::NUMBER,
    )]);

    let result = evaluator.resolve_call(func, &[arg]);
    assert!(matches!(result, CallResult::ArgumentTypeMismatch { .. }));
}
#[test]
fn test_generic_call_resets_constraint_step_budget() {
    let interner = TypeInterner::new();
    let mut checker = CompatChecker::new(&interner);
    let mut evaluator = CallEvaluator::new(&interner, &mut checker);

    let tp_name = interner.intern_string("T");
    let tp = TypeParamInfo {
        is_const: false,
        name: tp_name,
        constraint: None,
        default: None,
    };
    let tp_id = interner.type_param(tp);
    let identity = interner.function(FunctionShape {
        params: vec![ParamInfo::unnamed(tp_id)],
        this_type: None,
        return_type: tp_id,
        type_params: vec![tp],
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    evaluator.constraint_step_count.set(MAX_CONSTRAINT_STEPS);

    let result = evaluator.resolve_call(identity, &[TypeId::STRING]);
    match result {
        CallResult::Success(ret) => assert_eq!(ret, TypeId::STRING),
        _ => panic!("Expected successful generic inference, got {result:?}"),
    }
}

/// When a non-const type parameter has a constraint that the widened argument
/// type would violate, the solver should fall back to the unwidened (literal)
/// argument type. This prevents false TS2322 errors like:
///   `<T extends [string, string, 'a' | 'b']>(x: T): T`
///   called with `["x", "y", "a"]` → T should be `["x", "y", "a"]` not `[string, string, string]`
#[test]
fn test_generic_call_widening_falls_back_when_constraint_violated() {
    let interner = TypeInterner::new();
    let mut checker = CompatChecker::new(&interner);
    let mut evaluator = CallEvaluator::new(&interner, &mut checker);

    // Build constraint: [string, 'a' | 'b']
    let a_lit = interner.literal_string("a");
    let b_lit = interner.literal_string("b");
    let ab_union = interner.union(vec![a_lit, b_lit]);
    let constraint = interner.tuple(vec![
        TupleElement {
            type_id: TypeId::STRING,
            name: None,
            optional: false,
            rest: false,
        },
        TupleElement {
            type_id: ab_union,
            name: None,
            optional: false,
            rest: false,
        },
    ]);

    // Build: <T extends [string, 'a' | 'b']>(x: T): T
    let tp_name = interner.intern_string("T");
    let tp = TypeParamInfo {
        is_const: false,
        name: tp_name,
        constraint: Some(constraint),
        default: None,
    };
    let tp_id = interner.type_param(tp);
    let func = interner.function(FunctionShape {
        params: vec![ParamInfo::unnamed(tp_id)],
        this_type: None,
        return_type: tp_id,
        type_params: vec![tp],
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    // Call with ["hello", "a"] — literal tuple
    let hello_lit = interner.literal_string("hello");
    let arg = interner.tuple(vec![
        TupleElement {
            type_id: hello_lit,
            name: None,
            optional: false,
            rest: false,
        },
        TupleElement {
            type_id: a_lit,
            name: None,
            optional: false,
            rest: false,
        },
    ]);

    let result = evaluator.resolve_call(func, &[arg]);
    match result {
        CallResult::Success(ret) => {
            // The return type should be the unwidened literal tuple,
            // because widening to [string, string] would violate the constraint
            assert_eq!(ret, arg, "Expected unwidened literal tuple as return type");
        }
        other => panic!("Expected Success with literal tuple, got {other:?}"),
    }
}

/// When widening does NOT violate the constraint, the widened type should be used.
#[test]
fn test_generic_call_widening_applies_when_constraint_satisfied() {
    let interner = TypeInterner::new();
    let mut checker = CompatChecker::new(&interner);
    let mut evaluator = CallEvaluator::new(&interner, &mut checker);

    // Build constraint: [string, string]
    let constraint = interner.tuple(vec![
        TupleElement {
            type_id: TypeId::STRING,
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

    // Build: <T extends [string, string]>(x: T): T
    let tp_name = interner.intern_string("T");
    let tp = TypeParamInfo {
        is_const: false,
        name: tp_name,
        constraint: Some(constraint),
        default: None,
    };
    let tp_id = interner.type_param(tp);
    let func = interner.function(FunctionShape {
        params: vec![ParamInfo::unnamed(tp_id)],
        this_type: None,
        return_type: tp_id,
        type_params: vec![tp],
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    // Call with ["hello", "world"] — literal tuple
    let hello_lit = interner.literal_string("hello");
    let world_lit = interner.literal_string("world");
    let arg = interner.tuple(vec![
        TupleElement {
            type_id: hello_lit,
            name: None,
            optional: false,
            rest: false,
        },
        TupleElement {
            type_id: world_lit,
            name: None,
            optional: false,
            rest: false,
        },
    ]);

    let result = evaluator.resolve_call(func, &[arg]);
    match result {
        CallResult::Success(ret) => {
            // Widening ["hello", "world"] → [string, string] satisfies constraint,
            // so the widened type should be used
            let expected = interner.tuple(vec![
                TupleElement {
                    type_id: TypeId::STRING,
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
            assert_eq!(ret, expected, "Expected widened tuple as return type");
        }
        other => panic!("Expected Success with widened tuple, got {other:?}"),
    }
}
#[test]
fn test_get_contextual_signature_with_compat_checker_matches_call_evaluator() {
    let interner = TypeInterner::new();
    let contextual = interner.function(FunctionShape {
        params: vec![ParamInfo::unnamed(TypeId::STRING)],
        this_type: None,
        return_type: TypeId::NUMBER,
        type_params: Vec::new(),
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let via_helper = get_contextual_signature_with_compat_checker(&interner, contextual);
    let via_evaluator =
        CallEvaluator::<CompatChecker>::get_contextual_signature(&interner, contextual);

    assert_eq!(via_helper, via_evaluator);
    let sig = via_helper.expect("expected contextual signature");
    assert_eq!(sig.params.len(), 1);
    assert_eq!(sig.params[0].type_id, TypeId::STRING);
    assert_eq!(sig.return_type, TypeId::NUMBER);
}
#[test]
fn test_call_rest_parameter_allows_zero_args() {
    let interner = TypeInterner::new();
    let mut subtype = CompatChecker::new(&interner);
    let mut evaluator = CallEvaluator::new(&interner, &mut subtype);

    // function(...args: number[]): string
    let rest_array = interner.array(TypeId::NUMBER);
    let func = interner.function(FunctionShape {
        params: vec![ParamInfo {
            name: Some(interner.intern_string("args")),
            type_id: rest_array,
            optional: false,
            rest: true,
        }],
        this_type: None,
        return_type: TypeId::STRING,
        type_params: Vec::new(),
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let result = evaluator.resolve_call(func, &[]);
    match result {
        CallResult::Success(ret) => assert_eq!(ret, TypeId::STRING),
        _ => panic!("Expected success, got {result:?}"),
    }
}
#[test]
fn test_call_rest_parameter_min_args_with_required() {
    let interner = TypeInterner::new();
    let mut subtype = CompatChecker::new(&interner);
    let mut evaluator = CallEvaluator::new(&interner, &mut subtype);

    // function(x: string, ...args: number[]): string
    let rest_array = interner.array(TypeId::NUMBER);
    let func = interner.function(FunctionShape {
        params: vec![
            ParamInfo {
                name: Some(interner.intern_string("x")),
                type_id: TypeId::STRING,
                optional: false,
                rest: false,
            },
            ParamInfo {
                name: Some(interner.intern_string("args")),
                type_id: rest_array,
                optional: false,
                rest: true,
            },
        ],
        this_type: None,
        return_type: TypeId::STRING,
        type_params: Vec::new(),
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let result = evaluator.resolve_call(func, &[]);
    match result {
        CallResult::ArgumentCountMismatch {
            expected_min,
            actual,
            ..
        } => {
            assert_eq!(expected_min, 1);
            assert_eq!(actual, 0);
        }
        _ => panic!("Expected ArgumentCountMismatch, got {result:?}"),
    }
}
#[test]
fn test_binary_equality_disjoint_primitives_returns_boolean() {
    // Equality operators always return boolean regardless of operand types.
    // TS2367 diagnostics are the checker's responsibility, not the evaluator's.
    let interner = TypeInterner::new();
    let evaluator = BinaryOpEvaluator::new(&interner);

    let result = evaluator.evaluate(TypeId::STRING, TypeId::NUMBER, "===");
    assert!(matches!(result, BinaryOpResult::Success(TypeId::BOOLEAN)));

    let result = evaluator.evaluate(TypeId::NUMBER, TypeId::UNDEFINED, "!==");
    assert!(matches!(result, BinaryOpResult::Success(TypeId::BOOLEAN)));

    let result = evaluator.evaluate(TypeId::STRING, TypeId::NULL, "===");
    assert!(matches!(result, BinaryOpResult::Success(TypeId::BOOLEAN)));
}
#[test]
fn test_binary_equality_disjoint_primitives_loose_returns_boolean() {
    let interner = TypeInterner::new();
    let evaluator = BinaryOpEvaluator::new(&interner);

    let result = evaluator.evaluate(TypeId::STRING, TypeId::NUMBER, "==");
    assert!(matches!(result, BinaryOpResult::Success(TypeId::BOOLEAN)));

    let result = evaluator.evaluate(TypeId::BOOLEAN, TypeId::UNDEFINED, "!=");
    assert!(matches!(result, BinaryOpResult::Success(TypeId::BOOLEAN)));
}
#[test]
fn test_binary_equality_disjoint_literals_returns_boolean() {
    let interner = TypeInterner::new();
    let evaluator = BinaryOpEvaluator::new(&interner);

    let one = interner.literal_number(1.0);
    let two = interner.literal_number(2.0);

    let result = evaluator.evaluate(one, two, "===");
    assert!(matches!(result, BinaryOpResult::Success(TypeId::BOOLEAN)));

    let result = evaluator.evaluate(one, two, "!==");
    assert!(matches!(result, BinaryOpResult::Success(TypeId::BOOLEAN)));
}
#[test]
fn test_binary_overlap_union_literals() {
    let interner = TypeInterner::new();
    let evaluator = BinaryOpEvaluator::new(&interner);

    let lit_a = interner.literal_string("a");
    let lit_b = interner.literal_string("b");
    let lit_c = interner.literal_string("c");

    let left = interner.union(vec![lit_a, lit_b]);
    let right = interner.union(vec![lit_b, lit_c]);

    let result = evaluator.evaluate(left, right, "===");
    match result {
        BinaryOpResult::Success(result_type) => assert_eq!(result_type, TypeId::BOOLEAN),
        _ => panic!("Expected boolean result, got {result:?}"),
    }
}
#[test]
fn test_binary_overlap_with_any_unknown_never() {
    let interner = TypeInterner::new();
    let evaluator = BinaryOpEvaluator::new(&interner);

    let any_result = evaluator.evaluate(TypeId::ANY, TypeId::NUMBER, "===");
    assert!(matches!(
        any_result,
        BinaryOpResult::Success(TypeId::BOOLEAN)
    ));

    let unknown_result = evaluator.evaluate(TypeId::UNKNOWN, TypeId::NUMBER, "===");
    assert!(matches!(
        unknown_result,
        BinaryOpResult::Success(TypeId::BOOLEAN)
    ));

    // `never` is the bottom type — any operation on `never` produces `never`, not a type error.
    let never_result = evaluator.evaluate(TypeId::NEVER, TypeId::NUMBER, "===");
    assert!(matches!(
        never_result,
        BinaryOpResult::Success(TypeId::NEVER)
    ));
}
#[test]
fn test_binary_overlap_template_literal() {
    let interner = TypeInterner::new();
    let evaluator = BinaryOpEvaluator::new(&interner);

    let template = interner.template_literal(vec![
        TemplateSpan::Text(interner.intern_string("prefix")),
        TemplateSpan::Type(TypeId::STRING),
        TemplateSpan::Text(interner.intern_string("suffix")),
    ]);

    let ok_result = evaluator.evaluate(template, TypeId::STRING, "===");
    assert!(matches!(
        ok_result,
        BinaryOpResult::Success(TypeId::BOOLEAN)
    ));

    // Even non-overlapping equality comparisons produce boolean
    let non_overlap_result = evaluator.evaluate(template, TypeId::NUMBER, "===");
    assert!(matches!(
        non_overlap_result,
        BinaryOpResult::Success(TypeId::BOOLEAN)
    ));
}
#[test]
fn test_binary_equality_generic_constraint_disjoint_still_boolean() {
    // Even when a type parameter's constraint is disjoint from the other operand,
    // equality operators still produce boolean. TS2367 is the checker's job.
    let interner = TypeInterner::new();
    let evaluator = BinaryOpEvaluator::new(&interner);

    let type_param = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: interner.intern_string("T"),
        constraint: Some(TypeId::STRING),
        default: None,
        is_const: false,
    }));

    let result = evaluator.evaluate(type_param, TypeId::NUMBER, "===");
    assert!(matches!(result, BinaryOpResult::Success(TypeId::BOOLEAN)));
}
#[test]
fn test_binary_overlap_generic_constraint_overlap() {
    let interner = TypeInterner::new();
    let evaluator = BinaryOpEvaluator::new(&interner);

    let type_param = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: interner.intern_string("T"),
        constraint: Some(TypeId::STRING),
        default: None,
        is_const: false,
    }));

    let result = evaluator.evaluate(type_param, TypeId::STRING, "===");
    match result {
        BinaryOpResult::Success(result_type) => assert_eq!(result_type, TypeId::BOOLEAN),
        _ => panic!("Expected boolean result, got {result:?}"),
    }
}
#[test]
fn test_binary_overlap_unconstrained_type_param() {
    let interner = TypeInterner::new();
    let evaluator = BinaryOpEvaluator::new(&interner);

    let type_param = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: interner.intern_string("T"),
        constraint: None,
        default: None,
        is_const: false,
    }));

    let result = evaluator.evaluate(type_param, TypeId::NUMBER, "===");
    match result {
        BinaryOpResult::Success(result_type) => assert_eq!(result_type, TypeId::BOOLEAN),
        _ => panic!("Expected boolean result, got {result:?}"),
    }
}
#[test]
fn test_binary_equality_union_constraint_disjoint_still_boolean() {
    // Same principle: disjoint union constraint doesn't affect equality result type.
    let interner = TypeInterner::new();
    let evaluator = BinaryOpEvaluator::new(&interner);

    let constraint = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);
    let type_param = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: interner.intern_string("T"),
        constraint: Some(constraint),
        default: None,
        is_const: false,
    }));

    let result = evaluator.evaluate(type_param, TypeId::BOOLEAN, "===");
    assert!(matches!(result, BinaryOpResult::Success(TypeId::BOOLEAN)));
}
#[test]
fn test_binary_overlap_union_constraint_overlap() {
    let interner = TypeInterner::new();
    let evaluator = BinaryOpEvaluator::new(&interner);

    let constraint = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);
    let type_param = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: interner.intern_string("T"),
        constraint: Some(constraint),
        default: None,
        is_const: false,
    }));

    let result = evaluator.evaluate(type_param, TypeId::NUMBER, "===");
    match result {
        BinaryOpResult::Success(result_type) => assert_eq!(result_type, TypeId::BOOLEAN),
        _ => panic!("Expected boolean result, got {result:?}"),
    }
}
#[test]
fn test_binary_logical_and_contextual_callable_result() {
    let interner = TypeInterner::new();
    let evaluator = BinaryOpEvaluator::new(&interner);

    let contextual_fn = interner.function(FunctionShape {
        params: vec![ParamInfo::unnamed(TypeId::STRING)],
        this_type: None,
        return_type: TypeId::STRING,
        type_params: vec![],
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });
    let right_fn = interner.function(FunctionShape {
        params: vec![ParamInfo::unnamed(TypeId::STRING)],
        this_type: None,
        return_type: TypeId::NUMBER,
        type_params: vec![],
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });
    let left_true = interner.literal_boolean(true);

    let result = evaluator.evaluate_with_context(left_true, right_fn, "&&", Some(contextual_fn));
    match result {
        BinaryOpResult::Success(result_type) => assert_eq!(result_type, right_fn),
        _ => panic!("Expected callable result, got {result:?}"),
    }
}
#[test]
fn test_binary_logical_and_contextual_callable_false_left_preserves_false() {
    let interner = TypeInterner::new();
    let evaluator = BinaryOpEvaluator::new(&interner);

    let contextual_fn = interner.function(FunctionShape {
        params: vec![ParamInfo::unnamed(TypeId::STRING)],
        this_type: None,
        return_type: TypeId::STRING,
        type_params: vec![],
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });
    let right_fn = interner.function(FunctionShape {
        params: vec![ParamInfo::unnamed(TypeId::STRING)],
        this_type: None,
        return_type: TypeId::NUMBER,
        type_params: vec![],
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });
    let left_false = interner.literal_boolean(false);

    let result = evaluator.evaluate_with_context(left_false, right_fn, "&&", Some(contextual_fn));
    match result {
        BinaryOpResult::Success(result_type) => assert_eq!(result_type, left_false),
        _ => panic!("Expected false result, got {result:?}"),
    }
}
#[test]
fn test_binary_logical_and_with_boolean_produces_false_union() {
    // Verifies that `boolean && object_type` produces `false | object_type`,
    // which is critical for spread patterns like `...condition && { prop: value }`.
    // The spread checker filters out definitely-falsy types from unions, so
    // `false | { a: string }` is a valid spread type, but `unknown | { a: string }` is not.
    let interner = TypeInterner::new();
    let evaluator = BinaryOpEvaluator::new(&interner);

    let a_name = interner.intern_string("a");
    let obj = interner.object(vec![PropertyInfo::new(a_name, TypeId::STRING)]);

    // boolean && { a: string } should produce false | { a: string }
    let result = evaluator.evaluate(TypeId::BOOLEAN, obj, "&&");
    match result {
        BinaryOpResult::Success(result_type) => {
            // The result should be a union containing false and the object type
            let data = interner.lookup(result_type);
            assert!(
                matches!(data, Some(TypeData::Union(_))),
                "Expected union type, got {data:?}"
            );
        }
        _ => panic!("Expected success result, got {result:?}"),
    }
}
#[test]
fn test_call_rest_parameter_type_match() {
    let interner = TypeInterner::new();
    let mut subtype = CompatChecker::new(&interner);
    let mut evaluator = CallEvaluator::new(&interner, &mut subtype);

    // function(...args: number[]): string
    let rest_array = interner.array(TypeId::NUMBER);
    let func = interner.function(FunctionShape {
        params: vec![ParamInfo {
            name: Some(interner.intern_string("args")),
            type_id: rest_array,
            optional: false,
            rest: true,
        }],
        this_type: None,
        return_type: TypeId::STRING,
        type_params: Vec::new(),
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let result = evaluator.resolve_call(func, &[TypeId::NUMBER, TypeId::NUMBER]);
    match result {
        CallResult::Success(ret) => assert_eq!(ret, TypeId::STRING),
        _ => panic!("Expected success, got {result:?}"),
    }
}
#[test]
fn test_call_rest_parameter_type_mismatch() {
    let interner = TypeInterner::new();
    let mut subtype = CompatChecker::new(&interner);
    let mut evaluator = CallEvaluator::new(&interner, &mut subtype);

    // function(...args: number[]): string
    let rest_array = interner.array(TypeId::NUMBER);
    let func = interner.function(FunctionShape {
        params: vec![ParamInfo {
            name: Some(interner.intern_string("args")),
            type_id: rest_array,
            optional: false,
            rest: true,
        }],
        this_type: None,
        return_type: TypeId::STRING,
        type_params: Vec::new(),
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let result = evaluator.resolve_call(func, &[TypeId::NUMBER, TypeId::STRING]);
    match result {
        CallResult::ArgumentTypeMismatch {
            index,
            expected,
            actual,
            ..
        } => {
            assert_eq!(index, 1);
            assert_eq!(expected, TypeId::NUMBER);
            assert_eq!(actual, TypeId::STRING);
        }
        _ => panic!("Expected ArgumentTypeMismatch, got {result:?}"),
    }
}
#[test]
fn test_call_tuple_rest_argument_count_mismatch() {
    let interner = TypeInterner::new();
    let mut subtype = CompatChecker::new(&interner);
    let mut evaluator = CallEvaluator::new(&interner, &mut subtype);

    let tuple_rest = interner.tuple(vec![
        TupleElement {
            type_id: TypeId::NUMBER,
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

    let func = interner.function(FunctionShape {
        params: vec![ParamInfo {
            name: Some(interner.intern_string("args")),
            type_id: tuple_rest,
            optional: false,
            rest: true,
        }],
        this_type: None,
        return_type: TypeId::STRING,
        type_params: Vec::new(),
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let result = evaluator.resolve_call(func, &[TypeId::NUMBER]);
    match result {
        CallResult::ArgumentCountMismatch {
            expected_min,
            actual,
            ..
        } => {
            assert_eq!(expected_min, 2);
            assert_eq!(actual, 1);
        }
        _ => panic!("Expected ArgumentCountMismatch, got {result:?}"),
    }
}
#[test]
fn test_call_tuple_rest_argument_type_mismatch() {
    let interner = TypeInterner::new();
    let mut subtype = CompatChecker::new(&interner);
    let mut evaluator = CallEvaluator::new(&interner, &mut subtype);

    let tuple_rest = interner.tuple(vec![
        TupleElement {
            type_id: TypeId::NUMBER,
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

    let func = interner.function(FunctionShape {
        params: vec![ParamInfo {
            name: Some(interner.intern_string("args")),
            type_id: tuple_rest,
            optional: false,
            rest: true,
        }],
        this_type: None,
        return_type: TypeId::STRING,
        type_params: Vec::new(),
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let result = evaluator.resolve_call(func, &[TypeId::NUMBER, TypeId::BOOLEAN]);
    match result {
        CallResult::ArgumentTypeMismatch {
            index,
            expected,
            actual,
            ..
        } => {
            assert_eq!(index, 1);
            assert_eq!(expected, TypeId::STRING);
            assert_eq!(actual, TypeId::BOOLEAN);
        }
        _ => panic!("Expected ArgumentTypeMismatch, got {result:?}"),
    }
}
#[test]
fn test_call_tuple_rest_argument_success() {
    let interner = TypeInterner::new();
    let mut subtype = CompatChecker::new(&interner);
    let mut evaluator = CallEvaluator::new(&interner, &mut subtype);

    let tuple_rest = interner.tuple(vec![
        TupleElement {
            type_id: TypeId::NUMBER,
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

    let func = interner.function(FunctionShape {
        params: vec![ParamInfo {
            name: Some(interner.intern_string("args")),
            type_id: tuple_rest,
            optional: false,
            rest: true,
        }],
        this_type: None,
        return_type: TypeId::STRING,
        type_params: Vec::new(),
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let result = evaluator.resolve_call(func, &[TypeId::NUMBER, TypeId::STRING]);
    match result {
        CallResult::Success(ret) => assert_eq!(ret, TypeId::STRING),
        _ => panic!("Expected success, got {result:?}"),
    }
}
#[test]
fn test_call_tuple_rest_with_fixed_tail() {
    let interner = TypeInterner::new();
    let mut subtype = CompatChecker::new(&interner);
    let mut evaluator = CallEvaluator::new(&interner, &mut subtype);

    let rest_array = interner.array(TypeId::STRING);
    let tuple_rest = interner.tuple(vec![
        TupleElement {
            type_id: rest_array,
            name: None,
            optional: false,
            rest: true,
        },
        TupleElement {
            type_id: TypeId::NUMBER,
            name: None,
            optional: false,
            rest: false,
        },
    ]);

    let func = interner.function(FunctionShape {
        params: vec![ParamInfo {
            name: Some(interner.intern_string("args")),
            type_id: tuple_rest,
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

    let result = evaluator.resolve_call(func, &[TypeId::NUMBER]);
    match result {
        CallResult::Success(ret) => assert_eq!(ret, TypeId::VOID),
        _ => panic!("Expected success, got {result:?}"),
    }

    let result = evaluator.resolve_call(func, &[TypeId::STRING, TypeId::STRING, TypeId::NUMBER]);
    match result {
        CallResult::Success(ret) => assert_eq!(ret, TypeId::VOID),
        _ => panic!("Expected success, got {result:?}"),
    }

    let result = evaluator.resolve_call(func, &[TypeId::STRING, TypeId::NUMBER, TypeId::STRING]);
    match result {
        CallResult::ArgumentTypeMismatch {
            index,
            expected,
            actual,
            ..
        } => {
            assert_eq!(index, 1);
            assert_eq!(expected, TypeId::STRING);
            assert_eq!(actual, TypeId::NUMBER);
        }
        _ => panic!("Expected ArgumentTypeMismatch, got {result:?}"),
    }
}

/// Calling a variadic tuple rest param function with too few args should produce
/// `ArgumentTypeMismatch` (TS2345), not `ArgumentCountMismatch` (TS2555).
/// E.g. `f1(...args: [...T[], Required])` called as `f1()` → TS2345.
#[test]
fn test_call_variadic_tuple_rest_empty_args_produces_type_mismatch() {
    let interner = TypeInterner::new();
    let mut subtype = CompatChecker::new(&interner);
    let mut evaluator = CallEvaluator::new(&interner, &mut subtype);

    // Build tuple type: [...((arg: number) => void)[], (arg: string) => void]
    let num_fn = interner.function(FunctionShape {
        params: vec![ParamInfo {
            name: Some(interner.intern_string("arg")),
            type_id: TypeId::NUMBER,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: TypeId::VOID,
        type_params: Vec::new(),
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });
    let str_fn = interner.function(FunctionShape {
        params: vec![ParamInfo {
            name: Some(interner.intern_string("arg")),
            type_id: TypeId::STRING,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: TypeId::VOID,
        type_params: Vec::new(),
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let rest_array = interner.array(num_fn);
    let tuple_type = interner.tuple(vec![
        TupleElement {
            type_id: rest_array,
            name: None,
            optional: false,
            rest: true,
        },
        TupleElement {
            type_id: str_fn,
            name: None,
            optional: false,
            rest: false,
        },
    ]);

    let func = interner.function(FunctionShape {
        params: vec![ParamInfo {
            name: Some(interner.intern_string("args")),
            type_id: tuple_type,
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

    // Call with 0 args — should get ArgumentTypeMismatch (TS2345), not ArgumentCountMismatch
    let result = evaluator.resolve_call(func, &[]);
    match result {
        CallResult::ArgumentTypeMismatch {
            expected, actual, ..
        } => {
            // Expected: the variadic tuple type
            assert_eq!(expected, tuple_type);
            // Actual: an empty tuple []
            assert!(
                matches!(interner.lookup(actual), Some(TypeData::Tuple(elems)) if interner.tuple_list(elems).is_empty()),
                "Expected empty tuple for actual, got {:?}",
                interner.lookup(actual)
            );
        }
        _ => panic!(
            "Expected ArgumentTypeMismatch for empty args to variadic tuple rest, got {result:?}"
        ),
    }

    // Call with 1 arg (the required trailing element) — should succeed
    let result = evaluator.resolve_call(func, &[str_fn]);
    match result {
        CallResult::Success(ret) => assert_eq!(ret, TypeId::VOID),
        _ => panic!("Expected success with 1 arg to variadic tuple rest, got {result:?}"),
    }
}
#[test]
fn test_property_access_on_never_returns_never() {
    // never is the bottom type — all property accesses are vacuously valid
    // and return never (the code is unreachable). tsc does not emit TS2339 on never.
    let interner = TypeInterner::new();
    let evaluator = PropertyAccessEvaluator::new(&interner);

    let result = evaluator.resolve_property_access(TypeId::NEVER, "anything");
    match result {
        PropertyAccessResult::Success { type_id: t, .. } => assert_eq!(t, TypeId::NEVER),
        _ => panic!("Property access on never should succeed with never, got {result:?}"),
    }

    let result = evaluator.resolve_property_access(TypeId::NEVER, "nonexistent");
    match result {
        PropertyAccessResult::Success { type_id: t, .. } => assert_eq!(t, TypeId::NEVER),
        _ => panic!("Any property on never should return never, got {result:?}"),
    }
}
#[test]
fn test_property_access_object() {
    let interner = TypeInterner::new();
    let evaluator = PropertyAccessEvaluator::new(&interner);

    // { x: number, y: string }
    let obj = interner.object(vec![
        PropertyInfo::new(interner.intern_string("x"), TypeId::NUMBER),
        PropertyInfo::new(interner.intern_string("y"), TypeId::STRING),
    ]);

    // Access existing property
    let result = evaluator.resolve_property_access(obj, "x");
    match result {
        PropertyAccessResult::Success { type_id: t, .. } => assert_eq!(t, TypeId::NUMBER),
        _ => panic!("Expected success, got {result:?}"),
    }

    // Access non-existent property
    let result = evaluator.resolve_property_access(obj, "z");
    match result {
        PropertyAccessResult::PropertyNotFound { .. } => {}
        _ => panic!("Expected PropertyNotFound, got {result:?}"),
    }
}
#[test]
fn test_property_access_function_members() {
    let interner = TypeInterner::new();
    let evaluator = PropertyAccessEvaluator::new(&interner);

    let func = interner.function(FunctionShape {
        params: vec![],
        this_type: None,
        return_type: TypeId::VOID,
        type_params: Vec::new(),
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let result = evaluator.resolve_property_access(func, "call");
    match result {
        PropertyAccessResult::Success { type_id, .. } => {
            let Some(TypeData::Function(shape_id)) = interner.lookup(type_id) else {
                panic!("Expected call to resolve to function type");
            };
            let shape = interner.function_shape(shape_id);
            let rest_array = interner.array(TypeId::ANY);
            assert_eq!(shape.return_type, TypeId::ANY);
            assert_eq!(shape.params.len(), 1);
            assert!(shape.params[0].rest);
            assert_eq!(shape.params[0].type_id, rest_array);
        }
        _ => panic!("Expected success, got {result:?}"),
    }

    let result = evaluator.resolve_property_access(func, "length");
    match result {
        PropertyAccessResult::Success { type_id: t, .. } => assert_eq!(t, TypeId::NUMBER),
        _ => panic!("Expected success, got {result:?}"),
    }

    let result = evaluator.resolve_property_access(func, "toString");
    match result {
        PropertyAccessResult::Success { type_id, .. } => {
            let Some(TypeData::Function(shape_id)) = interner.lookup(type_id) else {
                panic!("Expected toString to resolve to function type");
            };
            let shape = interner.function_shape(shape_id);
            assert_eq!(shape.return_type, TypeId::STRING);
        }
        _ => panic!("Expected success, got {result:?}"),
    }
}
#[test]
fn test_property_access_callable_members() {
    let interner = TypeInterner::new();
    let evaluator = PropertyAccessEvaluator::new(&interner);

    let call_sig = CallSignature {
        type_params: vec![],
        params: vec![],
        this_type: None,
        return_type: TypeId::VOID,
        type_predicate: None,
        is_method: false,
    };
    let callable = interner.callable(CallableShape {
        symbol: None,
        is_abstract: false,
        call_signatures: vec![call_sig],
        construct_signatures: vec![],
        properties: vec![],
        ..Default::default()
    });

    let result = evaluator.resolve_property_access(callable, "bind");
    match result {
        PropertyAccessResult::Success { type_id, .. } => {
            let Some(TypeData::Function(shape_id)) = interner.lookup(type_id) else {
                panic!("Expected bind to resolve to function type");
            };
            let shape = interner.function_shape(shape_id);
            assert_eq!(shape.return_type, TypeId::ANY);
        }
        _ => panic!("Expected success, got {result:?}"),
    }
}
#[test]
fn test_property_access_optional_property() {
    let interner = TypeInterner::new();
    let evaluator = PropertyAccessEvaluator::new(&interner);

    let obj = interner.object(vec![PropertyInfo::opt(
        interner.intern_string("x"),
        TypeId::NUMBER,
    )]);

    let result = evaluator.resolve_property_access(obj, "x");
    match result {
        PropertyAccessResult::Success {
            type_id,
            write_type: _,
            from_index_signature,
        } => {
            let expected = interner.union(vec![TypeId::NUMBER, TypeId::UNDEFINED]);
            assert_eq!(type_id, expected);
            assert!(!from_index_signature);
        }
        _ => panic!("Expected success, got {result:?}"),
    }
}

/// Creates a `TypeEnvironment` with a mock Array<T> interface for testing.
/// The interface includes: length, map, at, entries, and reduce.
fn make_array_test_env(
    interner: &TypeInterner,
) -> (
    crate::relations::subtype::TypeEnvironment,
    crate::types::TypeParamInfo,
) {
    use crate::relations::subtype::TypeEnvironment;
    use crate::types::TypeParamInfo;

    let t_param = TypeParamInfo {
        name: interner.intern_string("T"),
        constraint: None,
        default: None,
        is_const: false,
    };
    let t_type = interner.intern(TypeData::TypeParameter(t_param));

    // length: number
    let length_prop = PropertyInfo::readonly(interner.intern_string("length"), TypeId::NUMBER);

    // map<U>(callbackfn: (value: T, index: number, array: T[]) => U, thisArg?: any): U[]
    let u_param = TypeParamInfo {
        name: interner.intern_string("U"),
        constraint: None,
        default: None,
        is_const: false,
    };
    let u_type = interner.intern(TypeData::TypeParameter(u_param));
    let map_callback = interner.function(FunctionShape {
        params: vec![
            ParamInfo {
                name: Some(interner.intern_string("value")),
                type_id: t_type,
                optional: false,
                rest: false,
            },
            ParamInfo {
                name: Some(interner.intern_string("index")),
                type_id: TypeId::NUMBER,
                optional: false,
                rest: false,
            },
            ParamInfo {
                name: Some(interner.intern_string("array")),
                type_id: interner.array(t_type),
                optional: false,
                rest: false,
            },
        ],
        return_type: u_type,
        type_params: vec![],
        this_type: None,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });
    let map_func = interner.function(FunctionShape {
        params: vec![
            ParamInfo {
                name: Some(interner.intern_string("callbackfn")),
                type_id: map_callback,
                optional: false,
                rest: false,
            },
            ParamInfo {
                name: Some(interner.intern_string("thisArg")),
                type_id: TypeId::ANY,
                optional: true,
                rest: false,
            },
        ],
        return_type: interner.array(u_type),
        type_params: vec![u_param],
        this_type: None,
        type_predicate: None,
        is_constructor: false,
        is_method: true,
    });

    // at(index: number): T | undefined
    let at_func = interner.function(FunctionShape {
        params: vec![ParamInfo {
            name: Some(interner.intern_string("index")),
            type_id: TypeId::NUMBER,
            optional: false,
            rest: false,
        }],
        return_type: interner.union(vec![t_type, TypeId::UNDEFINED]),
        type_params: vec![],
        this_type: None,
        type_predicate: None,
        is_constructor: false,
        is_method: true,
    });

    // entries(): Array<[number, T]>
    let entry_tuple = interner.tuple(vec![
        TupleElement {
            type_id: TypeId::NUMBER,
            name: None,
            optional: false,
            rest: false,
        },
        TupleElement {
            type_id: t_type,
            name: None,
            optional: false,
            rest: false,
        },
    ]);
    let entries_func = interner.function(FunctionShape {
        params: vec![],
        return_type: interner.array(entry_tuple),
        type_params: vec![],
        this_type: None,
        type_predicate: None,
        is_constructor: false,
        is_method: true,
    });

    // reduce(callbackfn: (prev: T, curr: T, idx: number, arr: T[]) => T): T
    use crate::types::CallSignature;
    let reduce_cb_1 = interner.function(FunctionShape {
        params: vec![
            ParamInfo {
                name: Some(interner.intern_string("previousValue")),
                type_id: t_type,
                optional: false,
                rest: false,
            },
            ParamInfo {
                name: Some(interner.intern_string("currentValue")),
                type_id: t_type,
                optional: false,
                rest: false,
            },
            ParamInfo {
                name: Some(interner.intern_string("currentIndex")),
                type_id: TypeId::NUMBER,
                optional: false,
                rest: false,
            },
            ParamInfo {
                name: Some(interner.intern_string("array")),
                type_id: interner.array(t_type),
                optional: false,
                rest: false,
            },
        ],
        return_type: t_type,
        type_params: vec![],
        this_type: None,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });
    let reduce_sig_1 = CallSignature {
        params: vec![ParamInfo {
            name: Some(interner.intern_string("callbackfn")),
            type_id: reduce_cb_1,
            optional: false,
            rest: false,
        }],
        return_type: t_type,
        type_params: vec![],
        this_type: None,
        type_predicate: None,
        is_method: true,
    };
    // reduce<U>(callbackfn: (prev: U, curr: T, idx: number, arr: T[]) => U, initialValue: U): U
    let reduce_cb_2 = interner.function(FunctionShape {
        params: vec![
            ParamInfo {
                name: Some(interner.intern_string("previousValue")),
                type_id: u_type,
                optional: false,
                rest: false,
            },
            ParamInfo {
                name: Some(interner.intern_string("currentValue")),
                type_id: t_type,
                optional: false,
                rest: false,
            },
            ParamInfo {
                name: Some(interner.intern_string("currentIndex")),
                type_id: TypeId::NUMBER,
                optional: false,
                rest: false,
            },
            ParamInfo {
                name: Some(interner.intern_string("array")),
                type_id: interner.array(t_type),
                optional: false,
                rest: false,
            },
        ],
        return_type: u_type,
        type_params: vec![],
        this_type: None,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });
    let reduce_sig_2 = CallSignature {
        params: vec![
            ParamInfo {
                name: Some(interner.intern_string("callbackfn")),
                type_id: reduce_cb_2,
                optional: false,
                rest: false,
            },
            ParamInfo {
                name: Some(interner.intern_string("initialValue")),
                type_id: u_type,
                optional: false,
                rest: false,
            },
        ],
        return_type: u_type,
        type_params: vec![u_param],
        this_type: None,
        type_predicate: None,
        is_method: true,
    };
    let reduce_callable = interner.callable(CallableShape {
        symbol: None,
        is_abstract: false,
        call_signatures: vec![reduce_sig_1, reduce_sig_2],
        construct_signatures: vec![],
        properties: vec![],
        string_index: None,
        number_index: None,
    });

    let array_interface = interner.object(vec![
        length_prop,
        PropertyInfo::method(interner.intern_string("map"), map_func),
        PropertyInfo::method(interner.intern_string("at"), at_func),
        PropertyInfo::method(interner.intern_string("entries"), entries_func),
        PropertyInfo::method(interner.intern_string("reduce"), reduce_callable),
    ]);

    // Set array base type on the interner so PropertyAccessEvaluator can find it
    interner.set_array_base_type(array_interface, vec![t_param]);

    let mut env = TypeEnvironment::new();
    env.set_array_base_type(array_interface, vec![t_param]);

    (env, t_param)
}
#[test]
fn test_property_access_readonly_array() {
    let interner = TypeInterner::new();
    let (_env, _) = make_array_test_env(&interner);
    let evaluator = PropertyAccessEvaluator::new(&interner);

    let array = interner.array(TypeId::STRING);
    let readonly_array = interner.intern(TypeData::ReadonlyType(array));

    let result = evaluator.resolve_property_access(readonly_array, "length");
    match result {
        PropertyAccessResult::Success { type_id: t, .. } => assert_eq!(t, TypeId::NUMBER),
        _ => panic!("Expected success, got {result:?}"),
    }
}
#[test]
fn test_property_access_tuple_length() {
    let interner = TypeInterner::new();
    let (_env, _) = make_array_test_env(&interner);
    let evaluator = PropertyAccessEvaluator::new(&interner);

    // Fixed-length tuple [number, string] → .length should be literal 2
    let tuple = interner.tuple(vec![
        TupleElement {
            type_id: TypeId::NUMBER,
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

    let expected_literal = interner.literal_number(2.0);
    let result = evaluator.resolve_property_access(tuple, "length");
    match result {
        PropertyAccessResult::Success { type_id: t, .. } => assert_eq!(t, expected_literal),
        _ => panic!("Expected success, got {result:?}"),
    }
}
#[test]
fn test_property_access_empty_tuple_length() {
    let interner = TypeInterner::new();
    let (_env, _) = make_array_test_env(&interner);
    let evaluator = PropertyAccessEvaluator::new(&interner);

    // Empty tuple [] → .length should be literal 0
    let tuple = interner.tuple(vec![]);
    let expected_literal = interner.literal_number(0.0);
    let result = evaluator.resolve_property_access(tuple, "length");
    match result {
        PropertyAccessResult::Success { type_id: t, .. } => assert_eq!(t, expected_literal),
        _ => panic!("Expected success, got {result:?}"),
    }
}
#[test]
fn test_property_access_single_element_tuple_length() {
    let interner = TypeInterner::new();
    let (_env, _) = make_array_test_env(&interner);
    let evaluator = PropertyAccessEvaluator::new(&interner);

    // Single element tuple [number] → .length should be literal 1
    let tuple = interner.tuple(vec![TupleElement {
        type_id: TypeId::NUMBER,
        name: None,
        optional: false,
        rest: false,
    }]);
    let expected_literal = interner.literal_number(1.0);
    let result = evaluator.resolve_property_access(tuple, "length");
    match result {
        PropertyAccessResult::Success { type_id: t, .. } => assert_eq!(t, expected_literal),
        _ => panic!("Expected success, got {result:?}"),
    }
}
#[test]
fn test_property_access_array_length_stays_number() {
    let interner = TypeInterner::new();
    let (_env, _) = make_array_test_env(&interner);
    let evaluator = PropertyAccessEvaluator::new(&interner);

    // Array type number[] → .length should remain `number`, not a literal
    let array = interner.array(TypeId::NUMBER);
    let result = evaluator.resolve_property_access(array, "length");
    match result {
        PropertyAccessResult::Success { type_id: t, .. } => assert_eq!(t, TypeId::NUMBER),
        _ => panic!("Expected success, got {result:?}"),
    }
}
#[test]
fn test_property_access_tuple_with_rest_length_stays_number() {
    let interner = TypeInterner::new();
    let (_env, _) = make_array_test_env(&interner);
    let evaluator = PropertyAccessEvaluator::new(&interner);

    // Tuple with array rest element [number, ...string[]] → variable length → `number`
    let rest_array = interner.array(TypeId::STRING);
    let tuple = interner.tuple(vec![
        TupleElement {
            type_id: TypeId::NUMBER,
            name: None,
            optional: false,
            rest: false,
        },
        TupleElement {
            type_id: rest_array,
            name: None,
            optional: false,
            rest: true,
        },
    ]);
    let result = evaluator.resolve_property_access(tuple, "length");
    match result {
        PropertyAccessResult::Success { type_id: t, .. } => assert_eq!(t, TypeId::NUMBER),
        _ => panic!("Expected success, got {result:?}"),
    }
}
#[test]
fn test_property_access_array_map_signature() {
    let interner = TypeInterner::new();
    let (_env, _) = make_array_test_env(&interner);
    let evaluator = PropertyAccessEvaluator::new(&interner);

    let array = interner.array(TypeId::NUMBER);
    let result = evaluator.resolve_property_access(array, "map");
    match result {
        PropertyAccessResult::Success { type_id, .. } => match interner.lookup(type_id) {
            Some(TypeData::Function(func_id)) => {
                let func = interner.function_shape(func_id);
                assert_eq!(func.type_params.len(), 1, "map should have 1 type param U");
                assert_eq!(func.params.len(), 2, "map should have 2 params");
                let u_param = &func.type_params[0];
                let u_type = interner.intern(TypeData::TypeParameter(*u_param));
                let expected_return = interner.array(u_type);
                assert_eq!(func.return_type, expected_return, "map should return U[]");

                let callback_type = func.params[0].type_id;
                match interner.lookup(callback_type) {
                    Some(TypeData::Function(cb_id)) => {
                        let callback = interner.function_shape(cb_id);
                        assert_eq!(callback.return_type, u_type);
                        assert_eq!(callback.params[0].type_id, TypeId::NUMBER); // T=number
                        assert_eq!(callback.params[1].type_id, TypeId::NUMBER); // index
                        assert_eq!(callback.params[2].type_id, array); // array: number[]
                    }
                    other => panic!("Expected callback function, got {other:?}"),
                }
            }
            other => panic!("Expected function, got {other:?}"),
        },
        _ => panic!("Expected success, got {result:?}"),
    }
}
#[test]
fn test_property_access_array_at_returns_optional_element() {
    let interner = TypeInterner::new();
    let (_env, _) = make_array_test_env(&interner);
    let evaluator = PropertyAccessEvaluator::new(&interner);

    let array = interner.array(TypeId::NUMBER);
    let result = evaluator.resolve_property_access(array, "at");
    match result {
        PropertyAccessResult::Success { type_id, .. } => match interner.lookup(type_id) {
            Some(TypeData::Function(func_id)) => {
                let func = interner.function_shape(func_id);
                let expected = interner.union(vec![TypeId::NUMBER, TypeId::UNDEFINED]);
                assert_eq!(func.return_type, expected);
            }
            other => panic!("Expected function, got {other:?}"),
        },
        _ => panic!("Expected success, got {result:?}"),
    }
}
#[test]
fn test_property_access_array_entries_returns_tuple_array() {
    let interner = TypeInterner::new();
    let (_env, _) = make_array_test_env(&interner);
    let evaluator = PropertyAccessEvaluator::new(&interner);

    let array = interner.array(TypeId::BOOLEAN);
    let result = evaluator.resolve_property_access(array, "entries");
    match result {
        PropertyAccessResult::Success { type_id, .. } => match interner.lookup(type_id) {
            Some(TypeData::Function(func_id)) => {
                let func = interner.function_shape(func_id);
                let Some(TypeData::Array(return_elem)) = interner.lookup(func.return_type) else {
                    panic!("Expected array return type");
                };
                let Some(TypeData::Tuple(tuple_id)) = interner.lookup(return_elem) else {
                    panic!("Expected tuple element type");
                };
                let tuple = interner.tuple_list(tuple_id);
                assert_eq!(tuple.len(), 2);
                assert_eq!(tuple[0].type_id, TypeId::NUMBER);
                assert_eq!(tuple[1].type_id, TypeId::BOOLEAN);
            }
            other => panic!("Expected function, got {other:?}"),
        },
        _ => panic!("Expected success, got {result:?}"),
    }
}
#[test]
fn test_property_access_array_reduce_callable() {
    let interner = TypeInterner::new();
    let (_env, _) = make_array_test_env(&interner);
    let evaluator = PropertyAccessEvaluator::new(&interner);

    let array = interner.array(TypeId::STRING);
    let result = evaluator.resolve_property_access(array, "reduce");
    match result {
        PropertyAccessResult::Success { type_id, .. } => match interner.lookup(type_id) {
            Some(TypeData::Callable(callable_id)) => {
                let callable = interner.callable_shape(callable_id);
                assert_eq!(callable.call_signatures.len(), 2);
                assert_eq!(callable.call_signatures[0].return_type, TypeId::STRING);
                let generic_sig = &callable.call_signatures[1];
                assert_eq!(generic_sig.type_params.len(), 1);
                let u_type = interner.intern(TypeData::TypeParameter(generic_sig.type_params[0]));
                assert_eq!(generic_sig.return_type, u_type);
            }
            other => panic!("Expected callable, got {other:?}"),
        },
        _ => panic!("Expected success, got {result:?}"),
    }
}
#[test]
fn test_property_access_void() {
    let interner = TypeInterner::new();
    let evaluator = PropertyAccessEvaluator::new(&interner);

    let result = evaluator.resolve_property_access(TypeId::VOID, "x");
    match result {
        PropertyAccessResult::PropertyNotFound { .. } => {
            // void has no properties; solver returns PropertyNotFound
        }
        _ => panic!("Expected PropertyNotFound, got {result:?}"),
    }
}
#[test]
fn test_property_access_index_signature_no_unchecked() {
    let interner = TypeInterner::new();
    let mut evaluator = PropertyAccessEvaluator::new(&interner);

    let obj = interner.object_with_index(ObjectShape {
        symbol: None,
        flags: ObjectFlags::empty(),
        properties: Vec::new(),
        string_index: Some(IndexSignature {
            key_type: TypeId::STRING,
            value_type: TypeId::NUMBER,
            readonly: false,
            param_name: None,
        }),
        number_index: None,
    });

    let result = evaluator.resolve_property_access(obj, "anything");
    match result {
        PropertyAccessResult::Success {
            type_id,
            write_type: _,
            from_index_signature,
        } => {
            assert_eq!(type_id, TypeId::NUMBER);
            assert!(from_index_signature);
        }
        _ => panic!("Expected success, got {result:?}"),
    }

    evaluator.set_no_unchecked_indexed_access(true);

    let result = evaluator.resolve_property_access(obj, "anything");
    match result {
        PropertyAccessResult::Success {
            type_id,
            write_type: _,
            from_index_signature,
        } => {
            let expected = interner.union(vec![TypeId::NUMBER, TypeId::UNDEFINED]);
            assert_eq!(type_id, expected);
            assert!(from_index_signature);
        }
        _ => panic!("Expected success, got {result:?}"),
    }
}
#[test]
fn test_property_access_object_with_index_optional_property() {
    let interner = TypeInterner::new();
    let evaluator = PropertyAccessEvaluator::new(&interner);

    let obj = interner.object_with_index(ObjectShape {
        symbol: None,
        flags: ObjectFlags::empty(),
        properties: vec![PropertyInfo::opt(
            interner.intern_string("x"),
            TypeId::NUMBER,
        )],
        string_index: Some(IndexSignature {
            key_type: TypeId::STRING,
            value_type: TypeId::BOOLEAN,
            readonly: false,
            param_name: None,
        }),
        number_index: None,
    });

    let result = evaluator.resolve_property_access(obj, "x");
    match result {
        PropertyAccessResult::Success {
            type_id,
            write_type: _,
            from_index_signature,
        } => {
            let expected = interner.union(vec![TypeId::NUMBER, TypeId::UNDEFINED]);
            assert_eq!(type_id, expected);
            assert!(!from_index_signature);
        }
        _ => panic!("Expected success, got {result:?}"),
    }
}
#[test]
fn test_property_access_string() {
    let interner = TypeInterner::new();
    let evaluator = PropertyAccessEvaluator::new(&interner);

    let result = evaluator.resolve_property_access(TypeId::STRING, "length");
    match result {
        PropertyAccessResult::Success { type_id: t, .. } => assert_eq!(t, TypeId::NUMBER),
        _ => panic!("Expected success, got {result:?}"),
    }
}
#[test]
fn test_property_access_number_method() {
    let interner = TypeInterner::new();
    let evaluator = PropertyAccessEvaluator::new(&interner);

    let result = evaluator.resolve_property_access(TypeId::NUMBER, "toFixed");
    match result {
        PropertyAccessResult::Success { type_id, .. } => match interner.lookup(type_id) {
            Some(TypeData::Function(func_id)) => {
                let func = interner.function_shape(func_id);
                assert_eq!(func.return_type, TypeId::STRING);
            }
            other => panic!("Expected function, got {other:?}"),
        },
        _ => panic!("Expected success, got {result:?}"),
    }
}
#[test]
fn test_property_access_boolean_method() {
    let interner = TypeInterner::new();
    let evaluator = PropertyAccessEvaluator::new(&interner);

    let result = evaluator.resolve_property_access(TypeId::BOOLEAN, "valueOf");
    match result {
        PropertyAccessResult::Success { type_id, .. } => match interner.lookup(type_id) {
            Some(TypeData::Function(func_id)) => {
                let func = interner.function_shape(func_id);
                assert_eq!(func.return_type, TypeId::BOOLEAN);
            }
            other => panic!("Expected function, got {other:?}"),
        },
        _ => panic!("Expected success, got {result:?}"),
    }
}
#[test]
fn test_property_access_bigint_method() {
    let interner = TypeInterner::new();
    let evaluator = PropertyAccessEvaluator::new(&interner);

    let result = evaluator.resolve_property_access(TypeId::BIGINT, "toString");
    match result {
        PropertyAccessResult::Success { type_id, .. } => match interner.lookup(type_id) {
            Some(TypeData::Function(func_id)) => {
                let func = interner.function_shape(func_id);
                assert_eq!(func.return_type, TypeId::STRING);
            }
            other => panic!("Expected function, got {other:?}"),
        },
        _ => panic!("Expected success, got {result:?}"),
    }
}
#[test]
fn test_property_access_object_methods_on_primitives() {
    let interner = TypeInterner::new();
    let evaluator = PropertyAccessEvaluator::new(&interner);

    let result = evaluator.resolve_property_access(TypeId::STRING, "hasOwnProperty");
    match result {
        PropertyAccessResult::Success { type_id, .. } => match interner.lookup(type_id) {
            Some(TypeData::Function(func_id)) => {
                let func = interner.function_shape(func_id);
                assert_eq!(func.return_type, TypeId::BOOLEAN);
            }
            other => panic!("Expected function, got {other:?}"),
        },
        _ => panic!("Expected success, got {result:?}"),
    }

    let result = evaluator.resolve_property_access(TypeId::NUMBER, "isPrototypeOf");
    match result {
        PropertyAccessResult::Success { type_id, .. } => match interner.lookup(type_id) {
            Some(TypeData::Function(func_id)) => {
                let func = interner.function_shape(func_id);
                assert_eq!(func.return_type, TypeId::BOOLEAN);
            }
            other => panic!("Expected function, got {other:?}"),
        },
        _ => panic!("Expected success, got {result:?}"),
    }

    let result = evaluator.resolve_property_access(TypeId::BOOLEAN, "propertyIsEnumerable");
    match result {
        PropertyAccessResult::Success { type_id, .. } => match interner.lookup(type_id) {
            Some(TypeData::Function(func_id)) => {
                let func = interner.function_shape(func_id);
                assert_eq!(func.return_type, TypeId::BOOLEAN);
            }
            other => panic!("Expected function, got {other:?}"),
        },
        _ => panic!("Expected success, got {result:?}"),
    }
}
#[test]
fn test_property_access_reuses_context_across_name_lengths() {
    let interner = TypeInterner::new();
    let evaluator = PropertyAccessEvaluator::new(&interner);

    let long_name = "hasOwnProperty";
    let short_name = "length";

    let long_result = evaluator.resolve_property_access(TypeId::STRING, long_name);
    match long_result {
        PropertyAccessResult::Success { type_id, .. } => match interner.lookup(type_id) {
            Some(TypeData::Function(_)) => {}
            other => panic!("Expected function for long name, got {other:?}"),
        },
        _ => panic!("Expected success for long name, got {long_result:?}"),
    }

    let short_result = evaluator.resolve_property_access(TypeId::STRING, short_name);
    match short_result {
        PropertyAccessResult::Success { type_id, .. } => assert_eq!(type_id, TypeId::NUMBER),
        _ => panic!("Expected success for short name, got {short_result:?}"),
    }
}
#[test]
fn test_property_access_primitive_constructor_value() {
    let interner = TypeInterner::new();
    let evaluator = PropertyAccessEvaluator::new(&interner);

    let result = evaluator.resolve_property_access(TypeId::SYMBOL, "constructor");
    match result {
        PropertyAccessResult::Success { type_id, .. } => assert_eq!(type_id, TypeId::FUNCTION),
        _ => panic!("Expected success, got {result:?}"),
    }
}
#[test]
fn test_property_access_template_literal() {
    let interner = TypeInterner::new();
    let evaluator = PropertyAccessEvaluator::new(&interner);

    let template = interner.template_literal(vec![
        TemplateSpan::Text(interner.intern_string("prefix")),
        TemplateSpan::Type(TypeId::STRING),
        TemplateSpan::Text(interner.intern_string("suffix")),
    ]);

    let result = evaluator.resolve_property_access(template, "length");
    match result {
        PropertyAccessResult::Success { type_id: t, .. } => assert_eq!(t, TypeId::NUMBER),
        _ => panic!("Expected success, got {result:?}"),
    }
}
#[test]
fn test_property_access_literal_string_length() {
    let interner = TypeInterner::new();
    let evaluator = PropertyAccessEvaluator::new(&interner);

    let literal = interner.literal_string("hello");
    let result = evaluator.resolve_property_access(literal, "length");
    match result {
        PropertyAccessResult::Success { type_id: t, .. } => assert_eq!(t, TypeId::NUMBER),
        _ => panic!("Expected success, got {result:?}"),
    }
}
#[test]
fn test_binary_op_addition() {
    let interner = TypeInterner::new();
    let evaluator = BinaryOpEvaluator::new(&interner);

    // number + number = number
    let result = evaluator.evaluate(TypeId::NUMBER, TypeId::NUMBER, "+");
    match result {
        BinaryOpResult::Success(t) => assert_eq!(t, TypeId::NUMBER),
        _ => panic!("Expected success, got {result:?}"),
    }

    // string + number = string
    let result = evaluator.evaluate(TypeId::STRING, TypeId::NUMBER, "+");
    match result {
        BinaryOpResult::Success(t) => assert_eq!(t, TypeId::STRING),
        _ => panic!("Expected success, got {result:?}"),
    }
}
#[test]
fn test_binary_op_logical() {
    let interner = TypeInterner::new();
    let evaluator = BinaryOpEvaluator::new(&interner);

    // number && string = 0 | string (definitely-falsy part of number is literal 0)
    let result = evaluator.evaluate(TypeId::NUMBER, TypeId::STRING, "&&");
    match result {
        BinaryOpResult::Success(t) => {
            // Should be a union type with 0 (literal) and string
            let key = interner.lookup(t).unwrap();
            match key {
                TypeData::Union(members) => {
                    let members = interner.type_list(members);
                    assert_eq!(members.len(), 2, "Expected 2 members, got {members:?}");
                    assert!(members.contains(&TypeId::STRING));
                    // The other member should be a number literal 0
                    let zero_type = members.iter().find(|&&m| m != TypeId::STRING).unwrap();
                    match interner.lookup(*zero_type) {
                        Some(TypeData::Literal(LiteralValue::Number(n))) => {
                            assert_eq!(n.0, 0.0, "Expected 0, got {}", n.0);
                        }
                        other => panic!("Expected number literal 0, got {other:?}"),
                    }
                }
                _ => panic!("Expected union, got {key:?}"),
            }
        }
        _ => panic!("Expected success, got {result:?}"),
    }
}
#[test]
fn test_call_generic_function_identity() {
    let interner = TypeInterner::new();
    let mut subtype = CompatChecker::new(&interner);
    let mut evaluator = CallEvaluator::new(&interner, &mut subtype);

    // Create type parameter T
    let t_param = TypeParamInfo {
        name: interner.intern_string("T"),
        constraint: None,
        default: None,
        is_const: false,
    };
    let t_type = interner.intern(TypeData::TypeParameter(t_param));

    // function identity<T>(x: T): T
    let func = interner.function(FunctionShape {
        type_params: vec![t_param],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("x")),
            type_id: t_type,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: t_type,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    // Call identity(42) -> should infer T = number
    let result = evaluator.resolve_call(func, &[TypeId::NUMBER]);
    match result {
        CallResult::Success(ret) => assert_eq!(ret, TypeId::NUMBER),
        _ => panic!("Expected success, got {result:?}"),
    }
}
#[test]
fn test_call_generic_function_with_string() {
    let interner = TypeInterner::new();
    let mut subtype = CompatChecker::new(&interner);
    let mut evaluator = CallEvaluator::new(&interner, &mut subtype);

    // Create type parameter T
    let t_param = TypeParamInfo {
        name: interner.intern_string("T"),
        constraint: None,
        default: None,
        is_const: false,
    };
    let t_type = interner.intern(TypeData::TypeParameter(t_param));

    // function identity<T>(x: T): T
    let func = interner.function(FunctionShape {
        type_params: vec![t_param],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("x")),
            type_id: t_type,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: t_type,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    // Call identity("hello") -> should infer T = string
    let result = evaluator.resolve_call(func, &[TypeId::STRING]);
    match result {
        CallResult::Success(ret) => assert_eq!(ret, TypeId::STRING),
        _ => panic!("Expected success, got {result:?}"),
    }
}
#[test]
fn test_call_generic_argument_type_mismatch_with_default() {
    let interner = TypeInterner::new();
    let mut subtype = CompatChecker::new(&interner);
    let mut evaluator = CallEvaluator::new(&interner, &mut subtype);

    let t_param = TypeParamInfo {
        name: interner.intern_string("T"),
        constraint: None,
        default: Some(TypeId::NUMBER),
        is_const: false,
    };
    let t_type = interner.intern(TypeData::TypeParameter(t_param));
    let param_type = interner.union(vec![t_type, TypeId::NUMBER]);

    let func = interner.function(FunctionShape {
        type_params: vec![t_param],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("x")),
            type_id: param_type,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: t_type,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    // Call foo<T = number>(x: T | number) with "hello".
    // In TypeScript, defaults are fallbacks when no inference candidates exist,
    // not constraints that prevent inference. T is inferred as string from the
    // argument, so x: string | number, and string is assignable → success.
    let result = evaluator.resolve_call(func, &[TypeId::STRING]);
    match result {
        CallResult::Success(ret) => assert_eq!(ret, TypeId::STRING),
        _ => panic!("Expected Success with T=string, got {result:?}"),
    }
}
#[test]
fn test_call_generic_direct_param_candidate_keeps_first_for_conflicting_literals() {
    // In tsc, f<T>(x: T, y: T) called with f(1, "") infers T as a union 1 | ""
    // (multiple inference candidates are unioned). The call succeeds.
    let interner = TypeInterner::new();
    let mut subtype = CompatChecker::new(&interner);
    let mut evaluator = CallEvaluator::new(&interner, &mut subtype);

    let t_param = TypeParamInfo {
        name: interner.intern_string("T"),
        constraint: None,
        default: None,
        is_const: false,
    };
    let t_type = interner.intern(TypeData::TypeParameter(t_param));

    let func = interner.function(FunctionShape {
        type_params: vec![t_param],
        params: vec![
            ParamInfo {
                name: Some(interner.intern_string("x")),
                type_id: t_type,
                optional: false,
                rest: false,
            },
            ParamInfo {
                name: Some(interner.intern_string("y")),
                type_id: t_type,
                optional: false,
                rest: false,
            },
        ],
        this_type: None,
        return_type: t_type,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let one = interner.literal_number(1.0);
    let two = interner.literal_string("");

    let result = evaluator.resolve_call(func, &[one, two]);
    // tsc's getSingleCommonSupertype uses first-wins for fresh literals:
    // T = 1 (widened to number), then "" is checked against number → TS2345.
    // So the call FAILS with ArgumentTypeMismatch, not Success.
    assert!(
        matches!(result, CallResult::ArgumentTypeMismatch { .. }),
        "Expected ArgumentTypeMismatch (tsc's first-wins), got {result:?}"
    );
}
#[test]
fn test_call_generic_argument_count_mismatch() {
    let interner = TypeInterner::new();
    let mut subtype = CompatChecker::new(&interner);
    let mut evaluator = CallEvaluator::new(&interner, &mut subtype);

    let t_param = TypeParamInfo {
        name: interner.intern_string("T"),
        constraint: None,
        default: None,
        is_const: false,
    };
    let t_type = interner.intern(TypeData::TypeParameter(t_param));

    let func = interner.function(FunctionShape {
        type_params: vec![t_param],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("x")),
            type_id: t_type,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: t_type,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let result = evaluator.resolve_call(func, &[]);
    match result {
        CallResult::ArgumentCountMismatch {
            expected_min,
            actual,
            ..
        } => {
            assert_eq!(expected_min, 1);
            assert_eq!(actual, 0);
        }
        _ => panic!("Expected ArgumentCountMismatch, got {result:?}"),
    }
}
#[test]
fn test_call_generic_rest_tuple_constraint_count_mismatch() {
    let interner = TypeInterner::new();
    let mut subtype = CompatChecker::new(&interner);
    let mut evaluator = CallEvaluator::new(&interner, &mut subtype);

    let tuple_constraint = interner.tuple(vec![
        TupleElement {
            type_id: TypeId::NUMBER,
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
    let t_param = TypeParamInfo {
        name: interner.intern_string("T"),
        constraint: Some(tuple_constraint),
        default: None,
        is_const: false,
    };
    let t_type = interner.intern(TypeData::TypeParameter(t_param));

    let func = interner.function(FunctionShape {
        type_params: vec![t_param],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("args")),
            type_id: t_type,
            optional: false,
            rest: true,
        }],
        this_type: None,
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let result = evaluator.resolve_call(func, &[]);
    match result {
        CallResult::ArgumentCountMismatch {
            expected_min,
            expected_max,
            actual,
        } => {
            assert_eq!(expected_min, 2);
            assert_eq!(expected_max, Some(2));
            assert_eq!(actual, 0);
        }
        _ => panic!("Expected ArgumentCountMismatch, got {result:?}"),
    }
}
#[test]
fn test_call_generic_default_rest_tuple_count_mismatch() {
    let interner = TypeInterner::new();
    let mut subtype = CompatChecker::new(&interner);
    let mut evaluator = CallEvaluator::new(&interner, &mut subtype);

    let tuple_default = interner.tuple(vec![
        TupleElement {
            type_id: TypeId::NUMBER,
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
    let t_param = TypeParamInfo {
        name: interner.intern_string("T"),
        constraint: None,
        default: Some(tuple_default),
        is_const: false,
    };
    let t_type = interner.intern(TypeData::TypeParameter(t_param));

    let func = interner.function(FunctionShape {
        type_params: vec![t_param],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("args")),
            type_id: t_type,
            optional: false,
            rest: true,
        }],
        this_type: None,
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let result = evaluator.resolve_call(func, &[]);
    match result {
        CallResult::ArgumentCountMismatch {
            expected_min,
            expected_max,
            actual,
        } => {
            assert_eq!(expected_min, 2);
            assert_eq!(expected_max, Some(2));
            assert_eq!(actual, 0);
        }
        _ => panic!("Expected ArgumentCountMismatch, got {result:?}"),
    }
}

/// Regression test: call<TS extends unknown[]>(handler: (...args: TS) => void, ...args: TS)
/// with too many args should emit TS2554. The handler's params infer TS = [number, number],
/// so the function expects 3 args total (handler + 2 numbers). Passing 8 args should fail.
/// This tests that `rest_tuple_inference` is skipped when the type variable also appears
/// in another parameter (the handler), preventing the rest args from overriding the
/// handler-inferred tuple type.
#[test]
fn test_call_generic_rest_excess_args_detected_when_shared_type_param() {
    let interner = TypeInterner::new();
    let mut subtype = CompatChecker::new(&interner);
    let mut evaluator = CallEvaluator::new(&interner, &mut subtype);

    // TS extends unknown[]
    let unknown_array = interner.array(TypeId::UNKNOWN);
    let ts_param = TypeParamInfo {
        name: interner.intern_string("TS"),
        constraint: Some(unknown_array),
        default: None,
        is_const: false,
    };
    let ts_type = interner.intern(TypeData::TypeParameter(ts_param));

    // handler: (...args: TS) => void
    let handler_fn = interner.function(FunctionShape {
        params: vec![ParamInfo {
            name: Some(interner.intern_string("args")),
            type_id: ts_type,
            optional: false,
            rest: true,
        }],
        this_type: None,
        return_type: TypeId::VOID,
        type_params: vec![],
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    // call<TS extends unknown[]>(handler: (...args: TS) => void, ...args: TS): void
    let call_fn = interner.function(FunctionShape {
        type_params: vec![ts_param],
        params: vec![
            ParamInfo {
                name: Some(interner.intern_string("handler")),
                type_id: handler_fn,
                optional: false,
                rest: false,
            },
            ParamInfo {
                name: Some(interner.intern_string("args")),
                type_id: ts_type,
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

    // The handler callback: (x: number, y: number) => number
    let handler_arg = interner.function(FunctionShape {
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
        type_params: vec![],
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    // Call with 8 args: call(handler, 1, 2, 3, 4, 5, 6, 7)
    let one = interner.literal_number(1.0);
    let two = interner.literal_number(2.0);
    let three = interner.literal_number(3.0);
    let four = interner.literal_number(4.0);
    let five = interner.literal_number(5.0);
    let six = interner.literal_number(6.0);
    let seven = interner.literal_number(7.0);
    let result = evaluator.resolve_call(
        call_fn,
        &[handler_arg, one, two, three, four, five, six, seven],
    );

    match result {
        CallResult::ArgumentCountMismatch {
            expected_min,
            expected_max,
            actual,
        } => {
            assert_eq!(expected_min, 3, "handler + 2 tuple elements");
            assert_eq!(expected_max, Some(3), "fixed-length tuple [number, number]");
            assert_eq!(actual, 8, "handler + 7 number args");
        }
        _ => panic!("Expected ArgumentCountMismatch for excess args, got {result:?}"),
    }
}
#[test]
fn test_call_generic_default_rest_tuple_optional_allows_empty() {
    let interner = TypeInterner::new();
    let mut subtype = CompatChecker::new(&interner);
    let mut evaluator = CallEvaluator::new(&interner, &mut subtype);

    let tuple_default = interner.tuple(vec![TupleElement {
        type_id: TypeId::NUMBER,
        name: None,
        optional: true,
        rest: false,
    }]);
    let t_param = TypeParamInfo {
        name: interner.intern_string("T"),
        constraint: None,
        default: Some(tuple_default),
        is_const: false,
    };
    let t_type = interner.intern(TypeData::TypeParameter(t_param));

    let func = interner.function(FunctionShape {
        type_params: vec![t_param],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("args")),
            type_id: t_type,
            optional: false,
            rest: true,
        }],
        this_type: None,
        return_type: t_type,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let result = evaluator.resolve_call(func, &[]);
    match result {
        CallResult::Success(ret) => assert_eq!(ret, tuple_default),
        _ => panic!("Expected success, got {result:?}"),
    }
}
#[test]
fn test_call_generic_argument_type_mismatch_non_generic_param() {
    let interner = TypeInterner::new();
    let mut subtype = CompatChecker::new(&interner);
    let mut evaluator = CallEvaluator::new(&interner, &mut subtype);

    let t_param = TypeParamInfo {
        name: interner.intern_string("T"),
        constraint: None,
        default: None,
        is_const: false,
    };
    let t_type = interner.intern(TypeData::TypeParameter(t_param));

    // function foo<T>(x: number, y: T): T
    let func = interner.function(FunctionShape {
        type_params: vec![t_param],
        params: vec![
            ParamInfo {
                name: Some(interner.intern_string("x")),
                type_id: TypeId::NUMBER,
                optional: false,
                rest: false,
            },
            ParamInfo {
                name: Some(interner.intern_string("y")),
                type_id: t_type,
                optional: false,
                rest: false,
            },
        ],
        this_type: None,
        return_type: t_type,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let result = evaluator.resolve_call(func, &[TypeId::STRING, TypeId::NUMBER]);
    match result {
        CallResult::ArgumentTypeMismatch {
            index,
            expected,
            actual,
            ..
        } => {
            assert_eq!(index, 0);
            assert_eq!(expected, TypeId::NUMBER);
            assert_eq!(actual, TypeId::STRING);
        }
        _ => panic!("Expected ArgumentTypeMismatch, got {result:?}"),
    }
}
#[test]
fn test_call_generic_callable_signature() {
    let interner = TypeInterner::new();
    let mut subtype = CompatChecker::new(&interner);
    let mut evaluator = CallEvaluator::new(&interner, &mut subtype);

    let t_param = TypeParamInfo {
        name: interner.intern_string("T"),
        constraint: None,
        default: None,
        is_const: false,
    };
    let t_type = interner.intern(TypeData::TypeParameter(t_param));

    let callable = interner.callable(CallableShape {
        symbol: None,
        is_abstract: false,
        call_signatures: vec![CallSignature {
            type_params: vec![t_param],
            params: vec![ParamInfo {
                name: Some(interner.intern_string("x")),
                type_id: t_type,
                optional: false,
                rest: false,
            }],
            this_type: None,
            return_type: t_type,
            type_predicate: None,
            is_method: false,
        }],
        construct_signatures: Vec::new(),
        properties: Vec::new(),
        ..Default::default()
    });

    let result = evaluator.resolve_call(callable, &[TypeId::NUMBER]);
    match result {
        CallResult::Success(ret) => assert_eq!(ret, TypeId::NUMBER),
        _ => panic!("Expected success, got {result:?}"),
    }
}
#[test]
fn test_call_generic_array_function() {
    let interner = TypeInterner::new();
    let mut subtype = CompatChecker::new(&interner);
    let mut evaluator = CallEvaluator::new(&interner, &mut subtype);

    // Create type parameter T
    let t_param = TypeParamInfo {
        name: interner.intern_string("T"),
        constraint: None,
        default: None,
        is_const: false,
    };
    let t_type = interner.intern(TypeData::TypeParameter(t_param));
    let array_t = interner.array(t_type);

    // function first<T>(arr: T[]): T
    let func = interner.function(FunctionShape {
        type_params: vec![t_param],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("arr")),
            type_id: array_t,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: t_type,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    // Call first(number[]) -> should infer T = number
    let number_array = interner.array(TypeId::NUMBER);
    let result = evaluator.resolve_call(func, &[number_array]);
    match result {
        CallResult::Success(ret) => assert_eq!(ret, TypeId::NUMBER),
        _ => panic!("Expected success, got {result:?}"),
    }
}
#[test]
fn test_infer_call_signature_identity() {
    let interner = TypeInterner::new();
    let mut subtype = CompatChecker::new(&interner);

    let t_param = TypeParamInfo {
        name: interner.intern_string("T"),
        constraint: None,
        default: None,
        is_const: false,
    };
    let t_type = interner.intern(TypeData::TypeParameter(t_param));

    let sig = CallSignature {
        type_params: vec![t_param],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("x")),
            type_id: t_type,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: t_type,
        type_predicate: None,
        is_method: false,
    };

    let result = infer_call_signature(&interner, &mut subtype, &sig, &[TypeId::NUMBER]);
    assert_eq!(result, TypeId::NUMBER);
}
#[test]
fn test_infer_generic_function_identity() {
    let interner = TypeInterner::new();
    let mut subtype = CompatChecker::new(&interner);

    let t_param = TypeParamInfo {
        name: interner.intern_string("T"),
        constraint: None,
        default: None,
        is_const: false,
    };
    let t_type = interner.intern(TypeData::TypeParameter(t_param));

    let func = FunctionShape {
        type_params: vec![t_param],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("x")),
            type_id: t_type,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: t_type,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    };

    let result = infer_generic_function(&interner, &mut subtype, &func, &[TypeId::STRING]);
    assert_eq!(result, TypeId::STRING);
}
#[test]
fn test_generic_call_resets_fixed_union_member_cache() {
    let interner = TypeInterner::new();
    let mut checker = CompatChecker::new(&interner);
    let mut evaluator = CallEvaluator::new(&interner, &mut checker);

    let t_param = TypeParamInfo {
        name: interner.intern_string("T"),
        constraint: None,
        default: None,
        is_const: false,
    };
    let t_type = interner.type_param(t_param);
    let identity = interner.function(FunctionShape {
        params: vec![ParamInfo::unnamed(t_type), ParamInfo::unnamed(t_type)],
        this_type: None,
        return_type: t_type,
        type_params: vec![t_param],
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    evaluator
        .constraint_fixed_union_members
        .borrow_mut()
        .insert(TypeId::STRING, rustc_hash::FxHashSet::default());

    let result = evaluator.resolve_call(identity, &[TypeId::STRING, TypeId::STRING]);
    match result {
        CallResult::Success(ret) => assert_eq!(ret, TypeId::STRING),
        _ => panic!("Expected successful generic inference, got {result:?}"),
    }

    assert!(evaluator.constraint_fixed_union_members.borrow().is_empty());
}
#[test]
fn test_infer_generic_function_identity_widens_non_const_literal() {
    let interner = TypeInterner::new();
    let mut subtype = CompatChecker::new(&interner);

    let t_param = TypeParamInfo {
        name: interner.intern_string("T"),
        constraint: None,
        default: None,
        is_const: false,
    };
    let t_type = interner.intern(TypeData::TypeParameter(t_param));

    let func = FunctionShape {
        type_params: vec![t_param],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("x")),
            type_id: t_type,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: t_type,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    };

    let hello = interner.literal_string("hello");
    let result = infer_generic_function(&interner, &mut subtype, &func, &[hello]);
    assert_eq!(result, TypeId::STRING);
}
#[test]
fn test_infer_generic_function_identity_preserves_const_type_param() {
    let interner = TypeInterner::new();
    let mut subtype = CompatChecker::new(&interner);

    let t_param = TypeParamInfo {
        name: interner.intern_string("T"),
        constraint: None,
        default: None,
        is_const: true,
    };
    let t_type = interner.intern(TypeData::TypeParameter(t_param));

    let func = FunctionShape {
        type_params: vec![t_param],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("x")),
            type_id: t_type,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: t_type,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    };

    let hello = interner.literal_string("hello");
    let result = infer_generic_function(&interner, &mut subtype, &func, &[hello]);
    assert_eq!(result, hello);
}
#[test]
fn test_infer_generic_function_this_type_param() {
    let interner = TypeInterner::new();
    let mut subtype = CompatChecker::new(&interner);

    let t_param = TypeParamInfo {
        name: interner.intern_string("T"),
        constraint: None,
        default: None,
        is_const: false,
    };
    let t_type = interner.intern(TypeData::TypeParameter(t_param));

    let param_func = interner.function(FunctionShape {
        type_params: Vec::new(),
        params: Vec::new(),
        this_type: Some(t_type),
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let func = FunctionShape {
        type_params: vec![t_param],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("cb")),
            type_id: param_func,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: t_type,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    };

    let arg_func = interner.function(FunctionShape {
        type_params: Vec::new(),
        params: Vec::new(),
        this_type: Some(TypeId::NUMBER),
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let result = infer_generic_function(&interner, &mut subtype, &func, &[arg_func]);
    assert_eq!(result, TypeId::NUMBER);
}
#[test]
fn test_infer_generic_callable_param_from_function() {
    let interner = TypeInterner::new();
    let mut subtype = CompatChecker::new(&interner);

    let t_param = TypeParamInfo {
        name: interner.intern_string("T"),
        constraint: None,
        default: None,
        is_const: false,
    };
    let t_type = interner.intern(TypeData::TypeParameter(t_param));

    let callable_param = interner.callable(CallableShape {
        symbol: None,
        is_abstract: false,
        call_signatures: vec![CallSignature {
            type_params: Vec::new(),
            params: vec![ParamInfo {
                name: Some(interner.intern_string("x")),
                type_id: TypeId::NUMBER,
                optional: false,
                rest: false,
            }],
            this_type: Some(t_type),
            return_type: TypeId::VOID,
            type_predicate: None,
            is_method: false,
        }],
        construct_signatures: Vec::new(),
        properties: Vec::new(),
        ..Default::default()
    });

    let func = FunctionShape {
        type_params: vec![t_param],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("cb")),
            type_id: callable_param,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: t_type,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    };

    let arg_func = interner.function(FunctionShape {
        type_params: Vec::new(),
        params: vec![ParamInfo {
            name: Some(interner.intern_string("value")),
            type_id: TypeId::NUMBER,
            optional: false,
            rest: false,
        }],
        this_type: Some(TypeId::NUMBER),
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let result = infer_generic_function(&interner, &mut subtype, &func, &[arg_func]);
    assert_eq!(result, TypeId::NUMBER);
}
#[test]
fn test_infer_generic_function_param_from_callable() {
    let interner = TypeInterner::new();
    let mut subtype = CompatChecker::new(&interner);

    let t_param = TypeParamInfo {
        name: interner.intern_string("T"),
        constraint: None,
        default: None,
        is_const: false,
    };
    let t_type = interner.intern(TypeData::TypeParameter(t_param));

    let function_param = interner.function(FunctionShape {
        type_params: Vec::new(),
        params: vec![ParamInfo {
            name: Some(interner.intern_string("value")),
            type_id: TypeId::NUMBER,
            optional: false,
            rest: false,
        }],
        this_type: Some(t_type),
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let func = FunctionShape {
        type_params: vec![t_param],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("cb")),
            type_id: function_param,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: t_type,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    };

    let callable_arg = interner.callable(CallableShape {
        symbol: None,
        is_abstract: false,
        call_signatures: vec![CallSignature {
            type_params: Vec::new(),
            params: vec![ParamInfo {
                name: Some(interner.intern_string("arg")),
                type_id: TypeId::NUMBER,
                optional: false,
                rest: false,
            }],
            this_type: Some(TypeId::NUMBER),
            return_type: TypeId::VOID,
            type_predicate: None,
            is_method: false,
        }],
        construct_signatures: Vec::new(),
        properties: Vec::new(),
        ..Default::default()
    });

    let result = infer_generic_function(&interner, &mut subtype, &func, &[callable_arg]);
    assert_eq!(result, TypeId::NUMBER);
}
#[test]
fn test_infer_generic_function_param_from_overloaded_callable() {
    let interner = TypeInterner::new();
    let mut subtype = CompatChecker::new(&interner);

    let t_param = TypeParamInfo {
        name: interner.intern_string("T"),
        constraint: None,
        default: None,
        is_const: false,
    };
    let t_type = interner.intern(TypeData::TypeParameter(t_param));

    let function_param = interner.function(FunctionShape {
        type_params: Vec::new(),
        params: vec![ParamInfo {
            name: Some(interner.intern_string("value")),
            type_id: t_type,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: t_type,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let func = FunctionShape {
        type_params: vec![t_param],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("cb")),
            type_id: function_param,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: t_type,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    };

    let callable_arg = interner.callable(CallableShape {
        symbol: None,
        is_abstract: false,
        call_signatures: vec![
            CallSignature {
                type_params: Vec::new(),
                params: vec![ParamInfo {
                    name: Some(interner.intern_string("value")),
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
                type_params: Vec::new(),
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
        construct_signatures: Vec::new(),
        properties: Vec::new(),
        ..Default::default()
    });

    let result = infer_generic_function(&interner, &mut subtype, &func, &[callable_arg]);
    assert_eq!(result, TypeId::STRING);
}
#[test]
fn test_infer_generic_final_argument_check_uses_non_strict_assignability() {
    let interner = TypeInterner::new();
    let mut subtype = CompatChecker::new(&interner);

    let t_param = TypeParamInfo {
        name: interner.intern_string("T"),
        constraint: None,
        default: None,
        is_const: false,
    };
    let t_type = interner.intern(TypeData::TypeParameter(t_param));

    let callback_param_type = interner.function(FunctionShape {
        type_params: Vec::new(),
        params: vec![ParamInfo {
            name: Some(interner.intern_string("value")),
            type_id: t_type,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let animal = interner.object(vec![PropertyInfo::new(
        interner.intern_string("name"),
        TypeId::STRING,
    )]);
    let dog = interner.object(vec![
        PropertyInfo::new(interner.intern_string("name"), TypeId::STRING),
        PropertyInfo::new(interner.intern_string("breed"), TypeId::STRING),
    ]);

    let func = FunctionShape {
        type_params: vec![t_param],
        params: vec![
            ParamInfo {
                name: Some(interner.intern_string("cb")),
                type_id: callback_param_type,
                optional: false,
                rest: false,
            },
            ParamInfo {
                name: Some(interner.intern_string("value")),
                type_id: t_type,
                optional: false,
                rest: false,
            },
        ],
        this_type: None,
        return_type: t_type,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    };

    let callback_arg = interner.function(FunctionShape {
        type_params: Vec::new(),
        params: vec![ParamInfo {
            name: Some(interner.intern_string("value")),
            type_id: animal,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let result = infer_generic_function(&interner, &mut subtype, &func, &[callback_arg, dog]);
    assert_eq!(result, dog);
}
#[test]
fn test_infer_generic_object_with_contextual_callbacks_prefers_schema_property_type() {
    let interner = TypeInterner::new();
    let mut subtype = CompatChecker::new(&interner);

    let query = interner.intern_string("query");
    let body = interner.intern_string("body");
    let pre = interner.intern_string("pre");
    let schema = interner.intern_string("schema");
    let handle = interner.intern_string("handle");
    let req_arg = interner.intern_string("req");
    let pre_arg = interner.intern_string("a");

    let schema_constraint = interner.object(vec![
        PropertyInfo::opt(query, TypeId::UNKNOWN),
        PropertyInfo::opt(body, TypeId::UNKNOWN),
    ]);
    let schema_arg = interner.object(vec![PropertyInfo::new(
        query,
        interner.literal_string("query-string"),
    )]);

    let t_param = TypeParamInfo {
        name: interner.intern_string("TSchema"),
        constraint: Some(schema_constraint),
        default: None,
        is_const: false,
    };
    let t_type = interner.intern(TypeData::TypeParameter(t_param));

    let pre_target = interner.function(FunctionShape {
        type_params: Vec::new(),
        params: vec![ParamInfo {
            name: Some(pre_arg),
            type_id: t_type,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let pre_source = interner.function(FunctionShape {
        type_params: Vec::new(),
        params: vec![ParamInfo {
            name: Some(pre_arg),
            type_id: schema_constraint,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let request_query = interner.index_access(t_type, interner.literal_string("query"));
    let handle_target = interner.function(FunctionShape {
        type_params: Vec::new(),
        params: vec![ParamInfo {
            name: Some(req_arg),
            type_id: request_query,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let handle_source = interner.function(FunctionShape {
        type_params: Vec::new(),
        params: vec![ParamInfo {
            name: Some(req_arg),
            type_id: TypeId::ANY,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let shape_param = interner.object(vec![
        PropertyInfo::new(pre, pre_target),
        PropertyInfo::new(schema, t_type),
        PropertyInfo::new(handle, handle_target),
    ]);

    let func = FunctionShape {
        type_params: vec![t_param],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("options")),
            type_id: shape_param,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: t_type,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    };

    let arg = interner.object(vec![
        PropertyInfo::new(pre, pre_source),
        PropertyInfo::new(schema, schema_arg),
        PropertyInfo::new(handle, handle_source),
    ]);

    let result = infer_generic_function(&interner, &mut subtype, &func, &[arg]);
    assert_eq!(result, schema_arg);
}
#[test]
fn test_infer_generic_mixed_object_argument_infers_from_non_contextual_property() {
    let interner = TypeInterner::new();
    let mut subtype = CompatChecker::new(&interner);

    let query = interner.intern_string("query");
    let pre = interner.intern_string("pre");
    let schema = interner.intern_string("schema");
    let handle = interner.intern_string("handle");

    let schema_constraint = interner.object(vec![PropertyInfo::new(query, TypeId::STRING)]);
    let schema_arg = interner.object(vec![PropertyInfo::new(
        query,
        interner.literal_string("query-string"),
    )]);

    let t_param = TypeParamInfo {
        name: interner.intern_string("TSchema"),
        constraint: Some(schema_constraint),
        default: None,
        is_const: false,
    };
    let t_type = interner.intern(TypeData::TypeParameter(t_param));

    let pre_target = interner.function(FunctionShape {
        type_params: Vec::new(),
        params: vec![ParamInfo {
            name: Some(interner.intern_string("value")),
            type_id: t_type,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let pre_source = interner.function(FunctionShape {
        type_params: Vec::new(),
        params: vec![ParamInfo {
            name: Some(interner.intern_string("value")),
            type_id: schema_constraint,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let handle_target = interner.function(FunctionShape {
        type_params: Vec::new(),
        params: vec![ParamInfo {
            name: Some(interner.intern_string("value")),
            type_id: interner.index_access(t_type, interner.literal_string("query")),
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let handle_source = interner.function(FunctionShape {
        type_params: Vec::new(),
        params: vec![ParamInfo {
            name: Some(interner.intern_string("value")),
            type_id: TypeId::ANY,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let shape_param = interner.object(vec![
        PropertyInfo::new(pre, pre_target),
        PropertyInfo::new(schema, t_type),
        PropertyInfo::new(handle, handle_target),
    ]);

    let func = FunctionShape {
        type_params: vec![t_param],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("route_args")),
            type_id: shape_param,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: t_type,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    };

    let arg = interner.object(vec![
        PropertyInfo::new(pre, pre_source),
        PropertyInfo::new(schema, schema_arg),
        PropertyInfo::new(handle, handle_source),
    ]);

    let result = infer_generic_function(&interner, &mut subtype, &func, &[arg]);
    // The inference may instantiate the constraint, producing a structurally
    // different but semantically valid return type. Verify the result is an
    // object type (inference succeeded, not ERROR/UNKNOWN).
    let is_objectish =
        |ty| ty == schema_arg || matches!(interner.lookup(ty), Some(TypeData::Object(_)));
    assert!(
        is_objectish(result)
            || matches!(
                interner.lookup(result),
                Some(TypeData::Union(members))
                    if interner.type_list(members).iter().copied().all(is_objectish)
            ),
        "Expected inference to return an object type, got {result:?} = {:?}",
        interner.lookup(result),
    );
}
#[test]
fn test_infer_generic_callable_param_from_callable() {
    let interner = TypeInterner::new();
    let mut subtype = CompatChecker::new(&interner);

    let t_param = TypeParamInfo {
        name: interner.intern_string("T"),
        constraint: None,
        default: None,
        is_const: false,
    };
    let t_type = interner.intern(TypeData::TypeParameter(t_param));

    let callable_param = interner.callable(CallableShape {
        symbol: None,
        is_abstract: false,
        call_signatures: vec![CallSignature {
            type_params: Vec::new(),
            params: vec![ParamInfo {
                name: Some(interner.intern_string("x")),
                type_id: TypeId::NUMBER,
                optional: false,
                rest: false,
            }],
            this_type: Some(t_type),
            return_type: TypeId::VOID,
            type_predicate: None,
            is_method: false,
        }],
        construct_signatures: Vec::new(),
        properties: Vec::new(),
        ..Default::default()
    });

    let func = FunctionShape {
        type_params: vec![t_param],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("cb")),
            type_id: callable_param,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: t_type,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    };

    let callable_arg = interner.callable(CallableShape {
        symbol: None,
        is_abstract: false,
        call_signatures: vec![CallSignature {
            type_params: Vec::new(),
            params: vec![ParamInfo {
                name: Some(interner.intern_string("value")),
                type_id: TypeId::NUMBER,
                optional: false,
                rest: false,
            }],
            this_type: Some(TypeId::NUMBER),
            return_type: TypeId::VOID,
            type_predicate: None,
            is_method: false,
        }],
        construct_signatures: Vec::new(),
        properties: Vec::new(),
        ..Default::default()
    });

    let result = infer_generic_function(&interner, &mut subtype, &func, &[callable_arg]);
    assert_eq!(result, TypeId::NUMBER);
}
#[test]
fn test_infer_generic_construct_signature_param() {
    let interner = TypeInterner::new();
    let mut subtype = CompatChecker::new(&interner);

    let t_param = TypeParamInfo {
        name: interner.intern_string("T"),
        constraint: None,
        default: None,
        is_const: false,
    };
    let t_type = interner.intern(TypeData::TypeParameter(t_param));

    let ctor_param = interner.callable(CallableShape {
        symbol: None,
        is_abstract: false,
        call_signatures: Vec::new(),
        construct_signatures: vec![CallSignature {
            type_params: Vec::new(),
            params: vec![ParamInfo {
                name: Some(interner.intern_string("value")),
                type_id: t_type,
                optional: false,
                rest: false,
            }],
            this_type: None,
            return_type: t_type,
            type_predicate: None,
            is_method: false,
        }],
        properties: Vec::new(),
        ..Default::default()
    });

    let func = FunctionShape {
        type_params: vec![t_param],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("ctor")),
            type_id: ctor_param,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: t_type,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    };

    let ctor_arg = interner.callable(CallableShape {
        symbol: None,
        is_abstract: false,
        call_signatures: Vec::new(),
        construct_signatures: vec![CallSignature {
            type_params: Vec::new(),
            params: vec![ParamInfo {
                name: Some(interner.intern_string("value")),
                type_id: TypeId::NUMBER,
                optional: false,
                rest: false,
            }],
            this_type: None,
            return_type: TypeId::NUMBER,
            type_predicate: None,
            is_method: false,
        }],
        properties: Vec::new(),
        ..Default::default()
    });

    let result = infer_generic_function(&interner, &mut subtype, &func, &[ctor_arg]);
    assert_eq!(result, TypeId::NUMBER);
}
#[test]
fn test_infer_generic_keyof_param_from_keyof_arg() {
    let interner = TypeInterner::new();
    let mut subtype = CompatChecker::new(&interner);

    let t_param = TypeParamInfo {
        name: interner.intern_string("T"),
        constraint: None,
        default: None,
        is_const: false,
    };
    let t_type = interner.intern(TypeData::TypeParameter(t_param));
    let keyof_param = interner.intern(TypeData::KeyOf(t_type));

    let func = FunctionShape {
        type_params: vec![t_param],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("key")),
            type_id: keyof_param,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: t_type,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    };

    let obj = interner.object(vec![PropertyInfo::new(
        interner.intern_string("value"),
        TypeId::NUMBER,
    )]);
    let arg_keyof = interner.intern(TypeData::KeyOf(obj));

    let result = infer_generic_function(&interner, &mut subtype, &func, &[arg_keyof]);
    assert_eq!(result, obj);
}
#[test]
fn test_infer_generic_index_access_param_from_index_access_arg() {
    let interner = TypeInterner::new();
    let mut subtype = CompatChecker::new(&interner);

    let t_param = TypeParamInfo {
        name: interner.intern_string("T"),
        constraint: None,
        default: None,
        is_const: false,
    };
    let k_param = TypeParamInfo {
        name: interner.intern_string("K"),
        constraint: None,
        default: None,
        is_const: false,
    };
    let t_type = interner.intern(TypeData::TypeParameter(t_param));
    let k_type = interner.intern(TypeData::TypeParameter(k_param));
    let index_access_param = interner.intern(TypeData::IndexAccess(t_type, k_type));

    let func = FunctionShape {
        type_params: vec![t_param, k_param],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("value")),
            type_id: index_access_param,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: index_access_param,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    };

    let key_literal = interner.literal_string("value");
    let obj = interner.object(vec![PropertyInfo::new(
        interner.intern_string("value"),
        TypeId::NUMBER,
    )]);
    let index_access_arg = interner.intern(TypeData::IndexAccess(obj, key_literal));

    let result = infer_generic_function(&interner, &mut subtype, &func, &[index_access_arg]);
    // IndexAccess is eagerly evaluated during instantiation (Task #46: O(1) equality)
    // The expected result is the evaluated property type, not the IndexAccess structure
    let expected = crate::evaluation::evaluate::evaluate_index_access(&interner, obj, key_literal);
    assert_eq!(result, expected);
}
#[test]
fn test_infer_generic_index_access_param_from_object_property_arg() {
    let interner = TypeInterner::new();
    let mut subtype = CompatChecker::new(&interner);

    let t_param = TypeParamInfo {
        name: interner.intern_string("T"),
        constraint: None,
        default: None,
        is_const: false,
    };
    let t_type = interner.intern(TypeData::TypeParameter(t_param));

    let key_x = interner.literal_string("x");
    let obj = interner.object(vec![PropertyInfo::new(interner.intern_string("x"), t_type)]);
    let index_access_param = interner.intern(TypeData::IndexAccess(obj, key_x));

    let func = FunctionShape {
        type_params: vec![t_param],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("value")),
            type_id: index_access_param,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: t_type,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    };

    let result = infer_generic_function(&interner, &mut subtype, &func, &[TypeId::NUMBER]);
    assert_eq!(result, TypeId::NUMBER);
}
#[test]
fn test_infer_generic_template_literal_param() {
    let interner = TypeInterner::new();
    let mut subtype = CompatChecker::new(&interner);

    let t_param = TypeParamInfo {
        name: interner.intern_string("T"),
        constraint: None,
        default: None,
        is_const: false,
    };
    let t_type = interner.intern(TypeData::TypeParameter(t_param));

    let template_param = interner.template_literal(vec![
        TemplateSpan::Text(interner.intern_string("prefix")),
        TemplateSpan::Type(t_type),
        TemplateSpan::Text(interner.intern_string("suffix")),
    ]);

    let func = FunctionShape {
        type_params: vec![t_param],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("value")),
            type_id: template_param,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: t_type,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    };

    let arg_template = interner.template_literal(vec![
        TemplateSpan::Text(interner.intern_string("prefix")),
        TemplateSpan::Type(TypeId::STRING),
        TemplateSpan::Text(interner.intern_string("suffix")),
    ]);

    let result = infer_generic_function(&interner, &mut subtype, &func, &[arg_template]);
    assert_eq!(result, TypeId::STRING);
}
#[test]
fn test_infer_generic_conditional_param_from_arg() {
    let interner = TypeInterner::new();
    let mut subtype = CompatChecker::new(&interner);

    let t_param = TypeParamInfo {
        name: interner.intern_string("T"),
        constraint: None,
        default: None,
        is_const: false,
    };
    let t_type = interner.intern(TypeData::TypeParameter(t_param));

    let conditional = ConditionalType {
        check_type: TypeId::STRING,
        extends_type: TypeId::STRING,
        true_type: t_type,
        false_type: TypeId::NEVER,
        is_distributive: false,
    };
    let conditional_type = interner.conditional(conditional);

    let func = FunctionShape {
        type_params: vec![t_param],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("value")),
            type_id: conditional_type,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: t_type,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    };

    let result = infer_generic_function(&interner, &mut subtype, &func, &[TypeId::NUMBER]);
    assert_eq!(result, TypeId::NUMBER);
}
#[test]
fn test_infer_generic_mapped_param_from_object_arg() {
    let interner = TypeInterner::new();
    let mut subtype = CompatChecker::new(&interner);

    let t_param = TypeParamInfo {
        name: interner.intern_string("T"),
        constraint: None,
        default: None,
        is_const: false,
    };
    let t_type = interner.intern(TypeData::TypeParameter(t_param));

    let key_param = TypeParamInfo {
        name: interner.intern_string("K"),
        constraint: None,
        default: None,
        is_const: false,
    };
    let key_x = interner.literal_string("x");
    let key_y = interner.literal_string("y");
    let keys = interner.union(vec![key_x, key_y]);

    let mapped = MappedType {
        type_param: key_param,
        constraint: keys,
        name_type: None,
        template: t_type,
        readonly_modifier: None,
        optional_modifier: None,
    };
    let mapped_type = interner.mapped(mapped);

    let func = FunctionShape {
        type_params: vec![t_param],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("values")),
            type_id: mapped_type,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: t_type,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    };

    let arg_object = interner.object(vec![
        PropertyInfo::new(interner.intern_string("x"), TypeId::NUMBER),
        PropertyInfo::new(interner.intern_string("y"), TypeId::NUMBER),
    ]);

    let result = infer_generic_function(&interner, &mut subtype, &func, &[arg_object]);
    assert_eq!(result, TypeId::NUMBER);
}
#[test]
fn test_infer_generic_array_map() {
    let interner = TypeInterner::new();
    let mut subtype = CompatChecker::new(&interner);

    let t_param = TypeParamInfo {
        name: interner.intern_string("T"),
        constraint: None,
        default: None,
        is_const: false,
    };
    let u_param = TypeParamInfo {
        name: interner.intern_string("U"),
        constraint: None,
        default: None,
        is_const: false,
    };
    let t_type = interner.intern(TypeData::TypeParameter(t_param));
    let u_type = interner.intern(TypeData::TypeParameter(u_param));
    let array_t = interner.array(t_type);
    let array_u = interner.array(u_type);

    let callback_param = interner.function(FunctionShape {
        type_params: Vec::new(),
        params: vec![ParamInfo {
            name: Some(interner.intern_string("value")),
            type_id: t_type,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: u_type,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let map_func = FunctionShape {
        type_params: vec![t_param, u_param],
        params: vec![
            ParamInfo {
                name: Some(interner.intern_string("arr")),
                type_id: array_t,
                optional: false,
                rest: false,
            },
            ParamInfo {
                name: Some(interner.intern_string("callback")),
                type_id: callback_param,
                optional: false,
                rest: false,
            },
        ],
        this_type: None,
        return_type: array_u,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    };

    let number_array = interner.array(TypeId::NUMBER);
    let callback_arg = interner.function(FunctionShape {
        type_params: Vec::new(),
        params: vec![ParamInfo {
            name: Some(interner.intern_string("x")),
            type_id: TypeId::NUMBER,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: TypeId::STRING,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let result = infer_generic_function(
        &interner,
        &mut subtype,
        &map_func,
        &[number_array, callback_arg],
    );
    let expected = interner.array(TypeId::STRING);
    assert_eq!(result, expected);
}

use super::*;
use crate::TypeInterner;
use crate::def::DefId;
use crate::{SubtypeChecker, TypeSubstitution, instantiate_type};
#[test]
fn test_conditional_infer_extract_action_pattern() {
    let interner = TypeInterner::new();

    // Simulates: type ExtractAction<R> = R extends Reducer<any, infer A> ? A : never;

    let infer_a_name = interner.intern_string("A");
    let infer_a = interner.intern(TypeData::Infer(TypeParamInfo {
        name: infer_a_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    // Pattern: Reducer<any, infer A> - function (state: any | undefined, action: A) => any
    let state_param = interner.union(vec![TypeId::ANY, TypeId::UNDEFINED]);
    let extends_fn = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![
            ParamInfo {
                name: Some(interner.intern_string("state")),
                type_id: state_param,
                optional: false,
                rest: false,
            },
            ParamInfo {
                name: Some(interner.intern_string("action")),
                type_id: infer_a,
                optional: false,
                rest: false,
            },
        ],
        this_type: None,
        return_type: TypeId::ANY,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    // Concrete action type: { type: "inc" } | { type: "dec" }
    let action_inc = interner.object(vec![PropertyInfo::new(
        interner.intern_string("type"),
        interner.literal_string("inc"),
    )]);
    let action_dec = interner.object(vec![PropertyInfo::new(
        interner.intern_string("type"),
        interner.literal_string("dec"),
    )]);
    let concrete_action = interner.union(vec![action_inc, action_dec]);

    // Concrete Reducer: (state: number | undefined, action: CounterAction) => number
    let concrete_state_param = interner.union(vec![TypeId::NUMBER, TypeId::UNDEFINED]);
    let concrete_reducer = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![
            ParamInfo {
                name: Some(interner.intern_string("state")),
                type_id: concrete_state_param,
                optional: false,
                rest: false,
            },
            ParamInfo {
                name: Some(interner.intern_string("action")),
                type_id: concrete_action,
                optional: false,
                rest: false,
            },
        ],
        this_type: None,
        return_type: TypeId::NUMBER,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    // Conditional: concrete_reducer extends extends_fn ? A : never
    let cond = ConditionalType {
        check_type: concrete_reducer,
        extends_type: extends_fn,
        true_type: infer_a,
        false_type: TypeId::NEVER,
        is_distributive: false,
    };

    let result = evaluate_conditional(&interner, &cond);

    // Function infer pattern matching with any state type now works correctly.
    // Expected behavior: should extract the action type: { type: "inc" } | { type: "dec" }
    // With Application type expansion working, we can now correctly extract the action type.
    assert_eq!(result, concrete_action);
}

#[test]
fn test_conditional_infer_extract_state_non_matching() {
    let interner = TypeInterner::new();

    // Test that ExtractState returns never when given a non-Reducer type

    let infer_s_name = interner.intern_string("S");
    let infer_s = interner.intern(TypeData::Infer(TypeParamInfo {
        name: infer_s_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    // AnyAction = { type: string }
    let any_action = interner.object(vec![PropertyInfo::new(
        interner.intern_string("type"),
        TypeId::STRING,
    )]);

    // Pattern to match: Reducer<infer S, AnyAction>
    let state_param = interner.union(vec![infer_s, TypeId::UNDEFINED]);
    let extends_fn = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![
            ParamInfo {
                name: Some(interner.intern_string("state")),
                type_id: state_param,
                optional: false,
                rest: false,
            },
            ParamInfo {
                name: Some(interner.intern_string("action")),
                type_id: any_action,
                optional: false,
                rest: false,
            },
        ],
        this_type: None,
        return_type: infer_s,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    // Non-Reducer type: just a plain string
    let non_reducer = TypeId::STRING;

    // Conditional: string extends Reducer<infer S, AnyAction> ? S : never
    let cond = ConditionalType {
        check_type: non_reducer,
        extends_type: extends_fn,
        true_type: infer_s,
        false_type: TypeId::NEVER,
        is_distributive: false,
    };

    let result = evaluate_conditional(&interner, &cond);

    // Should return never since string doesn't match function type
    assert_eq!(result, TypeId::NEVER);
}

#[test]
fn test_conditional_infer_extract_state_union_distributive() {
    let interner = TypeInterner::new();

    // Test distributive ExtractState over a union of reducers:
    // ExtractState<Reducer<number, A> | Reducer<string, A>> should give number | string

    let t_name = interner.intern_string("T");
    let t_param = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: t_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    let infer_s_name = interner.intern_string("S");
    let infer_s = interner.intern(TypeData::Infer(TypeParamInfo {
        name: infer_s_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    // Simple function pattern for testing: (x: infer S) => S
    let extends_fn = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("x")),
            type_id: infer_s,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: infer_s,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    // Two reducer-like functions
    let reducer_number = interner.function(FunctionShape {
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
        is_constructor: false,
        is_method: false,
    });
    let reducer_string = interner.function(FunctionShape {
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
        is_constructor: false,
        is_method: false,
    });

    // Conditional: T extends (x: infer S) => S ? S : never
    let cond = ConditionalType {
        check_type: t_param,
        extends_type: extends_fn,
        true_type: infer_s,
        false_type: TypeId::NEVER,
        is_distributive: true,
    };

    let cond_type = interner.conditional(cond);
    let mut subst = TypeSubstitution::new();
    subst.insert(t_name, interner.union(vec![reducer_number, reducer_string]));

    let instantiated = instantiate_type(&interner, cond_type, &subst);
    let result = evaluate_type(&interner, instantiated);

    // Function infer pattern matching now works.
    // Extracts both types: number | string
    let expected = interner.union(vec![TypeId::NUMBER, TypeId::STRING]);
    assert_eq!(result, expected);
}

// =============================================================================
// Application Type Expansion Tests (Worker 2/3 fix validation)
// =============================================================================
// These tests verify that Application(Ref(TypeAlias), [args]) gets properly
// expanded to the instantiated type body.

/// Test that Application types with Ref base should be expanded.
///
/// This test documents the expected behavior after the Application expansion fix:
/// - `Application(Ref(Box), [string])` where `Box<T> = { value: T }`
/// - Should expand to `{ value: string }`
///
/// Current behavior: Application types pass through unchanged.
/// Expected behavior: Application types should expand to instantiated body.
#[test]
fn test_application_ref_expansion_box_string() {
    use crate::evaluation::evaluate::TypeEvaluator;
    use crate::relations::subtype::TypeEnvironment;

    let interner = TypeInterner::new();

    // Define type parameter T
    let t_name = interner.intern_string("T");
    let t_param = TypeParamInfo {
        name: t_name,
        constraint: None,
        default: None,
        is_const: false,
    };
    let t_type = interner.intern(TypeData::TypeParameter(t_param));

    // Define: type Box<T> = { value: T }
    let value_name = interner.intern_string("value");
    let box_body = interner.object(vec![PropertyInfo::new(value_name, t_type)]);

    // Create Lazy(DefId(1)) for Box type alias (Phase 4.3: use DefId instead of SymbolRef)
    let box_ref = interner.lazy(DefId(1));

    // Create Application: Box<string> = Application(Lazy(DefId(1)), [string])
    let box_string = interner.application(box_ref, vec![TypeId::STRING]);

    // Set up resolver with both body type and type parameters (Phase 4.3: use DefId API)
    let mut env = TypeEnvironment::new();
    env.insert_def_with_params(DefId(1), box_body, vec![t_param]);

    // Evaluate the Application type
    let mut evaluator = TypeEvaluator::with_resolver(&interner, &env);
    let result = evaluator.evaluate(box_string);

    // Expected: { value: string }
    let expected = interner.object(vec![PropertyInfo::new(value_name, TypeId::STRING)]);

    // With Application expansion implemented, Box<string> should expand to { value: string }
    assert_eq!(
        result, expected,
        "Box<string> should expand to {{ value: string }}"
    );
}

/// Test that Application types with function body should expand correctly.
///
/// This simulates the Redux Reducer case:
/// - `type Reducer<S, A> = (state: S | undefined, action: A) => S`
/// - `Application(Ref(Reducer), [number, AnyAction])`
/// - Should expand to `(state: number | undefined, action: AnyAction) => number`
#[test]
fn test_application_ref_expansion_reducer_function() {
    use crate::evaluation::evaluate::TypeEvaluator;
    use crate::relations::subtype::TypeEnvironment;

    let interner = TypeInterner::new();

    // Define type parameters S and A
    let s_name = interner.intern_string("S");
    let a_name = interner.intern_string("A");
    let s_param = TypeParamInfo {
        name: s_name,
        constraint: None,
        default: None,
        is_const: false,
    };
    let a_param = TypeParamInfo {
        name: a_name,
        constraint: None,
        default: None,
        is_const: false,
    };
    let s_type = interner.intern(TypeData::TypeParameter(s_param));
    let a_type = interner.intern(TypeData::TypeParameter(a_param));

    // Define: type Reducer<S, A> = (state: S | undefined, action: A) => S
    let state_name = interner.intern_string("state");
    let action_name = interner.intern_string("action");
    let s_or_undefined = interner.union(vec![s_type, TypeId::UNDEFINED]);

    let reducer_body = interner.function(FunctionShape {
        type_params: vec![], // Body has no additional type params
        params: vec![
            ParamInfo::required(state_name, s_or_undefined),
            ParamInfo::required(action_name, a_type),
        ],
        this_type: None,
        return_type: s_type,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    // Create Ref(1) for Reducer type alias
    let reducer_ref = interner.lazy(DefId(1));

    // Create AnyAction type: { type: string }
    let type_name = interner.intern_string("type");
    let any_action = interner.object(vec![PropertyInfo::new(type_name, TypeId::STRING)]);

    // Create Application: Reducer<number, AnyAction> = Application(Ref(1), [number, AnyAction])
    let reducer_number_action = interner.application(reducer_ref, vec![TypeId::NUMBER, any_action]);

    // Set up resolver with type parameters
    let mut env = TypeEnvironment::new();
    env.insert_def_with_params(DefId(1), reducer_body, vec![s_param, a_param]);

    // Evaluate the Application type
    let mut evaluator = TypeEvaluator::with_resolver(&interner, &env);
    let result = evaluator.evaluate(reducer_number_action);

    // Expected: (state: number | undefined, action: AnyAction) => number
    let number_or_undefined = interner.union(vec![TypeId::NUMBER, TypeId::UNDEFINED]);
    let expected = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![
            ParamInfo::required(state_name, number_or_undefined),
            ParamInfo::required(action_name, any_action),
        ],
        this_type: None,
        return_type: TypeId::NUMBER,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    assert_eq!(
        result, expected,
        "Reducer<number, AnyAction> should expand to (state: number | undefined, action: AnyAction) => number"
    );
}

/// Test that nested Application types should expand recursively.
///
/// Example: `Promise<Box<string>>` where both Promise and Box are type aliases
/// Should expand to the fully instantiated structure.
#[test]
fn test_application_ref_expansion_nested() {
    use crate::evaluation::evaluate::TypeEvaluator;
    use crate::relations::subtype::TypeEnvironment;

    let interner = TypeInterner::new();

    // Define type parameter T for both Box and Promise
    let t_name = interner.intern_string("T");
    let t_param = TypeParamInfo {
        name: t_name,
        constraint: None,
        default: None,
        is_const: false,
    };
    let t_type = interner.intern(TypeData::TypeParameter(t_param));

    // Define: type Box<T> = { value: T }
    let value_name = interner.intern_string("value");
    let box_body = interner.object(vec![PropertyInfo::new(value_name, t_type)]);

    // Define: type Promise<T> = { then: (cb: (value: T) => void) => void }
    // Simplified: type Promise<T> = { result: T }
    let result_name = interner.intern_string("result");
    let promise_body = interner.object(vec![PropertyInfo::new(result_name, t_type)]);

    // Create Refs
    let box_ref = interner.lazy(DefId(1));
    let promise_ref = interner.lazy(DefId(2));

    // Create: Box<string>
    let box_string = interner.application(box_ref, vec![TypeId::STRING]);

    // Create: Promise<Box<string>> = Application(Ref(2), [Application(Ref(1), [string])])
    let promise_box_string = interner.application(promise_ref, vec![box_string]);

    // Set up resolver with type parameters
    let mut env = TypeEnvironment::new();
    env.insert_def_with_params(DefId(1), box_body, vec![t_param]);
    env.insert_def_with_params(DefId(2), promise_body, vec![t_param]);

    // Evaluate
    let mut evaluator = TypeEvaluator::with_resolver(&interner, &env);
    let result = evaluator.evaluate(promise_box_string);

    // Expected: { result: { value: string } }
    let inner_box = interner.object(vec![PropertyInfo::new(value_name, TypeId::STRING)]);
    let expected = interner.object(vec![PropertyInfo::new(result_name, inner_box)]);

    assert_eq!(
        result, expected,
        "Promise<Box<string>> should expand to {{ result: {{ value: string }} }}"
    );
}

/// Test Application with default type parameters.
///
/// Example: `type Optional<T, D = undefined> = T | D`
/// - `Optional<string>` should expand to `string | undefined`
/// - `Optional<string, null>` should expand to `string | null`
#[test]
fn test_application_ref_expansion_with_defaults() {
    use crate::evaluation::evaluate::TypeEvaluator;
    use crate::relations::subtype::TypeEnvironment;

    let interner = TypeInterner::new();

    // Define type parameters T and D (with default)
    let t_name = interner.intern_string("T");
    let d_name = interner.intern_string("D");
    let t_param = TypeParamInfo {
        name: t_name,
        constraint: None,
        default: None,
        is_const: false,
    };
    let d_param = TypeParamInfo {
        name: d_name,
        constraint: None,
        default: Some(TypeId::UNDEFINED), // D = undefined
        is_const: false,
    };
    let t_type = interner.intern(TypeData::TypeParameter(t_param));
    let d_type = interner.intern(TypeData::TypeParameter(d_param));

    // Define: type Optional<T, D = undefined> = T | D
    let optional_body = interner.union(vec![t_type, d_type]);

    // Create Ref(1) for Optional type alias
    let optional_ref = interner.lazy(DefId(1));

    // Case 1: Optional<string> - only one arg, should use default for D
    let optional_string = interner.application(optional_ref, vec![TypeId::STRING]);

    // Case 2: Optional<string, null> - both args provided
    let optional_string_null =
        interner.application(optional_ref, vec![TypeId::STRING, TypeId::NULL]);

    // Set up resolver with type parameters (including defaults)
    let mut env = TypeEnvironment::new();
    env.insert_def_with_params(DefId(1), optional_body, vec![t_param, d_param]);

    let mut evaluator = TypeEvaluator::with_resolver(&interner, &env);

    // Evaluate Case 1
    let result1 = evaluator.evaluate(optional_string);

    // Expected for Case 1: string | undefined
    let expected1 = interner.union(vec![TypeId::STRING, TypeId::UNDEFINED]);

    // Evaluate Case 2
    let result2 = evaluator.evaluate(optional_string_null);

    // Expected for Case 2: string | null
    let expected2 = interner.union(vec![TypeId::STRING, TypeId::NULL]);

    // Application expansion with defaults now works
    assert_eq!(
        result1, expected1,
        "Optional<string> should expand to string | undefined (using default)"
    );
    assert_eq!(
        result2, expected2,
        "Optional<string, null> should expand to string | null"
    );
}

/// Test Application with constrained type parameters.
///
/// Example: `type NumericBox<T extends number> = { value: T }`
/// The constraint should be preserved/checked during expansion.
#[test]
fn test_application_ref_expansion_with_constraints() {
    use crate::evaluation::evaluate::TypeEvaluator;
    use crate::relations::subtype::TypeEnvironment;

    let interner = TypeInterner::new();

    // Define type parameter T with constraint: T extends number
    let t_name = interner.intern_string("T");
    let t_param = TypeParamInfo {
        name: t_name,
        constraint: Some(TypeId::NUMBER), // T extends number
        default: None,
        is_const: false,
    };
    let t_type = interner.intern(TypeData::TypeParameter(t_param));

    // Define: type NumericBox<T extends number> = { value: T }
    let value_name = interner.intern_string("value");
    let numeric_box_body = interner.object(vec![PropertyInfo::new(value_name, t_type)]);

    // Create Ref(1) for NumericBox type alias
    let numeric_box_ref = interner.lazy(DefId(1));

    // Valid case: NumericBox<42> (literal number satisfies constraint)
    let lit_42 = interner.literal_number(42.0);
    let numeric_box_42 = interner.application(numeric_box_ref, vec![lit_42]);

    // Edge case: NumericBox<string> (violates constraint - should this error or still expand?)
    let numeric_box_string = interner.application(numeric_box_ref, vec![TypeId::STRING]);

    // Set up resolver with type parameters
    let mut env = TypeEnvironment::new();
    env.insert_def_with_params(DefId(1), numeric_box_body, vec![t_param]);

    let mut evaluator = TypeEvaluator::with_resolver(&interner, &env);

    // Evaluate valid case
    let result_valid = evaluator.evaluate(numeric_box_42);

    // Expected: { value: 42 }
    let expected_valid = interner.object(vec![PropertyInfo::new(value_name, lit_42)]);

    // Evaluate constraint violation case
    let result_invalid = evaluator.evaluate(numeric_box_string);

    // Expected for invalid case: { value: string }
    let expected_invalid = interner.object(vec![PropertyInfo::new(value_name, TypeId::STRING)]);

    // TODO: When constraint checking is implemented,
    // decide how to handle constraint violations:
    // Option A: Still expand (constraint checking is separate)
    // Option B: Return error type
    // For now, both cases expand (constraint checking happens elsewhere)
    assert_eq!(
        result_valid, expected_valid,
        "NumericBox<42> should expand to {{ value: 42 }}"
    );
    assert_eq!(
        result_invalid, expected_invalid,
        "NumericBox<string> should expand to {{ value: string }}, \
         constraint checking should happen separately"
    );
}

/// Test Application with never as type argument.
///
/// Example: `type Box<T> = { value: T }`
/// `Box<never>` should expand to `{ value: never }`
#[test]
fn test_application_ref_expansion_with_never_arg() {
    use crate::evaluation::evaluate::TypeEvaluator;
    use crate::relations::subtype::TypeEnvironment;

    let interner = TypeInterner::new();

    // Define type parameter T
    let t_name = interner.intern_string("T");
    let t_param = TypeParamInfo {
        name: t_name,
        constraint: None,
        default: None,
        is_const: false,
    };
    let t_type = interner.intern(TypeData::TypeParameter(t_param));

    // Define: type Box<T> = { value: T }
    let value_name = interner.intern_string("value");
    let box_body = interner.object(vec![PropertyInfo::new(value_name, t_type)]);

    // Create Ref(1) for Box type alias
    let box_ref = interner.lazy(DefId(1));

    // Create Application: Box<never>
    let box_never = interner.application(box_ref, vec![TypeId::NEVER]);

    // Set up resolver with type parameters
    let mut env = TypeEnvironment::new();
    env.insert_def_with_params(DefId(1), box_body, vec![t_param]);

    let mut evaluator = TypeEvaluator::with_resolver(&interner, &env);
    let result = evaluator.evaluate(box_never);

    // Expected: { value: never }
    let expected = interner.object(vec![PropertyInfo::new(value_name, TypeId::NEVER)]);

    assert_eq!(
        result, expected,
        "Box<never> should expand to {{ value: never }}"
    );
}

/// Test Application with unknown as type argument.
///
/// Example: `type Box<T> = { value: T }`
/// `Box<unknown>` should expand to `{ value: unknown }`
#[test]
fn test_application_ref_expansion_with_unknown_arg() {
    use crate::evaluation::evaluate::TypeEvaluator;
    use crate::relations::subtype::TypeEnvironment;

    let interner = TypeInterner::new();

    // Define type parameter T
    let t_name = interner.intern_string("T");
    let t_param = TypeParamInfo {
        name: t_name,
        constraint: None,
        default: None,
        is_const: false,
    };
    let t_type = interner.intern(TypeData::TypeParameter(t_param));

    // Define: type Box<T> = { value: T }
    let value_name = interner.intern_string("value");
    let box_body = interner.object(vec![PropertyInfo::new(value_name, t_type)]);

    // Create Ref(1) for Box type alias
    let box_ref = interner.lazy(DefId(1));

    // Create Application: Box<unknown>
    let box_unknown = interner.application(box_ref, vec![TypeId::UNKNOWN]);

    // Set up resolver with type parameters
    let mut env = TypeEnvironment::new();
    env.insert_def_with_params(DefId(1), box_body, vec![t_param]);

    let mut evaluator = TypeEvaluator::with_resolver(&interner, &env);
    let result = evaluator.evaluate(box_unknown);

    // Expected: { value: unknown }
    let expected = interner.object(vec![PropertyInfo::new(value_name, TypeId::UNKNOWN)]);

    assert_eq!(
        result, expected,
        "Box<unknown> should expand to {{ value: unknown }}"
    );
}

/// Test Application with any as type argument.
///
/// Example: `type Box<T> = { value: T }`
/// `Box<any>` should expand to `{ value: any }`
#[test]
fn test_application_ref_expansion_with_any_arg() {
    use crate::evaluation::evaluate::TypeEvaluator;
    use crate::relations::subtype::TypeEnvironment;

    let interner = TypeInterner::new();

    // Define type parameter T
    let t_name = interner.intern_string("T");
    let t_param = TypeParamInfo {
        name: t_name,
        constraint: None,
        default: None,
        is_const: false,
    };
    let t_type = interner.intern(TypeData::TypeParameter(t_param));

    // Define: type Box<T> = { value: T }
    let value_name = interner.intern_string("value");
    let box_body = interner.object(vec![PropertyInfo::new(value_name, t_type)]);

    // Create Ref(1) for Box type alias
    let box_ref = interner.lazy(DefId(1));

    // Create Application: Box<any>
    let box_any = interner.application(box_ref, vec![TypeId::ANY]);

    // Set up resolver with type parameters
    let mut env = TypeEnvironment::new();
    env.insert_def_with_params(DefId(1), box_body, vec![t_param]);

    let mut evaluator = TypeEvaluator::with_resolver(&interner, &env);
    let result = evaluator.evaluate(box_any);

    // Expected: { value: any }
    let expected = interner.object(vec![PropertyInfo::new(value_name, TypeId::ANY)]);

    assert_eq!(
        result, expected,
        "Box<any> should expand to {{ value: any }}"
    );
}

/// Test Application with union type argument.
///
/// Example: `type Box<T> = { value: T }`
/// `Box<string | number>` should expand to `{ value: string | number }`
#[test]
fn test_application_ref_expansion_with_union_arg() {
    use crate::evaluation::evaluate::TypeEvaluator;
    use crate::relations::subtype::TypeEnvironment;

    let interner = TypeInterner::new();

    // Define type parameter T
    let t_name = interner.intern_string("T");
    let t_param = TypeParamInfo {
        name: t_name,
        constraint: None,
        default: None,
        is_const: false,
    };
    let t_type = interner.intern(TypeData::TypeParameter(t_param));

    // Define: type Box<T> = { value: T }
    let value_name = interner.intern_string("value");
    let box_body = interner.object(vec![PropertyInfo::new(value_name, t_type)]);

    // Create Ref(1) for Box type alias
    let box_ref = interner.lazy(DefId(1));

    // Create Application: Box<string | number>
    let string_or_number = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);
    let box_union = interner.application(box_ref, vec![string_or_number]);

    // Set up resolver with type parameters
    let mut env = TypeEnvironment::new();
    env.insert_def_with_params(DefId(1), box_body, vec![t_param]);

    let mut evaluator = TypeEvaluator::with_resolver(&interner, &env);
    let result = evaluator.evaluate(box_union);

    // Expected: { value: string | number }
    let expected = interner.object(vec![PropertyInfo::new(value_name, string_or_number)]);

    assert_eq!(
        result, expected,
        "Box<string | number> should expand to {{ value: string | number }}"
    );
}

/// Test Application where the base is not a Ref (should pass through).
///
/// If the base is already a concrete type (not a Ref), expansion
/// should either pass through or handle appropriately.
#[test]
fn test_application_non_ref_base_passthrough() {
    use crate::evaluation::evaluate::TypeEvaluator;
    use crate::relations::subtype::TypeEnvironment;

    let interner = TypeInterner::new();

    // Create Application with a concrete type as base (not a Ref)
    // This is an unusual case - normally Application has Ref as base
    let object_base = interner.object(vec![]);
    let weird_application = interner.application(object_base, vec![TypeId::STRING]);

    // Set up empty resolver
    let env = TypeEnvironment::new();

    let mut evaluator = TypeEvaluator::with_resolver(&interner, &env);
    let result = evaluator.evaluate(weird_application);

    // Non-Ref base should pass through unchanged
    // (or potentially be an error case)
    assert_eq!(
        result, weird_application,
        "Application with non-Ref base should pass through unchanged"
    );
}

/// Test Application with recursive type alias.
///
/// This tests the pattern: type List<T> = { value: T, next: List<T> | null }
/// Recursive types need special handling to avoid infinite expansion.
#[test]
fn test_application_ref_expansion_recursive() {
    use crate::evaluation::evaluate::TypeEvaluator;
    use crate::relations::subtype::TypeEnvironment;

    let interner = TypeInterner::new();

    // Define type parameter T
    let t_name = interner.intern_string("T");
    let t_param = TypeParamInfo {
        name: t_name,
        constraint: None,
        default: None,
        is_const: false,
    };
    let t_type = interner.intern(TypeData::TypeParameter(t_param));

    // Create Lazy(DefId) for List type alias (self-reference)
    let list_def_id = DefId(1);
    let list_ref = interner.intern(TypeData::Lazy(list_def_id));

    // Create Application: List<T> (recursive reference in type body)
    let list_t = interner.application(list_ref, vec![t_type]);

    // next: List<T> | null
    let next_type = interner.union(vec![list_t, TypeId::NULL]);

    // Define: type List<T> = { value: T, next: List<T> | null }
    let value_name = interner.intern_string("value");
    let next_name = interner.intern_string("next");
    let list_body = interner.object(vec![
        PropertyInfo::new(value_name, t_type),
        PropertyInfo::new(next_name, next_type),
    ]);

    // Create Application: List<string>
    let list_string = interner.application(list_ref, vec![TypeId::STRING]);

    // Set up resolver with type parameters (Phase 4.2: use DefId-based API)
    let mut env = TypeEnvironment::new();
    env.insert_def_with_params(list_def_id, list_body, vec![t_param]);

    let mut evaluator = TypeEvaluator::with_resolver(&interner, &env);
    let result = evaluator.evaluate(list_string);

    // Expected: { value: string, next: List<string> | null }
    // The inner List<string> remains as Application to prevent infinite expansion
    let list_string_inner = interner.application(list_ref, vec![TypeId::STRING]);
    let next_type_expected = interner.union(vec![list_string_inner, TypeId::NULL]);
    let expected = interner.object(vec![
        PropertyInfo::new(value_name, TypeId::STRING),
        PropertyInfo::new(next_name, next_type_expected),
    ]);

    assert_eq!(
        result, expected,
        "List<string> should expand to {{ value: string, next: List<string> | null }}"
    );
}

/// Test Application with intersection type argument.
///
/// This tests: Box<string & { length: number }>
#[test]
fn test_application_ref_expansion_with_intersection_arg() {
    use crate::evaluation::evaluate::TypeEvaluator;
    use crate::relations::subtype::TypeEnvironment;

    let interner = TypeInterner::new();

    // Define type parameter T
    let t_name = interner.intern_string("T");
    let t_param = TypeParamInfo {
        name: t_name,
        constraint: None,
        default: None,
        is_const: false,
    };
    let t_type = interner.intern(TypeData::TypeParameter(t_param));

    // Define: type Box<T> = { value: T }
    let value_name = interner.intern_string("value");
    let box_body = interner.object(vec![PropertyInfo::new(value_name, t_type)]);

    // Create Ref(1) for Box type alias
    let box_ref = interner.lazy(DefId(1));

    // Create intersection: string & { length: number }
    let length_name = interner.intern_string("length");
    let length_obj = interner.object(vec![PropertyInfo::new(length_name, TypeId::NUMBER)]);
    let string_with_length = interner.intersection(vec![TypeId::STRING, length_obj]);

    // Create Application: Box<string & { length: number }>
    let box_intersection = interner.application(box_ref, vec![string_with_length]);

    // Set up resolver with type parameters
    let mut env = TypeEnvironment::new();
    env.insert_def_with_params(DefId(1), box_body, vec![t_param]);

    let mut evaluator = TypeEvaluator::with_resolver(&interner, &env);
    let result = evaluator.evaluate(box_intersection);

    // Expected: { value: string & { length: number } }
    let expected = interner.object(vec![PropertyInfo::new(value_name, string_with_length)]);

    assert_eq!(
        result, expected,
        "Box<string & {{ length: number }}> should expand to {{ value: string & {{ length: number }} }}"
    );
}

/// Test multi-parameter Application (Map<K, V> style).
///
/// This tests: type Map<K, V> = { key: K, value: V }
#[test]
fn test_application_ref_expansion_multi_param() {
    use crate::evaluation::evaluate::TypeEvaluator;
    use crate::relations::subtype::TypeEnvironment;

    let interner = TypeInterner::new();

    // Define type parameter K
    let k_name = interner.intern_string("K");
    let k_param = TypeParamInfo {
        name: k_name,
        constraint: None,
        default: None,
        is_const: false,
    };
    let k_type = interner.intern(TypeData::TypeParameter(k_param));

    // Define type parameter V
    let v_name = interner.intern_string("V");
    let v_param = TypeParamInfo {
        name: v_name,
        constraint: None,
        default: None,
        is_const: false,
    };
    let v_type = interner.intern(TypeData::TypeParameter(v_param));

    // Define: type Map<K, V> = { key: K, value: V }
    let key_name = interner.intern_string("key");
    let value_name = interner.intern_string("value");
    let map_body = interner.object(vec![
        PropertyInfo::new(key_name, k_type),
        PropertyInfo::new(value_name, v_type),
    ]);

    // Create Ref(1) for Map type alias
    let map_ref = interner.lazy(DefId(1));

    // Create Application: Map<string, number>
    let map_string_number = interner.application(map_ref, vec![TypeId::STRING, TypeId::NUMBER]);

    // Set up resolver with type parameters (K, V in order)
    let mut env = TypeEnvironment::new();
    env.insert_def_with_params(DefId(1), map_body, vec![k_param, v_param]);

    let mut evaluator = TypeEvaluator::with_resolver(&interner, &env);
    let result = evaluator.evaluate(map_string_number);

    // Expected: { key: string, value: number }
    let expected = interner.object(vec![
        PropertyInfo::new(key_name, TypeId::STRING),
        PropertyInfo::new(value_name, TypeId::NUMBER),
    ]);

    assert_eq!(
        result, expected,
        "Map<string, number> should expand to {{ key: string, value: number }}"
    );
}

/// Test Application with conditional type body.
///
/// This tests: type Unwrap<T> = T extends Array<infer U> ? U : T
/// Note: Full conditional evaluation is tested separately; this tests
/// that Application expansion properly triggers conditional evaluation.
#[test]
fn test_application_ref_expansion_with_conditional_body() {
    use crate::evaluation::evaluate::TypeEvaluator;
    use crate::relations::subtype::TypeEnvironment;

    let interner = TypeInterner::new();

    // Define type parameter T
    let t_name = interner.intern_string("T");
    let t_param = TypeParamInfo {
        name: t_name,
        constraint: None,
        default: None,
        is_const: false,
    };
    let t_type = interner.intern(TypeData::TypeParameter(t_param));

    // For simplicity, we'll use a basic conditional that we can verify:
    // type IsString<T> = T extends string ? string : number

    // Create the conditional type body:
    // T extends string ? string : number
    let conditional_body = interner.conditional(ConditionalType {
        check_type: t_type,
        extends_type: TypeId::STRING,
        true_type: TypeId::STRING,  // true branch returns string
        false_type: TypeId::NUMBER, // false branch returns number
        is_distributive: false,
    });

    // Create Ref(1) for IsString type alias
    let is_string_ref = interner.lazy(DefId(1));

    // Create Application: IsString<string>
    let is_string_string = interner.application(is_string_ref, vec![TypeId::STRING]);

    // Create Application: IsString<number>
    let is_string_number = interner.application(is_string_ref, vec![TypeId::NUMBER]);

    // Set up resolver with type parameters
    let mut env = TypeEnvironment::new();
    env.insert_def_with_params(DefId(1), conditional_body, vec![t_param]);

    let mut evaluator = TypeEvaluator::with_resolver(&interner, &env);

    let result_string = evaluator.evaluate(is_string_string);
    let result_number = evaluator.evaluate(is_string_number);

    // IsString<string> should evaluate to string (true branch: string extends string)
    // IsString<number> should evaluate to number (false branch: number doesn't extend string)
    assert_eq!(
        result_string,
        TypeId::STRING,
        "IsString<string> should evaluate to string (true branch)"
    );
    assert_eq!(
        result_number,
        TypeId::NUMBER,
        "IsString<number> should evaluate to number (false branch)"
    );
}

/// Test Application with tuple type argument.
///
/// This tests: Box<[string, number]>
#[test]
fn test_application_ref_expansion_with_tuple_arg() {
    use crate::evaluation::evaluate::TypeEvaluator;
    use crate::relations::subtype::TypeEnvironment;

    let interner = TypeInterner::new();

    // Define type parameter T
    let t_name = interner.intern_string("T");
    let t_param = TypeParamInfo {
        name: t_name,
        constraint: None,
        default: None,
        is_const: false,
    };
    let t_type = interner.intern(TypeData::TypeParameter(t_param));

    // Define: type Box<T> = { value: T }
    let value_name = interner.intern_string("value");
    let box_body = interner.object(vec![PropertyInfo::new(value_name, t_type)]);

    // Create Ref(1) for Box type alias
    let box_ref = interner.lazy(DefId(1));

    // Create tuple: [string, number]
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
    ]);

    // Create Application: Box<[string, number]>
    let box_tuple = interner.application(box_ref, vec![tuple_type]);

    // Set up resolver with type parameters
    let mut env = TypeEnvironment::new();
    env.insert_def_with_params(DefId(1), box_body, vec![t_param]);

    let mut evaluator = TypeEvaluator::with_resolver(&interner, &env);
    let result = evaluator.evaluate(box_tuple);

    // Expected: { value: [string, number] }
    let expected = interner.object(vec![PropertyInfo::new(value_name, tuple_type)]);

    assert_eq!(
        result, expected,
        "Box<[string, number]> should expand to {{ value: [string, number] }}"
    );
}

/// Test Application expansion with array element type in body.
///
/// `type ArrayOf<T> = T[]` with `ArrayOf<string>` should expand to `string[]`
#[test]
fn test_application_ref_expansion_with_array_body() {
    use crate::evaluation::evaluate::TypeEvaluator;
    use crate::relations::subtype::TypeEnvironment;

    let interner = TypeInterner::new();

    // Define type parameter T
    let t_name = interner.intern_string("T");
    let t_param = TypeParamInfo {
        name: t_name,
        constraint: None,
        default: None,
        is_const: false,
    };
    let t_type = interner.intern(TypeData::TypeParameter(t_param));

    // Define: type ArrayOf<T> = T[]
    let array_body = interner.array(t_type);

    // Create Ref(1) for ArrayOf type alias
    let array_of_ref = interner.lazy(DefId(1));

    // Create Application: ArrayOf<string>
    let array_of_string = interner.application(array_of_ref, vec![TypeId::STRING]);

    // Set up resolver with type parameters
    let mut env = TypeEnvironment::new();
    env.insert_def_with_params(DefId(1), array_body, vec![t_param]);

    let mut evaluator = TypeEvaluator::with_resolver(&interner, &env);
    let result = evaluator.evaluate(array_of_string);

    // Expected: string[]
    let expected = interner.array(TypeId::STRING);

    assert_eq!(
        result, expected,
        "ArrayOf<string> should expand to string[]"
    );
}

/// Test Application expansion with readonly property in body.
///
/// `type ReadonlyBox<T> = { readonly value: T }` with `ReadonlyBox<number>`
/// should expand to `{ readonly value: number }`
#[test]
fn test_application_ref_expansion_with_readonly_property() {
    use crate::evaluation::evaluate::TypeEvaluator;
    use crate::relations::subtype::TypeEnvironment;

    let interner = TypeInterner::new();

    // Define type parameter T
    let t_name = interner.intern_string("T");
    let t_param = TypeParamInfo {
        name: t_name,
        constraint: None,
        default: None,
        is_const: false,
    };
    let t_type = interner.intern(TypeData::TypeParameter(t_param));

    // Define: type ReadonlyBox<T> = { readonly value: T }
    let value_name = interner.intern_string("value");
    let readonly_box_body = interner.object(vec![PropertyInfo {
        name: value_name,
        type_id: t_type,
        write_type: t_type,
        optional: false,
        readonly: true, // readonly modifier
        is_method: false,
        is_class_prototype: false,
        visibility: Visibility::Public,
        parent_id: None,
        declaration_order: 0,
        is_string_named: false,
    }]);

    // Create Ref(1) for ReadonlyBox type alias
    let readonly_box_ref = interner.lazy(DefId(1));

    // Create Application: ReadonlyBox<number>
    let readonly_box_number = interner.application(readonly_box_ref, vec![TypeId::NUMBER]);

    // Set up resolver with type parameters
    let mut env = TypeEnvironment::new();
    env.insert_def_with_params(DefId(1), readonly_box_body, vec![t_param]);

    let mut evaluator = TypeEvaluator::with_resolver(&interner, &env);
    let result = evaluator.evaluate(readonly_box_number);

    // Expected: { readonly value: number }
    let expected = interner.object(vec![PropertyInfo::readonly(value_name, TypeId::NUMBER)]);

    assert_eq!(
        result, expected,
        "ReadonlyBox<number> should expand to {{ readonly value: number }}"
    );
}

use super::*;

// =============================================================================
// ADVANCED GENERIC INFERENCE PATTERNS
// =============================================================================

#[test]
fn test_higher_order_function_inference() {
    // Test: compose<A, B, C>(f: (b: B) => C, g: (a: A) => B): (a: A) => C
    // Inference flows through multiple functions
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);

    let a_name = interner.intern_string("A");
    let b_name = interner.intern_string("B");
    let c_name = interner.intern_string("C");

    let var_a = ctx.fresh_type_param(a_name, false);
    let var_b = ctx.fresh_type_param(b_name, false);
    let var_c = ctx.fresh_type_param(c_name, false);

    // g: (a: string) => number, so A = string, B = number
    ctx.add_lower_bound(var_a, TypeId::STRING);
    ctx.add_lower_bound(var_b, TypeId::NUMBER);

    // f: (b: number) => boolean, so B = number (consistent), C = boolean
    ctx.add_lower_bound(var_c, TypeId::BOOLEAN);

    let result_a = ctx.resolve_with_constraints(var_a).unwrap();
    let result_b = ctx.resolve_with_constraints(var_b).unwrap();
    let result_c = ctx.resolve_with_constraints(var_c).unwrap();

    assert_eq!(result_a, TypeId::STRING);
    assert_eq!(result_b, TypeId::NUMBER);
    assert_eq!(result_c, TypeId::BOOLEAN);
}

#[test]
fn test_method_chaining_inference() {
    // Test: array.filter(...).map(...) - type flows through chain
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);

    let t_name = interner.intern_string("T");
    let u_name = interner.intern_string("U");

    let var_t = ctx.fresh_type_param(t_name, false);
    let var_u = ctx.fresh_type_param(u_name, false);

    // Initial array element type
    ctx.add_lower_bound(var_t, TypeId::NUMBER);

    // After map, result type
    ctx.add_lower_bound(var_u, TypeId::STRING);

    let result_t = ctx.resolve_with_constraints(var_t).unwrap();
    let result_u = ctx.resolve_with_constraints(var_u).unwrap();

    assert_eq!(result_t, TypeId::NUMBER);
    assert_eq!(result_u, TypeId::STRING);
}

#[test]
fn test_partial_type_inference() {
    // Test: Partial<T> inference - each property becomes optional
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name, false);

    // Source object type
    let name_prop = interner.intern_string("name");
    let age_prop = interner.intern_string("age");

    let obj = interner.object(vec![
        PropertyInfo::new(name_prop, TypeId::STRING),
        PropertyInfo::new(age_prop, TypeId::NUMBER),
    ]);

    ctx.add_lower_bound(var_t, obj);

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    assert_eq!(result, obj);
}

#[test]
fn test_record_utility_inference() {
    // Test: Record<K, V> inference from object literal
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);

    let k_name = interner.intern_string("K");
    let v_name = interner.intern_string("V");

    let var_k = ctx.fresh_type_param(k_name, false);
    let var_v = ctx.fresh_type_param(v_name, false);

    // Keys from object: "a" | "b"
    let key_a = interner.literal_string("a");
    let key_b = interner.literal_string("b");
    ctx.add_lower_bound(var_k, interner.union(vec![key_a, key_b]));

    // Value type: number
    ctx.add_lower_bound(var_v, TypeId::NUMBER);

    let result_k = ctx.resolve_with_constraints(var_k).unwrap();
    let result_v = ctx.resolve_with_constraints(var_v).unwrap();

    let expected_k = interner.union(vec![key_a, key_b]);
    assert_eq!(result_k, expected_k);
    assert_eq!(result_v, TypeId::NUMBER);
}

#[test]
fn test_tuple_to_union_inference() {
    // Test: T[number] where T is a tuple - produces union of element types
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name, false);

    // Tuple [string, number, boolean]
    let tuple = interner.tuple(vec![
        TupleElement {
            type_id: TypeId::STRING,
            optional: false,
            name: None,
            rest: false,
        },
        TupleElement {
            type_id: TypeId::NUMBER,
            optional: false,
            name: None,
            rest: false,
        },
        TupleElement {
            type_id: TypeId::BOOLEAN,
            optional: false,
            name: None,
            rest: false,
        },
    ]);

    ctx.add_lower_bound(var_t, tuple);

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    assert_eq!(result, tuple);
}

#[test]
fn test_spread_tuple_inference() {
    // Test: [...T, ...U] inference from combined tuple
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);

    let t_name = interner.intern_string("T");
    let u_name = interner.intern_string("U");

    let var_t = ctx.fresh_type_param(t_name, false);
    let var_u = ctx.fresh_type_param(u_name, false);

    // First part of tuple: [string]
    let tuple_t = interner.tuple(vec![TupleElement {
        type_id: TypeId::STRING,
        optional: false,
        name: None,
        rest: false,
    }]);

    // Second part: [number, boolean]
    let tuple_u = interner.tuple(vec![
        TupleElement {
            type_id: TypeId::NUMBER,
            optional: false,
            name: None,
            rest: false,
        },
        TupleElement {
            type_id: TypeId::BOOLEAN,
            optional: false,
            name: None,
            rest: false,
        },
    ]);

    ctx.add_lower_bound(var_t, tuple_t);
    ctx.add_lower_bound(var_u, tuple_u);

    let result_t = ctx.resolve_with_constraints(var_t).unwrap();
    let result_u = ctx.resolve_with_constraints(var_u).unwrap();

    assert_eq!(result_t, tuple_t);
    assert_eq!(result_u, tuple_u);
}

#[test]
fn test_awaited_inference() {
    // Test: Awaited<Promise<T>> inference
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name, false);

    // Promise resolves to string
    ctx.add_lower_bound(var_t, TypeId::STRING);

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    assert_eq!(result, TypeId::STRING);
}

#[test]
fn test_return_type_inference_async() {
    // Test: async function returns Promise<T>, infer T
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name, false);

    // Return statements provide lower bound
    ctx.add_lower_bound(var_t, TypeId::NUMBER);

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    assert_eq!(result, TypeId::NUMBER);
}

#[test]
fn test_mapped_type_key_inference() {
    // Test: { [K in keyof T]: T[K] } - K inferred from source keys
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let k_name = interner.intern_string("K");

    let var_k = ctx.fresh_type_param(k_name, false);

    // Keys from iteration
    let key_x = interner.literal_string("x");
    let key_y = interner.literal_string("y");
    ctx.add_lower_bound(var_k, key_x);
    ctx.add_lower_bound(var_k, key_y);

    let result = ctx.resolve_with_constraints(var_k).unwrap();
    // Multiple string literal keys widen to string
    assert_eq!(result, TypeId::STRING);
}

#[test]
fn test_mapped_type_template_union_inference() {
    use crate::types::{MappedType, TypeParamInfo};

    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");
    let p_name = interner.intern_string("P");

    let var_t = ctx.fresh_type_param(t_name, false);

    let _p_type = interner.type_param(TypeParamInfo {
        name: p_name,
        constraint: None,
        default: None,
        is_const: false,
    });
    let t_type = interner.type_param(TypeParamInfo {
        name: t_name,
        constraint: None,
        default: None,
        is_const: false,
    });

    let source = interner.object(vec![
        PropertyInfo::new(interner.intern_string("sum"), TypeId::NUMBER),
        PropertyInfo::new(interner.intern_string("nested"), TypeId::NUMBER),
    ]);

    let mapped = MappedType {
        type_param: TypeParamInfo {
            name: p_name,
            constraint: None,
            default: None,
            is_const: false,
        },
        constraint: interner.keyof(t_type),
        name_type: None,
        template: interner.union(vec![t_type, TypeId::STRING]),
        readonly_modifier: None,
        optional_modifier: None,
    };
    let target = interner.mapped(mapped);

    // Before this fix, `mapped_template` containing a union prevents
    // the per-property values from inferring the outer generic `T`.
    ctx.infer_from_types(source, target, InferencePriority::NakedTypeVariable)
        .unwrap();

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    assert_eq!(result, TypeId::NUMBER);
}

#[test]
fn test_pick_utility_inference() {
    // Test: Pick<T, K> inference
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);

    let t_name = interner.intern_string("T");
    let k_name = interner.intern_string("K");

    let var_t = ctx.fresh_type_param(t_name, false);
    let var_k = ctx.fresh_type_param(k_name, false);

    let name_prop = interner.intern_string("name");
    let age_prop = interner.intern_string("age");
    let email_prop = interner.intern_string("email");

    // Source object with 3 properties
    let source = interner.object(vec![
        PropertyInfo::new(name_prop, TypeId::STRING),
        PropertyInfo::new(age_prop, TypeId::NUMBER),
        PropertyInfo::new(email_prop, TypeId::STRING),
    ]);

    ctx.add_lower_bound(var_t, source);

    // Pick only "name" | "email"
    let picked_keys = interner.union(vec![
        interner.literal_string("name"),
        interner.literal_string("email"),
    ]);
    ctx.add_lower_bound(var_k, picked_keys);

    let result_t = ctx.resolve_with_constraints(var_t).unwrap();
    let result_k = ctx.resolve_with_constraints(var_k).unwrap();

    assert_eq!(result_t, source);
    assert_eq!(result_k, picked_keys);
}

#[test]
fn test_omit_utility_inference() {
    // Test: Omit<T, K> - K represents keys to exclude
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let k_name = interner.intern_string("K");

    let var_k = ctx.fresh_type_param(k_name, false);

    // Keys to omit: "password"
    let password_key = interner.literal_string("password");
    ctx.add_lower_bound(var_k, password_key);

    let result = ctx.resolve_with_constraints(var_k).unwrap();
    assert_eq!(result, TypeId::STRING);
}

#[test]
fn test_extract_utility_inference() {
    // Test: Extract<T, U> - filter union to subtypes of U
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);

    let t_name = interner.intern_string("T");
    let u_name = interner.intern_string("U");

    let var_t = ctx.fresh_type_param(t_name, false);
    let var_u = ctx.fresh_type_param(u_name, false);

    // Union to filter: string | number | boolean
    let union_t = interner.union(vec![TypeId::STRING, TypeId::NUMBER, TypeId::BOOLEAN]);
    ctx.add_lower_bound(var_t, union_t);

    // Filter to: string | number
    let filter_u = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);
    ctx.add_lower_bound(var_u, filter_u);

    let result_t = ctx.resolve_with_constraints(var_t).unwrap();
    let result_u = ctx.resolve_with_constraints(var_u).unwrap();

    assert_eq!(result_t, union_t);
    assert_eq!(result_u, filter_u);
}

#[test]
fn test_parameters_utility_inference() {
    // Test: Parameters<T> - extract parameter types as tuple
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name, false);

    // Function type (a: string, b: number) => void
    let func = interner.function(FunctionShape {
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
        ],
        this_type: None,
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    ctx.add_lower_bound(var_t, func);

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    assert_eq!(result, func);
}

#[test]
fn test_constructor_parameters_inference() {
    // Test: ConstructorParameters<T> - extract constructor param types
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name, false);

    // Constructor type new (name: string) => Instance
    let ctor = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("name")),
            type_id: TypeId::STRING,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: TypeId::OBJECT,
        type_predicate: None,
        is_constructor: true,
        is_method: false,
    });

    ctx.add_lower_bound(var_t, ctor);

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    assert_eq!(result, ctor);
}

#[test]
fn test_instance_type_inference() {
    // Test: InstanceType<T> - extract instance type from constructor
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name, false);

    // Instance has specific shape
    let name_prop = interner.intern_string("name");
    let instance = interner.object(vec![PropertyInfo::new(name_prop, TypeId::STRING)]);

    ctx.add_lower_bound(var_t, instance);

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    assert_eq!(result, instance);
}

// ----------------------------------------------------------------------------
// Additional circular constraint edge cases
// ----------------------------------------------------------------------------

#[test]
fn test_circular_constraint_polymorphic_this() {
    // Test: class Chain { next(): this }
    // Polymorphic this type pattern
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let this_name = interner.intern_string("This");

    let var_this = ctx.fresh_type_param(this_name, false);

    let next_fn = interner.function(FunctionShape {
        type_params: Vec::new(),
        params: Vec::new(),
        this_type: None,
        return_type: TypeId::OBJECT, // Returns this
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let chain_type = interner.object(vec![
        PropertyInfo::method(interner.intern_string("next"), next_fn),
        PropertyInfo::new(interner.intern_string("value"), TypeId::UNKNOWN),
    ]);

    ctx.add_lower_bound(var_this, chain_type);
    ctx.add_upper_bound(var_this, TypeId::OBJECT);

    let result = ctx.resolve_with_constraints(var_this).unwrap();
    assert_eq!(result, chain_type);
}

#[test]
fn test_circular_constraint_recursive_promise() {
    // Test: type PromiseChain<T> = Promise<T | PromiseChain<T>>
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name, false);

    let then_fn = interner.function(FunctionShape {
        type_params: Vec::new(),
        params: vec![ParamInfo {
            name: Some(interner.intern_string("callback")),
            type_id: TypeId::OBJECT,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: TypeId::OBJECT,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let promise_type = interner.object(vec![PropertyInfo::method(
        interner.intern_string("then"),
        then_fn,
    )]);

    ctx.add_lower_bound(var_t, promise_type);
    ctx.add_upper_bound(var_t, TypeId::OBJECT);

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    assert_eq!(result, promise_type);
}

#[test]
fn test_circular_constraint_event_emitter() {
    // Test: interface EventEmitter<T extends EventEmitter<T>>
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name, false);

    let on_fn = interner.function(FunctionShape {
        type_params: Vec::new(),
        params: vec![
            ParamInfo {
                name: Some(interner.intern_string("event")),
                type_id: TypeId::STRING,
                optional: false,
                rest: false,
            },
            ParamInfo {
                name: Some(interner.intern_string("handler")),
                type_id: TypeId::OBJECT,
                optional: false,
                rest: false,
            },
        ],
        this_type: None,
        return_type: TypeId::OBJECT, // Returns this for chaining
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let emit_fn = interner.function(FunctionShape {
        type_params: Vec::new(),
        params: vec![ParamInfo {
            name: Some(interner.intern_string("event")),
            type_id: TypeId::STRING,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: TypeId::BOOLEAN,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let emitter_type = interner.object(vec![
        PropertyInfo::method(interner.intern_string("on"), on_fn),
        PropertyInfo::method(interner.intern_string("emit"), emit_fn),
    ]);

    ctx.add_lower_bound(var_t, emitter_type);
    ctx.add_upper_bound(var_t, TypeId::OBJECT);

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    assert_eq!(result, emitter_type);
}

#[test]
fn test_circular_constraint_fluent_interface() {
    // Test: interface FluentBuilder<T extends FluentBuilder<T>> with method chaining
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name, false);

    let with_fn = interner.function(FunctionShape {
        type_params: Vec::new(),
        params: vec![
            ParamInfo {
                name: Some(interner.intern_string("key")),
                type_id: TypeId::STRING,
                optional: false,
                rest: false,
            },
            ParamInfo {
                name: Some(interner.intern_string("value")),
                type_id: TypeId::UNKNOWN,
                optional: false,
                rest: false,
            },
        ],
        this_type: None,
        return_type: TypeId::OBJECT, // Returns this
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let fluent_type = interner.object(vec![
        PropertyInfo::method(interner.intern_string("withName"), with_fn),
        PropertyInfo::method(interner.intern_string("withValue"), with_fn),
        PropertyInfo::method(interner.intern_string("withConfig"), with_fn),
    ]);

    ctx.add_lower_bound(var_t, fluent_type);
    ctx.add_upper_bound(var_t, TypeId::OBJECT);

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    assert_eq!(result, fluent_type);
}

#[test]
fn test_circular_constraint_recursive_json() {
    // Test: type JSON = string | number | boolean | null | JSON[] | { [key: string]: JSON }
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let json_name = interner.intern_string("JSON");

    let var_json = ctx.fresh_type_param(json_name, false);

    // JSON is a union of primitives and recursive structures
    let json_union = interner.union(vec![
        TypeId::STRING,
        TypeId::NUMBER,
        TypeId::BOOLEAN,
        TypeId::NULL,
    ]);

    ctx.add_lower_bound(var_json, json_union);
    ctx.add_upper_bound(var_json, TypeId::UNKNOWN);

    let result = ctx.resolve_with_constraints(var_json).unwrap();
    assert_eq!(result, json_union);
}

#[test]
fn test_circular_constraint_linked_list_generic() {
    // Test: interface LinkedList<T, Self extends LinkedList<T, Self>>
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");
    let self_name = interner.intern_string("Self");

    let var_t = ctx.fresh_type_param(t_name, false);
    let var_self = ctx.fresh_type_param(self_name, false);

    let node_type = interner.object(vec![
        PropertyInfo {
            name: interner.intern_string("value"),
            type_id: TypeId::UNKNOWN, // Would be T
            write_type: TypeId::UNKNOWN,
            optional: false,
            readonly: false,
            is_method: false,
            is_class_prototype: false,
            visibility: Visibility::Public,
            parent_id: None,
            declaration_order: 0,
            is_string_named: false,
            is_symbol_named: false,
            single_quoted_name: false,
        },
        PropertyInfo {
            name: interner.intern_string("next"),
            type_id: TypeId::OBJECT, // Would be Self | null
            write_type: TypeId::OBJECT,
            optional: true,
            readonly: false,
            is_method: false,
            is_class_prototype: false,
            visibility: Visibility::Public,
            parent_id: None,
            declaration_order: 0,
            is_string_named: false,
            is_symbol_named: false,
            single_quoted_name: false,
        },
    ]);

    ctx.add_lower_bound(var_t, TypeId::STRING);
    ctx.add_lower_bound(var_self, node_type);
    ctx.add_upper_bound(var_self, TypeId::OBJECT);

    let results = ctx.resolve_all_with_constraints().unwrap();
    assert_eq!(results.len(), 2);
}

#[test]
fn test_circular_constraint_state_machine() {
    // Test: interface State<S extends State<S, E>, E extends Event>
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let s_name = interner.intern_string("S");
    let e_name = interner.intern_string("E");

    let var_s = ctx.fresh_type_param(s_name, false);
    let var_e = ctx.fresh_type_param(e_name, false);

    let transition_fn = interner.function(FunctionShape {
        type_params: Vec::new(),
        params: vec![ParamInfo {
            name: Some(interner.intern_string("event")),
            type_id: TypeId::OBJECT,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: TypeId::OBJECT, // Returns S
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let state_type = interner.object(vec![
        PropertyInfo::readonly(interner.intern_string("name"), TypeId::STRING),
        PropertyInfo::method(interner.intern_string("transition"), transition_fn),
    ]);

    let event_type = interner.object(vec![PropertyInfo::readonly(
        interner.intern_string("type"),
        TypeId::STRING,
    )]);

    ctx.add_lower_bound(var_s, state_type);
    ctx.add_lower_bound(var_e, event_type);
    ctx.add_upper_bound(var_s, TypeId::OBJECT);
    ctx.add_upper_bound(var_e, TypeId::OBJECT);

    let results = ctx.resolve_all_with_constraints().unwrap();
    assert_eq!(results.len(), 2);
}

#[test]
fn test_circular_constraint_visitor_pattern() {
    // Test: interface Visitor<T extends Visitable<T>>
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name, false);

    let accept_fn = interner.function(FunctionShape {
        type_params: Vec::new(),
        params: vec![ParamInfo {
            name: Some(interner.intern_string("visitor")),
            type_id: TypeId::OBJECT,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let visitable_type = interner.object(vec![PropertyInfo::method(
        interner.intern_string("accept"),
        accept_fn,
    )]);

    ctx.add_lower_bound(var_t, visitable_type);
    ctx.add_upper_bound(var_t, TypeId::OBJECT);

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    assert_eq!(result, visitable_type);
}

#[test]
fn test_circular_constraint_expression_tree() {
    // Test: interface Expr<T extends Expr<T>> { eval(): number; combine(other: T): T }
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name, false);

    let eval_fn = interner.function(FunctionShape {
        type_params: Vec::new(),
        params: Vec::new(),
        this_type: None,
        return_type: TypeId::NUMBER,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let combine_fn = interner.function(FunctionShape {
        type_params: Vec::new(),
        params: vec![ParamInfo {
            name: Some(interner.intern_string("other")),
            type_id: TypeId::OBJECT,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: TypeId::OBJECT, // Returns T
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let expr_type = interner.object(vec![
        PropertyInfo::method(interner.intern_string("eval"), eval_fn),
        PropertyInfo::method(interner.intern_string("combine"), combine_fn),
    ]);

    ctx.add_lower_bound(var_t, expr_type);
    ctx.add_upper_bound(var_t, TypeId::OBJECT);

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    assert_eq!(result, expr_type);
}

#[test]
fn test_circular_constraint_repository_pattern() {
    // Test: interface Repository<T, R extends Repository<T, R>>
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");
    let r_name = interner.intern_string("R");

    let var_t = ctx.fresh_type_param(t_name, false);
    let var_r = ctx.fresh_type_param(r_name, false);

    let find_fn = interner.function(FunctionShape {
        type_params: Vec::new(),
        params: vec![ParamInfo {
            name: Some(interner.intern_string("id")),
            type_id: TypeId::STRING,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: TypeId::OBJECT, // Returns T | undefined
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let save_fn = interner.function(FunctionShape {
        type_params: Vec::new(),
        params: vec![ParamInfo {
            name: Some(interner.intern_string("entity")),
            type_id: TypeId::OBJECT,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: TypeId::OBJECT, // Returns R for chaining
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let entity_type = interner.object(vec![PropertyInfo::new(
        interner.intern_string("id"),
        TypeId::STRING,
    )]);

    let repo_type = interner.object(vec![
        PropertyInfo::method(interner.intern_string("find"), find_fn),
        PropertyInfo::method(interner.intern_string("save"), save_fn),
    ]);

    ctx.add_lower_bound(var_t, entity_type);
    ctx.add_lower_bound(var_r, repo_type);
    ctx.add_upper_bound(var_t, TypeId::OBJECT);
    ctx.add_upper_bound(var_r, TypeId::OBJECT);

    let results = ctx.resolve_all_with_constraints().unwrap();
    assert_eq!(results.len(), 2);
}

// ----------------------------------------------------------------------------
// Advanced inference from usage patterns
// ----------------------------------------------------------------------------

#[test]
fn test_inference_from_method_chain() {
    // Test: array.map(x => x.name).filter(n => n.length > 0)
    // Infer T from chained method calls
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name, false);

    // T is inferred from the input array element type
    let obj_with_name = interner.object(vec![PropertyInfo::new(
        interner.intern_string("name"),
        TypeId::STRING,
    )]);

    ctx.add_lower_bound(var_t, obj_with_name);
    ctx.add_upper_bound(var_t, TypeId::OBJECT);

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    assert_eq!(result, obj_with_name);
}

#[test]
fn test_inference_from_spread_in_array() {
    // Test: [...arr1, ...arr2] infers common element type
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name, false);

    // Both arrays contribute to T
    ctx.add_lower_bound(var_t, TypeId::STRING);
    ctx.add_lower_bound(var_t, TypeId::NUMBER);

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    // tsc unions lower bounds: string | number
    let expected = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);
    assert_eq!(result, expected);
}

#[test]
fn test_inference_from_spread_in_object() {
    // Test: { ...obj1, ...obj2 } infers merged object type
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name, false);

    let obj1 = interner.object(vec![PropertyInfo::new(
        interner.intern_string("a"),
        TypeId::STRING,
    )]);

    ctx.add_lower_bound(var_t, obj1);
    ctx.add_upper_bound(var_t, TypeId::OBJECT);

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    assert_eq!(result, obj1);
}

#[test]
fn test_inference_from_optional_chain() {
    // Test: obj?.prop infers T | undefined
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name, false);

    // Optional chaining produces T | undefined
    let value_or_undef = interner.union(vec![TypeId::STRING, TypeId::UNDEFINED]);
    ctx.add_lower_bound(var_t, value_or_undef);

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    assert_eq!(result, value_or_undef);
}

#[test]
fn test_inference_from_nullish_coalescing() {
    // Test: value ?? defaultValue infers common type
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name, false);

    // ?? operator: left side is T | null | undefined, right is T
    ctx.add_lower_bound(var_t, TypeId::STRING);
    ctx.add_upper_bound(var_t, TypeId::STRING);

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    assert_eq!(result, TypeId::STRING);
}

#[test]
fn test_inference_from_default_param() {
    // Test: function(x = defaultValue) infers T from default
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name, false);

    // Default parameter provides lower bound
    ctx.add_lower_bound(var_t, TypeId::NUMBER);

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    assert_eq!(result, TypeId::NUMBER);
}

#[test]
fn test_inference_from_array_destructure() {
    // Test: const [first, ...rest] = arr infers element type
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name, false);

    // Array destructuring: first is T, rest is T[]
    ctx.add_lower_bound(var_t, TypeId::STRING);

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    assert_eq!(result, TypeId::STRING);
}

#[test]
fn test_inference_from_object_destructure() {
    // Test: const { a, b } = obj infers property types
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name, false);

    let obj_type = interner.object(vec![
        PropertyInfo::new(interner.intern_string("a"), TypeId::STRING),
        PropertyInfo::new(interner.intern_string("b"), TypeId::NUMBER),
    ]);

    ctx.add_lower_bound(var_t, obj_type);
    ctx.add_upper_bound(var_t, TypeId::OBJECT);

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    assert_eq!(result, obj_type);
}

#[test]
fn test_inference_from_computed_property() {
    // Test: obj[key] where key: K extends string - fresh literal widened
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let k_name = interner.intern_string("K");

    let var_k = ctx.fresh_type_param(k_name, false);

    ctx.add_lower_bound(var_k, interner.literal_string("x"));
    ctx.add_upper_bound(var_k, TypeId::STRING);

    let result = ctx.resolve_with_constraints(var_k).unwrap();
    assert_eq!(result, TypeId::STRING);
}

#[test]
fn test_inference_bidirectional_callback() {
    // Test: arr.map(x => ({ value: x })) bidirectional inference
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");
    let u_name = interner.intern_string("U");

    let var_t = ctx.fresh_type_param(t_name, false);
    let var_u = ctx.fresh_type_param(u_name, false);

    // T is inferred from array element, U from callback return
    ctx.add_lower_bound(var_t, TypeId::NUMBER);

    let wrapper = interner.object(vec![PropertyInfo::new(
        interner.intern_string("value"),
        TypeId::NUMBER,
    )]);
    ctx.add_lower_bound(var_u, wrapper);

    let results = ctx.resolve_all_with_constraints().unwrap();
    assert_eq!(results.len(), 2);
    assert_eq!(results[0].1, TypeId::NUMBER);
    assert_eq!(results[1].1, wrapper);
}

#[test]
fn test_inference_from_async_await() {
    // Test: async function returns Promise<T>, await unwraps to T
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name, false);

    // Awaited type should be the unwrapped value
    ctx.add_lower_bound(var_t, TypeId::STRING);

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    assert_eq!(result, TypeId::STRING);
}

#[test]
fn test_inference_from_generator_yield() {
    // Test: function* gen(): Generator<T, R, N> { yield value; }
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name, false);

    // Yield type contributes to T
    ctx.add_lower_bound(var_t, TypeId::NUMBER);

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    assert_eq!(result, TypeId::NUMBER);
}

#[test]
fn test_inference_from_for_of_loop() {
    // Test: for (const x of iterable) { } infers element type
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name, false);

    // Loop variable type is inferred from iterable
    let string_array = interner.array(TypeId::STRING);
    ctx.add_lower_bound(var_t, string_array);

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    assert_eq!(result, string_array);
}

#[test]
fn test_inference_from_ternary_branches() {
    // Test: cond ? valueA : valueB infers common type
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name, false);

    // Both branches contribute to T
    ctx.add_lower_bound(var_t, interner.literal_string("a"));
    ctx.add_lower_bound(var_t, interner.literal_string("b"));

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    // Ternary branches with string literals simplify to string
    assert_eq!(result, TypeId::STRING);
}

#[test]
fn test_inference_from_type_assertion() {
    // Test: value as T uses T as the inferred type
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name, false);

    // Type assertion provides the type
    ctx.add_lower_bound(var_t, TypeId::STRING);
    ctx.add_upper_bound(var_t, TypeId::STRING);

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    assert_eq!(result, TypeId::STRING);
}

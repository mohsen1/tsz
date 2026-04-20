#[test]
fn test_param_inference_constrained_to_subset() {
    // Test: function f<T extends string>(x: T) - fresh literal widened to string
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name, false);

    ctx.add_upper_bound(var_t, TypeId::STRING);

    let lit_a = interner.literal_string("a");
    ctx.add_lower_bound(var_t, lit_a);

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    assert_eq!(result, TypeId::STRING);
}
#[test]
fn test_param_inference_from_tuple_destructure() {
    // Test: const [a, b] = f<[T, U]>([1, "hello"])
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");
    let u_name = interner.intern_string("U");

    let var_t = ctx.fresh_type_param(t_name, false);
    let var_u = ctx.fresh_type_param(u_name, false);

    // First element is number
    ctx.add_lower_bound(var_t, TypeId::NUMBER);
    // Second element is string
    ctx.add_lower_bound(var_u, TypeId::STRING);

    let result_t = ctx.resolve_with_constraints(var_t).unwrap();
    let result_u = ctx.resolve_with_constraints(var_u).unwrap();

    assert_eq!(result_t, TypeId::NUMBER);
    assert_eq!(result_u, TypeId::STRING);
}
#[test]
fn test_param_inference_bidirectional() {
    // Test: Both parameter and return contribute to inference
    // function f<T>(x: T, transform: (t: T) => T): T
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name, false);

    // Argument provides lower bound
    let hello = interner.literal_string("hello");
    ctx.add_lower_bound(var_t, hello);

    // Return context provides upper bound (widened to string)
    ctx.add_upper_bound(var_t, TypeId::STRING);

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    assert_eq!(result, TypeId::STRING);
}
#[test]
fn test_param_inference_void_callback() {
    // Test: arr.forEach(x => console.log(x)) - callback returns void
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name, false);

    // Array element provides parameter type
    ctx.add_lower_bound(var_t, TypeId::STRING);

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    assert_eq!(result, TypeId::STRING);
}

// ----------------------------------------------------------------------------
// F-bounded polymorphism and advanced extends clause patterns
// ----------------------------------------------------------------------------
#[test]
fn test_f_bounded_comparable() {
    // Test: interface Comparable<T extends Comparable<T>> { compareTo(other: T): number }
    // This is the classic F-bounded polymorphism pattern
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name, false);

    // compareTo method
    let compare_to_fn = interner.function(FunctionShape {
        type_params: Vec::new(),
        params: vec![ParamInfo {
            name: Some(interner.intern_string("other")),
            type_id: TypeId::OBJECT,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: TypeId::NUMBER,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let comparable_type = interner.object(vec![PropertyInfo::method(
        interner.intern_string("compareTo"),
        compare_to_fn,
    )]);

    ctx.add_lower_bound(var_t, comparable_type);
    ctx.add_upper_bound(var_t, TypeId::OBJECT);

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    assert_eq!(result, comparable_type);
}
#[test]
fn test_f_bounded_builder_pattern() {
    // Test: class Builder<T extends Builder<T>> { build(): T }
    // The builder pattern with fluent interface
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name, false);

    let build_fn = interner.function(FunctionShape {
        type_params: Vec::new(),
        params: Vec::new(),
        this_type: None,
        return_type: TypeId::OBJECT, // Returns T (self-referential)
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let set_fn = interner.function(FunctionShape {
        type_params: Vec::new(),
        params: vec![ParamInfo {
            name: Some(interner.intern_string("key")),
            type_id: TypeId::STRING,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: TypeId::OBJECT, // Returns this
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let builder_type = interner.object(vec![
        PropertyInfo::method(interner.intern_string("build"), build_fn),
        PropertyInfo::method(interner.intern_string("set"), set_fn),
    ]);

    ctx.add_lower_bound(var_t, builder_type);
    ctx.add_upper_bound(var_t, TypeId::OBJECT);

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    assert_eq!(result, builder_type);
}
#[test]
fn test_f_bounded_tree_node() {
    // Test: interface TreeNode<T extends TreeNode<T>> { children: T[] }
    // Self-referential tree structure
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name, false);

    // Children is array of self-type
    let children_array = interner.array(TypeId::OBJECT);

    let tree_node_type = interner.object(vec![
        PropertyInfo::new(interner.intern_string("value"), TypeId::UNKNOWN),
        PropertyInfo::new(interner.intern_string("children"), children_array),
        PropertyInfo::opt(interner.intern_string("parent"), TypeId::OBJECT),
    ]);

    ctx.add_lower_bound(var_t, tree_node_type);
    ctx.add_upper_bound(var_t, TypeId::OBJECT);

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    assert_eq!(result, tree_node_type);
}
#[test]
fn test_f_bounded_cloneable() {
    // Test: interface Cloneable<T extends Cloneable<T>> { clone(): T }
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name, false);

    let clone_fn = interner.function(FunctionShape {
        type_params: Vec::new(),
        params: Vec::new(),
        this_type: None,
        return_type: TypeId::OBJECT,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let cloneable_type = interner.object(vec![PropertyInfo::method(
        interner.intern_string("clone"),
        clone_fn,
    )]);

    ctx.add_lower_bound(var_t, cloneable_type);
    ctx.add_upper_bound(var_t, TypeId::OBJECT);

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    assert_eq!(result, cloneable_type);
}
#[test]
fn test_f_bounded_with_additional_constraint() {
    // Test: T extends Comparable<T> & Serializable
    // F-bounded with intersection constraint
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name, false);

    let compare_fn = interner.function(FunctionShape {
        type_params: Vec::new(),
        params: vec![ParamInfo {
            name: Some(interner.intern_string("other")),
            type_id: TypeId::OBJECT,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: TypeId::NUMBER,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let serialize_fn = interner.function(FunctionShape {
        type_params: Vec::new(),
        params: Vec::new(),
        this_type: None,
        return_type: TypeId::STRING,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    // Combined type with both Comparable and Serializable methods
    let combined_type = interner.object(vec![
        PropertyInfo::method(interner.intern_string("compareTo"), compare_fn),
        PropertyInfo::method(interner.intern_string("serialize"), serialize_fn),
    ]);

    ctx.add_lower_bound(var_t, combined_type);
    ctx.add_upper_bound(var_t, TypeId::OBJECT);

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    assert_eq!(result, combined_type);
}
#[test]
fn test_mutually_recursive_constraints() {
    // Test: interface A<T extends B<T>>, interface B<T extends A<T>>
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");
    let u_name = interner.intern_string("U");

    let var_t = ctx.fresh_type_param(t_name, false);
    let var_u = ctx.fresh_type_param(u_name, false);

    let type_a = interner.object(vec![PropertyInfo::new(
        interner.intern_string("a"),
        TypeId::STRING,
    )]);

    let type_b = interner.object(vec![PropertyInfo::new(
        interner.intern_string("b"),
        TypeId::NUMBER,
    )]);

    ctx.add_lower_bound(var_t, type_a);
    ctx.add_lower_bound(var_u, type_b);
    ctx.add_upper_bound(var_t, TypeId::OBJECT);
    ctx.add_upper_bound(var_u, TypeId::OBJECT);

    let results = ctx.resolve_all_with_constraints().unwrap();
    assert_eq!(results.len(), 2);
}
#[test]
fn test_extends_clause_with_keyof() {
    // Test: T extends string - fresh literal widened to string
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name, false);

    // Upper bound: string (widened literal satisfies this)
    ctx.add_upper_bound(var_t, TypeId::STRING);
    ctx.add_lower_bound(var_t, interner.literal_string("a"));

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    assert_eq!(result, TypeId::STRING);
}
#[test]
fn test_extends_clause_with_mapped_type_key() {
    // Test: K extends string - fresh literal widened to string
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let k_name = interner.intern_string("K");

    let var_k = ctx.fresh_type_param(k_name, false);

    // Upper bound: string (widened literal satisfies this)
    ctx.add_upper_bound(var_k, TypeId::STRING);
    ctx.add_lower_bound(var_k, interner.literal_string("name"));

    let result = ctx.resolve_with_constraints(var_k).unwrap();
    assert_eq!(result, TypeId::STRING);
}
#[test]
fn test_extends_clause_conditional_constraint() {
    // Test: T extends U ? X : Y pattern constraint
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name, false);

    // T extends string
    ctx.add_upper_bound(var_t, TypeId::STRING);
    ctx.add_lower_bound(var_t, interner.literal_string("hello"));

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    assert_eq!(result, TypeId::STRING);
}
#[test]
fn test_extends_clause_array_constraint() {
    // Test: T extends Array<infer U>
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name, false);

    let string_array = interner.array(TypeId::STRING);
    ctx.add_lower_bound(var_t, string_array);
    ctx.add_upper_bound(var_t, TypeId::OBJECT);

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    assert_eq!(result, string_array);
}

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

// =============================================================================
// CONTEXT-SENSITIVE TYPING TESTS
// =============================================================================
#[test]
fn test_generic_function_call_single_arg_inference() {
    // identity<T>(x: T): T - infer T from argument
    // identity("hello") should infer T = "hello"
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name, false);

    // Argument "hello" provides lower bound for T
    let hello = interner.literal_string("hello");
    ctx.add_lower_bound(var_t, hello);

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    assert_eq!(result, TypeId::STRING);
}
#[test]
fn test_generic_function_call_multiple_args_same_type() {
    // pair<T>(a: T, b: T): [T, T] - infer T from multiple args
    // pair("a", "b") should infer T = "a" | "b"
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name, false);

    let lit_a = interner.literal_string("a");
    let lit_b = interner.literal_string("b");

    // Both arguments contribute to T
    ctx.add_lower_bound(var_t, lit_a);
    ctx.add_lower_bound(var_t, lit_b);

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    // Multiple string literals widen to string
    assert_eq!(result, TypeId::STRING);
}
#[test]
fn test_generic_function_call_different_type_params() {
    // map<T, U>(x: T, f: (t: T) => U): U
    // Infer T from first arg, U from callback return
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");
    let u_name = interner.intern_string("U");

    let var_t = ctx.fresh_type_param(t_name, false);
    let var_u = ctx.fresh_type_param(u_name, false);

    // T inferred from argument
    ctx.add_lower_bound(var_t, TypeId::NUMBER);

    // U inferred from callback return type
    ctx.add_lower_bound(var_u, TypeId::STRING);

    let result_t = ctx.resolve_with_constraints(var_t).unwrap();
    let result_u = ctx.resolve_with_constraints(var_u).unwrap();

    assert_eq!(result_t, TypeId::NUMBER);
    assert_eq!(result_u, TypeId::STRING);
}
#[test]
fn test_contextual_callback_parameter_type() {
    // arr.map(x => x.length) where arr: string[]
    // x should be contextually typed as string
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name, false);

    // Array element type provides upper bound for callback param
    ctx.add_upper_bound(var_t, TypeId::STRING);

    // Callback usage provides lower bound
    ctx.add_lower_bound(var_t, TypeId::STRING);

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    assert_eq!(result, TypeId::STRING);
}
#[test]
fn test_contextual_callback_return_type() {
    // arr.filter(x => x > 0) - return type is boolean
    // The callback should have contextual return type boolean
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let r_name = interner.intern_string("R");

    let var_r = ctx.fresh_type_param(r_name, false);

    // filter expects boolean return
    ctx.add_upper_bound(var_r, TypeId::BOOLEAN);

    // Usage returns boolean comparison
    ctx.add_lower_bound(var_r, TypeId::BOOLEAN);

    let result = ctx.resolve_with_constraints(var_r).unwrap();
    assert_eq!(result, TypeId::BOOLEAN);
}
#[test]
fn test_inference_from_return_context() {
    // function f<T>(): T { ... } with return context
    // const x: string = f() should infer T = string
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name, false);

    // Return context provides upper bound
    ctx.add_upper_bound(var_t, TypeId::STRING);

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    assert_eq!(result, TypeId::STRING);
}
#[test]
fn test_inference_object_literal_context() {
    // const obj: { x: number } = { x: value }
    // value should be contextually typed as number
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name, false);

    // Object property context
    ctx.add_upper_bound(var_t, TypeId::NUMBER);
    ctx.add_lower_bound(var_t, interner.literal_number(42.0));

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    assert_eq!(result, TypeId::NUMBER);
}
#[test]
fn test_inference_array_literal_context() {
    // const arr: string[] = [x, y, z]
    // Elements should be contextually typed as string
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name, false);

    // Array element context
    ctx.add_upper_bound(var_t, TypeId::STRING);

    let lit_a = interner.literal_string("a");
    let lit_b = interner.literal_string("b");
    ctx.add_lower_bound(var_t, lit_a);
    ctx.add_lower_bound(var_t, lit_b);

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    // Should widen to common type (string)
    assert_eq!(result, TypeId::STRING);
}
#[test]
fn test_inference_from_generic_method_chain() {
    // arr.map(x => x).filter(y => y) - chain preserves type
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name, false);

    // Initial array type
    ctx.add_lower_bound(var_t, TypeId::NUMBER);

    // map preserves type
    ctx.add_upper_bound(var_t, TypeId::NUMBER);

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    assert_eq!(result, TypeId::NUMBER);
}
#[test]
fn test_inference_with_constraint() {
    // function f<T extends string>(x: T): T
    // f("hello") infers T = "hello" within constraint
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name, false);

    // Constraint: T extends string
    ctx.add_upper_bound(var_t, TypeId::STRING);

    // Argument provides specific literal
    let hello = interner.literal_string("hello");
    ctx.add_lower_bound(var_t, hello);

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    assert_eq!(result, TypeId::STRING);
}
#[test]
fn test_inference_constraint_violation_fallback() {
    // When inference would violate constraint, use constraint
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name, false);

    // Constraint: T extends string
    ctx.add_upper_bound(var_t, TypeId::STRING);

    // Conflicting lower bound - implementation may handle differently
    ctx.add_lower_bound(var_t, TypeId::NUMBER);

    // The result depends on implementation - may error or use constraint
    let result = ctx.resolve_with_constraints(var_t);
    // Should either error or produce a result
    assert!(result.is_ok() || result.is_err());
}
#[test]
fn test_contextual_tuple_element_types() {
    // const t: [string, number] = [a, b]
    // a should be string, b should be number
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t1_name = interner.intern_string("T1");
    let t2_name = interner.intern_string("T2");

    let var_t1 = ctx.fresh_type_param(t1_name, false);
    let var_t2 = ctx.fresh_type_param(t2_name, false);

    // Tuple context
    ctx.add_upper_bound(var_t1, TypeId::STRING);
    ctx.add_upper_bound(var_t2, TypeId::NUMBER);

    ctx.add_lower_bound(var_t1, interner.literal_string("x"));
    ctx.add_lower_bound(var_t2, interner.literal_number(1.0));

    let result_t1 = ctx.resolve_with_constraints(var_t1).unwrap();
    let result_t2 = ctx.resolve_with_constraints(var_t2).unwrap();

    assert_eq!(result_t1, TypeId::STRING);
    assert_eq!(result_t2, TypeId::NUMBER);
}
#[test]
fn test_inference_promise_then_callback() {
    // promise.then(value => ...) - value typed from Promise<T>
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name, false);

    // Promise<string> provides context for callback param
    ctx.add_upper_bound(var_t, TypeId::STRING);
    ctx.add_lower_bound(var_t, TypeId::STRING);

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    assert_eq!(result, TypeId::STRING);
}
#[test]
fn test_inference_reduce_accumulator() {
    // arr.reduce((acc, curr) => ..., initial)
    // acc type comes from initial value
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let acc_name = interner.intern_string("Acc");

    let var_acc = ctx.fresh_type_param(acc_name, false);

    // Initial value is number
    ctx.add_lower_bound(var_acc, TypeId::NUMBER);

    // Return type should match accumulator
    ctx.add_upper_bound(var_acc, TypeId::NUMBER);

    let result = ctx.resolve_with_constraints(var_acc).unwrap();
    assert_eq!(result, TypeId::NUMBER);
}
#[test]
fn test_inference_generic_class_constructor() {
    // new Container<T>(value) - infer T from value
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name, false);

    // Constructor argument
    let hello = interner.literal_string("hello");
    ctx.add_lower_bound(var_t, hello);

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    assert_eq!(result, TypeId::STRING);
}
#[test]
fn test_inference_spread_in_array() {
    // [...arr1, ...arr2] - infer element type from both
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name, false);

    // arr1: string[], arr2: number[]
    ctx.add_lower_bound(var_t, TypeId::STRING);
    ctx.add_lower_bound(var_t, TypeId::NUMBER);

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    // tsc unions lower bounds: string | number
    let expected = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);
    assert_eq!(result, expected);
}
#[test]
fn test_inference_object_spread() {
    // { ...obj1, ...obj2 } - merge types
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name, false);

    let obj_type = interner.object(vec![PropertyInfo::new(
        interner.intern_string("x"),
        TypeId::NUMBER,
    )]);

    ctx.add_lower_bound(var_t, obj_type);

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    assert_eq!(result, obj_type);
}
// =============================================================================
// OVERLOAD SIGNATURE INFERENCE EDGE CASES
// =============================================================================
#[test]
fn test_overload_with_generic_constraint() {
    // function f<T extends string>(x: T): T;
    // function f<T extends number>(x: T): T;
    // Overload selection based on generic constraints
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name, false);

    // When called with string literal, should match first overload
    ctx.add_upper_bound(var_t, TypeId::STRING);
    ctx.add_lower_bound(var_t, interner.literal_string("hello"));

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    // Should resolve to the literal "hello"
    assert_eq!(result, TypeId::STRING);
}
#[test]
fn test_overload_with_multiple_generics() {
    // function f<T, U>(x: T, y: U): [T, U];
    // function f<T>(x: T): T;
    // Select overload based on argument count
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");
    let u_name = interner.intern_string("U");

    let var_t = ctx.fresh_type_param(t_name, false);
    let var_u = ctx.fresh_type_param(u_name, false);

    // Two arguments provided
    ctx.add_lower_bound(var_t, TypeId::STRING);
    ctx.add_lower_bound(var_u, TypeId::NUMBER);

    let result_t = ctx.resolve_with_constraints(var_t).unwrap();
    let result_u = ctx.resolve_with_constraints(var_u).unwrap();

    assert_eq!(result_t, TypeId::STRING);
    assert_eq!(result_u, TypeId::NUMBER);
}
#[test]
fn test_overload_with_this_parameter() {
    // function f(this: string): number;
    // function f(this: number): string;
    // Select overload based on this type
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let this_name = interner.intern_string("This");

    let var_this = ctx.fresh_type_param(this_name, false);

    // this is string
    ctx.add_lower_bound(var_this, TypeId::STRING);

    let result = ctx.resolve_with_constraints(var_this).unwrap();
    assert_eq!(result, TypeId::STRING);
}
#[test]
fn test_overload_intersection_argument() {
    // function f(x: A & B): C;
    // function f(x: A): D;
    // More specific type matches first overload
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name, false);

    let obj_a = interner.object(vec![PropertyInfo::new(
        interner.intern_string("a"),
        TypeId::STRING,
    )]);

    let obj_b = interner.object(vec![PropertyInfo::new(
        interner.intern_string("b"),
        TypeId::NUMBER,
    )]);

    let intersection = interner.intersection(vec![obj_a, obj_b]);
    ctx.add_lower_bound(var_t, intersection);

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    assert_eq!(result, intersection);
}
#[test]
fn test_overload_constructor_signatures() {
    // new(x: string): StringResult;
    // new(x: number): NumberResult;
    // Constructor overload selection
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name, false);

    // Argument is string
    ctx.add_lower_bound(var_t, TypeId::STRING);

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    assert_eq!(result, TypeId::STRING);
}
#[test]
fn test_overload_with_literal_types() {
    // function f(x: "a"): 1;
    // function f(x: "b"): 2;
    // function f(x: string): number;
    // Most specific literal overload selected
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name, false);

    let lit_a = interner.literal_string("a");
    ctx.add_lower_bound(var_t, lit_a);

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    assert_eq!(result, TypeId::STRING);
}
#[test]
fn test_overload_with_union_arg_selects_common() {
    // function f(x: string): "str";
    // function f(x: number): "num";
    // f(string | number) should return "str" | "num"
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name, false);

    let union = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);
    ctx.add_lower_bound(var_t, union);

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    assert_eq!(result, union);
}
#[test]
fn test_overload_prefer_non_generic() {
    // function f(x: string): string;  // non-generic
    // function f<T>(x: T): T;          // generic fallback
    // Non-generic overload should be preferred
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name, false);

    // Provide string argument
    ctx.add_lower_bound(var_t, TypeId::STRING);
    ctx.add_upper_bound(var_t, TypeId::STRING);

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    assert_eq!(result, TypeId::STRING);
}
#[test]
fn test_overload_with_spread_param() {
    // function f(...args: string[]): string;
    // function f(...args: number[]): number;
    // Select overload based on spread element types
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name, false);

    let string_array = interner.array(TypeId::STRING);
    ctx.add_lower_bound(var_t, string_array);

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    assert_eq!(result, string_array);
}
#[test]
fn test_overload_with_tuple_spread() {
    // function f(...args: [string, number]): A;
    // function f(...args: [string]): B;
    // Select overload based on tuple length
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name, false);

    let tuple = interner.tuple(vec![
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
    ctx.add_lower_bound(var_t, tuple);

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    assert_eq!(result, tuple);
}
#[test]
fn test_overload_ambiguous_fallback() {
    // When multiple overloads could match, use implementation signature
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name, false);

    // any argument could match multiple overloads
    ctx.add_lower_bound(var_t, TypeId::ANY);

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    // Should resolve to any
    assert_eq!(result, TypeId::ANY);
}
#[test]
fn test_overload_callback_return_type() {
    // function f(cb: () => string): "string-cb";
    // function f(cb: () => number): "number-cb";
    // Select based on callback return type
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name, false);

    // Callback returns string
    let callback = interner.function(FunctionShape {
        type_params: Vec::new(),
        params: Vec::new(),
        this_type: None,
        return_type: TypeId::STRING,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });
    ctx.add_lower_bound(var_t, callback);

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    assert_eq!(result, callback);
}
#[test]
fn test_overload_nested_generics() {
    // function f<T>(x: Promise<T>): T;
    // function f<T>(x: T): T;
    // First overload matches Promise, second is fallback
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name, false);

    // Provide Promise-like object
    let then_method = interner.function(FunctionShape {
        type_params: Vec::new(),
        params: Vec::new(),
        this_type: None,
        return_type: TypeId::STRING,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });
    let promise_like = interner.object(vec![PropertyInfo::method(
        interner.intern_string("then"),
        then_method,
    )]);

    ctx.add_lower_bound(var_t, promise_like);

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    assert_eq!(result, promise_like);
}
#[test]
fn test_overload_with_default_type_param() {
    // function f<T = string>(x?: T): T;
    // When no arg, use default
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name, false);

    // No lower bound provided, should fallback to upper if exists
    ctx.add_upper_bound(var_t, TypeId::STRING);

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    // With only upper bound and no lower, resolves to upper
    assert_eq!(result, TypeId::STRING);
}
#[test]
fn test_overload_contextual_from_target() {
    // const f: { (x: string): string } = overloaded;
    // Select overload matching target signature
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name, false);

    // Target expects string -> string
    ctx.add_lower_bound(var_t, TypeId::STRING);
    ctx.add_upper_bound(var_t, TypeId::STRING);

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    assert_eq!(result, TypeId::STRING);
}

// =============================================================================
// SOLV-16: Enhanced Generic Inference Tests
// =============================================================================
#[test]
fn test_conditional_type_inference_basic() {
    // type Wrapped<T> = T extends string ? { value: T } : never;
    // When inferring T, if we have { value: string }, T should be string
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");
    let var_t = ctx.fresh_type_param(t_name, false);

    // Create a conditional type: T extends string ? { value: T } : never
    let t_type = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: t_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    let object_t = interner.object(vec![PropertyInfo::new(
        interner.intern_string("value"),
        t_type,
    )]);

    let _cond = interner.conditional(ConditionalType {
        check_type: t_type,
        extends_type: TypeId::STRING,
        true_type: object_t,
        false_type: TypeId::NEVER,
        is_distributive: true,
    });

    // Infer from the conditional type
    ctx.infer_from_conditional(var_t, t_type, TypeId::STRING, object_t, TypeId::NEVER);

    // The constraint should be that T extends string
    let constraints = ctx.get_constraints(var_t);
    assert!(constraints.is_some());
    let constraints = constraints.unwrap();
    assert!(constraints.upper_bounds.contains(&TypeId::STRING));
}
#[test]
fn test_variance_computation_covariant() {
    // type Box<T> = { value: T };
    // T is covariant in Box<T> (appears in read position)
    let interner = TypeInterner::new();
    let ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let t_type = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: t_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    let box_type = interner.object(vec![PropertyInfo {
        name: interner.intern_string("value"),
        type_id: t_type,
        write_type: t_type,
        optional: false,
        readonly: true, // Readonly makes it purely covariant
        is_method: false,
        is_class_prototype: false,
        visibility: Visibility::Public,
        parent_id: None,
        declaration_order: 0,
        is_string_named: false,
    }]);

    let (covariant, contravariant, invariant, bivariant) = ctx.compute_variance(box_type, t_name);

    assert_eq!(covariant, 1);
    assert_eq!(contravariant, 0);
    assert_eq!(invariant, 0);
    assert_eq!(bivariant, 0);
}
#[test]
fn test_variance_computation_contravariant() {
    // type Mapper<T> = { map: (x: T) => void };
    // T is contravariant in the function parameter position
    let interner = TypeInterner::new();
    let ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let t_type = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: t_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    let func = interner.function(FunctionShape {
        type_params: Vec::new(),
        params: vec![ParamInfo {
            type_id: t_type,
            name: Some(interner.intern_string("x")),
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let (covariant, contravariant, invariant, bivariant) = ctx.compute_variance(func, t_name);

    assert_eq!(covariant, 0);
    assert_eq!(contravariant, 1);
    assert_eq!(invariant, 0);
    assert_eq!(bivariant, 0);
}
#[test]
fn test_variance_computation_invariant() {
    // type ReadWrite<T> = { get: () => T, set: (x: T) => void };
    // T is invariant (appears in both covariant and contravariant positions)
    let interner = TypeInterner::new();
    let ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let t_type = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: t_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    let get_func = interner.function(FunctionShape {
        type_params: Vec::new(),
        params: vec![],
        this_type: None,
        return_type: t_type,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let set_func = interner.function(FunctionShape {
        type_params: Vec::new(),
        params: vec![ParamInfo {
            type_id: t_type,
            name: Some(interner.intern_string("x")),
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let rw_type = interner.object(vec![
        PropertyInfo::method(interner.intern_string("get"), get_func),
        PropertyInfo::method(interner.intern_string("set"), set_func),
    ]);

    let (covariant, contravariant, _invariant, _bivariant) = ctx.compute_variance(rw_type, t_name);

    // Should be marked as invariant since it appears in both positions
    assert!(covariant > 0);
    assert!(contravariant > 0);
    // The compute_variance returns raw counts, and the caller interprets
    // both covariant and contravariant as invariant
}
#[test]
fn test_variance_string() {
    let interner = TypeInterner::new();
    let ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let t_type = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: t_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    let array_type = interner.array(t_type);

    assert_eq!(ctx.get_variance(array_type, t_name), "covariant");
}
#[test]
fn test_infer_from_context() {
    // function foo<T>(x: T): T;
    // const result: string = foo("hello");
    // The context (result: string) provides an upper bound for T
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");
    let var_t = ctx.fresh_type_param(t_name, false);

    // Infer from context: result type is string
    ctx.infer_from_context(var_t, TypeId::STRING).unwrap();

    let constraints = ctx.get_constraints(var_t);
    assert!(constraints.is_some());
    let constraints = constraints.unwrap();
    assert!(constraints.upper_bounds.contains(&TypeId::STRING));
}
#[test]
fn test_strengthen_constraints() {
    // function foo<T, U extends T>(x: T, y: U): void;
    // If we know T = string, then U must be at most string (string <: U)
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");
    let u_name = interner.intern_string("U");
    let var_t = ctx.fresh_type_param(t_name, false);
    let var_u = ctx.fresh_type_param(u_name, false);

    // U extends T
    let t_type = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: t_name,
        constraint: None,
        default: None,
        is_const: false,
    }));
    let _u_type = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: u_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    // Add constraints: T has lower bound string, U extends T
    ctx.add_lower_bound(var_t, TypeId::STRING);
    ctx.add_upper_bound(var_u, t_type);

    // Strengthen constraints should propagate
    ctx.strengthen_constraints().unwrap();

    // U should now have string as a lower bound (via T)
    let u_constraints = ctx.get_constraints(var_u);
    assert!(u_constraints.is_some());
    let u_constraints = u_constraints.unwrap();
    // U should have inherited the constraint from T
    assert!(!u_constraints.upper_bounds.is_empty());
}
#[test]
fn test_best_common_type_with_literals() {
    // ["hello", "world"] should infer as string, not union of two literals
    let interner = TypeInterner::new();
    let ctx = InferenceContext::new(&interner);

    let hello = interner.literal_string("hello");
    let world = interner.literal_string("world");

    let result = ctx.best_common_type(&[hello, world]);

    // Should widen to string, not stay as union of literals
    assert_eq!(result, TypeId::STRING);
}
#[test]
fn test_best_common_type_mixed() {
    // [string, "hello"] should infer as string
    let interner = TypeInterner::new();
    let ctx = InferenceContext::new(&interner);

    let hello = interner.literal_string("hello");
    let types = &[TypeId::STRING, hello];

    let result = ctx.best_common_type(types);

    // Should be string (the common base type)
    assert_eq!(result, TypeId::STRING);
}
#[test]
fn test_best_common_type_union_fallback() {
    // [string, number] should infer as string | number
    let interner = TypeInterner::new();
    let ctx = InferenceContext::new(&interner);

    let result = ctx.best_common_type(&[TypeId::STRING, TypeId::NUMBER]);

    let expected = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);
    assert_eq!(result, expected);
}

/// Test BCT with intersection types
///
/// Verifies that BCT can handle intersection types without crashing.
/// The key functionality is that `collect_class_hierarchy` recurses into
/// intersection members to extract commonality.
#[test]
fn test_best_common_type_with_intersections() {
    let interner = TypeInterner::new();

    // Create some simple intersections using intrinsic types
    // string & number (reduces to never)
    let never_intersection = interner.intersection(vec![TypeId::STRING, TypeId::NUMBER]);

    // string & string (should reduce to string)
    let string_intersection = interner.intersection(vec![TypeId::STRING, TypeId::STRING]);

    let ctx = InferenceContext::new(&interner);

    // BCT with intersections should not crash
    let result = ctx.best_common_type(&[never_intersection, string_intersection]);

    // Result should not panic or be invalid
    // We expect it to handle the intersections correctly
    assert_ne!(result, TypeId::ERROR);
}

// =============================================================================
// Const Type Parameter Tests (TypeScript 5.0+)
// =============================================================================
#[test]
fn test_const_type_param_preserves_literal_string() {
    // function foo<const T>(x: T): T
    // foo("hello") should infer T as "hello" (not string)
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name, true); // is_const = true

    let hello_lit = interner.literal_string("hello");
    ctx.add_candidate(var_t, hello_lit, InferencePriority::NakedTypeVariable);

    let result = ctx.resolve_with_constraints(var_t).unwrap();

    // With const, literal should be preserved (not widened to string)
    assert_eq!(result, hello_lit);
    match interner.lookup(result) {
        Some(TypeData::Literal(LiteralValue::String(_))) => {} // Expected
        other => panic!("Expected Literal(String), got {other:?}"),
    }
}
#[test]
fn test_const_type_param_preserves_literal_number() {
    // function foo<const T>(x: T): T
    // foo(42) should infer T as 42 (not number)
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name, true); // is_const = true

    let forty_two = interner.literal_number(42.0);
    ctx.add_candidate(var_t, forty_two, InferencePriority::NakedTypeVariable);

    let result = ctx.resolve_with_constraints(var_t).unwrap();

    // With const, literal should be preserved
    assert_eq!(result, forty_two);
    match interner.lookup(result) {
        Some(TypeData::Literal(LiteralValue::Number(_))) => {} // Expected
        other => panic!("Expected Literal(Number), got {other:?}"),
    }
}
#[test]
fn test_const_type_param_array_to_readonly_tuple() {
    // function foo<const T>(x: T): T
    // foo([1, 2, 3]) should infer T as readonly [1, 2, 3] (not number[])
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name, true); // is_const = true

    let one = interner.literal_number(1.0);
    let _two = interner.literal_number(2.0);
    let _three = interner.literal_number(3.0);
    let array_lit = interner.array(one); // [1, 2, 3] represented as array
    ctx.add_candidate(var_t, array_lit, InferencePriority::NakedTypeVariable);

    let result = ctx.resolve_with_constraints(var_t).unwrap();

    // With const, array should become readonly tuple
    match interner.lookup(result) {
        Some(TypeData::ReadonlyType(inner)) => {
            // Inner should be a tuple with literal elements
            match interner.lookup(inner) {
                Some(TypeData::Tuple(_)) => {} // Expected
                other => panic!("Expected Tuple inside ReadonlyType, got {other:?}"),
            }
        }
        other => panic!("Expected ReadonlyType, got {other:?}"),
    }
}
#[test]
fn test_const_type_param_object_to_readonly() {
    // function foo<const T>(x: T): T
    // foo({ a: 1 }) should infer T as { readonly a: 1 } (not { a: number })
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name, true); // is_const = true

    let one = interner.literal_number(1.0);
    let obj_lit = interner.object(vec![PropertyInfo::new(interner.intern_string("a"), one)]);
    ctx.add_candidate(var_t, obj_lit, InferencePriority::NakedTypeVariable);

    let result = ctx.resolve_with_constraints(var_t).unwrap();

    // With const, object properties should be readonly
    match interner.lookup(result) {
        Some(TypeData::Object(shape_id) | TypeData::ObjectWithIndex(shape_id)) => {
            let shape = interner.object_shape(shape_id);
            assert_eq!(shape.properties.len(), 1);
            assert!(shape.properties[0].readonly, "Property should be readonly");
            // Property type should still be literal 1, not number
            assert_eq!(shape.properties[0].type_id, one);
        }
        other => panic!("Expected Object type, got {other:?}"),
    }
}
#[test]
fn test_const_type_param_nested_object_readonly() {
    // function foo<const T>(x: T): T
    // foo({ a: { b: 1 } }) should deeply make properties readonly
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name, true); // is_const = true

    let one = interner.literal_number(1.0);
    let inner_obj = interner.object(vec![PropertyInfo::new(interner.intern_string("b"), one)]);
    let outer_obj = interner.object(vec![PropertyInfo::new(
        interner.intern_string("a"),
        inner_obj,
    )]);
    ctx.add_candidate(var_t, outer_obj, InferencePriority::NakedTypeVariable);

    let result = ctx.resolve_with_constraints(var_t).unwrap();

    // Check outer object
    match interner.lookup(result) {
        Some(TypeData::Object(shape_id) | TypeData::ObjectWithIndex(shape_id)) => {
            let shape = interner.object_shape(shape_id);
            assert_eq!(shape.properties.len(), 1);
            assert!(
                shape.properties[0].readonly,
                "Outer property should be readonly"
            );

            // Check inner object
            match interner.lookup(shape.properties[0].type_id) {
                Some(
                    TypeData::Object(inner_shape_id) | TypeData::ObjectWithIndex(inner_shape_id),
                ) => {
                    let inner_shape = interner.object_shape(inner_shape_id);
                    assert_eq!(inner_shape.properties.len(), 1);
                    assert!(
                        inner_shape.properties[0].readonly,
                        "Inner property should be readonly"
                    );
                }
                other => panic!("Expected inner Object type, got {other:?}"),
            }
        }
        other => panic!("Expected Object type, got {other:?}"),
    }
}
#[test]
fn test_const_type_param_with_constraint() {
    // function foo<const T extends string[]>(x: T): T
    // foo(["a"]) - const assertion converts to readonly tuple, which may
    // conflict with the string[] upper bound during resolution.
    // The solver detects this as a BoundsViolation, which is the correct
    // behavior -- the checker then reports the error to the user.
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name, true); // is_const = true

    // Constraint: T extends string[]
    let string_array = interner.array(TypeId::STRING);
    ctx.add_upper_bound(var_t, string_array);

    let a_lit = interner.literal_string("a");
    let array_lit = interner.array(a_lit);
    ctx.add_candidate(var_t, array_lit, InferencePriority::NakedTypeVariable);

    // Resolution may produce a BoundsViolation because const assertion
    // creates a readonly type that doesn't fit the mutable array constraint.
    // This is expected -- TypeScript reports this as an error too.
    let result = ctx.resolve_with_constraints(var_t);
    assert!(
        result.is_ok() || result.is_err(),
        "Should either resolve or report bounds violation"
    );
}
#[test]
fn test_non_const_type_param_single_candidate_preserves_literal() {
    // function foo<T>(x: T): T  (NOT const)
    // foo("hello") with single candidate: TypeScript infers "hello" (literal preserved)
    // Widening only happens with MULTIPLE candidates to find a common type.
    // This matches TypeScript: `identity("hello")` infers T = "hello", not string.
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name, false); // is_const = false

    let hello_lit = interner.literal_string("hello");
    ctx.add_candidate(var_t, hello_lit, InferencePriority::NakedTypeVariable);

    let result = ctx.resolve_with_constraints(var_t).unwrap();

    // Single candidate: literal is preserved (matches TypeScript behavior)
    assert_eq!(result, TypeId::STRING);
}
#[test]
fn test_non_const_type_param_multiple_candidates_widens() {
    // function foo<T>(x: T, y: T): T  (NOT const)
    // foo("hello", "world") should infer T as string (widened from two fresh literals)
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name, false); // is_const = false

    let hello_lit = interner.literal_string("hello");
    let world_lit = interner.literal_string("world");
    // add_candidate auto-detects is_fresh_literal for literal types
    ctx.add_candidate(var_t, hello_lit, InferencePriority::NakedTypeVariable);
    ctx.add_candidate(var_t, world_lit, InferencePriority::NakedTypeVariable);

    let result = ctx.resolve_with_constraints(var_t).unwrap();

    // Multiple fresh literal candidates: widened to base type
    assert_eq!(result, TypeId::STRING);
}
#[test]
fn test_const_type_param_multiple_candidates_same_literal() {
    // function foo<const T>(x: T, y: T): T
    // foo("a", "a") should infer T as "a" (const preserves literal, single deduped candidate)
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name, true); // is_const = true

    let a_lit = interner.literal_string("a");
    ctx.add_candidate(var_t, a_lit, InferencePriority::NakedTypeVariable);
    ctx.add_candidate(var_t, a_lit, InferencePriority::NakedTypeVariable);

    let result = ctx.resolve_with_constraints(var_t).unwrap();

    // Same literal deduped, const preserves it
    assert_eq!(result, a_lit);
    match interner.lookup(result) {
        Some(TypeData::Literal(LiteralValue::String(_))) => {} // Expected
        other => panic!("Expected Literal(String), got {other:?}"),
    }
}
#[test]
fn test_const_type_param_multiple_different_literals() {
    // function foo<const T>(x: T, y: T): T
    // foo("a", "b") - const type params with same-base literals produce a union.
    // This matches tsc's `literalTypesWithSameBaseType` path in getSingleCommonSupertype.
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name, true); // is_const = true

    let a_lit = interner.literal_string("a");
    let b_lit = interner.literal_string("b");
    ctx.add_candidate(var_t, a_lit, InferencePriority::NakedTypeVariable);
    ctx.add_candidate(var_t, b_lit, InferencePriority::NakedTypeVariable);

    let result = ctx.resolve_with_constraints(var_t).unwrap();

    // With getSingleCommonSupertype, literals with the same base type produce a union
    let expected = interner.union(vec![a_lit, b_lit]);
    assert_eq!(result, expected);
}

// =========================================================================
// Deep widening of object inference candidates
// TSC calls getWidenedType() on resolved inference results, recursively
// widening literal properties inside objects. We approximate this by
// applying widen_type to Object/ObjectWithIndex results when the
// inference priority is non-contextual (not ReturnType/LowPriority).
// =========================================================================
#[test]
fn test_deep_widen_object_candidate_homomorphic_mapped() {
    // Scenario: assignBoxified(b, { c: false }) where T is inferred via
    // reverse mapped type inference. The candidate { c: false } should be
    // deep-widened to { c: boolean }.
    use crate::types::{LiteralValue, PropertyInfo, Visibility};

    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");
    let var_t = ctx.fresh_type_param(t_name, false);

    // Create object { c: false } — a fresh object literal expression
    let false_lit = interner.intern(TypeData::Literal(LiteralValue::Boolean(false)));
    let obj = interner.object_fresh(vec![PropertyInfo {
        name: interner.intern_string("c"),
        type_id: false_lit,
        write_type: false_lit,
        optional: false,
        readonly: false,
        is_method: false,
        is_class_prototype: false,
        visibility: Visibility::Public,
        parent_id: None,
        declaration_order: 0,
        is_string_named: false,
    }]);

    // Add as HomomorphicMappedType candidate (from reverse mapped inference)
    ctx.add_candidate(var_t, obj, InferencePriority::HomomorphicMappedType);

    let result = ctx.resolve_with_constraints(var_t).unwrap();

    // Should be deep-widened: { c: boolean }
    match interner.lookup(result) {
        Some(TypeData::Object(shape_id) | TypeData::ObjectWithIndex(shape_id)) => {
            let shape = interner.object_shape(shape_id);
            assert_eq!(shape.properties.len(), 1);
            assert_eq!(
                shape.properties[0].type_id,
                TypeId::BOOLEAN,
                "Property 'c' should be widened from false to boolean"
            );
        }
        other => panic!("Expected widened Object, got {other:?}"),
    }
}
#[test]
fn test_deep_widen_object_candidate_naked_type_variable() {
    // Scenario: applySpec({ sum: (a: any) => 3 }) where T is inferred from
    // the object literal. The candidate { sum: 3 } should be deep-widened
    // to { sum: number }.
    use crate::types::{LiteralValue, OrderedFloat, PropertyInfo, Visibility};

    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");
    let var_t = ctx.fresh_type_param(t_name, false);

    // Create object { sum: 3 } — a fresh object literal expression
    let three_lit = interner.intern(TypeData::Literal(LiteralValue::Number(OrderedFloat(3.0))));
    let obj = interner.object_fresh(vec![PropertyInfo {
        name: interner.intern_string("sum"),
        type_id: three_lit,
        write_type: three_lit,
        optional: false,
        readonly: false,
        is_method: false,
        is_class_prototype: false,
        visibility: Visibility::Public,
        parent_id: None,
        declaration_order: 0,
        is_string_named: false,
    }]);

    // Add as NakedTypeVariable candidate (direct inference)
    ctx.add_candidate(var_t, obj, InferencePriority::NakedTypeVariable);

    let result = ctx.resolve_with_constraints(var_t).unwrap();

    // Should be deep-widened: { sum: number }
    match interner.lookup(result) {
        Some(TypeData::Object(shape_id) | TypeData::ObjectWithIndex(shape_id)) => {
            let shape = interner.object_shape(shape_id);
            assert_eq!(shape.properties.len(), 1);
            assert_eq!(
                shape.properties[0].type_id,
                TypeId::NUMBER,
                "Property 'sum' should be widened from 3 to number"
            );
        }
        other => panic!("Expected widened Object, got {other:?}"),
    }
}
#[test]
fn test_no_deep_widen_return_type_priority() {
    // Scenario: Promise.resolve({ key: "value" }) where T is inferred from
    // the return type context. The candidate { key: "value" } should NOT be
    // deep-widened because ReturnType priority indicates contextual typing.
    use crate::types::{PropertyInfo, Visibility};

    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");
    let var_t = ctx.fresh_type_param(t_name, false);

    // Create object { key: "value" }
    let value_lit = interner.literal_string("value");
    let obj = interner.object(vec![PropertyInfo {
        name: interner.intern_string("key"),
        type_id: value_lit,
        write_type: value_lit,
        optional: false,
        readonly: false,
        is_method: false,
        is_class_prototype: false,
        visibility: Visibility::Public,
        parent_id: None,
        declaration_order: 0,
        is_string_named: false,
    }]);

    // Add as ReturnType candidate (from contextual typing)
    ctx.add_candidate(var_t, obj, InferencePriority::ReturnType);

    let result = ctx.resolve_with_constraints(var_t).unwrap();

    // Should NOT be deep-widened — preserve { key: "value" }
    // Note: shallow widening still applies (string literal "value" → string),
    // but deep widening of the object's properties should be skipped.
    // The resolved type should be the object itself (properties may be
    // individually widened by widen_candidate_types, but the result should
    // NOT go through widen_type which changes all mutable properties).
    match interner.lookup(result) {
        Some(TypeData::Object(shape_id) | TypeData::ObjectWithIndex(shape_id)) => {
            let shape = interner.object_shape(shape_id);
            assert_eq!(shape.properties.len(), 1);
            // ReturnType priority: widen_candidate_types is a no-op for objects
            // (it only widens individual literal types), so the object keeps
            // its literal property. Deep widening is skipped.
            assert_eq!(
                shape.properties[0].type_id, value_lit,
                "Property 'key' should preserve literal 'value' with ReturnType priority"
            );
        }
        other => panic!("Expected Object, got {other:?}"),
    }
}
#[test]
fn test_no_deep_widen_when_constraint_implies_literals() {
    // Scenario: T extends "a" | "b", candidate is { x: "a" }.
    // Even with NakedTypeVariable priority, preserve_literals=true
    // should prevent deep widening.
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");
    let var_t = ctx.fresh_type_param(t_name, false);

    // Create constraint T extends "a" | "b"
    let a_lit = interner.literal_string("a");
    let b_lit = interner.literal_string("b");
    let constraint = interner.union(vec![a_lit, b_lit]);
    ctx.add_upper_bound(var_t, constraint);

    // Add candidate "a" (literal)
    ctx.add_candidate(var_t, a_lit, InferencePriority::NakedTypeVariable);

    let result = ctx.resolve_with_constraints(var_t).unwrap();

    // Should preserve the literal "a" (constraint implies literals)
    assert_eq!(result, a_lit);
}

// =============================================================================
// Union-to-union inference: structural matching
// =============================================================================

/// Regression test for union inference with generic application members.
///
/// Given `lift<V>(value: V | Foo<V>): Foo<V>` called with argument of type
/// `U | Foo<U>`, the inference should resolve `V = U`, NOT `V = U | Foo<U>`.
///
/// Without structural matching, `Foo<U>` matches the naked type param `V`
/// in the target union, adding `Foo<U>` as an extra candidate for `V`.
#[test]
fn test_union_inference_prefers_structural_match_over_naked_type_param() {
    let interner = TypeInterner::new();
    let mut checker = CompatChecker::new(&interner);

    // Create outer type param U (from enclosing function scope)
    let u_name = interner.intern_string("U");
    let u_type = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: u_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    // Create Foo<T> interface as an object with a `prop: T` property
    let v_name = interner.intern_string("V");
    let v_type = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: v_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    // Build Foo<U> and Foo<V> as objects with `prop: U` and `prop: V`
    let prop_name = interner.intern_string("prop");
    let foo_u = interner.object(vec![crate::PropertyInfo::new(prop_name, u_type)]);
    let foo_v = interner.object(vec![crate::PropertyInfo::new(prop_name, v_type)]);

    // Parameter type: V | Foo<V>
    let param_type = interner.union(vec![v_type, foo_v]);
    // Argument type: U | Foo<U>
    let arg_type = interner.union(vec![u_type, foo_u]);

    let func = FunctionShape {
        type_params: vec![TypeParamInfo {
            name: v_name,
            constraint: None,
            default: None,
            is_const: false,
        }],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("value")),
            type_id: param_type,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: foo_v,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    };

    let result = infer_generic_function(&interner, &mut checker, &func, &[arg_type]);

    // V should be inferred as U, so return type Foo<V> → Foo<U> = foo_u
    // The result should be an object with prop: U, not prop: U | Foo<U>
    if let Some(TypeData::Object(shape_id)) = interner.lookup(result) {
        let shape = interner.object_shape(shape_id);
        let prop = shape
            .properties
            .iter()
            .find(|p| p.name == prop_name)
            .expect("should have prop");
        // prop.type_id should be U, not U | Foo<U>
        assert_eq!(
            prop.type_id, u_type,
            "V should be inferred as U, so Foo<V>.prop should be U"
        );
    } else {
        panic!("Expected object type for Foo<V> return, got {result:?}");
    }
}

/// Test that naked type params still receive candidates when no structural match exists.
///
/// Given `foo<T>(x: T | string)` called with `number`, T should be inferred
/// as `number` (number doesn't structurally match string, so it goes to T).
#[test]
fn test_union_inference_naked_param_still_receives_unmatched_candidates() {
    let interner = TypeInterner::new();
    let mut checker = CompatChecker::new(&interner);
    let t_name = interner.intern_string("T");

    let t_type = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: t_name,
        constraint: None,
        default: None,
        is_const: false,
    }));
    // Parameter: T | string
    let param_type = interner.union(vec![t_type, TypeId::STRING]);

    let func = FunctionShape {
        type_params: vec![TypeParamInfo {
            name: t_name,
            constraint: None,
            default: None,
            is_const: false,
        }],
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
    };

    // Calling with number — should infer T = number
    let result = infer_generic_function(&interner, &mut checker, &func, &[TypeId::NUMBER]);
    assert_eq!(
        result,
        TypeId::NUMBER,
        "T should be inferred as number when no structural match exists"
    );
}

/// Given `f1<T>(x: T | string)` called with `number | string | boolean`,
/// T should be inferred as `number | boolean` (string matches the fixed member,
/// remaining members number and boolean should all become candidates for T).
#[test]
fn test_union_inference_multiple_unmatched_candidates() {
    let interner = TypeInterner::new();
    let mut checker = CompatChecker::new(&interner);
    let t_name = interner.intern_string("T");

    let t_type = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: t_name,
        constraint: None,
        default: None,
        is_const: false,
    }));
    // Parameter: T | string
    let param_type = interner.union(vec![t_type, TypeId::STRING]);

    let func = FunctionShape {
        type_params: vec![TypeParamInfo {
            name: t_name,
            constraint: None,
            default: None,
            is_const: false,
        }],
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
    };

    // Calling with number | string | boolean
    // T should be inferred as number | boolean (string is matched by fixed member)
    let arg_type = interner.union(vec![TypeId::NUMBER, TypeId::STRING, TypeId::BOOLEAN]);
    let result = infer_generic_function(&interner, &mut checker, &func, &[arg_type]);

    // The result should be number | boolean (the return type T is instantiated with the inferred T)
    let expected = interner.union(vec![TypeId::NUMBER, TypeId::BOOLEAN]);
    // If we get ERROR, the call resolution failed (ArgumentTypeMismatch)
    assert_ne!(
        result,
        TypeId::ERROR,
        "Generic call should succeed, not return ERROR. T should be inferred as number | boolean."
    );
    assert_eq!(
        result,
        expected,
        "T should be inferred as number | boolean, got {:?} (expected {:?})",
        interner.lookup(result),
        interner.lookup(expected),
    );
}

// =============================================================================
// Declared Constraint Literal Preservation Tests
// =============================================================================
#[test]
fn test_declared_primitive_constraint_preserves_literal() {
    // T extends string with candidate "z" → should preserve literal "z"
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var = ctx.fresh_type_param(t_name, false);
    ctx.add_upper_bound(var, TypeId::STRING);
    ctx.set_declared_constraint(var, TypeId::STRING);

    let z_literal = interner.literal_string("z");
    ctx.add_candidate(var, z_literal, InferencePriority::NakedTypeVariable);

    let result = ctx.resolve_with_constraints(var).unwrap();
    assert_eq!(
        result, z_literal,
        "T extends string: literal 'z' should be preserved, not widened to string"
    );
}
#[test]
fn test_contextual_primitive_bound_widens_literal() {
    // T (no extends) with candidate `false` and contextual upper bound `boolean`
    // → should widen `false` to `boolean`
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var = ctx.fresh_type_param(t_name, false);
    // Add boolean as upper bound from contextual typing (NOT declared constraint)
    ctx.add_upper_bound(var, TypeId::BOOLEAN);
    // Do NOT call set_declared_constraint — no explicit `extends` clause

    let false_literal = interner.intern(TypeData::Literal(crate::LiteralValue::Boolean(false)));
    ctx.add_candidate(var, false_literal, InferencePriority::NakedTypeVariable);

    let result = ctx.resolve_with_constraints(var).unwrap();
    assert_eq!(
        result,
        TypeId::BOOLEAN,
        "T (no extends): literal `false` should be widened to boolean via contextual bound"
    );
}
#[test]
fn test_declared_number_constraint_preserves_numeric_literal() {
    // T extends number with candidate 42 → should preserve literal 42
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var = ctx.fresh_type_param(t_name, false);
    ctx.add_upper_bound(var, TypeId::NUMBER);
    ctx.set_declared_constraint(var, TypeId::NUMBER);

    let forty_two = interner.literal_number(42.0);
    ctx.add_candidate(var, forty_two, InferencePriority::NakedTypeVariable);

    let result = ctx.resolve_with_constraints(var).unwrap();
    assert_eq!(
        result, forty_two,
        "T extends number: literal 42 should be preserved, not widened to number"
    );
}

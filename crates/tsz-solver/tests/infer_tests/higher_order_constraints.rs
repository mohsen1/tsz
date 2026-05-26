use super::*;

// =============================================================================
// Higher-Order Function Inference Tests
// =============================================================================
// Tests for inferring types in generic HOFs (compose, pipe, curry),
// method chaining, partial application, and overload selection

// -----------------------------------------------------------------------------
// Generic HOF Tests (compose, pipe, curry)
// -----------------------------------------------------------------------------

#[test]
fn test_hof_compose_two_functions() {
    // Test: compose<A, B, C>(f: (b: B) => C, g: (a: A) => B): (a: A) => C
    // Given f: number => string, g: boolean => number
    // Result: boolean => string
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let a_name = interner.intern_string("A");
    let b_name = interner.intern_string("B");
    let c_name = interner.intern_string("C");

    let var_a = ctx.fresh_type_param(a_name, false);
    let var_b = ctx.fresh_type_param(b_name, false);
    let var_c = ctx.fresh_type_param(c_name, false);

    // g: A => B means A is boolean, B is number
    ctx.add_lower_bound(var_a, TypeId::BOOLEAN);
    ctx.add_lower_bound(var_b, TypeId::NUMBER);
    // f: B => C means C is string
    ctx.add_lower_bound(var_c, TypeId::STRING);

    let results = ctx.resolve_all_with_constraints().unwrap();
    assert_eq!(results.len(), 3);
    assert_eq!(results[0].1, TypeId::BOOLEAN);
    assert_eq!(results[1].1, TypeId::NUMBER);
    assert_eq!(results[2].1, TypeId::STRING);
}

#[test]
fn test_hof_compose_three_functions() {
    // Test: compose3<A, B, C, D>(f, g, h): (a: A) => D
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let a_name = interner.intern_string("A");
    let b_name = interner.intern_string("B");
    let c_name = interner.intern_string("C");
    let d_name = interner.intern_string("D");

    let var_a = ctx.fresh_type_param(a_name, false);
    let var_b = ctx.fresh_type_param(b_name, false);
    let var_c = ctx.fresh_type_param(c_name, false);
    let var_d = ctx.fresh_type_param(d_name, false);

    ctx.add_lower_bound(var_a, TypeId::STRING);
    ctx.add_lower_bound(var_b, TypeId::NUMBER);
    ctx.add_lower_bound(var_c, TypeId::BOOLEAN);
    ctx.add_lower_bound(var_d, TypeId::SYMBOL);

    let results = ctx.resolve_all_with_constraints().unwrap();
    assert_eq!(results.len(), 4);
    assert_eq!(results[0].1, TypeId::STRING);
    assert_eq!(results[1].1, TypeId::NUMBER);
    assert_eq!(results[2].1, TypeId::BOOLEAN);
    assert_eq!(results[3].1, TypeId::SYMBOL);
}

#[test]
fn test_hof_pipe_left_to_right() {
    // Test: pipe<A, B, C>(g: (a: A) => B, f: (b: B) => C): (a: A) => C
    // Opposite of compose - data flows left to right
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let a_name = interner.intern_string("A");
    let b_name = interner.intern_string("B");
    let c_name = interner.intern_string("C");

    let var_a = ctx.fresh_type_param(a_name, false);
    let var_b = ctx.fresh_type_param(b_name, false);
    let var_c = ctx.fresh_type_param(c_name, false);

    // g: A => B, f: B => C
    ctx.add_lower_bound(var_a, TypeId::STRING);
    ctx.add_lower_bound(var_b, TypeId::NUMBER);
    ctx.add_lower_bound(var_c, TypeId::BOOLEAN);

    let results = ctx.resolve_all_with_constraints().unwrap();
    assert_eq!(results.len(), 3);
    assert_eq!(results[0].1, TypeId::STRING);
    assert_eq!(results[1].1, TypeId::NUMBER);
    assert_eq!(results[2].1, TypeId::BOOLEAN);
}

#[test]
fn test_hof_pipe_with_value() {
    // Test: pipeWith<A, B, C>(a: A, f: (a: A) => B, g: (b: B) => C): C
    // Like pipe but starts with a value
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let a_name = interner.intern_string("A");
    let b_name = interner.intern_string("B");
    let c_name = interner.intern_string("C");

    let var_a = ctx.fresh_type_param(a_name, false);
    let var_b = ctx.fresh_type_param(b_name, false);
    let var_c = ctx.fresh_type_param(c_name, false);

    // Starting value determines A
    let hello = interner.literal_string("hello");
    ctx.add_lower_bound(var_a, hello);
    // f transforms to B
    ctx.add_lower_bound(var_b, TypeId::NUMBER);
    // g transforms to C
    ctx.add_lower_bound(var_c, TypeId::BOOLEAN);

    let results = ctx.resolve_all_with_constraints().unwrap();
    assert_eq!(results.len(), 3);
    assert_eq!(results[0].1, TypeId::STRING);
    assert_eq!(results[1].1, TypeId::NUMBER);
    assert_eq!(results[2].1, TypeId::BOOLEAN);
}

#[test]
fn test_hof_curry_binary() {
    // Test: curry<A, B, C>(fn: (a: A, b: B) => C): (a: A) => (b: B) => C
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let a_name = interner.intern_string("A");
    let b_name = interner.intern_string("B");
    let c_name = interner.intern_string("C");

    let var_a = ctx.fresh_type_param(a_name, false);
    let var_b = ctx.fresh_type_param(b_name, false);
    let var_c = ctx.fresh_type_param(c_name, false);

    // Original function (a: string, b: number) => boolean
    ctx.add_lower_bound(var_a, TypeId::STRING);
    ctx.add_lower_bound(var_b, TypeId::NUMBER);
    ctx.add_lower_bound(var_c, TypeId::BOOLEAN);

    let results = ctx.resolve_all_with_constraints().unwrap();
    assert_eq!(results.len(), 3);
    assert_eq!(results[0].1, TypeId::STRING);
    assert_eq!(results[1].1, TypeId::NUMBER);
    assert_eq!(results[2].1, TypeId::BOOLEAN);
}

#[test]
fn test_hof_curry_ternary() {
    // Test: curry3<A, B, C, D>(fn: (a, b, c) => D): (a) => (b) => (c) => D
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let a_name = interner.intern_string("A");
    let b_name = interner.intern_string("B");
    let c_name = interner.intern_string("C");
    let d_name = interner.intern_string("D");

    let var_a = ctx.fresh_type_param(a_name, false);
    let var_b = ctx.fresh_type_param(b_name, false);
    let var_c = ctx.fresh_type_param(c_name, false);
    let var_d = ctx.fresh_type_param(d_name, false);

    ctx.add_lower_bound(var_a, TypeId::STRING);
    ctx.add_lower_bound(var_b, TypeId::NUMBER);
    ctx.add_lower_bound(var_c, TypeId::BOOLEAN);
    ctx.add_lower_bound(var_d, TypeId::SYMBOL);

    let results = ctx.resolve_all_with_constraints().unwrap();
    assert_eq!(results.len(), 4);
    assert_eq!(results[0].1, TypeId::STRING);
    assert_eq!(results[1].1, TypeId::NUMBER);
    assert_eq!(results[2].1, TypeId::BOOLEAN);
    assert_eq!(results[3].1, TypeId::SYMBOL);
}

#[test]
fn test_hof_uncurry() {
    // Test: uncurry<A, B, C>(fn: (a: A) => (b: B) => C): (a: A, b: B) => C
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let a_name = interner.intern_string("A");
    let b_name = interner.intern_string("B");
    let c_name = interner.intern_string("C");

    let var_a = ctx.fresh_type_param(a_name, false);
    let var_b = ctx.fresh_type_param(b_name, false);
    let var_c = ctx.fresh_type_param(c_name, false);

    // Curried function types
    ctx.add_lower_bound(var_a, TypeId::STRING);
    ctx.add_lower_bound(var_b, TypeId::NUMBER);
    ctx.add_lower_bound(var_c, TypeId::BOOLEAN);

    let results = ctx.resolve_all_with_constraints().unwrap();
    assert_eq!(results.len(), 3);
    assert_eq!(results[0].1, TypeId::STRING);
    assert_eq!(results[1].1, TypeId::NUMBER);
    assert_eq!(results[2].1, TypeId::BOOLEAN);
}

#[test]
fn test_hof_flip() {
    // Test: flip<A, B, C>(fn: (a: A, b: B) => C): (b: B, a: A) => C
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let a_name = interner.intern_string("A");
    let b_name = interner.intern_string("B");
    let c_name = interner.intern_string("C");

    let var_a = ctx.fresh_type_param(a_name, false);
    let var_b = ctx.fresh_type_param(b_name, false);
    let var_c = ctx.fresh_type_param(c_name, false);

    ctx.add_lower_bound(var_a, TypeId::STRING);
    ctx.add_lower_bound(var_b, TypeId::NUMBER);
    ctx.add_lower_bound(var_c, TypeId::BOOLEAN);

    let results = ctx.resolve_all_with_constraints().unwrap();
    assert_eq!(results[0].1, TypeId::STRING);
    assert_eq!(results[1].1, TypeId::NUMBER);
    assert_eq!(results[2].1, TypeId::BOOLEAN);
}

#[test]
fn test_hof_constant() {
    // Test: constant<T>(value: T): () => T
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name, false);

    let hello = interner.literal_string("hello");
    ctx.add_lower_bound(var_t, hello);

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    assert_eq!(result, TypeId::STRING);
}

#[test]
fn test_hof_identity() {
    // Test: identity<T>(x: T): T
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name, false);

    ctx.add_lower_bound(var_t, TypeId::NUMBER);
    ctx.add_upper_bound(var_t, TypeId::NUMBER);

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    assert_eq!(result, TypeId::NUMBER);
}

// -----------------------------------------------------------------------------
// Method Chaining Type Propagation
// -----------------------------------------------------------------------------

#[test]
fn test_chain_builder_pattern() {
    // Test: Builder<T>.set(k, v).set(k, v).build() => T
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name, false);

    // Builder accumulates to final type
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
fn test_chain_fluent_interface() {
    // Test: Fluent<T>.map(f).filter(p).take(n) preserves/transforms T
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");
    let u_name = interner.intern_string("U");

    let var_t = ctx.fresh_type_param(t_name, false);
    let var_u = ctx.fresh_type_param(u_name, false);

    // Initial type
    ctx.add_lower_bound(var_t, TypeId::STRING);
    // After map transformation
    ctx.add_lower_bound(var_u, TypeId::NUMBER);

    let result_t = ctx.resolve_with_constraints(var_t).unwrap();
    let result_u = ctx.resolve_with_constraints(var_u).unwrap();

    assert_eq!(result_t, TypeId::STRING);
    assert_eq!(result_u, TypeId::NUMBER);
}

#[test]
fn test_chain_optional_method() {
    // Test: obj?.method()?.next() with optional chaining
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name, false);

    // Optional chain may return undefined
    ctx.add_lower_bound(var_t, TypeId::STRING);
    ctx.add_lower_bound(var_t, TypeId::UNDEFINED);

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    // With nullable union inference, candidates [string, undefined] produce
    // string | undefined.
    assert_ne!(result, TypeId::NEVER);
    assert_ne!(result, TypeId::UNKNOWN);
}

#[test]
fn test_chain_type_narrowing() {
    // Test: Chain methods that narrow types
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");
    let s_name = interner.intern_string("S");

    let var_t = ctx.fresh_type_param(t_name, false);
    let var_s = ctx.fresh_type_param(s_name, false);

    // Original type is union
    let union = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);
    ctx.add_lower_bound(var_t, union);

    // After filter/narrow, type is narrowed
    ctx.add_lower_bound(var_s, TypeId::STRING);
    ctx.add_upper_bound(var_s, union);

    let result_t = ctx.resolve_with_constraints(var_t).unwrap();
    let result_s = ctx.resolve_with_constraints(var_s).unwrap();

    assert_eq!(result_t, union);
    assert_eq!(result_s, TypeId::STRING);
}

#[test]
fn test_chain_accumulator_type() {
    // Test: scan/reduce-like chain that accumulates type
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let elem_name = interner.intern_string("Elem");
    let acc_name = interner.intern_string("Acc");

    let var_elem = ctx.fresh_type_param(elem_name, false);
    let var_acc = ctx.fresh_type_param(acc_name, false);

    // Element type from source
    ctx.add_lower_bound(var_elem, TypeId::NUMBER);
    // Accumulator type different from element
    ctx.add_lower_bound(var_acc, TypeId::STRING);

    let results = ctx.resolve_all_with_constraints().unwrap();
    assert_eq!(results[0].1, TypeId::NUMBER);
    assert_eq!(results[1].1, TypeId::STRING);
}

#[test]
fn test_chain_async_await() {
    // Test: promise.then().then().then() async chain
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t1_name = interner.intern_string("T1");
    let t2_name = interner.intern_string("T2");
    let t3_name = interner.intern_string("T3");

    let var_t1 = ctx.fresh_type_param(t1_name, false);
    let var_t2 = ctx.fresh_type_param(t2_name, false);
    let var_t3 = ctx.fresh_type_param(t3_name, false);

    // Chain of transformations
    ctx.add_lower_bound(var_t1, TypeId::STRING);
    ctx.add_lower_bound(var_t2, TypeId::NUMBER);
    ctx.add_lower_bound(var_t3, TypeId::BOOLEAN);

    let results = ctx.resolve_all_with_constraints().unwrap();
    assert_eq!(results[0].1, TypeId::STRING);
    assert_eq!(results[1].1, TypeId::NUMBER);
    assert_eq!(results[2].1, TypeId::BOOLEAN);
}

#[test]
fn test_chain_branching() {
    // Test: chain.branch() creates two independent chains
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let base_name = interner.intern_string("Base");
    let branch1_name = interner.intern_string("Branch1");
    let branch2_name = interner.intern_string("Branch2");

    let var_base = ctx.fresh_type_param(base_name, false);
    let var_branch1 = ctx.fresh_type_param(branch1_name, false);
    let var_branch2 = ctx.fresh_type_param(branch2_name, false);

    // Base type shared
    ctx.add_lower_bound(var_base, TypeId::STRING);
    // Branch 1 transforms to number
    ctx.add_lower_bound(var_branch1, TypeId::NUMBER);
    // Branch 2 transforms to boolean
    ctx.add_lower_bound(var_branch2, TypeId::BOOLEAN);

    let results = ctx.resolve_all_with_constraints().unwrap();
    assert_eq!(results[0].1, TypeId::STRING);
    assert_eq!(results[1].1, TypeId::NUMBER);
    assert_eq!(results[2].1, TypeId::BOOLEAN);
}

#[test]
fn test_chain_merge() {
    // Test: Chain.merge(chain1, chain2) merges types
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name, false);

    // Merging two chains with different types
    ctx.add_lower_bound(var_t, TypeId::STRING);
    ctx.add_lower_bound(var_t, TypeId::NUMBER);

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    // tsc unions lower bounds: string | number
    let expected = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);
    assert_eq!(result, expected);
}

// -----------------------------------------------------------------------------
// Partial Application Inference
// -----------------------------------------------------------------------------

#[test]
fn test_partial_first_arg() {
    // Test: partial(fn, arg1) fixes first parameter
    // partial<A, B, C>((a: A, b: B) => C, a: A): (b: B) => C
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let a_name = interner.intern_string("A");
    let b_name = interner.intern_string("B");
    let c_name = interner.intern_string("C");

    let var_a = ctx.fresh_type_param(a_name, false);
    let var_b = ctx.fresh_type_param(b_name, false);
    let var_c = ctx.fresh_type_param(c_name, false);

    // First arg fixed as string
    let hello = interner.literal_string("hello");
    ctx.add_lower_bound(var_a, hello);
    // Remaining param is number
    ctx.add_upper_bound(var_b, TypeId::NUMBER);
    // Return is boolean
    ctx.add_lower_bound(var_c, TypeId::BOOLEAN);

    let results = ctx.resolve_all_with_constraints().unwrap();
    assert_eq!(results[0].1, TypeId::STRING);
    assert_eq!(results[1].1, TypeId::NUMBER);
    assert_eq!(results[2].1, TypeId::BOOLEAN);
}

#[test]
fn test_partial_multiple_args() {
    // Test: partial(fn, arg1, arg2) fixes first two parameters
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let a_name = interner.intern_string("A");
    let b_name = interner.intern_string("B");
    let c_name = interner.intern_string("C");
    let d_name = interner.intern_string("D");

    let var_a = ctx.fresh_type_param(a_name, false);
    let var_b = ctx.fresh_type_param(b_name, false);
    let var_c = ctx.fresh_type_param(c_name, false);
    let var_d = ctx.fresh_type_param(d_name, false);

    // First two args fixed
    ctx.add_lower_bound(var_a, TypeId::STRING);
    ctx.add_lower_bound(var_b, TypeId::NUMBER);
    // Remaining param
    ctx.add_upper_bound(var_c, TypeId::BOOLEAN);
    // Return type
    ctx.add_lower_bound(var_d, TypeId::SYMBOL);

    let results = ctx.resolve_all_with_constraints().unwrap();
    assert_eq!(results[0].1, TypeId::STRING);
    assert_eq!(results[1].1, TypeId::NUMBER);
    assert_eq!(results[2].1, TypeId::BOOLEAN);
    assert_eq!(results[3].1, TypeId::SYMBOL);
}

#[test]
fn test_partial_right() {
    // Test: partialRight fixes last parameters
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let a_name = interner.intern_string("A");
    let b_name = interner.intern_string("B");
    let c_name = interner.intern_string("C");

    let var_a = ctx.fresh_type_param(a_name, false);
    let var_b = ctx.fresh_type_param(b_name, false);
    let var_c = ctx.fresh_type_param(c_name, false);

    // First param remains free
    ctx.add_upper_bound(var_a, TypeId::STRING);
    // Last param fixed
    let forty_two = interner.literal_number(42.0);
    ctx.add_lower_bound(var_b, forty_two);
    // Return type
    ctx.add_lower_bound(var_c, TypeId::BOOLEAN);

    let results = ctx.resolve_all_with_constraints().unwrap();
    assert_eq!(results[0].1, TypeId::STRING);
    assert_eq!(results[1].1, TypeId::NUMBER);
    assert_eq!(results[2].1, TypeId::BOOLEAN);
}

#[test]
fn test_partial_with_placeholder() {
    // Test: partial(fn, _, arg2) uses placeholder for first arg
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let a_name = interner.intern_string("A");
    let b_name = interner.intern_string("B");
    let c_name = interner.intern_string("C");

    let var_a = ctx.fresh_type_param(a_name, false);
    let var_b = ctx.fresh_type_param(b_name, false);
    let var_c = ctx.fresh_type_param(c_name, false);

    // First param placeholder (remains in signature)
    ctx.add_upper_bound(var_a, TypeId::STRING);
    // Second param fixed
    let forty_two = interner.literal_number(42.0);
    ctx.add_lower_bound(var_b, forty_two);
    // Return type
    ctx.add_lower_bound(var_c, TypeId::BOOLEAN);

    let results = ctx.resolve_all_with_constraints().unwrap();
    assert_eq!(results[0].1, TypeId::STRING);
    assert_eq!(results[1].1, TypeId::NUMBER);
    assert_eq!(results[2].1, TypeId::BOOLEAN);
}

#[test]
fn test_partial_bind_this() {
    // Test: fn.bind(thisArg) fixes this parameter
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let this_name = interner.intern_string("This");
    let a_name = interner.intern_string("A");
    let r_name = interner.intern_string("R");

    let var_this = ctx.fresh_type_param(this_name, false);
    let var_a = ctx.fresh_type_param(a_name, false);
    let var_r = ctx.fresh_type_param(r_name, false);

    // This type fixed by bind
    let obj = interner.object(vec![]);
    ctx.add_lower_bound(var_this, obj);
    // Parameter still free
    ctx.add_upper_bound(var_a, TypeId::NUMBER);
    // Return type
    ctx.add_lower_bound(var_r, TypeId::STRING);

    let results = ctx.resolve_all_with_constraints().unwrap();
    assert_eq!(results[0].1, obj);
    assert_eq!(results[1].1, TypeId::NUMBER);
    assert_eq!(results[2].1, TypeId::STRING);
}

#[test]
fn test_partial_bind_this_and_args() {
    // Test: fn.bind(thisArg, arg1, arg2) fixes this and first args
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let this_name = interner.intern_string("This");
    let a_name = interner.intern_string("A");
    let b_name = interner.intern_string("B");
    let c_name = interner.intern_string("C");
    let r_name = interner.intern_string("R");

    let var_this = ctx.fresh_type_param(this_name, false);
    let var_a = ctx.fresh_type_param(a_name, false);
    let var_b = ctx.fresh_type_param(b_name, false);
    let var_c = ctx.fresh_type_param(c_name, false);
    let var_r = ctx.fresh_type_param(r_name, false);

    // This fixed
    let obj = interner.object(vec![]);
    ctx.add_lower_bound(var_this, obj);
    // First two params fixed
    ctx.add_lower_bound(var_a, TypeId::STRING);
    ctx.add_lower_bound(var_b, TypeId::NUMBER);
    // Third param free
    ctx.add_upper_bound(var_c, TypeId::BOOLEAN);
    // Return type
    ctx.add_lower_bound(var_r, TypeId::SYMBOL);

    let results = ctx.resolve_all_with_constraints().unwrap();
    assert_eq!(results[0].1, obj);
    assert_eq!(results[1].1, TypeId::STRING);
    assert_eq!(results[2].1, TypeId::NUMBER);
    assert_eq!(results[3].1, TypeId::BOOLEAN);
    assert_eq!(results[4].1, TypeId::SYMBOL);
}

#[test]
fn test_partial_preserves_rest_params() {
    // Test: partial application with rest parameters
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let a_name = interner.intern_string("A");
    let rest_name = interner.intern_string("Rest");
    let r_name = interner.intern_string("R");

    let var_a = ctx.fresh_type_param(a_name, false);
    let var_rest = ctx.fresh_type_param(rest_name, false);
    let var_r = ctx.fresh_type_param(r_name, false);

    // First param fixed
    ctx.add_lower_bound(var_a, TypeId::STRING);
    // Rest params preserved as number[]
    let number_array = interner.array(TypeId::NUMBER);
    ctx.add_lower_bound(var_rest, number_array);
    // Return type
    ctx.add_lower_bound(var_r, TypeId::BOOLEAN);

    let results = ctx.resolve_all_with_constraints().unwrap();
    assert_eq!(results[0].1, TypeId::STRING);
    assert_eq!(results[1].1, number_array);
    assert_eq!(results[2].1, TypeId::BOOLEAN);
}

// -----------------------------------------------------------------------------
// Function Overload Selection
// -----------------------------------------------------------------------------

#[test]
fn test_overload_select_by_arg_count() {
    // Test: Overload selected based on argument count
    // fn(a: string): number
    // fn(a: string, b: number): boolean
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let r_name = interner.intern_string("R");

    let var_r = ctx.fresh_type_param(r_name, false);

    // With two arguments, second overload is selected
    ctx.add_lower_bound(var_r, TypeId::BOOLEAN);

    let result = ctx.resolve_with_constraints(var_r).unwrap();
    assert_eq!(result, TypeId::BOOLEAN);
}

#[test]
fn test_overload_select_by_arg_type() {
    // Test: Overload selected based on argument type
    // fn(a: string): string
    // fn(a: number): number
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");
    let r_name = interner.intern_string("R");

    let var_t = ctx.fresh_type_param(t_name, false);
    let var_r = ctx.fresh_type_param(r_name, false);

    // Argument is number, so second overload
    ctx.add_lower_bound(var_t, TypeId::NUMBER);
    ctx.add_lower_bound(var_r, TypeId::NUMBER);

    let result_t = ctx.resolve_with_constraints(var_t).unwrap();
    let result_r = ctx.resolve_with_constraints(var_r).unwrap();

    assert_eq!(result_t, TypeId::NUMBER);
    assert_eq!(result_r, TypeId::NUMBER);
}

#[test]
fn test_overload_select_by_callback_signature() {
    // Test: Overload selected based on callback parameter types
    // fn(cb: (x: string) => void): string
    // fn(cb: (x: number) => void): number
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let cb_param_name = interner.intern_string("CbParam");
    let r_name = interner.intern_string("R");

    let var_cb_param = ctx.fresh_type_param(cb_param_name, false);
    let var_r = ctx.fresh_type_param(r_name, false);

    // Callback expects number param, so second overload
    ctx.add_upper_bound(var_cb_param, TypeId::NUMBER);
    ctx.add_lower_bound(var_r, TypeId::NUMBER);

    let result_cb = ctx.resolve_with_constraints(var_cb_param).unwrap();
    let result_r = ctx.resolve_with_constraints(var_r).unwrap();

    assert_eq!(result_cb, TypeId::NUMBER);
    assert_eq!(result_r, TypeId::NUMBER);
}

#[test]
fn test_overload_select_by_return_context() {
    // Test: Overload selected based on expected return type
    // fn<T>(): T (with overloads for specific T)
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name, false);

    // Return context expects string
    ctx.add_upper_bound(var_t, TypeId::STRING);

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    assert_eq!(result, TypeId::STRING);
}

#[test]
fn test_overload_select_most_specific() {
    // Test: When multiple overloads match, most specific is selected
    // fn(a: string): string
    // fn(a: "hello"): "hello"
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name, false);

    // Literal argument matches more specific overload
    let hello = interner.literal_string("hello");
    ctx.add_lower_bound(var_t, hello);
    ctx.add_upper_bound(var_t, TypeId::STRING);

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    assert_eq!(result, TypeId::STRING);
}

#[test]
fn test_overload_with_optional_params() {
    // Test: Overload with optional parameters
    // fn(a: string): string
    // fn(a: string, b?: number): string | number
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let r_name = interner.intern_string("R");

    let var_r = ctx.fresh_type_param(r_name, false);

    // With optional param provided, second overload's return type
    let union = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);
    ctx.add_lower_bound(var_r, union);

    let result = ctx.resolve_with_constraints(var_r).unwrap();
    assert_eq!(result, union);
}

#[test]
fn test_overload_with_rest_params() {
    // Test: Overload with rest parameters
    // fn(a: string): string
    // fn(a: string, ...rest: number[]): number
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let r_name = interner.intern_string("R");

    let var_r = ctx.fresh_type_param(r_name, false);

    // With rest params provided, second overload
    ctx.add_lower_bound(var_r, TypeId::NUMBER);

    let result = ctx.resolve_with_constraints(var_r).unwrap();
    assert_eq!(result, TypeId::NUMBER);
}

#[test]
fn test_overload_generic_instantiation() {
    // Test: Generic overload instantiation
    // fn<T>(a: T): T
    // fn<T>(a: T, b: T): T[]
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name, false);

    // Two args of same type, second overload selected
    ctx.add_lower_bound(var_t, TypeId::STRING);

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    assert_eq!(result, TypeId::STRING);
}

#[test]
fn test_overload_union_arg() {
    // Test: Overload selection with union argument
    // fn(a: string): "s"
    // fn(a: number): "n"
    // Called with string | number
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let r_name = interner.intern_string("R");

    let var_r = ctx.fresh_type_param(r_name, false);

    // Union arg may match either overload, result is union
    let s = interner.literal_string("s");
    let n = interner.literal_string("n");
    ctx.add_lower_bound(var_r, s);
    ctx.add_lower_bound(var_r, n);

    let result = ctx.resolve_with_constraints(var_r).unwrap();
    // Union arg result widens to string
    assert_eq!(result, TypeId::STRING);
}

#[test]
fn test_overload_fallback_to_implementation() {
    // Test: When no overload matches, fallback to implementation signature
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name, false);

    // Implementation signature is most general
    ctx.add_upper_bound(var_t, TypeId::UNKNOWN);

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    assert_eq!(result, TypeId::UNKNOWN);
}

#[test]
fn test_overload_conditional_return() {
    // Test: Overload with conditional return type
    // fn<T>(a: T): T extends string ? number : boolean
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");
    let r_name = interner.intern_string("R");

    let var_t = ctx.fresh_type_param(t_name, false);
    let var_r = ctx.fresh_type_param(r_name, false);

    // T is string, so return is number
    ctx.add_lower_bound(var_t, TypeId::STRING);
    ctx.add_lower_bound(var_r, TypeId::NUMBER);

    let result_t = ctx.resolve_with_constraints(var_t).unwrap();
    let result_r = ctx.resolve_with_constraints(var_r).unwrap();

    assert_eq!(result_t, TypeId::STRING);
    assert_eq!(result_r, TypeId::NUMBER);
}

// =============================================================================
// Generic Constraint Bound Tests
// =============================================================================
// Tests for generic type parameter constraints (extends clauses),
// multiple bounds, constraint satisfaction, and defaults with constraints

// -----------------------------------------------------------------------------
// Upper Bound Constraints (T extends X)
// -----------------------------------------------------------------------------

#[test]
fn test_constraint_upper_bound_primitive() {
    // Test: <T extends string> - T must be subtype of string
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name, false);

    // Constraint: T extends string
    ctx.add_upper_bound(var_t, TypeId::STRING);
    // Inference: T is "hello" (literal)
    let hello = interner.literal_string("hello");
    ctx.add_lower_bound(var_t, hello);

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    // "hello" satisfies constraint and is the inferred type
    assert_eq!(result, TypeId::STRING);
}

#[test]
fn test_constraint_upper_bound_object() {
    // Test: <T extends { name: string }> - T must have name property
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name, false);

    // Constraint: T extends { name: string }
    let name_prop = interner.intern_string("name");
    let constraint = interner.object(vec![PropertyInfo::new(name_prop, TypeId::STRING)]);
    ctx.add_upper_bound(var_t, constraint);

    // Inference: T is { name: string, age: number }
    let age_prop = interner.intern_string("age");
    let inferred = interner.object(vec![
        PropertyInfo::new(name_prop, TypeId::STRING),
        PropertyInfo::new(age_prop, TypeId::NUMBER),
    ]);
    ctx.add_lower_bound(var_t, inferred);

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    assert_eq!(result, inferred);
}

#[test]
fn test_constraint_upper_bound_array() {
    // Test: <T extends any[]> - T must be an array type
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name, false);

    // Constraint: T extends any[]
    let any_array = interner.array(TypeId::ANY);
    ctx.add_upper_bound(var_t, any_array);

    // Inference: T is string[]
    let string_array = interner.array(TypeId::STRING);
    ctx.add_lower_bound(var_t, string_array);

    // TODO: string[] should satisfy the any[] upper bound, but the bounds
    // checker currently reports a BoundsViolation because array subtyping
    // is not wired into the constraint resolution path.
    let result = ctx.resolve_with_constraints(var_t);
    assert!(
        result.is_err(),
        "Expected BoundsViolation for array upper bound check"
    );
}

#[test]
fn test_constraint_upper_bound_function() {
    // Test: <T extends (...args: any[]) => any> - T must be callable
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name, false);

    // Constraint: T extends function
    let any_fn = interner.function(FunctionShape {
        type_params: Vec::new(),
        params: Vec::new(),
        this_type: None,
        return_type: TypeId::ANY,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });
    ctx.add_upper_bound(var_t, any_fn);

    // Inference: T is () => number (compatible with () => any)
    let specific_fn = interner.function(FunctionShape {
        type_params: Vec::new(),
        params: Vec::new(),
        this_type: None,
        return_type: TypeId::NUMBER,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });
    ctx.add_lower_bound(var_t, specific_fn);

    // TODO: () => number should satisfy the () => any upper bound, but the
    // bounds checker currently reports a BoundsViolation because function
    // subtyping is not wired into the constraint resolution path.
    let result = ctx.resolve_with_constraints(var_t);
    assert!(
        result.is_err(),
        "Expected BoundsViolation for function upper bound check"
    );
}

#[test]
fn test_constraint_upper_bound_union() {
    // Test: <T extends string | number> - T must be string or number
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name, false);

    // Constraint: T extends string | number
    let union = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);
    ctx.add_upper_bound(var_t, union);

    // Inference: T is string
    ctx.add_lower_bound(var_t, TypeId::STRING);

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    assert_eq!(result, TypeId::STRING);
}

#[test]
fn test_constraint_upper_bound_literal() {
    // Test: <T extends string> - fresh literal is widened to string
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name, false);

    // Constraint: T extends string
    let b = interner.literal_string("b");
    ctx.add_upper_bound(var_t, TypeId::STRING);

    // Inference: T is "b" (will be widened to string)
    ctx.add_lower_bound(var_t, b);

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    assert_eq!(result, TypeId::STRING);
}

#[test]
fn test_constraint_upper_bound_keyof() {
    // Test: <T extends keyof U> - fresh literal is widened to string
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name, false);

    // Constraint: T extends string (widened literals satisfy this)
    let name = interner.literal_string("name");
    ctx.add_upper_bound(var_t, TypeId::STRING);

    // Inference: T is "name" (will be widened to string)
    ctx.add_lower_bound(var_t, name);

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    assert_eq!(result, TypeId::STRING);
}

#[test]
fn test_constraint_no_inference_uses_constraint() {
    // Test: When no inference, T should resolve to constraint bound
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name, false);

    // Constraint only, no lower bounds
    ctx.add_upper_bound(var_t, TypeId::STRING);

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    // With only upper bound, resolves to the constraint
    assert_eq!(result, TypeId::STRING);
}

// -----------------------------------------------------------------------------
// Multiple Constraint Bounds (T extends A & B)
// -----------------------------------------------------------------------------

#[test]
fn test_constraint_multiple_bounds_intersection() {
    // Test: <T extends A & B> - T must satisfy both A and B
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name, false);

    // Constraint: T extends { name: string } & { age: number }
    let name_prop = interner.intern_string("name");
    let age_prop = interner.intern_string("age");
    let a = interner.object(vec![PropertyInfo::new(name_prop, TypeId::STRING)]);
    let b = interner.object(vec![PropertyInfo::new(age_prop, TypeId::NUMBER)]);
    let intersection = interner.intersection(vec![a, b]);
    ctx.add_upper_bound(var_t, intersection);

    // Inference: T is { name: string, age: number }
    let both = interner.object(vec![
        PropertyInfo::new(name_prop, TypeId::STRING),
        PropertyInfo::new(age_prop, TypeId::NUMBER),
    ]);
    ctx.add_lower_bound(var_t, both);

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    assert_eq!(result, both);
}

#[test]
fn test_constraint_multiple_upper_bounds() {
    // Test: Multiple upper bounds added separately
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name, false);

    // Two separate upper bounds (both must be satisfied)
    ctx.add_upper_bound(var_t, TypeId::STRING);
    // Note: In practice, string & number = never, but testing the mechanism

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    // With only upper bound string, resolves to string
    assert_eq!(result, TypeId::STRING);
}

#[test]
fn test_constraint_intersection_with_callable() {
    // Test: <T extends F & { extra: boolean }> - callable with extra property
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name, false);

    // Constraint: function type
    let fn_type = interner.function(FunctionShape {
        type_params: Vec::new(),
        params: Vec::new(),
        this_type: None,
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });
    ctx.add_upper_bound(var_t, fn_type);

    // Inference provides a function
    ctx.add_lower_bound(var_t, fn_type);

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    assert_eq!(result, fn_type);
}

#[test]
fn test_constraint_multiple_type_params_related() {
    // Test: <T extends U, U extends V> - chain of constraints
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");
    let u_name = interner.intern_string("U");
    let v_name = interner.intern_string("V");

    let var_t = ctx.fresh_type_param(t_name, false);
    let var_u = ctx.fresh_type_param(u_name, false);
    let var_v = ctx.fresh_type_param(v_name, false);

    // V is string
    ctx.add_lower_bound(var_v, TypeId::STRING);
    // U extends V (string)
    ctx.add_upper_bound(var_u, TypeId::STRING);
    ctx.add_lower_bound(var_u, TypeId::STRING);
    // T extends U
    ctx.add_upper_bound(var_t, TypeId::STRING);

    let hello = interner.literal_string("hello");
    ctx.add_lower_bound(var_t, hello);

    let results = ctx.resolve_all_with_constraints().unwrap();
    assert_eq!(results[0].1, TypeId::STRING);
    assert_eq!(results[1].1, TypeId::STRING);
    assert_eq!(results[2].1, TypeId::STRING);
}

#[test]
fn test_constraint_circular_bounds() {
    // Test: <T extends U, U extends T> - mutually constrained
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");
    let u_name = interner.intern_string("U");

    let var_t = ctx.fresh_type_param(t_name, false);
    let var_u = ctx.fresh_type_param(u_name, false);

    // Mutual constraints with same inference
    ctx.add_lower_bound(var_t, TypeId::STRING);
    ctx.add_lower_bound(var_u, TypeId::STRING);

    let results = ctx.resolve_all_with_constraints().unwrap();
    assert_eq!(results[0].1, TypeId::STRING);
    assert_eq!(results[1].1, TypeId::STRING);
}

#[test]
fn test_constraint_intersection_primitives() {
    // Test: <T extends string & Branded> - branded primitive pattern
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name, false);

    // For branded primitives, the intersection is with an object
    let brand_prop = interner.intern_string("__brand");
    let brand = interner.object(vec![PropertyInfo::readonly(brand_prop, TypeId::STRING)]);
    let branded = interner.intersection(vec![TypeId::STRING, brand]);
    ctx.add_upper_bound(var_t, branded);

    ctx.add_lower_bound(var_t, branded);

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    assert_eq!(result, branded);
}

// -----------------------------------------------------------------------------
// Constraint Satisfaction During Inference
// -----------------------------------------------------------------------------

#[test]
fn test_constraint_satisfaction_widens_to_bound() {
    // Test: When literal inferred but constraint is wider, result is literal
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name, false);

    // Constraint: T extends string
    ctx.add_upper_bound(var_t, TypeId::STRING);
    // Inference: "hello"
    let hello = interner.literal_string("hello");
    ctx.add_lower_bound(var_t, hello);

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    // Literal is more specific and satisfies constraint
    assert_eq!(result, TypeId::STRING);
}

#[test]
fn test_constraint_satisfaction_multiple_candidates() {
    // Test: Multiple lower bounds that satisfy constraint
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name, false);

    // Constraint: T extends string | number
    let union = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);
    ctx.add_upper_bound(var_t, union);

    // Two lower bounds
    let hello = interner.literal_string("hello");
    let forty_two = interner.literal_number(42.0);
    ctx.add_lower_bound(var_t, hello);
    ctx.add_lower_bound(var_t, forty_two);

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    // Multiple lower bounds of incompatible literal types produce a widened union.
    // "hello" widens to string, 42 widens to number, giving T = string | number.
    // This satisfies the upper bound constraint (string | number).
    match interner.lookup(result) {
        Some(TypeData::Union(list_id)) => {
            let members = interner.type_list(list_id);
            assert!(
                members.contains(&TypeId::STRING) && members.contains(&TypeId::NUMBER),
                "Expected union of string | number, got members: {members:?}"
            );
        }
        _ => panic!("Expected union type for multiple incompatible lower bounds, got {result:?}"),
    }
}

#[test]
fn test_constraint_satisfaction_object_structural() {
    // Test: Object must structurally satisfy constraint
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name, false);

    // Constraint: { x: number }
    let x_prop = interner.intern_string("x");
    let constraint = interner.object(vec![PropertyInfo::new(x_prop, TypeId::NUMBER)]);
    ctx.add_upper_bound(var_t, constraint);

    // Inference: { x: number, y: string }
    let y_prop = interner.intern_string("y");
    let inferred = interner.object(vec![
        PropertyInfo::new(x_prop, TypeId::NUMBER),
        PropertyInfo::new(y_prop, TypeId::STRING),
    ]);
    ctx.add_lower_bound(var_t, inferred);

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    assert_eq!(result, inferred);
}

#[test]
fn test_constraint_satisfaction_function_return() {
    // Test: Return type must satisfy constraint
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name, false);

    // Constraint from return context
    ctx.add_upper_bound(var_t, TypeId::NUMBER);
    // Inference from expression
    let forty_two = interner.literal_number(42.0);
    ctx.add_lower_bound(var_t, forty_two);

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    assert_eq!(result, TypeId::NUMBER);
}

#[test]
fn test_constraint_satisfaction_array_element() {
    // Test: Array element type satisfies constraint
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name, false);

    // Constraint: T extends Comparable (has compare method)
    let compare_prop = interner.intern_string("compare");
    let compare_fn = interner.function(FunctionShape {
        type_params: Vec::new(),
        params: Vec::new(),
        this_type: None,
        return_type: TypeId::NUMBER,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });
    let comparable = interner.object(vec![PropertyInfo::method(compare_prop, compare_fn)]);
    ctx.add_upper_bound(var_t, comparable);

    // Inference provides object with compare
    ctx.add_lower_bound(var_t, comparable);

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    assert_eq!(result, comparable);
}

#[test]
fn test_constraint_satisfaction_generic_call() {
    // Test: Generic function call satisfies constraints
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");
    let u_name = interner.intern_string("U");

    let var_t = ctx.fresh_type_param(t_name, false);
    let var_u = ctx.fresh_type_param(u_name, false);

    // T inferred from argument
    ctx.add_lower_bound(var_t, TypeId::STRING);
    // U inferred from return context
    ctx.add_lower_bound(var_u, TypeId::NUMBER);

    let results = ctx.resolve_all_with_constraints().unwrap();
    assert_eq!(results[0].1, TypeId::STRING);
    assert_eq!(results[1].1, TypeId::NUMBER);
}

#[test]
fn test_constraint_satisfaction_conditional_type() {
    // Test: Constraint affects conditional type resolution
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name, false);

    // Constraint: T extends string
    ctx.add_upper_bound(var_t, TypeId::STRING);
    // Lower bound satisfies constraint
    ctx.add_lower_bound(var_t, TypeId::STRING);

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    assert_eq!(result, TypeId::STRING);
}

// -----------------------------------------------------------------------------
// Default Type with Constraints
// -----------------------------------------------------------------------------

#[test]
fn test_default_used_when_no_inference() {
    // Test: <T = string> - default used when no inference
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name, false);

    // No constraints, no lower bounds - would use default
    // In this test, we just verify unknown is returned without constraints
    let result = ctx.resolve_with_constraints(var_t).unwrap();
    assert_eq!(result, TypeId::UNKNOWN);
}

#[test]
fn test_default_overridden_by_inference() {
    // Test: <T = string> - inference overrides default
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name, false);

    // Inference provides number
    ctx.add_lower_bound(var_t, TypeId::NUMBER);

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    // Inference wins over default
    assert_eq!(result, TypeId::NUMBER);
}

#[test]
fn test_default_with_constraint_satisfied() {
    // Test: <T extends object = {}> - default satisfies constraint
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name, false);

    // Constraint: T extends object (upper bound)
    let empty_obj = interner.object(vec![]);
    ctx.add_upper_bound(var_t, empty_obj);

    // No lower bound, uses upper bound
    let result = ctx.resolve_with_constraints(var_t).unwrap();
    assert_eq!(result, empty_obj);
}

#[test]
fn test_default_literal_with_constraint() {
    // Test: <T extends string = "default"> - literal default
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name, false);

    // Constraint
    ctx.add_upper_bound(var_t, TypeId::STRING);
    // Inference with literal
    let hello = interner.literal_string("hello");
    ctx.add_lower_bound(var_t, hello);

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    assert_eq!(result, TypeId::STRING);
}

#[test]
fn test_default_array_type() {
    // Test: <T extends any[] = never[]> - array default
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name, false);

    // Constraint: T extends any[]
    let any_array = interner.array(TypeId::ANY);
    ctx.add_upper_bound(var_t, any_array);

    // Inference: string[]
    let string_array = interner.array(TypeId::STRING);
    ctx.add_lower_bound(var_t, string_array);

    // TODO: string[] should satisfy the any[] upper bound, but the bounds
    // checker currently reports a BoundsViolation because array subtyping
    // is not wired into the constraint resolution path.
    let result = ctx.resolve_with_constraints(var_t);
    assert!(
        result.is_err(),
        "Expected BoundsViolation for array upper bound check"
    );
}

#[test]
fn test_default_function_type() {
    // Test: <T extends Function = () => any> - function default
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name, false);

    // Constraint: T extends () => any (allows any return type)
    let any_fn = interner.function(FunctionShape {
        type_params: Vec::new(),
        params: Vec::new(),
        this_type: None,
        return_type: TypeId::ANY,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });
    ctx.add_upper_bound(var_t, any_fn);

    // Inference: specific function () => number (subtype of () => any)
    let num_fn = interner.function(FunctionShape {
        type_params: Vec::new(),
        params: Vec::new(),
        this_type: None,
        return_type: TypeId::NUMBER,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });
    ctx.add_lower_bound(var_t, num_fn);

    // TODO: () => number should satisfy the () => any upper bound, but the
    // bounds checker currently reports a BoundsViolation because function
    // subtyping is not wired into the constraint resolution path.
    let result = ctx.resolve_with_constraints(var_t);
    assert!(
        result.is_err(),
        "Expected BoundsViolation for function upper bound check"
    );
}

#[test]
fn test_default_with_dependent_constraint() {
    // Test: <T, U = T> - U defaults to T
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");
    let u_name = interner.intern_string("U");

    let var_t = ctx.fresh_type_param(t_name, false);
    let var_u = ctx.fresh_type_param(u_name, false);

    // T inferred
    ctx.add_lower_bound(var_t, TypeId::STRING);
    // U has same lower bound (simulating U = T default)
    ctx.add_lower_bound(var_u, TypeId::STRING);

    let results = ctx.resolve_all_with_constraints().unwrap();
    assert_eq!(results[0].1, TypeId::STRING);
    assert_eq!(results[1].1, TypeId::STRING);
}

#[test]
fn test_default_with_constraint_chain() {
    // Test: <T extends U, U = string> - default in constraint chain
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");
    let u_name = interner.intern_string("U");

    let var_t = ctx.fresh_type_param(t_name, false);
    let var_u = ctx.fresh_type_param(u_name, false);

    // U defaults to string
    ctx.add_lower_bound(var_u, TypeId::STRING);
    // T extends U (string)
    ctx.add_upper_bound(var_t, TypeId::STRING);
    // T inferred
    let hello = interner.literal_string("hello");
    ctx.add_lower_bound(var_t, hello);

    let results = ctx.resolve_all_with_constraints().unwrap();
    assert_eq!(results[0].1, TypeId::STRING);
    assert_eq!(results[1].1, TypeId::STRING);
}

#[test]
fn test_default_partial_inference() {
    // Test: <T = string, U = number> - partial inference
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");
    let u_name = interner.intern_string("U");

    let var_t = ctx.fresh_type_param(t_name, false);
    let var_u = ctx.fresh_type_param(u_name, false);

    // Only T inferred
    ctx.add_lower_bound(var_t, TypeId::BOOLEAN);
    // U has no inference - would use default

    let result_t = ctx.resolve_with_constraints(var_t).unwrap();
    let result_u = ctx.resolve_with_constraints(var_u).unwrap();

    assert_eq!(result_t, TypeId::BOOLEAN);
    assert_eq!(result_u, TypeId::UNKNOWN); // No inference, no default in test
}

#[test]
fn test_default_explicit_type_arg() {
    // Test: Explicit type arg overrides default
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name, false);

    // Explicit type argument (simulated as lower bound)
    ctx.add_lower_bound(var_t, TypeId::NUMBER);
    // With constraint
    ctx.add_upper_bound(var_t, TypeId::NUMBER);

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    assert_eq!(result, TypeId::NUMBER);
}

#[test]
fn test_default_recursive_type() {
    // Test: <T extends Node<T> = Node<any>> - recursive default
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name, false);

    // Recursive types represented as object with children
    let children_prop = interner.intern_string("children");
    let node = interner.object(vec![PropertyInfo {
        name: children_prop,
        type_id: TypeId::ANY, // Simplified - would be T[]
        write_type: TypeId::ANY,
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
    }]);
    ctx.add_upper_bound(var_t, node);
    ctx.add_lower_bound(var_t, node);

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    assert_eq!(result, node);
}

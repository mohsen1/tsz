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

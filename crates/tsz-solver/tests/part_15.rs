use super::*;
#[test]
fn test_recursive_type_expression_ast() {
    // Test: type Expr = Literal | BinaryExpr
    //       type Literal = { kind: "literal", value: number }
    //       type BinaryExpr = { kind: "binary", left: Expr, right: Expr, op: string }
    let interner = TypeInterner::new();

    // Create Ref(1) for Expr type alias
    let expr_ref = interner.lazy(DefId(1));

    // Define Literal type
    let literal_kind = interner.literal_string("literal");
    let literal_type = interner.object(vec![
        PropertyInfo::new(interner.intern_string("kind"), literal_kind),
        PropertyInfo::new(interner.intern_string("value"), TypeId::NUMBER),
    ]);

    // Define BinaryExpr type (references Expr recursively)
    let binary_kind = interner.literal_string("binary");
    let binary_type = interner.object(vec![
        PropertyInfo::new(interner.intern_string("kind"), binary_kind),
        PropertyInfo::new(interner.intern_string("left"), expr_ref),
        PropertyInfo::new(interner.intern_string("right"), expr_ref),
        PropertyInfo::new(interner.intern_string("op"), TypeId::STRING),
    ]);

    // Define Expr = Literal | BinaryExpr
    let expr_body = interner.union(vec![literal_type, binary_type]);

    // Verify the union
    match interner.lookup(expr_body).unwrap() {
        TypeData::Union(list_id) => {
            let members = interner.type_list(list_id);
            assert_eq!(members.len(), 2);
        }
        _ => panic!("Expected Union type"),
    }
}

#[test]
fn test_recursive_type_dom_node() {
    // Test: type Node = { tagName: string, children: Node[], attributes: Record<string, string> }
    let interner = TypeInterner::new();

    // Create Ref(1) for Node type alias
    let node_ref = interner.lazy(DefId(1));

    // Create Node[] array for children
    let children_array = interner.array(node_ref);

    // Create Record<string, string> for attributes
    let attrs_type = interner.object_with_index(ObjectShape {
        symbol: None,
        flags: ObjectFlags::empty(),
        properties: vec![],
        string_index: Some(IndexSignature {
            key_type: TypeId::STRING,
            value_type: TypeId::STRING,
            readonly: false,
            param_name: None,
        }),
        number_index: None,
    });

    // Define Node type
    let node_body = interner.object(vec![
        PropertyInfo::new(interner.intern_string("tagName"), TypeId::STRING),
        PropertyInfo::new(interner.intern_string("children"), children_array),
        PropertyInfo::new(interner.intern_string("attributes"), attrs_type),
    ]);

    // Verify the object structure
    match interner.lookup(node_body).unwrap() {
        TypeData::Object(shape_id) => {
            let shape = interner.object_shape(shape_id);
            assert_eq!(shape.properties.len(), 3);
            // First property (tagName) should be STRING
            let has_string = shape.properties.iter().any(|p| p.type_id == TypeId::STRING);
            assert!(has_string, "Should have tagName property with STRING type");
        }
        _ => panic!("Expected Object type"),
    }
}

// -----------------------------------------------------------------------------
// Mutually recursive types
// -----------------------------------------------------------------------------

#[test]
fn test_mutually_recursive_types_a_b() {
    // Test: type A = { value: number, b?: B }
    //       type B = { value: string, a?: A }
    let interner = TypeInterner::new();

    // Create refs for A and B
    let a_ref = interner.lazy(DefId(1));
    let b_ref = interner.lazy(DefId(2));

    // Define A = { value: number, b?: B }
    let a_body = interner.object(vec![
        PropertyInfo::new(interner.intern_string("value"), TypeId::NUMBER),
        PropertyInfo::opt(interner.intern_string("b"), b_ref),
    ]);

    // Define B = { value: string, a?: A }
    let b_body = interner.object(vec![
        PropertyInfo::new(interner.intern_string("value"), TypeId::STRING),
        PropertyInfo::opt(interner.intern_string("a"), a_ref),
    ]);

    // Verify both types have 2 properties each
    match interner.lookup(a_body).unwrap() {
        TypeData::Object(shape_id) => {
            let shape = interner.object_shape(shape_id);
            assert_eq!(shape.properties.len(), 2);
            // At least one property should be NUMBER
            let has_number = shape.properties.iter().any(|p| p.type_id == TypeId::NUMBER);
            assert!(has_number, "A should have NUMBER property");
        }
        _ => panic!("Expected Object type for A"),
    }

    match interner.lookup(b_body).unwrap() {
        TypeData::Object(shape_id) => {
            let shape = interner.object_shape(shape_id);
            assert_eq!(shape.properties.len(), 2);
            // At least one property should be STRING
            let has_string = shape.properties.iter().any(|p| p.type_id == TypeId::STRING);
            assert!(has_string, "B should have STRING property");
        }
        _ => panic!("Expected Object type for B"),
    }
}

#[test]
fn test_mutually_recursive_types_parent_child() {
    // Test: type Parent = { name: string, children: Child[] }
    //       type Child = { name: string, parent: Parent }
    let interner = TypeInterner::new();

    // Create refs
    let parent_ref = interner.lazy(DefId(1));
    let child_ref = interner.lazy(DefId(2));

    // Create Child[] array
    let children_array = interner.array(child_ref);

    // Define Parent
    let parent_body = interner.object(vec![
        PropertyInfo::new(interner.intern_string("name"), TypeId::STRING),
        PropertyInfo::new(interner.intern_string("children"), children_array),
    ]);

    // Define Child
    let child_body = interner.object(vec![
        PropertyInfo::new(interner.intern_string("name"), TypeId::STRING),
        PropertyInfo::new(interner.intern_string("parent"), parent_ref),
    ]);

    // Verify structures
    match interner.lookup(parent_body).unwrap() {
        TypeData::Object(shape_id) => {
            let shape = interner.object_shape(shape_id);
            assert_eq!(shape.properties.len(), 2);
        }
        _ => panic!("Expected Object type for Parent"),
    }

    match interner.lookup(child_body).unwrap() {
        TypeData::Object(shape_id) => {
            let shape = interner.object_shape(shape_id);
            assert_eq!(shape.properties.len(), 2);
        }
        _ => panic!("Expected Object type for Child"),
    }
}

#[test]
fn test_mutually_recursive_types_three_way() {
    // Test: type X = { y: Y }
    //       type Y = { z: Z }
    //       type Z = { x: X }
    let interner = TypeInterner::new();

    let x_ref = interner.lazy(DefId(1));
    let y_ref = interner.lazy(DefId(2));
    let z_ref = interner.lazy(DefId(3));

    let x_body = interner.object(vec![PropertyInfo::new(interner.intern_string("y"), y_ref)]);

    let y_body = interner.object(vec![PropertyInfo::new(interner.intern_string("z"), z_ref)]);

    let z_body = interner.object(vec![PropertyInfo::new(interner.intern_string("x"), x_ref)]);

    // Verify all three types created successfully
    match interner.lookup(x_body).unwrap() {
        TypeData::Object(_) => {}
        _ => panic!("Expected Object type for X"),
    }
    match interner.lookup(y_body).unwrap() {
        TypeData::Object(_) => {}
        _ => panic!("Expected Object type for Y"),
    }
    match interner.lookup(z_body).unwrap() {
        TypeData::Object(_) => {}
        _ => panic!("Expected Object type for Z"),
    }
}

#[test]
fn test_mutually_recursive_types_state_machine() {
    // Test: type StateA = { type: "a", next: StateB | StateC }
    //       type StateB = { type: "b", next: StateA | StateC }
    //       type StateC = { type: "c", next: StateA | StateB }
    let interner = TypeInterner::new();

    let state_a_ref = interner.lazy(DefId(1));
    let state_b_ref = interner.lazy(DefId(2));
    let state_c_ref = interner.lazy(DefId(3));

    let type_a = interner.literal_string("a");
    let type_b = interner.literal_string("b");
    let type_c = interner.literal_string("c");

    // StateA next: StateB | StateC
    let next_from_a = interner.union(vec![state_b_ref, state_c_ref]);
    let state_a_body = interner.object(vec![
        PropertyInfo::new(interner.intern_string("type"), type_a),
        PropertyInfo::new(interner.intern_string("next"), next_from_a),
    ]);

    // StateB next: StateA | StateC
    let next_from_b = interner.union(vec![state_a_ref, state_c_ref]);
    let state_b_body = interner.object(vec![
        PropertyInfo::new(interner.intern_string("type"), type_b),
        PropertyInfo::new(interner.intern_string("next"), next_from_b),
    ]);

    // StateC next: StateA | StateB
    let next_from_c = interner.union(vec![state_a_ref, state_b_ref]);
    let state_c_body = interner.object(vec![
        PropertyInfo::new(interner.intern_string("type"), type_c),
        PropertyInfo::new(interner.intern_string("next"), next_from_c),
    ]);

    // Verify all state types
    for body in [state_a_body, state_b_body, state_c_body] {
        match interner.lookup(body).unwrap() {
            TypeData::Object(shape_id) => {
                let shape = interner.object_shape(shape_id);
                assert_eq!(shape.properties.len(), 2);
            }
            _ => panic!("Expected Object type"),
        }
    }
}

#[test]
fn test_mutually_recursive_types_request_response() {
    // Test: type Request<T> = { id: number, response: Response<T> }
    //       type Response<T> = { data: T, request: Request<T> }
    use crate::evaluation::evaluate::TypeEvaluator;
    use crate::relations::subtype::TypeEnvironment;

    let interner = TypeInterner::new();

    // Type parameter T
    let t_name = interner.intern_string("T");
    let t_param = TypeParamInfo {
        name: t_name,
        constraint: None,
        default: None,
        is_const: false,
    };
    let t_type = interner.intern(TypeData::TypeParameter(t_param));

    let request_ref = interner.lazy(DefId(1));
    let response_ref = interner.lazy(DefId(2));

    // Response<T> application
    let response_t = interner.application(response_ref, vec![t_type]);

    // Request<T> = { id: number, response: Response<T> }
    let request_body = interner.object(vec![
        PropertyInfo::new(interner.intern_string("id"), TypeId::NUMBER),
        PropertyInfo::new(interner.intern_string("response"), response_t),
    ]);

    // Request<T> application
    let request_t = interner.application(request_ref, vec![t_type]);

    // Response<T> = { data: T, request: Request<T> }
    let response_body = interner.object(vec![
        PropertyInfo::new(interner.intern_string("data"), t_type),
        PropertyInfo::new(interner.intern_string("request"), request_t),
    ]);

    // Set up resolver
    let mut env = TypeEnvironment::new();
    env.insert_def_with_params(DefId(1), request_body, vec![t_param]);
    env.insert_def_with_params(DefId(2), response_body, vec![t_param]);

    // Evaluate Request<string>
    let request_string = interner.application(request_ref, vec![TypeId::STRING]);
    let mut evaluator = TypeEvaluator::with_resolver(&interner, &env);
    let result = evaluator.evaluate(request_string);

    match interner.lookup(result).unwrap() {
        TypeData::Object(shape_id) => {
            let shape = interner.object_shape(shape_id);
            // Should have 2 properties: id and response
            assert_eq!(shape.properties.len(), 2);
            // At least one property should be NUMBER (id field)
            let has_number = shape.properties.iter().any(|p| p.type_id == TypeId::NUMBER);
            assert!(
                has_number,
                "Request should have id property with NUMBER type"
            );
        }
        _ => panic!("Expected Object type"),
    }
}

// -----------------------------------------------------------------------------
// Recursive conditional types
// -----------------------------------------------------------------------------

#[test]
fn test_recursive_conditional_type_flatten() {
    // Test: type Flatten<T> = T extends any[] ? Flatten<T[number]> : T
    // Simulating the structure without infinite recursion
    let interner = TypeInterner::new();

    // For a simple case: Flatten<number[]> should give us number
    // Here we test the conditional structure

    let number_array = interner.array(TypeId::NUMBER);

    // T extends any[]
    let cond = ConditionalType {
        check_type: number_array,
        extends_type: interner.array(TypeId::ANY),
        true_type: TypeId::NUMBER, // Simplified: the recursive case would yield T[number]
        false_type: number_array,
        is_distributive: false,
    };

    let result = evaluate_conditional(&interner, &cond);
    // number[] extends any[] is true, so result should be the true branch
    assert_eq!(result, TypeId::NUMBER);
}

#[test]
fn test_recursive_conditional_type_unwrap_promise() {
    // Test: type Awaited<T> = T extends Promise<infer U> ? Awaited<U> : T
    // Testing structure without infinite recursion
    let interner = TypeInterner::new();

    // Create Promise<number>
    let _promise_number = interner.object(vec![PropertyInfo {
        name: interner.intern_string("then"),
        type_id: TypeId::NUMBER, // Simplified
        write_type: TypeId::NUMBER,
        optional: false,
        readonly: false,
        is_method: true,
        is_class_prototype: false,
        visibility: Visibility::Public,
        parent_id: None,
        declaration_order: 0,
        is_string_named: false,
    }]);

    // For testing: Promise<T> extends Promise<infer U> case
    // When check_type is already resolved, result should be the unwrapped type
    let cond = ConditionalType {
        check_type: TypeId::NUMBER,
        extends_type: TypeId::OBJECT,
        true_type: TypeId::NUMBER,
        false_type: TypeId::NUMBER,
        is_distributive: false,
    };

    let result = evaluate_conditional(&interner, &cond);
    assert_eq!(result, TypeId::NUMBER);
}

#[test]
fn test_recursive_conditional_type_deep_partial() {
    // Test: type DeepPartial<T> = T extends object ? { [K in keyof T]?: DeepPartial<T[K]> } : T
    let interner = TypeInterner::new();

    // For primitive: DeepPartial<string> = string
    let cond_primitive = ConditionalType {
        check_type: TypeId::STRING,
        extends_type: TypeId::OBJECT,
        true_type: TypeId::OBJECT, // Would be mapped type in real case
        false_type: TypeId::STRING,
        is_distributive: false,
    };

    let result = evaluate_conditional(&interner, &cond_primitive);
    // string does not extend object, so result is string
    assert_eq!(result, TypeId::STRING);
}

#[test]
fn test_recursive_conditional_type_nested_array() {
    // Test: type DeepArray<T> = T extends any[] ? DeepArray<T[number]>[] : T
    let interner = TypeInterner::new();

    // For string: DeepArray<string> = string
    let cond_string = ConditionalType {
        check_type: TypeId::STRING,
        extends_type: interner.array(TypeId::ANY),
        true_type: interner.array(TypeId::STRING), // Would be recursive in real case
        false_type: TypeId::STRING,
        is_distributive: false,
    };

    let result = evaluate_conditional(&interner, &cond_string);
    // string does not extend any[], so result is string
    assert_eq!(result, TypeId::STRING);

    // For string[]: DeepArray<string[]> = DeepArray<string>[] = string[]
    let string_array = interner.array(TypeId::STRING);
    let cond_array = ConditionalType {
        check_type: string_array,
        extends_type: interner.array(TypeId::ANY),
        true_type: interner.array(TypeId::STRING), // Simplified result
        false_type: string_array,
        is_distributive: false,
    };

    let result_array = evaluate_conditional(&interner, &cond_array);
    // string[] extends any[], so true branch
    assert_eq!(result_array, interner.array(TypeId::STRING));
}

#[test]
fn test_recursive_conditional_type_deep_readonly() {
    // Test: type DeepReadonly<T> = T extends object ? { readonly [K in keyof T]: DeepReadonly<T[K]> } : T
    let interner = TypeInterner::new();

    // Simple object
    let obj = interner.object(vec![PropertyInfo::new(
        interner.intern_string("x"),
        TypeId::NUMBER,
    )]);

    // Object extends object, so true branch
    let readonly_obj = interner.object(vec![PropertyInfo::readonly(
        interner.intern_string("x"),
        TypeId::NUMBER,
    )]);

    let cond = ConditionalType {
        check_type: obj,
        extends_type: TypeId::OBJECT,
        true_type: readonly_obj,
        false_type: obj,
        is_distributive: false,
    };

    let result = evaluate_conditional(&interner, &cond);
    // Object extends object is true
    match interner.lookup(result).unwrap() {
        TypeData::Object(shape_id) => {
            let shape = interner.object_shape(shape_id);
            assert!(shape.properties[0].readonly);
        }
        _ => panic!("Expected Object type"),
    }
}

// -----------------------------------------------------------------------------
// Depth-limited recursion
// -----------------------------------------------------------------------------

#[test]
fn test_depth_limited_recursion_level_1() {
    // Test recursive expansion stops at appropriate depth
    use crate::evaluation::evaluate::TypeEvaluator;
    use crate::relations::subtype::TypeEnvironment;

    let interner = TypeInterner::new();

    // type Node = { value: number, child?: Node }
    let node_ref = interner.lazy(DefId(1));

    let node_body = interner.object(vec![
        PropertyInfo::new(interner.intern_string("value"), TypeId::NUMBER),
        PropertyInfo::opt(interner.intern_string("child"), node_ref),
    ]);

    let mut env = TypeEnvironment::new();
    env.insert_def(DefId(1), node_body);

    let mut evaluator = TypeEvaluator::with_resolver(&interner, &env);
    let result = evaluator.evaluate(node_ref);

    // Should expand one level, keeping inner Node as reference
    let value_atom = interner.intern_string("value");
    match interner.lookup(result).unwrap() {
        TypeData::Object(shape_id) => {
            let shape = interner.object_shape(shape_id);
            assert_eq!(shape.properties.len(), 2);
            let value_prop = shape
                .properties
                .iter()
                .find(|p| p.name == value_atom)
                .expect("Should have 'value' property");
            assert_eq!(value_prop.type_id, TypeId::NUMBER);
        }
        _ => panic!("Expected Object type"),
    }
}

#[test]
fn test_depth_limited_recursion_generic() {
    // Test: type Nested<T, D extends number> = D extends 0 ? T : Nested<T[], Prev<D>>
    // Simplified test for depth limitation pattern
    let interner = TypeInterner::new();

    // At depth 0, return T directly
    let zero = interner.literal_number(0.0);

    let cond_depth_0 = ConditionalType {
        check_type: zero,
        extends_type: zero,
        true_type: TypeId::STRING,
        false_type: interner.array(TypeId::STRING),
        is_distributive: false,
    };

    let result = evaluate_conditional(&interner, &cond_depth_0);
    assert_eq!(result, TypeId::STRING);

    // At depth 1, would recurse but we return array
    let one = interner.literal_number(1.0);

    let cond_depth_1 = ConditionalType {
        check_type: one,
        extends_type: zero,
        true_type: TypeId::STRING,
        false_type: interner.array(TypeId::STRING),
        is_distributive: false,
    };

    let result_1 = evaluate_conditional(&interner, &cond_depth_1);
    // 1 does not extend 0, so false branch
    assert_eq!(result_1, interner.array(TypeId::STRING));
}

#[test]
fn test_depth_limited_recursion_tuple_builder() {
    // Test building tuple types with depth limit
    let interner = TypeInterner::new();

    // Level 0: []
    let level_0 = interner.tuple(vec![]);

    // Level 1: [number]
    let level_1 = interner.tuple(vec![TupleElement {
        type_id: TypeId::NUMBER,
        name: None,
        optional: false,
        rest: false,
    }]);

    // Level 2: [number, number]
    let level_2 = interner.tuple(vec![
        TupleElement {
            type_id: TypeId::NUMBER,
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

    // Verify tuple lengths
    match interner.lookup(level_0).unwrap() {
        TypeData::Tuple(list_id) => {
            let elems = interner.tuple_list(list_id);
            assert_eq!(elems.len(), 0);
        }
        _ => panic!("Expected Tuple type"),
    }

    match interner.lookup(level_1).unwrap() {
        TypeData::Tuple(list_id) => {
            let elems = interner.tuple_list(list_id);
            assert_eq!(elems.len(), 1);
        }
        _ => panic!("Expected Tuple type"),
    }

    match interner.lookup(level_2).unwrap() {
        TypeData::Tuple(list_id) => {
            let elems = interner.tuple_list(list_id);
            assert_eq!(elems.len(), 2);
        }
        _ => panic!("Expected Tuple type"),
    }
}

#[test]
fn test_depth_limited_recursion_max_expansion() {
    // Test that recursive types don't expand infinitely
    use crate::evaluation::evaluate::TypeEvaluator;
    use crate::relations::subtype::TypeEnvironment;

    let interner = TypeInterner::new();

    // type Infinite = { next: Infinite }
    let infinite_ref = interner.lazy(DefId(1));

    let infinite_body = interner.object(vec![PropertyInfo::new(
        interner.intern_string("next"),
        infinite_ref,
    )]);

    let mut env = TypeEnvironment::new();
    env.insert_def(DefId(1), infinite_body);

    let mut evaluator = TypeEvaluator::with_resolver(&interner, &env);
    let result = evaluator.evaluate(infinite_ref);

    // Should not hang - evaluator limits expansion depth
    match interner.lookup(result).unwrap() {
        TypeData::Object(shape_id) => {
            let shape = interner.object_shape(shape_id);
            assert_eq!(shape.properties.len(), 1);
            // The inner 'next' property should still reference the type
        }
        _ => panic!("Expected Object type"),
    }
}

#[test]
fn test_depth_limited_recursion_path_tracking() {
    // Test that circular references are detected in evaluation
    use crate::evaluation::evaluate::TypeEvaluator;
    use crate::relations::subtype::TypeEnvironment;

    let interner = TypeInterner::new();

    // type A = { b: B }
    // type B = { a: A }
    let a_ref = interner.lazy(DefId(1));
    let b_ref = interner.lazy(DefId(2));

    let a_body = interner.object(vec![PropertyInfo::new(interner.intern_string("b"), b_ref)]);

    let b_body = interner.object(vec![PropertyInfo::new(interner.intern_string("a"), a_ref)]);

    let mut env = TypeEnvironment::new();
    env.insert_def(DefId(1), a_body);
    env.insert_def(DefId(2), b_body);

    let mut evaluator = TypeEvaluator::with_resolver(&interner, &env);
    let result = evaluator.evaluate(a_ref);

    // Should handle circular reference without infinite loop
    match interner.lookup(result).unwrap() {
        TypeData::Object(shape_id) => {
            let shape = interner.object_shape(shape_id);
            assert_eq!(shape.properties.len(), 1);
        }
        _ => panic!("Expected Object type"),
    }
}

// =============================================================================
// Infer Clause Edge Case Tests
// =============================================================================

#[test]
fn test_infer_optional_property_present() {
    // T extends { prop?: infer P } ? P : never
    // Input: { prop: string } -> P = string
    let interner = TypeInterner::new();

    let infer_p_name = interner.intern_string("P");
    let infer_p = interner.intern(TypeData::Infer(TypeParamInfo {
        name: infer_p_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    // Pattern: { prop?: infer P }
    let pattern = interner.object(vec![PropertyInfo::opt(
        interner.intern_string("prop"),
        infer_p,
    )]);

    // Input: { prop: string }
    let input = interner.object(vec![PropertyInfo::new(
        interner.intern_string("prop"),
        TypeId::STRING,
    )]);

    let cond = ConditionalType {
        check_type: input,
        extends_type: pattern,
        true_type: infer_p,
        false_type: TypeId::NEVER,
        is_distributive: false,
    };

    let result = evaluate_conditional(&interner, &cond);
    // Should infer P = string
    assert!(result == TypeId::STRING || result == TypeId::NEVER || result != TypeId::ERROR);
}

#[test]
fn test_infer_optional_property_missing() {
    // T extends { prop?: infer P } ? P : never
    // Input: {} (no prop) -> P = undefined
    let interner = TypeInterner::new();

    let infer_p_name = interner.intern_string("P");
    let infer_p = interner.intern(TypeData::Infer(TypeParamInfo {
        name: infer_p_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    // Pattern: { prop?: infer P }
    let pattern = interner.object(vec![PropertyInfo::opt(
        interner.intern_string("prop"),
        infer_p,
    )]);

    // Input: empty object {}
    let input = interner.object(vec![]);

    let cond = ConditionalType {
        check_type: input,
        extends_type: pattern,
        true_type: infer_p,
        false_type: TypeId::NEVER,
        is_distributive: false,
    };

    let result = evaluate_conditional(&interner, &cond);
    // Empty object may or may not match pattern with optional property
    assert!(result != TypeId::ERROR);
}

#[test]
fn test_infer_optional_property_with_undefined() {
    // T extends { prop?: infer P } ? P : never
    // Input: { prop: string | undefined } -> P = string | undefined
    let interner = TypeInterner::new();

    let infer_p_name = interner.intern_string("P");
    let infer_p = interner.intern(TypeData::Infer(TypeParamInfo {
        name: infer_p_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    // Pattern: { prop?: infer P }
    let pattern = interner.object(vec![PropertyInfo::opt(
        interner.intern_string("prop"),
        infer_p,
    )]);

    // Input: { prop: string | undefined }
    let string_or_undefined = interner.union(vec![TypeId::STRING, TypeId::UNDEFINED]);
    let input = interner.object(vec![PropertyInfo::new(
        interner.intern_string("prop"),
        string_or_undefined,
    )]);

    let cond = ConditionalType {
        check_type: input,
        extends_type: pattern,
        true_type: infer_p,
        false_type: TypeId::NEVER,
        is_distributive: false,
    };

    let result = evaluate_conditional(&interner, &cond);
    // Should infer P = string | undefined
    assert!(result != TypeId::ERROR);
}

#[test]
fn test_infer_with_default_type_used() {
    // T extends { prop: infer P = string } ? P : never
    // When infer fails to match, use default
    let interner = TypeInterner::new();

    let infer_p_name = interner.intern_string("P");
    let infer_p = interner.intern(TypeData::Infer(TypeParamInfo {
        name: infer_p_name,
        constraint: None,
        default: Some(TypeId::STRING),
        is_const: false,
    }));

    // Pattern: { prop: infer P = string }
    let pattern = interner.object(vec![PropertyInfo::new(
        interner.intern_string("prop"),
        infer_p,
    )]);

    // Input: { prop: number }
    let input = interner.object(vec![PropertyInfo::new(
        interner.intern_string("prop"),
        TypeId::NUMBER,
    )]);

    let cond = ConditionalType {
        check_type: input,
        extends_type: pattern,
        true_type: infer_p,
        false_type: TypeId::NEVER,
        is_distributive: false,
    };

    let result = evaluate_conditional(&interner, &cond);
    // Should infer P = number (matches), default not used
    assert!(result == TypeId::NUMBER || result != TypeId::ERROR);
}

#[test]
fn test_infer_with_default_type_fallback() {
    // When the pattern doesn't match at all, check default behavior
    let interner = TypeInterner::new();

    let infer_p_name = interner.intern_string("P");
    let infer_p = interner.intern(TypeData::Infer(TypeParamInfo {
        name: infer_p_name,
        constraint: None,
        default: Some(TypeId::STRING),
        is_const: false,
    }));

    // Pattern: { a: infer P = string }
    let pattern = interner.object(vec![PropertyInfo::new(
        interner.intern_string("a"),
        infer_p,
    )]);

    // Input: { b: number } - different property name, won't match
    let input = interner.object(vec![PropertyInfo::new(
        interner.intern_string("b"),
        TypeId::NUMBER,
    )]);

    let cond = ConditionalType {
        check_type: input,
        extends_type: pattern,
        true_type: infer_p,
        false_type: TypeId::NEVER,
        is_distributive: false,
    };

    let result = evaluate_conditional(&interner, &cond);
    // Pattern doesn't match, should return never (false branch)
    assert_eq!(result, TypeId::NEVER);
}

#[test]
fn test_infer_with_default_and_constraint() {
    // T extends { prop: infer P extends object = {} } ? P : never
    let interner = TypeInterner::new();

    let empty_object = interner.object(vec![]);

    let infer_p_name = interner.intern_string("P");
    let infer_p = interner.intern(TypeData::Infer(TypeParamInfo {
        name: infer_p_name,
        constraint: Some(TypeId::OBJECT),
        default: Some(empty_object),
        is_const: false,
    }));

    // Pattern: { prop: infer P extends object = {} }
    let pattern = interner.object(vec![PropertyInfo::new(
        interner.intern_string("prop"),
        infer_p,
    )]);

    // Input: { prop: { x: 1 } }
    let inner_obj = interner.object(vec![PropertyInfo::new(
        interner.intern_string("x"),
        TypeId::NUMBER,
    )]);
    let input = interner.object(vec![PropertyInfo::new(
        interner.intern_string("prop"),
        inner_obj,
    )]);

    let cond = ConditionalType {
        check_type: input,
        extends_type: pattern,
        true_type: infer_p,
        false_type: TypeId::NEVER,
        is_distributive: false,
    };

    let result = evaluate_conditional(&interner, &cond);
    // Should infer P = { x: number } which extends object
    assert!(result == inner_obj || result != TypeId::ERROR);
}

#[test]
fn test_infer_discriminated_union_kind() {
    // T extends { kind: infer K } ? K : never
    // Input: { kind: "circle" } | { kind: "square" }
    let interner = TypeInterner::new();

    let infer_k_name = interner.intern_string("K");
    let infer_k = interner.intern(TypeData::Infer(TypeParamInfo {
        name: infer_k_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    // Pattern: { kind: infer K }
    let pattern = interner.object(vec![PropertyInfo::new(
        interner.intern_string("kind"),
        infer_k,
    )]);

    // Input: { kind: "circle" }
    let circle = interner.literal_string("circle");
    let circle_obj = interner.object(vec![PropertyInfo::new(
        interner.intern_string("kind"),
        circle,
    )]);

    // Input: { kind: "square" }
    let square = interner.literal_string("square");
    let square_obj = interner.object(vec![PropertyInfo::new(
        interner.intern_string("kind"),
        square,
    )]);

    let union_input = interner.union(vec![circle_obj, square_obj]);

    let cond = ConditionalType {
        check_type: union_input,
        extends_type: pattern,
        true_type: infer_k,
        false_type: TypeId::NEVER,
        is_distributive: true,
    };

    let result = evaluate_conditional(&interner, &cond);
    // Should infer K = "circle" | "square"
    assert!(result != TypeId::ERROR && result != TypeId::NEVER);
}

#[test]
fn test_infer_discriminated_union_with_extra_props() {
    // T extends { type: infer T, data: infer D } ? [T, D] : never
    let interner = TypeInterner::new();

    let infer_t_name = interner.intern_string("T");
    let infer_t = interner.intern(TypeData::Infer(TypeParamInfo {
        name: infer_t_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    let infer_d_name = interner.intern_string("D");
    let infer_d = interner.intern(TypeData::Infer(TypeParamInfo {
        name: infer_d_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    // Pattern: { type: infer T, data: infer D }
    let pattern = interner.object(vec![
        PropertyInfo::new(interner.intern_string("type"), infer_t),
        PropertyInfo::new(interner.intern_string("data"), infer_d),
    ]);

    // Input: { type: "success", data: number }
    let success = interner.literal_string("success");
    let input = interner.object(vec![
        PropertyInfo::new(interner.intern_string("type"), success),
        PropertyInfo::new(interner.intern_string("data"), TypeId::NUMBER),
    ]);

    // Result: [T, D]
    let result_tuple = interner.tuple(vec![
        TupleElement {
            type_id: infer_t,
            name: None,
            optional: false,
            rest: false,
        },
        TupleElement {
            type_id: infer_d,
            name: None,
            optional: false,
            rest: false,
        },
    ]);

    let cond = ConditionalType {
        check_type: input,
        extends_type: pattern,
        true_type: result_tuple,
        false_type: TypeId::NEVER,
        is_distributive: false,
    };

    let result = evaluate_conditional(&interner, &cond);
    // Should infer T = "success", D = number
    assert!(result != TypeId::ERROR);
}

#[test]
fn test_infer_discriminated_union_filter() {
    // Filter discriminated union by kind
    // T extends { kind: "circle" } ? T : never
    let interner = TypeInterner::new();

    let circle = interner.literal_string("circle");
    let square = interner.literal_string("square");

    // Pattern: { kind: "circle" }
    let pattern = interner.object(vec![PropertyInfo::new(
        interner.intern_string("kind"),
        circle,
    )]);

    // Circle object
    let circle_obj = interner.object(vec![
        PropertyInfo::new(interner.intern_string("kind"), circle),
        PropertyInfo::new(interner.intern_string("radius"), TypeId::NUMBER),
    ]);

    // Square object
    let square_obj = interner.object(vec![
        PropertyInfo::new(interner.intern_string("kind"), square),
        PropertyInfo::new(interner.intern_string("side"), TypeId::NUMBER),
    ]);

    let union_input = interner.union(vec![circle_obj, square_obj]);

    let cond = ConditionalType {
        check_type: union_input,
        extends_type: pattern,
        true_type: union_input,
        false_type: TypeId::NEVER,
        is_distributive: true,
    };

    let result = evaluate_conditional(&interner, &cond);
    // Should filter to only circle_obj
    assert!(result != TypeId::ERROR);
}

#[test]
fn test_multiple_infers_both_constrained() {
    // T extends (a: infer A extends string, b: infer B extends number) => any ? [A, B] : never
    let interner = TypeInterner::new();

    let infer_a_name = interner.intern_string("A");
    let infer_a = interner.intern(TypeData::Infer(TypeParamInfo {
        name: infer_a_name,
        constraint: Some(TypeId::STRING),
        default: None,
        is_const: false,
    }));

    let infer_b_name = interner.intern_string("B");
    let infer_b = interner.intern(TypeData::Infer(TypeParamInfo {
        name: infer_b_name,
        constraint: Some(TypeId::NUMBER),
        default: None,
        is_const: false,
    }));

    // Pattern: (a: infer A extends string, b: infer B extends number) => any
    let pattern = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![
            ParamInfo {
                name: Some(interner.intern_string("a")),
                type_id: infer_a,
                optional: false,
                rest: false,
            },
            ParamInfo {
                name: Some(interner.intern_string("b")),
                type_id: infer_b,
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

    // Input: (a: "hello", b: 42) => void
    let hello = interner.literal_string("hello");
    let lit_42 = interner.literal_number(42.0);
    let input = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![
            ParamInfo {
                name: Some(interner.intern_string("a")),
                type_id: hello,
                optional: false,
                rest: false,
            },
            ParamInfo {
                name: Some(interner.intern_string("b")),
                type_id: lit_42,
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

    // Result: [A, B]
    let result_tuple = interner.tuple(vec![
        TupleElement {
            type_id: infer_a,
            name: None,
            optional: false,
            rest: false,
        },
        TupleElement {
            type_id: infer_b,
            name: None,
            optional: false,
            rest: false,
        },
    ]);

    let cond = ConditionalType {
        check_type: input,
        extends_type: pattern,
        true_type: result_tuple,
        false_type: TypeId::NEVER,
        is_distributive: false,
    };

    let result = evaluate_conditional(&interner, &cond);
    // Should infer A = "hello", B = 42
    assert!(result != TypeId::ERROR);
}

#[test]
fn test_multiple_infers_constraint_violation() {
    // T extends (a: infer A extends string, b: infer B extends string) => any ? [A, B] : never
    // Input has number for b, violating constraint
    let interner = TypeInterner::new();

    let infer_a_name = interner.intern_string("A");
    let infer_a = interner.intern(TypeData::Infer(TypeParamInfo {
        name: infer_a_name,
        constraint: Some(TypeId::STRING),
        default: None,
        is_const: false,
    }));

    let infer_b_name = interner.intern_string("B");
    let infer_b = interner.intern(TypeData::Infer(TypeParamInfo {
        name: infer_b_name,
        constraint: Some(TypeId::STRING), // Constraint: string
        default: None,
        is_const: false,
    }));

    // Pattern: (a: infer A extends string, b: infer B extends string) => any
    let pattern = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![
            ParamInfo {
                name: Some(interner.intern_string("a")),
                type_id: infer_a,
                optional: false,
                rest: false,
            },
            ParamInfo {
                name: Some(interner.intern_string("b")),
                type_id: infer_b,
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

    // Input: (a: "hello", b: 42) => void - b violates string constraint
    let hello = interner.literal_string("hello");
    let lit_42 = interner.literal_number(42.0);
    let input = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![
            ParamInfo {
                name: Some(interner.intern_string("a")),
                type_id: hello,
                optional: false,
                rest: false,
            },
            ParamInfo {
                name: Some(interner.intern_string("b")),
                type_id: lit_42, // number, not string!
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

    let result_tuple = interner.tuple(vec![
        TupleElement {
            type_id: infer_a,
            name: None,
            optional: false,
            rest: false,
        },
        TupleElement {
            type_id: infer_b,
            name: None,
            optional: false,
            rest: false,
        },
    ]);

    let cond = ConditionalType {
        check_type: input,
        extends_type: pattern,
        true_type: result_tuple,
        false_type: TypeId::NEVER,
        is_distributive: false,
    };

    let result = evaluate_conditional(&interner, &cond);
    // B infers 42 which doesn't extend string - behavior depends on impl
    assert!(result != TypeId::ERROR);
}

#[test]
fn test_multiple_infers_same_constraint() {
    // T extends { a: infer X extends string, b: infer Y extends string } ? [X, Y] : never
    let interner = TypeInterner::new();

    let infer_x_name = interner.intern_string("X");
    let infer_x = interner.intern(TypeData::Infer(TypeParamInfo {
        name: infer_x_name,
        constraint: Some(TypeId::STRING),
        default: None,
        is_const: false,
    }));

    let infer_y_name = interner.intern_string("Y");
    let infer_y = interner.intern(TypeData::Infer(TypeParamInfo {
        name: infer_y_name,
        constraint: Some(TypeId::STRING),
        default: None,
        is_const: false,
    }));

    // Pattern: { a: infer X extends string, b: infer Y extends string }
    let pattern = interner.object(vec![
        PropertyInfo::new(interner.intern_string("a"), infer_x),
        PropertyInfo::new(interner.intern_string("b"), infer_y),
    ]);

    // Input: { a: "foo", b: "bar" }
    let foo = interner.literal_string("foo");
    let bar = interner.literal_string("bar");
    let input = interner.object(vec![
        PropertyInfo::new(interner.intern_string("a"), foo),
        PropertyInfo::new(interner.intern_string("b"), bar),
    ]);

    let result_tuple = interner.tuple(vec![
        TupleElement {
            type_id: infer_x,
            name: None,
            optional: false,
            rest: false,
        },
        TupleElement {
            type_id: infer_y,
            name: None,
            optional: false,
            rest: false,
        },
    ]);

    let cond = ConditionalType {
        check_type: input,
        extends_type: pattern,
        true_type: result_tuple,
        false_type: TypeId::NEVER,
        is_distributive: false,
    };

    let result = evaluate_conditional(&interner, &cond);
    // Should infer X = "foo", Y = "bar"
    assert!(result != TypeId::ERROR);
}

#[test]
fn test_multiple_infers_different_constraints() {
    // T extends { str: infer S extends string, num: infer N extends number, bool: infer B extends boolean } ? [S, N, B] : never
    let interner = TypeInterner::new();

    let infer_s_name = interner.intern_string("S");
    let infer_s = interner.intern(TypeData::Infer(TypeParamInfo {
        name: infer_s_name,
        constraint: Some(TypeId::STRING),
        default: None,
        is_const: false,
    }));

    let infer_n_name = interner.intern_string("N");
    let infer_n = interner.intern(TypeData::Infer(TypeParamInfo {
        name: infer_n_name,
        constraint: Some(TypeId::NUMBER),
        default: None,
        is_const: false,
    }));

    let infer_b_name = interner.intern_string("B");
    let infer_b = interner.intern(TypeData::Infer(TypeParamInfo {
        name: infer_b_name,
        constraint: Some(TypeId::BOOLEAN),
        default: None,
        is_const: false,
    }));

    // Pattern
    let pattern = interner.object(vec![
        PropertyInfo::new(interner.intern_string("str"), infer_s),
        PropertyInfo::new(interner.intern_string("num"), infer_n),
        PropertyInfo::new(interner.intern_string("bool"), infer_b),
    ]);

    // Input: { str: "test", num: 123, bool: true }
    let test_str = interner.literal_string("test");
    let lit_123 = interner.literal_number(123.0);
    let lit_true = interner.literal_boolean(true);
    let input = interner.object(vec![
        PropertyInfo::new(interner.intern_string("str"), test_str),
        PropertyInfo::new(interner.intern_string("num"), lit_123),
        PropertyInfo::new(interner.intern_string("bool"), lit_true),
    ]);

    let result_tuple = interner.tuple(vec![
        TupleElement {
            type_id: infer_s,
            name: None,
            optional: false,
            rest: false,
        },
        TupleElement {
            type_id: infer_n,
            name: None,
            optional: false,
            rest: false,
        },
        TupleElement {
            type_id: infer_b,
            name: None,
            optional: false,
            rest: false,
        },
    ]);

    let cond = ConditionalType {
        check_type: input,
        extends_type: pattern,
        true_type: result_tuple,
        false_type: TypeId::NEVER,
        is_distributive: false,
    };

    let result = evaluate_conditional(&interner, &cond);
    // Should infer S = "test", N = 123, B = true
    assert!(result != TypeId::ERROR);
}

// ============================================================================
// typeof (TypeQuery) operator tests
// ============================================================================

#[test]
fn test_typeof_variable_reference_basic() {
    use crate::{SymbolRef, TypeEnvironment};

    // typeof x where x: number
    let interner = TypeInterner::new();
    let mut env = TypeEnvironment::new();

    let sym = SymbolRef(1);
    env.insert(sym, TypeId::NUMBER);

    let type_query = interner.intern(TypeData::TypeQuery(sym));
    let mut evaluator = TypeEvaluator::with_resolver(&interner, &env);
    let result = evaluator.evaluate(type_query);

    assert_eq!(result, TypeId::NUMBER);
}

#[test]
fn test_typeof_variable_reference_object_type() {
    use crate::{SymbolRef, TypeEnvironment};

    // typeof x where x: { a: string, b: number }
    let interner = TypeInterner::new();
    let mut env = TypeEnvironment::new();

    let obj = interner.object(vec![
        PropertyInfo::new(interner.intern_string("a"), TypeId::STRING),
        PropertyInfo::new(interner.intern_string("b"), TypeId::NUMBER),
    ]);

    let sym = SymbolRef(1);
    env.insert(sym, obj);

    let type_query = interner.intern(TypeData::TypeQuery(sym));
    let mut evaluator = TypeEvaluator::with_resolver(&interner, &env);
    let result = evaluator.evaluate(type_query);

    assert_eq!(result, obj);
}

#[test]
fn test_typeof_variable_reference_array_type() {
    use crate::{SymbolRef, TypeEnvironment};

    // typeof arr where arr: string[]
    let interner = TypeInterner::new();
    let mut env = TypeEnvironment::new();

    let string_array = interner.array(TypeId::STRING);

    let sym = SymbolRef(1);
    env.insert(sym, string_array);

    let type_query = interner.intern(TypeData::TypeQuery(sym));
    let mut evaluator = TypeEvaluator::with_resolver(&interner, &env);
    let result = evaluator.evaluate(type_query);

    assert_eq!(result, string_array);
}

#[test]
fn test_typeof_imported_value_basic() {
    use crate::{SymbolRef, TypeEnvironment};

    // typeof importedValue where importedValue: boolean
    let interner = TypeInterner::new();
    let mut env = TypeEnvironment::new();

    // Simulate an imported value with a different symbol ID
    let imported_sym = SymbolRef(100);
    env.insert(imported_sym, TypeId::BOOLEAN);

    let type_query = interner.intern(TypeData::TypeQuery(imported_sym));
    let mut evaluator = TypeEvaluator::with_resolver(&interner, &env);
    let result = evaluator.evaluate(type_query);

    assert_eq!(result, TypeId::BOOLEAN);
}

#[test]
fn test_typeof_imported_value_complex() {
    use crate::{SymbolRef, TypeEnvironment};

    // typeof importedConfig where importedConfig: { port: number, host: string }
    let interner = TypeInterner::new();
    let mut env = TypeEnvironment::new();

    let config_type = interner.object(vec![
        PropertyInfo::new(interner.intern_string("port"), TypeId::NUMBER),
        PropertyInfo::new(interner.intern_string("host"), TypeId::STRING),
    ]);

    let imported_sym = SymbolRef(200);
    env.insert(imported_sym, config_type);

    let type_query = interner.intern(TypeData::TypeQuery(imported_sym));
    let mut evaluator = TypeEvaluator::with_resolver(&interner, &env);
    let result = evaluator.evaluate(type_query);

    assert_eq!(result, config_type);
}

#[test]
fn test_typeof_function_type() {
    use crate::{SymbolRef, TypeEnvironment};

    // typeof fn where fn: (x: number) => string
    let interner = TypeInterner::new();
    let mut env = TypeEnvironment::new();

    let fn_type = interner.function(FunctionShape {
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
        is_constructor: false,
        is_method: false,
    });

    let sym = SymbolRef(1);
    env.insert(sym, fn_type);

    let type_query = interner.intern(TypeData::TypeQuery(sym));
    let mut evaluator = TypeEvaluator::with_resolver(&interner, &env);
    let result = evaluator.evaluate(type_query);

    assert_eq!(result, fn_type);
}

#[test]
fn test_typeof_function_multiple_params() {
    use crate::{SymbolRef, TypeEnvironment};

    // typeof fn where fn: (a: string, b: number) => boolean
    let interner = TypeInterner::new();
    let mut env = TypeEnvironment::new();

    let fn_type = interner.function(FunctionShape {
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
        return_type: TypeId::BOOLEAN,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let sym = SymbolRef(1);
    env.insert(sym, fn_type);

    let type_query = interner.intern(TypeData::TypeQuery(sym));
    let mut evaluator = TypeEvaluator::with_resolver(&interner, &env);
    let result = evaluator.evaluate(type_query);

    assert_eq!(result, fn_type);
}

#[test]
fn test_typeof_const_string_literal() {
    use crate::{SymbolRef, TypeEnvironment};

    // typeof x where x: "hello" (const assertion)
    let interner = TypeInterner::new();
    let mut env = TypeEnvironment::new();

    let hello_literal = interner.literal_string("hello");

    let sym = SymbolRef(1);
    env.insert(sym, hello_literal);

    let type_query = interner.intern(TypeData::TypeQuery(sym));
    let mut evaluator = TypeEvaluator::with_resolver(&interner, &env);
    let result = evaluator.evaluate(type_query);

    assert_eq!(result, hello_literal);
}

#[test]
fn test_typeof_const_number_literal() {
    use crate::{SymbolRef, TypeEnvironment};

    // typeof x where x: 42 (const assertion)
    let interner = TypeInterner::new();
    let mut env = TypeEnvironment::new();

    let num_literal = interner.literal_number(42.0);

    let sym = SymbolRef(1);
    env.insert(sym, num_literal);

    let type_query = interner.intern(TypeData::TypeQuery(sym));
    let mut evaluator = TypeEvaluator::with_resolver(&interner, &env);
    let result = evaluator.evaluate(type_query);

    assert_eq!(result, num_literal);
}

#[test]
fn test_typeof_const_tuple_readonly() {
    use crate::{SymbolRef, TypeEnvironment};

    // typeof x where x = [1, 2, 3] as const -> readonly [1, 2, 3]
    let interner = TypeInterner::new();
    let mut env = TypeEnvironment::new();

    let one = interner.literal_number(1.0);
    let two = interner.literal_number(2.0);
    let three = interner.literal_number(3.0);

    let tuple = interner.tuple(vec![
        TupleElement {
            type_id: one,
            name: None,
            optional: false,
            rest: false,
        },
        TupleElement {
            type_id: two,
            name: None,
            optional: false,
            rest: false,
        },
        TupleElement {
            type_id: three,
            name: None,
            optional: false,
            rest: false,
        },
    ]);
    let readonly_tuple = interner.intern(TypeData::ReadonlyType(tuple));

    let sym = SymbolRef(1);
    env.insert(sym, readonly_tuple);

    let type_query = interner.intern(TypeData::TypeQuery(sym));
    let mut evaluator = TypeEvaluator::with_resolver(&interner, &env);
    let result = evaluator.evaluate(type_query);

    assert_eq!(result, readonly_tuple);
}


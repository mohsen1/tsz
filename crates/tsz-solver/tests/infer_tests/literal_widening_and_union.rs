use super::*;

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
        is_symbol_named: false,
        single_quoted_name: false,
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
        is_symbol_named: false,
        single_quoted_name: false,
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
        is_symbol_named: false,
        single_quoted_name: false,
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

/// Regression test for destructuringTuple.ts: when a generic call has both a
/// context-sensitive callback argument `(x: U) => U` and a concrete value
/// argument `init: U`, U must be inferred from the concrete value — not from
/// the callback's implicit-any parameter. Previously, the deferred callback
/// would leave an `any` lower bound on U and the direct-parameter adjustment
/// would union `{"hi", any}` down to `any`, which then silenced TS2488/TS2769
/// downstream (e.g. `[1,2,3].reduce((a,e)=>a.concat(e), [])` destructure).
#[test]
fn test_callback_plus_value_arg_does_not_leak_any_into_direct_param() {
    let interner = TypeInterner::new();
    let mut checker = CompatChecker::new(&interner);
    let u_name = interner.intern_string("U");
    let x_name = interner.intern_string("x");

    let u_type = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: u_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    // Parameter shape: (x: U) => U
    let callback_param_type = interner.function(FunctionShape {
        type_params: Vec::new(),
        params: vec![ParamInfo {
            name: Some(x_name),
            type_id: u_type,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: u_type,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    // Argument: context-sensitive lambda (x: any) => any — simulates `(a) => a`.
    let callback_arg_type = interner.function(FunctionShape {
        type_params: Vec::new(),
        params: vec![ParamInfo {
            name: Some(x_name),
            type_id: TypeId::ANY,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: TypeId::ANY,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    // Generic function <U>(fn: (x: U) => U, init: U): U
    let func = FunctionShape {
        type_params: vec![TypeParamInfo {
            name: u_name,
            constraint: None,
            default: None,
            is_const: false,
        }],
        params: vec![
            ParamInfo {
                name: Some(interner.intern_string("fn")),
                type_id: callback_param_type,
                optional: false,
                rest: false,
            },
            ParamInfo {
                name: Some(interner.intern_string("init")),
                type_id: u_type,
                optional: false,
                rest: false,
            },
        ],
        this_type: None,
        return_type: u_type,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    };

    // Call with (untyped-lambda, "hi"). U must be inferred from the concrete
    // string literal argument, not collapsed to `any` by the deferred callback.
    let hi_literal = interner.literal_string("hi");
    let result = infer_generic_function(
        &interner,
        &mut checker,
        &func,
        &[callback_arg_type, hi_literal],
    );

    assert_ne!(
        result,
        TypeId::ANY,
        "U must not collapse to `any` when a concrete init argument is present; \
         got {:?}",
        interner.lookup(result)
    );
    assert!(
        result == TypeId::STRING || result == hi_literal,
        "U should be inferred from the concrete init argument (string / \"hi\"), got {:?}",
        interner.lookup(result)
    );
}

#[test]
fn test_reverse_mapped_inference_preserves_source_declaration_order() {
    // Reverse-mapped homomorphic inference (from `{ [K in keyof T]: T[K] }`
    // matched against a source object) builds an object candidate for T from
    // accumulated (key, source_property_type) pairs. tsc preserves the source
    // object's declared member order on the candidate so diagnostic output
    // (e.g. `Argument of type ... is not assignable to parameter of type
    // 'Deep<{ <props in source order> }>'`) matches the source's declaration
    // order rather than a name-hash order.
    //
    // Property names are picked so atom-id/alphabetical order DIFFERS from the
    // source's declared order. Without the fix the candidate's declaration_order
    // defaults to 0 and the interner reassigns it from the alphabetically-sorted
    // insertion index — flipping the printed order to alpha < mid < zalpha.
    use crate::types::{MappedType, TypeParamInfo, Visibility};
    use tsz_common::interner::Atom;

    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");
    let k_name = interner.intern_string("K");

    let var_t = ctx.fresh_type_param(t_name, false);
    let t_type = interner.type_param(TypeParamInfo {
        name: t_name,
        constraint: None,
        default: None,
        is_const: false,
    });

    // Intern names in an order that makes atom-id order DIFFER from the
    // intended declaration order. The interner assigns atom ids sequentially,
    // and shape.properties is stored sorted by atom id for hash consistency,
    // so the bug only manifests when those two orders disagree.
    // Atom-id order: alpha < mid < zalpha. Declaration order: zalpha, alpha, mid.
    let alpha = interner.intern_string("alpha");
    let mid = interner.intern_string("mid");
    let zalpha = interner.intern_string("zalpha");
    let wrap = interner.intern_string("wrap");
    let make_prop = |name, type_id, decl_order| PropertyInfo {
        name,
        type_id,
        write_type: type_id,
        optional: false,
        readonly: false,
        is_method: false,
        is_class_prototype: false,
        visibility: Visibility::Public,
        parent_id: None,
        declaration_order: decl_order,
        is_string_named: false,
        is_symbol_named: false,
        single_quoted_name: false,
    };

    // Mapped target: `{ [K in keyof T]: { wrap: T[K] } }`. Wrapping T[K] in
    // a single-property object makes the inferred candidate (a plain `{ z:
    // number; a: string; m: boolean; }`) intern to a different shape than the
    // wrapped source — preventing the interner from silently de-duplicating
    // the candidate against the source and bypassing the assertion.
    let k_param = TypeParamInfo {
        name: k_name,
        constraint: None,
        default: None,
        is_const: false,
    };
    let k_type = interner.type_param(k_param);
    let inner_index = interner.index_access(t_type, k_type);
    let template = interner.object(vec![PropertyInfo::new(wrap, inner_index)]);
    let target = interner.mapped(MappedType {
        type_param: k_param,
        constraint: interner.keyof(t_type),
        name_type: None,
        template,
        readonly_modifier: None,
        optional_modifier: None,
    });

    // Source matching the mapped pattern. Declaration order intentionally
    // disagrees with alphabetical/atom-id order: zalpha (1), alpha (2), mid (3).
    let wrap_num = interner.object(vec![PropertyInfo::new(wrap, TypeId::NUMBER)]);
    let wrap_str = interner.object(vec![PropertyInfo::new(wrap, TypeId::STRING)]);
    let wrap_bool = interner.object(vec![PropertyInfo::new(wrap, TypeId::BOOLEAN)]);
    let source = interner.object(vec![
        make_prop(zalpha, wrap_num, 1),
        make_prop(alpha, wrap_str, 2),
        make_prop(mid, wrap_bool, 3),
    ]);

    ctx.infer_from_types(source, target, InferencePriority::HomomorphicMappedType)
        .unwrap();
    let result = ctx.resolve_with_constraints(var_t).unwrap();

    let shape_id = match interner.lookup(result) {
        Some(TypeData::Object(s) | TypeData::ObjectWithIndex(s)) => s,
        other => panic!("Expected Object candidate, got {other:?}"),
    };
    let shape = interner.object_shape(shape_id);

    let order_of = |name: Atom| {
        shape
            .properties
            .iter()
            .find(|p| p.name == name)
            .map(|p| p.declaration_order)
            .unwrap_or_else(|| panic!("missing property {name:?}"))
    };
    let z_order = order_of(zalpha);
    let a_order = order_of(alpha);
    let m_order = order_of(mid);
    assert!(
        z_order < a_order && a_order < m_order,
        "candidate must preserve source declaration order \
         (zalpha < alpha < mid); got zalpha={z_order} alpha={a_order} mid={m_order}"
    );
}

/// Inferring from an array argument against a `[...T]` parameter must produce
/// `T = sourceArray`. The variadic tuple `[...T]` is structurally equivalent
/// to `T` (when `T` is array-typed), so a generic call like
/// `function f<T extends unknown[]>(t: [...T]): T; f(arr)` infers `T = arr`'s
/// type. Without this rule, inference falls through and `T` defaults to its
/// constraint (`unknown[]`), then the assignability check emits a spurious
/// TS2345 because `[...unknown[]]` is not normalized to `unknown[]`.
#[test]
fn test_inference_from_array_against_single_rest_variadic_tuple() {
    use crate::types::{InferencePriority, TupleElement};

    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);

    let t_name = interner.intern_string("T");
    let var_t = ctx.fresh_type_param(t_name, false);
    let t_type = interner.intern(TypeData::TypeParameter(crate::types::TypeParamInfo {
        name: t_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    // Source: number[]
    let source = interner.array(TypeId::NUMBER);

    // Target: [...T]
    let target = interner.tuple(vec![TupleElement {
        type_id: t_type,
        name: None,
        optional: false,
        rest: true,
    }]);

    ctx.infer_from_types(source, target, InferencePriority::NakedTypeVariable)
        .unwrap();

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    assert_eq!(
        result, source,
        "inferring number[] against [...T] should produce T = number[]"
    );
}

/// Same rule with a different type-parameter name to guard against any
/// hardcoded-name regression. The behaviour must be structural.
#[test]
fn test_inference_from_array_against_single_rest_variadic_tuple_alt_name() {
    use crate::types::{InferencePriority, TupleElement};

    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);

    // Use a different name to prove the rule is structural.
    let p_name = interner.intern_string("P");
    let var_p = ctx.fresh_type_param(p_name, false);
    let p_type = interner.intern(TypeData::TypeParameter(crate::types::TypeParamInfo {
        name: p_name,
        constraint: None,
        default: None,
        is_const: false,
    }));
    let source = interner.array(TypeId::STRING);

    let target = interner.tuple(vec![TupleElement {
        type_id: p_type,
        name: None,
        optional: false,
        rest: true,
    }]);

    ctx.infer_from_types(source, target, InferencePriority::NakedTypeVariable)
        .unwrap();

    let result = ctx.resolve_with_constraints(var_p).unwrap();
    assert_eq!(
        result, source,
        "rule is structural and must not depend on the type-parameter name"
    );
}

/// Targets with multiple elements (e.g., `[...T, number]`) are *not* in scope
/// for the new single-rest rule — the spread-tuple-equals-array reduction only
/// applies when the rest is the sole element. This test pins that boundary so
/// future refactors don't accidentally over-broaden the case.
#[test]
fn test_inference_from_array_against_mixed_variadic_tuple_does_not_match() {
    use crate::types::{InferencePriority, TupleElement};

    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);

    let t_name = interner.intern_string("T");
    let var_t = ctx.fresh_type_param(t_name, false);
    let t_type = interner.intern(TypeData::TypeParameter(crate::types::TypeParamInfo {
        name: t_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    let source = interner.array(TypeId::NUMBER);

    // Target: [...T, number] — has a fixed trailing element after the rest.
    let target = interner.tuple(vec![
        TupleElement {
            type_id: t_type,
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

    ctx.infer_from_types(source, target, InferencePriority::NakedTypeVariable)
        .unwrap();

    // The single-rest reduction must not fire here. Either no candidate is
    // recorded, or any recorded candidate must come from a different rule —
    // we only assert that the rule under test does not naively bind T to the
    // entire source array.
    let resolved = ctx.resolve_with_constraints(var_t).unwrap_or(TypeId::ERROR);
    assert_ne!(
        resolved, source,
        "single-rest rule must not fire on multi-element variadic tuples"
    );
}

/// Extract Inference Improvement (TypeScript issue #25065): inferring a type
/// parameter K from a source against a target shaped like `Extract<K, U>`
/// (`K extends U ? K : never`) must infer K = source. Without this, K is left
/// unresolved and falls back to its constraint, which produces wrong-shape
/// diagnostics like `Argument of type 'unique symbol' is not assignable to
/// parameter of type 'keyof StrNum'` instead of tsc's
/// `... parameter of type 'never'` on `Extract<K, string>` parameter sites.
#[test]
fn test_inference_through_extract_pattern_conditional() {
    use crate::types::InferencePriority;

    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);

    let k_name = interner.intern_string("K");
    let var_k = ctx.fresh_type_param(k_name, false);
    let k_type = interner.intern(TypeData::TypeParameter(crate::types::TypeParamInfo {
        name: k_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    // Source: a unique symbol stand-in (any concrete type works for this rule;
    // the rule should propagate whatever the argument type is).
    let source = TypeId::SYMBOL;

    // Target: `K extends string ? K : never` — the canonical Extract shape
    // after the type alias is inlined during inference.
    let target = interner.conditional(ConditionalType {
        check_type: k_type,
        extends_type: TypeId::STRING,
        true_type: k_type,
        false_type: TypeId::NEVER,
        is_distributive: true,
    });

    ctx.infer_from_types(source, target, InferencePriority::NakedTypeVariable)
        .unwrap();

    let resolved = ctx.resolve_with_constraints(var_k).unwrap();
    assert_eq!(
        resolved, source,
        "inferring K from `Extract<K, U>`-shaped target must yield K = source"
    );
}

/// Same rule under a different type-parameter name to guarantee the fix is
/// structural and does not depend on `K` (or any other name) being hardcoded.
#[test]
fn test_inference_through_extract_pattern_conditional_alt_name() {
    use crate::types::InferencePriority;

    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);

    // Use a name that has no special meaning in the compiler.
    let p_name = interner.intern_string("P");
    let var_p = ctx.fresh_type_param(p_name, false);
    let p_type = interner.intern(TypeData::TypeParameter(crate::types::TypeParamInfo {
        name: p_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    let source = TypeId::NUMBER;

    // `P extends string ? P : never`
    let target = interner.conditional(ConditionalType {
        check_type: p_type,
        extends_type: TypeId::STRING,
        true_type: p_type,
        false_type: TypeId::NEVER,
        is_distributive: true,
    });

    ctx.infer_from_types(source, target, InferencePriority::NakedTypeVariable)
        .unwrap();

    let resolved = ctx.resolve_with_constraints(var_p).unwrap();
    assert_eq!(
        resolved, source,
        "Extract-pattern inference rule must be structural, not name-dependent"
    );
}

/// The Extract-pattern rule must NOT fire when the conditional is
/// non-distributive (e.g. `[T] extends [U] ? T : never`). Non-distributive
/// conditionals carry different semantics in tsc and must not be reduced to
/// a naked-parameter inference site.
#[test]
fn test_inference_skips_non_distributive_extract_pattern() {
    use crate::types::InferencePriority;

    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);

    let t_name = interner.intern_string("T");
    let var_t = ctx.fresh_type_param(t_name, false);
    let t_type = interner.intern(TypeData::TypeParameter(crate::types::TypeParamInfo {
        name: t_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    let source = TypeId::NUMBER;

    // `T extends string ? T : never` but flagged non-distributive — the rule
    // must not reduce this to inference on T.
    let target = interner.conditional(ConditionalType {
        check_type: t_type,
        extends_type: TypeId::STRING,
        true_type: t_type,
        false_type: TypeId::NEVER,
        is_distributive: false,
    });

    ctx.infer_from_types(source, target, InferencePriority::NakedTypeVariable)
        .unwrap();

    let resolved = ctx.resolve_with_constraints(var_t).unwrap_or(TypeId::ERROR);
    assert_ne!(
        resolved, source,
        "non-distributive conditionals must not be treated as Extract-like \
         inference sites"
    );
}

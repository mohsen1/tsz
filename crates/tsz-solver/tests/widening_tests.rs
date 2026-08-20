use super::*;
use crate::construction::TypeInterner;
use crate::types::{
    LiteralValue, ObjectFlags, OrderedFloat, PropertyInfo, SymbolRef, TypeData, TypeParamInfo,
    Visibility,
};

#[test]
fn test_widen_string_literal() {
    let interner = TypeInterner::new();
    let string_lit = interner.intern(TypeData::Literal(LiteralValue::String(
        interner.intern_string("hello"),
    )));
    let widened = widen_type(
        &interner as &dyn crate::construction::TypeDatabase,
        string_lit,
    );
    assert_eq!(widened, TypeId::STRING);
}

#[test]
fn test_widen_number_literal() {
    let interner = TypeInterner::new();
    let number_lit = interner.intern(TypeData::Literal(LiteralValue::Number(OrderedFloat(42.0))));
    let widened = widen_type(
        &interner as &dyn crate::construction::TypeDatabase,
        number_lit,
    );
    assert_eq!(widened, TypeId::NUMBER);
}

#[test]
fn test_widen_boolean_literal() {
    let interner = TypeInterner::new();
    let bool_lit = interner.intern(TypeData::Literal(LiteralValue::Boolean(true)));
    let widened = widen_type(
        &interner as &dyn crate::construction::TypeDatabase,
        bool_lit,
    );
    assert_eq!(widened, TypeId::BOOLEAN);
}

#[test]
fn test_widen_union() {
    let interner = TypeInterner::new();
    let lit1 = interner.intern(TypeData::Literal(LiteralValue::Number(OrderedFloat(1.0))));
    let lit2 = interner.intern(TypeData::Literal(LiteralValue::Number(OrderedFloat(2.0))));
    let union = interner.union(vec![lit1, lit2]);

    let widened = widen_type(&interner as &dyn crate::construction::TypeDatabase, union);
    // After widening, we get number | number which dedups to number
    assert_eq!(widened, TypeId::NUMBER);
}

#[test]
fn test_widen_primitive_preserved() {
    let interner = TypeInterner::new();
    // Primitives should be preserved (already widened)
    let widened = widen_type(&interner, TypeId::STRING);
    assert_eq!(widened, TypeId::STRING);
}

#[test]
fn test_type_param_not_widened() {
    let interner = TypeInterner::new();
    // Type parameters are NOT widened
    let name = interner.intern_string("T");
    let info = TypeParamInfo {
        name,
        constraint: Some(
            interner.intern(TypeData::Literal(LiteralValue::Number(OrderedFloat(1.0)))),
        ),
        default: None,
        is_const: false,
        origin: crate::types::TypeParamOrigin::User,
    };
    let type_param = interner.intern(TypeData::TypeParameter(info));

    let widened = widen_type(&interner, type_param);
    // Should preserve the original type_param type
    assert_eq!(widened, type_param);
}

#[test]
fn test_widen_unique_symbol() {
    let interner = TypeInterner::new();
    let unique_sym = interner.intern(TypeData::UniqueSymbol(SymbolRef(42)));
    let widened = widen_type(&interner, unique_sym);
    assert_eq!(widened, TypeId::SYMBOL);
}

#[test]
fn test_widen_object_properties() {
    let interner = TypeInterner::new();
    // Create object { x: 1 } where x is a literal number
    let props = vec![PropertyInfo {
        name: interner.intern_string("x"),
        type_id: interner.intern(TypeData::Literal(LiteralValue::Number(OrderedFloat(1.0)))),
        write_type: interner.intern(TypeData::Literal(LiteralValue::Number(OrderedFloat(1.0)))),
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
        non_widening: false,
    }];
    let obj_type = interner.object(props);

    let widened = widen_type(&interner, obj_type);

    // Check that the widened type has number, not the literal 1
    let widened_key = interner.lookup(widened);
    match widened_key {
        Some(TypeData::Object(shape_id) | TypeData::ObjectWithIndex(shape_id)) => {
            let shape = interner.object_shape(shape_id);
            assert_eq!(shape.properties.len(), 1);
            assert_eq!(shape.properties[0].type_id, TypeId::NUMBER);
            assert_eq!(shape.properties[0].write_type, TypeId::NUMBER);
        }
        _ => panic!("Expected widened object type"),
    }
}

#[test]
fn widen_type_memo_hit_replays_late_display_properties() {
    let interner = TypeInterner::new();
    let name = interner.intern_string("x");
    let literal = interner.literal_string("late");
    let obj_type = interner.object(vec![PropertyInfo::new(name, literal)]);

    let widened = widen_type(&interner, obj_type);
    assert_eq!(
        interner.get_display_properties(widened),
        None,
        "first semantic widening ran before display provenance existed"
    );

    interner.store_display_properties(obj_type, vec![PropertyInfo::new(name, literal)]);

    let warmed = widen_type(&interner, obj_type);
    assert_eq!(warmed, widened, "memo hit must preserve semantic result");
    let display_props = interner
        .get_display_properties(warmed)
        .expect("memo hit should replay display properties onto widened result");
    assert_eq!(display_props.len(), 1);
    assert_eq!(display_props[0].name, name);
    assert_eq!(display_props[0].type_id, literal);
}

#[test]
fn test_widen_nested_object_properties() {
    let interner = TypeInterner::new();
    // Create nested object { a: { b: "hello" } }
    let inner_props = vec![PropertyInfo {
        name: interner.intern_string("b"),
        type_id: interner.intern(TypeData::Literal(LiteralValue::String(
            interner.intern_string("hello"),
        ))),
        write_type: interner.intern(TypeData::Literal(LiteralValue::String(
            interner.intern_string("hello"),
        ))),
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
        non_widening: false,
    }];
    let inner_obj = interner.object(inner_props);

    let outer_props = vec![PropertyInfo {
        name: interner.intern_string("a"),
        type_id: inner_obj,
        write_type: inner_obj,
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
        non_widening: false,
    }];
    let outer_obj = interner.object(outer_props);

    let widened = widen_type(&interner, outer_obj);

    // Check that both inner and outer properties are widened
    let widened_key = interner.lookup(widened);
    match widened_key {
        Some(TypeData::Object(shape_id) | TypeData::ObjectWithIndex(shape_id)) => {
            let shape = interner.object_shape(shape_id);
            assert_eq!(shape.properties.len(), 1);

            // Outer property 'a' should be an object
            let inner_type = shape.properties[0].type_id;
            let inner_key = interner.lookup(inner_type);
            match inner_key {
                Some(
                    TypeData::Object(inner_shape_id) | TypeData::ObjectWithIndex(inner_shape_id),
                ) => {
                    let inner_shape = interner.object_shape(inner_shape_id);
                    assert_eq!(inner_shape.properties.len(), 1);
                    // Inner property 'b' should be widened to string
                    assert_eq!(inner_shape.properties[0].type_id, TypeId::STRING);
                }
                _ => panic!("Expected inner object type"),
            }
        }
        _ => panic!("Expected widened object type"),
    }
}

#[test]
fn test_widen_readonly_property_preserved() {
    let interner = TypeInterner::new();
    // { a: 1, readonly b: 2 }
    let props = vec![
        PropertyInfo {
            name: interner.intern_string("a"),
            type_id: interner.intern(TypeData::Literal(LiteralValue::Number(OrderedFloat(1.0)))),
            write_type: interner.intern(TypeData::Literal(LiteralValue::Number(OrderedFloat(1.0)))),
            optional: false,
            readonly: false, // Mutable -> Widens
            is_method: false,
            is_class_prototype: false,
            visibility: Visibility::Public,
            parent_id: None,
            declaration_order: 0,
            is_string_named: false,
            is_symbol_named: false,
            single_quoted_name: false,
            non_widening: false,
        },
        PropertyInfo {
            name: interner.intern_string("b"),
            type_id: interner.intern(TypeData::Literal(LiteralValue::Number(OrderedFloat(2.0)))),
            write_type: interner.intern(TypeData::Literal(LiteralValue::Number(OrderedFloat(2.0)))),
            optional: false,
            readonly: true, // Readonly -> Preserved
            is_method: false,
            is_class_prototype: false,
            visibility: Visibility::Public,
            parent_id: None,
            declaration_order: 0,
            is_string_named: false,
            is_symbol_named: false,
            single_quoted_name: false,
            non_widening: false,
        },
    ];
    let obj_type = interner.object(props);
    let widened = widen_type(&interner, obj_type);

    // Verify 'a' is number, 'b' is literal 2
    let shape = match interner.lookup(widened).unwrap() {
        TypeData::Object(id) => interner.object_shape(id),
        _ => panic!("Expected object"),
    };

    let a = shape
        .properties
        .iter()
        .find(|p| interner.resolve_atom(p.name) == "a")
        .unwrap();
    let b = shape
        .properties
        .iter()
        .find(|p| interner.resolve_atom(p.name) == "b")
        .unwrap();

    assert_eq!(a.type_id, TypeId::NUMBER);
    assert!(matches!(
        interner.lookup(b.type_id),
        Some(TypeData::Literal(_))
    ));
}

#[test]
fn test_widen_readonly_nested_object_widens_inner_literals() {
    // Synthetic readonly property shape: the outer `nested` is readonly,
    // but the INNER `p: 1` still widens to `number`. The readonly modifier
    // only preserves primitive literals on its direct value, not literals
    // nested inside compound types.
    let interner = TypeInterner::new();
    let inner_props = vec![PropertyInfo {
        name: interner.intern_string("p"),
        type_id: interner.literal_number(1.0),
        write_type: interner.literal_number(1.0),
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
        non_widening: false,
    }];
    let inner_obj = interner.object(inner_props);

    let outer_props = vec![PropertyInfo {
        name: interner.intern_string("nested"),
        type_id: inner_obj,
        write_type: inner_obj,
        optional: false,
        readonly: true,
        is_method: false,
        is_class_prototype: false,
        visibility: Visibility::Public,
        parent_id: None,
        declaration_order: 0,
        is_string_named: false,
        is_symbol_named: false,
        single_quoted_name: false,
        non_widening: false,
    }];
    let outer_obj = interner.object(outer_props);

    let widened = widen_type(&interner, outer_obj);
    let outer_shape = match interner.lookup(widened).unwrap() {
        TypeData::Object(id) | TypeData::ObjectWithIndex(id) => interner.object_shape(id),
        _ => panic!("Expected outer object"),
    };
    assert_eq!(outer_shape.properties.len(), 1);
    assert!(
        outer_shape.properties[0].readonly,
        "outer 'nested' should still be readonly"
    );
    let inner_shape = match interner.lookup(outer_shape.properties[0].type_id).unwrap() {
        TypeData::Object(id) | TypeData::ObjectWithIndex(id) => interner.object_shape(id),
        _ => panic!("Expected inner object even on readonly parent"),
    };
    assert_eq!(
        inner_shape.properties[0].type_id,
        TypeId::NUMBER,
        "inner 'p' must widen to number even when its parent property is readonly"
    );
}

#[test]
fn test_widen_readonly_array_widens_element_type() {
    // Synthetic readonly array property: the inner element literals widen
    // even though the outer property is readonly.
    let interner = TypeInterner::new();
    let lit1 = interner.literal_number(1.0);
    let lit2 = interner.literal_number(2.0);
    let lit3 = interner.literal_number(3.0);
    let union = interner.union(vec![lit1, lit2, lit3]);
    let arr = interner.array(union);

    let outer_props = vec![PropertyInfo {
        name: interner.intern_string("arr"),
        type_id: arr,
        write_type: arr,
        optional: false,
        readonly: true,
        is_method: false,
        is_class_prototype: false,
        visibility: Visibility::Public,
        parent_id: None,
        declaration_order: 0,
        is_string_named: false,
        is_symbol_named: false,
        single_quoted_name: false,
        non_widening: false,
    }];
    let outer_obj = interner.object(outer_props);

    let widened = widen_type(&interner, outer_obj);
    let outer_shape = match interner.lookup(widened).unwrap() {
        TypeData::Object(id) | TypeData::ObjectWithIndex(id) => interner.object_shape(id),
        _ => panic!("Expected outer object"),
    };
    assert!(outer_shape.properties[0].readonly);
    let elem = match interner.lookup(outer_shape.properties[0].type_id).unwrap() {
        TypeData::Array(e) => e,
        _ => panic!("Expected array on readonly property"),
    };
    assert_eq!(
        elem,
        TypeId::NUMBER,
        "array element widens to number even when the array property is readonly"
    );
}

// ============================================================================
// Additional widening helper coverage
//
// These tests cover the public widening surface beyond the basic `widen_type`
// path. Each test pins a single behavior so a future drift surfaces a single
// failure rather than a cascade.
// ============================================================================

use crate::types::{
    CallSignature, CallableShape, FunctionShape, IndexSignature, ObjectShape, ParamInfo,
    TemplateSpan, TupleElement,
};

// -------- widen_type: bigint and boolean intrinsic edge cases ----------------

#[test]
fn test_widen_bigint_literal_to_bigint() {
    let interner = TypeInterner::new();
    let bigint_atom = interner.intern_string("42");
    let bigint_lit = interner.intern(TypeData::Literal(LiteralValue::BigInt(bigint_atom)));
    let widened = widen_type(&interner, bigint_lit);
    assert_eq!(widened, TypeId::BIGINT);
}

#[test]
fn test_widen_boolean_true_intrinsic_to_boolean() {
    let interner = TypeInterner::new();
    let widened = widen_type(&interner, TypeId::BOOLEAN_TRUE);
    assert_eq!(widened, TypeId::BOOLEAN);
}

#[test]
fn test_widen_boolean_false_intrinsic_to_boolean() {
    let interner = TypeInterner::new();
    let widened = widen_type(&interner, TypeId::BOOLEAN_FALSE);
    assert_eq!(widened, TypeId::BOOLEAN);
}

#[test]
fn test_widen_array_of_literals_widens_element() {
    let interner = TypeInterner::new();
    let lit = interner.literal_number(1.0);
    let arr = interner.array(lit);
    let widened = widen_type(&interner, arr);
    match interner.lookup(widened) {
        Some(TypeData::Array(elem)) => assert_eq!(elem, TypeId::NUMBER),
        other => panic!("Expected Array(NUMBER), got {other:?}"),
    }
}

#[test]
fn test_widen_array_of_primitives_returns_same_typeid() {
    let interner = TypeInterner::new();
    // string[] should be returned unchanged (already widened)
    let arr = interner.array(TypeId::STRING);
    let widened = widen_type(&interner, arr);
    assert_eq!(widened, arr);
}

#[test]
fn test_widen_tuple_of_literals_widens_each_element() {
    let interner = TypeInterner::new();
    let one = interner.literal_number(1.0);
    let two = interner.literal_number(2.0);
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
    ]);
    let widened = widen_type(&interner, tuple);
    match interner.lookup(widened) {
        Some(TypeData::Tuple(list_id)) => {
            let elements = interner.tuple_list(list_id);
            assert_eq!(elements.len(), 2);
            assert_eq!(elements[0].type_id, TypeId::NUMBER);
            assert_eq!(elements[1].type_id, TypeId::NUMBER);
        }
        other => panic!("Expected widened tuple, got {other:?}"),
    }
}

#[test]
fn test_widen_intrinsic_string_returns_self() {
    // Non-boolean intrinsics short-circuit and do not allocate.
    let interner = TypeInterner::new();
    assert_eq!(widen_type(&interner, TypeId::STRING), TypeId::STRING);
    assert_eq!(widen_type(&interner, TypeId::NUMBER), TypeId::NUMBER);
    assert_eq!(widen_type(&interner, TypeId::BIGINT), TypeId::BIGINT);
    assert_eq!(widen_type(&interner, TypeId::ANY), TypeId::ANY);
    assert_eq!(widen_type(&interner, TypeId::UNKNOWN), TypeId::UNKNOWN);
}

#[test]
fn test_widen_function_returns_self_via_general_widen_type() {
    // widen_type's fast path skips Function entirely (preserves contravariant
    // parameter positions). Even though the param is a literal, the function
    // type is returned unchanged.
    let interner = TypeInterner::new();
    let lit = interner.literal_number(1.0);
    let func = interner.function(FunctionShape {
        type_params: Vec::new(),
        params: vec![ParamInfo {
            name: Some(interner.intern_string("x")),
            type_id: lit,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: lit,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });
    let widened = widen_type(&interner, func);
    assert_eq!(widened, func);
}

// -------- widen_type_for_display: preserves boolean literals -----------------

#[test]
fn test_widen_type_for_display_preserves_boolean_true() {
    // For diagnostic display, BOOLEAN_TRUE must NOT widen to BOOLEAN so that
    // narrowed types like `string | false` render correctly.
    let interner = TypeInterner::new();
    let widened = widen_type_for_display(&interner, TypeId::BOOLEAN_TRUE);
    assert_eq!(widened, TypeId::BOOLEAN_TRUE);
}

#[test]
fn test_widen_type_for_display_preserves_boolean_false() {
    let interner = TypeInterner::new();
    let widened = widen_type_for_display(&interner, TypeId::BOOLEAN_FALSE);
    assert_eq!(widened, TypeId::BOOLEAN_FALSE);
}

#[test]
fn test_widen_type_for_display_widens_string_literal() {
    let interner = TypeInterner::new();
    let lit = interner.literal_string("hi");
    assert_eq!(widen_type_for_display(&interner, lit), TypeId::STRING);
}

#[test]
fn test_widen_type_for_display_widens_number_literal() {
    let interner = TypeInterner::new();
    let lit = interner.literal_number(7.0);
    assert_eq!(widen_type_for_display(&interner, lit), TypeId::NUMBER);
}

#[test]
fn test_widen_type_for_display_does_not_recurse_into_function_params() {
    // Function param types are preserved by display widening (widen_functions=false).
    let interner = TypeInterner::new();
    let lit = interner.literal_string("foo");
    let func = interner.function(FunctionShape {
        type_params: Vec::new(),
        params: vec![ParamInfo {
            name: Some(interner.intern_string("x")),
            type_id: lit,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });
    let widened = widen_type_for_display(&interner, func);
    // Function returned unchanged
    assert_eq!(widened, func);
}

// -------- widen_type_deep: recurses into function signatures -----------------

#[test]
fn test_widen_type_deep_recurses_into_function_param_and_return() {
    let interner = TypeInterner::new();
    let lit_string = interner.literal_string("foo");
    let lit_number = interner.literal_number(3.0);
    let func = interner.function(FunctionShape {
        type_params: Vec::new(),
        params: vec![ParamInfo {
            name: Some(interner.intern_string("x")),
            type_id: lit_string,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: lit_number,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });
    let widened = widen_type_deep(&interner, func);
    match interner.lookup(widened) {
        Some(TypeData::Function(shape_id)) => {
            let shape = interner.function_shape(shape_id);
            assert_eq!(shape.params[0].type_id, TypeId::STRING);
            assert_eq!(shape.return_type, TypeId::NUMBER);
        }
        other => panic!("Expected widened function, got {other:?}"),
    }
}

#[test]
fn test_widen_type_deep_intrinsic_short_circuit() {
    let interner = TypeInterner::new();
    assert_eq!(widen_type_deep(&interner, TypeId::STRING), TypeId::STRING);
    assert_eq!(widen_type_deep(&interner, TypeId::ANY), TypeId::ANY);
}

#[test]
fn test_widen_type_deep_widens_boolean_intrinsics() {
    // Like widen_type, deep widening still flips boolean true/false intrinsics.
    let interner = TypeInterner::new();
    assert_eq!(
        widen_type_deep(&interner, TypeId::BOOLEAN_TRUE),
        TypeId::BOOLEAN
    );
    assert_eq!(
        widen_type_deep(&interner, TypeId::BOOLEAN_FALSE),
        TypeId::BOOLEAN
    );
}

// -------- widen_type_for_inference (pub(crate)) ------------------------------

#[test]
fn test_widen_type_for_inference_widens_top_level_literal() {
    let interner = TypeInterner::new();
    let lit = interner.literal_number(5.0);
    assert_eq!(widen_type_for_inference(&interner, lit), TypeId::NUMBER);
}

#[test]
fn test_widen_type_for_inference_does_not_recurse_into_function() {
    // Inference widening must NOT widen function param/return types — that
    // creates contravariant mismatches for strict function types.
    let interner = TypeInterner::new();
    let lit_string = interner.literal_string("foo");
    let func = interner.function(FunctionShape {
        type_params: Vec::new(),
        params: vec![ParamInfo {
            name: Some(interner.intern_string("x")),
            type_id: lit_string,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });
    let widened = widen_type_for_inference(&interner, func);
    assert_eq!(widened, func);
}

#[test]
fn test_widen_type_for_inference_preserves_unique_symbol() {
    // tsc's `getWidenedLiteralType` never widens a `unique symbol`, so a
    // const-bound unique symbol inferred into a type-parameter position must be
    // preserved — unlike the mutable-location/display widener, which applies
    // `getWidenedUniqueESSymbolType` (`unique symbol` -> `symbol`). Regression
    // guard for jotai's `[typeof RESET]` rest tuple collapsing to `[symbol]`.
    let interner = TypeInterner::new();
    let unique_sym = interner.intern(TypeData::UniqueSymbol(SymbolRef(7)));

    // Mutable-location / display widening still collapses to `symbol`.
    assert_eq!(widen_type(&interner, unique_sym), TypeId::SYMBOL);
    assert_eq!(
        widen_type_for_mutable_binding(&interner, unique_sym),
        TypeId::SYMBOL
    );

    // Inference-position widening preserves it, both bare and nested in a tuple
    // element (the jotai `[typeof RESET]` shape).
    assert_eq!(widen_type_for_inference(&interner, unique_sym), unique_sym);
    let tuple = interner.tuple(vec![TupleElement {
        type_id: unique_sym,
        name: None,
        optional: false,
        rest: false,
    }]);
    assert_eq!(widen_type_for_inference(&interner, tuple), tuple);
}

// -------- widen_object_literal_properties (pub(crate)) -----------------------

fn mutable_lit_prop(interner: &TypeInterner, name: &str, lit: TypeId) -> PropertyInfo {
    PropertyInfo {
        name: interner.intern_string(name),
        type_id: lit,
        write_type: lit,
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
        non_widening: false,
    }
}

#[test]
fn test_widen_object_literal_properties_widens_fresh_mutable_props() {
    let interner = TypeInterner::new();
    let lit = interner.literal_number(1.0);
    let props = vec![mutable_lit_prop(&interner, "x", lit)];
    // Fresh object literal carries the widening flag, so mutable props widen.
    let obj = interner.object_with_flags(props, ObjectFlags::FRESH_LITERAL);
    let widened = widen_object_literal_properties(&interner, obj);
    match interner.lookup(widened) {
        Some(TypeData::Object(shape_id) | TypeData::ObjectWithIndex(shape_id)) => {
            let shape = interner.object_shape(shape_id);
            assert_eq!(shape.properties[0].type_id, TypeId::NUMBER);
        }
        other => panic!("Expected widened object, got {other:?}"),
    }
}

#[test]
fn test_widen_object_literal_properties_preserves_non_fresh_props() {
    // A non-fresh object (declared/annotated type, alias instance, or
    // object-spread result) keeps its literal property types, matching tsc's
    // `getWidenedType`, which only widens types with `ContainsWideningType`.
    let interner = TypeInterner::new();
    let lit = interner.literal_number(1.0);
    let props = vec![mutable_lit_prop(&interner, "x", lit)];
    let obj = interner.object(props);
    let widened = widen_object_literal_properties(&interner, obj);
    assert_eq!(widened, obj, "non-fresh object must be returned unchanged");
    match interner.lookup(widened) {
        Some(TypeData::Object(shape_id) | TypeData::ObjectWithIndex(shape_id)) => {
            let shape = interner.object_shape(shape_id);
            assert_eq!(shape.properties[0].type_id, lit);
        }
        other => panic!("Expected object, got {other:?}"),
    }
}

#[test]
fn test_widen_object_literal_properties_skips_top_level_union() {
    // A top-level union of string literals must NOT be widened by this helper.
    let interner = TypeInterner::new();
    let a = interner.literal_string("a");
    let b = interner.literal_string("b");
    let union = interner.union(vec![a, b]);
    let widened = widen_object_literal_properties(&interner, union);
    assert_eq!(widened, union);
}

#[test]
fn test_widen_object_literal_properties_skips_top_level_literal() {
    // Direct literal should pass through unchanged (only enters objects).
    let interner = TypeInterner::new();
    let lit = interner.literal_string("foo");
    let widened = widen_object_literal_properties(&interner, lit);
    assert_eq!(widened, lit);
}

#[test]
fn test_widen_object_literal_properties_preserves_readonly() {
    let interner = TypeInterner::new();
    let lit = interner.literal_number(2.0);
    let props = vec![PropertyInfo {
        name: interner.intern_string("y"),
        type_id: lit,
        write_type: lit,
        optional: false,
        readonly: true,
        is_method: false,
        is_class_prototype: false,
        visibility: Visibility::Public,
        parent_id: None,
        declaration_order: 0,
        is_string_named: false,
        is_symbol_named: false,
        single_quoted_name: false,
        non_widening: false,
    }];
    let obj = interner.object(props);
    let widened = widen_object_literal_properties(&interner, obj);
    let shape = match interner.lookup(widened).unwrap() {
        TypeData::Object(id) | TypeData::ObjectWithIndex(id) => interner.object_shape(id),
        _ => panic!("Expected object"),
    };
    // readonly literal preserved
    assert!(matches!(
        interner.lookup(shape.properties[0].type_id),
        Some(TypeData::Literal(_))
    ));
}

// -------- get_base_type_for_comparison ---------------------------------------

#[test]
fn test_get_base_type_for_comparison_string_literal() {
    let interner = TypeInterner::new();
    let lit = interner.literal_string("abc");
    assert_eq!(get_base_type_for_comparison(&interner, lit), TypeId::STRING);
}

#[test]
fn test_get_base_type_for_comparison_number_literal() {
    let interner = TypeInterner::new();
    let lit = interner.literal_number(3.0);
    assert_eq!(get_base_type_for_comparison(&interner, lit), TypeId::NUMBER);
}

#[test]
fn test_get_base_type_for_comparison_boolean_literal() {
    let interner = TypeInterner::new();
    let lit = interner.intern(TypeData::Literal(LiteralValue::Boolean(true)));
    assert_eq!(
        get_base_type_for_comparison(&interner, lit),
        TypeId::BOOLEAN
    );
}

#[test]
fn test_get_base_type_for_comparison_template_literal_returns_string() {
    let interner = TypeInterner::new();
    // Template literal `${string}` (one type span) must collapse to STRING.
    let template =
        interner.template_literal(vec![TemplateSpan::Text(interner.intern_string("hi"))]);
    // Pure-text template literal may be normalized to a string literal; in
    // either case the comparison base must be string.
    assert_eq!(
        get_base_type_for_comparison(&interner, template),
        TypeId::STRING
    );
}

#[test]
fn test_get_base_type_for_comparison_string_intrinsic_returns_string() {
    use crate::types::StringIntrinsicKind;
    let interner = TypeInterner::new();
    let lit = interner.literal_string("foo");
    let upper = interner.intern(TypeData::StringIntrinsic {
        kind: StringIntrinsicKind::Uppercase,
        type_arg: lit,
    });
    assert_eq!(
        get_base_type_for_comparison(&interner, upper),
        TypeId::STRING
    );
}

#[test]
fn test_get_base_type_for_comparison_type_param_with_constraint() {
    let interner = TypeInterner::new();
    let a = interner.literal_string("a");
    let b = interner.literal_string("b");
    let constraint = interner.union(vec![a, b]);
    let info = TypeParamInfo {
        name: interner.intern_string("T"),
        constraint: Some(constraint),
        default: None,
        is_const: false,
        origin: crate::types::TypeParamOrigin::User,
    };
    let tp = interner.intern(TypeData::TypeParameter(info));
    // T extends "a" | "b" → comparison base is string (collapse via union)
    assert_eq!(get_base_type_for_comparison(&interner, tp), TypeId::STRING);
}

#[test]
fn test_get_base_type_for_comparison_type_param_no_constraint_unchanged() {
    let interner = TypeInterner::new();
    let info = TypeParamInfo {
        name: interner.intern_string("T"),
        constraint: None,
        default: None,
        is_const: false,
        origin: crate::types::TypeParamOrigin::User,
    };
    let tp = interner.intern(TypeData::TypeParameter(info));
    assert_eq!(get_base_type_for_comparison(&interner, tp), tp);
}

#[test]
fn test_get_base_type_for_comparison_union_of_literals() {
    let interner = TypeInterner::new();
    let s = interner.literal_string("x");
    let n = interner.literal_number(1.0);
    let union = interner.union(vec![s, n]);
    let mapped = get_base_type_for_comparison(&interner, union);
    // Result is union(string, number) — order/dedup not guaranteed by us;
    // verify it contains both via a structural lookup.
    let members = match interner.lookup(mapped) {
        Some(TypeData::Union(list_id)) => interner.type_list(list_id).to_vec(),
        Some(_) => vec![mapped],
        None => panic!("Expected mapped type to be in interner"),
    };
    assert!(members.contains(&TypeId::STRING));
    assert!(members.contains(&TypeId::NUMBER));
}

#[test]
fn test_get_base_type_for_comparison_passthrough_for_unrelated() {
    let interner = TypeInterner::new();
    // Object types fall through unchanged.
    let props = vec![PropertyInfo {
        name: interner.intern_string("x"),
        type_id: TypeId::NUMBER,
        write_type: TypeId::NUMBER,
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
        non_widening: false,
    }];
    let obj = interner.object(props);
    assert_eq!(get_base_type_for_comparison(&interner, obj), obj);
}

// -------- widen_literal_type -------------------------------------------------

#[test]
fn test_widen_literal_type_string_literal() {
    let interner = TypeInterner::new();
    let lit = interner.literal_string("foo");
    assert_eq!(widen_literal_type(&interner, lit), TypeId::STRING);
}

#[test]
fn test_widen_literal_type_number_literal() {
    let interner = TypeInterner::new();
    let lit = interner.literal_number(0.0);
    assert_eq!(widen_literal_type(&interner, lit), TypeId::NUMBER);
}

#[test]
fn test_widen_literal_type_boolean_literal_value() {
    let interner = TypeInterner::new();
    let lit = interner.intern(TypeData::Literal(LiteralValue::Boolean(false)));
    assert_eq!(widen_literal_type(&interner, lit), TypeId::BOOLEAN);
}

#[test]
fn test_widen_literal_type_boolean_intrinsic_true_value() {
    let interner = TypeInterner::new();
    assert_eq!(
        widen_literal_type(&interner, TypeId::BOOLEAN_TRUE),
        TypeId::BOOLEAN
    );
}

#[test]
fn test_widen_literal_type_bigint_literal() {
    let interner = TypeInterner::new();
    let bigint_atom = interner.intern_string("100");
    let lit = interner.intern(TypeData::Literal(LiteralValue::BigInt(bigint_atom)));
    assert_eq!(widen_literal_type(&interner, lit), TypeId::BIGINT);
}

#[test]
fn widen_literal_type_terminates_on_cyclic_union_origin() {
    let interner = TypeInterner::new();
    let db = &interner as &dyn crate::construction::TypeDatabase;
    let lit1 = interner.literal_number(1.0);
    let lit2 = interner.literal_number(2.0);
    let union = interner.union(vec![lit1, lit2]);

    // Record a self-referential display origin: the union lists itself as one of
    // its origin members. `widen_literal_type` recurses through origin members,
    // so before the on-stack cycle guard this overflowed the stack (the mobx
    // project-compile crash). It must now terminate.
    display_provenance::record_union_origin(
        db,
        UnionOriginProvenance {
            union_type_id: union,
            origin_members: vec![union, lit1, lit2],
        },
    );

    let widened = widen_literal_type(db, union);

    // Terminated (no stack overflow). The self-reference is returned unchanged
    // rather than recursed into, and the number literals widen to `number`;
    // unioning the widened members collapses to `number`.
    assert_eq!(widened, TypeId::NUMBER);
}

#[test]
fn test_widen_literal_type_union_maps_each_member() {
    let interner = TypeInterner::new();
    let s = interner.literal_string("x");
    let n = interner.literal_number(1.0);
    let union = interner.union(vec![s, n]);
    let mapped = widen_literal_type(&interner, union);
    let members = match interner.lookup(mapped) {
        Some(TypeData::Union(list_id)) => interner.type_list(list_id).to_vec(),
        Some(_) => vec![mapped],
        None => panic!("Expected mapped type"),
    };
    assert!(members.contains(&TypeId::STRING));
    assert!(members.contains(&TypeId::NUMBER));
}

#[test]
fn test_widen_literal_type_union_without_literals_noop() {
    let interner = TypeInterner::new();
    let union = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);

    assert_eq!(widen_literal_type(&interner, union), union);
}

#[test]
fn test_widen_literal_type_object_passthrough() {
    // Unlike get_base_type_for_comparison, widen_literal_type does NOT recurse
    // into objects (returns top-level type unchanged for non-literal/non-union).
    let interner = TypeInterner::new();
    let props = vec![PropertyInfo {
        name: interner.intern_string("x"),
        type_id: interner.literal_number(1.0),
        write_type: interner.literal_number(1.0),
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
        non_widening: false,
    }];
    let obj = interner.object(props);
    assert_eq!(widen_literal_type(&interner, obj), obj);
}

#[test]
fn test_widen_literal_type_primitive_passthrough() {
    let interner = TypeInterner::new();
    assert_eq!(
        widen_literal_type(&interner, TypeId::STRING),
        TypeId::STRING
    );
    assert_eq!(
        widen_literal_type(&interner, TypeId::NUMBER),
        TypeId::NUMBER
    );
    assert_eq!(
        widen_literal_type(&interner, TypeId::BOOLEAN),
        TypeId::BOOLEAN
    );
}

// -------- widen_non_string_bigint_literal (pub(crate)) -----------------------

#[test]
fn test_widen_non_string_bigint_number_literal_widened() {
    let interner = TypeInterner::new();
    let lit = interner.literal_number(7.0);
    assert_eq!(
        widen_non_string_bigint_literal(&interner, lit),
        TypeId::NUMBER
    );
}

#[test]
fn test_widen_non_string_bigint_boolean_literal_widened() {
    let interner = TypeInterner::new();
    let lit = interner.intern(TypeData::Literal(LiteralValue::Boolean(true)));
    assert_eq!(
        widen_non_string_bigint_literal(&interner, lit),
        TypeId::BOOLEAN
    );
}

#[test]
fn test_widen_non_string_bigint_string_literal_preserved() {
    // String literals are preserved by this helper for TS2367 message text.
    let interner = TypeInterner::new();
    let lit = interner.literal_string("foo");
    assert_eq!(widen_non_string_bigint_literal(&interner, lit), lit);
}

#[test]
fn test_widen_non_string_bigint_bigint_literal_preserved() {
    let interner = TypeInterner::new();
    let bigint_atom = interner.intern_string("123");
    let lit = interner.intern(TypeData::Literal(LiteralValue::BigInt(bigint_atom)));
    assert_eq!(widen_non_string_bigint_literal(&interner, lit), lit);
}

#[test]
fn test_widen_non_string_bigint_non_literal_passthrough() {
    let interner = TypeInterner::new();
    assert_eq!(
        widen_non_string_bigint_literal(&interner, TypeId::ANY),
        TypeId::ANY
    );
    assert_eq!(
        widen_non_string_bigint_literal(&interner, TypeId::STRING),
        TypeId::STRING
    );
}

// -------- apply_const_assertion ----------------------------------------------

#[test]
fn test_apply_const_assertion_array_becomes_readonly_array() {
    // Declared arrays keep array shape inside the ReadonlyType wrapper.
    let interner = TypeInterner::new();
    let lit = interner.literal_number(1.0);
    let arr = interner.array(lit);
    let result = apply_const_assertion(&interner, arr);
    let array_inner = match interner.lookup(result) {
        Some(TypeData::ReadonlyType(inner)) => inner,
        other => panic!("Expected ReadonlyType, got {other:?}"),
    };
    let element = match interner.lookup(array_inner) {
        Some(TypeData::Array(element)) => element,
        other => panic!("Expected Array, got {other:?}"),
    };
    assert_eq!(element, lit);
}

#[test]
fn test_apply_const_assertion_tuple_marked_readonly() {
    let interner = TypeInterner::new();
    let lit = interner.literal_string("a");
    let tuple = interner.tuple(vec![TupleElement {
        type_id: lit,
        name: None,
        optional: false,
        rest: false,
    }]);
    let result = apply_const_assertion(&interner, tuple);
    // Tuples are wrapped in ReadonlyType
    assert!(matches!(
        interner.lookup(result),
        Some(TypeData::ReadonlyType(_))
    ));
}

#[test]
fn test_apply_const_assertion_object_marks_props_readonly() {
    let interner = TypeInterner::new();
    let lit = interner.literal_number(1.0);
    let props = vec![PropertyInfo {
        name: interner.intern_string("x"),
        type_id: lit,
        write_type: lit,
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
        non_widening: false,
    }];
    let obj = interner.object(props);
    let result = apply_const_assertion(&interner, obj);
    let shape = match interner.lookup(result) {
        Some(TypeData::Object(id) | TypeData::ObjectWithIndex(id)) => interner.object_shape(id),
        other => panic!("Expected object, got {other:?}"),
    };
    assert_eq!(shape.properties.len(), 1);
    assert!(shape.properties[0].readonly, "property must be readonly");
    // Literal value is preserved (not widened)
    assert_eq!(shape.properties[0].type_id, lit);
}

#[test]
fn test_apply_const_assertion_object_preserves_freshness_and_display_provenance() {
    // A const assertion keeps the operand's fresh object-literal identity in
    // tsc (`FreshLiteral` survives `checkAssertionWorker`), so the readonly
    // rebuild must preserve the shape's flags — and any display-provenance
    // properties must be carried forward with the assertion applied
    // (readonly), not as the mutable pre-assertion snapshot.
    let interner = TypeInterner::new();
    let lit = interner.literal_number(1.0);
    let name = interner.intern_string("qq");
    let props = vec![PropertyInfo::new(name, lit)];
    let fresh = interner.object_with_flags(props.clone(), ObjectFlags::FRESH_LITERAL);
    interner.store_display_properties(fresh, props);

    let result = apply_const_assertion(&interner, fresh);
    assert_ne!(result, fresh, "readonly rebuild must produce a new type");
    let shape = match interner.lookup(result) {
        Some(TypeData::Object(id)) => interner.object_shape(id),
        other => panic!("Expected object, got {other:?}"),
    };
    assert!(
        shape.flags.contains(ObjectFlags::FRESH_LITERAL),
        "const assertion must not launder FRESH_LITERAL"
    );
    assert!(shape.properties[0].readonly);

    let display = interner
        .get_display_properties(result)
        .expect("display provenance must carry forward");
    assert!(
        display[0].readonly,
        "carried display properties must be const-asserted (readonly)"
    );
    assert_eq!(display[0].type_id, lit);
}

#[test]
fn test_apply_const_assertion_non_fresh_object_stays_non_fresh() {
    // Flag preservation is symmetric: a non-fresh object shape does not GAIN
    // freshness through a const assertion.
    let interner = TypeInterner::new();
    let lit = interner.literal_number(2.0);
    let name = interner.intern_string("zz");
    let obj = interner.object(vec![PropertyInfo::new(name, lit)]);
    let result = apply_const_assertion(&interner, obj);
    let shape = match interner.lookup(result) {
        Some(TypeData::Object(id)) => interner.object_shape(id),
        other => panic!("Expected object, got {other:?}"),
    };
    assert!(
        !shape.flags.contains(ObjectFlags::FRESH_LITERAL),
        "non-fresh input must stay non-fresh"
    );
    assert!(shape.properties[0].readonly);
}

#[test]
fn test_apply_const_assertion_literal_preserved() {
    // Top-level literals pass through unchanged — `as const` does not widen.
    let interner = TypeInterner::new();
    let lit = interner.literal_number(42.0);
    assert_eq!(apply_const_assertion(&interner, lit), lit);
}

#[test]
fn test_apply_const_assertion_intrinsic_preserved() {
    let interner = TypeInterner::new();
    assert_eq!(
        apply_const_assertion(&interner, TypeId::NUMBER),
        TypeId::NUMBER
    );
    assert_eq!(apply_const_assertion(&interner, TypeId::ANY), TypeId::ANY);
}

// --- widen_annotation_literals_for_display (#13075) ---

fn annotation_widen_all(interner: &TypeInterner, type_id: TypeId) -> AnnotationWideningOutcome {
    widen_annotation_literals_for_display(interner, type_id, AnnotationLiteralWideningPolicy::ALL)
}

fn property_type(interner: &TypeInterner, shape: &crate::types::ObjectShape, name: &str) -> TypeId {
    let atom = interner.intern_string(name);
    shape
        .properties
        .iter()
        .find(|prop| prop.name == atom)
        .unwrap_or_else(|| panic!("property {name} not found"))
        .type_id
}

#[test]
fn annotation_widen_object_property_literals() {
    let interner = TypeInterner::new();
    let props = vec![
        PropertyInfo::new(interner.intern_string("s"), interner.literal_string("x")),
        PropertyInfo::new(interner.intern_string("n"), interner.literal_number(1.0)),
        PropertyInfo::new(interner.intern_string("b"), TypeId::BOOLEAN_TRUE),
    ];
    let obj = interner.object(props);
    let outcome = annotation_widen_all(&interner, obj);
    assert!(!outcome.display_residue);
    let shape = match interner.lookup(outcome.type_id) {
        Some(TypeData::Object(id)) => interner.object_shape(id),
        other => panic!("expected object, got {other:?}"),
    };
    assert_eq!(property_type(&interner, &shape, "s"), TypeId::STRING);
    assert_eq!(property_type(&interner, &shape, "n"), TypeId::NUMBER);
    assert_eq!(property_type(&interner, &shape, "b"), TypeId::BOOLEAN);
}

#[test]
fn annotation_widen_preserves_non_widening_property_literals() {
    let interner = TypeInterner::new();
    let pinned = interner.literal_number(1.0);
    let mut pinned_prop = PropertyInfo::new(interner.intern_string("pinned"), pinned);
    pinned_prop.non_widening = true;
    let obj = interner.object(vec![
        pinned_prop,
        PropertyInfo::new(
            interner.intern_string("fresh"),
            interner.literal_number(2.0),
        ),
    ]);

    let outcome = annotation_widen_all(&interner, obj);
    assert!(!outcome.display_residue);
    let shape = match interner.lookup(outcome.type_id) {
        Some(TypeData::Object(id)) => interner.object_shape(id),
        other => panic!("expected object, got {other:?}"),
    };
    assert_eq!(property_type(&interner, &shape, "pinned"), pinned);
    assert_eq!(property_type(&interner, &shape, "fresh"), TypeId::NUMBER);
}

#[test]
fn annotation_widen_non_widening_display_only_has_no_residue() {
    let interner = TypeInterner::new();
    let pinned = interner.literal_number(1.0);
    let mut pinned_prop = PropertyInfo::new(interner.intern_string("pinned"), pinned);
    pinned_prop.non_widening = true;
    let obj = interner.object(vec![pinned_prop.clone()]);
    interner.store_display_properties(obj, vec![pinned_prop]);

    let outcome = annotation_widen_all(&interner, obj);
    assert_eq!(outcome.type_id, obj);
    assert!(
        !outcome.display_residue,
        "a non-widening display literal is canonical, not disposable residue"
    );
}

#[test]
fn annotation_widen_mixed_display_properties_preserve_only_non_widening_literal() {
    let interner = TypeInterner::new();
    let pinned = interner.literal_number(1.0);
    let mut canonical_pinned = PropertyInfo::new(interner.intern_string("pinned"), pinned);
    canonical_pinned.non_widening = true;
    let obj = interner.object(vec![
        canonical_pinned.clone(),
        PropertyInfo::new(interner.intern_string("fresh"), TypeId::NUMBER),
    ]);
    let display_pinned = canonical_pinned;
    interner.store_display_properties(
        obj,
        vec![
            display_pinned,
            PropertyInfo::new(
                interner.intern_string("fresh"),
                interner.literal_number(2.0),
            ),
        ],
    );

    let outcome = annotation_widen_all(&interner, obj);
    assert_eq!(
        outcome.type_id, obj,
        "canonical shape is already normalized"
    );
    assert!(
        outcome.display_residue,
        "only the genuinely fresh display property should leave residue"
    );
    let shape = match interner.lookup(outcome.type_id) {
        Some(TypeData::Object(id)) => interner.object_shape(id),
        other => panic!("expected object, got {other:?}"),
    };
    assert_eq!(property_type(&interner, &shape, "pinned"), pinned);
    assert_eq!(property_type(&interner, &shape, "fresh"), TypeId::NUMBER);
}

#[test]
fn annotation_widen_object_index_signature_literals() {
    let interner = TypeInterner::new();
    let obj = interner.object_with_index(ObjectShape {
        flags: ObjectFlags::empty(),
        properties: Vec::new(),
        string_index: Some(IndexSignature {
            key_type: TypeId::STRING,
            value_type: interner.literal_string("x"),
            readonly: false,
            param_name: None,
        }),
        number_index: Some(IndexSignature {
            key_type: TypeId::NUMBER,
            value_type: interner.literal_number(1.0),
            readonly: false,
            param_name: None,
        }),
        symbol_index: Some(IndexSignature {
            key_type: TypeId::SYMBOL,
            value_type: TypeId::BOOLEAN_TRUE,
            readonly: false,
            param_name: None,
        }),
        symbol: None,
    });

    let outcome = annotation_widen_all(&interner, obj);
    let shape = match interner.lookup(outcome.type_id) {
        Some(TypeData::ObjectWithIndex(id)) => interner.object_shape(id),
        other => panic!("expected object-with-index, got {other:?}"),
    };

    assert_eq!(shape.string_index.unwrap().value_type, TypeId::STRING);
    assert_eq!(shape.number_index.unwrap().value_type, TypeId::NUMBER);
    assert_eq!(shape.symbol_index.unwrap().value_type, TypeId::BOOLEAN);
}

#[test]
fn annotation_widen_callable_index_and_write_literals() {
    let interner = TypeInterner::new();
    let mut prop = PropertyInfo::new(interner.intern_string("p"), TypeId::STRING);
    prop.write_type = interner.literal_string("write");
    let callable = interner.callable(CallableShape {
        call_signatures: vec![CallSignature::new(
            vec![ParamInfo::unnamed(TypeId::STRING)],
            TypeId::NUMBER,
        )],
        construct_signatures: Vec::new(),
        properties: vec![prop],
        string_index: Some(IndexSignature {
            key_type: TypeId::STRING,
            value_type: interner.literal_string("x"),
            readonly: false,
            param_name: None,
        }),
        number_index: Some(IndexSignature {
            key_type: TypeId::NUMBER,
            value_type: TypeId::BOOLEAN_TRUE,
            readonly: false,
            param_name: None,
        }),
        symbol: None,
        is_abstract: false,
    });

    let outcome = annotation_widen_all(&interner, callable);
    let shape = match interner.lookup(outcome.type_id) {
        Some(TypeData::Callable(id)) => interner.callable_shape(id),
        other => panic!("expected callable, got {other:?}"),
    };

    assert_eq!(shape.properties[0].type_id, TypeId::STRING);
    assert_eq!(shape.properties[0].write_type, TypeId::STRING);
    assert_eq!(shape.string_index.unwrap().value_type, TypeId::STRING);
    assert_eq!(shape.number_index.unwrap().value_type, TypeId::BOOLEAN);
}

#[test]
fn annotation_widen_noop_compounds_preserve_type_id() {
    let interner = TypeInterner::new();
    let obj = interner.object_with_index(ObjectShape {
        flags: ObjectFlags::empty(),
        properties: vec![PropertyInfo::new(
            interner.intern_string("value"),
            TypeId::STRING,
        )],
        string_index: Some(IndexSignature {
            key_type: TypeId::STRING,
            value_type: TypeId::NUMBER,
            readonly: false,
            param_name: None,
        }),
        number_index: None,
        symbol_index: Some(IndexSignature {
            key_type: TypeId::SYMBOL,
            value_type: TypeId::BOOLEAN,
            readonly: false,
            param_name: None,
        }),
        symbol: None,
    });
    let func = interner.function(FunctionShape {
        type_params: Vec::new(),
        params: vec![ParamInfo::unnamed(TypeId::STRING)],
        this_type: Some(TypeId::NUMBER),
        return_type: TypeId::BOOLEAN,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });
    let callable = interner.callable(CallableShape {
        call_signatures: vec![CallSignature::new(
            vec![ParamInfo::unnamed(TypeId::STRING)],
            TypeId::NUMBER,
        )],
        construct_signatures: Vec::new(),
        properties: vec![PropertyInfo::new(
            interner.intern_string("p"),
            TypeId::BOOLEAN,
        )],
        string_index: Some(IndexSignature {
            key_type: TypeId::STRING,
            value_type: TypeId::STRING,
            readonly: false,
            param_name: None,
        }),
        number_index: None,
        symbol: None,
        is_abstract: false,
    });

    interner.store_display_properties(
        obj,
        vec![PropertyInfo::new(
            interner.intern_string("value"),
            TypeId::STRING,
        )],
    );

    let obj_outcome = annotation_widen_all(&interner, obj);
    assert_eq!(obj_outcome.type_id, obj);
    assert!(!obj_outcome.display_residue);
    assert_eq!(annotation_widen_all(&interner, func).type_id, func);
    assert_eq!(annotation_widen_all(&interner, callable).type_id, callable);
}

#[test]
fn annotation_widen_preserves_root_literal_and_bare_union_members() {
    let interner = TypeInterner::new();
    let lit = interner.literal_number(42.0);
    assert_eq!(annotation_widen_all(&interner, lit).type_id, lit);
    let union = interner.union(vec![
        interner.literal_string("a"),
        interner.literal_string("b"),
    ]);
    assert_eq!(annotation_widen_all(&interner, union).type_id, union);
}

#[test]
fn annotation_widen_method_return_but_not_arrow_return() {
    let interner = TypeInterner::new();
    let lit_return = interner.literal_number(1.0);
    let arrow = interner.function(FunctionShape {
        type_params: Vec::new(),
        params: Vec::new(),
        this_type: None,
        return_type: lit_return,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });
    let mut method_prop = PropertyInfo::new(interner.intern_string("m"), arrow);
    method_prop.is_method = true;
    let plain_prop = PropertyInfo::new(interner.intern_string("f"), arrow);
    let obj = interner.object(vec![method_prop, plain_prop]);
    let outcome = annotation_widen_all(&interner, obj);
    let shape = match interner.lookup(outcome.type_id) {
        Some(TypeData::Object(id)) => interner.object_shape(id),
        other => panic!("expected object, got {other:?}"),
    };
    let widened_method = match interner.lookup(property_type(&interner, &shape, "m")) {
        Some(TypeData::Function(id)) => interner.function_shape(id),
        other => panic!("expected function, got {other:?}"),
    };
    assert_eq!(
        widened_method.return_type,
        TypeId::NUMBER,
        "method property return renders `m(): 1` (annotation position) and widens"
    );
    assert_eq!(
        property_type(&interner, &shape, "f"),
        arrow,
        "non-method property renders `f: () => 1` (no colon before the return) and stays"
    );
}

#[test]
fn annotation_widen_array_string_element_but_not_number_element() {
    let interner = TypeInterner::new();
    let string_arr = interner.array(interner.literal_string("no"));
    let number_arr = interner.array(interner.literal_number(12.0));
    let obj = interner.object(vec![
        PropertyInfo::new(interner.intern_string("s"), string_arr),
        PropertyInfo::new(interner.intern_string("n"), number_arr),
    ]);
    let outcome = annotation_widen_all(&interner, obj);
    let shape = match interner.lookup(outcome.type_id) {
        Some(TypeData::Object(id)) => interner.object_shape(id),
        other => panic!("expected object, got {other:?}"),
    };
    assert_eq!(
        property_type(&interner, &shape, "s"),
        interner.array(TypeId::STRING),
        "`s: \"no\"[]` widens: quoted text rewrote unconditionally"
    );
    assert_eq!(
        property_type(&interner, &shape, "n"),
        number_arr,
        "`n: 12[]` stays: `[` is not a boundary after a number"
    );
}

#[test]
fn annotation_widen_union_first_display_member() {
    let interner = TypeInterner::new();
    let prop_type = interner.union(vec![
        interner.array(interner.literal_string("no")),
        TypeId::UNDEFINED,
    ]);
    let obj = interner.object(vec![PropertyInfo::new(
        interner.intern_string("x"),
        prop_type,
    )]);
    let outcome = annotation_widen_all(&interner, obj);
    let shape = match interner.lookup(outcome.type_id) {
        Some(TypeData::Object(id)) => interner.object_shape(id),
        other => panic!("expected object, got {other:?}"),
    };
    let expected = interner.union(vec![interner.array(TypeId::STRING), TypeId::UNDEFINED]);
    assert_eq!(
        property_type(&interner, &shape, "x"),
        expected,
        "`x: \"no\"[] | undefined` widens its leading member"
    );
}

#[test]
fn annotation_widen_application_args_only_policy() {
    let interner = TypeInterner::new();
    let arg_obj = interner.object(vec![
        PropertyInfo::new(interner.intern_string("a"), interner.literal_string("x")),
        PropertyInfo::new(interner.intern_string("n"), interner.literal_number(1.0)),
    ]);
    let base = interner.literal_string("Base");
    let app = interner.application(base, vec![arg_obj]);
    let outer = interner.object(vec![
        PropertyInfo::new(interner.intern_string("p"), app),
        PropertyInfo::new(interner.intern_string("q"), interner.literal_string("y")),
    ]);
    let outcome = widen_annotation_literals_for_display(
        &interner,
        outer,
        AnnotationLiteralWideningPolicy::STRINGS_AND_BOOLEANS_INSIDE_APPLICATION_ARGS,
    );
    let shape = match interner.lookup(outcome.type_id) {
        Some(TypeData::Object(id)) => interner.object_shape(id),
        other => panic!("expected object, got {other:?}"),
    };
    let widened_app = match interner.lookup(property_type(&interner, &shape, "p")) {
        Some(TypeData::Application(id)) => interner.type_application(id),
        other => panic!("expected application, got {other:?}"),
    };
    let widened_arg = match interner.lookup(widened_app.args[0]) {
        Some(TypeData::Object(id)) => interner.object_shape(id),
        other => panic!("expected object arg, got {other:?}"),
    };
    assert_eq!(
        property_type(&interner, &widened_arg, "a"),
        TypeId::STRING,
        "string annotations inside application args widen"
    );
    assert_eq!(
        property_type(&interner, &widened_arg, "n"),
        interner.literal_number(1.0),
        "number annotations are preserved by this policy"
    );
    assert_eq!(
        property_type(&interner, &shape, "q"),
        interner.literal_string("y"),
        "annotations outside application args are preserved by this policy"
    );
}

#[test]
fn annotation_widen_reports_display_residue() {
    let interner = TypeInterner::new();
    let obj = interner.object(vec![PropertyInfo::new(
        interner.intern_string("a"),
        TypeId::STRING,
    )]);
    interner.store_display_properties(
        obj,
        vec![PropertyInfo::new(
            interner.intern_string("a"),
            interner.literal_string("x"),
        )],
    );
    let outcome = annotation_widen_all(&interner, obj);
    assert_eq!(outcome.type_id, obj, "canonical shape is already widened");
    assert!(
        outcome.display_residue,
        "literal spellings live only in display provenance"
    );
}

// -------- recursive union-origin termination (issue #13507) ------------------
//
// `widen_literal_type` follows a union's display-origin member list when one is
// recorded. A few generic-heavy projects (mobx's observable/computed generics)
// produce a union whose origin members reach the union again, forming a cycle
// in the member graph. Before the path-scoped cycle guard, widening such a union
// recursed until the worker stack overflowed (SIGABRT, exit 134). These tests
// pin the termination — they abort the whole test binary on regression.

/// A union whose display origin references the union itself must terminate.
#[test]
fn widen_literal_type_self_referential_union_origin_terminates() {
    let interner = TypeInterner::new();
    let lit = interner.literal_string("a");
    let union = interner.union(vec![lit, TypeId::NUMBER]);
    // Poison the display origin so a member of the union is the union itself.
    interner.store_union_origin(union, vec![union, lit]);

    let widened = widen_literal_type(&interner, union);
    // Reaching here proves no stack overflow. Widening still happened (the
    // non-cyclic literal member became `string`), so the result stays a union.
    assert!(matches!(interner.lookup(widened), Some(TypeData::Union(_))));
}

/// Two unions whose display origins reference each other must terminate.
#[test]
fn widen_literal_type_mutually_recursive_union_origins_terminate() {
    let interner = TypeInterner::new();
    let a = interner.literal_string("a");
    let b = interner.literal_string("b");
    let u1 = interner.union(vec![a, TypeId::NUMBER]);
    let u2 = interner.union(vec![b, TypeId::BOOLEAN]);
    interner.store_union_origin(u1, vec![u2, a]);
    interner.store_union_origin(u2, vec![u1, b]);

    let widened = widen_literal_type(&interner, u1);
    assert!(matches!(interner.lookup(widened), Some(TypeData::Union(_))));
}

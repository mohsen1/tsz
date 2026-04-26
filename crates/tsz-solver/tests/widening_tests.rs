use super::*;
use crate::TypeInterner;
use crate::types::{
    LiteralValue, OrderedFloat, PropertyInfo, SymbolRef, TypeData, TypeParamInfo, Visibility,
};

#[test]
fn test_widen_string_literal() {
    let interner = TypeInterner::new();
    let string_lit = interner.intern(TypeData::Literal(LiteralValue::String(
        interner.intern_string("hello"),
    )));
    let widened = widen_type(&interner as &dyn crate::TypeDatabase, string_lit);
    assert_eq!(widened, TypeId::STRING);
}

#[test]
fn test_widen_number_literal() {
    let interner = TypeInterner::new();
    let number_lit = interner.intern(TypeData::Literal(LiteralValue::Number(OrderedFloat(42.0))));
    let widened = widen_type(&interner as &dyn crate::TypeDatabase, number_lit);
    assert_eq!(widened, TypeId::NUMBER);
}

#[test]
fn test_widen_boolean_literal() {
    let interner = TypeInterner::new();
    let bool_lit = interner.intern(TypeData::Literal(LiteralValue::Boolean(true)));
    let widened = widen_type(&interner as &dyn crate::TypeDatabase, bool_lit);
    assert_eq!(widened, TypeId::BOOLEAN);
}

#[test]
fn test_widen_union() {
    let interner = TypeInterner::new();
    let lit1 = interner.intern(TypeData::Literal(LiteralValue::Number(OrderedFloat(1.0))));
    let lit2 = interner.intern(TypeData::Literal(LiteralValue::Number(OrderedFloat(2.0))));
    let union = interner.union(vec![lit1, lit2]);

    let widened = widen_type(&interner as &dyn crate::TypeDatabase, union);
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

// ============================================================================
// Additional widening helper coverage
//
// These tests cover the public widening surface beyond the basic `widen_type`
// path. Each test pins a single behavior so a future drift surfaces a single
// failure rather than a cascade.
// ============================================================================

use crate::types::{FunctionShape, ParamInfo, TemplateSpan, TupleElement};

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

// -------- widen_object_literal_properties (pub(crate)) -----------------------

#[test]
fn test_widen_object_literal_properties_widens_mutable_props() {
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
    }];
    let obj = interner.object(props);
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
fn test_apply_const_assertion_array_becomes_readonly_tuple() {
    // [1] as const → readonly [1] (tuple wrapped in ReadonlyType).
    let interner = TypeInterner::new();
    let lit = interner.literal_number(1.0);
    let arr = interner.array(lit);
    let result = apply_const_assertion(&interner, arr);
    let tuple_inner = match interner.lookup(result) {
        Some(TypeData::ReadonlyType(inner)) => inner,
        other => panic!("Expected ReadonlyType, got {other:?}"),
    };
    let elements = match interner.lookup(tuple_inner) {
        Some(TypeData::Tuple(list_id)) => interner.tuple_list(list_id).to_vec(),
        other => panic!("Expected Tuple, got {other:?}"),
    };
    assert_eq!(elements.len(), 1);
    assert_eq!(elements[0].type_id, lit);
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

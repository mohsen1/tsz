//! Tests for mapped type key remapping with 'as never'
//!
//! Tests Rule #41: Key remapping to Never should skip that property.

use crate::types::*;
use crate::{
    evaluation::evaluate::evaluate_type,
    intern::TypeInterner,
    type_queries::{collect_finite_mapped_property_names, get_finite_mapped_property_type},
};

#[test]
fn test_mapped_type_as_never_skips_property() {
    let interner = TypeInterner::new();

    // Test: type Omit<T, K> = { [P in keyof T as P extends K ? never : P]: T[P] }
    // When P extends K, the key is remapped to 'never' and should be skipped

    // Create a simple type: { x: number, y: string }
    let source_type = interner.object(vec![
        PropertyInfo::new(interner.intern_string("x"), TypeId::NUMBER),
        PropertyInfo::new(interner.intern_string("y"), TypeId::STRING),
    ]);

    // Create keyof T
    let keyof_t = interner.intern(TypeData::KeyOf(source_type));

    // Create type parameters P and K
    let type_param_p_info = TypeParamInfo {
        name: interner.intern_string("P"),
        constraint: None,
        default: None,
        is_const: false,
    };
    let type_param_p = interner.intern(TypeData::TypeParameter(type_param_p_info.clone()));

    let type_param_k_info = TypeParamInfo {
        name: interner.intern_string("K"),
        constraint: None,
        default: None,
        is_const: false,
    };
    let type_param_k = interner.intern(TypeData::TypeParameter(type_param_k_info));

    // Create the conditional: P extends K ? never : P
    let conditional_type = ConditionalType {
        check_type: type_param_p,
        extends_type: type_param_k,
        true_type: TypeId::NEVER,
        false_type: type_param_p,
        is_distributive: true,
    };

    let cond_id = interner.conditional(conditional_type);

    // Create the mapped type: { [P in keyof T as P extends K ? never : P]: T[P] }
    let mapped_type = MappedType {
        type_param: type_param_p_info,
        constraint: keyof_t,
        name_type: Some(cond_id),
        template: TypeId::ERROR,
        optional_modifier: None,
        readonly_modifier: None,
    };

    let mapped_id = interner.mapped(mapped_type);

    // Evaluate the mapped type
    let result = evaluate_type(&interner, mapped_id);

    // When K is an unresolved type parameter, the conditional `P extends K ? never : P`
    // is deferred (since the result depends on what K is instantiated to).
    // This means the mapped type can't fully resolve its key remapping,
    // so it stays as a Mapped type rather than evaluating to an object.
    assert!(
        matches!(interner.lookup(result), Some(TypeData::Mapped(_))),
        "Expected mapped type (deferred due to unresolved K), got {:?}",
        interner.lookup(result)
    );
}

#[test]
fn test_mapped_type_key_remap_to_never_filters_property() {
    let interner = TypeInterner::new();

    // Test a simpler case: type Keys = 'a' | 'b'
    // type Mapped = { [K in Keys as K extends 'a' ? never : K]: any }

    let literal_a = interner.literal_string("a");
    let literal_b = interner.literal_string("b");
    let keys_union = interner.union(vec![literal_a, literal_b]);

    // Create type parameter K
    let type_param_k_info = TypeParamInfo {
        name: interner.intern_string("K"),
        constraint: None,
        default: None,
        is_const: false,
    };
    let type_param_k = interner.intern(TypeData::TypeParameter(type_param_k_info.clone()));

    // Create conditional: K extends 'a' ? never : K
    let conditional = ConditionalType {
        check_type: type_param_k,
        extends_type: literal_a,
        true_type: TypeId::NEVER,
        false_type: type_param_k,
        is_distributive: true,
    };

    let cond_id = interner.conditional(conditional);

    // Create mapped type
    let mapped_type = MappedType {
        type_param: type_param_k_info,
        constraint: keys_union,
        name_type: Some(cond_id),
        template: TypeId::ANY,
        optional_modifier: None,
        readonly_modifier: None,
    };

    let mapped_id = interner.mapped(mapped_type);

    // Evaluate
    let result = evaluate_type(&interner, mapped_id);

    // Should get an object with only 'b' (since 'a' is remapped to 'never')
    if let Some(TypeData::Object(shape_id)) = interner.lookup(result) {
        let shape = interner.object_shape(shape_id);
        // Should only have 'b' since 'a' was filtered out by 'as never'
        assert_eq!(shape.properties.len(), 1);
        let prop_name = interner.resolve_atom(shape.properties[0].name);
        assert_eq!(prop_name, "b");
    } else {
        panic!(
            "Expected object type with one property, got {:?}",
            interner.lookup(result)
        );
    }
}

#[test]
fn test_finite_mapped_property_names_resolve_concrete_filtering_remap() {
    let interner = TypeInterner::new();

    let literal_foo = interner.literal_string("FOO");
    let literal_bar = interner.literal_string("bar");
    let keys_union = interner.union(vec![literal_foo, literal_bar]);

    let key_param_info = TypeParamInfo {
        name: interner.intern_string("K"),
        constraint: None,
        default: None,
        is_const: false,
    };
    let key_param = interner.intern(TypeData::TypeParameter(key_param_info.clone()));

    let uppercase_string =
        interner.string_intrinsic(crate::types::StringIntrinsicKind::Uppercase, TypeId::STRING);
    let name_type = interner.conditional(ConditionalType {
        check_type: key_param,
        extends_type: uppercase_string,
        true_type: key_param,
        false_type: TypeId::NEVER,
        is_distributive: true,
    });

    let mapped = interner.mapped(MappedType {
        type_param: key_param_info,
        constraint: keys_union,
        name_type: Some(name_type),
        template: TypeId::NUMBER,
        optional_modifier: None,
        readonly_modifier: None,
    });
    let mapped_id = crate::mapped_type_id(&interner, mapped).expect("expected mapped type id");

    let names =
        collect_finite_mapped_property_names(&interner, mapped_id).expect("expected finite keys");
    let rendered_names: Vec<_> = names
        .iter()
        .map(|name| interner.resolve_atom(*name))
        .collect();
    assert!(
        names.contains(&interner.intern_string("FOO")),
        "expected FOO in remapped names, got {rendered_names:?}"
    );
    assert!(
        !names.contains(&interner.intern_string("bar")),
        "expected bar to be filtered out, got {rendered_names:?}"
    );

    let foo_ty =
        get_finite_mapped_property_type(&interner, mapped_id, "FOO").expect("expected FOO type");
    assert_eq!(foo_ty, TypeId::NUMBER);
    assert!(
        get_finite_mapped_property_type(&interner, mapped_id, "bar").is_none(),
        "lowercase key should be filtered out by the remap conditional"
    );
}

#[test]
fn test_finite_mapped_property_type_specializes_key_filtered_template() {
    let interner = TypeInterner::new();

    let foo_name = interner.intern_string("FOO");
    let bar_name = interner.intern_string("bar");
    let type_name = interner.intern_string("type");

    let literal_foo = interner.literal_string_atom(foo_name);
    let literal_bar = interner.literal_string_atom(bar_name);
    let keys_union = interner.union(vec![literal_foo, literal_bar]);

    let foo_event = interner.object(vec![PropertyInfo::new(type_name, literal_foo)]);
    let bar_event = interner.object(vec![PropertyInfo::new(type_name, literal_bar)]);
    let events_union = interner.union(vec![foo_event, bar_event]);

    let key_param_info = TypeParamInfo {
        name: interner.intern_string("K"),
        constraint: None,
        default: None,
        is_const: false,
    };
    let key_param = interner.intern(TypeData::TypeParameter(key_param_info.clone()));

    let uppercase_string =
        interner.string_intrinsic(crate::types::StringIntrinsicKind::Uppercase, TypeId::STRING);
    let name_type = interner.conditional(ConditionalType {
        check_type: key_param,
        extends_type: uppercase_string,
        true_type: key_param,
        false_type: TypeId::NEVER,
        is_distributive: true,
    });

    let extends_shape = interner.object(vec![PropertyInfo::new(type_name, key_param)]);
    let template = interner.conditional(ConditionalType {
        check_type: events_union,
        extends_type: extends_shape,
        true_type: events_union,
        false_type: TypeId::NEVER,
        is_distributive: true,
    });

    let mapped = interner.mapped(MappedType {
        type_param: key_param_info,
        constraint: keys_union,
        name_type: Some(name_type),
        template,
        optional_modifier: None,
        readonly_modifier: None,
    });
    let mapped_id = crate::mapped_type_id(&interner, mapped).expect("expected mapped type id");

    let foo_ty =
        get_finite_mapped_property_type(&interner, mapped_id, "FOO").expect("expected FOO type");
    let foo_ty = evaluate_type(&interner, foo_ty);
    let foo_members =
        crate::type_queries::get_union_members(&interner, foo_ty).unwrap_or_else(|| vec![foo_ty]);
    assert_eq!(foo_members, vec![foo_event]);

    assert!(
        get_finite_mapped_property_type(&interner, mapped_id, "bar").is_none(),
        "lowercase key should be filtered out when resolving the specialized property type"
    );
}

#[test]
fn test_finite_mapped_property_type_resolves_infer_conditional_keys() {
    let interner = TypeInterner::new();

    let tag_name = interner.intern_string("_tag");
    let a_name = interner.intern_string("A");
    let b_name = interner.intern_string("B");
    let tag_param_name = interner.intern_string("Tag");
    let infer_name = interner.intern_string("X");

    let a_key = interner.literal_string_atom(a_name);
    let b_key = interner.literal_string_atom(b_name);

    let a_value = interner.object(vec![PropertyInfo::new(tag_name, a_key)]);
    let b_value = interner.object(vec![PropertyInfo::new(tag_name, b_key)]);
    let values = interner.union(vec![a_value, b_value]);

    let infer_x = interner.intern(TypeData::Infer(TypeParamInfo {
        name: infer_name,
        constraint: None,
        default: None,
        is_const: false,
    }));
    let record_pattern = interner.object(vec![PropertyInfo::new(tag_name, infer_x)]);
    let tags = interner.conditional(ConditionalType {
        check_type: values,
        extends_type: record_pattern,
        true_type: infer_x,
        false_type: TypeId::NEVER,
        is_distributive: true,
    });

    let tag_param = TypeParamInfo {
        name: tag_param_name,
        constraint: None,
        default: None,
        is_const: false,
    };
    let tag_type = interner.intern(TypeData::TypeParameter(tag_param.clone()));
    let string_constraint = interner.intersection(vec![tags, TypeId::STRING]);

    let event_pattern = interner.object(vec![PropertyInfo::new(tag_name, tag_type)]);
    let template = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("_")),
            type_id: interner.conditional(ConditionalType {
                check_type: values,
                extends_type: event_pattern,
                true_type: values,
                false_type: TypeId::NEVER,
                is_distributive: true,
            }),
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: TypeId::ANY,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let mapped = interner.mapped(MappedType {
        type_param: tag_param,
        constraint: string_constraint,
        name_type: None,
        template,
        optional_modifier: Some(MappedModifier::Add),
        readonly_modifier: Some(MappedModifier::Add),
    });
    let mapped_id = crate::mapped_type_id(&interner, mapped).expect("expected mapped type id");

    let names =
        collect_finite_mapped_property_names(&interner, mapped_id).expect("expected finite keys");
    assert!(
        names.contains(&a_name),
        "expected A in finite keys, got {names:?}"
    );
    assert!(
        names.contains(&b_name),
        "expected B in finite keys, got {names:?}"
    );

    let a_type =
        get_finite_mapped_property_type(&interner, mapped_id, "A").expect("expected A property");
    let a_type = evaluate_type(&interner, a_type);
    let function_type = crate::type_queries::get_union_members(&interner, a_type)
        .unwrap_or_else(|| vec![a_type])
        .into_iter()
        .find(|&member| member != TypeId::UNDEFINED)
        .expect("expected callable member");
    let TypeData::Function(shape_id) = interner
        .lookup(function_type)
        .expect("expected function type")
    else {
        panic!(
            "expected function type, got {:?}",
            interner.lookup(function_type)
        );
    };
    let param_type = interner.function_shape(shape_id).params[0].type_id;
    let param_type = evaluate_type(&interner, param_type);
    let members = crate::type_queries::get_union_members(&interner, param_type)
        .unwrap_or_else(|| vec![param_type]);
    assert_eq!(members, vec![a_value]);
}

#[test]
fn test_finite_mapped_property_type_specializes_unique_symbol_keys() {
    let interner = TypeInterner::new();

    let sym_a = crate::types::SymbolRef(101);
    let sym_b = crate::types::SymbolRef(202);
    let key_a = interner.unique_symbol(sym_a);
    let key_b = interner.unique_symbol(sym_b);
    let keys_union = interner.union(vec![key_a, key_b]);

    let key_param_info = TypeParamInfo {
        name: interner.intern_string("K"),
        constraint: None,
        default: None,
        is_const: false,
    };
    let key_param = interner.intern(TypeData::TypeParameter(key_param_info.clone()));

    let template = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("p")),
            type_id: key_param,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let mapped = interner.mapped(MappedType {
        type_param: key_param_info,
        constraint: keys_union,
        name_type: None,
        template,
        optional_modifier: None,
        readonly_modifier: None,
    });
    let mapped_id = crate::mapped_type_id(&interner, mapped).expect("expected mapped type id");

    let names =
        collect_finite_mapped_property_names(&interner, mapped_id).expect("expected finite keys");
    assert!(names.contains(&interner.intern_string("__unique_101")));
    assert!(names.contains(&interner.intern_string("__unique_202")));

    let prop_ty = get_finite_mapped_property_type(&interner, mapped_id, "__unique_101")
        .expect("expected unique-symbol mapped property");
    let Some(crate::types::TypeData::Function(shape_id)) = interner.lookup(prop_ty) else {
        panic!("expected function property type, got {:?}", interner.lookup(prop_ty));
    };
    let shape = interner.function_shape(shape_id);
    assert_eq!(shape.params.len(), 1);
    assert_eq!(shape.params[0].type_id, key_a);
}

#[test]
fn test_finite_mapped_property_names_do_not_materialize_string_index_keys() {
    let interner = TypeInterner::new();

    let source = interner.object_with_index(ObjectShape {
        flags: ObjectFlags::empty(),
        properties: vec![],
        string_index: Some(IndexSignature {
            key_type: TypeId::STRING,
            value_type: TypeId::NUMBER,
            readonly: false,
            param_name: None,
        }),
        number_index: None,
        symbol: None,
    });
    let keyof_source = interner.intern(TypeData::KeyOf(source));

    let key_param = TypeParamInfo {
        name: interner.intern_string("K"),
        constraint: None,
        default: None,
        is_const: false,
    };

    let mapped = interner.mapped(MappedType {
        type_param: key_param,
        constraint: keyof_source,
        name_type: None,
        template: TypeId::BOOLEAN,
        optional_modifier: Some(MappedModifier::Add),
        readonly_modifier: None,
    });
    let mapped_id = crate::mapped_type_id(&interner, mapped).expect("expected mapped type id");

    assert!(
        collect_finite_mapped_property_names(&interner, mapped_id).is_none(),
        "string index constraints should remain non-finite"
    );
    assert!(
        get_finite_mapped_property_type(&interner, mapped_id, "anything").is_none(),
        "string index constraints should not synthesize exact property types"
    );
}

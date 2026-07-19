#[test]
fn cached_index_access_fast_path_uses_resolver_rereduce_when_flagged() {
    let interner = TypeInterner::new();
    let query_cache = crate::caches::query_cache::QueryCache::new(&interner);

    let object_param_name = interner.intern_string("Obj");
    let object_param = interner.type_param(TypeParamInfo {
        name: object_param_name,
        constraint: None,
        default: None,
        is_const: false,
        origin: crate::types::TypeParamOrigin::User,
    });

    let a_name = interner.intern_string("a");
    let k_name = interner.intern_string("k");
    let object = interner.object(vec![PropertyInfo::new(a_name, TypeId::STRING)]);
    let key_source = interner.object(vec![PropertyInfo::new(k_name, interner.literal_string("a"))]);
    let nested_key = interner.index_access(key_source, interner.literal_string("k"));
    let indexed = interner.index_access(object_param, nested_key);

    let mut subst = TypeSubstitution::new();
    subst.insert(object_param_name, object);

    {
        let _flag = super::flags::InstResolverRereduceFlagGuard::new(false);
        let deferred = instantiate_type_cached(&interner, Some(&query_cache), indexed, &subst);
        assert!(
            matches!(interner.lookup(deferred), Some(TypeData::IndexAccess(_, _))),
            "flag-off fast path should preserve the historical deferred index access"
        );
    }

    let _flag = super::flags::InstResolverRereduceFlagGuard::new(true);
    let reduced = instantiate_type_cached(&interner, Some(&query_cache), indexed, &subst);
    assert_eq!(
        reduced,
        TypeId::STRING,
        "flag-on fast path must enter the resolver-aware index re-reduce seam"
    );
}

#[test]
fn instantiated_keyof_uses_store_backed_rereduce_when_flagged() {
    let interner = TypeInterner::new();
    let store = crate::def::DefinitionStore::new();
    let def_id = DefId(143_571);
    let property_name = interner.intern_string("ready");
    let body = interner.object(vec![PropertyInfo::new(property_name, TypeId::STRING)]);
    store.set_body(def_id, body);

    let query_cache = crate::caches::query_cache::QueryCache::new(&interner)
        .with_definition_store(&store);
    let object_param_name = interner.intern_string("Obj");
    let object_param = interner.type_param(TypeParamInfo {
        name: object_param_name,
        constraint: None,
        default: None,
        is_const: false,
        origin: crate::types::TypeParamOrigin::User,
    });
    let lazy = interner.lazy(def_id);
    let keyed = interner.keyof(object_param);

    let mut subst = TypeSubstitution::new();
    subst.insert(object_param_name, lazy);

    {
        let _flag = super::flags::InstResolverRereduceFlagGuard::new(false);
        let deferred = instantiate_type_cached(&interner, Some(&query_cache), keyed, &subst);
        assert!(
            matches!(interner.lookup(deferred), Some(TypeData::KeyOf(inner)) if inner == lazy),
            "flag-off instantiation should preserve the historical deferred keyof"
        );
    }

    let _flag = super::flags::InstResolverRereduceFlagGuard::new(true);
    let reduced = instantiate_type_cached(&interner, Some(&query_cache), keyed, &subst);
    assert_eq!(
        reduced,
        interner.literal_string_atom(property_name),
        "flag-on instantiation must evaluate keyof through the store-backed resolver"
    );
}

#[test]
fn instantiated_merged_origin_replays_index_key_and_value_slots() {
    let interner = TypeInterner::new();
    let key_name = interner.intern_string("Key");
    let value_name = interner.intern_string("Value");
    let key_param = interner.type_param(TypeParamInfo {
        name: key_name,
        constraint: None,
        default: None,
        is_const: false,
        origin: crate::types::TypeParamOrigin::User,
    });
    let value_param = interner.type_param(TypeParamInfo {
        name: value_name,
        constraint: None,
        default: None,
        is_const: false,
        origin: crate::types::TypeParamOrigin::User,
    });
    let indexed_member = |property_name| {
        interner.object_with_index(ObjectShape {
            flags: ObjectFlags::empty(),
            properties: vec![PropertyInfo::new(property_name, value_param)],
            string_index: Some(IndexSignature {
                key_type: key_param,
                value_type: value_param,
                readonly: false,
                param_name: None,
            }),
            number_index: None,
            symbol_index: None,
            symbol: None,
        })
    };
    let left = indexed_member(interner.intern_string("left"));
    let right = indexed_member(interner.intern_string("right"));
    let merged = interner.intersection(vec![left, right]);
    assert!(interner.get_merged_intersection_origin(merged).is_some());

    let mut substitution = TypeSubstitution::new();
    substitution.insert(key_name, TypeId::STRING);
    substitution.insert(value_name, TypeId::NUMBER);
    let instantiated = instantiate_type(&interner, merged, &substitution);
    let origin = interner
        .get_merged_intersection_origin(instantiated)
        .expect("instantiated merged object should retain a raw origin");
    let Some(TypeData::Intersection(origin_list)) = interner.lookup(origin) else {
        panic!("retained origin must remain an unmerged intersection");
    };
    let members = interner.type_list(origin_list);
    assert_eq!(members.len(), 2);
    for &member in members.iter() {
        let Some(TypeData::ObjectWithIndex(shape_id)) = interner.lookup(member) else {
            panic!("retained origin member must preserve its index signature");
        };
        let index = interner
            .object_shape(shape_id)
            .string_index
            .expect("string index signature");
        assert_eq!(index.key_type, TypeId::STRING);
        assert_eq!(index.value_type, TypeId::NUMBER);
    }
}

#[test]
fn instantiated_merged_origin_preserves_nested_merged_provenance() {
    let interner = TypeInterner::new();
    let type_name = interner.intern_string("Element");
    let type_param = interner.type_param(TypeParamInfo {
        name: type_name,
        constraint: None,
        default: None,
        is_const: false,
        origin: crate::types::TypeParamOrigin::User,
    });
    let kind = interner.intern_string("kind");
    let left_value = interner.intern_string("leftValue");
    let left = interner.object(vec![
        PropertyInfo::new(kind, interner.literal_string("left")),
        PropertyInfo::new(left_value, type_param),
    ]);
    let right = interner.object(vec![
        PropertyInfo::new(kind, interner.literal_string("right")),
        PropertyInfo::new(interner.intern_string("rightValue"), type_param),
    ]);
    let nested_merged = interner.object(vec![
        PropertyInfo::new(kind, TypeId::NEVER),
        PropertyInfo::new(left_value, type_param),
        PropertyInfo::new(interner.intern_string("rightValue"), type_param),
    ]);
    interner.store_merged_intersection_origin(
        nested_merged,
        interner.intersect_types_raw2(left, right),
    );
    interner.record_application_eval_origin(
        nested_merged,
        interner.application(interner.lazy(DefId(143_572)), vec![type_param]),
    );
    let outer_member = interner.object(vec![PropertyInfo::new(
        interner.intern_string("outerValue"),
        type_param,
    )]);
    let outer_merged = interner.intersection(vec![nested_merged, outer_member]);

    let substitution = TypeSubstitution::single(type_name, TypeId::NUMBER);
    let instantiated = instantiate_type(&interner, outer_merged, &substitution);
    let outer_origin = interner
        .get_merged_intersection_origin(instantiated)
        .expect("outer merged object should retain its origin");
    let Some(TypeData::Intersection(origin_list)) = interner.lookup(outer_origin) else {
        panic!("outer origin must remain a raw intersection");
    };
    let members = interner.type_list(origin_list);
    let nested_member = members
        .iter()
        .copied()
        .find(|&member| match interner.lookup(member) {
            Some(TypeData::Object(shape_id) | TypeData::ObjectWithIndex(shape_id)) => interner
                .object_shape(shape_id)
                .properties
                .iter()
                .any(|property| property.name == left_value),
            _ => false,
        })
        .expect("outer origin should retain the nested merged member");
    let nested_origin = interner
        .get_merged_intersection_origin(nested_member)
        .expect("instantiating an outer merge must retain the inner merge's provenance");
    assert_eq!(interner.get_display_alias(nested_member), Some(nested_origin));
    let application = interner
        .get_application_eval_origin(nested_member)
        .expect("nested application provenance should survive structural replay");
    let Some(TypeData::Application(application_id)) = interner.lookup(application) else {
        panic!("nested application provenance must remain an application");
    };
    assert_eq!(
        interner.type_application(application_id).args.as_slice(),
        &[TypeId::NUMBER]
    );
}

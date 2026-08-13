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
fn overloaded_member_rebinds_dependent_local_constraint_after_outer_instantiation() {
    let interner = TypeInterner::new();
    let outer_info = TypeParamInfo::simple(interner.intern_string("OuterDb"));
    let key_info = TypeParamInfo::simple(interner.intern_string("OuterKey"));
    let outer_type = interner.fresh_type_param(outer_info);
    let key_type = interner.fresh_type_param(key_info);

    let table_base = interner.lazy(DefId(902_010));
    let table_expression = interner.application(table_base, vec![outer_type, key_type]);
    let table_info = TypeParamInfo {
        constraint: Some(table_expression),
        ..TypeParamInfo::simple(interner.intern_string("Table"))
    };
    let table_type = interner.fresh_type_param(table_info);

    let reference_base = interner.lazy(DefId(902_011));
    let reference_expression =
        interner.application(reference_base, vec![outer_type, key_type, table_type]);
    let reference_info = TypeParamInfo {
        constraint: Some(reference_expression),
        ..TypeParamInfo::simple(interner.intern_string("Reference"))
    };

    let primary = CallSignature {
        type_params: vec![table_info, reference_info],
        params: vec![
            ParamInfo::unnamed(table_type),
            ParamInfo::unnamed(TypeId::STRING),
            ParamInfo::unnamed(TypeId::STRING),
        ],
        this_type: None,
        return_type: TypeId::UNKNOWN,
        type_predicate: None,
        is_method: true,
    };
    let fallback = CallSignature {
        type_params: Vec::new(),
        params: vec![
            ParamInfo::unnamed(TypeId::STRING),
            ParamInfo::unnamed(TypeId::UNKNOWN),
        ],
        this_type: None,
        return_type: TypeId::UNKNOWN,
        type_predicate: None,
        is_method: true,
    };
    let callable = interner.callable(CallableShape {
        call_signatures: vec![primary, fallback],
        ..Default::default()
    });
    let method_name = interner.intern_string("method");
    let interface_body = interner.object(vec![PropertyInfo {
        is_method: true,
        ..PropertyInfo::new(method_name, callable)
    }]);

    let concrete_outer = interner.object(vec![PropertyInfo::new(
        interner.intern_string("table"),
        TypeId::NUMBER,
    )]);
    let concrete_key = interner.literal_string("table");
    let substitution = TypeSubstitution::from_args(
        &interner,
        &[outer_info, key_info],
        &[concrete_outer, concrete_key],
    );
    let instantiated = instantiate_type(&interner, interface_body, &substitution);

    let Some(TypeData::Object(shape_id)) = interner.lookup(instantiated) else {
        panic!("instantiated interface body must remain an object");
    };
    let method = interner
        .object_shape(shape_id)
        .properties
        .iter()
        .find(|property| property.name == method_name)
        .expect("instantiated interface body must retain its overloaded method")
        .type_id;
    let Some(TypeData::Callable(callable_id)) = interner.lookup(method) else {
        panic!("overloaded method must remain callable");
    };
    let callable = interner.callable_shape(callable_id);
    let primary = &callable.call_signatures[0];
    let rewritten_table = interner.type_param(primary.type_params[0]);
    assert_eq!(primary.params[0].type_id, rewritten_table);

    let rewritten_reference = primary.type_params[1]
        .constraint
        .expect("dependent type parameter must retain its constraint");
    let Some(TypeData::Application(reference_id)) = interner.lookup(rewritten_reference) else {
        panic!("dependent constraint must remain an application");
    };
    let rewritten_reference = interner.type_application(reference_id);
    assert_eq!(
        rewritten_reference.args.as_slice(),
        &[concrete_outer, concrete_key, rewritten_table]
    );
}

#[test]
fn changed_local_rebinding_preserves_same_name_foreign_seen_before_binding() {
    let interner = TypeInterner::new();
    let outer_info = TypeParamInfo::simple(interner.intern_string("OuterValue"));
    let outer_type = interner.fresh_type_param(outer_info);
    let shared_name = interner.intern_string("Item");
    let scope_file = interner.intern_string("scope.ts");
    let foreign_info = TypeParamInfo {
        constraint: Some(TypeId::NUMBER),
        origin: crate::types::TypeParamOrigin::DeclScoped {
            file: scope_file,
            node: 10,
        },
        ..TypeParamInfo::simple(shared_name)
    };
    let foreign_type = interner.fresh_type_param(foreign_info);

    let constraint_base = interner.lazy(DefId(902_012));
    let local_constraint =
        interner.application(constraint_base, vec![outer_type, foreign_type]);
    let local_info = TypeParamInfo {
        constraint: Some(local_constraint),
        origin: crate::types::TypeParamOrigin::DeclScoped {
            file: scope_file,
            node: 20,
        },
        ..TypeParamInfo::simple(shared_name)
    };
    let local_type = interner.fresh_type_param(local_info);
    let method = interner.function(FunctionShape {
        type_params: vec![local_info],
        params: vec![
            ParamInfo::unnamed(local_type),
            ParamInfo::unnamed(foreign_type),
        ],
        this_type: None,
        return_type: TypeId::UNKNOWN,
        type_predicate: None,
        is_constructor: false,
        is_method: true,
    });

    let substitution = TypeSubstitution::single(outer_info.name, TypeId::STRING);
    let instantiated = instantiate_type(&interner, method, &substitution);
    let Some(TypeData::Function(method_id)) = interner.lookup(instantiated) else {
        panic!("instantiated method must remain a function");
    };
    let method = interner.function_shape(method_id);
    let rewritten_local = interner.type_param(method.type_params[0]);
    assert_eq!(method.params[0].type_id, rewritten_local);
    assert_eq!(method.params[1].type_id, foreign_type);
    assert_ne!(method.params[1].type_id, rewritten_local);

    let rewritten_constraint = method.type_params[0]
        .constraint
        .expect("rewritten local must retain its constraint");
    let Some(TypeData::Application(constraint_id)) = interner.lookup(rewritten_constraint) else {
        panic!("rewritten constraint must remain an application");
    };
    assert_eq!(
        interner.type_application(constraint_id).args.as_slice(),
        &[TypeId::STRING, foreign_type]
    );
}

#[test]
fn changed_local_rebinding_preserves_same_name_foreign_seen_after_binding() {
    let interner = TypeInterner::new();
    let outer_info = TypeParamInfo::simple(interner.intern_string("OuterValue"));
    let outer_type = interner.fresh_type_param(outer_info);
    let shared_name = interner.intern_string("Item");
    let scope_file = interner.intern_string("scope.ts");
    let foreign_info = TypeParamInfo {
        constraint: Some(TypeId::NUMBER),
        origin: crate::types::TypeParamOrigin::DeclScoped {
            file: scope_file,
            node: 30,
        },
        ..TypeParamInfo::simple(shared_name)
    };
    let foreign_type = interner.fresh_type_param(foreign_info);

    let constraint_base = interner.lazy(DefId(902_013));
    let local_constraint = interner.application(constraint_base, vec![outer_type]);
    let local_info = TypeParamInfo {
        constraint: Some(local_constraint),
        origin: crate::types::TypeParamOrigin::DeclScoped {
            file: scope_file,
            node: 40,
        },
        ..TypeParamInfo::simple(shared_name)
    };
    let local_type = interner.fresh_type_param(local_info);
    let method = interner.function(FunctionShape {
        type_params: vec![local_info],
        params: vec![ParamInfo::unnamed(local_type)],
        this_type: None,
        // This foreign binder is first encountered after the changed local
        // declaration has been instantiated and bound.
        return_type: foreign_type,
        type_predicate: None,
        is_constructor: false,
        is_method: true,
    });

    let substitution = TypeSubstitution::single(outer_info.name, TypeId::STRING);
    let instantiated = instantiate_type(&interner, method, &substitution);
    let Some(TypeData::Function(method_id)) = interner.lookup(instantiated) else {
        panic!("instantiated method must remain a function");
    };
    let method = interner.function_shape(method_id);
    let rewritten_local = interner.type_param(method.type_params[0]);
    assert_eq!(method.params[0].type_id, rewritten_local);
    assert_eq!(method.return_type, foreign_type);
    assert_ne!(method.return_type, rewritten_local);
}

#[test]
fn changed_local_binding_invalidates_completed_composite_cache_entries() {
    let interner = TypeInterner::new();
    let outer_info = TypeParamInfo::simple(interner.intern_string("OuterValue"));
    let outer_type = interner.fresh_type_param(outer_info);
    let scope_file = interner.intern_string("scope.ts");

    // Lowering binds this declaration-shaped placeholder before it lowers the
    // self-referential constraint, then replaces the binding with the complete
    // declaration info for later signature positions.
    let local_placeholder_info = TypeParamInfo {
        origin: crate::types::TypeParamOrigin::DeclScoped {
            file: scope_file,
            node: 50,
        },
        ..TypeParamInfo::simple(interner.intern_string("Item"))
    };
    let local_placeholder = interner.fresh_type_param(local_placeholder_info);
    let box_base = interner.lazy(DefId(902_014));
    let shared_box = interner.application(box_base, vec![outer_type, local_placeholder]);
    let local_info = TypeParamInfo {
        constraint: Some(shared_box),
        ..local_placeholder_info
    };
    let local_type = interner.fresh_type_param(local_info);

    let dependent_info = TypeParamInfo {
        constraint: Some(shared_box),
        origin: crate::types::TypeParamOrigin::DeclScoped {
            file: scope_file,
            node: 60,
        },
        ..TypeParamInfo::simple(interner.intern_string("Dependent"))
    };
    let method = interner.function(FunctionShape {
        type_params: vec![local_info, dependent_info],
        params: vec![ParamInfo::unnamed(local_type)],
        this_type: None,
        return_type: shared_box,
        type_predicate: None,
        is_constructor: false,
        is_method: true,
    });

    let substitution = TypeSubstitution::single(outer_info.name, TypeId::STRING);
    let instantiated = instantiate_type(&interner, method, &substitution);
    let Some(TypeData::Function(method_id)) = interner.lookup(instantiated) else {
        panic!("instantiated method must remain a function");
    };
    let method = interner.function_shape(method_id);
    let rewritten_local = interner.type_param(method.type_params[0]);
    assert_eq!(method.params[0].type_id, rewritten_local);

    let rewritten_dependent = method.type_params[1]
        .constraint
        .expect("dependent parameter must retain its shared constraint");
    let Some(TypeData::Application(dependent_id)) = interner.lookup(rewritten_dependent) else {
        panic!("dependent constraint must remain an application");
    };
    assert_eq!(
        interner.type_application(dependent_id).args.as_slice(),
        &[TypeId::STRING, rewritten_local]
    );

    let Some(TypeData::Application(return_id)) = interner.lookup(method.return_type) else {
        panic!("shared return type must remain an application");
    };
    assert_eq!(
        interner.type_application(return_id).args.as_slice(),
        &[TypeId::STRING, rewritten_local]
    );
}

#[test]
fn shadowing_scope_invalidates_completed_composite_cache_entries() {
    let interner = TypeInterner::new();
    let local_info = TypeParamInfo::simple(interner.intern_string("Item"));
    let local_type = interner.type_param(local_info);
    let shared_array = interner.array(local_type);
    let method = interner.function(FunctionShape {
        type_params: vec![local_info],
        params: vec![ParamInfo::unnamed(shared_array)],
        this_type: None,
        return_type: shared_array,
        type_predicate: None,
        is_constructor: false,
        is_method: true,
    });
    // The first element caches `Item[] -> string[]` before the method's own
    // `<Item>` enters scope. Reusing that composite inside the method must
    // observe shadowing even though the local declaration itself is unchanged.
    let container = interner.tuple(vec![
        crate::types::TupleElement::fixed(shared_array),
        crate::types::TupleElement::fixed(method),
    ]);

    let substitution = TypeSubstitution::single(local_info.name, TypeId::STRING);
    let instantiated = instantiate_type(&interner, container, &substitution);
    let Some(TypeData::Tuple(tuple_id)) = interner.lookup(instantiated) else {
        panic!("instantiated container must remain a tuple");
    };
    let elements = interner.tuple_list(tuple_id);
    assert_eq!(elements.len(), 2);
    assert_eq!(elements[0].type_id, interner.array(TypeId::STRING));

    let Some(TypeData::Function(method_id)) = interner.lookup(elements[1].type_id) else {
        panic!("second element must remain a function");
    };
    let method = interner.function_shape(method_id);
    assert_eq!(method.type_params, vec![local_info]);
    assert_eq!(method.params[0].type_id, shared_array);
    assert_eq!(method.return_type, shared_array);
}

#[test]
fn declaration_preservation_mode_invalidates_completed_type_param_cache_entries() {
    let interner = TypeInterner::new();
    let outer_info = TypeParamInfo::simple(interner.intern_string("OuterValue"));
    let outer_type = interner.fresh_type_param(outer_info);
    let scope_file = interner.intern_string("scope.ts");
    let foreign_info = TypeParamInfo {
        constraint: Some(interner.array(outer_type)),
        origin: crate::types::TypeParamOrigin::DeclScoped {
            file: scope_file,
            node: 70,
        },
        ..TypeParamInfo::simple(interner.intern_string("Foreign"))
    };
    let foreign_type = interner.fresh_type_param(foreign_info);
    let local_info = TypeParamInfo {
        constraint: Some(foreign_type),
        origin: crate::types::TypeParamOrigin::DeclScoped {
            file: scope_file,
            node: 80,
        },
        ..TypeParamInfo::simple(interner.intern_string("Local"))
    };
    let method = interner.function(FunctionShape {
        type_params: vec![local_info],
        params: vec![ParamInfo::unnamed(foreign_type)],
        this_type: None,
        return_type: TypeId::UNKNOWN,
        type_predicate: None,
        is_constructor: false,
        is_method: true,
    });

    let substitution = TypeSubstitution::single(outer_info.name, TypeId::STRING);
    let instantiated = instantiate_type(&interner, method, &substitution);
    let Some(TypeData::Function(method_id)) = interner.lookup(instantiated) else {
        panic!("instantiated method must remain a function");
    };
    let method = interner.function_shape(method_id);
    assert_eq!(method.type_params, vec![local_info]);
    let Some(TypeData::Array(element)) = interner.lookup(method.params[0].type_id) else {
        panic!("normal signature-body mode must apply the foreign constraint fallback");
    };
    assert_eq!(element, TypeId::STRING);
}

#[test]
fn empty_type_param_list_does_not_advance_memo_environment() {
    let interner = TypeInterner::new();
    let substitution = TypeSubstitution::new();
    let mut instantiator = TypeInstantiator::new(&interner, &substitution);
    let initial_epoch = instantiator.memo_environment_epoch;

    assert_eq!(instantiator.instantiate_type_params_if_changed(&[]), None);
    assert_eq!(instantiator.memo_environment_epoch, initial_epoch);
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

/// Entering a shadowing scope no longer clones the whole `visiting` memo; it
/// records a mark into a transactional undo log and reverse-replays on exit.
/// This pins the invariant that matters for correctness: after a scope (and any
/// nested scope) unwinds, `visiting` and the memo epoch are restored to their
/// exact pre-entry state — byte-for-byte what the historical full-map clone
/// produced — regardless of inserts, removals, and overwrites performed inside.
#[test]
fn shadowing_scope_undo_log_restores_visiting_and_epoch_exactly() {
    use super::InstantiationMemoEntry;

    let interner = TypeInterner::new();
    let substitution = TypeSubstitution::new();
    let mut instantiator = TypeInstantiator::new(&interner, &substitution);

    let t_name = interner.intern_string("T");
    let u_name = interner.intern_string("U");
    let t_info = TypeParamInfo {
        name: t_name,
        constraint: None,
        default: None,
        is_const: false,
        origin: crate::types::TypeParamOrigin::User,
    };
    let u_info = TypeParamInfo {
        name: u_name,
        constraint: None,
        default: None,
        is_const: false,
        origin: crate::types::TypeParamOrigin::User,
    };
    let t_id = interner.type_param(t_info);
    let u_id = interner.type_param(u_info);

    // Seed outer memo state (no scope active yet, so nothing is logged): an
    // entry for the soon-to-be-shadowed `T`, plus two unrelated keys.
    let base_epoch = instantiator.memo_environment_epoch;
    instantiator
        .visiting
        .insert(t_id, InstantiationMemoEntry::Active);
    instantiator.visiting.insert(
        TypeId::STRING,
        InstantiationMemoEntry::Completed {
            result: TypeId::NUMBER,
            environment_epoch: base_epoch,
        },
    );
    instantiator
        .visiting
        .insert(TypeId::BOOLEAN, InstantiationMemoEntry::Active);
    let baseline = instantiator.visiting.clone();

    // Enter a scope shadowing `T`: `T`'s entry is removed and the epoch advances.
    let (outer_len, outer_mark) = instantiator.enter_shadowing_scope(&[t_info]);
    assert!(outer_mark.is_some(), "a non-empty parameter list opens a scope");
    assert!(
        !instantiator.visiting.contains_key(&t_id),
        "the shadowed parameter's memo entry is removed inside the scope"
    );
    assert_ne!(
        instantiator.memo_environment_epoch, base_epoch,
        "entering a scope advances the memo epoch"
    );

    // Mutate inside the scope: overwrite an outer key, remove another, add new.
    instantiator.visiting_insert(
        TypeId::STRING,
        InstantiationMemoEntry::Completed {
            result: TypeId::BOOLEAN,
            environment_epoch: instantiator.memo_environment_epoch,
        },
    );
    instantiator.visiting_remove(TypeId::BOOLEAN);
    instantiator.visiting_insert(u_id, InstantiationMemoEntry::Active);

    // A nested scope shadowing `U` (whose entry exists here), then unwound.
    let (inner_len, inner_mark) = instantiator.enter_shadowing_scope(&[u_info]);
    assert!(!instantiator.visiting.contains_key(&u_id));
    instantiator.visiting_insert(TypeId::NUMBER, InstantiationMemoEntry::Active);
    instantiator.exit_shadowing_scope(inner_len, inner_mark);
    assert!(
        instantiator.visiting.contains_key(&u_id),
        "leaving the nested scope restores its shadowed parameter"
    );
    assert!(
        !instantiator.visiting.contains_key(&TypeId::NUMBER),
        "leaving the nested scope discards entries added inside it"
    );

    // Unwind the outer scope: everything returns to the pre-entry state.
    instantiator.exit_shadowing_scope(outer_len, outer_mark);
    assert_eq!(
        instantiator.visiting, baseline,
        "the undo log restores `visiting` to its exact pre-scope contents"
    );
    assert_eq!(
        instantiator.memo_environment_epoch, base_epoch,
        "the undo log restores the memo epoch"
    );
    assert!(
        instantiator.visiting_undo_log.is_empty(),
        "a fully unwound walk leaves no undo-log residue"
    );
}

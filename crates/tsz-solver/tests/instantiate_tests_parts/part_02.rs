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

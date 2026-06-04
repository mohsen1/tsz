#[test]
fn assignability_cache_erase_generics_matches_uncached_relation_policy() {
    let interner = TypeInterner::new();
    let db = QueryCache::new(&interner);

    let target_t = TypeParamInfo {
        name: interner.intern_string("Target"),
        constraint: None,
        default: None,
        is_const: false,
    };
    let target_t_type = interner.type_param(target_t);
    let source = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![],
        this_type: None,
        return_type: target_t_type,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });
    let target = interner.function(FunctionShape {
        type_params: vec![target_t],
        params: vec![],
        this_type: None,
        return_type: target_t_type,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let erased = RelationPolicy::default().with_erase_generics(true);
    let strict = RelationPolicy::default().with_erase_generics(false);
    let erased_key = RelationCacheKey::for_assignability(source, target, erased.cache_config());
    let strict_key = RelationCacheKey::for_assignability(source, target, strict.cache_config());

    assert_ne!(
        erased_key, strict_key,
        "erased and strict generic-signature policies must occupy distinct assignability cache slots",
    );

    let erased_uncached = query_relation(
        &interner,
        source,
        target,
        RelationKind::Assignable,
        erased,
        RelationContext::default(),
    )
    .is_related();
    let strict_uncached = query_relation(
        &interner,
        source,
        target,
        RelationKind::Assignable,
        strict,
        RelationContext::default(),
    )
    .is_related();

    assert!(
        erased_uncached,
        "erased generic-signature compatibility should allow the relation",
    );
    assert!(
        !strict_uncached,
        "strict generic-signature compatibility must not promote an outer type parameter into a generic signature",
    );

    assert_eq!(
        db.is_assignable_to_with_policy(source, target, strict),
        strict_uncached,
        "cached strict generic policy must match direct query_relation",
    );
    assert_eq!(
        db.lookup_assignability_cache(strict_key),
        Some(strict_uncached),
        "strict generic result must be stored in the strict assignability slot",
    );
    assert_eq!(
        db.lookup_assignability_cache(erased_key),
        None,
        "erased-generic lookup must not hit the strict slot",
    );

    assert_eq!(
        db.is_assignable_to_with_policy(source, target, erased),
        erased_uncached,
        "cached erased generic policy must match direct query_relation",
    );
    assert_eq!(
        db.lookup_assignability_cache(erased_key),
        Some(erased_uncached),
        "erased generic result must be stored in the erased assignability slot",
    );
    assert_eq!(
        db.lookup_assignability_cache(strict_key),
        Some(strict_uncached),
        "strict generic slot must remain intact after the erased lookup",
    );
}

#[test]
fn assignability_cache_erased_generic_retry_matches_uncached_relation_policy() {
    let interner = TypeInterner::new();
    let db = QueryCache::new(&interner);

    let source_s = TypeParamInfo {
        name: interner.intern_string("Source"),
        constraint: None,
        default: None,
        is_const: false,
    };
    let source_s_type = interner.type_param(source_s);
    let source = interner.function(FunctionShape {
        type_params: vec![source_s],
        params: vec![ParamInfo::unnamed(source_s_type)],
        this_type: None,
        return_type: source_s_type,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let target_t = TypeParamInfo {
        name: interner.intern_string("TargetT"),
        constraint: None,
        default: None,
        is_const: false,
    };
    let target_u = TypeParamInfo {
        name: interner.intern_string("TargetU"),
        constraint: None,
        default: None,
        is_const: false,
    };
    let target_t_type = interner.type_param(target_t);
    let target_u_type = interner.type_param(target_u);
    let target = interner.function(FunctionShape {
        type_params: vec![target_t, target_u],
        params: vec![ParamInfo::unnamed(target_t_type)],
        this_type: None,
        return_type: target_u_type,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let no_retry = RelationPolicy::default();
    let retry =
        RelationPolicy::from_relation_flags(RelationFlags::ALLOW_ERASED_GENERIC_SIGNATURE_RETRY);
    let no_retry_key = RelationCacheKey::for_assignability(source, target, no_retry.cache_config());
    let retry_key = RelationCacheKey::for_assignability(source, target, retry.cache_config());

    assert_ne!(
        no_retry_key, retry_key,
        "erased generic retry policy must occupy a distinct assignability cache slot",
    );

    let no_retry_uncached = query_relation(
        &interner,
        source,
        target,
        RelationKind::Assignable,
        no_retry,
        RelationContext::default(),
    )
    .is_related();
    let retry_uncached = query_relation(
        &interner,
        source,
        target,
        RelationKind::Assignable,
        retry,
        RelationContext::default(),
    )
    .is_related();

    assert!(
        !no_retry_uncached,
        "contextual inference should reject the unequal-arity generic signatures before retry",
    );
    assert!(
        retry_uncached,
        "erased generic retry should allow the unequal-arity signatures",
    );

    assert_eq!(
        db.is_assignable_to_with_policy(source, target, no_retry),
        no_retry_uncached,
        "cached no-retry policy must match direct query_relation",
    );
    assert_eq!(
        db.lookup_assignability_cache(no_retry_key),
        Some(no_retry_uncached),
        "no-retry result must be stored in the no-retry assignability slot",
    );
    assert_eq!(
        db.lookup_assignability_cache(retry_key),
        None,
        "retry lookup must not hit the no-retry slot",
    );

    assert_eq!(
        db.is_assignable_to_with_policy(source, target, retry),
        retry_uncached,
        "cached retry policy must match direct query_relation",
    );
    assert_eq!(
        db.lookup_assignability_cache(retry_key),
        Some(retry_uncached),
        "retry result must be stored in its own assignability slot",
    );
    assert_eq!(
        db.lookup_assignability_cache(no_retry_key),
        Some(no_retry_uncached),
        "no-retry slot must remain intact after the retry lookup",
    );
}

#[test]
fn assignability_cache_in_callback_param_check_matches_uncached_relation_policy() {
    let interner = TypeInterner::new();
    let db = QueryCache::new(&interner);
    let name = interner.intern_string("name");
    let breed = interner.intern_string("breed");

    let animal = interner.object(vec![PropertyInfo::new(name, TypeId::STRING)]);
    let dog = interner.object(vec![
        PropertyInfo::new(name, TypeId::STRING),
        PropertyInfo::new(breed, TypeId::STRING),
    ]);

    let mut dog_method_shape = FunctionShape::new(vec![ParamInfo::unnamed(dog)], TypeId::VOID);
    dog_method_shape.is_method = true;
    let source = interner.function(dog_method_shape);

    let mut animal_method_shape =
        FunctionShape::new(vec![ParamInfo::unnamed(animal)], TypeId::VOID);
    animal_method_shape.is_method = true;
    let target = interner.function(animal_method_shape);

    let ordinary_method_policy =
        RelationPolicy::from_relation_flags(RelationFlags::STRICT_FUNCTION_TYPES);
    let callback_policy = RelationPolicy::from_relation_flags(
        RelationFlags::STRICT_FUNCTION_TYPES | RelationFlags::IN_CALLBACK_PARAM_CHECK,
    );
    let ordinary_key =
        RelationCacheKey::for_assignability(source, target, ordinary_method_policy.cache_config());
    let callback_key =
        RelationCacheKey::for_assignability(source, target, callback_policy.cache_config());

    assert_ne!(
        ordinary_key, callback_key,
        "callback parameter mode must occupy a distinct assignability cache slot",
    );

    let ordinary_uncached = query_relation(
        &interner,
        source,
        target,
        RelationKind::Assignable,
        ordinary_method_policy,
        RelationContext::default(),
    )
    .is_related();
    let callback_uncached = query_relation(
        &interner,
        source,
        target,
        RelationKind::Assignable,
        callback_policy,
        RelationContext::default(),
    )
    .is_related();

    assert!(
        ordinary_uncached,
        "ordinary strict-function method comparison keeps method parameters bivariant",
    );
    assert!(
        !callback_uncached,
        "callback parameter mode must disable method bivariance for the immediate signature comparison",
    );

    let ordinary_cached = db.is_assignable_to_with_policy(source, target, ordinary_method_policy);
    assert_eq!(
        ordinary_cached, ordinary_uncached,
        "cached ordinary method policy must match direct query_relation",
    );
    assert_eq!(
        db.lookup_assignability_cache(ordinary_key),
        Some(ordinary_cached),
        "ordinary method result must use its own cache slot",
    );
    assert_eq!(
        db.lookup_assignability_cache(callback_key),
        None,
        "callback-mode lookup must not hit the ordinary method slot",
    );

    let callback_cached = db.is_assignable_to_with_policy(source, target, callback_policy);
    assert_eq!(
        callback_cached, callback_uncached,
        "cached callback-mode policy must match direct query_relation",
    );
    assert_eq!(
        db.lookup_assignability_cache(callback_key),
        Some(callback_cached),
        "callback-mode result must use its own cache slot",
    );
    assert_eq!(
        db.lookup_assignability_cache(ordinary_key),
        Some(ordinary_cached),
        "ordinary method slot must remain intact after the callback-mode lookup",
    );
}

#[test]
fn assignability_cache_disable_method_bivariance_matches_uncached_method_parameter_count() {
    let interner = TypeInterner::new();
    let db = QueryCache::new(&interner);

    let mut source_shape = FunctionShape::new(
        vec![
            ParamInfo::unnamed(TypeId::STRING),
            ParamInfo::unnamed(TypeId::NUMBER),
        ],
        TypeId::VOID,
    );
    source_shape.is_method = true;
    let source = interner.function(source_shape);

    let mut target_shape =
        FunctionShape::new(vec![ParamInfo::unnamed(TypeId::STRING)], TypeId::VOID);
    target_shape.is_method = true;
    let target = interner.function(target_shape);

    let bivariant_method = RelationPolicy::from_relation_flags(
        RelationFlags::STRICT_FUNCTION_TYPES | RelationFlags::ALLOW_BIVARIANT_PARAM_COUNT,
    );
    let sound_method = RelationPolicy::from_relation_flags(
        RelationFlags::STRICT_FUNCTION_TYPES
            | RelationFlags::ALLOW_BIVARIANT_PARAM_COUNT
            | RelationFlags::DISABLE_METHOD_BIVARIANCE,
    );
    let bivariant_key =
        RelationCacheKey::for_assignability(source, target, bivariant_method.cache_config());
    let sound_key =
        RelationCacheKey::for_assignability(source, target, sound_method.cache_config());

    assert_ne!(
        bivariant_key, sound_key,
        "method-bivariant and sound-method parameter-count policies must occupy distinct assignability cache slots",
    );

    let bivariant_uncached = query_relation(
        &interner,
        source,
        target,
        RelationKind::Assignable,
        bivariant_method,
        RelationContext::default(),
    )
    .is_related();
    let sound_uncached = query_relation(
        &interner,
        source,
        target,
        RelationKind::Assignable,
        sound_method,
        RelationContext::default(),
    )
    .is_related();

    assert!(
        bivariant_uncached,
        "method bivariance should allow extra required method parameters when the count exception is enabled",
    );
    assert!(
        !sound_uncached,
        "disabling method bivariance should also disable the method parameter-count exception",
    );

    let bivariant_cached = db.is_assignable_to_with_policy(source, target, bivariant_method);
    assert_eq!(
        bivariant_cached, bivariant_uncached,
        "cached method-bivariant parameter-count policy must match direct query_relation",
    );
    assert_eq!(
        db.lookup_assignability_cache(bivariant_key),
        Some(bivariant_cached),
        "method-bivariant parameter-count result must use its own cache slot",
    );
    assert_eq!(
        db.lookup_assignability_cache(sound_key),
        None,
        "sound-method lookup must not hit the method-bivariant parameter-count slot",
    );

    let sound_cached = db.is_assignable_to_with_policy(source, target, sound_method);
    assert_eq!(
        sound_cached, sound_uncached,
        "cached sound-method parameter-count policy must match direct query_relation",
    );
    assert_eq!(
        db.lookup_assignability_cache(sound_key),
        Some(sound_cached),
        "sound-method parameter-count result must use its own cache slot",
    );
    assert_eq!(
        db.lookup_assignability_cache(bivariant_key),
        Some(bivariant_cached),
        "method-bivariant parameter-count slot must remain intact after the sound-method lookup",
    );
}

#[test]
fn subtype_cache_split_accessor_variance_matches_uncached_property_policy() {
    let interner = TypeInterner::new();
    let db = QueryCache::new(&interner);
    let value = interner.intern_string("value");
    let wide_write = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);

    let wide_accessor = interner.object(vec![PropertyInfo {
        write_type: wide_write,
        ..PropertyInfo::new(value, TypeId::STRING)
    }]);
    let narrow_accessor = interner.object(vec![PropertyInfo::new(value, TypeId::STRING)]);
    let policy = RelationPolicy::unflagged_compatibility();
    let wide_to_narrow_key =
        RelationCacheKey::for_subtype(wide_accessor, narrow_accessor, policy.cache_config());
    let narrow_to_wide_key =
        RelationCacheKey::for_subtype(narrow_accessor, wide_accessor, policy.cache_config());

    let wide_to_narrow_uncached = query_relation(
        &interner,
        wide_accessor,
        narrow_accessor,
        RelationKind::Subtype,
        policy,
        RelationContext::default(),
    )
    .is_related();
    let narrow_to_wide_uncached = query_relation(
        &interner,
        narrow_accessor,
        wide_accessor,
        RelationKind::Subtype,
        policy,
        RelationContext::default(),
    )
    .is_related();

    assert!(
        wide_to_narrow_uncached,
        "split accessor with a wider write type should satisfy a uniform property target",
    );
    assert!(
        !narrow_to_wide_uncached,
        "uniform property write type should not satisfy a wider split-accessor target",
    );

    let wide_to_narrow_cached =
        db.is_subtype_of_with_policy(wide_accessor, narrow_accessor, policy);
    assert_eq!(
        wide_to_narrow_cached, wide_to_narrow_uncached,
        "cached split-accessor subtype must match direct query_relation",
    );
    assert_eq!(
        db.lookup_subtype_cache(wide_to_narrow_key),
        Some(wide_to_narrow_cached),
        "wide-to-narrow split-accessor result must use its own subtype cache slot",
    );
    assert_eq!(
        db.lookup_subtype_cache(narrow_to_wide_key),
        None,
        "reverse split-accessor lookup must not hit the wide-to-narrow slot",
    );

    let narrow_to_wide_cached =
        db.is_subtype_of_with_policy(narrow_accessor, wide_accessor, policy);
    assert_eq!(
        narrow_to_wide_cached, narrow_to_wide_uncached,
        "cached reverse split-accessor subtype must match direct query_relation",
    );
    assert_eq!(
        db.lookup_subtype_cache(narrow_to_wide_key),
        Some(narrow_to_wide_cached),
        "reverse split-accessor result must use its own subtype cache slot",
    );
    assert_eq!(
        db.lookup_subtype_cache(wide_to_narrow_key),
        Some(wide_to_narrow_cached),
        "wide-to-narrow split-accessor slot must remain intact after the reverse lookup",
    );
}
